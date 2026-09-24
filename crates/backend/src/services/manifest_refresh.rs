//! Periodic manifest refresh cron job.
//!
//! Fetches latest Fabric meta and Mojang manifests, updates cache.

use tokio::time::{interval, Duration};
use tracing::info;

pub async fn run_refresh_loop() {
    let mut interval = interval(Duration::from_secs(900)); // 15 minutes
    loop {
        interval.tick().await;
        info!("Refreshing manifests...");
        // TODO: fetch fresh manifests from Mojang/Fabric APIs
    }
}
