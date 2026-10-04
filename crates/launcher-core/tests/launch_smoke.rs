use launcher_core::auth::{offline_uuid, Account};
use launcher_core::install::pipeline::{prepare, InstallOptions, Progress, ProgressSink};
use launcher_core::launch::process::run_game;
use launcher_core::perf::{GraphicsPreset, PerfConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn run_for() -> Duration {
    std::env::var("AETHEL_SMOKE_RUN_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(120))
}

const STARTUP_GRACE: Duration = Duration::from_secs(300);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "downloads ~300MB and launches the real game; run explicitly"]
async fn launches_real_minecraft_without_fatal_errors() {
    let version = std::env::var("AETHEL_SMOKE_VERSION").unwrap_or_else(|_| "1.21.4".to_string());
    let root = std::path::PathBuf::from(format!("/tmp/aethel-smoke/{version}"));
    let shared = root.join("shared");
    let game_dir = root.join("instance").join("minecraft");

    let auth = Account::Offline {
        name: "AethelTest".to_string(),
        uuid: offline_uuid("AethelTest"),
    };

    let transcript: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let sink: ProgressSink = {
        let transcript = transcript.clone();
        Arc::new(move |event: Progress| {
            let line = match event {
                Progress::Stage(stage) => format!("[stage] {stage}"),
                Progress::Files {
                    completed,
                    total,
                    label,
                } => format!("[files] {label} {completed}/{total}"),
                Progress::Log(line) => line,
            };
            println!("{line}");
            transcript.lock().unwrap().push(line);
        })
    };

    let performance = match std::env::var("AETHEL_SMOKE_PERF").as_deref() {
        Ok("1") | Ok("true") => PerfConfig {
            enabled: true,
            preset: GraphicsPreset::Performance,
        },
        _ => PerfConfig {
            enabled: false,
            preset: GraphicsPreset::Performance,
        },
    };

    let options = InstallOptions {
        version_id: version.clone(),
        game_dir: game_dir.clone(),
        shared_dir: shared.clone(),
        ram_mb: 2048,
        renderer: std::env::var("AETHEL_SMOKE_RENDERER").unwrap_or_else(|_| "auto".to_string()),
        auth: auth.clone(),
        java_path: None,
        performance,
    };

    let prepared = prepare(&options, &sink)
        .await
        .expect("install pipeline failed");
    println!("java: {}", prepared.java.display());
    println!("main class: {}", prepared.main_class);
    println!("classpath entries: {}", prepared.args.classpath.len());
    println!("perf: {}", prepared.perf.summary());

    if std::env::var("AETHEL_SMOKE_DUMP").is_ok() {
        let quoted: Vec<String> = prepared
            .args
            .jvm_args
            .iter()
            .chain(std::iter::once(&prepared.main_class))
            .chain(prepared.args.game_args.iter())
            .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
            .collect();
        println!("COMMAND: {} {}", prepared.java.display(), quoted.join(" "));
    }
    assert!(
        prepared.args.classpath.len() > 5,
        "classpath is suspiciously small"
    );

    let heard_from_the_game = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let collector = {
        let transcript = transcript.clone();
        let heard = heard_from_the_game.clone();
        tokio::spawn(async move {
            while let Some(line) = log_rx.recv().await {
                heard.store(true, std::sync::atomic::Ordering::Relaxed);
                println!("[game] {line}");
                transcript.lock().unwrap().push(line);
            }
        })
    };

    let stop = Arc::new(tokio::sync::Notify::new());
    let run_for = run_for();
    let watchdog = {
        let stop = stop.clone();
        let heard = heard_from_the_game.clone();
        tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + STARTUP_GRACE;
            while !heard.load(std::sync::atomic::Ordering::Relaxed)
                && tokio::time::Instant::now() < deadline
            {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            tokio::time::sleep(run_for).await;
            stop.notify_one();
        })
    };

    let exit_code = run_game(
        &prepared.java,
        &prepared.args,
        &prepared.main_class,
        &prepared.cwd,
        None,
        log_tx,
        stop,
    )
    .await
    .expect("failed to run the game process");

    watchdog.abort();
    let _ = collector.await;
    println!("exit code: {exit_code:?}");

    let output = transcript.lock().unwrap().join("\n");

    let relevant: String = output
        .lines()
        .filter(|line| !line.contains("Error loading class:"))
        .collect::<Vec<_>>()
        .join("\n");

    for fatal in [
        "ClassNotFoundException",
        "NoClassDefFoundError",
        "UnsatisfiedLinkError",
        "Could not find or load main class",
        "Error: Could not find or load main class",
        "natives directory",
        "Failed to locate library",
        "Incompatible mods found",
        "Mod resolution failed",
        "duplicate ASM classes",
    ] {
        assert!(
            !relevant.contains(fatal),
            "launch hit a fatal resolution error (`{fatal}`):\n\n{output}"
        );
    }

    match std::env::var("AETHEL_SMOKE_RENDERER").as_deref() {
        Ok("vulkan") => assert!(
            output.to_ascii_lowercase().contains("vulkanmod"),
            "renderer=vulkan but VulkanMod never loaded:\n\n{output}"
        ),
        Ok("opengl") => assert!(
            output.to_ascii_lowercase().contains("sodium"),
            "renderer=opengl but Sodium never loaded:\n\n{output}"
        ),
        _ => {}
    }

    let initialised = output.contains("Setting user")
        || output.contains("LWJGL")
        || output.contains("Backend library")
        || output.contains("Fabric Loader")
        || output.contains("Loading Minecraft")
        || exit_code == Some(0);
    assert!(initialised, "the game never initialised:\n\n{output}");
}
