use crate::model::{Payload, Point, Stroke, Text, PAGE_H, PAGE_W};
use fontdue::{
    layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle},
    Font, FontSettings,
};
use std::collections::HashMap;

pub const WHITE: u32 = 0xffffffff;
pub const BLACK: u32 = 0xff181818;
pub const GRAY: u32 = 0xff777777;
pub const LINE: u32 = 0xffcccccc;
pub const BACK: u32 = 0xffeeeeee;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Box2 {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Box2 {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub right: usize,
    pub bottom: usize,
}
impl Rect {
    pub fn union(self, b: Self) -> Self {
        Self {
            x: self.x.min(b.x),
            y: self.y.min(b.y),
            right: self.right.max(b.right),
            bottom: self.bottom.max(b.bottom),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub clip: Box2,
}
impl View {
    pub fn world(self, x: f64, y: f64, p: f64) -> Point {
        [(x - self.x) / self.zoom, (y - self.y) / self.zoom, p]
    }
    pub fn screen(self, p: Point) -> Point {
        [self.x + p[0] * self.zoom, self.y + p[1] * self.zoom, p[2]]
    }
    pub fn contains(self, x: f64, y: f64) -> bool {
        let p = self.world(x, y, 0.);
        self.clip.contains(x, y) && p[0] >= 0. && p[0] <= PAGE_W && p[1] >= 0. && p[1] <= PAGE_H
    }
}
pub struct Surface {
    pub pixels: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub scale: f64,
    pub logical_w: f64,
    pub logical_h: f64,
    pub clip: Box2,
    fonts: [Font; 2],
    glyphs: HashMap<(usize, u16, u32), (fontdue::Metrics, Vec<u8>)>,
    stamps: Vec<u32>,
    coverage: Vec<u8>,
    base: Vec<u32>,
    generation: u32,
    raw: Vec<u32>,
    tone_lut: Option<[u32; 256]>,
    pub contrast: f64,
    pub gamma: f64,
}
pub struct SurfaceSnapshot {
    pixels: Vec<u32>,
    raw: Vec<u32>,
}
impl Surface {
    pub fn snapshot(&self) -> SurfaceSnapshot {
        SurfaceSnapshot {
            pixels: self.pixels.clone(),
            raw: self.raw.clone(),
        }
    }
    pub fn restore(&mut self, snapshot: &SurfaceSnapshot, rect: Rect) {
        if self.pixels.len() != snapshot.pixels.len() {
            return;
        }
        for y in rect.y..rect.bottom {
            let range = y * self.width + rect.x..y * self.width + rect.right;
            self.pixels[range.clone()].copy_from_slice(&snapshot.pixels[range.clone()]);
            if self.raw.len() == snapshot.raw.len() && !self.raw.is_empty() {
                self.raw[range.clone()].copy_from_slice(&snapshot.raw[range]);
            }
        }
    }
    pub fn new() -> Self {
        Self {
            pixels: vec![],
            width: 0,
            height: 0,
            scale: 1.,
            logical_w: 0.,
            logical_h: 0.,
            clip: Box2::default(),
            fonts: [
                Font::from_bytes(
                    include_bytes!("../assets/DejaVuSans.ttf") as &[u8],
                    FontSettings::default(),
                )
                .unwrap(),
                Font::from_bytes(
                    include_bytes!("../assets/DejaVuSerif.ttf") as &[u8],
                    FontSettings::default(),
                )
                .unwrap(),
            ],
            glyphs: HashMap::new(),
            stamps: vec![],
            coverage: vec![],
            base: vec![],
            generation: 0,
            raw: vec![],
            tone_lut: None,
            contrast: 1.,
            gamma: 1.,
        }
    }
    pub fn resize(&mut self, w: usize, h: usize, scale: i32) {
        self.scale = scale.max(1) as f64;
        self.logical_w = w as f64;
        self.logical_h = h as f64;
        self.width = w * scale.max(1) as usize;
        self.height = h * scale.max(1) as usize;
        self.pixels = vec![WHITE; self.width * self.height];
        self.stamps = vec![0; self.pixels.len()];
        self.coverage = vec![0; self.pixels.len()];
        self.base = vec![0; self.pixels.len()];
        self.tone_lut = None;
        self.raw.clear();
        self.reset_clip();
    }
    pub fn full_rect(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            right: self.width,
            bottom: self.height,
        }
    }
    pub fn reset_clip(&mut self) {
        self.clip = Box2::new(0., 0., self.logical_w, self.logical_h);
    }
    pub fn rect(&self, b: Box2) -> Rect {
        Rect {
            x: ((b.x.max(self.clip.x) * self.scale).floor().max(0.) as usize).min(self.width),
            y: ((b.y.max(self.clip.y) * self.scale).floor().max(0.) as usize).min(self.height),
            right: (((b.x + b.w).min(self.clip.x + self.clip.w) * self.scale)
                .ceil()
                .max(0.) as usize)
                .min(self.width),
            bottom: (((b.y + b.h).min(self.clip.y + self.clip.h) * self.scale)
                .ceil()
                .max(0.) as usize)
                .min(self.height),
        }
    }
    pub fn fill(&mut self, b: Box2, color: u32) {
        let r = self.rect(b);
        if r.x >= r.right {
            return;
        }
        for y in r.y..r.bottom {
            self.pixels[y * self.width + r.x..y * self.width + r.right].fill(color);
        }
    }
    pub fn border(&mut self, b: Box2, c: u32, weight: f64) {
        self.fill(Box2::new(b.x, b.y, b.w, weight), c);
        self.fill(Box2::new(b.x, b.y + b.h - weight, b.w, weight), c);
        self.fill(Box2::new(b.x, b.y, weight, b.h), c);
        self.fill(Box2::new(b.x + b.w - weight, b.y, weight, b.h), c);
    }
    pub fn rounded(&mut self, b: Box2, radius: f64, color: u32) {
        let r = radius.min(b.w / 2.).min(b.h / 2.);
        let bounds = self.rect(b);
        // A circle has a zero-width center range. Avoid inverted clamp bounds
        // when fractional positions round x+r and x+w-r differently.
        let min_x = b.x + r;
        let max_x = (b.x + b.w - r).max(min_x);
        let min_y = b.y + r;
        let max_y = (b.y + b.h - r).max(min_y);
        for y in bounds.y..bounds.bottom {
            for x in bounds.x..bounds.right {
                let px = (x as f64 + 0.5) / self.scale;
                let py = (y as f64 + 0.5) / self.scale;
                let cx = px.clamp(min_x, max_x);
                let cy = py.clamp(min_y, max_y);
                let alpha = ((r - (px - cx).hypot(py - cy)) * self.scale + 0.5).clamp(0., 1.);
                let i = y * self.width + x;
                self.pixels[i] = blend(self.pixels[i], color, alpha);
            }
        }
    }
    #[allow(clippy::too_many_arguments)] // Raster primitive; position and style are explicit.
    pub fn text(
        &mut self,
        t: &str,
        x: f64,
        y: f64,
        size: f64,
        color: u32,
        max_w: f64,
        serif: bool,
    ) -> f64 {
        let idx = usize::from(serif);
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: (x * self.scale) as f32,
            y: (y * self.scale) as f32,
            max_width: Some((max_w.max(1.) * self.scale) as f32),
            line_height: if serif { 1.28 } else { 1.0 },
            ..LayoutSettings::default()
        });
        layout.append(
            &self.fonts,
            &TextStyle::new(t, (size * self.scale) as f32, idx),
        );
        if self.glyphs.len() > 6000 {
            self.glyphs.clear();
        }
        let clip = self.rect(self.clip);
        for g in layout.glyphs() {
            if g.parent.is_control() {
                continue;
            }
            let key = (idx, g.key.glyph_index, g.key.px.to_bits());
            let (metrics, bits) = self
                .glyphs
                .entry(key)
                .or_insert_with(|| self.fonts[idx].rasterize_indexed(g.key.glyph_index, g.key.px));
            for yy in 0..metrics.height {
                let py = g.y as i64 + yy as i64;
                if py < clip.y as i64 || py >= clip.bottom as i64 {
                    continue;
                }
                for xx in 0..metrics.width {
                    let px = g.x as i64 + xx as i64;
                    if px < clip.x as i64 || px >= clip.right as i64 {
                        continue;
                    }
                    let alpha = bits[yy * metrics.width + xx];
                    if alpha > 0 {
                        let i = py as usize * self.width + px as usize;
                        self.pixels[i] = blend(self.pixels[i], color, alpha as f64 / 255.);
                    }
                }
            }
        }
        layout.height() as f64 / self.scale
    }
    pub fn label(&mut self, t: &str, b: Box2, size: f64, c: u32) {
        self.aligned_label(t, b, size, c, false);
    }
    pub fn centered_label(&mut self, t: &str, b: Box2, size: f64, c: u32) {
        self.aligned_label(t, b, size, c, true);
    }
    fn aligned_label(&mut self, t: &str, b: Box2, size: f64, c: u32, centered: bool) {
        let old = self.clip;
        self.clip = Box2::new(
            b.x.max(old.x),
            b.y.max(old.y),
            (b.x + b.w).min(old.x + old.w) - b.x.max(old.x),
            (b.y + b.h).min(old.y + old.h) - b.y.max(old.y),
        );
        let mut label = String::new();
        let mut width = 0.;
        for ch in t.chars() {
            let advance = self.fonts[0].metrics(ch, size as f32).advance_width as f64;
            if width + advance > b.w {
                if !label.is_empty() {
                    label.pop();
                }
                label.push('…');
                break;
            }
            label.push(ch);
            width += advance;
        }
        let (x, y) = if centered && !label.is_empty() {
            let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
            layout.append(
                &self.fonts,
                &TextStyle::new(&label, (size * self.scale) as f32, 0),
            );
            let mut bounds: Option<(f64, f64, f64, f64)> = None;
            for g in layout
                .glyphs()
                .iter()
                .filter(|g| g.width > 0 && g.height > 0)
            {
                let (x, y) = (g.x as f64, g.y as f64);
                let (right, bottom) = (x + g.width as f64, y + g.height as f64);
                bounds = Some(match bounds {
                    Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(right), b.max(bottom)),
                    None => (x, y, right, bottom),
                });
            }
            if let Some((l, t, r, bottom)) = bounds {
                (
                    b.x + b.w / 2. - (l + r) / (2. * self.scale),
                    b.y + b.h / 2. - (t + bottom) / (2. * self.scale),
                )
            } else {
                (b.x, b.y)
            }
        } else {
            (b.x, b.y + (b.h - size * 1.2) / 2.)
        };
        self.text(&label, x, y, size, c, 100000., false);
        self.clip = old;
    }
    pub fn begin_stroke(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamps.fill(0);
            self.generation = 1;
        }
    }
    pub fn segment(&mut self, v: View, a: Point, b: Point, stroke: &Stroke) -> Rect {
        self.segment_raster::<true>(v, a, b, stroke)
    }
    fn segment_raster<const SCANLINE: bool>(
        &mut self,
        v: View,
        a: Point,
        b: Point,
        stroke: &Stroke,
    ) -> Rect {
        let old = self.clip;
        self.clip = v.clip;
        let aa = v.screen(a);
        let bb = v.screen(b);
        let a = [aa[0] * self.scale, aa[1] * self.scale, a[2]];
        let b = [bb[0] * self.scale, bb[1] * self.scale, b[2]];
        let pressure = ((a[2] + b[2]) / 2.).clamp(0.08, 1.);
        let mult = match stroke.pen.as_str() {
            "Mechanical" => 0.7,
            "Marker" => 2.5,
            "Ballpoint" => 0.5 + pressure,
            "Pencil" => 0.5 + pressure * 0.9,
            "Calligraphy" => {
                (0.5 + 2.
                    * ((b[1] - a[1]).atan2(b[0] - a[0]) - std::f64::consts::FRAC_PI_4)
                        .sin()
                        .abs())
                    * (0.5 + pressure)
            }
            _ => 1.,
        };
        let highlight = stroke.tool == "highlighter";
        let width = if highlight {
            (stroke.width * 7.).max(14.)
        } else {
            stroke.width * mult
        };
        let r = (width * v.zoom * self.scale / 2.).max(0.35);
        let opacity = if highlight {
            0.19
        } else if stroke.pen == "Pencil" {
            0.72
        } else {
            1.
        };
        let gray = if highlight {
            0.15
        } else {
            stroke.color.clamp(0., 1.)
        };
        let col = gray_color(gray);
        let bounds = self.rect(Box2::new(
            (a[0].min(b[0]) - r - 1.) / self.scale,
            (a[1].min(b[1]) - r - 1.) / self.scale,
            ((a[0] - b[0]).abs() + 2. * r + 2.) / self.scale,
            ((a[1] - b[1]).abs() + 2. * r + 2.) / self.scale,
        ));
        // Distance to the segment gives a round, antialiased stroke without a frame clock.
        // Per-stroke coverage prevents translucent ink darkening at every input sample.
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let dd = dx * dx + dy * dy;
        for y in bounds.y..bounds.bottom {
            // Intersect this row with the segment's antialiased capsule. A long
            // diagonal must visit pixels near the ink, not its entire rectangle.
            // Keep the exact coverage calculation below (including round caps).
            let (left, right) = if SCANLINE && dy.abs() > 1e-9 {
                let radius = r + 0.5;
                let t0 = (y as f64 + 0.5 - radius - a[1]) / dy;
                let t1 = (y as f64 + 0.5 + radius - a[1]) / dy;
                let lo = t0.min(t1).max(0.);
                let hi = t0.max(t1).min(1.);
                if lo > hi {
                    continue;
                }
                let x0 = a[0] + lo * dx;
                let x1 = a[0] + hi * dx;
                (
                    bounds
                        .x
                        .max((x0.min(x1) - radius - 1.).floor().max(0.) as usize),
                    bounds
                        .right
                        .min((x0.max(x1) + radius + 1.).ceil().max(0.) as usize),
                )
            } else {
                (bounds.x, bounds.right)
            };
            for x in left..right {
                let t = if dd > 0. {
                    (((x as f64 + 0.5 - a[0]) * dx + (y as f64 + 0.5 - a[1]) * dy) / dd)
                        .clamp(0., 1.)
                } else {
                    0.
                };
                let dist = (x as f64 + 0.5 - a[0] - t * dx).hypot(y as f64 + 0.5 - a[1] - t * dy);
                let alpha = ((r + 0.5 - dist).clamp(0., 1.) * 255.) as u8;
                if alpha == 0 {
                    continue;
                }
                let i = y * self.width + x;
                if self.stamps[i] != self.generation {
                    self.stamps[i] = self.generation;
                    self.coverage[i] = 0;
                    self.base[i] = if self.tone_lut.is_some() {
                        self.raw[i]
                    } else {
                        self.pixels[i]
                    };
                }
                if alpha > self.coverage[i] {
                    self.coverage[i] = alpha;
                    let p = blend(self.base[i], col, alpha as f64 / 255. * opacity);
                    self.pixels[i] = if let Some(lut) = &self.tone_lut {
                        self.raw[i] = p;
                        toned(p, lut)
                    } else {
                        p
                    };
                }
            }
        }
        self.clip = old;
        bounds
    }
    pub fn stroke(&mut self, v: View, s: &Stroke) {
        self.begin_stroke();
        for i in 0..s.points.len() {
            self.segment(v, s.points[i.saturating_sub(1)], s.points[i], s);
        }
    }
    pub fn line(&mut self, a: (f64, f64), b: (f64, f64), width: f64, c: f64) {
        let v = View {
            x: 0.,
            y: 0.,
            zoom: 1.,
            clip: self.clip,
        };
        let s = Stroke {
            width,
            color: c,
            ..Stroke::default()
        };
        self.stroke(
            v,
            &Stroke {
                points: vec![[a.0, a.1, 0.5], [b.0, b.1, 0.5]],
                ..s
            },
        );
    }
    pub fn image(&mut self, img: &image::RgbImage, v: View) {
        let ratio = (PAGE_W / img.width() as f64).min(PAGE_H / img.height() as f64);
        let w = img.width() as f64 * ratio;
        let h = img.height() as f64 * ratio;
        let left = v.x + (PAGE_W - w) / 2. * v.zoom;
        let top = v.y + (PAGE_H - h) / 2. * v.zoom;
        let r = self.rect(Box2::new(left, top, w * v.zoom, h * v.zoom));
        for y in r.y..r.bottom {
            for x in r.x..r.right {
                let ix = (((x as f64 / self.scale - left) / (ratio * v.zoom)) as u32)
                    .min(img.width() - 1);
                let iy = (((y as f64 / self.scale - top) / (ratio * v.zoom)) as u32)
                    .min(img.height() - 1);
                let p = img.get_pixel(ix, iy);
                self.pixels[y * self.width + x] =
                    0xff000000 | ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32;
            }
        }
    }
    pub fn page(&mut self, v: View, p: &Payload, pdf: Option<&image::RgbImage>, annotations: bool) {
        let old = self.clip;
        self.clip = v.clip;
        self.fill(Box2::new(v.x, v.y, PAGE_W * v.zoom, PAGE_H * v.zoom), WHITE);
        if let Some(img) = pdf {
            self.image(img, v);
        } else {
            self.template(v, &p.template);
        }
        if let Some(body) = &p.body {
            self.text(
                p.heading.as_deref().unwrap_or(""),
                v.x + 68. * v.zoom,
                v.y + 48. * v.zoom,
                15. * v.zoom,
                GRAY,
                624. * v.zoom,
                false,
            );
            self.line(
                (v.x + 68. * v.zoom, v.y + 80. * v.zoom),
                (v.x + 692. * v.zoom, v.y + 80. * v.zoom),
                0.7 * v.zoom,
                0.78,
            );
            self.text(
                body,
                v.x + 68. * v.zoom,
                v.y + 118. * v.zoom,
                28. * v.zoom,
                BLACK,
                624. * v.zoom,
                true,
            );
        }
        if annotations {
            self.objects(v, &p.strokes, &p.texts);
        }
        self.clip = old;
    }
    fn objects(&mut self, v: View, strokes: &[Stroke], texts: &[Text]) {
        for s in strokes {
            self.stroke(v, s);
        }
        for t in texts {
            self.text(
                &t.text,
                v.x + t.x * v.zoom,
                v.y + t.y * v.zoom,
                t.size * v.zoom,
                gray_color(t.color),
                (PAGE_W - t.x - 24.).max(1.) * v.zoom,
                t.font == "Serif",
            );
        }
    }
    /// Cache the fixed layers below an edit once, retaining the exact draw order.
    pub fn object_background(
        &mut self,
        view: View,
        payload: &Payload,
        pdf: Option<&image::RgbImage>,
        first_stroke: usize,
        first_text: usize,
    ) -> SurfaceSnapshot {
        let previous = self.snapshot();
        let mut lower = payload.clone();
        lower.strokes.truncate(first_stroke);
        lower.texts.truncate(first_text);
        let rect = self.repaint_page_region(view, &lower, pdf, view.clip);
        let background = self.snapshot();
        self.restore(&previous, rect);
        background
    }
    /// Restore cached lower layers, then paint the edited object and layers above
    /// it. PDF/template/text layout and earlier ink are not rerendered per sample.
    #[allow(clippy::too_many_arguments)]
    pub fn repaint_objects(
        &mut self,
        view: View,
        payload: &Payload,
        background: &SurfaceSnapshot,
        first_stroke: usize,
        first_text: usize,
        region: Box2,
    ) -> Rect {
        let old_clip = self.clip;
        self.clip = view.clip;
        let rect = self.rect(region);
        if rect.x >= rect.right || rect.y >= rect.bottom {
            self.clip = old_clip;
            return rect;
        }
        self.restore(background, rect);
        let clip = Box2::new(
            rect.x as f64 / self.scale,
            rect.y as f64 / self.scale,
            (rect.right - rect.x) as f64 / self.scale,
            (rect.bottom - rect.y) as f64 / self.scale,
        );
        let lut = self.tone_lut.take();
        if lut.is_some() {
            for y in rect.y..rect.bottom {
                let row = y * self.width + rect.x..y * self.width + rect.right;
                self.pixels[row.clone()].copy_from_slice(&self.raw[row]);
            }
        }
        self.clip = clip;
        self.objects(
            View { clip, ..view },
            &payload.strokes[first_stroke..],
            &payload.texts[first_text..],
        );
        if let Some(table) = &lut {
            for y in rect.y..rect.bottom {
                for x in rect.x..rect.right {
                    let i = y * self.width + x;
                    self.raw[i] = self.pixels[i];
                    self.pixels[i] = toned(self.pixels[i], table);
                }
            }
        }
        self.tone_lut = lut;
        self.clip = old_clip;
        rect
    }
    /// Rebuild only the dirty region, preserving raw pixels outside it and draw order.
    pub fn repaint_page_region(
        &mut self,
        view: View,
        payload: &Payload,
        pdf: Option<&image::RgbImage>,
        region: Box2,
    ) -> Rect {
        let old_clip = self.clip;
        self.clip = view.clip;
        let rect = self.rect(region);
        if rect.x >= rect.right || rect.y >= rect.bottom {
            self.clip = old_clip;
            return rect;
        }
        let clip = Box2::new(
            rect.x as f64 / self.scale,
            rect.y as f64 / self.scale,
            (rect.right - rect.x) as f64 / self.scale,
            (rect.bottom - rect.y) as f64 / self.scale,
        );
        let lut = self.tone_lut.take();
        self.clip = clip;
        self.fill(clip, BACK);
        self.page(View { clip, ..view }, payload, pdf, true);
        if let Some(table) = &lut {
            for y in rect.y..rect.bottom {
                for x in rect.x..rect.right {
                    let i = y * self.width + x;
                    self.raw[i] = self.pixels[i];
                    self.pixels[i] = toned(self.pixels[i], table);
                }
            }
        }
        self.tone_lut = lut;
        self.clip = old_clip;
        rect
    }
    pub fn text_bounds(&self, text: &Text) -> Box2 {
        let serif = text.font == "Serif";
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            x: text.x as f32,
            y: text.y as f32,
            max_width: Some((PAGE_W - text.x - 24.).max(1.) as f32),
            line_height: if serif { 1.28 } else { 1.0 },
            ..LayoutSettings::default()
        });
        layout.append(
            &self.fonts,
            &TextStyle::new(&text.text, text.size as f32, usize::from(serif)),
        );
        let right = layout
            .glyphs()
            .iter()
            .map(|g| g.x as f64 + g.width as f64)
            .fold(text.x, f64::max);
        Box2::new(
            text.x,
            text.y,
            (right - text.x).max(1.),
            (layout.height() as f64).max(text.size),
        )
    }
    fn template(&mut self, v: View, name: &str) {
        let line = |s: &mut Self, x: f64, y: f64, xx: f64, yy: f64| {
            s.line(
                (v.x + x * v.zoom, v.y + y * v.zoom),
                (v.x + xx * v.zoom, v.y + yy * v.zoom),
                0.65 * v.zoom,
                0.82,
            )
        };
        match name {
            "Lined" | "Checklist" => {
                for y in (104..1015).step_by(38) {
                    line(self, 42., y as f64, 718., y as f64);
                    if name == "Checklist" {
                        self.border(
                            Box2::new(
                                v.x + 48. * v.zoom,
                                v.y + (y as f64 - 22.) * v.zoom,
                                13. * v.zoom,
                                13. * v.zoom,
                            ),
                            LINE,
                            0.65 * v.zoom,
                        );
                    }
                }
            }
            "Grid" => {
                for x in (28..760).step_by(28) {
                    line(self, x as f64, 0., x as f64, 1040.);
                }
                for y in (28..1040).step_by(28) {
                    line(self, 0., y as f64, 760., y as f64);
                }
            }
            "Dots" => {
                for x in (32..760).step_by(28) {
                    for y in (32..1040).step_by(28) {
                        self.fill(
                            Box2::new(
                                v.x + x as f64 * v.zoom,
                                v.y + y as f64 * v.zoom,
                                1.5 * v.zoom,
                                1.5 * v.zoom,
                            ),
                            GRAY,
                        );
                    }
                }
            }
            "Music" => {
                for y in (100..960).step_by(136) {
                    for n in 0..5 {
                        line(self, 40., (y + n * 12) as f64, 720., (y + n * 12) as f64);
                    }
                }
            }
            _ => (),
        }
    }
    pub fn clear_tone(&mut self) {
        self.tone_lut = None;
    }
    pub fn tone(&mut self, b: Box2, contrast: f64, gamma: f64) {
        if contrast == 1. && gamma == 1. {
            return;
        }
        let mut lut = [0u32; 256];
        for (i, c) in lut.iter_mut().enumerate() {
            *c = (((i as f64 / 255.).powf(gamma) - 0.5) * contrast + 0.5)
                .clamp(0., 1.)
                .mul_add(255., 0.5) as u32;
        }
        self.raw.clone_from(&self.pixels);
        let r = self.rect(b);
        for y in r.y..r.bottom {
            for x in r.x..r.right {
                let i = y * self.width + x;
                self.pixels[i] = toned(self.pixels[i], &lut);
            }
        }
        self.tone_lut = Some(lut);
    }
    pub fn save_png(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut img = image::RgbImage::new(self.width as u32, self.height as u32);
        for (p, &c) in img.pixels_mut().zip(&self.pixels) {
            *p = image::Rgb([
                ((c >> 16) & 255) as u8,
                ((c >> 8) & 255) as u8,
                (c & 255) as u8,
            ]);
        }
        img.save(path)?;
        Ok(())
    }
}
impl Default for Surface {
    fn default() -> Self {
        Self::new()
    }
}
pub fn gray_color(v: f64) -> u32 {
    let c = (v.clamp(0., 1.) * 255.).round() as u32;
    0xff000000 | (c * 0x010101)
}
pub fn blend(a: u32, b: u32, t: f64) -> u32 {
    let chan = |s: u32| {
        ((((a >> s) & 255) as f64 * (1. - t) + ((b >> s) & 255) as f64 * t).round() as u32) << s
    };
    0xff000000 | chan(16) | chan(8) | chan(0)
}

