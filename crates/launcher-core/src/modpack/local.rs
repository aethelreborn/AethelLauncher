//! Local folder import for modpacks.
//!
//! Supports importing modpacks from local directories containing:
//! - `.mrpack` files (Modrinth package format)
//! - `.zip` files with manifest.json (FTB/CurseForge format)
//! - Directories with manifest.json

use super::types::{LoaderType, Modpack, ModpackFile};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Import a modpack from a local directory.
///
/// Scans the directory for supported modpack formats and returns a [`Modpack`].
pub fn import_local(dir: &Path) -> Result<Modpack> {
    if !dir.is_dir() {
        anyhow::bail!("Path is not a directory: {:?}", dir);
    }

    // Try MRPACK first
    if let Some(mrpack) = find_file_ext(dir, "mrpack") {
        return import_mrpack(&mrpack);
    }

    // Try manifest.json (FTB/CurseForge format)
    let manifest_path = dir.join("manifest.json");
    if manifest_path.exists() {
        return import_ftb_manifest(&manifest_path, dir);
    }

    // Fallback to largest zip
    if let Some(zip) = find_largest_zip(dir) {
        return import_zip(&zip);
    }

    anyhow::bail!("No recognizable modpack format found in {:?}", dir)
}

/// Import a .mrpack file (Modrinth package format).
fn import_mrpack(path: &Path) -> Result<Modpack> {
    let file =
        std::fs::File::open(path).with_context(|| format!("Failed to open mrpack: {:?}", path))?;
    let mut archive =
        zip::ZipArchive::new(file).with_context(|| format!("Not a valid zip: {:?}", path))?;

    let manifest_text = archive
        .by_name("manifest.json")
        .map(|mut f| {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut f, &mut s).unwrap_or_default();
            s
        })
        .or(Err(anyhow::anyhow!("manifest.json not found in mrpack")))?;

    #[derive(serde::Deserialize)]
    struct MrpackManifest {
        name: String,
        #[serde(rename = "mcversion")]
        mc_version: String,
        author: Option<String>,
        dependencies: Option<std::collections::HashMap<String, String>>,
    }

    let meta: MrpackManifest =
        serde_json::from_str(&manifest_text).context("Failed to parse manifest.json")?;

    let files = extract_local_mods(path)?;
    let loader = infer_loader_from_path(path);

    Ok(Modpack {
        id: format!("local-{}", path.file_stem().unwrap().to_string_lossy()),
        name: meta.name,
        mc_versions: vec![meta.mc_version],
        loader,
        loader_version: None,
        description: meta.author.map(|a| format!("by {a}")),
        project_url: None,
        icon_url: None,
        download_count: None,
        files,
        extra: meta.dependencies.unwrap_or_default(),
    })
}

/// Import an FTB/CurseForge manifest.json format.
fn import_ftb_manifest(manifest_path: &Path, base_dir: &Path) -> Result<Modpack> {
    let text =
        std::fs::read_to_string(manifest_path).with_context(|| "Failed to read manifest.json")?;
    #[derive(serde::Deserialize)]
    struct FtBManifest {
        #[serde(rename = "minecraft")]
        minecraft: FtBMinecraft,
        #[serde(rename = "manifestType")]
        _manifest_type: String,
        #[serde(rename = "name")]
        _name: String,
        #[serde(rename = "version")]
        _version: String,
        mods: Vec<FtBMod>,
    }

    #[derive(serde::Deserialize)]
    struct FtBMinecraft {
        version: String,
        #[serde(rename = "modLoaders")]
        mod_loaders: Vec<FtBLoader>,
    }

    #[derive(serde::Deserialize)]
    struct FtBLoader {
        id: String,
        #[serde(rename = "primary")]
        _primary: bool,
    }

    #[derive(serde::Deserialize)]
    struct FtBMod {
        #[serde(rename = "projectID")]
        project_id: Option<u64>,
        #[serde(rename = "filePath")]
        file_path: Option<String>,
    }

    let manifest: FtBManifest =
        serde_json::from_str(&text).context("Failed to parse manifest.json")?;

    let loader_ids: Vec<String> = manifest
        .minecraft
        .mod_loaders
        .iter()
        .map(|l| l.id.clone())
        .collect();
    let loader = infer_loader_from_ftb(&loader_ids);

    let files = manifest
        .mods
        .into_iter()
        .filter_map(|m| {
            if let Some(ref fp) = m.file_path {
                let p = base_dir.join(fp);
                if p.exists() {
                    if let Ok(meta) = p.metadata() {
                        if let Some(name) = p.file_name() {
                            return Some(ModpackFile {
                                name: name.to_string_lossy().to_string(),
                                url: format!("local://{}", p.display()),
                                size_bytes: meta.len(),
                                sha1: None,
                                install_path: format!("mods/{}", name.to_string_lossy()),
                            });
                        }
                    }
                }
            } else if let Some(pid) = m.project_id {
                warn!("Missing file path for mod ID: {pid}, using placeholder");
                return Some(ModpackFile {
                    name: format!("cf-{pid}.jar"),
                    url: format!("placeholder://curseforge/{pid}"),
                    size_bytes: 0,
                    sha1: None,
                    install_path: format!("mods/cf-{pid}.jar"),
                });
            }
            None
        })
        .collect();

    Ok(Modpack {
        id: format!(
            "local-manifest-{}",
            manifest_path.file_stem().unwrap().to_string_lossy()
        ),
        name: format!("{} v{}", manifest._name, manifest._version),
        mc_versions: vec![manifest.minecraft.version],
        loader,
        loader_version: None,
        description: None,
        project_url: None,
        icon_url: None,
        download_count: None,
        files,
        extra: std::collections::HashMap::new(),
    })
}

