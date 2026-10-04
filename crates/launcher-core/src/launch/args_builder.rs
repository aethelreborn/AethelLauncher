use crate::auth::Account;
use crate::manifest::mojang::{classpath_separator, rules_allow, Arg, VersionJson};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ArgContext {
    pub auth: Account,
    pub version_id: String,
    pub version_name: String,
    pub version_type: String,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub assets_index_name: String,
    pub natives_dir: PathBuf,
    pub library_dir: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub launcher_name: String,
    pub launcher_version: String,
    pub width: u32,
    pub height: u32,
    pub renderer: String,
    pub ram_mb: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LaunchArgs {
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub classpath: Vec<PathBuf>,
}

impl ArgContext {
    fn uuid_nodash(&self) -> String {
        self.auth.uuid_str().replace('-', "")
    }

    fn user_type(&self) -> &'static str {
        match self.auth {
            Account::Offline { .. } => "legacy",
            Account::Microsoft { .. } => "msa",
        }
    }

    fn access_token(&self) -> String {
        match &self.auth {
            Account::Offline { .. } => "0".to_string(),
            Account::Microsoft {
                mc_access_token, ..
            } => mc_access_token.clone(),
        }
    }

    fn tokens(&self) -> BTreeMap<&'static str, String> {
        let classpath = self
            .classpath
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(classpath_separator());

        let mut t: BTreeMap<&'static str, String> = BTreeMap::new();
        t.insert("auth_player_name", self.auth.username().to_string());
        t.insert("auth_uuid", self.uuid_nodash());
        t.insert("auth_access_token", self.access_token());
        t.insert("auth_xuid", "0".to_string());
        t.insert("auth_session", self.access_token());
        t.insert("clientid", "0".to_string());
        t.insert("user_type", self.user_type().to_string());
        t.insert("user_properties", "{}".to_string());
        t.insert("version_name", self.version_name.clone());
        t.insert("version_type", self.version_type.clone());
        t.insert("profile_name", self.version_name.clone());
        t.insert(
            "game_directory",
            self.game_dir.to_string_lossy().to_string(),
        );
        t.insert("assets_root", self.assets_dir.to_string_lossy().to_string());
        t.insert("assets_index_name", self.assets_index_name.clone());
        t.insert(
            "natives_directory",
            self.natives_dir.to_string_lossy().to_string(),
        );
        t.insert(
            "library_directory",
            self.library_dir.to_string_lossy().to_string(),
        );
        t.insert("classpath", classpath);
        t.insert("classpath_separator", classpath_separator().to_string());
        t.insert("launcher_name", self.launcher_name.clone());
        t.insert("launcher_version", self.launcher_version.clone());
        t.insert("resolution_width", self.width.to_string());
        t.insert("resolution_height", self.height.to_string());
        t.insert("quickPlayPath", String::new());
        t.insert("quickPlaySingleplayer", String::new());
        t.insert("quickPlayMultiplayer", String::new());
        t.insert("quickPlayRealms", String::new());
        t
    }
}

pub fn expand(raw: &str, ctx: &ArgContext) -> String {
    let tokens = ctx.tokens();
    expand_with(raw, &tokens)
}

