use std::sync::Arc;
use crate::stats::BrokerStats;

// ── Feature-gated e-paper implementation ────────────────────────────────────

#[cfg(feature = "rpi-display")]
mod epaper {
    use std::sync::Arc;
    use std::time::Duration;

    use embedded_graphics::{
        mono_font::{ascii::FONT_6X10, ascii::FONT_9X18_BOLD, MonoTextStyle},
        pixelcolor::BinaryColor,
        prelude::*,
        primitives::{Line, PrimitiveStyle},
        text::{Baseline, Text},
    };
    use epd_waveshare::{
        epd1in54_v2::{Display1in54V2, Epd1in54V2},
        prelude::*,
    };
    use rppal::{
        gpio::Gpio,
        spi::{Bus, Mode, SlaveSelect, Spi},
    };
    use tracing::{error, info, warn};

    use crate::stats::BrokerStats;

    // BCM GPIO numbers — adjust if your wiring differs
    const DC_PIN: u8   = 25;
    const RST_PIN: u8  = 17;
    const BUSY_PIN: u8 = 24;

    // Seconds between full-panel refreshes (e-paper is slow ~2 s per refresh)
    const REFRESH_SECS: u64 = 30;

    // ── Minimal embedded-hal delay backed by thread::sleep ──────────────────

    struct Delay;

    impl embedded_hal::blocking::delay::DelayMs<u8> for Delay {
        fn delay_ms(&mut self, ms: u8) {
            std::thread::sleep(Duration::from_millis(ms as u64));
        }
    }

    impl embedded_hal::blocking::delay::DelayMs<u32> for Delay {
        fn delay_ms(&mut self, ms: u32) {
            std::thread::sleep(Duration::from_millis(ms as u64));
        }
    }

    // ── No-op CS pin ─────────────────────────────────────────────────────────
    // Hardware SPI (CE0 / GPIO 8) manages chip-select automatically.
    // We pass this dummy to the EPD constructor so it does nothing extra.

    struct NoPin;

    impl embedded_hal::digital::v2::OutputPin for NoPin {
        type Error = core::convert::Infallible;
        fn set_high(&mut self) -> Result<(), Self::Error> { Ok(()) }
        fn set_low(&mut self)  -> Result<(), Self::Error> { Ok(()) }
    }

    // ── Main blocking loop ────────────────────────────────────────────────────

    pub fn run_blocking(stats: Arc<BrokerStats>) {
        let gpio = match Gpio::new() {
            Ok(g)  => g,
            Err(e) => { error!("GPIO init failed: {e}"); return; }
        };

        let dc   = match gpio.get(DC_PIN)   { Ok(p) => p.into_output(), Err(e) => { error!("DC pin {DC_PIN}: {e}");   return; } };
        let rst  = match gpio.get(RST_PIN)  { Ok(p) => p.into_output(), Err(e) => { error!("RST pin {RST_PIN}: {e}");  return; } };
        let busy = match gpio.get(BUSY_PIN) { Ok(p) => p.into_input(),  Err(e) => { error!("BUSY pin {BUSY_PIN}: {e}"); return; } };

        // SPI0, CE0, 4 MHz, Mode 0
        let mut spi = match Spi::new(Bus::Spi0, SlaveSelect::Ss0, 4_000_000, Mode::Mode0) {
            Ok(s)  => s,
            Err(e) => { error!("SPI init failed: {e}"); return; }
        };

        let mut delay = Delay;

        // If you have the V1 display, change this to:
        //   epd_waveshare::epd1in54::{Display1in54, Epd1in54}
        // and update the types below accordingly.
        let mut epd = match Epd1in54V2::new(&mut spi, NoPin, busy, dc, rst, &mut delay) {
            Ok(e)  => e,
            Err(e) => { error!("EPD init failed: {e:?}"); return; }
        };

        let mut display = Display1in54V2::default();
        info!("E-paper display ready — refreshing every {REFRESH_SECS} s");

        loop {
            render(&mut display, &stats);

            if let Err(e) = epd.update_frame(&mut spi, display.buffer(), &mut delay) {
                warn!("Frame update error: {e:?}");
            } else if let Err(e) = epd.display_frame(&mut spi, &mut delay) {
                warn!("Display frame error: {e:?}");
            }

            std::thread::sleep(Duration::from_secs(REFRESH_SECS));
        }
    }

    fn render(display: &mut Display1in54V2, stats: &BrokerStats) {
        display.clear_buffer(Color::White);

        let bold   = MonoTextStyle::new(&FONT_9X18_BOLD, BinaryColor::On);
        let normal = MonoTextStyle::new(&FONT_6X10,      BinaryColor::On);
        let stroke = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

        // Title row
        Text::with_baseline("MQTT Broker", Point::new(12, 6), bold, Baseline::Top)
            .draw(display).ok();

        // Divider
        Line::new(Point::new(0, 30), Point::new(200, 30))
            .into_styled(stroke)
            .draw(display).ok();

        // Local IP
        let ip_str = local_ip_address::local_ip()
            .map(|ip| format!("{}:1883", ip))
            .unwrap_or_else(|_| "?.?.?.?:1883".into());

        Text::with_baseline(&format!("IP  {ip_str}"),  Point::new(4, 38), normal, Baseline::Top).draw(display).ok();

        // Stats
        let clients  = stats.get_clients();
        let messages = stats.get_messages();
        let secs     = stats.uptime_secs();
        let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);

        Text::with_baseline(&format!("CLI {clients}"),        Point::new(4, 56), normal, Baseline::Top).draw(display).ok();
        Text::with_baseline(&format!("MSG {messages}"),       Point::new(4, 70), normal, Baseline::Top).draw(display).ok();
        Text::with_baseline(&format!("UP  {h:02}:{m:02}:{s:02}"), Point::new(4, 84), normal, Baseline::Top).draw(display).ok();

        // Second divider + status
        Line::new(Point::new(0, 100), Point::new(200, 100))
            .into_styled(stroke)
            .draw(display).ok();
        Text::with_baseline("STATUS  RUNNING", Point::new(4, 106), normal, Baseline::Top).draw(display).ok();
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
