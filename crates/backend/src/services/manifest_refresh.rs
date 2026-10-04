
use tokio::time::{interval, Duration};
use tracing::info;

pub async fn run_refresh_loop() {
    let mut interval = interval(Duration::from_secs(900));
    loop {
        interval.tick().await;
        info!("Refreshing manifests...");
    }
}