fn toned(p: u32, lut: &[u32; 256]) -> u32 {
    0xff000000
        | (lut[((p >> 16) & 255) as usize] << 16)
        | (lut[((p >> 8) & 255) as usize] << 8)
        | lut[(p & 255) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanline_capsules_match_exhaustive_raster_for_clips_caps_pressure_and_tone() {
        let mut fast = Surface::new();
        let mut reference = Surface::new();
        for scale in [1, 2] {
            fast.resize(140, 180, scale);
            reference.resize(140, 180, scale);
            let v = View {
                x: 3.2,
                y: 4.7,
                zoom: 0.85,
                clip: Box2::new(9.25, 11.5, 110.5, 145.25),
            };
            fast.tone(v.clip, 1.3, 0.8);
            reference.tone(v.clip, 1.3, 0.8);
            for i in 0..180 {
                let angle = i as f64 * std::f64::consts::TAU / 180.;
                let a = [72.3 + 120. * angle.cos(), 88.7 + 145. * angle.sin(), 0.17];
                let b = if i % 11 == 0 {
                    a
                } else {
                    [72.3 - 95. * angle.cos(), 88.7 - 115. * angle.sin(), 0.94]
                };
                let s = Stroke {
                    width: [0.3, 2.4, 8., 22.][i % 4],
                    pen: ["Fineliner", "Ballpoint", "Pencil", "Calligraphy"][i % 4].into(),
                    tool: if i % 5 == 0 { "highlighter" } else { "pen" }.into(),
                    color: 0.3,
                    ..Stroke::default()
                };
                fast.begin_stroke();
                reference.begin_stroke();
                for (a, b) in [(a, a), (a, b), (b, a)] {
                    let actual = fast.segment(v, a, b, &s);
                    let expected = reference.segment_raster::<false>(v, a, b, &s);
                    assert_eq!(actual, expected);
                }
                assert!(
                    fast.pixels == reference.pixels,
                    "Visible raster differs at angle {i}, scale {scale}"
                );
                assert!(
                    fast.raw == reference.raw,
                    "Untinted raster differs at angle {i}, scale {scale}"
                );
            }
        }
    }
}
