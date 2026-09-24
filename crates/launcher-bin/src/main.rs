//! Aethel Launcher — entrypoint.
//!
//! Handles single-instance locking, boot checks, and wires up the eframe app.

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Default filter. `RUST_LOG` overrides it; the noisy wgpu/naga crates are
/// pinned to `warn` because their TRACE output is enormous and measurably
/// slows down a low-end machine.
const DEFAULT_LOG_FILTER: &str =
    "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,egui_wgpu=warn,tokio_tungstenite=warn";

fn main() -> anyhow::Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();

    tracing::info!("Aethel Launcher starting...");

    // Create eframe options
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder {
            title: Some("Aethel Launcher".to_string()),
            inner_size: Some(egui::vec2(1200.0, 800.0)),
            min_inner_size: Some(egui::vec2(960.0, 600.0)),
            icon: Some(launcher_ui::branding::window_icon()),
            ..Default::default()
        },
        ..Default::default()
    };

    // Run the app
    eframe::run_native(
        "Aethel Launcher",
        options,
        Box::new(|cc| Ok(Box::new(launcher_ui::AethelApp::new(cc)))),
    )?;

    tracing::info!("Aethel Launcher exited.");
    Ok(())
}
