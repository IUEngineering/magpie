use anyhow::Result;
use rumqttd::{Broker, Config};

const CONFIG_TOML: &str = include_str!("../config/broker.toml");

pub fn start() -> Result<()> {
    let config: Config = toml::from_str(CONFIG_TOML)?;
    let mut broker = Broker::new(config);

    std::thread::Builder::new()
        .name("rumqttd".into())
        .spawn(move || {
            if let Err(e) = broker.start() {
                tracing::error!("Broker exited with error: {e}");
            }
        })?;

    Ok(())
}
