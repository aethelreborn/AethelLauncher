use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FabricLoader {
    pub loader_version: String,
    pub intermediary_version: String,
    pub profile: FabricProfile,
}

pub async fn fetch_loader_entries(
    client: &reqwest::Client,
    mc_version: &str,
) -> anyhow::Result<Vec<FabricLoaderEntry>> {
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc_version}");
    let response = client.get(&url).send().await?.error_for_status()?;
    Ok(response.json::<Vec<FabricLoaderEntry>>().await?)
}

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

fn maven_group_artifact(name: &str) -> Option<&str> {
    let mut parts = name.split(':').take(3);
    let group = parts.next()?;
    let artifact = parts.next()?;
    if group.is_empty() || artifact.is_empty() {
        return None;
    }
    Some(&name[..group.len() + 1 + artifact.len()])
}

pub fn merge_into(version: &mut crate::manifest::mojang::VersionJson, loader: &FabricLoader) {
    version.main_class = loader
        .profile
        .main_class
        .clone()
        .unwrap_or_else(|| "net.fabricmc.loader.impl.launch.knot.KnotClient".to_string());

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
            None => true,
        });

    let mut existing: std::collections::HashSet<String> =
        version.libraries.iter().map(|l| l.name.clone()).collect();

    for library in &loader.profile.libraries {
        if existing.insert(library.name.clone()) {
            version.libraries.push(library.clone());
        }
    }

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
        assert_eq!(
            maven_group_artifact("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            Some("org.lwjgl:lwjgl")
        );
        assert_eq!(maven_group_artifact("nonsense"), None);
    }
}
