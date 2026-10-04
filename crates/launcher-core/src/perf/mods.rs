use crate::install::downloader::{download_many, file_matches, DownloadTask};
use crate::perf::Renderer;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const MANAGED_MANIFEST: &str = ".aethel-managed.json";

pub type PackProgress = Arc<dyn Fn(usize, usize, &str) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderTarget {
    Any,
    Opengl,
    Vulkan,
}

#[derive(Debug, Clone, Copy)]
pub struct CuratedMod {
    pub slug: &'static str,
    pub name: &'static str,
    pub core: bool,
    pub target: RenderTarget,
}

pub const PERFORMANCE_PACK: &[CuratedMod] = &[
    CuratedMod {
        slug: "vulkanmod",
        name: "VulkanMod",
        core: true,
        target: RenderTarget::Vulkan,
    },
    CuratedMod {
        slug: "sodium",
        name: "Sodium",
        core: true,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "fabric-api",
        name: "Fabric API",
        core: true,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "lithium",
        name: "Lithium",
        core: true,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "ferrite-core",
        name: "FerriteCore",
        core: true,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "c2me-fabric",
        name: "Concurrent Chunk Management Engine",
        core: true,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "krypton",
        name: "Krypton",
        core: true,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "entityculling",
        name: "EntityCulling",
        core: false,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "immediatelyfast",
        name: "ImmediatelyFast",
        core: false,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "moreculling",
        name: "More Culling",
        core: false,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "sodium-extra",
        name: "Sodium Extra",
        core: false,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "reeses-sodium-options",
        name: "Reese's Sodium Options",
        core: false,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "exordium",
        name: "Exordium",
        core: false,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "scalablelux",
        name: "ScalableLux",
        core: false,
        target: RenderTarget::Opengl,
    },
    CuratedMod {
        slug: "dynamic-fps",
        name: "Dynamic FPS",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "memoryleakfix",
        name: "Memory Leak Fix",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "badoptimizations",
        name: "BadOptimizations",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "very-many-players",
        name: "Very Many Players",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "threadtweak",
        name: "ThreadTweak",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "noisium",
        name: "Noisium",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "lazydfu",
        name: "LazyDFU",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "debugify",
        name: "Debugify",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "modernfix",
        name: "ModernFix",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "cloth-config",
        name: "Cloth Config API",
        core: false,
        target: RenderTarget::Any,
    },
    CuratedMod {
        slug: "modmenu",
        name: "Mod Menu",
        core: false,
        target: RenderTarget::Any,
    },
];

pub fn pack_for(renderer: Renderer) -> Vec<CuratedMod> {
    let renderer = renderer.resolved();
    PERFORMANCE_PACK
        .iter()
        .filter(|m| match m.target {
            RenderTarget::Any => true,
            RenderTarget::Opengl => renderer == Renderer::Opengl,
            RenderTarget::Vulkan => renderer == Renderer::Vulkan,
        })
        .copied()
        .collect()
}

pub fn pack_label(renderer: Renderer) -> &'static str {
    match renderer.resolved() {
        Renderer::Vulkan => "Vulkan — VulkanMod + optimisations",
        _ => "OpenGL — Sodium + optimisations",
    }
}

