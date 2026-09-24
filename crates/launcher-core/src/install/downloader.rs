//! Parallel download manager with hash verification and atomic writes.
//!
//! Every file is written to `<dest>.part`, verified against the announced
//! SHA-1/SHA-256, and only then renamed into place — so an interrupted launch
//! never leaves a half-written jar in the libraries cache.

use anyhow::{bail, Context};
use serde::Serialize;
use sha1::Digest;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

const MAX_CONCURRENT_DOWNLOADS: usize = 16;
const MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct DownloadTask {
    pub url: String,
    pub dest: PathBuf,
    pub expected_sha1: Option<String>,
    pub expected_sha256: Option<String>,
    /// Modrinth publishes SHA-512; verifying it is stronger than SHA-1 but is
    /// only checked when the publisher supplies one.
    pub expected_sha512: Option<String>,
    /// Advertised size, used only for progress reporting.
    pub size: Option<u64>,
}

impl DownloadTask {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            expected_sha1: None,
            expected_sha256: None,
            expected_sha512: None,
            size: None,
        }
    }

    pub fn with_sha1(mut self, sha1: impl Into<String>) -> Self {
        let sha1 = sha1.into();
        if !sha1.is_empty() {
            self.expected_sha1 = Some(sha1);
        }
        self
    }

    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        let sha256 = sha256.into();
        if !sha256.is_empty() {
            self.expected_sha256 = Some(sha256);
        }
        self
    }

    pub fn with_sha512(mut self, sha512: impl Into<String>) -> Self {
        let sha512 = sha512.into();
        if !sha512.is_empty() {
            self.expected_sha512 = Some(sha512);
        }
        self
    }
}

/// Hashes computed for a file, keyed by algorithm.
#[derive(Debug, Clone, Default)]
pub struct FileHashes {
    pub sha1: String,
    pub sha256: String,
    pub sha512: String,
}

impl FileHashes {
    /// True when every supplied expectation matches.
    pub fn matches(&self, sha1: Option<&str>, sha256: Option<&str>, sha512: Option<&str>) -> bool {
        let ok = |expected: Option<&str>, actual: &str| {
            expected
                .map(|e| e.eq_ignore_ascii_case(actual))
                .unwrap_or(true)
        };
        ok(sha1, &self.sha1) && ok(sha256, &self.sha256) && ok(sha512, &self.sha512)
    }
}

/// Does an existing file on disk already match the expected hashes?
pub fn file_matches(path: &Path, sha1: Option<&str>, sha256: Option<&str>) -> bool {
    if !path.is_file() {
        return false;
    }
    if sha1.is_none() && sha256.is_none() {
        // No hash to check against — trust a non-empty file.
        return path.metadata().map(|m| m.len() > 0).unwrap_or(false);
    }
    hash_file(path)
        .map(|hashes| hashes.matches(sha1, sha256, None))
        .unwrap_or(false)
}

fn hash_file(path: &Path) -> anyhow::Result<FileHashes> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut sha1 = sha1::Sha1::new();
    let mut sha256 = sha2::Sha256::new();
    let mut sha512 = sha2::Sha512::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha1.update(&buf[..n]);
        Digest::update(&mut sha256, &buf[..n]);
        Digest::update(&mut sha512, &buf[..n]);
    }
    Ok(FileHashes {
        sha1: format!("{:x}", sha1.finalize()),
        sha256: format!("{:x}", sha256.finalize()),
        sha512: format!("{:x}", sha512.finalize()),
    })
}

