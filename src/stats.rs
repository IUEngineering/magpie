use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::time::{sleep, Duration};
use tracing::{debug, warn};

pub struct BrokerStats {
    pub connected_clients: AtomicU64,
    pub total_messages: AtomicU64,
    start_time: Instant,
}

impl BrokerStats {
    pub fn new() -> Self {
        Self {
            connected_clients: AtomicU64::new(0),
            total_messages: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn get_clients(&self) -> u64 {
        self.connected_clients.load(Ordering::Relaxed)
    }

    pub fn get_messages(&self) -> u64 {
        self.total_messages.load(Ordering::Relaxed)
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

/// Connects as an internal MQTT client and keeps stats up-to-date.
/// Subscribes to `#` to count every published message and to
/// `$SYS/broker/clients/connected` for the live client count
/// (rumqttd publishes $SYS stats when the console feature is active).
pub async fn run_monitor(stats: Arc<BrokerStats>) {
    // Give the broker a moment to finish binding its socket.
    sleep(Duration::from_secs(2)).await;

    let mut opts = MqttOptions::new("internal-monitor", "127.0.0.1", 1883);
    opts.set_keep_alive(Duration::from_secs(30));

    let (client, mut eventloop) = AsyncClient::new(opts, 256);

    if let Err(e) = client.subscribe("#", QoS::AtMostOnce).await {
        warn!("Monitor subscribe failed: {e}");
        return;
    }

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::Publish(p))) => {
                if p.topic.starts_with("$SYS/broker/clients/connected") {
                    if let Some(n) = std::str::from_utf8(&p.payload)
                        .ok()
                        .and_then(|s| s.trim().parse::<u64>().ok())
                    {
                        stats.connected_clients.store(n, Ordering::Relaxed);
                    }
                } else if !p.topic.starts_with("$SYS/") {
                    stats.total_messages.fetch_add(1, Ordering::Relaxed);
                    debug!("pub {}", p.topic);
                }
            }
            Ok(_) => {}
            Err(e) => {
                warn!("Monitor error: {e} — reconnecting in 5 s");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
