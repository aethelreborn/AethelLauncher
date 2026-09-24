//! Mojang version manifest and per-version JSON parsing.
//!
//! See [05 · Launch engine](../../opencode-docs/05-launch-engine.md) for the full resolution pipeline.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

// ---------------------------------------------------------------------------
// version_manifest_v2.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<VersionRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRecord {
    pub id: String,
    pub url: String,
    pub time: DateTime<Utc>,
    #[serde(rename = "releaseTime", default)]
    pub release_time: Option<DateTime<Utc>>,
    /// `"release" | "snapshot" | "old_beta" | "old_alpha"`
    #[serde(rename = "type")]
    pub type_: String,
}

// ---------------------------------------------------------------------------
// Per-version JSON (`<id>.json`)
// ---------------------------------------------------------------------------

/// A full per-version JSON document.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    pub main_class: String,
    /// Base `assets` name (legacy versions only).
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: VersionDownloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<VersionArguments>,
    /// Legacy (`<= 1.13`) single-string argument list.
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    /// Fabric/Forge-style inheritance chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDownloads {
    #[serde(default)]
    pub client: Option<Artifact>,
    #[serde(default)]
    pub client_mappings: Option<Artifact>,
    #[serde(default)]
    pub server: Option<Artifact>,
    #[serde(default)]
    pub server_mappings: Option<Artifact>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub total_size: u64,
}

/// A downloadable file. `path` is a Maven-style relative path and is only
/// present on library artifacts; it can be derived from the library name when
/// Mojang omits it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Artifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

impl Artifact {
    /// Relative path for this artifact, deriving it from a Maven name when the
    /// manifest omits `path` (common on pre-1.19 versions).
    pub fn relative_path(&self, fallback_name: Option<&str>) -> Option<String> {
        if let Some(p) = &self.path {
            if !p.is_empty() {
                return Some(p.clone());
            }
        }
        fallback_name.map(maven_to_path)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    #[serde(default)]
    pub component: String,
    #[serde(rename = "majorVersion", default)]
    pub major: u32,
}

/// `arguments` object — modern (1.13+) argument format.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VersionArguments {
    #[serde(default)]
    pub game: Vec<Arg>,
    #[serde(default)]
    pub jvm: Vec<Arg>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Arg {
    String(String),
    Object(ArgObject),
}

/// A conditional argument: a rules list plus one or more values.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArgObject {
    #[serde(default)]
    pub rules: Vec<Rule>,
    pub value: ArgValue,
}

/// Mojang emits `value` as either a single string or an array of strings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

impl ArgValue {
    pub fn as_slice(&self) -> Vec<&str> {
        match self {
            ArgValue::One(s) => vec![s.as_str()],
            ArgValue::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    /// `"allow" | "disallow"`
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Library {
    /// Maven coordinates: `group:artifact:version[:classifier][@ext]`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub downloads: LibraryDownloads,
    /// Legacy native mapping, e.g. `{"windows": "natives-windows"}`.
    #[serde(default)]
    pub natives: HashMap<String, String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LibraryDownloads {
    /// Main jar. Absent for libraries Mojang bundles inside `client.jar`
    /// (the 1.21.x `client-extra` case) — those are skipped on the classpath.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: HashMap<String, Artifact>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

/// Mojang's OS identifier: `"windows" | "osx" | "linux"`.
pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// Mojang rules only ever distinguish `x86` from everything else, so report the
/// raw architecture and let [`arch_matches`] do a tolerant comparison.
pub fn os_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        std::env::consts::ARCH
    }
}

/// `;` on Windows, `:` everywhere else.
pub fn classpath_separator() -> &'static str {
    if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    }
}

/// Legacy helper — the classifier suffix for this platform.
pub fn native_classifier() -> &'static str {
    if cfg!(target_os = "windows") {
        "natives-windows"
    } else if cfg!(target_os = "macos") {
        "natives-macos"
    } else {
        "natives-linux"
    }
}

/// Keys a `Library::natives` map may use for the current platform. Modern
/// manifests use `macos`, older ones use `osx`.
pub fn native_keys() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["windows"]
    } else if cfg!(target_os = "macos") {
        &["osx", "macos"]
    } else {
        &["linux"]
    }
}

