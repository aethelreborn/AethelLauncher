use anyhow::Context;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::path::Path;

pub fn verify_sha1(path: &Path, expected: &str) -> anyhow::Result<()> {
    let content =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let actual = format!("{:x}", Sha1::digest(&content));
    if actual != expected {
        anyhow::bail!(
            "SHA-1 mismatch for {}: expected {}, got {}",
            path.display(),
            expected,
            actual
        );
    }
    Ok(())
}

pub fn verify_sha256(path: &Path, expected: &str) -> anyhow::Result<()> {
    let content =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let actual = format!("{:x}", Sha256::digest(&content));
    if actual != expected {
        anyhow::bail!(
            "SHA-256 mismatch for {}: expected {}, got {}",
            path.display(),
            expected,
            actual
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_verification() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let file = tmp.path().join("test.txt");
        std::fs::write(&file, b"hello world").expect("write file");

        let expected = format!("{:x}", Sha1::digest(b"hello world"));
        verify_sha1(&file, &expected).expect("verify sha1");
    }

    #[test]
    fn test_sha256_verification() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let file = tmp.path().join("test.txt");
        std::fs::write(&file, b"hello world").expect("write file");

        let expected = format!("{:x}", Sha256::digest(b"hello world"));
        verify_sha256(&file, &expected).expect("verify sha256");
    }
}
