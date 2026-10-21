//! Optional manual PineNote check. Run as the desktop user with the app open.
//! --exercise temporarily changes the lights/driver settings and restores them.
use anyhow::{ensure, Result};
use pinepaper::platform::{self, Hardware, Status};

fn main() -> Result<()> {
    let status = Status::read();
    println!("Status: {}", status.brief());
    let before = Hardware::read();
    println!("Hardware: {before:?}");
    if std::env::args().any(|a| a == "--exercise") {
        ensure!(
            before.cool.is_some() && before.warm.is_some() && before.driver.is_some(),
            "PineNote hardware unavailable"
        );
        let mut test = before.clone();
        test.lights(65., 60.);
        test.driver = Some(if before.driver == Some(1) { 0 } else { 1 });
        test.dither = Some(if before.dither == Some(1) { 2 } else { 1 });
        test.delay = before.delay.map(|v| if v == 220 { 240 } else { 220 });
        let attempt = test.apply(&before);
        println!("Requested: {test:?}\nApplied: {attempt:?}");
        // Restore even if readback/refresh failed partway through the check.
        let actual = Hardware::read();
        let restore = before.apply(&actual);
        println!("Restored: {restore:?}");
        let changed = attempt?;
        let restored = restore?;
        ensure!(
            changed.matches_requested(&test),
            "Hardware readback differs"
        );
        ensure!(
            restored.matches_requested(&before),
            "Hardware restoration differs"
        );
        platform::refresh()?;
        println!("Light, warmth, driver, dither, cleanup and refresh checks passed; original values restored.");
    }
    Ok(())
}
