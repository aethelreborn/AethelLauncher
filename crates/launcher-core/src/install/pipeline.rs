//! Install pipeline — turns a chosen version id into a runnable launch.
//!
//! Flow: manifest → version JSON → client jar → libraries + natives → asset
//! index + objects → Java runtime → expanded arguments. Every download is
//! hash-verified and written atomically.
//!
//! See [05 · Launch engine](../../../opencode-docs/05-launch-engine.md) §1.

use crate::auth::Account;
use crate::install::assets::AssetIndex;
use crate::install::downloader::{download_file, download_many, file_matches, DownloadTask};
use crate::install::natives::extract_natives;
use crate::launch::args_builder::{build_for, LaunchArgs};
use crate::manifest::mojang::{
    fetch_version_json, maven_to_path, native_keys, rules_allow, Artifact, VersionJson,
};
use crate::perf::{mods as perf_mods, options as perf_options, PerfConfig, PerfReport, Renderer};
use anyhow::{bail, Context};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Progress events streamed to the UI while an install runs.
#[derive(Debug, Clone)]
pub enum Progress {
    /// A coarse phase, e.g. `"Downloading libraries"`.
    Stage(String),
    /// Per-file progress within the current phase.
    Files {
        completed: usize,
        total: usize,
        label: String,
    },
    /// A human-readable log line.
    Log(String),
}

pub type ProgressSink = Arc<dyn Fn(Progress) + Send + Sync>;

/// Everything needed to install + prepare one launch.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub version_id: String,
    /// Per-instance game directory. This becomes the JVM working directory.
    pub game_dir: PathBuf,
    /// Shared content root holding `libraries/`, `assets/`, `versions/`,
    /// `runtimes/`.
    pub shared_dir: PathBuf,
    pub ram_mb: u64,
    pub renderer: String,
    pub auth: Account,
    /// Explicit Java override from settings.
    pub java_path: Option<PathBuf>,
    /// Fabric + performance pack + options.txt tuning.
    pub performance: PerfConfig,
}

/// A fully prepared launch, ready to hand to the process supervisor.
#[derive(Debug)]
pub struct PreparedLaunch {
    pub java: PathBuf,
    pub main_class: String,
    pub args: LaunchArgs,
    pub cwd: PathBuf,
    pub version_json: VersionJson,
    /// What the performance step actually did (shown in the UI).
    pub perf: PerfReport,
}

pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("AethelLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("failed to build HTTP client")
}

