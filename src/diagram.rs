//! Basic diagram geometry, stored as ordinary ink objects for export and sharing.
use crate::model::{Point, Stroke, PAGE_H, PAGE_W};
use crate::render::Box2;
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Line,
    #[default]
    Arrow,
    DoubleArrow,
    Rectangle,
    Square,
    Ellipse,
    Circle,
    Diamond,
}
impl Shape {
    pub const ALL: [Self; 8] = [
        Self::Line,
        Self::Arrow,
        Self::DoubleArrow,
        Self::Rectangle,
        Self::Square,
        Self::Ellipse,
        Self::Circle,
        Self::Diamond,
    ];
    pub fn closed(self) -> bool {
        matches!(
            self,
            Self::Rectangle | Self::Square | Self::Ellipse | Self::Circle | Self::Diamond
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::DoubleArrow => "Double arrow",
            Self::Rectangle => "Rectangle",
            Self::Square => "Square",
            Self::Ellipse => "Ellipse",
            Self::Circle => "Circle",
            Self::Diamond => "Diamond",
        }
    }
    pub fn stroke(self, start: Point, end: Point, width: f64, color: f64) -> Stroke {
        let a = [start[0].clamp(0., PAGE_W), start[1].clamp(0., PAGE_H), 0.5];
        let mut b = [end[0].clamp(0., PAGE_W), end[1].clamp(0., PAGE_H), 0.5];
        if matches!(self, Self::Square | Self::Circle) {
            let sx = if b[0] < a[0] { -1. } else { 1. };
            let sy = if b[1] < a[1] { -1. } else { 1. };
            let side = (b[0] - a[0])
                .abs()
                .max((b[1] - a[1]).abs())
                .min(if sx > 0. { PAGE_W - a[0] } else { a[0] })
                .min(if sy > 0. { PAGE_H - a[1] } else { a[1] });
            b = [a[0] + sx * side, a[1] + sy * side, 0.5];
        }
        let point = |x, y| [x, y, 0.5];
        let (cx, cy) = ((a[0] + b[0]) / 2., (a[1] + b[1]) / 2.);
        let mut points = match self {
            Self::Rectangle | Self::Square => vec![a, point(b[0], a[1]), b, point(a[0], b[1]), a],
            Self::Diamond => vec![
                point(cx, a[1]),
                point(b[0], cy),
                point(cx, b[1]),
                point(a[0], cy),
                point(cx, a[1]),
            ],
            Self::Ellipse | Self::Circle => (0..=128)
                .map(|n| {
                    let angle = n as f64 * TAU / 128.;
                    point(
                        cx + (b[0] - a[0]) / 2. * angle.cos(),
                        cy + (b[1] - a[1]) / 2. * angle.sin(),
                    )
                })
                .collect(),
            _ => vec![a, b],
        };
        if matches!(self, Self::Arrow | Self::DoubleArrow) {
            let head = |from: Point, tip: Point| {
                let angle = (tip[1] - from[1]).atan2(tip[0] - from[0]);
                let length =
                    (12. + width * 2.).min((tip[0] - from[0]).hypot(tip[1] - from[1]) * 0.35);
                [
                    point(
                        tip[0] - length * (angle - 0.48).cos(),
                        tip[1] - length * (angle - 0.48).sin(),
                    ),
                    tip,
                    point(
                        tip[0] - length * (angle + 0.48).cos(),
                        tip[1] - length * (angle + 0.48).sin(),
                    ),
                    tip,
                ]
            };
            points.extend(head(a, b));
            if self == Self::DoubleArrow {
                points.push(a);
                points.extend(head(b, a));
            }
        }
        // Keep arrowheads on the page even when the tip reaches an edge.
        for p in &mut points {
            p[0] = p[0].clamp(0., PAGE_W);
            p[1] = p[1].clamp(0., PAGE_H);
        }
        let mut stroke = Stroke {
            width,
            color,
            points,
            ..Stroke::default()
        };
        stroke
            .extra
            .insert("diagram_shape".into(), serde_json::json!(self));
        stroke
    }
}

/// No new file format: these fields were already written by the first diagram tool.
pub fn shape(stroke: &Stroke) -> Option<Shape> {
    serde_json::from_value(stroke.extra.get("diagram_shape")?.clone()).ok()
}
pub fn endpoints(stroke: &Stroke) -> Option<[Point; 2]> {
    if matches!(
        shape(stroke)?,
        Shape::Line | Shape::Arrow | Shape::DoubleArrow
    ) && stroke.points.len() >= 2
    {
        Some([stroke.points[0], stroke.points[1]])
    } else {
        None
    }
}
/// Connection points on the actual geometry, not sampled ink or arrowhead tips.
pub fn anchors(stroke: &Stroke) -> Vec<Point> {
    if let Some(ends) = endpoints(stroke) {
        return ends.into_iter().collect();
    }
    let Some(b) = bounds(stroke) else {
        return vec![];
    };
    let handles: &[ResizeHandle] =
        if matches!(shape(stroke), Some(Shape::Rectangle | Shape::Square)) {
            &ResizeHandle::ALL
        } else {
            &[
                ResizeHandle::North,
                ResizeHandle::East,
                ResizeHandle::South,
                ResizeHandle::West,
            ]
        };
    handles.iter().map(|h| h.point(b)).collect()
}
pub fn with_endpoint(stroke: &Stroke, end: usize, point: Point) -> Option<Stroke> {
    let mut ends = endpoints(stroke)?;
    let previous = *ends.get(end)?;
    if (previous[0] - point[0]).hypot(previous[1] - point[1]) < 1e-9 {
        return Some(stroke.clone());
    }
    *ends.get_mut(end)? = point;
    let geometry = shape(stroke)?.stroke(ends[0], ends[1], stroke.width, stroke.color);
    let mut updated = stroke.clone();
    updated.points = geometry.points;
    Some(updated) // Keep shared object identity, style and all unknown metadata.
}

