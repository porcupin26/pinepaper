use super::*;
use crate::{diagram, model::distance};

pub(super) enum EditKind {
    Move,
    Endpoint {
        stroke: usize,
        end: usize,
    },
    Resize {
        stroke: usize,
        handle: diagram::ResizeHandle,
        bounds: Box2,
        uniform: bool,
    },
}
pub(super) struct ObjectEdit {
    pub original: Payload,
    start: Point,
    kind: EditKind,
    dragging: bool,
    bounds: Box2,
    pub background: Option<SurfaceSnapshot>,
    first_stroke: usize,
    first_text: usize,
}

impl App {
    fn selected_resize_shape(&self) -> Option<(usize, Box2, bool)> {
        if self.tool != "select"
            || self.selection.strokes.len() != 1
            || !self.selection.texts.is_empty()
        {
            return None;
        }
        let i = self.selection.strokes[0];
        let s = self.page()?.payload.strokes.get(i)?;
        Some((
            i,
            diagram::bounds(s)?,
            matches!(
                diagram::shape(s)?,
                diagram::Shape::Square | diagram::Shape::Circle
            ),
        ))
    }
    pub fn selection_frame(&self) -> Option<Box2> {
        let b = self.selection.bounds?;
        if self.selected_resize_shape().is_some() {
            // Keep the frame and its handles on paper, including shapes at an edge.
            let x = b.x.clamp(0., PAGE_W);
            let y = b.y.clamp(0., PAGE_H);
            Some(Box2::new(
                x,
                y,
                (b.x + b.w).clamp(x, PAGE_W) - x,
                (b.y + b.h).clamp(y, PAGE_H) - y,
            ))
        } else {
            Some(b)
        }
    }
    pub fn selected_resize_handles(&self) -> Vec<(diagram::ResizeHandle, Point)> {
        let Some((_, _, uniform)) = self.selected_resize_shape() else {
            return vec![];
        };
        let Some(b) = self.selection_frame() else {
            return vec![];
        };
        diagram::ResizeHandle::ALL
            .into_iter()
            .filter(|handle| {
                let (x, y) = handle.axes();
                // Keep tiny objects usable: omit edge handles that overlap corners.
                (x != 0 && y != 0)
                    || (!uniform
                        && if x == 0 {
                            b.w * self.view.zoom >= 60.
                        } else {
                            b.h * self.view.zoom >= 60.
                        })
            })
            .map(|handle| (handle, handle.point(b)))
            .collect()
    }
    pub(super) fn resize_hit(&self, p: Point) -> Option<diagram::ResizeHandle> {
        self.selected_resize_handles()
            .into_iter()
            .map(|(handle, q)| (handle, (q[0] - p[0]).hypot(q[1] - p[1]) * self.view.zoom))
            .filter(|(_, d)| *d <= 15.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(h, _)| h)
    }
    /// Endpoints are derived from the first two points, including diagrams saved by 0.6.0.
    pub fn selected_endpoints(&self) -> Option<[Point; 2]> {
        if self.tool != "select"
            || self.selection.strokes.len() != 1
            || !self.selection.texts.is_empty()
        {
            return None;
        }
        diagram::endpoints(
            self.page()?
                .payload
                .strokes
                .get(self.selection.strokes[0])?,
        )
    }
    pub(super) fn endpoint_hit(&self, p: Point) -> Option<usize> {
        self.selected_endpoints()?
            .iter()
            .enumerate()
            .map(|(i, q)| (i, (q[0] - p[0]).hypot(q[1] - p[1]) * self.view.zoom))
            .filter(|(_, distance)| *distance <= 15.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
    pub(super) fn begin_select(&mut self, p: Point) {
        let endpoint = self.endpoint_hit(p);
        let kind = if let Some(end) = endpoint {
            EditKind::Endpoint {
                stroke: self.selection.strokes[0],
                end,
            }
        } else if let (Some(handle), Some((stroke, bounds, uniform))) =
            (self.resize_hit(p), self.selected_resize_shape())
        {
            EditKind::Resize {
                stroke,
                handle,
                bounds,
                uniform,
            }
        } else {
            let Some(page) = self.page() else {
                return;
            };
            let tolerance = 10. / self.view.zoom;
            let text = page.payload.texts.iter().rposition(|t| {
                let b = self.surface.text_bounds(t);
                Box2::new(
                    b.x - tolerance / 2.,
                    b.y - tolerance / 2.,
                    b.w + tolerance,
                    b.h + tolerance,
                )
                .contains(p[0], p[1])
            });
            let stroke = if text.is_none() {
                // Test ink before empty shape interiors so lines inside boxes remain reachable.
                page.payload
                    .strokes
                    .iter()
                    .rposition(|s| {
                        let radius =
                            tolerance + s.width * if s.tool == "highlighter" { 3.5 } else { 1.5 };
                        s.points
                            .first()
                            .is_some_and(|&q| distance(p, q, q) <= radius)
                            || s.points
                                .windows(2)
                                .any(|pair| distance(p, pair[0], pair[1]) <= radius)
                    })
                    .or_else(|| {
                        page.payload.strokes.iter().rposition(|s| {
                            diagram::shape(s).is_some_and(|shape| shape.closed())
                                && inside(p, &s.points)
                        })
                    })
            } else {
                None
            };
            self.selection = Selection {
                strokes: stroke.into_iter().collect(),
                texts: text.into_iter().collect(),
                bounds: None,
            };
            EditKind::Move
        };
        self.start_object_edit(p, kind);
    }
    pub(super) fn begin_selection_move(&mut self, p: Point) {
        self.start_object_edit(p, EditKind::Move);
    }
    fn start_object_edit(&mut self, p: Point, kind: EditKind) {
        self.selection_bounds();
        self.redraw = true;
        let Some(bounds) = self.selection.bounds else {
            self.gesture = Some(Gesture::SelectEmpty);
            return;
        };
        if let Some(page) = self.page() {
            self.gesture = Some(Gesture::ObjectEdit(Box::new(ObjectEdit {
                original: page.payload.clone(),
                start: p,
                kind,
                dragging: false,
                bounds,
                background: None,
                first_stroke: self
                    .selection
                    .strokes
                    .iter()
                    .copied()
                    .min()
                    .unwrap_or(page.payload.strokes.len()),
                first_text: if self.selection.strokes.is_empty() {
                    self.selection
                        .texts
                        .iter()
                        .copied()
                        .min()
                        .unwrap_or(page.payload.texts.len())
                } else {
                    0
                },
            })));
        }
    }
    pub(super) fn move_object(&mut self, edit: &mut ObjectEdit, p: Point) {
        let delta = ((p[0] - edit.start[0]) * self.view.zoom)
            .hypot((p[1] - edit.start[1]) * self.view.zoom);
        if !edit.dragging && delta < 3. {
            return;
        }
        edit.dragging = true;
        let old = self.selection_region();
        let Some(page) = self.pages.get_mut(self.index) else {
            return;
        };
        match edit.kind {
            EditKind::Resize {
                stroke,
                handle,
                bounds,
                uniform,
            } => {
                if let (Some(original), Some(target)) = (
                    edit.original.strokes.get(stroke),
                    page.payload.strokes.get_mut(stroke),
                ) {
                    let next = handle.resized(
                        bounds,
                        p[0] - edit.start[0],
                        p[1] - edit.start[1],
                        uniform,
                        4. / self.view.zoom,
                    );
                    if let Some(updated) = diagram::with_bounds(original, next) {
                        *target = updated;
                    }
                }
            }
            EditKind::Move => {
                let b = edit.bounds;
                let dx = (p[0] - edit.start[0]).clamp((-b.x).min(0.), (PAGE_W - b.x - b.w).max(0.));
                let dy = (p[1] - edit.start[1]).clamp((-b.y).min(0.), (PAGE_H - b.y - b.h).max(0.));
                for &i in &self.selection.strokes {
                    if let (Some(target), Some(original)) = (
                        page.payload.strokes.get_mut(i),
                        edit.original.strokes.get(i),
                    ) {
                        for (q, source) in target.points.iter_mut().zip(&original.points) {
                            q[0] = source[0] + dx;
                            q[1] = source[1] + dy;
                        }
                    }
                }
                for &i in &self.selection.texts {
                    if let (Some(target), Some(original)) =
                        (page.payload.texts.get_mut(i), edit.original.texts.get(i))
                    {
                        target.x = original.x + dx;
                        target.y = original.y + dy;
                    }
                }
            }
            EditKind::Endpoint { stroke, end } => {
                if let Some(original) = edit.original.strokes.get(stroke) {
                    if let Some(ends) = diagram::endpoints(original) {
                        // Preserve where inside the handle the user grabbed it; no jump on contact.
                        let q = [
                            (ends[end][0] + p[0] - edit.start[0]).clamp(0., PAGE_W),
                            (ends[end][1] + p[1] - edit.start[1]).clamp(0., PAGE_H),
                            0.5,
                        ];
                        if (q[0] - ends[1 - end][0]).hypot(q[1] - ends[1 - end][1]) * self.view.zoom
                            >= 2.
                        {
                            if let Some(updated) = diagram::with_endpoint(original, end, q) {
                                if let Some(target) = page.payload.strokes.get_mut(stroke) {
                                    *target = updated;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.selection_bounds();
        if !self.redraw {
            if let (Some(old), Some(new)) = (old, self.selection_region()) {
                let x = old.x.min(new.x);
                let y = old.y.min(new.y);
                let region = Box2::new(
                    x,
                    y,
                    (old.x + old.w).max(new.x + new.w) - x,
                    (old.y + old.h).max(new.y + new.h) - y,
                );
                if let Some(page) = self.pages.get(self.index) {
                    let background = edit.background.get_or_insert_with(|| {
                        self.surface.object_background(
                            self.view,
                            &page.payload,
                            self.pdf.as_ref(),
                            edit.first_stroke,
                            edit.first_text,
                        )
                    });
                    let damage = self.surface.repaint_objects(
                        self.view,
                        &page.payload,
                        background,
                        edit.first_stroke,
                        edit.first_text,
                        region,
                    );
                    ui::selection_overlay(self);
                    self.dirty(damage);
                }
            }
        }
    }
    fn selection_region(&self) -> Option<Box2> {
        let b = self.selection.bounds?;
        let v = self.view;
        // Extra display-space padding covers endpoint handles at every zoom level.
        Some(Box2::new(
            v.x + b.x * v.zoom - 18.,
            v.y + b.y * v.zoom - 18.,
            b.w * v.zoom + 36.,
            b.h * v.zoom + 36.,
        ))
    }
}