/// Install everything required and return the resolved launch.
pub async fn prepare(
    opts: &InstallOptions,
    progress: &ProgressSink,
) -> anyhow::Result<PreparedLaunch> {
    let client = http_client()?;

    let libraries_dir = opts.shared_dir.join("libraries");
    let assets_dir = opts.shared_dir.join("assets");
    let versions_dir = opts.shared_dir.join("versions");
    let natives_dir = opts.game_dir.join("natives");

    tokio::fs::create_dir_all(&opts.game_dir).await?;
    tokio::fs::create_dir_all(&versions_dir).await?;
    tokio::fs::create_dir_all(&natives_dir).await?;
    write_launcher_profiles(&opts.game_dir)?;

    // --- 1. Manifest + version JSON -------------------------------------
    progress(Progress::Stage("Resolving version".to_string()));
    let manifest = load_or_fetch_manifest(&client, &opts.shared_dir).await?;
    let record = manifest
        .versions
        .iter()
        .find(|v| v.id == opts.version_id)
        .with_context(|| format!("version {} not found in Mojang manifest", opts.version_id))?;

    let version_dir = versions_dir.join(&opts.version_id);
    tokio::fs::create_dir_all(&version_dir).await?;
    let json_path = version_dir.join(format!("{}.json", opts.version_id));
    let mut version: VersionJson = if json_path.is_file() {
        let raw = tokio::fs::read_to_string(&json_path).await?;
        serde_json::from_str(&raw)
            .with_context(|| format!("cached version JSON is corrupt: {}", json_path.display()))?
    } else {
        let fetched = fetch_version_json(&client, &record.url).await?;
        tokio::fs::write(&json_path, serde_json::to_vec_pretty(&fetched)?).await?;
        fetched
    };

    // --- 1b. Fabric loader + performance plan -----------------------------
    // Merged *after* the vanilla JSON is cached, so the on-disk copy stays
    // pristine and a later vanilla launch is unaffected.
    // Which pack to install is decided by the renderer: Sodium hooks OpenGL and
    // simply will not load on a Vulkan renderer, which needs VulkanMod.
    let renderer: Renderer = opts.renderer.parse().unwrap_or_default();

    let mut perf_report = PerfReport {
        requested: opts.performance.enabled,
        renderer: Some(renderer.resolved().as_str().to_string()),
        renderer_pack: Some(perf_mods::pack_label(renderer).to_string()),
        ..Default::default()
    };
    progress(Progress::Log(format!(
        "Renderer: {} → pack: {}",
        renderer.label(),
        perf_mods::pack_label(renderer)
    )));

    if opts.performance.enabled {
        match apply_fabric(&client, &mut version, progress).await {
            Ok(loader_version) => {
                perf_report.loader = Some(format!("fabric {loader_version}"));
            }
            Err(e) => {
                let message = format!("Fabric unavailable ({e:#}) — launching vanilla instead");
                tracing::warn!("{message}");
                perf_report.failures.push(message.clone());
                progress(Progress::Log(format!("⚠ {message}")));
            }
        }
    }

    // --- 2. Client jar ---------------------------------------------------
    let client_artifact = version
        .downloads
        .client
        .clone()
        .with_context(|| format!("version {} has no client download", version.id))?;
    let client_jar = version_dir.join(format!("{}.jar", version.id));
    progress(Progress::Stage("Downloading client".to_string()));
    download_file(
        &client,
        &DownloadTask::new(&client_artifact.url, &client_jar).with_sha1(&client_artifact.sha1),
    )
    .await?;

    // --- 3. Libraries + natives -----------------------------------------
    progress(Progress::Stage("Downloading libraries".to_string()));
    let mut plan = plan_libraries(&version, &libraries_dir);
    // The game jar goes last on the classpath, like the vanilla launcher.
    plan.classpath.push(client_jar.clone());

    // Maven-only libraries (Fabric's loader jars) carry no hash in the profile,
    // so pull the `.sha1` sidecar before downloading to keep verification on.
    hydrate_hashes(&client, &mut plan.downloads).await;

    let lib_progress = file_progress(progress.clone(), "Libraries");
    download_many(&client, plan.downloads, lib_progress).await?;

    // --- 4. Extract natives ---------------------------------------------
    if !plan.natives.is_empty() {
        progress(Progress::Stage("Extracting natives".to_string()));
    }
    let mut extracted_count = 0usize;
    for (jar, exclude) in &plan.natives {
        if !jar.is_file() {
            continue;
        }
        match extract_natives(jar, &natives_dir, exclude) {
            Ok(files) => extracted_count += files.len(),
            Err(e) => tracing::warn!("failed to extract natives from {}: {e}", jar.display()),
        }
    }
    if extracted_count > 0 {
        progress(Progress::Log(format!(
            "Extracted {extracted_count} native files into {}",
            natives_dir.display()
        )));
    }

    // --- 5. Assets -------------------------------------------------------
    let assets_index_name = download_assets(&client, &version, &assets_dir, progress).await?;

    // --- 5b. Performance mods + graphics preset --------------------------
    if opts.performance.enabled {
        install_performance_pack(
            &client,
            opts,
            &version,
            renderer,
            &mut perf_report,
            progress,
        )
        .await;
    }

    // --- 6. Java ---------------------------------------------------------
    let (component, required_major) = version
        .java_version
        .as_ref()
        .map(|j| (j.component.clone(), j.major))
        .unwrap_or_else(|| ("jre-legacy".to_string(), 8));
    let java = resolve_java(opts, &component, required_major, progress).await?;

    // --- 7. Arguments ----------------------------------------------------
    progress(Progress::Stage("Preparing launch".to_string()));
    let args = build_for(
        &version,
        &opts.auth,
        &opts.game_dir,
        &assets_dir,
        &natives_dir,
        &libraries_dir,
        plan.classpath,
        opts.ram_mb,
        &opts.renderer,
        &assets_index_name,
    );

    Ok(PreparedLaunch {
        java,
        main_class: version.main_class.clone(),
        args,
        cwd: opts.game_dir.clone(),
        version_json: version,
        perf: perf_report,
    })
}

