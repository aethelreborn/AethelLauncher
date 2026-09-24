//! Fabric loader metadata.
//!
//! Uses the two endpoints that actually matter:
//! * `GET /v2/versions/loader/{game}` — pick a loader build
//! * `GET /v2/versions/loader/{game}/{loader}/profile/json` — a version-JSON
//!   fragment (mainClass + libraries) that we merge into the vanilla version.
//!
//! See <https://fabricmc.net/develop/>.

use serde::{Deserialize, Serialize};

/// One entry from `/v2/versions/loader/{game}`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FabricLoaderEntry {
    pub loader: FabricComponent,
    pub intermediary: FabricComponent,
    #[serde(rename = "launcherMeta", default)]
    pub launcher_meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FabricComponent {
    pub version: String,
    #[serde(default)]
    pub stable: bool,
}

impl FabricLoaderEntry {
    pub fn loader_version(&self) -> &str {
        &self.loader.version
    }

    pub fn intermediary_version(&self) -> &str {
        &self.intermediary.version
    }
}

/// The loader profile, structurally a `version.json` that inherits from vanilla.
///
/// Reuses [`crate::manifest::mojang::VersionJson`] for the parts we need; the
/// `libraries` entries carry `name` + `url` (a Maven base) and frequently have
/// no `downloads` block at all.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricProfile {
    pub id: String,
    #[serde(default)]
    pub main_class: Option<String>,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub libraries: Vec<crate::manifest::mojang::Library>,
    #[serde(default)]
    pub arguments: Option<crate::manifest::mojang::VersionArguments>,
    /// Fabric puts a few `-D` properties here.
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
}

/// A resolved, ready-to-use loader.
#[derive(Debug, Clone)]
pub struct FabricLoader {
    pub loader_version: String,
    pub intermediary_version: String,
    pub profile: FabricProfile,
}

/// Latest stable loader entry for a Minecraft version, falling back to the
/// newest unstable one when no stable build exists yet (common on snapshots).
pub async fn fetch_loader_entries(
    client: &reqwest::Client,
    mc_version: &str,
) -> anyhow::Result<Vec<FabricLoaderEntry>> {
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc_version}");
    let response = client.get(&url).send().await?.error_for_status()?;
    Ok(response.json::<Vec<FabricLoaderEntry>>().await?)
}

/// Pick the best loader for a Minecraft version.
pub async fn resolve_loader(
    client: &reqwest::Client,
    mc_version: &str,
) -> anyhow::Result<FabricLoader> {
    let entries = fetch_loader_entries(client, mc_version).await?;

    let chosen = entries
        .iter()
        .find(|e| e.loader.stable)
        .or_else(|| entries.first())
        .ok_or_else(|| anyhow::anyhow!("Fabric has no loader build for Minecraft {mc_version}"))?;

    let profile = fetch_profile(client, mc_version, chosen.loader_version()).await?;

    Ok(FabricLoader {
        loader_version: chosen.loader_version().to_string(),
        intermediary_version: chosen.intermediary_version().to_string(),
        profile,
    })
}

/// `GET /v2/versions/loader/{game}/{loader}/profile/json`
pub async fn fetch_profile(
    client: &reqwest::Client,
    mc_version: &str,
    loader_version: &str,
) -> anyhow::Result<FabricProfile> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{mc_version}/{loader_version}/profile/json"
    );
    let response = client.get(&url).send().await?.error_for_status()?;
    Ok(response.json::<FabricProfile>().await?)
}

/// Maven `group:artifact` — what identifies a dependency regardless of which
/// version was resolved for it.
///
/// `org.ow2.asm:asm:9.6` and `org.ow2.asm:asm:9.10.1` are the *same* library
/// at different versions: they provide identical classes. Comparing full
/// coordinates (which include the version) is why both used to end up on the
/// classpath.
fn maven_group_artifact(name: &str) -> Option<&str> {
    let mut parts = name.split(':').take(3);
    let group = parts.next()?;
    let artifact = parts.next()?;
    if group.is_empty() || artifact.is_empty() {
        return None;
    }
    Some(&name[..group.len() + 1 + artifact.len()])
}

