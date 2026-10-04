use super::local::import_local;
use super::types::{LoaderType, Modpack};
use anyhow::{Context, Result};
use reqwest::Client;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum InstallProgress {
    DownloadStart {
        index: usize,
        total: usize,
        filename: String,
    },
    DownloadProgress {
        index: usize,
        bytes: u64,
    },
    DownloadComplete {
        index: usize,
    },
    VerifyingChecksum {
        index: usize,
    },
    Extracting {
        index: usize,
    },
    Warning(String),
}

pub type ProgressCallback = Box<dyn Fn(&InstallProgress) + Send + Sync>;

pub struct InstallParams<'a> {
    pub client: &'a Client,
    pub base_dir: &'a Path,
    pub modpack: &'a Modpack,
    pub instance_name: &'a str,
    pub on_progress: Option<ProgressCallback>,
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub instance_path: PathBuf,
    pub files_installed: usize,
}

pub async fn install_modpack(params: InstallParams<'_>) -> Result<InstallResult> {
    let instance_dir = params.base_dir.join(params.instance_name);
    create_instance_dirs(&instance_dir).await?;

    let total = params.modpack.files.len();
    let mut installed = 0usize;

    for (idx, file) in params.modpack.files.iter().enumerate() {
        emit(
            &params.on_progress,
            InstallProgress::DownloadStart {
                index: idx,
                total,
                filename: file.name.clone(),
            },
        );

        let staging = instance_dir
            .join(".staging")
            .join(format!("file-{idx}"))
            .join(&file.name);
        download_and_verify(params.client, file, &staging, &params.on_progress, idx)
            .await
            .with_context(|| format!("failed to download {}", file.name))?;

        move_to_dest(&staging, &instance_dir, file)?;

        let dest = instance_dir.join(&file.install_path);
        if file.name.ends_with(".zip") || file.name.ends_with(".mrpack") {
            emit(
                &params.on_progress,
                InstallProgress::Extracting { index: idx },
            );
            let extract_root = dest.parent().unwrap_or(&instance_dir);
            extract_archive(&dest, extract_root)?;
            let _ = std::fs::remove_file(&dest);
        } else if is_bare_filename(&file.install_path) {
            let mods_dest = instance_dir.join("mods").join(&file.name);
            if dest != mods_dest {
                std::fs::rename(&dest, &mods_dest)
                    .with_context(|| format!("move to mods/: {}", file.name))?;
            }
        }

        emit(
            &params.on_progress,
            InstallProgress::DownloadComplete { index: idx },
        );
        installed += 1;
    }

    write_instance_toml(&instance_dir, params.modpack, params.instance_name)?;

    emit(
        &params.on_progress,
        InstallProgress::Warning(format!(
            "installed {installed} files to {}",
            instance_dir.display()
        )),
    );

    Ok(InstallResult {
        instance_path: instance_dir,
        files_installed: installed,
    })
}

async fn create_instance_dirs(dir: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("create dir: {}", dir.display()))?;
    tokio::fs::create_dir_all(dir.join("mods")).await?;
    tokio::fs::create_dir_all(dir.join("config")).await?;
    tokio::fs::create_dir_all(dir.join(".staging")).await?;
    Ok(())
}

async fn download_and_verify(
    client: &Client,
    file: &super::types::ModpackFile,
    dest: &Path,
    on_progress: &Option<ProgressCallback>,
    idx: usize,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut resp = client.get(&file.url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {} for {}", resp.status(), file.url);
    }

    let mut writer = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(dest)
        .await?;

    let mut hasher = Sha1::new();
    let mut total_bytes = 0u64;

    while let Some(chunk) = resp.chunk().await? {
        tokio::io::AsyncWriteExt::write_all(&mut writer, &chunk).await?;
        hasher.update(&chunk);
        total_bytes += chunk.len() as u64;

        if total_bytes % (4 * 1024 * 1024) < chunk.len() as u64 {
            emit(
                on_progress,
                InstallProgress::DownloadProgress {
                    index: idx,
                    bytes: total_bytes,
                },
            );
        }
    }
    drop(writer);

    if let Some(expected) = &file.sha1 {
        emit(
            on_progress,
            InstallProgress::VerifyingChecksum { index: idx },
        );
        let actual = format!("{:x}", hasher.finalize());
        if actual != *expected {
            let _ = tokio::fs::remove_file(dest).await;
            anyhow::bail!(
                "SHA-1 mismatch for {}: expected {}, got {}",
                dest.display(),
                expected,
                actual
            );
        }
    }

    Ok(())
}

fn move_to_dest(src: &Path, instance_dir: &Path, file: &super::types::ModpackFile) -> Result<()> {
    let dest_dir = instance_dir
        .join(&file.install_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| instance_dir.to_path_buf());
    std::fs::create_dir_all(&dest_dir)?;
    std::fs::rename(src, dest_dir.join(&file.name))?;
    Ok(())
}

fn is_bare_filename(install_path: &str) -> bool {
    Path::new(install_path)
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
}