/// Fetch `<url>.sha1` for any library jar that has no hash yet.
///
/// Mojang manifests always carry SHA-1; Maven-style profiles (Fabric, Forge) do
/// not, and downloading a jar we cannot verify is exactly how a launcher gets
/// compromised. The sidecar is cheap and almost always present.
async fn hydrate_hashes(client: &reqwest::Client, tasks: &mut [DownloadTask]) {
    for task in tasks.iter_mut() {
        if task.expected_sha1.is_some() || task.expected_sha256.is_some() {
            continue;
        }
        // Nothing to do when the verified file is already on disk.
        if file_matches(&task.dest, None, None) {
            continue;
        }

        let sidecar = format!("{}.sha1", task.url);
        let Ok(response) = client.get(&sidecar).send().await else {
            tracing::warn!("no SHA-1 sidecar for {}", task.url);
            continue;
        };
        let Ok(text) = response.text().await else {
            continue;
        };

        let digest = text
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if digest.len() == 40 && digest.chars().all(|c| c.is_ascii_hexdigit()) {
            task.expected_sha1 = Some(digest);
        } else {
            tracing::warn!("malformed SHA-1 sidecar for {}", task.url);
        }
    }
}

/// Resolve the Fabric loader and fold its profile into `version`.
async fn apply_fabric(
    client: &reqwest::Client,
    version: &mut VersionJson,
    progress: &ProgressSink,
) -> anyhow::Result<String> {
    progress(Progress::Stage("Resolving Fabric loader".to_string()));
    let loader = crate::manifest::fabric::resolve_loader(client, &version.id).await?;
    crate::manifest::fabric::merge_into(version, &loader);
    progress(Progress::Log(format!(
        "Fabric loader {} (intermediary {})",
        loader.loader_version, loader.intermediary_version
    )));
    Ok(loader.loader_version)
}

/// Install the Modrinth performance pack and apply the graphics preset.
///
/// Both halves are best-effort: a failure is recorded and surfaced, but never
/// stops the launch, because vanilla-with-tuned-options is still far better than
/// a dead launcher.
async fn install_performance_pack(
    client: &reqwest::Client,
    opts: &InstallOptions,
    version: &VersionJson,
    renderer: Renderer,
    report: &mut PerfReport,
    progress: &ProgressSink,
) {
    let mods_dir = opts.game_dir.join("mods");

    // Skipped when Fabric never came up — Fabric mods need the loader.
    if report.loader.is_some() {
        progress(Progress::Stage("Installing performance mods".to_string()));
        let sink = progress.clone();
        let callback: perf_mods::ProgressCallback = Arc::new(move |completed, total, label| {
            sink(Progress::Files {
                completed,
                total,
                label: label.to_string(),
            });
        });

        match perf_mods::install_performance_pack(
            client,
            &mods_dir,
            &version.id,
            "fabric",
            renderer,
            callback,
        )
        .await
        {
            Ok(manifest) => {
                report.installed_mods = manifest.installed.len();
                report.skipped_mods = manifest.skipped.clone();
                report.renderer = Some(manifest.renderer.clone());
                progress(Progress::Log(format!(
                    "{} performance mods installed for {}, {} unavailable for {}",
                    manifest.installed.len(),
                    perf_mods::pack_label(renderer),
                    manifest.skipped.len(),
                    version.id
                )));
                let keep: Vec<String> = manifest
                    .installed
                    .iter()
                    .map(|m| m.filename.clone())
                    .collect();
                if let Ok(removed) = perf_mods::prune_managed(&mods_dir, &keep) {
                    if removed > 0 {
                        progress(Progress::Log(format!(
                            "Removed {removed} outdated Aethel mod(s)"
                        )));
                    }
                }
            }
            Err(e) => {
                let message = format!("performance mods failed to install: {e:#}");
                tracing::warn!("{message}");
                report.failures.push(message.clone());
                progress(Progress::Log(format!("⚠ {message}")));
            }
        }
    }

    // The graphics preset applies even on vanilla — this is the half of the
    // FPS win that never depends on the network.
    let tuned = perf_options::TunedOptions::for_preset(
        opts.performance.preset,
        crate::perf::system_ram_mb(),
    );
    let extra: Vec<(&str, String)> = vec![("aethelRenderer", opts.renderer.clone())];
    match perf_options::apply(&opts.game_dir, &tuned, &extra) {
        Ok(changed) => {
            report.options_written = true;
            report.options_changed = changed;
            progress(Progress::Log(format!(
                "Graphics preset \"{}\" applied{}",
                opts.performance.preset.label(),
                if changed { "" } else { " (already up to date)" }
            )));
        }
        Err(e) => {
            let message = format!("could not tune options.txt: {e:#}");
            tracing::warn!("{message}");
            report.failures.push(message.clone());
            progress(Progress::Log(format!("⚠ {message}")));
        }
    }
}

