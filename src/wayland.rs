use crate::{
    app::{Action, App},
    platform::APP_ID,
    render::Rect,
};
use anyhow::Result;
use memmap2::MmapMut;
use std::io::Read;
use std::{collections::HashMap, fs::File, os::fd::AsFd, time::Instant};
use wayland_client::{
    backend::ObjectId,
    delegate_noop, event_created_child,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface, wl_touch,
    },
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use wayland_protocols::{
    wp::tablet::zv2::client::{
        zwp_tablet_manager_v2, zwp_tablet_pad_v2, zwp_tablet_seat_v2, zwp_tablet_tool_v2,
        zwp_tablet_v2,
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

#[derive(Clone, Copy, Default)]
struct Point {
    x: f64,
    y: f64,
}
struct Buffer {
    id: usize,
    object: wl_buffer::WlBuffer,
    map: MmapMut,
    width: usize,
    height: usize,
    busy: bool,
    dirty: Option<Rect>,
}
impl Drop for Buffer {
    fn drop(&mut self) {
        self.object.destroy();
    }
}

#[derive(Clone, Copy)]
struct Pen {
    p: Point,
    inside: bool,
    down: bool,
    start: bool,
    end: bool,
    moved: bool,
    pressure: f64,
    serial: u32,
    cursor_shown: bool,
}
impl Default for Pen {
    fn default() -> Self {
        Self {
            p: Point { x: 0.0, y: 0.0 },
            inside: false,
            down: false,
            start: false,
            end: false,
            moved: false,
            pressure: 0.5,
            serial: 0,
            cursor_shown: false,
        }
    }
}

struct State {
    app: App,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm: Option<xdg_wm_base::XdgWmBase>,
    seat: Option<wl_seat::WlSeat>,
    tablet_manager: Option<zwp_tablet_manager_v2::ZwpTabletManagerV2>,
    surface: Option<wl_surface::WlSurface>,
    shell: Option<xdg_surface::XdgSurface>,
    toplevel: Option<xdg_toplevel::XdgToplevel>,
    configured: bool,
    width: usize,
    height: usize,
    scale: i32,
    resized: bool,
    outputs: HashMap<ObjectId, i32>,
    active_outputs: Vec<ObjectId>,
    buffers: Vec<Buffer>,
    next_buffer: usize,
    dirty: Option<Rect>,
    pens: HashMap<ObjectId, Pen>,
    keyboard_device: Option<wl_keyboard::WlKeyboard>,
    pointer_device: Option<wl_pointer::WlPointer>,
    touch_device: Option<wl_touch::WlTouch>,
    cursor: Option<(wl_surface::WlSurface, wl_buffer::WlBuffer)>,
    tablet_cursors: HashMap<ObjectId, (wl_surface::WlSurface, wl_buffer::WlBuffer)>,
    pointer: Point,
    pointer_down: bool,
    touch: Option<i32>,
    touch_position: Point,
    xkb: Keyboard,
    commits: usize,
    commit_max_us: u128,
}
impl State {
    fn new(app: App) -> Self {
        Self {
            app,
            compositor: None,
            shm: None,
            wm: None,
            seat: None,
            tablet_manager: None,
            surface: None,
            shell: None,
            toplevel: None,
            configured: false,
            width: 936,
            height: 702,
            scale: 1,
            resized: true,
            outputs: HashMap::new(),
            active_outputs: vec![],
            buffers: vec![],
            next_buffer: 0,
            dirty: None,
            pens: HashMap::new(),
            keyboard_device: None,
            pointer_device: None,
            touch_device: None,
            cursor: None,
            tablet_cursors: HashMap::new(),
            pointer: Point::default(),
            pointer_down: false,
            touch: None,
            touch_position: Point::default(),
            xkb: Keyboard::default(),
            commits: 0,
            commit_max_us: 0,
        }
    }
    fn initialize(&mut self, qh: &QueueHandle<Self>) -> Result<(), anyhow::Error> {
        let compositor = self
            .compositor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wayland compositor unavailable"))?;
        let wm = self
            .wm
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("XDG shell unavailable"))?;
        self.shm
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wayland shared-memory buffers unavailable"))?;
        let surface = compositor.create_surface(qh, ());
        let shell = wm.get_xdg_surface(&surface, qh, ());
        let toplevel = shell.get_toplevel(qh, ());
        toplevel.set_app_id(APP_ID.into());
        toplevel.set_title("PinePaper".into());
        toplevel.set_min_size(600, 500);
        if self.app.fullscreen {
            toplevel.set_fullscreen(None);
        }
        self.surface = Some(surface.clone());
        self.shell = Some(shell);
        self.toplevel = Some(toplevel);
        if let (Some(manager), Some(seat)) = (&self.tablet_manager, &self.seat) {
            manager.get_tablet_seat(seat, qh, ());
        }
        self.cursor = Some(self.make_cursor(qh)?);
        surface.commit();
        Ok(())
    }
    fn make_cursor(
        &self,
        qh: &QueueHandle<Self>,
    ) -> Result<(wl_surface::WlSurface, wl_buffer::WlBuffer)> {
        let file = File::from(rustix::fs::memfd_create(
            "pinepaper-cursor",
            rustix::fs::MemfdFlags::CLOEXEC,
        )?);
        file.set_len(32 * 32 * 4)?;
        let mut map = unsafe { MmapMut::map_mut(&file)? };
        for y in 0..32usize {
            for x in 0..32usize {
                let distance = (x as f64 - 15.5).hypot(y as f64 - 15.5);
                let c: u32 = if (5.0..7.0).contains(&distance) {
                    0xff101010
                } else if (4.0..8.0).contains(&distance) {
                    0xffffffff
                } else {
                    0
                };
                let i = (y * 32 + x) * 4;
                map[i..i + 4].copy_from_slice(&c.to_ne_bytes());
            }
        }
        let pool = self
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), 4096, qh, ());
        let buffer = pool.create_buffer(0, 32, 32, 128, wl_shm::Format::Argb8888, qh, usize::MAX);
        pool.destroy();
        let surface = self.compositor.as_ref().unwrap().create_surface(qh, ());
        surface.set_buffer_scale(2);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        Ok((surface, buffer))
    }
    fn damage(&mut self, rect: Rect) {
        if rect.x >= rect.right || rect.y >= rect.bottom {
            return;
        }
        self.dirty = Some(self.dirty.map_or(rect, |r| r.union(rect)));
        for b in &mut self.buffers {
            if b.width == self.app.surface.width && b.height == self.app.surface.height {
                b.dirty = Some(b.dirty.map_or(rect, |r| r.union(rect)));
            }
        }
    }
    fn pen_frame(&mut self, p: Pen) {
        if p.inside {
            self.app.pen_near();
        }
        if p.start && p.inside {
            self.app.down(p.p.x, p.p.y, p.pressure, false);
        } else if (p.down || p.end) && p.moved {
            self.app.queue_motion(p.p.x, p.p.y, p.pressure);
        }
        if p.end {
            self.app.up(p.p.x, p.p.y);
        }
    }
    fn buffer(&mut self, qh: &QueueHandle<Self>) -> Result<Buffer, anyhow::Error> {
        let len = self
            .app
            .surface
            .width
            .checked_mul(self.app.surface.height)
            .and_then(|v| v.checked_mul(4))
            .ok_or_else(|| anyhow::anyhow!("Buffer overflow"))?;
        if len > 256 * 1024 * 1024 {
            anyhow::bail!("Display is too large");
        }
        let fd = rustix::fs::memfd_create("rust-pen-buffer", rustix::fs::MemfdFlags::CLOEXEC)?;
        let file = File::from(fd);
        file.set_len(len as u64)?;
        // Only this client writes its mappings; busy buffers are never modified.
        // The compositor receives its own fd and may read until wl_buffer.release.
        let map = unsafe { MmapMut::map_mut(&file)? };
        let pool = self
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), len as i32, qh, ());
        let id = self.next_buffer;
        self.next_buffer += 1;
        let object = pool.create_buffer(
            0,
            self.app.surface.width as i32,
            self.app.surface.height as i32,
            self.app.surface.width as i32 * 4,
            wl_shm::Format::Xrgb8888,
            qh,
            id,
        );
        pool.destroy();
        Ok(Buffer {
            id,
            object,
            map,
            width: self.app.surface.width,
            height: self.app.surface.height,
            busy: false,
            dirty: Some(self.app.surface.full_rect()),
        })
    }
    fn present(&mut self, qh: &QueueHandle<Self>) -> Result<(), anyhow::Error> {
        if !self.configured {
            return Ok(());
        }
        if self.app.fullscreen_changed {
            if let Some(t) = &self.toplevel {
                if self.app.fullscreen {
                    t.set_fullscreen(None);
                } else {
                    t.unset_fullscreen();
                }
            }
            self.app.fullscreen_changed = false;
        }
        if self.resized {
            self.app.resize(self.width, self.height, self.scale);
            self.resized = false;
            let mut fresh = Vec::new();
            for _ in 0..3 {
                fresh.push(self.buffer(qh)?);
            }
            self.buffers.extend(fresh);
        }
        self.app.tick();
        self.app.render();
        if let Some(r) = self.app.damage.take() {
            self.damage(r);
        }
        self.buffers.retain(|b| {
            b.busy || (b.width == self.app.surface.width && b.height == self.app.surface.height)
        });
        let Some(dirty) = self.dirty else {
            return Ok(());
        };
        let Some(b) = self.buffers.iter_mut().find(|b| {
            !b.busy && b.width == self.app.surface.width && b.height == self.app.surface.height
        }) else {
            return Ok(());
        };
        let started = Instant::now();
        if let Some(copy) = b.dirty.take() {
            // u32 has no padding and this uses native endian, as wl_shm requires.
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    self.app.surface.pixels.as_ptr().cast::<u8>(),
                    self.app.surface.pixels.len() * 4,
                )
            };
            for y in copy.y..copy.bottom {
                let start = (y * self.app.surface.width + copy.x) * 4;
                let end = (y * self.app.surface.width + copy.right) * 4;
                b.map[start..end].copy_from_slice(&bytes[start..end]);
            }
        }
        let surface = self.surface.as_ref().unwrap();
        surface.set_buffer_scale(self.scale);
        surface.attach(Some(&b.object), 0, 0);
        surface.damage_buffer(
            dirty.x as i32,
            dirty.y as i32,
            (dirty.right - dirty.x) as i32,
            (dirty.bottom - dirty.y) as i32,
        );
        surface.commit();
        b.busy = true;
        self.dirty = None;
        self.commits += 1;
        self.commit_max_us = self.commit_max_us.max(started.elapsed().as_micros());
        Ok(())
    }
    fn output_scale(&mut self) {
        let scale = self
            .active_outputs
            .iter()
            .filter_map(|o| self.outputs.get(o))
            .copied()
            .max()
            .unwrap_or(1);
        if scale != self.scale {
            self.scale = scale;
            self.resized = true;
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = e
        {
            match interface.as_str() {
                "wl_compositor" if version >= 4 => s.compositor = Some(r.bind(name, 4, q, ())),
                "wl_shm" => s.shm = Some(r.bind(name, 1, q, ())),
                "xdg_wm_base" => s.wm = Some(r.bind(name, 1, q, ())),
                "wl_seat" => s.seat = Some(r.bind(name, version.min(7), q, ())),
                "wl_output" => {
                    let output = r.bind::<wl_output::WlOutput, _, _>(name, version.min(3), q, ());
                    s.outputs.insert(output.id(), 1);
                }
                "zwp_tablet_manager_v2" => s.tablet_manager = Some(r.bind(name, 1, q, ())),
                _ => (),
            }
        }
    }
}
impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        s: &mut Self,
        _: &wl_surface::WlSurface,
        e: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            wl_surface::Event::Enter { output } => {
                if !s.active_outputs.contains(&output.id()) {
                    s.active_outputs.push(output.id());
                }
                s.output_scale();
            }
            wl_surface::Event::Leave { output } => {
                s.active_outputs.retain(|o| *o != output.id());
                s.output_scale();
            }
            _ => (),
        }
    }
}
impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        s: &mut Self,
        o: &wl_output::WlOutput,
        e: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Scale { factor } = e {
            s.outputs.insert(o.id(), factor.max(1));
            s.output_scale();
        }
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        o: &xdg_wm_base::XdgWmBase,
        e: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = e {
            o.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        s: &mut Self,
        o: &xdg_surface::XdgSurface,
        e: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = e {
            o.ack_configure(serial);
            s.configured = true;
        }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        s: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        e: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            xdg_toplevel::Event::Configure { width, height, .. } if width > 0 && height > 0 => {
                if (s.width, s.height) != (width as usize, height as usize) {
                    s.width = width as usize;
                    s.height = height as usize;
                    s.resized = true;
                }
            }
            xdg_toplevel::Event::Close => s.app.act(Action::Quit),
            _ => (),
        }
    }
}
impl Dispatch<wl_buffer::WlBuffer, usize> for State {
    fn event(
        s: &mut Self,
        _: &wl_buffer::WlBuffer,
        e: wl_buffer::Event,
        id: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = e {
            if let Some(b) = s.buffers.iter_mut().find(|b| b.id == *id) {
                b.busy = false;
            }
        }
    }
}
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        s: &mut Self,
        o: &wl_seat::WlSeat,
        e: wl_seat::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(c),
        } = e
        {
            if c.contains(wl_seat::Capability::Keyboard) {
                if s.keyboard_device.is_none() {
                    s.keyboard_device = Some(o.get_keyboard(q, ()));
                }
            } else if let Some(device) = s.keyboard_device.take() {
                if device.version() >= 3 {
                    device.release();
                }
            }
            if c.contains(wl_seat::Capability::Pointer) {
                if s.pointer_device.is_none() {
                    s.pointer_device = Some(o.get_pointer(q, ()));
                }
            } else if let Some(device) = s.pointer_device.take() {
                if device.version() >= 3 {
                    device.release();
                }
                s.pointer_down = false;
            }
            if c.contains(wl_seat::Capability::Touch) {
                if s.touch_device.is_none() {
                    s.touch_device = Some(o.get_touch(q, ()));
                }
            } else if let Some(device) = s.touch_device.take() {
                if device.version() >= 3 {
                    device.release();
                }
                s.touch = None;
            }
        }
    }
}
impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        s: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        e: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            wl_keyboard::Event::Keymap {
                format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
                fd,
                size,
            } => {
                if let Err(e) = s.xkb.load(fd, size) {
                    eprintln!("Keyboard: {e}");
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => s
                .xkb
                .modifiers(mods_depressed, mods_latched, mods_locked, group),
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(wl_keyboard::KeyState::Pressed),
                ..
            } => {
                if let Some((name, text, ctrl, shift)) = s.xkb.key(key) {
                    s.app.key(&name, &text, ctrl, shift);
                }
            }
            _ => (),
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        s: &mut Self,
        o: &wl_pointer::WlPointer,
        e: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            wl_pointer::Event::Enter {
                serial,
                surface_x,
                surface_y,
                ..
            } => {
                o.set_cursor(serial, s.cursor.as_ref().map(|c| &c.0), 8, 8);
                s.pointer = Point {
                    x: surface_x,
                    y: surface_y,
                };
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                s.pointer = Point {
                    x: surface_x,
                    y: surface_y,
                };
                if s.pointer_down {
                    s.app.queue_motion(surface_x, surface_y, 0.5);
                }
            }
            wl_pointer::Event::Button {
                button: 272,
                state: WEnum::Value(state),
                ..
            } => {
                if state == wl_pointer::ButtonState::Pressed {
                    s.pointer_down = true;
                    s.app.down(s.pointer.x, s.pointer.y, 0.5, false);
                } else {
                    s.pointer_down = false;
                    s.app.up(s.pointer.x, s.pointer.y);
                }
            }
            wl_pointer::Event::Axis {
                axis: WEnum::Value(wl_pointer::Axis::VerticalScroll),
                value,
                ..
            } => s.app.scroll_by(value * 3.),
            wl_pointer::Event::Leave { .. } => {
                if s.pointer_down {
                    s.app.finish();
                }
                s.pointer_down = false;
            }
            _ => (),
        }
    }
}
impl Dispatch<wl_touch::WlTouch, ()> for State {
    fn event(
        s: &mut Self,
        _: &wl_touch::WlTouch,
        e: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            wl_touch::Event::Down { id, x, y, .. }
                if s.touch.is_none() && !s.pens.values().any(|p| p.inside) =>
            {
                s.touch = Some(id);
                s.touch_position = Point { x, y };
                s.app.down(x, y, 0.5, true);
            }
            wl_touch::Event::Motion { id, x, y, .. } if s.touch == Some(id) => {
                s.touch_position = Point { x, y };
                s.app.queue_motion(x, y, 0.5);
            }
            wl_touch::Event::Up { id, .. } if s.touch == Some(id) => {
                s.app.up(s.touch_position.x, s.touch_position.y);
                s.touch = None;
            }
            wl_touch::Event::Cancel => {
                s.app.finish();
                s.touch = None;
            }
            _ => (),
        }
    }
}
impl Dispatch<zwp_tablet_seat_v2::ZwpTabletSeatV2, ()> for State {
    fn event(
        s: &mut Self,
        _: &zwp_tablet_seat_v2::ZwpTabletSeatV2,
        e: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        match e {
            zwp_tablet_seat_v2::Event::ToolAdded { id } => {
                s.pens.insert(id.id(), Pen::default());
                // Surface roles are permanent. A tablet cursor cannot reuse the
                // wl_pointer cursor surface, even when the pointer is hidden.
                if let Ok(cursor) = s.make_cursor(q) {
                    s.tablet_cursors.insert(id.id(), cursor);
                }
                eprintln!("tablet tool connected");
            }
            zwp_tablet_seat_v2::Event::PadAdded { id } => id.destroy(),
            _ => (),
        }
    }
    event_created_child!(State,zwp_tablet_seat_v2::ZwpTabletSeatV2,[
        0=>(zwp_tablet_v2::ZwpTabletV2,()),1=>(zwp_tablet_tool_v2::ZwpTabletToolV2,()),2=>(zwp_tablet_pad_v2::ZwpTabletPadV2,())
    ]);
}
impl Dispatch<zwp_tablet_tool_v2::ZwpTabletToolV2, ()> for State {
    fn event(
        s: &mut Self,
        o: &zwp_tablet_tool_v2::ZwpTabletToolV2,
        e: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let pen = s.pens.entry(o.id()).or_default();
        match e {
            zwp_tablet_tool_v2::Event::ProximityIn {
                serial, surface, ..
            } => {
                pen.inside = s.surface.as_ref() == Some(&surface);
                pen.serial = serial;
                pen.cursor_shown = false;
                o.set_cursor(serial, None, 0, 0);
                if s.touch.take().is_some() {
                    s.app.finish();
                }
            }
            zwp_tablet_tool_v2::Event::ProximityOut => {
                pen.inside = false;
                if pen.down {
                    pen.end = true;
                    pen.down = false;
                }
            }
            zwp_tablet_tool_v2::Event::Down { .. } => {
                pen.down = true;
                pen.start = true;
            }
            zwp_tablet_tool_v2::Event::Up => {
                pen.down = false;
                pen.end = true;
            }
            zwp_tablet_tool_v2::Event::Motion { x, y } => {
                pen.p = Point { x, y };
                pen.moved = true;
            }
            zwp_tablet_tool_v2::Event::Pressure { pressure } => {
                pen.pressure = pressure as f64 / 65535.
            }
            zwp_tablet_tool_v2::Event::Frame { .. } => {
                let show = pen.inside
                    && (!s.app.prefs.hide_cursor
                        || s.app.dialog.is_some()
                        || s.app.screen != crate::app::Screen::Editor
                        || s.app.hit(pen.p.x, pen.p.y).is_some());
                if show != pen.cursor_shown {
                    pen.cursor_shown = show;
                    o.set_cursor(
                        pen.serial,
                        if show {
                            s.tablet_cursors.get(&o.id()).map(|c| &c.0)
                        } else {
                            None
                        },
                        8,
                        8,
                    );
                }
                let frame = *pen;
                pen.start = false;
                pen.end = false;
                pen.moved = false;
                s.pen_frame(frame);
            }
            zwp_tablet_tool_v2::Event::Removed => {
                s.pens.remove(&o.id());
                if let Some((surface, buffer)) = s.tablet_cursors.remove(&o.id()) {
                    surface.destroy();
                    buffer.destroy();
                }
                s.app.finish();
                o.destroy();
            }
            _ => (),
        }
    }
}
impl Dispatch<zwp_tablet_v2::ZwpTabletV2, ()> for State {
    fn event(
        _: &mut Self,
        o: &zwp_tablet_v2::ZwpTabletV2,
        e: zwp_tablet_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            zwp_tablet_v2::Event::Name { name } => eprintln!("tablet: {name}"),
            zwp_tablet_v2::Event::Removed => o.destroy(),
            _ => (),
        }
    }
}
delegate_noop!(State:ignore wl_compositor::WlCompositor);
delegate_noop!(State:ignore wl_shm::WlShm);
delegate_noop!(State:ignore wl_shm_pool::WlShmPool);
delegate_noop!(State:ignore zwp_tablet_manager_v2::ZwpTabletManagerV2);
delegate_noop!(State:ignore zwp_tablet_pad_v2::ZwpTabletPadV2);