/// Download a single file unless it is already present and hash-correct.
///
/// Returns `Ok(true)` when bytes were fetched, `Ok(false)` on a cache hit.
pub async fn download_file(client: &reqwest::Client, task: &DownloadTask) -> anyhow::Result<bool> {
    if task.dest.is_file() {
        let cached = hash_file(&task.dest)
            .map(|hashes| {
                hashes.matches(
                    task.expected_sha1.as_deref(),
                    task.expected_sha256.as_deref(),
                    task.expected_sha512.as_deref(),
                )
            })
            .unwrap_or(false);
        if cached {
            return Ok(false);
        }
    }

    if let Some(parent) = task.dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create dir: {}", parent.display()))?;
    }

    let part = part_path(&task.dest);
    let mut last_err = None;

    for attempt in 1..=MAX_ATTEMPTS {
        match fetch_once(client, task, &part).await {
            Ok(()) => {
                tokio::fs::rename(&part, &task.dest)
                    .await
                    .with_context(|| format!("failed to finalize {}", task.dest.display()))?;
                return Ok(true);
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                tracing::debug!(
                    "download attempt {attempt}/{MAX_ATTEMPTS} failed for {}: {e}",
                    task.url
                );
                last_err = Some(e);
                if attempt < MAX_ATTEMPTS {
                    let backoff =
                        std::time::Duration::from_millis(500 * 2u64.pow(attempt as u32 - 1));
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("download failed")))
        .with_context(|| format!("failed to download {}", task.url))
}

async fn fetch_once(
    client: &reqwest::Client,
    task: &DownloadTask,
    part: &Path,
) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut resp = client.get(&task.url).send().await?.error_for_status()?;

    let mut file = tokio::fs::File::create(part)
        .await
        .with_context(|| format!("failed to create {}", part.display()))?;

    let mut sha1 = sha1::Sha1::new();
    let mut sha256 = sha2::Sha256::new();
    let mut sha512 = sha2::Sha512::new();

    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk).await?;
        sha1.update(&chunk);
        Digest::update(&mut sha256, &chunk);
        Digest::update(&mut sha512, &chunk);
    }
    file.flush().await?;
    drop(file);

    let actual = FileHashes {
        sha1: format!("{:x}", sha1.finalize()),
        sha256: format!("{:x}", sha256.finalize()),
        sha512: format!("{:x}", sha512.finalize()),
    };

    if let Some(expected) = &task.expected_sha1 {
        if !expected.eq_ignore_ascii_case(&actual.sha1) {
            bail!("SHA-1 mismatch: expected {expected}, got {}", actual.sha1);
        }
    }
    if let Some(expected) = &task.expected_sha256 {
        if !expected.eq_ignore_ascii_case(&actual.sha256) {
            bail!(
                "SHA-256 mismatch: expected {expected}, got {}",
                actual.sha256
            );
        }
    }
    if let Some(expected) = &task.expected_sha512 {
        if !expected.eq_ignore_ascii_case(&actual.sha512) {
            bail!(
                "SHA-512 mismatch: expected {expected}, got {}",
                actual.sha512
            );
        }
    }

    Ok(())
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

/// Download many files in parallel (bounded to 16 in flight).
///
/// `progress` is called with `(completed, total)` after each file settles.
/// Failures are collected rather than short-circuiting, so one bad mirror does
/// not hide the rest of the report.
pub async fn download_many(
    client: &reqwest::Client,
    tasks: Vec<DownloadTask>,
    progress: Arc<dyn Fn(usize, usize) + Send + Sync>,
) -> anyhow::Result<usize> {
    let total = tasks.len();
    if total == 0 {
        return Ok(0);
    }

    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));
    let done = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(total);

    for task in tasks {
        let sem = sem.clone();
        let client = client.clone();
        let done = done.clone();
        let progress = progress.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem
                .acquire()
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let result = download_file(&client, &task).await;
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(n, total);
            result
        }));
    }

    let mut fetched = 0usize;
    let mut errors = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(fetched_this)) => {
                if fetched_this {
                    fetched += 1;
                }
            }
            Ok(Err(e)) => errors.push(e.to_string()),
            Err(e) => errors.push(e.to_string()),
        }
    }

    if !errors.is_empty() {
        bail!(
            "{} of {total} downloads failed: {}",
            errors.len(),
            errors
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }

    Ok(fetched)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_task_serialization() {
        let task = DownloadTask::new("https://example.com/test.jar", "/tmp/test.jar")
            .with_sha1("abc123")
            .with_sha256("def456")
            .with_sha512("aabbcc");
        let json = serde_json::to_string(&task).expect("serialize");
        assert!(json.contains("test.jar"));
        assert!(json.contains("aabbcc"));
    }

    #[test]
    fn each_algorithm_is_actually_verified() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload.bin");
        std::fs::write(&file, b"hello world").unwrap();

        let hashes = hash_file(&file).unwrap();
        assert_eq!(hashes.sha1.len(), 40);
        assert_eq!(hashes.sha256.len(), 64);
        assert_eq!(hashes.sha512.len(), 128);

        // Correct hashes all pass.
        assert!(hashes.matches(Some(&hashes.sha1), None, Some(&hashes.sha512)));
        // A SHA-512 supplied where a SHA-256 belongs must NOT pass — this was a
        // real bug that silently rejected every Modrinth download.
        assert!(!hashes.matches(None, Some(&hashes.sha512), None));
        assert!(!hashes.matches(Some("deadbeef"), None, None));
    }

    #[test]
    fn part_path_appends_suffix() {
        assert_eq!(
            part_path(Path::new("/tmp/a/b.jar")),
            PathBuf::from("/tmp/a/b.jar.part")
        );
    }

    #[test]
    fn matching_sha1_is_a_cache_hit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        std::fs::write(&file, b"hello world").unwrap();
        let sha1 = format!("{:x}", sha1::Sha1::digest(b"hello world"));
        assert!(file_matches(&file, Some(&sha1), None));
        assert!(!file_matches(&file, Some("deadbeef"), None));
        assert!(!file_matches(
            &dir.path().join("missing.txt"),
            Some(&sha1),
            None
        ));
    }
}