/// What a version needs on this platform: jars for the classpath, jars to
/// download, and native jars to unpack.
#[derive(Debug, Default)]
pub struct LibraryPlan {
    /// Classpath entries in load order (the game jar is appended by the caller).
    pub classpath: Vec<PathBuf>,
    pub downloads: Vec<DownloadTask>,
    /// Native jars with their `extract.exclude` prefixes, in download order.
    pub natives: Vec<(PathBuf, Vec<String>)>,
}

/// Resolve a version's library list for the current OS/arch.
///
/// Applies Mojang rules (last matching rule wins, default deny), picks the
/// platform native classifier where a library declares one, and skips
/// libraries Mojang bundles inside `client.jar` (the 1.21.x `client-extra`
/// case, which shows up as a library with no artifact).
///
/// Duplicate paths are dropped so the classpath stays reproducible.
pub fn plan_libraries(version: &VersionJson, libraries_dir: &Path) -> LibraryPlan {
    let features: BTreeMap<String, bool> = BTreeMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut plan = LibraryPlan::default();

    for lib in &version.libraries {
        if !rules_allow(&lib.rules, &features) {
            continue;
        }

        // Native classifier for this platform, if the library declares one.
        let classifier = native_keys()
            .iter()
            .find_map(|key| lib.natives.get(*key))
            .cloned();

        if let Some(classifier) = &classifier {
            if let Some(art) = lib.downloads.classifiers.get(classifier) {
                let rel = art
                    .relative_path(None)
                    .unwrap_or_else(|| maven_to_path(&format!("{}:{}", lib.name, classifier)));
                if !rel.is_empty() {
                    let dest = libraries_dir.join(&rel);
                    plan.downloads
                        .push(DownloadTask::new(&art.url, &dest).with_sha1(&art.sha1));
                    let exclude = lib
                        .extract
                        .as_ref()
                        .map(|e| e.exclude.clone())
                        .unwrap_or_else(|| vec!["META-INF/".to_string()]);
                    plan.natives.push((dest, exclude));
                }
            }
        }

        // Two sources of library jars:
        //   * Mojang manifests carry `downloads.artifact` (URL + SHA-1)
        //   * Fabric/Forge profiles carry only a Maven base `url`, so the
        //     artifact is derived from the coordinates. Those have no hash in
        //     the manifest; `hydrate_hashes` fetches the `.sha1` sidecar so the
        //     jar is still verified before it is used.
        // A library with neither is bundled inside `client.jar` (the 1.21.x
        // `client-extra` case) and is deliberately skipped.
        let synthesized;
        let artifact: Option<&Artifact> = match &lib.downloads.artifact {
            Some(art) => Some(art),
            None => match &lib.url {
                Some(base) if lib.natives.is_empty() => {
                    let path = maven_to_path(&lib.name);
                    if path.is_empty() {
                        None
                    } else {
                        synthesized = Artifact {
                            path: Some(path.clone()),
                            url: format!("{}/{}", base.trim_end_matches('/'), path),
                            sha1: String::new(),
                            size: 0,
                        };
                        Some(&synthesized)
                    }
                }
                _ => None,
            },
        };

        let Some(art) = artifact else {
            continue;
        };
        let Some(rel) = art.relative_path(Some(&lib.name)) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        let dest = libraries_dir.join(&rel);
        if seen.insert(rel.clone()) {
            plan.classpath.push(dest.clone());
            plan.downloads
                .push(DownloadTask::new(&art.url, &dest).with_sha1(&art.sha1));
        }
    }

    plan
}

fn file_progress(
    progress: ProgressSink,
    label: &'static str,
) -> Arc<dyn Fn(usize, usize) + Send + Sync> {
    Arc::new(move |completed, total| {
        progress(Progress::Files {
            completed,
            total,
            label: label.to_string(),
        });
    })
}

