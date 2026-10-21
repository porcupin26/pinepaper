//! A virtual pointer for the isolated headless compositor in wayland_smoke.py.
//! Refuses to connect to the user's desktop.
use anyhow::{ensure, Context, Result};
use std::{io::BufRead, time::Duration};
use wayland_client::{delegate_noop, protocol::wl_registry, Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::{self, ZwlrVirtualPointerManagerV1},
    zwlr_virtual_pointer_v1,
};

#[derive(Default)]
struct State(Option<ZwlrVirtualPointerManagerV1>);
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == "zwlr_virtual_pointer_manager_v1" {
                s.0 = Some(registry.bind(name, version.min(2), q, ()));
            }
        }
    }
}
delegate_noop!(State: ignore zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ignore zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1);

fn main() -> Result<()> {
    ensure!(
        std::env::var("XDG_RUNTIME_DIR")?.contains("pinepaper-wayland-"),
        "This test helper only accepts an isolated PinePaper test compositor"
    );
    let c = Connection::connect_to_env()?;
    let mut q = c.new_event_queue();
    let handle = q.handle();
    c.display().get_registry(&handle, ());
    let mut s = State::default();
    q.roundtrip(&mut s)?;
    let pointer =
        s.0.as_ref()
            .context("Virtual pointers unsupported")?
            .create_virtual_pointer(None, &handle, ());
    c.flush()?;
    std::thread::sleep(Duration::from_millis(200));
    let start = std::time::Instant::now();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<_> = line.split_whitespace().collect();
        let time = start.elapsed().as_millis() as u32;
        match fields.as_slice() {
            ["move", x, y, w, h] => {
                pointer.motion_absolute(time, x.parse()?, y.parse()?, w.parse()?, h.parse()?)
            }
            ["down"] => pointer.button(
                time,
                0x110,
                wayland_client::protocol::wl_pointer::ButtonState::Pressed,
            ),
            ["up"] => pointer.button(
                time,
                0x110,
                wayland_client::protocol::wl_pointer::ButtonState::Released,
            ),
            _ => anyhow::bail!("Invalid test command"),
        }
        pointer.frame();
        c.flush()?;
        std::thread::sleep(Duration::from_millis(8));
    }
    pointer.destroy();
    c.flush()?;
    Ok(())
}
