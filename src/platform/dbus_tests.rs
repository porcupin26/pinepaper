//! Test the real wire protocol on a private bus, never the user's system bus.
use super::*;
use std::{
    io::{BufRead, BufReader},
    process::Child,
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicU8, AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};
use zbus::{connection::Builder, interface, Connection, OwnedObjectPath};

const NM: &str = "org.freedesktop.NetworkManager";
const WIFI: &str = "/org/freedesktop/NetworkManager/Devices/1";
const ETHERNET: &str = "/org/freedesktop/NetworkManager/Devices/2";
const AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";

#[derive(Default)]
struct State {
    wireless: AtomicBool,
    connected: AtomicBool,
    strength: AtomicU8,
    driver: AtomicU8,
    dither: AtomicU8,
    delay: AtomicU16,
    pending_delay: AtomicU16,
    reject_dither: AtomicBool,
    refreshes: AtomicUsize,
}
struct NetworkManager(Arc<State>);
#[interface(name = "org.freedesktop.NetworkManager")]
impl NetworkManager {
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.0.wireless.load(SeqCst)
    }
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        [ETHERNET, WIFI]
            .map(|p| OwnedObjectPath::try_from(p).unwrap())
            .into()
    }
}
struct Device(u32);
#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.0
    }
}
struct Wireless(Arc<State>);
#[interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(if self.0.connected.load(SeqCst) {
            AP
        } else {
            "/"
        })
        .unwrap()
    }
}
struct AccessPoint(Arc<State>);
#[interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        "PineNote café".as_bytes().to_vec()
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.0.strength.load(SeqCst)
    }
}
struct Display(Arc<State>);
#[interface(name = "org.pinenote.Ebc1")]
impl Display {
    #[zbus(property)]
    fn driver_mode(&self) -> u8 {
        self.0.driver.load(SeqCst)
    }
    #[zbus(property)]
    fn set_driver_mode(&self, value: u8) {
        self.0.driver.store(value, SeqCst);
    }
    #[zbus(property)]
    fn dither_mode(&self) -> u8 {
        self.0.dither.load(SeqCst)
    }
    #[zbus(property)]
    fn set_dither_mode(&self, value: u8) -> zbus::fdo::Result<()> {
        if self.0.reject_dither.swap(false, SeqCst) {
            return Err(zbus::fdo::Error::AccessDenied("Test denial".into()));
        }
        self.0.dither.store(value, SeqCst);
        Ok(())
    }
    #[zbus(property)]
    fn redraw_delay(&self) -> u16 {
        self.0.delay.load(SeqCst)
    }
    #[zbus(property)]
    fn set_redraw_delay(&self, value: u16) {
        // PineNoteCtl applies this setting when a full refresh occurs.
        self.0.pending_delay.store(value, SeqCst);
    }
    fn global_refresh(&self) {
        self.0.refreshes.fetch_add(1, SeqCst);
        let state = self.0.clone();
        let old = state.driver.swap(4, SeqCst);
        thread::spawn(move || {
            // No PropertiesChanged signal: readback must obtain fresh values.
            thread::sleep(Duration::from_millis(250));
            state.delay.store(state.pending_delay.load(SeqCst), SeqCst);
            state.driver.store(old, SeqCst);
        });
    }
}

fn services(state: &Arc<State>) -> Connection {
    zbus::block_on(async {
        Builder::session()
            .name(NM)
            .name(BUS)
            .serve_at(
                "/org/freedesktop/NetworkManager",
                NetworkManager(state.clone()),
            )
            .serve_at(ETHERNET, Device(1))
            .serve_at(WIFI, Device(2))
            .serve_at(WIFI, Wireless(state.clone()))
            .serve_at(AP, AccessPoint(state.clone()))
            .serve_at(OBJECT, Display(state.clone()))
            .build()
            .await
            .unwrap()
    })
}

struct StopChild(Child);
impl Drop for StopChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn platform_protocol_on_private_bus() {
    const CHILD: &str = "PINEPAPER_TEST_PRIVATE_DBUS";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("bus.conf");
        // No service directories: even the missing-service checks must never
        // auto-activate a real NetworkManager or PineNoteCtl installation.
        fs::write(
            &config,
            r#"<busconfig>
              <type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
              <policy context="default">
                <allow user="*"/><allow own="*"/>
                <allow send_destination="*"/><allow receive_sender="*"/>
              </policy>
            </busconfig>"#,
        )
        .unwrap();
        let mut bus = StopChild(
            Command::new("dbus-daemon")
                .arg("--config-file")
                .arg(&config)
                .args(["--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .spawn()
                .expect("platform tests require dbus-daemon"),
        );
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        assert!(address.starts_with("unix:"));
        // Change the bus addresses only in a subprocess, so parallel tests and
        // the developer's real hardware cannot see these mock services.
        let mut test = StopChild(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "platform::dbus_tests::platform_protocol_on_private_bus",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
                .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim())
                .env_remove("SWAYSOCK")
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = test.0.try_wait().unwrap() {
                assert!(status.success(), "private-bus assertions failed: {status}");
                return;
            }
            assert!(Instant::now() < deadline, "private-bus test hung");
            thread::sleep(Duration::from_millis(20));
        }
    }

    // Absent services must remain an ordinary unavailable status/error.
    assert!(wifi_status().is_err());
    assert_eq!(Status::read().wifi, "Unavailable");
    assert!(property("DriverMode").is_err());
    assert!(refresh().is_err());

    let state = Arc::new(State::default());
    let service = services(&state);
    assert_eq!(wifi_status().unwrap(), ("Off".into(), String::new(), None));
    state.wireless.store(true, SeqCst);
    assert_eq!(wifi_status().unwrap().0, "Disconnected");
    state.connected.store(true, SeqCst);
    state.strength.store(73, SeqCst);
    assert_eq!(
        wifi_status().unwrap(),
        ("Connected".into(), "PineNote café".into(), Some(73))
    );
    // Every poll must see changes even without change notifications.
    state.strength.store(41, SeqCst);
    assert_eq!(wifi_status().unwrap().2, Some(41));
    let status = status_worker()
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    assert_eq!(status.signal, Some(41));
    assert_eq!(status.ssid, "PineNote café");

    set_property("DriverMode", 1).unwrap();
    set_property("DitherMode", 2).unwrap();
    set_property("RedrawDelay", 300).unwrap(); // u16, not a truncated u8
    assert_eq!(property("DriverMode").unwrap(), 1);
    assert_eq!(property("DitherMode").unwrap(), 2);
    assert_eq!(property("RedrawDelay").unwrap(), 300);
    assert_eq!(state.refreshes.load(SeqCst), 1);
    refresh().unwrap();
    assert_eq!(state.driver.load(SeqCst), 1);

    // A partial display update must roll back, including the refreshed delay.
    let before = Hardware::read().display_only();
    let mut requested = before.clone();
    requested.delay = Some(400);
    requested.dither = Some(1);
    state.reject_dither.store(true, SeqCst);
    let error = requested.apply(&before).unwrap_err().to_string();
    assert!(error.contains("Test denial"), "{error}");
    assert!(
        error.contains("Previous hardware values restored"),
        "{error}"
    );
    assert!(Hardware::read().matches_requested(&before));

    // A service disappearing and coming back must not poison later calls.
    zbus::block_on(service.close()).unwrap();
    assert!(wifi_status().is_err());
    assert!(set_property("DriverMode", 0).is_err());
    let service = services(&state);
    assert_eq!(wifi_status().unwrap().2, Some(41));
    assert_eq!(property("DriverMode").unwrap(), 1);
    zbus::block_on(service.close()).unwrap();
}
