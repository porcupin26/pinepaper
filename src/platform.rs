use crate::model::Store;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
pub const APP_ID: &str = "eu.pinepaper.PinePaper.Rust";
const BUS: &str = "org.pinenote.PineNoteCtl";
const OBJECT: &str = "/org/pinenote/PineNoteCtl";
const INTERFACE: &str = "org.pinenote.Ebc1";

#[cfg(test)]
mod dbus_tests;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Status {
    pub battery: Option<u8>,
    pub charging: bool,
    pub wifi: String,
    pub ssid: String,
    pub signal: Option<u8>,
}
impl Status {
    pub fn read() -> Self {
        let mut s = Self {
            wifi: "Unavailable".into(),
            ..Self::default()
        };
        if let Ok(files) = fs::read_dir("/sys/class/power_supply") {
            for p in files.flatten() {
                let p = p.path();
                let read = |name: &str| {
                    fs::read_to_string(p.join(name))
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                };
                if read("type") == "Battery"
                    && read("scope") != "Device"
                    && !p.to_string_lossy().contains("pen")
                {
                    if let Ok(n) = read("capacity").parse::<u8>() {
                        s.battery = Some(n.min(100));
                        s.charging = matches!(read("status").as_str(), "Charging" | "Full");
                        break;
                    }
                }
            }
        }
        if let Ok((wifi, ssid, signal)) = wifi_status() {
            s.wifi = wifi;
            s.ssid = ssid;
            s.signal = signal;
        }
        s
    }
    pub fn brief(&self) -> String {
        let wifi = if self.wifi == "Connected" {
            format!("Wi-Fi {}%", self.signal.unwrap_or(0))
        } else {
            format!("Wi-Fi {}", self.wifi.to_lowercase())
        };
        let b = self
            .battery
            .map(|p| format!("{}{}%", if self.charging { "+" } else { "" }, p))
            .unwrap_or("Battery --".into());
        format!("{wifi}   {b}")
    }
}
// Keep every D-Bus exchange inside its worker's runtime scope. Nothing here
// runs on the Wayland/input thread or carries a connection across worker threads.
fn wifi_status() -> Result<(String, String, Option<u8>)> {
    use zbus::{Connection, OwnedObjectPath, Proxy};
    zbus::block_on(async {
        let connection = Connection::system().await?;
        let nm = Proxy::new(
            &connection,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
        )
        .await?;
        if !nm.get_property::<bool>("WirelessEnabled").await? {
            return Ok(("Off".into(), String::new(), None));
        }
        let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &()).await?;
        for path in devices {
            let dev = Proxy::new(
                &connection,
                "org.freedesktop.NetworkManager",
                path.clone(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;
            if dev.get_property::<u32>("DeviceType").await? != 2 {
                continue;
            }
            let wifi = Proxy::new(
                &connection,
                "org.freedesktop.NetworkManager",
                path,
                "org.freedesktop.NetworkManager.Device.Wireless",
            )
            .await?;
            let ap: OwnedObjectPath = wifi.get_property("ActiveAccessPoint").await?;
            if ap.as_str() == "/" {
                continue;
            }
            let ap = Proxy::new(
                &connection,
                "org.freedesktop.NetworkManager",
                ap,
                "org.freedesktop.NetworkManager.AccessPoint",
            )
            .await?;
            let ssid: Vec<u8> = ap.get_property("Ssid").await?;
            let signal: u8 = ap.get_property("Strength").await?;
            return Ok((
                "Connected".into(),
                String::from_utf8_lossy(&ssid).into_owned(),
                Some(signal),
            ));
        }
        Ok(("Disconnected".into(), String::new(), None))
    })
}
pub fn status_worker() -> mpsc::Receiver<Status> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || loop {
        if tx.send(Status::read()).is_err() {
            break;
        }
        thread::sleep(Duration::from_secs(15));
    });
    rx
}

