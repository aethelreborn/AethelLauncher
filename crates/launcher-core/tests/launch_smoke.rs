//! End-to-end smoke test: install a real version and actually spawn the JVM.
//!
//! Downloads a few hundred MB and opens a game window, so it is `#[ignore]`d.
//! Run explicitly:
//!
//! ```text
//! AETHEL_SMOKE_VERSION=1.21.4 \
//!   cargo test -p launcher-core --test launch_smoke -- --ignored --nocapture
//! ```
//!
//! The point is not to play the game — it is to prove the classpath, natives,
//! asset index and argument expansion are all correct, by asserting the JVM
//! gets past class loading without the classic launch failures.

use launcher_core::auth::{offline_uuid, Account};
use launcher_core::install::pipeline::{prepare, InstallOptions, Progress, ProgressSink};
use launcher_core::launch::process::run_game;
use launcher_core::perf::{GraphicsPreset, PerfConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the game is allowed to run, counted **from its first log line**
/// rather than from spawn.
///
/// That distinction matters: the first launch of a freshly installed Fabric
/// profile remaps every jar, and the JVM can sit completely silent for a
/// minute or more while it does. A fixed window measured from spawn would kill
/// the game mid-remap and report a launch failure that is not real.
///
/// Overridable with `AETHEL_SMOKE_RUN_SECS`.
fn run_for() -> Duration {
    std::env::var("AETHEL_SMOKE_RUN_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(120))
}

/// How long the JVM may stay silent before we give up waiting for it to say
/// anything at all.
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

    // The performance layer defaults on, but the smoke test is about whether
    // the vanilla classpath/natives/args are right, so it runs with it off.
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

    // Dumping the exact command makes a failing launch reproducible by hand,
    // outside the harness (which is the only way to see a JVM that hangs
    // without ever writing to stdout).
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

    // Stream the game's output into the same transcript.
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
            // Wait for the JVM to say something (cold Fabric installs remap
            // silently for a while), then let it run for the configured window.
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

    // Mod loaders probe for *optional* integrations by trying to load a class
    // and swallowing the failure. Those probes log a `ClassNotFoundException`
    // as an expected warning (Mod Menu looking for Sodium, for instance), so
    // they must not be mistaken for a broken classpath.
    let relevant: String = output
        .lines()
        .filter(|line| !line.contains("Error loading class:"))
        .collect::<Vec<_>>()
        .join("\n");

    // These are the failures that mean *our* resolution was wrong, as opposed to
    // the sandbox having no GPU/audio.
    for fatal in [
        "ClassNotFoundException",
        "NoClassDefFoundError",
        "UnsatisfiedLinkError",
        "Could not find or load main class",
        "Error: Could not find or load main class",
        "natives directory",
        "Failed to locate library",
        // A mod the launcher picked has an unsatisfied dependency, or the
        // renderer pack was wrong. Fabric refuses to load *anything* in that
        // case, so it is a launcher bug, not an environment problem.
        "Incompatible mods found",
        "Mod resolution failed",
        "duplicate ASM classes",
    ] {
        assert!(
            !relevant.contains(fatal),
            "launch hit a fatal resolution error (`{fatal}`):\n\n{output}"
        );
    }

    // When the renderer pack was requested, the renderer mod itself must have
    // made it into the game: a Vulkan launch that silently fell back to Sodium
    // (or to nothing) is exactly the bug this test exists to catch.
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

    // Some evidence the JVM actually executed game code rather than bailing out.
    // The Fabric banner counts: reaching the loader proves the classpath,
    // natives and argument expansion all resolved, which is what this test is
    // for. (VulkanMod has no working device in CI, so it never gets further.)
    let initialised = output.contains("Setting user")
        || output.contains("LWJGL")
        || output.contains("Backend library")
        || output.contains("Fabric Loader")
        || output.contains("Loading Minecraft")
        || exit_code == Some(0);
    assert!(initialised, "the game never initialised:\n\n{output}");
}