/// Import a raw zip file as a modpack.
fn import_zip(path: &Path) -> Result<Modpack> {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let meta = std::fs::metadata(path).context("Failed to read file metadata")?;

    Ok(Modpack {
        id: format!("local-zip-{}", uuid::Uuid::new_v4()),
        name,
        mc_versions: vec![],
        loader: infer_loader_from_path(path),
        loader_version: None,
        description: None,
        project_url: None,
        icon_url: None,
        download_count: None,
        files: vec![ModpackFile {
            name: path.file_name().unwrap().to_string_lossy().to_string(),
            url: format!("local://{}", path.display()),
            size_bytes: meta.len(),
            sha1: None,
            install_path: "modpack.zip".to_string(),
        }],
        extra: std::collections::HashMap::new(),
    })
}

fn extract_local_mods(path: &Path) -> Result<Vec<ModpackFile>> {
    let file = std::fs::File::open(path).with_context(|| format!("open: {:?}", path))?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut out = Vec::new();

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name.starts_with("mods/") && entry.size() > 0 {
            if let Some(basename) = Path::new(&name).file_name() {
                let basename_str = basename.to_string_lossy().to_string();
                out.push(ModpackFile {
                    name: basename_str.clone(),
                    url: format!("local://{}", path.display()),
                    size_bytes: entry.size(),
                    sha1: None,
                    install_path: format!("mods/{basename_str}"),
                });
            }
        }
    }
    Ok(out)
}

fn find_file_ext(dir: &Path, ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ex| ex == ext).unwrap_or(false))
        .map(|e| e.path())
        .next()
}

fn find_largest_zip(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().map(|e| e == "zip").unwrap_or(false) {
                if let Ok(meta) = p.metadata() {
                    let cur = best.as_ref().map(|(s, _)| *s).unwrap_or(0);
                    if meta.len() > cur {
                        best = Some((meta.len(), p));
                    }
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn infer_loader_from_path(path: &Path) -> LoaderType {
    let lower = path.to_string_lossy().to_lowercase();
    if lower.contains("fabric") {
        LoaderType::Fabric
    } else if lower.contains("forge") {
        LoaderType::Forge
    } else if lower.contains("quilt") {
        LoaderType::Quilt
    } else {
        LoaderType::Vanilla
    }
}

fn infer_loader_from_ftb(loader_ids: &[String]) -> LoaderType {
    for id in loader_ids {
        if id.contains("fabric") {
            return LoaderType::Fabric;
        }
        if id.contains("forge") {
            return LoaderType::Forge;
        }
        if id.contains("quilt") {
            return LoaderType::Quilt;
        }
    }
    LoaderType::Vanilla
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_import_missing_dir() {
        let r = import_local(Path::new("/does/not/exist"));
        assert!(r.is_err());
    }

    #[test]
    fn test_infer_loader_from_filename() {
        assert_eq!(
            infer_loader_from_path(Path::new("/tmp/pack-fabric.zip")),
            LoaderType::Fabric
        );
        assert_eq!(
            infer_loader_from_path(Path::new("/tmp/pack-forge.zip")),
            LoaderType::Forge
        );
        assert_eq!(
            infer_loader_from_path(Path::new("/tmp/plain.zip")),
            LoaderType::Vanilla
        );
    }

    #[test]
    fn test_find_file_ext_misses() {
        let tmp = TempDir::new().unwrap();
        assert!(find_file_ext(tmp.path(), "mrpack").is_none());
    }

    #[test]
    fn test_find_file_ext_finds_mrpack() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("pack.mrpack"), b"fake").unwrap();
        let found = find_file_ext(tmp.path(), "mrpack").expect("should find mrpack");
        assert_eq!(found.file_name().unwrap(), "pack.mrpack");
    }

    #[test]
    fn test_extract_local_files() {
        let tmp = TempDir::new().unwrap();
        // Create a fake mrpack with a mod inside
        let mrpack_path = tmp.path().join("test.mrpack");
        {
            let mut archive = zip::ZipWriter::new(std::fs::File::create(&mrpack_path).unwrap());
            archive
                .start_file("mods/hello.jar", zip::write::SimpleFileOptions::default())
                .unwrap();
            use std::io::Write;
            archive.write_all(b"fake-jar-content").unwrap();
            archive.finish().unwrap();
        }

        let files = extract_local_mods(&mrpack_path).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "hello.jar");
    }
}