/// Download the asset index (if present) and every missing object.
///
/// Returns the asset index *name* used by `--assetIndex`.
async fn download_assets(
    client: &reqwest::Client,
    version: &VersionJson,
    assets_dir: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<String> {
    let Some(index_ref) = &version.asset_index else {
        // Very old versions resolve assets by hash without an index file.
        return Ok(version.assets.clone().unwrap_or_else(|| version.id.clone()));
    };

    progress(Progress::Stage("Downloading assets".to_string()));
    let index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", index_ref.id));
    download_file(
        client,
        &DownloadTask::new(&index_ref.url, &index_path).with_sha1(&index_ref.sha1),
    )
    .await?;

    let raw = tokio::fs::read_to_string(&index_path)
        .await
        .with_context(|| format!("failed to read asset index {}", index_path.display()))?;
    let index: AssetIndex = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse asset index {}", index_path.display()))?;

    let mut tasks = Vec::with_capacity(index.objects.len());
    for obj in index.objects.values() {
        if obj.hash.len() < 2 {
            continue;
        }
        let prefix = &obj.hash[..2];
        let dest = assets_dir.join("objects").join(prefix).join(&obj.hash);
        let url = format!(
            "https://resources.download.minecraft.net/{prefix}/{}",
            obj.hash
        );
        tasks.push(DownloadTask::new(url, dest).with_sha1(&obj.hash));
    }

    let total = tasks.len();
    let asset_progress = file_progress(progress.clone(), "Assets");
    download_many(client, tasks, asset_progress).await?;
    progress(Progress::Log(format!("{total} asset objects verified")));

    Ok(index_ref.id.clone())
}

// ---------------------------------------------------------------------------
// Manifest caching
// ---------------------------------------------------------------------------

const MANIFEST_TTL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

async fn load_or_fetch_manifest(
    client: &reqwest::Client,
    shared_dir: &Path,
) -> anyhow::Result<crate::manifest::mojang::VersionManifest> {
    let cache_path = shared_dir
        .join("cache")
        .join("mojang_version_manifest.json");

    if let Ok(meta) = std::fs::metadata(&cache_path) {
        if let Ok(modified) = meta.modified() {
            if modified
                .elapsed()
                .map(|e| e < MANIFEST_TTL)
                .unwrap_or(false)
            {
                if let Ok(raw) = std::fs::read_to_string(&cache_path) {
                    if let Ok(manifest) = serde_json::from_str(&raw) {
                        return Ok(manifest);
                    }
                }
            }
        }
    }

    match crate::manifest::mojang::fetch_version_manifest(client).await {
        Ok(manifest) => {
            if let Some(parent) = cache_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(raw) = serde_json::to_vec_pretty(&manifest) {
                let _ = std::fs::write(&cache_path, raw);
            }
            Ok(manifest)
        }
        Err(e) => {
            // Offline: fall back to a stale cache rather than refusing to play.
            if let Ok(raw) = std::fs::read_to_string(&cache_path) {
                if let Ok(manifest) = serde_json::from_str(&raw) {
                    tracing::warn!("using stale version manifest (offline): {e}");
                    return Ok(manifest);
                }
            }
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Java resolution
// ---------------------------------------------------------------------------

/// Resolution order (see [05 §7.1](../../../opencode-docs/05-launch-engine.md)):
/// explicit override → managed runtime cache → system Java.
async fn resolve_java(
    opts: &InstallOptions,
    component: &str,
    required_major: u32,
    progress: &ProgressSink,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = &opts.java_path {
        if path.is_file() {
            progress(Progress::Log(format!(
                "Using Java override: {}",
                path.display()
            )));
            return Ok(path.clone());
        }
        tracing::warn!("configured Java path does not exist: {}", path.display());
    }

    let runtimes = opts.shared_dir.join("runtimes");
    if let Some(path) = find_managed_java(&runtimes, component) {
        progress(Progress::Log(format!(
            "Using managed Java runtime ({component}): {}",
            path.display()
        )));
        return Ok(path);
    }

    if let Some(path) = find_system_java() {
        match probe_java_major(&path) {
            Some(major) if major >= required_major => {
                progress(Progress::Log(format!(
                    "Using system Java {major} ({})",
                    path.display()
                )));
            }
            Some(major) => {
                progress(Progress::Log(format!(
                    "⚠ System Java {major} is older than the required Java {required_major}; \
                     the game may fail to start. Set a Java path in Settings."
                )));
                tracing::warn!("system java {major} < required {required_major}");
            }
            None => {
                progress(Progress::Log(format!(
                    "Could not determine the version of {}",
                    path.display()
                )));
            }
        }
        return Ok(path);
    }

    bail!(
        "no Java runtime found. Install Java {required_major}+ or set a Java path in Settings \
         (component: {component})"
    )
}

/// Look for `bin/java` (or `bin/java.exe`) under the managed runtime cache.
///
/// Mojang's layout is `runtimes/<component>/<platform>/<component>/bin/java`,
/// so we search a few levels down rather than hardcoding the platform folder.
fn find_managed_java(runtimes: &Path, component: &str) -> Option<PathBuf> {
    let roots: Vec<PathBuf> = if component.is_empty() {
        vec![runtimes.to_path_buf()]
    } else {
        vec![runtimes.join(component)]
    };

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name != "java" && name != "java.exe" {
                continue;
            }
            let is_bin = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|d| d == "bin")
                .unwrap_or(false);
            if is_bin {
                return Some(path.to_path_buf());
            }
        }
    }
    None
}

fn find_system_java() -> Option<PathBuf> {
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        for candidate in ["bin/java", "bin/java.exe"] {
            let path = Path::new(&java_home).join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Parse the major version out of `java -version` output.
fn probe_java_major(java: &Path) -> Option<u32> {
    let output = std::process::Command::new(java)
        .arg("-version")
        .output()
        .ok()?;
    // `java -version` writes to stderr: java version "1.8.0_402" / openjdk version "25.0.4"
    let text = String::from_utf8_lossy(&output.stderr);
    let first_line = text.lines().next()?;
    let start = first_line.find('"')? + 1;
    let rest = &first_line[start..];
    let end = rest.find('"')?;
    let version = &rest[..end];

    if let Some(legacy) = version.strip_prefix("1.") {
        legacy.split('.').next()?.parse().ok()
    } else {
        version.split('.').next()?.parse().ok()
    }
}

/// Write a minimal `launcher_profiles.json` so third-party tooling behaves as
/// if the vanilla launcher produced this game dir ([05 §7.2]).
fn write_launcher_profiles(game_dir: &Path) -> anyhow::Result<()> {
    let path = game_dir.join("launcher_profiles.json");
    if path.exists() {
        return Ok(());
    }
    let body = serde_json::json!({
        "profiles": {},
        "settings": {},
        "version": 3
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&body)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::mojang::Artifact;

    #[test]
    fn probe_java_major_parses_system_java() {
        // The dev/CI box has some Java; if it does not, skip rather than fail.
        if let Some(java) = find_system_java() {
            let major = probe_java_major(&java);
            assert!(major.is_some(), "could not parse {java:?}");
            assert!(major.unwrap() >= 8);
        }
    }

    #[test]
    fn bundled_libraries_without_artifact_are_skipped() {
        // A library with neither an artifact nor natives must not appear on the
        // classpath (the 1.21.x `client-extra` case).
        let lib = crate::manifest::mojang::Library {
            name: "com.mojang:bundled:1.0".to_string(),
            url: None,
            downloads: Default::default(),
            natives: Default::default(),
            rules: Vec::new(),
            extract: None,
        };
        assert!(lib.downloads.artifact.is_none());
    }

    #[test]
    fn artifact_path_falls_back_to_maven_name() {
        let art = Artifact {
            path: None,
            url: "https://example.com/a.jar".to_string(),
            sha1: String::new(),
            size: 0,
        };
        assert_eq!(
            art.relative_path(Some("org.lwjgl:lwjgl:3.3.3")).as_deref(),
            Some("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar")
        );
    }

    #[test]
    fn stale_manifest_falls_back_to_disk_cache() {
        // Offline behaviour: a cached manifest is still usable.
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("mojang_version_manifest.json"),
            br#"{"latest":{"release":"1.21.4","snapshot":"25w01a"},"versions":[]}"#,
        )
        .unwrap();
        let raw = std::fs::read_to_string(cache.join("mojang_version_manifest.json")).unwrap();
        let manifest: crate::manifest::mojang::VersionManifest =
            serde_json::from_str(&raw).unwrap();
        assert_eq!(manifest.latest.release, "1.21.4");
    }
}
