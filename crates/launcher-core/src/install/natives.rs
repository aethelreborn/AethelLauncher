use anyhow::Context;
use std::path::{Path, PathBuf};

pub fn extract_natives(
    jar_path: &Path,
    dest_dir: &Path,
    exclude: &[String],
) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("failed to create natives dir: {}", dest_dir.display()))?;

    let file = std::fs::File::open(jar_path)
        .with_context(|| format!("failed to open native jar: {}", jar_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read native jar: {}", jar_path.display()))?;

    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if entry.is_dir() {
            continue;
        }
        if exclude
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
        {
            continue;
        }
        let rel = Path::new(&name);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }

        let dest = dest_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
        extracted.push(dest);
    }

    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_entries_and_honours_excludes() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let jar_path = dir.path().join("natives.jar");
        {
            let file = std::fs::File::create(&jar_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("libfoo.so", opts).unwrap();
            zip.write_all(b"native").unwrap();
            zip.start_file("META-INF/MANIFEST.MF", opts).unwrap();
            zip.write_all(b"ignore me").unwrap();
            zip.finish().unwrap();
        }

        let out = dir.path().join("out");
        let exclude = vec!["META-INF/".to_string()];
        let files = extract_natives(&jar_path, &out, &exclude).unwrap();

        assert_eq!(files.len(), 1);
        assert!(out.join("libfoo.so").is_file());
        assert!(!out.join("META-INF/MANIFEST.MF").exists());
    }
}