#[derive(Default)]
struct Keyboard {
    context: *mut xkbcommon_dl::xkb_context,
    keymap: *mut xkbcommon_dl::xkb_keymap,
    state: *mut xkbcommon_dl::xkb_state,
}
impl Keyboard {
    fn load(&mut self, fd: std::os::fd::OwnedFd, size: u32) -> Result<()> {
        use xkbcommon_dl::*;
        let lib = xkbcommon_option().ok_or_else(|| anyhow::anyhow!("libxkbcommon unavailable"))?;
        let mut file = File::from(fd);
        let mut bytes = Vec::new();
        file.by_ref().take(size as u64).read_to_end(&mut bytes)?;
        if bytes.last() != Some(&0) {
            bytes.push(0);
        }
        unsafe {
            self.clear();
            self.context = (lib.xkb_context_new)(xkb_context_flags::XKB_CONTEXT_NO_FLAGS);
            if self.context.is_null() {
                anyhow::bail!("XKB context creation failed");
            }
            self.keymap = (lib.xkb_keymap_new_from_string)(
                self.context,
                bytes.as_ptr().cast(),
                xkb_keymap_format::XKB_KEYMAP_FORMAT_TEXT_V1,
                xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            );
            if self.keymap.is_null() {
                anyhow::bail!("XKB keymap creation failed");
            }
            self.state = (lib.xkb_state_new)(self.keymap);
        }
        Ok(())
    }
    fn modifiers(&mut self, d: u32, l: u32, k: u32, g: u32) {
        if self.state.is_null() {
            return;
        }
        if let Some(lib) = xkbcommon_dl::xkbcommon_option() {
            unsafe {
                (lib.xkb_state_update_mask)(self.state, d, l, k, 0, 0, g);
            }
        }
    }
    fn key(&self, key: u32) -> Option<(String, String, bool, bool)> {
        use xkbcommon_dl::*;
        let lib = xkbcommon_option()?;
        if self.state.is_null() {
            return None;
        }
        unsafe {
            let sym = (lib.xkb_state_key_get_one_sym)(self.state, key + 8);
            let mut name = [0 as std::ffi::c_char; 128];
            let mut text = [0 as std::ffi::c_char; 128];
            (lib.xkb_keysym_get_name)(sym, name.as_mut_ptr(), name.len());
            (lib.xkb_keysym_to_utf8)(sym, text.as_mut_ptr(), text.len());
            let active = |s: &[u8]| {
                (lib.xkb_state_mod_name_is_active)(
                    self.state,
                    s.as_ptr().cast(),
                    xkb_state_component::XKB_STATE_MODS_EFFECTIVE,
                ) > 0
            };
            Some((
                std::ffi::CStr::from_ptr(name.as_ptr())
                    .to_string_lossy()
                    .into_owned(),
                std::ffi::CStr::from_ptr(text.as_ptr())
                    .to_string_lossy()
                    .into_owned(),
                active(b"Control\0"),
                active(b"Shift\0"),
            ))
        }
    }
    unsafe fn clear(&mut self) {
        if let Some(lib) = xkbcommon_dl::xkbcommon_option() {
            if !self.state.is_null() {
                (lib.xkb_state_unref)(self.state);
            }
            if !self.keymap.is_null() {
                (lib.xkb_keymap_unref)(self.keymap);
            }
            if !self.context.is_null() {
                (lib.xkb_context_unref)(self.context);
            }
        }
        self.state = std::ptr::null_mut();
        self.keymap = std::ptr::null_mut();
        self.context = std::ptr::null_mut();
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        unsafe {
            self.clear();
        }
    }
}
pub fn run(app: App) -> Result<()> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut state = State::new(app);
    queue.roundtrip(&mut state)?;
    state.initialize(&qh)?;
    let mut hint = false;
    while state.app.running {
        queue.dispatch_pending(&mut state)?;
        state.present(&qh)?;
        connection.flush()?;
        if state.configured && !hint {
            hint = true;
            let h = state.app.prefs.display_hint.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let _ = crate::platform::set_hint(&h);
            });
        }
        if let Some(guard) = queue.prepare_read() {
            let fd = connection.as_fd();
            let mut fds = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN,
            )];
            let ready = rustix::event::poll(
                &mut fds,
                Some(&rustix::event::Timespec {
                    tv_sec: 0,
                    tv_nsec: 50_000_000,
                }),
            )?;
            if ready > 0 {
                guard.read()?;
            } else {
                drop(guard);
            }
        }
    }
    state.app.finish();
    state.app.saver.flush()?;
    Ok(())
}