/// Merge a loader profile into a vanilla version JSON in place.
///
/// Vanilla libraries stay first so a loader can never shadow a Mojang jar with
/// an unexpected version, and the loader's `mainClass` wins.
///
/// Where the loader ships a different version of a library vanilla already
/// provides, the **loader's version replaces it**. Both cannot coexist: the
/// duplicate classes make Fabric's `verifyClasspath` refuse to start the game.
pub fn merge_into(version: &mut crate::manifest::mojang::VersionJson, loader: &FabricLoader) {
    version.main_class = loader
        .profile
        .main_class
        .clone()
        .unwrap_or_else(|| "net.fabricmc.loader.impl.launch.knot.KnotClient".to_string());

    // Every `group:artifact` the loader pins is authoritative.
    let claimed: std::collections::HashSet<&str> = loader
        .profile
        .libraries
        .iter()
        .filter_map(|l| maven_group_artifact(&l.name))
        .collect();

    version
        .libraries
        .retain(|library| match maven_group_artifact(&library.name) {
            Some(ga) => !claimed.contains(ga),
            // Unparseable names are left alone rather than guessed at.
            None => true,
        });

    // Then add the loader's own copy, skipping exact duplicates.
    let mut existing: std::collections::HashSet<String> =
        version.libraries.iter().map(|l| l.name.clone()).collect();

    for library in &loader.profile.libraries {
        if existing.insert(library.name.clone()) {
            version.libraries.push(library.clone());
        }
    }

    // Loader-only JVM properties (`-Dfabric.skipMcProvider`, game version, ...).
    if let Some(arguments) = &loader.profile.arguments {
        let target = version.arguments.get_or_insert_with(Default::default);
        for arg in &arguments.jvm {
            target.jvm.push(arg.clone());
        }
        for arg in &arguments.game {
            target.game.push(arg.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::mojang::{Library, VersionJson};

    #[test]
    fn parses_loader_entry_list() {
        // Shape returned by /v2/versions/loader/1.21.4
        let json = r#"[{
            "loader": {"separator": ".", "build": 12, "maven": "net.fabricmc:fabric-loader:0.16.10", "version": "0.16.10", "stable": true},
            "intermediary": {"maven": "net.fabricmc:intermediary:1.21.4", "version": "1.21.4", "stable": true},
            "launcherMeta": {"version": 2, "libraries": {}}
        }]"#;
        let entries: Vec<FabricLoaderEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].loader_version(), "0.16.10");
        assert!(entries[0].loader.stable);
    }

    #[test]
    fn parses_profile_with_url_only_libraries() {
        let json = r#"{
            "id": "fabric-loader-0.16.10-1.21.4",
            "inheritsFrom": "1.21.4",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"jvm": ["-DFabricMcEmu=net.minecraft.client.main.Main "], "game": []},
            "libraries": [
                {"name": "org.ow2.asm:asm:9.7.1", "url": "https://maven.fabricmc.net/"},
                {"name": "net.fabricmc:sponge-mixin:0.15.4+mixin.0.8.7", "url": "https://maven.fabricmc.net/"}
            ]
        }"#;
        let profile: FabricProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.libraries.len(), 2);
        assert!(profile.libraries[0].downloads.artifact.is_none());
        assert_eq!(
            profile.libraries[0].url.as_deref(),
            Some("https://maven.fabricmc.net/")
        );
    }

    #[test]
    fn merge_sets_main_class_and_skips_duplicate_libraries() {
        let mut vanilla = VersionJson {
            id: "1.21.4".into(),
            kind: "release".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            assets: None,
            asset_index: None,
            downloads: Default::default(),
            libraries: vec![Library {
                name: "org.ow2.asm:asm:9.7.1".to_string(),
                url: None,
                downloads: Default::default(),
                natives: Default::default(),
                rules: Vec::new(),
                extract: None,
            }],
            arguments: None,
            minecraft_arguments: None,
            java_version: None,
            inherits_from: None,
        };

        let loader = FabricLoader {
            loader_version: "0.16.10".into(),
            intermediary_version: "1.21.4".into(),
            profile: FabricProfile {
                id: "fabric-loader-0.16.10-1.21.4".into(),
                main_class: Some("net.fabricmc.loader.impl.launch.knot.KnotClient".into()),
                inherits_from: Some("1.21.4".into()),
                libraries: vec![
                    // Duplicate of the vanilla list — must not be added twice.
                    Library {
                        name: "org.ow2.asm:asm:9.7.1".to_string(),
                        url: Some("https://maven.fabricmc.net/".into()),
                        downloads: Default::default(),
                        natives: Default::default(),
                        rules: Vec::new(),
                        extract: None,
                    },
                    Library {
                        name: "net.fabricmc:sponge-mixin:0.15.4".to_string(),
                        url: Some("https://maven.fabricmc.net/".into()),
                        downloads: Default::default(),
                        natives: Default::default(),
                        rules: Vec::new(),
                        extract: None,
                    },
                ],
                arguments: None,
                minecraft_arguments: None,
            },
        };

        merge_into(&mut vanilla, &loader);

        assert_eq!(
            vanilla.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(
            vanilla.libraries.len(),
            2,
            "duplicate ASM library was added"
        );
        assert!(vanilla
            .libraries
            .iter()
            .any(|l| l.name.starts_with("net.fabricmc:sponge-mixin")));
    }

    /// Regression: vanilla ships `org.ow2.asm:asm:9.6` and Fabric 0.19.x ships
    /// `9.10.1`. De-duplicating on the *full* coordinate kept both, and the
    /// duplicate classes made Fabric abort with
    /// `duplicate ASM classes found on classpath` before the game ever started.
    #[test]
    fn merge_replaces_vanilla_library_with_a_different_loader_version() {
        let vanilla_lib = |name: &str| Library {
            name: name.to_string(),
            url: Some("https://libraries.minecraft.net/".into()),
            downloads: Default::default(),
            natives: Default::default(),
            rules: Vec::new(),
            extract: None,
        };

        let mut vanilla = VersionJson {
            id: "1.21.4".into(),
            kind: "release".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            assets: None,
            asset_index: None,
            downloads: Default::default(),
            libraries: vec![
                vanilla_lib("org.ow2.asm:asm:9.6"),
                vanilla_lib("org.ow2.asm:asm-tree:9.6"),
                // Unrelated library: must survive the merge untouched.
                vanilla_lib("com.google.guava:guava:33.3.1-jre"),
            ],
            arguments: None,
            minecraft_arguments: None,
            java_version: None,
            inherits_from: None,
        };

        let loader = FabricLoader {
            loader_version: "0.19.5".into(),
            intermediary_version: "1.21.4".into(),
            profile: FabricProfile {
                id: "fabric-loader-0.19.5-1.21.4".into(),
                main_class: None,
                inherits_from: Some("1.21.4".into()),
                libraries: vec![
                    vanilla_lib("org.ow2.asm:asm:9.10.1"),
                    vanilla_lib("org.ow2.asm:asm-tree:9.10.1"),
                    vanilla_lib("net.fabricmc:fabric-loader:0.19.5"),
                ],
                arguments: None,
                minecraft_arguments: None,
            },
        };

        merge_into(&mut vanilla, &loader);

        let names: Vec<&str> = vanilla.libraries.iter().map(|l| l.name.as_str()).collect();
        assert!(
            !names.contains(&"org.ow2.asm:asm:9.6"),
            "the stale vanilla ASM must be replaced, got {names:?}"
        );
        assert!(names.contains(&"org.ow2.asm:asm:9.10.1"));
        assert!(!names.contains(&"org.ow2.asm:asm-tree:9.6"));
        assert!(names.contains(&"org.ow2.asm:asm-tree:9.10.1"));
        assert!(names.contains(&"com.google.guava:guava:33.3.1-jre"));

        // The property that actually matters: one jar per Maven artifact.
        let mut gas: Vec<&str> = names
            .iter()
            .filter_map(|n| maven_group_artifact(n))
            .collect();
        let total = gas.len();
        gas.sort_unstable();
        gas.dedup();
        assert_eq!(
            total,
            gas.len(),
            "duplicate library on the classpath: {names:?}"
        );
    }

    #[test]
    fn group_artifact_parsing() {
        assert_eq!(
            maven_group_artifact("org.ow2.asm:asm:9.10.1"),
            Some("org.ow2.asm:asm")
        );
        assert_eq!(
            maven_group_artifact("net.fabricmc:fabric-loader:0.19.5"),
            Some("net.fabricmc:fabric-loader")
        );
        // Classifier-suffixed coordinates still resolve to the same artifact.
        assert_eq!(
            maven_group_artifact("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            Some("org.lwjgl:lwjgl")
        );
        assert_eq!(maven_group_artifact("nonsense"), None);
    }
}