fn expand_with(raw: &str, tokens: &BTreeMap<&'static str, String>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match tokens.get(key) {
                    Some(value) => out.push_str(value),
                    None => tracing::warn!("unknown launch token ${{{key}}}"),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

fn flatten(list: &[Arg], features: &BTreeMap<String, bool>) -> Vec<String> {
    let mut out = Vec::new();
    for arg in list {
        match arg {
            Arg::String(s) => out.push(s.clone()),
            Arg::Object(obj) => {
                if rules_allow(&obj.rules, features) {
                    for v in obj.value.as_slice() {
                        out.push(v.to_string());
                    }
                }
            }
        }
    }
    out
}

pub fn feature_set() -> BTreeMap<String, bool> {
    BTreeMap::new()
}

pub fn perf_jvm_flags(ram_mb: u64, renderer: &str) -> Vec<String> {
    let total_mb = total_system_ram_mb();
    let low_memory = total_mb > 0 && total_mb <= 8192;

    let mut flags = vec![
        format!("-Xms{ram_mb}M"),
        format!("-Xmx{ram_mb}M"),
        "-XX:+UseG1GC".to_string(),
        "-XX:+ParallelRefProcEnabled".to_string(),
        "-XX:MaxGCPauseMillis=200".to_string(),
        "-XX:+UnlockExperimentalVMOptions".to_string(),
        "-XX:+DisableExplicitGC".to_string(),
        "-XX:G1NewSizePercent=30".to_string(),
        "-XX:G1MaxNewSizePercent=40".to_string(),
        "-XX:G1HeapRegionSize=8M".to_string(),
        "-XX:G1ReservePercent=20".to_string(),
        "-XX:G1MixedGCCountTarget=4".to_string(),
        "-XX:InitiatingHeapOccupancyPercent=15".to_string(),
        "-XX:G1MixedGCLiveThresholdPercent=90".to_string(),
        "-XX:SurvivorRatio=32".to_string(),
        "-XX:MaxTenuringThreshold=1".to_string(),
        "-Dlog4j2.formatMsgNoLookups=true".to_string(),
        format!("-Daethel.renderer={renderer}"),
    ];

    if !low_memory {
        flags.push("-XX:+AlwaysPreTouch".to_string());
    }

    flags
}

fn total_system_ram_mb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    if let Some(kb) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb.parse::<u64>() {
                            return kb / 1024;
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

pub fn build_args(version: &VersionJson, ctx: &ArgContext) -> LaunchArgs {
    let features = feature_set();

    let mut jvm_args = perf_jvm_flags(ctx.ram_mb, &ctx.renderer);

    match &version.arguments {
        Some(args) if !args.jvm.is_empty() => {
            let raw = flatten(&args.jvm, &features);
            jvm_args.extend(raw.iter().map(|a| expand_with(a, &ctx.tokens())));
        }
        _ => {
            jvm_args.push(format!(
                "-Djava.library.path={}",
                ctx.natives_dir.to_string_lossy()
            ));
            let cp = ctx
                .classpath
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(classpath_separator());
            jvm_args.push("-cp".to_string());
            jvm_args.push(cp);
        }
    }

    let game_args = match &version.arguments {
        Some(args) if !args.game.is_empty() => {
            let raw = flatten(&args.game, &features);
            raw.iter().map(|a| expand_with(a, &ctx.tokens())).collect()
        }
        _ if version.minecraft_arguments.is_some() => {
            let raw = version.minecraft_arguments.clone().unwrap_or_default();
            raw.split_whitespace()
                .map(|a| expand_with(a, &ctx.tokens()))
                .collect()
        }
        _ => Vec::new(),
    };

    LaunchArgs {
        jvm_args,
        game_args,
        classpath: ctx.classpath.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_for(
    version: &VersionJson,
    auth: &Account,
    game_dir: &Path,
    assets_dir: &Path,
    natives_dir: &Path,
    library_dir: &Path,
    classpath: Vec<PathBuf>,
    ram_mb: u64,
    renderer: &str,
    assets_index_name: &str,
) -> LaunchArgs {
    let ctx = ArgContext {
        auth: auth.clone(),
        version_id: version.id.clone(),
        version_name: format!("Aethel {}", version.id),
        version_type: if version.kind.is_empty() {
            "release".to_string()
        } else {
            version.kind.clone()
        },
        game_dir: game_dir.to_path_buf(),
        assets_dir: assets_dir.to_path_buf(),
        assets_index_name: assets_index_name.to_string(),
        natives_dir: natives_dir.to_path_buf(),
        library_dir: library_dir.to_path_buf(),
        classpath,
        launcher_name: "aethel".to_string(),
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
        width: 854,
        height: 480,
        renderer: renderer.to_string(),
        ram_mb,
    };
    build_args(version, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::mojang::VersionJson;

    fn version_json() -> VersionJson {
        let json = r#"{
          "id": "1.21.4",
          "type": "release",
          "mainClass": "net.minecraft.client.main.Main",
          "assets": "19",
          "assetIndex": {"id":"19","url":"https://example.com/19.json"},
          "downloads": {"client": {"url":"https://example.com/client.jar"}},
          "libraries": [],
          "arguments": {
            "game": ["--username","${auth_player_name}","--uuid","${auth_uuid}","--accessToken","${auth_access_token}","--version","${version_name}","--gameDir","${game_directory}","--assetsDir","${assets_root}","--assetIndex","${assets_index_name}","--userType","${user_type}"],
            "jvm": ["-Djava.library.path=${natives_directory}","-cp","${classpath}"]
          }
        }"#;
        serde_json::from_str(json).unwrap()
    }

    fn offline() -> Account {
        Account::Offline {
            name: "Steve".to_string(),
            uuid: crate::auth::offline::offline_uuid("Steve"),
        }
    }

    #[test]
    fn expands_game_args_from_real_template() {
        let v = version_json();
        let args = build_for(
            &v,
            &offline(),
            Path::new("/tmp/game"),
            Path::new("/tmp/assets"),
            Path::new("/tmp/natives"),
            Path::new("/tmp/libs"),
            vec![
                PathBuf::from("/tmp/libs/a.jar"),
                PathBuf::from("/tmp/client.jar"),
            ],
            2048,
            "vulkan",
            "19",
        );

        let get = |flag: &str| {
            let i = args
                .game_args
                .iter()
                .position(|a| a == flag)
                .expect("flag present");
            args.game_args[i + 1].clone()
        };
        assert_eq!(get("--username"), "Steve");
        assert_eq!(get("--assetIndex"), "19");
        assert_eq!(get("--userType"), "legacy");
        assert_eq!(get("--version"), "Aethel 1.21.4");
        let uuid = get("--uuid");
        assert_eq!(uuid.len(), 32);
        assert!(!uuid.contains('-'));
    }

    #[test]
    fn offline_and_ms_tokens_differ() {
        let v = version_json();
        let ms = Account::Microsoft {
            mc_access_token: "tok".to_string(),
            username: "Alex".to_string(),
            uuid: "11111111-2222-3333-4444-555555555555".to_string(),
            msa_refresh_token: None,
        };
        let args = build_for(
            &v,
            &ms,
            Path::new("/tmp/game"),
            Path::new("/tmp/assets"),
            Path::new("/tmp/natives"),
            Path::new("/tmp/libs"),
            vec![],
            2048,
            "auto",
            "19",
        );
        let get = |flag: &str| {
            let i = args.game_args.iter().position(|a| a == flag).unwrap();
            args.game_args[i + 1].clone()
        };
        assert_eq!(get("--accessToken"), "tok");
        assert_eq!(get("--userType"), "msa");
        assert_eq!(get("--uuid"), "11111111222233334444555555555555");
    }

    #[test]
    fn classpath_uses_platform_separator() {
        let v = version_json();
        let a = if cfg!(windows) { "C:/a.jar" } else { "/a.jar" };
        let b = if cfg!(windows) { "C:/b.jar" } else { "/b.jar" };
        let args = build_for(
            &v,
            &offline(),
            Path::new("/tmp/game"),
            Path::new("/tmp/assets"),
            Path::new("/tmp/natives"),
            Path::new("/tmp/libs"),
            vec![PathBuf::from(a), PathBuf::from(b)],
            2048,
            "auto",
            "19",
        );
        let cp_index = args
            .jvm_args
            .iter()
            .position(|x| x == "-cp")
            .expect("-cp present");
        let joined = &args.jvm_args[cp_index + 1];
        assert_eq!(
            joined,
            &format!("{a}{}{b}", classpath_separator()),
            "classpath must join with the platform separator"
        );
        assert!(args.jvm_args.iter().any(|f| f == "-Xmx2048M"));
        assert!(args.jvm_args.iter().any(|f| f == "-Xms2048M"));
    }

    #[test]
    fn legacy_minecraft_arguments_are_split_and_expanded() {
        let mut v = version_json();
        v.arguments = None;
        v.minecraft_arguments = Some(
            "--username ${auth_player_name} --gameDir ${game_directory} --assetsDir ${assets_root}"
                .to_string(),
        );
        let args = build_for(
            &v,
            &offline(),
            Path::new("/tmp/game"),
            Path::new("/tmp/assets"),
            Path::new("/tmp/natives"),
            Path::new("/tmp/libs"),
            vec![],
            1024,
            "auto",
            "legacy",
        );
        assert_eq!(args.game_args[0], "--username");
        assert_eq!(args.game_args[1], "Steve");
        assert!(args
            .jvm_args
            .iter()
            .any(|f| f.starts_with("-Djava.library.path=")));
    }

    #[test]
    fn unknown_tokens_become_empty_not_fatal() {
        let tokens = BTreeMap::new();
        assert_eq!(expand_with("a${nope}b", &tokens), "ab");
        assert_eq!(expand_with("plain", &tokens), "plain");
        assert_eq!(expand_with("${unterminated", &tokens), "${unterminated");
    }

    #[test]
    fn low_memory_skips_pretouch() {
        let flags = perf_jvm_flags(1024, "vulkan");
        assert!(flags.iter().any(|f| f == "-Xmx1024M"));
        assert!(flags.iter().any(|f| f == "-Daethel.renderer=vulkan"));
    }
}