// All commands here are platform utilities, use fixed argument vectors, and have a deadline.
pub fn command(cmd: &mut Command) -> Result<String> {
    use std::io::Read;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let reader = thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        out
    });
    let errors = thread::spawn(move || {
        let mut out = String::new();
        let _ = stderr.read_to_string(&mut out);
        out
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(7) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("System service timed out");
        }
        thread::sleep(Duration::from_millis(20));
    };
    let out = reader.join().unwrap_or_default();
    let error = errors.join().unwrap_or_default();
    if !status.success() {
        bail!("{}", error.trim());
    }
    Ok(out)
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    pub page_contrast: f64,
    pub page_gamma: f64,
    pub ui_font_size: f64,
    pub keyboard_layout: String,
    pub keyboard_key_height: f64,
    pub keyboard_auto: bool,
    pub hide_cursor: bool,
    pub finger_draw: bool,
    pub refresh_pages: usize,
    pub start_fullscreen: bool,
    pub display_hint: String,
    pub exchange_url: String,
}
impl Default for Prefs {
    fn default() -> Self {
        Self {
            page_contrast: 1.,
            page_gamma: 1.,
            ui_font_size: 14.,
            keyboard_layout: "QWERTY".into(),
            keyboard_key_height: 38.,
            keyboard_auto: true,
            hide_cursor: true,
            finger_draw: false,
            refresh_pages: 0,
            start_fullscreen: true,
            display_hint: "Y1|T|R".into(),
            exchange_url: String::new(),
        }
    }
}
impl Prefs {
    pub fn load(store: &Store) -> Self {
        let defaults = serde_json::to_value(Self::default()).unwrap();
        let mut map = serde_json::Map::new();
        for (k, v) in defaults.as_object().unwrap() {
            map.insert(k.clone(), store.setting(k, v.clone()));
        }
        serde_json::from_value(Value::Object(map)).unwrap_or_default()
    }
    pub fn save(&self, store: &Store) -> Result<()> {
        let transaction = store.db.unchecked_transaction()?;
        for (k, v) in serde_json::to_value(self)?.as_object().unwrap() {
            store.set(k, v)?;
        }
        transaction.commit()?;
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hardware {
    pub cool: Option<u32>,
    pub warm: Option<u32>,
    pub cool_max: u32,
    pub warm_max: u32,
    pub driver: Option<u32>,
    pub dither: Option<u32>,
    pub delay: Option<u32>,
    pub hint: Option<String>,
}
impl Hardware {
    pub fn read() -> Self {
        Self {
            driver: property("DriverMode").ok(),
            dither: property("DitherMode").ok(),
            delay: property("RedrawDelay").ok(),
            hint: window_hint().ok().flatten(),
            ..Self::read_lights()
        }
    }
    /// Read only the front light; live adjustments do not need display IPC.
    pub fn read_lights() -> Self {
        let read = |color: &str, name: &str| {
            if name == "brightness"
                && fs::OpenOptions::new()
                    .write(true)
                    .open(format!("/sys/class/backlight/backlight_{color}/brightness"))
                    .is_err()
            {
                return None;
            }
            fs::read_to_string(format!("/sys/class/backlight/backlight_{color}/{name}"))
                .ok()?
                .trim()
                .parse::<u32>()
                .ok()
        };
        Self {
            cool: read("cool", "brightness"),
            warm: read("warm", "brightness"),
            cool_max: read("cool", "max_brightness").unwrap_or(255),
            warm_max: read("warm", "max_brightness").unwrap_or(255),
            ..Self::default()
        }
    }
    pub fn apply_lights(brightness: f64, warmth: f64) -> Result<Self> {
        let before = Self::read_lights();
        if before.cool.is_none() || before.warm.is_none() {
            bail!("Front-light controls are unavailable");
        }
        let mut requested = before.clone();
        requested.lights(brightness, warmth);
        requested.apply(&before)
    }
    pub fn display_only(&self) -> Self {
        Self {
            driver: self.driver,
            dither: self.dither,
            delay: self.delay,
            hint: self.hint.clone(),
            ..Self::default()
        }
    }
    pub fn copy_display_from(&mut self, other: &Self) {
        self.driver = other.driver;
        self.dither = other.dither;
        self.delay = other.delay;
        self.hint = other.hint.clone();
    }
    pub fn apply_display(&self) -> Result<Self> {
        // A refresh adjustment must never overwrite a newer light adjustment.
        let before = Self::read().display_only();
        self.display_only().apply(&before).map(|h| h.display_only())
    }
    pub fn light_controls(&self) -> (f64, f64) {
        let c = self.cool.unwrap_or(0) as f64 / self.cool_max.max(1) as f64;
        let w = self.warm.unwrap_or(0) as f64 / self.warm_max.max(1) as f64;
        (
            c.max(w) * 100.,
            if c + w > 0. { w / (c + w) * 100. } else { 50. },
        )
    }
    pub fn lights(&mut self, brightness: f64, warmth: f64) {
        let (c, w) = light_levels(brightness, warmth);
        self.cool = Some((c * self.cool_max as f64).round() as u32);
        self.warm = Some((w * self.warm_max as f64).round() as u32);
    }
    pub fn apply(&self, before: &Self) -> Result<Self> {
        let attempt = || -> Result<Self> {
            if self.cool != before.cool || self.warm != before.warm {
                write_lights(self)?;
            }
            for (k, new, old) in [
                ("RedrawDelay", self.delay, before.delay),
                ("DitherMode", self.dither, before.dither),
                ("DriverMode", self.driver, before.driver),
            ] {
                if let (Some(v), Some(o)) = (new, old) {
                    if v != o {
                        set_property(k, v)?;
                    }
                }
            }
            if self.hint != before.hint {
                if let Some(h) = &self.hint {
                    set_hint(h)?;
                }
            }
            self.wait_settled()
        };
        let result = attempt();
        if let Err(e) = result {
            let mut failures = vec![];
            if self.cool != before.cool || self.warm != before.warm {
                if let Err(e) = write_lights(before) {
                    failures.push(e.to_string());
                }
            }
            for (k, new, old) in [
                ("RedrawDelay", self.delay, before.delay),
                ("DitherMode", self.dither, before.dither),
                ("DriverMode", self.driver, before.driver),
            ] {
                if new != old {
                    if let Some(v) = old {
                        if let Err(e) = set_property(k, v) {
                            failures.push(e.to_string());
                        }
                    }
                }
            }
            if self.hint != before.hint {
                if let Some(h) = &before.hint {
                    if let Err(e) = set_hint(h) {
                        failures.push(e.to_string());
                    }
                }
            }
            if let Err(e) = before.wait_settled() {
                failures.push(e.to_string());
            }
            bail!(
                "{e}. {}",
                if failures.is_empty() {
                    "Previous hardware values restored".into()
                } else {
                    format!("Rollback errors: {}", failures.join("; "))
                }
            );
        }
        result
    }
    /// The paired front-light driver can round either channel by one level
    /// after the other channel changes. All other requested values are exact.
    pub fn matches_requested(&self, requested: &Self) -> bool {
        let light = |actual: Option<u32>, target: Option<u32>| {
            target.is_none_or(|t| actual.is_some_and(|a| a.abs_diff(t) <= 1))
        };
        light(self.cool, requested.cool)
            && light(self.warm, requested.warm)
            && requested.driver.is_none_or(|v| self.driver == Some(v))
            && requested.dither.is_none_or(|v| self.dither == Some(v))
            && requested.delay.is_none_or(|v| self.delay == Some(v))
            && requested
                .hint
                .as_ref()
                .is_none_or(|v| self.hint.as_ref() == Some(v))
    }
    fn wait_settled(&self) -> Result<Self> {
        let mut matches = 0;
        for _ in 0..60 {
            thread::sleep(Duration::from_millis(100));
            let actual = if self.driver.is_none()
                && self.dither.is_none()
                && self.delay.is_none()
                && self.hint.is_none()
            {
                Self::read_lights()
            } else {
                Self::read()
            };
            matches = if actual.matches_requested(self) {
                matches + 1
            } else {
                0
            };
            if matches >= 2 {
                return Ok(actual);
            }
        }
        bail!("The display did not settle at the requested settings")
    }
}
pub fn light_levels(brightness: f64, warmth: f64) -> (f64, f64) {
    let b = brightness.clamp(0., 100.) / 100.;
    let w = warmth.clamp(0., 100.) / 100.;
    let n = w.max(1. - w);
    (b * (1. - w) / n, b * w / n)
}
fn write_lights(h: &Hardware) -> Result<()> {
    for (color, value, max) in [("cool", h.cool, h.cool_max), ("warm", h.warm, h.warm_max)] {
        if let Some(v) = value {
            if v > max {
                bail!("Light value exceeds hardware maximum");
            }
            fs::write(
                format!("/sys/class/backlight/backlight_{color}/brightness"),
                v.to_string(),
            )?;
            let actual =
                fs::read_to_string(format!("/sys/class/backlight/backlight_{color}/brightness"))?;
            if actual.trim().parse::<u32>()?.abs_diff(v) > 1 {
                bail!("Front-light readback mismatch");
            }
        }
    }
    Ok(())
}
fn property(key: &str) -> Result<u32> {
    zbus::block_on(async {
        let c = zbus::Connection::session().await?;
        let p = zbus::Proxy::new(&c, BUS, OBJECT, INTERFACE).await?;
        if key == "RedrawDelay" {
            Ok(p.get_property::<u16>(key).await? as u32)
        } else {
            Ok(p.get_property::<u8>(key).await? as u32)
        }
    })
}
fn set_property(key: &str, value: u32) -> Result<()> {
    zbus::block_on(async {
        let c = zbus::Connection::session().await?;
        let p = zbus::Proxy::new(&c, BUS, OBJECT, INTERFACE).await?;
        if key == "RedrawDelay" {
            p.set_property(key, value as u16).await?;
        } else {
            p.set_property(key, value as u8).await?;
        }
        Ok::<_, anyhow::Error>(())
    })?;
    if key == "RedrawDelay" {
        refresh()?;
    }
    for _ in 0..50 {
        if property(key)? == value {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("{key} readback mismatch")
}
pub fn refresh() -> Result<()> {
    let old = property("DriverMode")?;
    zbus::block_on(async {
        let c = zbus::Connection::session().await?;
        let p = zbus::Proxy::new(&c, BUS, OBJECT, INTERFACE).await?;
        p.call::<_, _, ()>("GlobalRefresh", &()).await
    })?;
    thread::sleep(Duration::from_millis(150));
    for _ in 0..50 {
        if property("DriverMode")? == old {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("Refresh did not return to selected speed")
}
fn sway(args: &[&str]) -> Result<Value> {
    use std::io::{Read, Write};
    let socket = std::env::var_os("SWAYSOCK").context("Sway is unavailable")?;
    let mut stream = std::os::unix::net::UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let (kind, payload) = if args == ["-t", "get_tree"] {
        (4u32, "")
    } else {
        (0u32, args[0])
    };
    let mut head = b"i3-ipc".to_vec();
    head.extend((payload.len() as u32).to_le_bytes());
    head.extend(kind.to_le_bytes());
    stream.write_all(&head)?;
    stream.write_all(payload.as_bytes())?;
    let mut header = [0u8; 14];
    stream.read_exact(&mut header)?;
    if &header[..6] != b"i3-ipc" {
        bail!("Invalid Sway IPC response");
    }
    let size = u32::from_le_bytes(header[6..10].try_into()?) as usize;
    if size > 16 * 1024 * 1024 {
        bail!("Sway response too large");
    }
    let mut body = vec![0; size];
    stream.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}
pub fn activate(fullscreen: bool) -> Result<()> {
    let command = format!(
        "[app_id=\"{}\"] focus{}",
        APP_ID,
        if fullscreen {
            ", fullscreen enable"
        } else {
            ""
        }
    );
    sway(&[&command])?;
    Ok(())
}
fn windows<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    if node["app_id"] == APP_ID {
        out.push(node);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node[key].as_array() {
            for c in children {
                windows(c, out);
            }
        }
    }
}
pub fn window_hint() -> Result<Option<String>> {
    let tree = sway(&["-t", "get_tree"])?;
    let mut nodes = vec![];
    windows(&tree, &mut nodes);
    Ok(nodes
        .into_iter()
        .flat_map(|n| n["marks"].as_array().into_iter().flatten())
        .filter_map(|m| m.as_str())
        .find(|m| m.starts_with("_ebchint:") || m.starts_with("ebchint:"))
        .and_then(|m| m.splitn(3, ':').nth(2))
        .map(str::to_owned))
}
pub fn set_hint(hint: &str) -> Result<()> {
    let p: Vec<_> = hint.split('|').collect();
    if p.len() != 3
        || !["Y1", "Y2", "Y4"].contains(&p[0])
        || !["T", "D"].contains(&p[1])
        || !["R", "r"].contains(&p[2])
    {
        bail!("Invalid display hint");
    }
    let tree = sway(&["-t", "get_tree"])?;
    let mut nodes = vec![];
    windows(&tree, &mut nodes);
    if nodes.is_empty() {
        bail!("PinePaper window is not available to Sway");
    }
    for n in nodes {
        let id = n["id"].as_u64().context("Invalid window id")?;
        if let Some(marks) = n["marks"].as_array() {
            for mark in marks
                .iter()
                .filter_map(|m| m.as_str())
                .filter(|m| m.starts_with("_ebchint:") || m.starts_with("ebchint:"))
            {
                sway(&[&format!(
                    "[con_id={id}] unmark {}",
                    serde_json::to_string(mark)?
                )])?;
            }
        }
        let reply = sway(&[&format!(
            "[con_id={id}] mark --add \"_ebchint:pinepaper:{hint}\""
        )])?;
        if !reply
            .as_array()
            .is_some_and(|a| a.iter().all(|r| r["success"] == true))
        {
            bail!("Sway rejected the display hint");
        }
    }
    if window_hint()?.as_deref() != Some(hint) {
        bail!("Display hint readback mismatch");
    }
    Ok(())
}
pub fn clipboard_copy(text: &str) -> Result<()> {
    use std::io::Write;
    let mut child = Command::new("wl-copy")
        .arg("--type")
        .arg("text/plain;charset=utf-8")
        .stdin(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    if !child.wait()?.success() {
        bail!("Clipboard copy failed");
    }
    Ok(())
}
pub fn clipboard_paste() -> Result<String> {
    command(Command::new("wl-paste").arg("--no-newline"))
}
pub fn expand_path(input: &str) -> std::path::PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        Path::new(&std::env::var("HOME").unwrap_or_default()).join(rest)
    } else {
        input.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_preference_save_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let before = Prefs::default();
        before.save(&store).unwrap();
        store.db.execute_batch("CREATE TRIGGER fail_preference BEFORE INSERT ON settings WHEN NEW.key = 'keyboard_layout' BEGIN SELECT RAISE(ABORT, 'simulated write failure'); END;").unwrap();
        let mut changed = before.clone();
        changed.display_hint = "Y4|D|R".into();
        changed.keyboard_auto = false;
        changed.keyboard_layout = "AZERTY".into();
        assert!(changed.save(&store).is_err());
        assert_eq!(Prefs::load(&store), before);
    }
    #[test]
    fn lights() {
        assert_eq!(light_levels(100., 50.), (1., 1.));
        assert_eq!(light_levels(0., 80.), (0., 0.));
        assert_eq!(light_levels(100., 0.), (1., 0.));
    }
    #[test]
    fn hardware_readback_accepts_only_single_level_light_rounding() {
        let requested = Hardware {
            cool: Some(111),
            warm: Some(166),
            driver: Some(1),
            dither: Some(2),
            delay: Some(220),
            ..Hardware::default()
        };
        let mut actual = requested.clone();
        actual.cool = Some(112);
        assert!(actual.matches_requested(&requested));
        actual.cool = Some(113);
        assert!(!actual.matches_requested(&requested));
        actual.cool = None;
        assert!(!actual.matches_requested(&requested));
        actual.cool = requested.cool;
        actual.driver = Some(0);
        assert!(!actual.matches_requested(&requested));
        actual.driver = requested.driver;
        actual.delay = Some(219);
        assert!(!actual.matches_requested(&requested));
    }
}
