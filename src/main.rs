mod broker;
mod display;
mod stats;

use anyhow::Result;
use std::sync::Arc;
use tokio::signal;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("mqtt_broker_rust=info".parse()?)
                .add_directive("rumqttd=warn".parse()?),
        )
        .init();

    let stats = Arc::new(stats::BrokerStats::new());

    broker::start()?;
    info!("MQTT broker started on 0.0.0.0:1883  (console → http://<ip>:3030)");

    // Monitor: connects as an internal MQTT client to gather stats
    tokio::spawn(stats::run_monitor(Arc::clone(&stats)));

    // E-paper display: runs in a blocking OS thread (SPI is synchronous)
    let display_stats = Arc::clone(&stats);
    tokio::task::spawn_blocking(move || display::run_blocking(display_stats));

    signal::ctrl_c().await?;
    info!("Shutting down");
    std::process::exit(0);
}
