use super::manifest::{BundleManifest, ManagedState};
use anyhow::Context as _;
use sha2::{Digest as _, Sha256};
use std::path::Path;

pub async fn verify_and_restore(
    instance_dir: &Path,
    manifest: &BundleManifest,
    cache_dir: &Path,
) -> anyhow::Result<()> {
    let managed_path = instance_dir.join("managed.json");
    let mut managed = if managed_path.exists() {
        let content = tokio::fs::read_to_string(&managed_path)
            .await
            .with_context(|| "failed to read managed.json")?;
        serde_json::from_str(&content).with_context(|| "failed to parse managed.json")?
    } else {
        ManagedState::default()
    };

    let minecraft_dir = instance_dir.join("minecraft");

    for file in &manifest.files {
        let local = minecraft_dir.join(&file.rel);
        match tokio::fs::read(&local).await {
            Ok(contents) => {
                let current_hash = format!("{:x}", Sha256::digest(&contents));
                if current_hash != file.sha256 {
                    tracing::info!("Restoring {} (hash mismatch)", file.rel);
                    restore_file(instance_dir, cache_dir, file, &mut managed).await?;
                } else {
                    managed.files.insert(file.rel.clone(), file.sha256.clone());
                }
            }
            Err(_) => {
                tracing::info!("Restoring {} (missing)", file.rel);
                restore_file(instance_dir, cache_dir, file, &mut managed).await?;
            }
        }
    }

    patch_options_txt(instance_dir, &manifest.options_gen).await?;

    let managed_json = serde_json::to_string_pretty(&managed)?;
    tokio::fs::write(&managed_path, managed_json)
        .await
        .context("failed to write managed.json")?;

    #[cfg(unix)]
    set_readonly_permissions(&minecraft_dir).await?;

    Ok(())
}

async fn restore_file(
    instance_dir: &Path,
    cache_dir: &Path,
    file: &super::manifest::BundleFile,
    managed: &mut ManagedState,
) -> anyhow::Result<()> {
    let minecraft_dir = instance_dir.join("minecraft");
    let target = minecraft_dir.join(&file.rel);

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let cache_file = cache_dir.join(&file.sha256[..8]).join(&file.rel);
    if cache_file.exists() {
        tokio::fs::create_dir_all(cache_file.parent().unwrap()).await?;
        tokio::fs::copy(&cache_file, &target).await?;
        managed.files.insert(file.rel.clone(), file.sha256.clone());
        return Ok(());
    }

    tracing::warn!("Download needed for {}", file.rel);
    Ok(())
}

async fn patch_options_txt(
    instance_dir: &Path,
    options_gen: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let options_path = instance_dir.join("minecraft").join("options.txt");
    let content = tokio::fs::read_to_string(&options_path)
        .await
        .unwrap_or_default();

    let lines: Vec<String> = content.lines().map(String::from).collect();
    let filtered: Vec<String> = lines
        .into_iter()
        .filter(|line| {
            !options_gen
                .keys()
                .any(|k| line.starts_with(&format!("{}=", k)))
        })
        .collect();

    let mut new_content = filtered.join("\n");
    for (key, value) in options_gen {
        new_content.push_str(&format!("\n{}={}", key, value));
    }

    tokio::fs::write(&options_path, new_content).await?;
    Ok(())
}

#[cfg(unix)]
async fn set_readonly_permissions(dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o444))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_patch_options_txt() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let mc_dir = tmp.path().join("minecraft");
        tokio::fs::create_dir_all(&mc_dir)
            .await
            .expect("create mc dir");
        tokio::fs::write(
            mc_dir.join("options.txt"),
            "gfxApi=OPENGL\nrenderDistance=8\n",
        )
        .await
        .expect("write options");

        let mut opts = std::collections::BTreeMap::new();
        opts.insert("gfxApi".to_string(), "VULKAN".to_string());
        opts.insert("entityDistanceScaling".to_string(), "0.75".to_string());

        patch_options_txt(tmp.path(), &opts)
            .await
            .expect("patch options");

        let content = tokio::fs::read_to_string(mc_dir.join("options.txt"))
            .await
            .expect("read options");
        assert!(content.contains("gfxApi=VULKAN"));
        assert!(content.contains("entityDistanceScaling=0.75"));
        assert!(!content.contains("gfxApi=OPENGL"));
    }
}
