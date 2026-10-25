//! Optional manual PineNote check. Run as the desktop user with the app open.
//! --exercise temporarily changes the lights/driver settings and restores them.
//! --benchmark times repeated read-only polls and reports worker resources.
use anyhow::{ensure, Result};
use pinepaper::platform::{self, Hardware, Status};
use std::{fs, hint::black_box, time::Instant};

fn main() -> Result<()> {
    let status = Status::read();
    println!("Status: {}", status.brief());
    let before = Hardware::read();
    println!("Hardware: {before:?}");
    if std::env::args().any(|a| a == "--benchmark") {
        let resources = || {
            let count = |path| fs::read_dir(path).map(|files| files.count()).unwrap_or(0);
            (count("/proc/self/task"), count("/proc/self/fd"))
        };
        let initial = resources();
        let mut status_ms = Vec::new();
        let mut hardware_ms = Vec::new();
        for _ in 0..40 {
            let start = Instant::now();
            black_box(Status::read());
            status_ms.push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            black_box(Hardware::read());
            hardware_ms.push(start.elapsed().as_secs_f64() * 1000.);
        }
        for (name, mut samples) in [("status", status_ms), ("hardware", hardware_ms)] {
            samples.sort_by(f64::total_cmp);
            println!(
                "{name}: median={:.2}ms p95={:.2}ms",
                samples[20], samples[38]
            );
        }
        // Allow asynchronous connection teardown to finish before counting.
        std::thread::sleep(std::time::Duration::from_millis(200));
        println!("Threads/fds: before={initial:?} after={:?}", resources());
    }
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
