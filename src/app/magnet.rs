//! Gesture-local diagram snapping. Targets are collected once, never per pen sample.
use crate::{
    diagram,
    model::{Point, PAGE_H, PAGE_W},
    render::Box2,
};

#[derive(Default)]
pub(super) struct Magnet {
    pub targets: Vec<Point>,
    active: Option<(usize, usize)>,
}
impl Magnet {
    pub fn new(targets: Vec<Point>) -> Self {
        Self {
            targets: targets
                .into_iter()
                .filter(|p| (0. ..=PAGE_W).contains(&p[0]) && (0. ..=PAGE_H).contains(&p[1]))
                .collect(),
            active: None,
        }
    }
    /// A wider release radius keeps an acquired point stable under small pen jitter.
    /// `accept` rejects corrections that violate page bounds or shape constraints.
    pub fn snap<T>(
        &mut self,
        sources: &[Point],
        zoom: f64,
        mut accept: impl FnMut(usize, Point) -> Option<T>,
    ) -> Option<(T, Point)> {
        if let Some((s, t)) = self.active {
            if let (Some(&p), Some(&q)) = (sources.get(s), self.targets.get(t)) {
                if distance(p, q) * zoom <= 21. {
                    if let Some(value) = accept(s, q) {
                        return Some((value, q));
                    }
                }
            }
        }
        self.active = None;
        let mut best = 14.;
        let mut result = None;
        for (s, &p) in sources.iter().enumerate() {
            for (t, &q) in self.targets.iter().enumerate() {
                let d = distance(p, q) * zoom;
                if d <= best {
                    if let Some(value) = accept(s, q) {
                        best = d;
                        self.active = Some((s, t));
                        result = Some((value, q));
                    }
                }
            }
        }
        result
    }
}
pub(super) fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
/// Snap the actual geometric handle, rather than the padded selection frame.
/// Reject unreachable points instead of moving the fixed anchor or distorting a circle.
pub(super) fn resized_to(
    handle: diagram::ResizeHandle,
    bounds: Box2,
    target: Point,
    uniform: bool,
    minimum: f64,
) -> Option<Box2> {
    let old = handle.point(bounds);
    let next = handle.resized(
        bounds,
        target[0] - old[0],
        target[1] - old[1],
        uniform,
        minimum,
    );
    (distance(handle.point(next), target) < 1e-7).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acquisition_release_and_nearest_feasible_target_are_stable() {
        let mut m = Magnet::new(vec![[100., 100., 0.5], [110., 100., 0.5]]);
        assert_eq!(
            m.snap(&[[94., 100., 0.5]], 1., |_, q| Some(q)).unwrap().1[0],
            100.
        );
        assert_eq!(
            m.snap(&[[115., 100., 0.5]], 1., |_, q| Some(q)).unwrap().1[0],
            100.
        );
        assert_eq!(
            m.snap(&[[123., 100., 0.5]], 1., |_, q| Some(q)).unwrap().1[0],
            110.
        );
        assert!(m.snap(&[[140., 100., 0.5]], 1., |_, q| Some(q)).is_none());
        assert_eq!(
            m.snap(&[[102., 100., 0.5]], 1., |_, q| (q[0] > 100.).then_some(q))
                .unwrap()
                .1[0],
            110.
        );
    }
    #[test]
    fn radius_uses_display_pixels_and_cannot_snap_to_invalid_resize() {
        let mut m = Magnet::new(vec![[100., 100., 0.5]]);
        assert!(m.snap(&[[120., 100., 0.5]], 1., |_, q| Some(q)).is_none());
        assert!(m.snap(&[[120., 100., 0.5]], 0.5, |_, q| Some(q)).is_some());
        let b = Box2::new(100., 100., 100., 100.);
        use diagram::ResizeHandle as H;
        assert!(resized_to(H::SouthEast, b, [250., 250., 0.5], true, 4.).is_some());
        assert!(resized_to(H::SouthEast, b, [250., 240., 0.5], true, 4.).is_none());
        assert!(resized_to(H::East, b, [250., 160., 0.5], false, 4.).is_none());
        assert!(resized_to(H::SouthEast, b, [90., 90., 0.5], false, 4.).is_none());
    }
}