/// Clockwise handles around a closed shape's selection frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeHandle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}
impl ResizeHandle {
    pub const ALL: [Self; 8] = [
        Self::NorthWest,
        Self::North,
        Self::NorthEast,
        Self::East,
        Self::SouthEast,
        Self::South,
        Self::SouthWest,
        Self::West,
    ];
    pub fn axes(self) -> (i8, i8) {
        match self {
            Self::NorthWest => (-1, -1),
            Self::North => (0, -1),
            Self::NorthEast => (1, -1),
            Self::East => (1, 0),
            Self::SouthEast => (1, 1),
            Self::South => (0, 1),
            Self::SouthWest => (-1, 1),
            Self::West => (-1, 0),
        }
    }
    pub fn point(self, b: Box2) -> Point {
        let (x, y) = self.axes();
        [
            b.x + (x as f64 + 1.) * b.w / 2.,
            b.y + (y as f64 + 1.) * b.h / 2.,
            0.5,
        ]
    }
    /// Keep the opposite corner/edge fixed. Stop at a small visible size rather
    /// than flipping the shape through its anchor; constrain all edges to paper.
    pub fn resized(self, b: Box2, dx: f64, dy: f64, uniform: bool, minimum: f64) -> Box2 {
        let (sx, sy) = self.axes();
        if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
            return b;
        }
        let axis = |low: f64, size: f64, delta: f64, sign: i8, limit: f64| {
            if sign == 0 || delta.abs() < 1e-9 {
                return (low, size);
            }
            let anchor = if sign < 0 { low + size } else { low };
            let maximum = if sign < 0 { anchor } else { limit - anchor }.max(0.);
            let size = (size + delta * sign as f64).clamp(minimum.min(maximum), maximum);
            (if sign < 0 { anchor - size } else { anchor }, size)
        };
        if uniform && sx != 0 && sy != 0 {
            if (dx * sx as f64 + dy * sy as f64).abs() < 1e-9 {
                return b;
            }
            let ax = if sx < 0 { b.x + b.w } else { b.x };
            let ay = if sy < 0 { b.y + b.h } else { b.y };
            let maximum = (if sx < 0 { ax } else { PAGE_W - ax })
                .min(if sy < 0 { ay } else { PAGE_H - ay })
                .max(0.);
            let side = ((b.w + dx * sx as f64 + b.h + dy * sy as f64) / 2.)
                .clamp(minimum.min(maximum), maximum);
            Box2::new(
                if sx < 0 { ax - side } else { ax },
                if sy < 0 { ay - side } else { ay },
                side,
                side,
            )
        } else {
            let (x, w) = axis(b.x, b.w, dx, sx, PAGE_W);
            let (y, h) = axis(b.y, b.h, dy, sy, PAGE_H);
            Box2::new(x, y, w, h)
        }
    }
}
pub fn bounds(stroke: &Stroke) -> Option<Box2> {
    if !shape(stroke)?.closed() || stroke.points.is_empty() {
        return None;
    }
    let (mut x, mut y, mut right, mut bottom) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for p in &stroke.points {
        if !p[0].is_finite() || !p[1].is_finite() {
            return None;
        }
        x = x.min(p[0]);
        y = y.min(p[1]);
        right = right.max(p[0]);
        bottom = bottom.max(p[1]);
    }
    Some(Box2::new(x, y, right - x, bottom - y))
}
pub fn with_bounds(stroke: &Stroke, next: Box2) -> Option<Stroke> {
    let old = bounds(stroke)?;
    if [
        old.x - next.x,
        old.y - next.y,
        old.w - next.w,
        old.h - next.h,
    ]
    .iter()
    .all(|d| d.abs() < 1e-9)
    {
        return Some(stroke.clone());
    }
    let mut updated = stroke.clone();
    updated.points = shape(stroke)?
        .stroke(
            [next.x, next.y, 0.5],
            [next.x + next.w, next.y + next.h, 0.5],
            stroke.width,
            stroke.color,
        )
        .points;
    Some(updated) // Preserve line weight, style, shared identity and unknown metadata.
}
