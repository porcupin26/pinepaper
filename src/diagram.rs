//! Basic diagram geometry, stored as ordinary ink objects for export and sharing.
use crate::model::{Point, Stroke, PAGE_H, PAGE_W};
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