fn extract_archive(archive_path: &Path, instance_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let count = archive.len();

    for i in 0..count {
        let mut entry = archive.by_index(i)?;
        let dest_path = instance_dir.join(entry.mangled_name());
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&dest_path)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

fn write_instance_toml(dir: &Path, modpack: &Modpack, name_hint: &str) -> Result<()> {
    let loader_str = match modpack.loader {
        LoaderType::Fabric => "fabric",
        LoaderType::Forge => "forge",
        LoaderType::Quilt => "quilt",
        LoaderType::Vanilla => "vanilla",
    };
    let loader_ver = modpack.loader_version.as_deref().unwrap_or("");
    let ts = chrono::Utc::now().to_rfc3339();

    let toml_str = format!(
        r#"id = "{id}"
name = "{name}"
mc_version = ""
loader = "{loader}"
loader_version = "{loader_ver}"
ram_mb = 4096
renderer = "auto"
auth_mode = "offline"
created_at = "{ts}"
"#,
        id = modpack.id,
        name = name_hint,
        loader = loader_str,
    );

    let toml_path = dir.join("instance.toml");
    std::fs::write(&toml_path, toml_str)?;
    Ok(())
}

fn emit(cb: &Option<ProgressCallback>, event: InstallProgress) {
    if let Some(callback) = cb {
        callback(&event);
    }
}

pub async fn install_local_modpack(
    client: &Client,
    base_dir: &Path,
    source_dir: &Path,
    instance_name: &str,
    on_progress: Option<ProgressCallback>,
) -> Result<InstallResult> {
    let modpack = import_local(source_dir)?;
    install_modpack(InstallParams {
        client,
        base_dir,
        modpack: &modpack,
        instance_name,
        on_progress,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::super::types::{LoaderType, ModpackFile};
    use super::*;

    fn sample_modpack() -> Modpack {
        Modpack {
            id: "test-1".to_string(),
            name: "Test Pack".to_string(),
            mc_versions: vec!["1.21.1".to_string()],
            loader: LoaderType::Fabric,
            loader_version: Some("0.16.0".to_string()),
            description: Some("A test pack.".to_string()),
            project_url: None,
            icon_url: None,
            download_count: None,
            files: vec![
                ModpackFile {
                    name: "fabric-api-0.92.0.jar".to_string(),
                    url: "https://example.com/fake.jar".to_string(),
                    size_bytes: 1000,
                    sha1: None,
                    install_path: "mods/fabric-api-0.92.0.jar".to_string(),
                },
                ModpackFile {
                    name: "some-mod-1.0.zip".to_string(),
                    url: "https://example.com/fake.zip".to_string(),
                    size_bytes: 2000,
                    sha1: None,
                    install_path: "mods/some-mod-1.0.zip".to_string(),
                },
            ],
            extra: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_install_creates_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let mp = sample_modpack();
        let _ = install_modpack(InstallParams {
            client: &Client::new(),
            base_dir: tmp.path(),
            modpack: &mp,
            instance_name: "test-inst",
            on_progress: None,
        })
        .await;
        let inst = tmp.path().join("test-inst");
        assert!(inst.exists());
        assert!(inst.join("mods").exists());
        assert!(inst.join("config").exists());
    }

    #[test]
    fn test_write_toml_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mp = sample_modpack();
        write_instance_toml(tmp.path(), &mp, "Test Pack").expect("write toml");
        let toml_str = std::fs::read_to_string(tmp.path().join("instance.toml")).unwrap();
        assert!(toml_str.contains("name = \"Test Pack\""));
        assert!(toml_str.contains("loader = \"fabric\""));
    }

    #[test]
    fn test_is_bare_filename() {
        assert!(is_bare_filename("foo.jar"));
        assert!(is_bare_filename(""));
        assert!(!is_bare_filename("mods/foo.jar"));
        assert!(!is_bare_filename("config/sub/bar.toml"));
    }

    #[test]
    fn test_move_to_dest_honours_install_path() {
        let tmp = tempfile::tempdir().unwrap();
        let instance = tmp.path().join("inst");
        std::fs::create_dir_all(instance.join("mods")).unwrap();
        let staging = tmp.path().join("staging.jar");
        std::fs::write(&staging, b"jar").unwrap();

        let file = ModpackFile {
            name: "staging.jar".into(),
            url: "https://example.com/x".into(),
            size_bytes: 3,
            sha1: None,
            install_path: "mods/staging.jar".into(),
        };
        move_to_dest(&staging, &instance, &file).expect("move");
        assert!(instance.join("mods/staging.jar").exists());
        assert!(!instance.join("staging.jar").exists());
    }

    #[test]
    fn test_extract_empty_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let archive =
            zip::ZipWriter::new(std::fs::File::create(tmp.path().join("empty.zip")).unwrap());
        archive.finish().unwrap();
        extract_archive(tmp.path().join("empty.zip").as_path(), tmp.path()).unwrap();
    }

    #[test]
    fn test_extract_zip_with_content() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let file = std::fs::File::create(tmp.path().join("pack.zip")).unwrap();
            let mut archive = zip::ZipWriter::new(file);
            archive
                .start_file("mods/hello.jar", zip::write::SimpleFileOptions::default())
                .unwrap();
            use std::io::Write;
            archive.write_all(b"fake-jar-content").unwrap();
            archive.finish().unwrap();
        }
        extract_archive(tmp.path().join("pack.zip").as_path(), tmp.path()).unwrap();
        assert!(tmp.path().join("mods/hello.jar").exists());
        let content = std::fs::read_to_string(tmp.path().join("mods/hello.jar")).unwrap();
        assert_eq!(content, "fake-jar-content");
    }
}