pub fn uses_sodium(renderer: Renderer) -> bool {
    renderer.resolved() == Renderer::Opengl
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: ModrinthHashes,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ModrinthHashes {
    #[serde(default)]
    sha1: Option<String>,
    #[serde(default)]
    sha512: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthVersion {
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstalledMod {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub filename: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ManagedManifest {
    pub mc_version: String,
    pub loader: String,
    #[serde(default)]
    pub renderer: String,
    pub installed: Vec<InstalledMod>,
    pub skipped: Vec<String>,
}

impl ManagedManifest {
    pub fn path(mods_dir: &Path) -> PathBuf {
        mods_dir.join(MANAGED_MANIFEST)
    }

    pub fn load(mods_dir: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(Self::path(mods_dir)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self, mods_dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(mods_dir)?;
        let raw = serde_json::to_vec_pretty(self)?;
        std::fs::write(Self::path(mods_dir), raw)?;
        Ok(())
    }
}

pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!(
            "AethelLauncher/",
            env!("CARGO_PKG_VERSION"),
            " (Minecraft launcher; +https://github.com/aethel-launcher)"
        ))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build the Modrinth HTTP client")
}

async fn resolve_mod(
    client: &reqwest::Client,
    slug: &str,
    mc_version: &str,
    loader: &str,
) -> anyhow::Result<Option<(ModrinthFile, String)>> {
    let url = format!("{MODRINTH_API}/project/{slug}/version");
    let response = client
        .get(&url)
        .query(&[
            ("loaders", format!(r#"["{loader}"]"#)),
            ("game_versions", format!(r#"["{mc_version}"]"#)),
        ])
        .send()
        .await
        .with_context(|| format!("Modrinth request failed for {slug}"))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;

    let versions: Vec<ModrinthVersion> = response
        .json()
        .await
        .with_context(|| format!("Modrinth returned an unexpected payload for {slug}"))?;

    let Some(version) = versions
        .into_iter()
        .find(|v| v.game_versions.is_empty() || v.game_versions.iter().any(|g| g == mc_version))
    else {
        return Ok(None);
    };

    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.iter().find(|f| f.filename.ends_with(".jar")))
        .cloned();

    Ok(file.map(|f| (f, version.version_number)))
}

pub async fn install_performance_pack(
    client: &reqwest::Client,
    mods_dir: &Path,
    mc_version: &str,
    loader: &str,
    renderer: Renderer,
    progress: PackProgress,
) -> anyhow::Result<ManagedManifest> {
    tokio::fs::create_dir_all(mods_dir)
        .await
        .with_context(|| format!("failed to create {}", mods_dir.display()))?;

    let pack = pack_for(renderer);
    let total = pack.len();
    let mut manifest = ManagedManifest {
        mc_version: mc_version.to_string(),
        loader: loader.to_string(),
        renderer: renderer.resolved().as_str().to_string(),
        ..Default::default()
    };

    let mut tasks: Vec<DownloadTask> = Vec::new();
    let mut pending: Vec<InstalledMod> = Vec::new();

    for (index, curated) in pack.iter().enumerate() {
        progress(index, total, curated.name);

        let resolved = match resolve_mod(client, curated.slug, mc_version, loader).await {
            Ok(resolved) => resolved,
            Err(e) => {
                tracing::debug!("could not resolve {}: {e:#}", curated.slug);
                None
            }
        };

        let Some((file, version)) = resolved else {
            manifest.skipped.push(curated.name.to_string());
            continue;
        };

        let filename = decode_filename(&file.filename);
        let dest = mods_dir.join(&filename);
        let mut task = DownloadTask::new(&file.url, &dest);
        if let Some(sha1) = &file.hashes.sha1 {
            task = task.with_sha1(sha1);
        }
        if let Some(sha512) = &file.hashes.sha512 {
            task = task.with_sha512(sha512);
        }

        pending.push(InstalledMod {
            slug: curated.slug.to_string(),
            name: curated.name.to_string(),
            version,
            filename,
        });
        tasks.push(task);
    }

    let download_progress = {
        let progress = progress.clone();
        let total = tasks.len().max(1);
        Arc::new(move |done: usize, _all: usize| {
            progress(done, total, "Downloading mods");
        })
    };

    if !tasks.is_empty() {
        download_many(client, tasks, download_progress).await?;
    }

    manifest.installed = pending;
    manifest.save(mods_dir)?;

    Ok(manifest)
}

fn decode_filename(filename: &str) -> String {
    let decoded = filename.replace("%2B", "+").replace("%2b", "+");
    let cleaned: String = decoded
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == '\0' {
                '_'
            } else {
                c
            }
        })
        .collect();
    if cleaned.is_empty() {
        "mod.jar".to_string()
    } else {
        cleaned
    }
}

pub fn prune_managed(mods_dir: &Path, keep: &[String]) -> anyhow::Result<usize> {
    let Some(manifest) = ManagedManifest::load(mods_dir) else {
        return Ok(0);
    };

    let mut removed = 0usize;
    for installed in &manifest.installed {
        if keep.iter().any(|k| k == &installed.filename) {
            continue;
        }
        let path = mods_dir.join(&installed.filename);
        if path.is_file() && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn pack_is_installed(
    mods_dir: &Path,
    mc_version: &str,
    loader: &str,
    renderer: Renderer,
) -> bool {
    let Some(manifest) = ManagedManifest::load(mods_dir) else {
        return false;
    };
    if manifest.mc_version != mc_version
        || manifest.loader != loader
        || manifest.renderer != renderer.resolved().as_str()
    {
        return false;
    }
    if manifest.installed.is_empty() {
        return false;
    }
    manifest
        .installed
        .iter()
        .all(|m| file_matches(&mods_dir.join(&m.filename), None, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_core_first(pack: &[CuratedMod]) {
        let first_non_core = pack
            .iter()
            .position(|m| !m.core)
            .expect("pack should have non-core mods");
        let last_core = pack
            .iter()
            .rposition(|m| m.core)
            .expect("pack should have core mods");
        assert!(
            last_core < first_non_core,
            "core mods must come first so a timed-out install still helps"
        );
    }

    #[test]
    fn every_renderer_pack_is_ordered_with_core_mods_first() {
        assert_core_first(&pack_for(Renderer::Vulkan));
        assert_core_first(&pack_for(Renderer::Opengl));
        assert_core_first(&pack_for(Renderer::Auto));
    }

    #[test]
    fn pack_contains_the_high_impact_mods() {
        let opengl = pack_for(Renderer::Opengl);
        for slug in ["sodium", "lithium", "ferrite-core", "entityculling"] {
            assert!(
                opengl.iter().any(|m| m.slug == slug),
                "OpenGL pack is missing {slug}"
            );
        }
        let vulkan = pack_for(Renderer::Vulkan);
        assert!(
            vulkan.iter().any(|m| m.slug == "vulkanmod"),
            "Vulkan pack must install VulkanMod"
        );
        for slug in ["lithium", "ferrite-core"] {
            assert!(
                vulkan.iter().any(|m| m.slug == slug),
                "Vulkan pack is missing the renderer-agnostic {slug}"
            );
        }
        let mut slugs: Vec<&str> = PERFORMANCE_PACK.iter().map(|m| m.slug).collect();
        slugs.sort_unstable();
        let count = slugs.len();
        slugs.dedup();
        assert_eq!(count, slugs.len(), "duplicate slug in the performance pack");
    }

    #[test]
    fn sodium_and_vulkanmod_never_share_a_pack() {
        let slug_list =
            |r: Renderer| -> Vec<&'static str> { pack_for(r).iter().map(|m| m.slug).collect() };

        let vulkan = slug_list(Renderer::Vulkan);
        assert!(vulkan.contains(&"vulkanmod"));
        assert!(!vulkan.contains(&"sodium"));
        assert!(!vulkan.contains(&"sodium-extra"));
        assert!(!vulkan.contains(&"immediatelyfast"));

        let opengl = slug_list(Renderer::Opengl);
        assert!(opengl.contains(&"sodium"));
        assert!(!opengl.contains(&"vulkanmod"));

        assert_eq!(slug_list(Renderer::Auto), vulkan);
    }

    #[test]
    fn vulkan_pack_still_carries_the_agnostic_wins() {
        let vulkan = pack_for(Renderer::Vulkan);
        assert!(vulkan.len() >= 8, "Vulkan pack should still be substantial");
        assert!(pack_label(Renderer::Vulkan).contains("VulkanMod"));
        assert!(pack_label(Renderer::Opengl).contains("Sodium"));
        assert!(!uses_sodium(Renderer::Auto));
        assert!(uses_sodium(Renderer::Opengl));
    }

    #[test]
    fn a_build_for_the_wrong_game_version_is_rejected() {
        let versions = [
            ModrinthVersion {
                version_number: "0.1.7+mc1.21.5".into(),
                game_versions: vec!["1.21.5".into()],
                files: Vec::new(),
            },
            ModrinthVersion {
                version_number: "0.1.6+mc1.21.4".into(),
                game_versions: vec!["1.21.4".into()],
                files: Vec::new(),
            },
        ];

        let chosen = versions
            .iter()
            .find(|v| v.game_versions.is_empty() || v.game_versions.iter().any(|g| g == "1.21.4"))
            .expect("a 1.21.4 build should be picked");
        assert_eq!(chosen.version_number, "0.1.6+mc1.21.4");

        let wrong = versions
            .iter()
            .find(|v| v.game_versions.is_empty() || v.game_versions.iter().any(|g| g == "1.20.1"));
        assert!(wrong.is_none());
    }

    #[test]
    fn fabric_api_is_part_of_every_pack() {
        for renderer in [Renderer::Vulkan, Renderer::Opengl, Renderer::Auto] {
            assert!(
                pack_for(renderer).iter().any(|m| m.slug == "fabric-api"),
                "{renderer:?} pack is missing Fabric API"
            );
        }
    }

    #[test]
    fn modrinth_filenames_are_decoded_and_sanitised() {
        assert_eq!(
            decode_filename("sodium-fabric-0.6.13%2Bmc1.21.4.jar"),
            "sodium-fabric-0.6.13+mc1.21.4.jar"
        );
        assert_eq!(decode_filename("lithium-0.15.3.jar"), "lithium-0.15.3.jar");
        assert_eq!(decode_filename("../../evil.jar"), ".._.._evil.jar");
        assert_eq!(decode_filename(""), "mod.jar");
    }

    #[test]
    fn manifest_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = ManagedManifest {
            mc_version: "1.21.4".into(),
            loader: "fabric".into(),
            renderer: "opengl".into(),
            installed: vec![InstalledMod {
                slug: "sodium".into(),
                name: "Sodium".into(),
                version: "0.6.9".into(),
                filename: "sodium-fabric-0.6.9.jar".into(),
            }],
            skipped: vec!["LazyDFU".into()],
        };
        manifest.save(dir.path()).unwrap();

        let loaded = ManagedManifest::load(dir.path()).unwrap();
        assert_eq!(loaded, manifest);
        assert!(ManagedManifest::path(dir.path()).ends_with(MANAGED_MANIFEST));
    }

    #[test]
    fn legacy_manifest_without_renderer_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            ManagedManifest::path(dir.path()),
            br#"{"mc_version":"1.21.4","loader":"fabric","installed":[],"skipped":[]}"#,
        )
        .unwrap();
        let loaded = ManagedManifest::load(dir.path()).unwrap();
        assert_eq!(loaded.renderer, "");
    }

    #[test]
    fn prune_only_removes_managed_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("aethel-installed.jar"), b"managed").unwrap();
        std::fs::write(dir.path().join("player-own-mod.jar"), b"mine").unwrap();

        let manifest = ManagedManifest {
            mc_version: "1.21.4".into(),
            loader: "fabric".into(),
            renderer: "opengl".into(),
            installed: vec![InstalledMod {
                slug: "sodium".into(),
                name: "Sodium".into(),
                version: "1".into(),
                filename: "aethel-installed.jar".into(),
            }],
            skipped: Vec::new(),
        };
        manifest.save(dir.path()).unwrap();

        let removed = prune_managed(dir.path(), &[]).unwrap();
        assert_eq!(removed, 1);
        assert!(!dir.path().join("aethel-installed.jar").exists());
        assert!(
            dir.path().join("player-own-mod.jar").exists(),
            "player's own mods must never be deleted"
        );
    }

    #[test]
    fn pack_is_installed_requires_matching_version_renderer_and_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!pack_is_installed(
            dir.path(),
            "1.21.4",
            "fabric",
            Renderer::Opengl
        ));

        std::fs::write(dir.path().join("sodium.jar"), b"x").unwrap();
        let manifest = ManagedManifest {
            mc_version: "1.21.4".into(),
            loader: "fabric".into(),
            renderer: "opengl".into(),
            installed: vec![InstalledMod {
                slug: "sodium".into(),
                name: "Sodium".into(),
                version: "1".into(),
                filename: "sodium.jar".into(),
            }],
            skipped: Vec::new(),
        };
        manifest.save(dir.path()).unwrap();

        assert!(pack_is_installed(
            dir.path(),
            "1.21.4",
            "fabric",
            Renderer::Opengl
        ));
        assert!(!pack_is_installed(
            dir.path(),
            "1.20.1",
            "fabric",
            Renderer::Opengl
        ));
        assert!(!pack_is_installed(
            dir.path(),
            "1.21.4",
            "quilt",
            Renderer::Opengl
        ));
        assert!(!pack_is_installed(
            dir.path(),
            "1.21.4",
            "fabric",
            Renderer::Vulkan
        ));

        std::fs::remove_file(dir.path().join("sodium.jar")).unwrap();
        assert!(!pack_is_installed(
            dir.path(),
            "1.21.4",
            "fabric",
            Renderer::Opengl
        ));
    }
}
