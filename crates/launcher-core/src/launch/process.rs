use super::args_builder::LaunchArgs;
use super::ipc_server::{IpcMessage, IpcServer};
use anyhow::Context;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

const GRACE_MS: u64 = 5000;

pub async fn spawn_game(
    java: &Path,
    args: &LaunchArgs,
    main_class: &str,
    cwd: &Path,
    ipc_server: Option<&IpcServer>,
) -> anyhow::Result<tokio::process::Child> {
    let mut cmd = Command::new(java);

    for flag in &args.jvm_args {
        cmd.arg(flag);
    }

    if let Some(ipc) = ipc_server {
        for ipc_arg in ipc.jvm_args() {
            cmd.arg(ipc_arg);
        }
    }

    if !main_class.is_empty() {
        cmd.arg(main_class);
    }
    for arg in &args.game_args {
        cmd.arg(arg);
    }

    cmd.current_dir(cwd);

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());

    cmd.spawn().context("failed to spawn Java process")
}

pub async fn run_game(
    java: &Path,
    args: &LaunchArgs,
    main_class: &str,
    cwd: &Path,
    ipc_server: Option<&IpcServer>,
    log: UnboundedSender<String>,
    stop: Arc<tokio::sync::Notify>,
) -> anyhow::Result<Option<i32>> {
    let mut child = spawn_game(java, args, main_class, cwd, ipc_server).await?;

    let mut pumps = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let tx = log.clone();
        pumps.push(tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).is_err() {
                    break;
                }
            }
        }));
    }
    if let Some(stderr) = child.stderr.take() {
        let tx = log.clone();
        pumps.push(tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).is_err() {
                    break;
                }
            }
        }));
    }

    let status = tokio::select! {
        result = child.wait() => result?,
        _ = stop.notified() => {
            if let Some(ipc) = ipc_server {
                ipc.send(&IpcMessage::Exit {
                    v: 1,
                    code: 0,
                    reason: "launcher_stop".to_string(),
                    grace_ms: Some(GRACE_MS),
                });
                tokio::select! {
                    _ = ipc.wait_bye() => {}
                    _ = tokio::time::sleep(Duration::from_millis(GRACE_MS)) => {}
                }
            }
            let _ = child.kill().await;
            child.wait().await?
        }
    };

    let grace = tokio::time::Instant::now() + Duration::from_secs(2);
    for mut pump in pumps {
        let remaining = grace.saturating_duration_since(tokio::time::Instant::now());
        if tokio::time::timeout(remaining, &mut pump).await.is_err() {
            pump.abort();
        }
    }

    Ok(status.code())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::args_builder::LaunchArgs;

    #[tokio::test]
    async fn run_game_streams_output_and_reports_exit_code() {
        let args = LaunchArgs {
            jvm_args: Vec::new(),
            game_args: vec![
                "-c".to_string(),
                "echo hello-from-game; echo oops >&2; exit 3".to_string(),
            ],
            classpath: Vec::new(),
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let stop = Arc::new(tokio::sync::Notify::new());

        let code = run_game(
            Path::new("/bin/sh"),
            &args,
            "",
            Path::new("/tmp"),
            None,
            tx,
            stop,
        )
        .await
        .expect("run");

        assert_eq!(code, Some(3));

        let mut seen = Vec::new();
        while let Ok(line) = rx.try_recv() {
            seen.push(line);
        }
        assert!(
            seen.iter().any(|l| l.contains("hello-from-game")),
            "stdout not captured: {seen:?}"
        );
        assert!(
            seen.iter().any(|l| l.contains("oops")),
            "stderr not captured: {seen:?}"
        );
    }

    #[tokio::test]
    async fn stop_signal_kills_the_child() {
        let args = LaunchArgs {
            jvm_args: Vec::new(),
            game_args: vec!["-c".to_string(), "sleep 30".to_string()],
            classpath: Vec::new(),
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let stop = Arc::new(tokio::sync::Notify::new());

        let stop_handle = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            stop_handle.notify_one();
        });

        let started = std::time::Instant::now();
        let code = run_game(
            Path::new("/bin/sh"),
            &args,
            "",
            Path::new("/tmp"),
            None,
            tx,
            stop,
        )
        .await
        .expect("run");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(code, None);
    }
}
