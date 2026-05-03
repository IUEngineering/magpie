use std::sync::Arc;
use crate::stats::BrokerStats;

// ── Feature-gated e-paper implementation ────────────────────────────────────

#[cfg(feature = "rpi-display")]
mod epaper {
    use std::sync::Arc;
    use std::time::Duration;

    use embedded_graphics::{
        mono_font::{ascii::FONT_10X20, ascii::FONT_9X15_BOLD, MonoTextStyle},
        prelude::*,
        primitives::{Line, PrimitiveStyle},
        text::{Baseline, Text},
    };
    use epd_waveshare::{
        // epd1in54_v2 is the right module for the 1.54" rev2.1 (B&W, 200×200)
        epd1in54_v2::{Display1in54, Epd1in54},
        prelude::*,
    };
    use rppal::{
        gpio::Gpio,
        spi::{Bus, Mode, SimpleHalSpiDevice, SlaveSelect, Spi},
    };
    use tracing::{error, info, warn};

    use crate::stats::BrokerStats;

    // BCM GPIO numbers — adjust if your wiring differs
    const DC_PIN: u8   = 25;
    const RST_PIN: u8  = 17;
    const BUSY_PIN: u8 = 24;

    // Seconds between full-panel refreshes (e-paper full refresh takes ~2 s)
    const REFRESH_SECS: u64 = 5;

    // ── embedded-hal 1.0 delay backed by thread::sleep ──────────────────────

    struct Delay;

    impl embedded_hal::delay::DelayNs for Delay {
        fn delay_ns(&mut self, ns: u32) {
            std::thread::sleep(Duration::from_nanos(ns as u64));
        }
    }

    // ── Main blocking loop ────────────────────────────────────────────────────

    pub fn run_blocking(stats: Arc<BrokerStats>) {
        info!("Display thread started");

        let gpio = match Gpio::new() {
            Ok(g)  => g,
            Err(e) => { error!("GPIO init failed: {e}"); return; }
        };
        info!("GPIO ok");

        let dc   = match gpio.get(DC_PIN)   { Ok(p) => p.into_output(), Err(e) => { error!("DC pin {DC_PIN}: {e}");    return; } };
        let rst  = match gpio.get(RST_PIN)  { Ok(p) => p.into_output(), Err(e) => { error!("RST pin {RST_PIN}: {e}");  return; } };
        let busy = match gpio.get(BUSY_PIN) { Ok(p) => p.into_input(),  Err(e) => { error!("BUSY pin {BUSY_PIN}: {e}"); return; } };
        info!("GPIO pins ok (DC={DC_PIN}, RST={RST_PIN}, BUSY={BUSY_PIN})");

        // SPI0, CE0 (hardware CS on GPIO 8), 4 MHz, Mode 0.
        // Wrap with SimpleHalSpiDevice to satisfy the SpiDevice trait bound.
        let raw_spi = match Spi::new(Bus::Spi0, SlaveSelect::Ss0, 4_000_000, Mode::Mode0) {
            Ok(s)  => s,
            Err(e) => { error!("SPI init failed: {e}"); return; }
        };
        info!("SPI ok");

        let mut spi = SimpleHalSpiDevice::new(raw_spi);
        let mut delay = Delay;

        info!("Initializing EPD (may take a few seconds)...");
        let mut epd = match Epd1in54::new(&mut spi, busy, dc, rst, &mut delay, None) {
            Ok(e)  => e,
            Err(e) => { error!("EPD init failed: {e:?}"); return; }
        };

        let mut display = Display1in54::default();
        info!("E-paper display ready — refreshing every {REFRESH_SECS} s");

        loop {
            render(&mut display, &stats);

            match epd.update_frame(&mut spi, display.buffer(), &mut delay) {
                Ok(_)  => info!("Frame sent to display"),
                Err(e) => { warn!("Frame update error: {e:?}"); continue; }
            }
            match epd.display_frame(&mut spi, &mut delay) {
                Ok(_)  => info!("Display refreshed"),
                Err(e) => warn!("Display frame error: {e:?}"),
            }

            std::thread::sleep(Duration::from_secs(REFRESH_SECS));
        }
    }

    fn render(display: &mut Display1in54, stats: &BrokerStats) {
        display.clear(Color::White).ok();

        let title  = MonoTextStyle::new(&FONT_10X20,    Color::Black);
        let body   = MonoTextStyle::new(&FONT_9X15_BOLD, Color::Black);
        let stroke = PrimitiveStyle::with_stroke(Color::Black, 2);

        // Title — centred
        Text::with_baseline("MQTT Broker", Point::new(15, 4), title, Baseline::Top)
            .draw(display).ok();

        Line::new(Point::new(0, 28), Point::new(200, 28))
            .into_styled(stroke)
            .draw(display).ok();

        let ip_str = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "?.?.?.?".into());

        let clients  = stats.get_clients();
        let messages = stats.get_messages();
        let secs     = stats.uptime_secs();
        let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);

        Text::with_baseline(&format!("IP {ip_str}"),            Point::new(4,  36), body, Baseline::Top).draw(display).ok();
        Text::with_baseline(&format!("Port    1883"),           Point::new(4,  58), body, Baseline::Top).draw(display).ok();
        Text::with_baseline(&format!("Clients {clients}"),      Point::new(4,  80), body, Baseline::Top).draw(display).ok();
        Text::with_baseline(&format!("Msgs    {messages}"),     Point::new(4, 102), body, Baseline::Top).draw(display).ok();
        Text::with_baseline(&format!("Up {h:02}:{m:02}:{s:02}"), Point::new(4, 124), body, Baseline::Top).draw(display).ok();

        Line::new(Point::new(0, 148), Point::new(200, 148))
            .into_styled(stroke)
            .draw(display).ok();

        Text::with_baseline("RUNNING", Point::new(50, 156), title, Baseline::Top)
            .draw(display).ok();
    }
}

// ── Public entry point ───────────────────────────────────────────────────────

#[cfg(feature = "rpi-display")]
pub fn run_blocking(stats: Arc<BrokerStats>) {
    epaper::run_blocking(stats);
}

#[cfg(not(feature = "rpi-display"))]
pub fn run_blocking(_stats: Arc<BrokerStats>) {
    tracing::info!(
        "E-paper display disabled. Rebuild with `--features rpi-display` on the Pi."
    );
}
