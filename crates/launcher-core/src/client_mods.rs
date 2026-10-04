//! The Aethel in-game client — the mods that ship inside the launcher binary.
//!
//! Performance mods come from Modrinth ([`crate::perf::mods`]), but these are ours: they are
//! built from `gamesupport/` and embedded here, so every install gets the exact client build the
//! launcher was compiled with (cape/wing rendering and the click GUI) with no network round-trip.
//!
//! A development override lets a local Gradle build shadow the embedded jars:
//! `AETHEL_CLIENT_MODS=<dir>` with files named `aethel-cosmetics.jar` / `aethel-hud.jar`.

use crate::perf::mods::{InstalledMod, ManagedManifest};
use anyhow::Context;
use std::path::{Path, PathBuf};

/// `(mod id, human name, file name, jar bytes)`
const EMBEDDED: &[(&str, &str, &str, &[u8])] = &[
    (
        "aethel-cosmetics",
        "Aethel Cosmetics",
        "aethel-cosmetics.jar",
        include_bytes!("../assets/client-mods/aethel-cosmetics.jar"),
    ),
    (
        "aethel-hud",
        "Aethel HUD",
        "aethel-hud.jar",
        include_bytes!("../assets/client-mods/aethel-hud.jar"),
    ),
];

/// The mods that make up the in-game client, in load order.
pub fn client_mods() -> Vec<InstalledMod> {
    EMBEDDED
        .iter()
        .map(|(slug, name, filename, _)| InstalledMod {
            slug: (*slug).to_string(),
            name: (*name).to_string(),
            version: embedded_version(filename).unwrap_or_else(|| "bundled".to_string()),
            filename: (*filename).to_string(),
        })
        .collect()
}

fn embedded_version(filename: &str) -> Option<String> {
    let stem = filename.strip_suffix(".jar")?;
    let version = stem.rsplit_once('-')?.1;
    Some(version.to_string())
}

/// `true` when both client mods are present in `mods_dir` at the size we shipped.
pub async fn client_is_installed(mods_dir: &Path) -> bool {
    for (_, _, filename, bytes) in EMBEDDED {
        let len = tokio::fs::metadata(mods_dir.join(filename))
            .await
            .map(|meta| meta.len())
            .ok();
        if len != Some(bytes.len() as u64) {
            return false;
        }
    }
    true
}

/// Writes any missing or stale client mod into `mods_dir` and records them in the managed
/// manifest, so prune/verify treat them like any other pinned mod.
pub async fn install_client_mods(mods_dir: &Path) -> anyhow::Result<Vec<InstalledMod>> {
    tokio::fs::create_dir_all(mods_dir)
        .await
        .with_context(|| format!("failed to create {}", mods_dir.display()))?;

    let override_dir = std::env::var_os("AETHEL_CLIENT_MODS").map(PathBuf::from);
    let mut installed = Vec::new();

    for (slug, name, filename, bytes) in EMBEDDED {
        let dest = mods_dir.join(filename);

        let source = override_dir.as_ref().and_then(|dir| {
            let candidate = dir.join(filename);
            candidate.is_file().then_some(candidate)
        });

        let current_len = tokio::fs::metadata(&dest).await.map(|m| m.len()).ok();
        let needs_write = match (&source, current_len) {
            (Some(_), current) => current != tokio::fs::metadata(source.as_ref().unwrap()).await.ok().map(|m| m.len()),
            (None, current) => current != Some(bytes.len() as u64),
        };

        if needs_write {
            match source {
                Some(path) => {
                    tokio::fs::copy(&path, &dest).await.with_context(|| {
                        format!("failed to install client mod {} from {}", slug, path.display())
                    })?;
                    tracing::info!("installed client mod {slug} from {}", path.display());
                }
                None => {
                    tokio::fs::write(&dest, bytes).await.with_context(|| {
                        format!("failed to write bundled client mod {slug}")
                    })?;
                    tracing::info!("installed bundled client mod {slug}");
                }
            }
        }

        installed.push(InstalledMod {
            slug: (*slug).to_string(),
            name: (*name).to_string(),
            version: embedded_version(filename).unwrap_or_else(|| "bundled".to_string()),
            filename: (*filename).to_string(),
        });
    }

    if let Some(mut manifest) = ManagedManifest::load(mods_dir) {
        manifest
            .installed
            .retain(|m| !installed.iter().any(|c| c.filename == m.filename));
        manifest.installed.extend(installed.iter().cloned());
        manifest.save(mods_dir)?;
    }

    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_jars_are_real_jars() {
        for (slug, _, _, bytes) in EMBEDDED {
            assert!(bytes.len() > 1024, "{slug} looks empty");
            assert_eq!(&bytes[..2], b"PK", "{slug} is not a zip");
        }
    }

    #[test]
    fn client_mods_are_listed_in_load_order() {
        let mods = client_mods();
        assert_eq!(mods.len(), EMBEDDED.len());
        assert_eq!(mods[0].slug, "aethel-cosmetics");
        assert_eq!(mods[1].slug, "aethel-hud");
        assert!(mods.iter().all(|m| m.filename.ends_with(".jar")));
    }

    #[tokio::test]
    async fn install_writes_both_mods_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let installed = install_client_mods(tmp.path()).await.expect("install");
        assert_eq!(installed.len(), 2);
        for entry in &installed {
            let path = tmp.path().join(&entry.filename);
            assert!(path.is_file(), "{} missing", entry.filename);
        }
        assert!(client_is_installed(tmp.path()).await);

        // a second pass must not change anything
        let before = tokio::fs::metadata(tmp.path().join("aethel-hud.jar"))
            .await
            .unwrap()
            .len();
        install_client_mods(tmp.path()).await.expect("second install");
        let after = tokio::fs::metadata(tmp.path().join("aethel-hud.jar"))
            .await
            .unwrap()
            .len();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn a_truncated_mod_is_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        install_client_mods(tmp.path()).await.expect("install");
        tokio::fs::write(tmp.path().join("aethel-cosmetics.jar"), b"broken")
            .await
            .unwrap();
        assert!(!client_is_installed(tmp.path()).await);
        install_client_mods(tmp.path()).await.expect("repair");
        assert!(client_is_installed(tmp.path()).await);
    }
}