/// Tolerant architecture match. Mojang writes `x86` for 32-bit and
/// `x86_64`/`arm64` for 64-bit natives.
pub fn arch_matches(rule_arch: &str) -> bool {
    let actual = os_arch();
    match rule_arch {
        // Mojang's `x86` marker means 32-bit and must NOT match x86_64.
        "x86" => matches!(actual, "x86" | "i386" | "i686"),
        "x86_64" | "amd64" => actual == "x86_64",
        "arm64" | "aarch64" => matches!(actual, "aarch64" | "arm64"),
        other => actual == other,
    }
}

// ---------------------------------------------------------------------------
// Rules evaluation
// ---------------------------------------------------------------------------

/// Evaluate a Mojang rules list. An empty list means "allowed" — otherwise the
/// last matching rule wins and nothing matches means disallowed.
pub fn rules_allow(rules: &[Rule], features: &BTreeMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        if rule_applies(rule, features) {
            allow = rule.action == "allow";
        }
    }
    allow
}

fn rule_applies(rule: &Rule, features: &BTreeMap<String, bool>) -> bool {
    if let Some(os) = &rule.os {
        if let Some(name) = &os.name {
            if !name.eq_ignore_ascii_case(os_name()) {
                return false;
            }
        }
        if let Some(arch) = &os.arch {
            if !arch_matches(arch) {
                return false;
            }
        }
        // `os.version` is a regex over the OS release string. Matching it
        // reliably across distros is not worth the failure mode, so we accept.
    }
    if let Some(required) = &rule.features {
        for (key, want) in required {
            let have = features.get(key).copied().unwrap_or(false);
            if have != *want {
                return false;
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Maven coordinates
// ---------------------------------------------------------------------------

/// `com.mojang:authlib:6.0.54` → `com/mojang/authlib/6.0.54/authlib-6.0.54.jar`
///
/// Also handles the optional classifier (`group:artifact:version:classifier`)
/// and extension (`group:artifact:version@app`) forms.
pub fn maven_to_path(name: &str) -> String {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return String::new();
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied().filter(|c| !c.is_empty());
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    format!("{group}/{artifact}/{version}/{file}")
}

/// Parse a semantic-like version string into (major, minor, patch).
pub fn parse_mc_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim_start_matches('v');
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let major: u64 = parts[0].parse().ok()?;
    let minor: u64 = parts[1].parse().ok()?;
    let patch: u64 = if parts.len() > 2 {
        parts[2]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    } else {
        0
    };
    Some((major, minor, patch))
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

/// Parse the Mojang version manifest JSON.
pub async fn fetch_version_manifest(client: &reqwest::Client) -> anyhow::Result<VersionManifest> {
    let resp = client
        .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json::<VersionManifest>().await?)
}

/// Fetch a specific version's JSON by URL.
pub async fn fetch_version_json(
    client: &reqwest::Client,
    version_url: &str,
) -> anyhow::Result<VersionJson> {
    let resp = client.get(version_url).send().await?.error_for_status()?;
    Ok(resp.json::<VersionJson>().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mc_version() {
        assert_eq!(parse_mc_version("1.21.11"), Some((1, 21, 11)));
        assert_eq!(parse_mc_version("26.2.0"), Some((26, 2, 0)));
        assert_eq!(parse_mc_version("1.16.5"), Some((1, 16, 5)));
        assert_eq!(parse_mc_version("invalid"), None);
    }

    #[test]
    fn test_native_classifier() {
        assert!(native_classifier().starts_with("natives-"));
    }

    #[test]
    fn test_maven_to_path() {
        assert_eq!(
            maven_to_path("com.mojang:authlib:6.0.54"),
            "com/mojang/authlib/6.0.54/authlib-6.0.54.jar"
        );
        assert_eq!(
            maven_to_path("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
        );
        assert_eq!(
            maven_to_path("net.minecraft:client:1.21.4@jar"),
            "net/minecraft/client/1.21.4/client-1.21.4.jar"
        );
        assert_eq!(maven_to_path("nonsense"), "");
    }

    /// The real 1.21.4 shape: nested `os` rules and `value` arrays must parse.
    #[test]
    fn parses_real_version_json_shape() {
        let json = r#"{
          "id": "1.21.4",
          "type": "release",
          "mainClass": "net.minecraft.client.main.Main",
          "assets": "19",
          "assetIndex": {"id":"19","sha1":"abc","size":123,"totalSize":456,"url":"https://example.com/19.json"},
          "javaVersion": {"component":"java-runtime-delta","majorVersion":21},
          "downloads": {
            "client": {"sha1":"aa","size":10,"url":"https://example.com/client.jar"},
            "server": {"sha1":"bb","size":20,"url":"https://example.com/server.jar"}
          },
          "libraries": [
            {
              "name": "org.lwjgl:lwjgl:3.3.3",
              "downloads": {
                "artifact": {"path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar","sha1":"cc","size":1,"url":"https://example.com/lwjgl.jar"},
                "classifiers": {
                  "natives-linux": {"path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar","sha1":"dd","size":2,"url":"https://example.com/lwjgl-natives.jar"}
                }
              },
              "natives": {"linux":"natives-linux","osx":"natives-macos","windows":"natives-windows"}
            },
            {
              "name": "com.mojang:bundled-in-client:1.0",
              "downloads": {}
            },
            {
              "name": "org.lwjgl:lwjgl:3.3.3",
              "rules": [{"action":"allow","os":{"name":"osx"}}]
            }
          ],
          "arguments": {
            "game": ["--username","${auth_player_name}",{"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}],
            "jvm": [
              {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":["-XstartOnFirstThread"]},
              "-Djava.library.path=${natives_directory}",
              "-cp","${classpath}"
            ]
          }
        }"#;

        let v: VersionJson = serde_json::from_str(json).expect("parse version json");
        assert_eq!(v.id, "1.21.4");
        assert_eq!(v.kind, "release");
        assert_eq!(v.main_class, "net.minecraft.client.main.Main");
        assert_eq!(v.java_version.as_ref().unwrap().major, 21);
        assert_eq!(v.downloads.client.as_ref().unwrap().size, 10);
        assert_eq!(v.asset_index.as_ref().unwrap().id, "19");
        assert_eq!(v.libraries.len(), 3);
        // bundled lib flagged by a missing artifact
        assert!(v.libraries[1].downloads.artifact.is_none());
        let natives = &v.libraries[0].downloads.classifiers;
        assert!(natives.contains_key("natives-linux"));
        let args = v.arguments.as_ref().unwrap();
        assert_eq!(args.game.len(), 3);
        assert_eq!(args.jvm.len(), 4);
    }

    #[test]
    fn rules_default_allow_and_ordering() {
        let features = BTreeMap::new();
        assert!(rules_allow(&[], &features));

        // Last matching rule wins.
        let rules = vec![
            Rule {
                action: "allow".into(),
                os: None,
                features: None,
            },
            Rule {
                action: "disallow".into(),
                os: Some(OsRule {
                    name: Some(os_name().into()),
                    ..Default::default()
                }),
                features: None,
            },
        ];
        assert!(!rules_allow(&rules, &features));

        // A rule for a different OS never matches, so default (deny) stands.
        let other_os = if os_name() == "windows" {
            "linux"
        } else {
            "windows"
        };
        let rules = vec![Rule {
            action: "allow".into(),
            os: Some(OsRule {
                name: Some(other_os.into()),
                ..Default::default()
            }),
            features: None,
        }];
        assert!(!rules_allow(&rules, &features));
    }

    #[test]
    fn feature_rules_require_matching_flags() {
        let mut features = BTreeMap::new();
        features.insert("has_custom_resolution".to_string(), true);
        let rules = vec![Rule {
            action: "allow".into(),
            os: None,
            features: Some(BTreeMap::from([(
                "has_custom_resolution".to_string(),
                true,
            )])),
        }];
        assert!(rules_allow(&rules, &features));

        let empty = BTreeMap::new();
        assert!(!rules_allow(&rules, &empty));
    }

    #[test]
    fn arch_matching_tolerates_rule_shorthand() {
        // The rule `x86` is Mojang's 32-bit marker; it must not match x86_64.
        if os_arch() == "x86_64" {
            assert!(!arch_matches("x86"));
            assert!(arch_matches("x86_64"));
        }
    }
}
