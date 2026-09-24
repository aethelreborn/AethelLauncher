//! Crash reporting.

use anyhow::Context;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CrashReport {
    pub kind: String,
    pub mc_version: String,
    pub bundle_version: String,
    pub stack_trace: String,
    pub reported_at: String,
}

pub fn parse_crash_report(path: &std::path::Path) -> anyhow::Result<CrashReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read crash report: {:?}", path))?;

    let mut mc_version = "unknown".to_string();
    let mut stack_trace = String::new();

    for line in content.lines() {
        if line.starts_with("Minecraft Version:") {
            mc_version = line
                .split(':')
                .nth(1)
                .unwrap_or("unknown")
                .trim()
                .to_string();
        }
        if line.starts_with("Stacktrace:") || line.starts_with("Caused by:") {
            stack_trace.push_str(line);
            stack_trace.push('\n');
        }
    }

    Ok(CrashReport {
        kind: "game_crash".to_string(),
        mc_version,
        bundle_version: env!("CARGO_PKG_VERSION").to_string(),
        stack_trace,
        reported_at: chrono::Utc::now().to_rfc3339(),
    })
}
