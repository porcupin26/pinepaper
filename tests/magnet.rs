use pinepaper::{
    app::{Action, App},
    diagram::{self, ResizeHandle as H, Shape},
    model::*,
};

fn fixture(shapes: Vec<Stroke>) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    let mut page = Payload::blank("Grid");
    page.strokes = shapes;
    let id = a
        .store
        .create("Magnet", "notebook", None, vec![page], None)
        .unwrap();
    a.open(&id).unwrap();
    a.resize(702, 936, 2);
    a.act(Action::Tool("select".into()));
    a.render();
    (dir, a)
}
fn shape(s: Shape, a: (f64, f64), b: (f64, f64)) -> Stroke {
    let mut s = s.stroke([a.0, a.1, 0.5], [b.0, b.1, 0.5], 2., 0.15);
    s.extra.insert(
        "pinepaper_object_id".into(),
        serde_json::json!(format!("{}-{}", a.0, a.1)),
    );
    s
}
fn tap(a: &mut App, p: Point) {
    let q = a.view.screen(p);
    a.down(q[0], q[1], 0.5, false);
    a.up(q[0], q[1]);
    a.render();
}
fn begin(a: &mut App, p: Point) {
    let q = a.view.screen(p);
    a.down(q[0], q[1], 0.5, false);
    a.render();
}
fn motion(a: &mut App, p: Point) {
    let q = a.view.screen(p);
    a.queue_motion(q[0], q[1], 0.5);
    a.render();
}
fn release(a: &mut App, p: Point) {
    let q = a.view.screen(p);
    a.up(q[0], q[1]);
    a.render();
}
fn near(a: Point, b: Point) {
    assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-7, "{a:?} != {b:?}");
}
fn exact_preview(a: &mut App) {
    let pixels = a.surface.pixels.clone();
    a.redraw = true;
    a.render();
    // Saving/status text may change independently; compare the complete page viewport.
    let r = a.surface.rect(a.view.clip);
    for y in r.y..r.bottom {
        let row = y * a.surface.width + r.x..y * a.surface.width + r.right;
        assert!(
            pixels[row.clone()] == a.surface.pixels[row],
            "Cached preview must equal full render, including marker cleanup"
        );
    }
}
#[test]
fn anchors_are_geometric_not_sampled_ink_or_arrowheads() {
    for kind in Shape::ALL {
        let s = shape(kind, (180., 400.), (100., 200.));
        let anchors = diagram::anchors(&s);
        let n = match kind {
            Shape::Line | Shape::Arrow | Shape::DoubleArrow => 2,
            Shape::Rectangle | Shape::Square | Shape::Diamond => 8,
            _ => 4,
        };
        assert_eq!(anchors.len(), n);
        if let Some(ends) = diagram::endpoints(&s) {
            assert_eq!(anchors, ends);
        } else {
            assert!(anchors
                .iter()
                .all(|p| s.points.windows(2).any(|q| distance(*p, q[0], q[1]) < 1e-7)));
        }
    }
    assert!(diagram::anchors(&Stroke::default()).is_empty());
}
#[test]
fn toggle_is_visible_in_both_orientations_persists_and_preserves_selection() {
    let (dir, mut a) = fixture(vec![shape(Shape::Rectangle, (150., 200.), (350., 400.))]);
    assert!(!a.magnet);
    for (w, h) in [(702, 936), (936, 702)] {
        a.resize(w, h, 2);
        a.render();
        tap(&mut a, [250., 300., 0.5]);
        let saved = a.page().unwrap().payload.clone();
        let b = a.hits.iter().find(|(_, x)| *x == Action::Magnet).unwrap().0;
        assert!(b.x >= 0. && b.x + b.w <= w as f64);
        a.down(b.x + b.w / 2., b.y + b.h / 2., 0.5, true);
        a.up(b.x + b.w / 2., b.y + b.h / 2.);
        a.render();
        assert_eq!(a.selection.strokes, vec![0]);
        assert_eq!(a.tool, "select");
        assert_eq!(a.page().unwrap().payload, saved);
        assert!(!a.action_available(&Action::Undo));
    }
    a.act(Action::Magnet);
    a.saver.flush().unwrap();
    drop(a);
    let mut a = App::new(dir.path().into()).unwrap();
    assert!(a.magnet);
    a.resize(702, 936, 2);
    a.render();
    assert!(!a.hits.iter().any(|(_, x)| *x == Action::Magnet));
}
#[test]
fn endpoint_snapping_obeys_toggle_release_undo_identity_and_persistence() {
    let (_dir, mut a) = fixture(vec![
        shape(Shape::DoubleArrow, (100., 300.), (300., 300.)),
        shape(Shape::Rectangle, (450., 200.), (650., 400.)),
    ]);
    tap(&mut a, [200., 300., 0.5]);
    let original = a.page().unwrap().payload.clone();
    begin(&mut a, [300., 300., 0.5]);
    release(&mut a, [444., 305., 0.5]);
    near(
        diagram::endpoints(&a.page().unwrap().payload.strokes[0]).unwrap()[1],
        [444., 305., 0.5],
    );
    a.act(Action::Undo);
    a.act(Action::Magnet);
    a.render();
    tap(&mut a, [200., 300., 0.5]);
    begin(&mut a, [300., 300., 0.5]);
    motion(&mut a, [444., 305., 0.5]);
    near(a.snap_target.unwrap(), [450., 300., 0.5]);
    exact_preview(&mut a);
    // Final release wins over a coalesced, unrendered intermediate position.
    let stale = a.view.screen([600., 600., 0.5]);
    a.queue_motion(stale[0], stale[1], 0.5);
    release(&mut a, [445., 301., 0.5]);
    assert!(a.snap_target.is_none());
    assert!(!a.redraw);
    exact_preview(&mut a);
    let saved = a.page().unwrap().payload.clone();
    let ends = diagram::endpoints(&saved.strokes[0]).unwrap();
    near(ends[0], [100., 300., 0.5]);
    near(ends[1], [450., 300., 0.5]);
    assert_eq!(saved.strokes[0].extra, original.strokes[0].extra);
    assert_eq!(saved.strokes[1], original.strokes[1]);
    a.act(Action::Undo);
    assert_eq!(a.page().unwrap().payload, original);
    a.act(Action::Redo);
    assert_eq!(a.page().unwrap().payload, saved);
    a.saver.flush().unwrap();
    let id = a.doc.as_ref().unwrap().id.clone();
    a.open(&id).unwrap();
    let persisted: Payload = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
    assert_eq!(a.page().unwrap().payload, persisted);
}
#[test]
fn moving_shapes_and_lasso_groups_snap_by_translation_without_self_attraction() {
    let (_dir, mut a) = fixture(vec![
        shape(Shape::Rectangle, (150., 200.), (250., 300.)),
        shape(Shape::Arrow, (170., 230.), (230., 270.)),
        shape(Shape::Ellipse, (400., 300.), (600., 500.)),
    ]);
    a.act(Action::Magnet);
    a.act(Action::Tool("lasso".into()));
    a.render();
    begin(&mut a, [120., 170., 0.5]);
    for p in [
        [280., 170., 0.5],
        [280., 330., 0.5],
        [120., 330., 0.5],
        [120., 170., 0.5],
    ] {
        motion(&mut a, p);
    }
    release(&mut a, [120., 170., 0.5]);
    assert_eq!(a.selection.strokes, vec![0, 1]);
    let original = a.page().unwrap().payload.clone();
    begin(&mut a, [200., 250., 0.5]);
    motion(&mut a, [345., 345., 0.5]);
    near(a.snap_target.unwrap(), [400., 400., 0.5]);
    release(&mut a, [345., 345., 0.5]);
    for i in [0, 1] {
        for (p, q) in a.page().unwrap().payload.strokes[i]
            .points
            .iter()
            .zip(&original.strokes[i].points)
        {
            near(*p, [q[0] + 150., q[1] + 100., q[2]]);
        }
    }
    assert_eq!(a.page().unwrap().payload.strokes[2], original.strokes[2]);
    a.act(Action::Undo);
    a.pages[0].payload.strokes.truncate(2);
    a.act(Action::Tool("select".into()));
    a.render();
    tap(&mut a, [160., 210., 0.5]);
    begin(&mut a, [160., 210., 0.5]);
    release(&mut a, [165., 215., 0.5]);
    near(
        a.page().unwrap().payload.strokes[0].points[0],
        [155., 205., 0.5],
    );
}
#[test]
fn resize_snapping_preserves_opposite_corner_and_uniform_shapes() {
    for kind in [
        Shape::Rectangle,
        Shape::Ellipse,
        Shape::Square,
        Shape::Circle,
        Shape::Diamond,
    ] {
        let (_dir, mut a) = fixture(vec![
            shape(kind, (150., 200.), (350., 400.)),
            shape(Shape::Line, (450., 500.), (600., 550.)),
        ]);
        a.act(Action::Magnet);
        a.render();
        tap(&mut a, [250., 300., 0.5]);
        let p = a
            .selected_resize_handles()
            .into_iter()
            .find(|(h, _)| *h == H::SouthEast)
            .unwrap()
            .1;
        begin(&mut a, p);
        let end = [p[0] + 95., p[1] + 95., 0.5];
        motion(&mut a, end);
        near(a.snap_target.unwrap(), [450., 500., 0.5]);
        exact_preview(&mut a);
        release(&mut a, end);
        let b = diagram::bounds(&a.page().unwrap().payload.strokes[0]).unwrap();
        near([b.x, b.y, 0.5], [150., 200., 0.5]);
        near([b.x + b.w, b.y + b.h, 0.5], [450., 500., 0.5]);
        exact_preview(&mut a);
    }
}
#[test]
fn page_limits_take_priority_and_hover_never_snaps() {
    let (_dir, mut a) = fixture(vec![
        shape(Shape::Rectangle, (150., 200.), (350., 400.)),
        shape(Shape::Line, (PAGE_W, 400.), (PAGE_W, 600.)),
    ]);
    a.act(Action::Magnet);
    a.render();
    tap(&mut a, [250., 300., 0.5]);
    begin(&mut a, [250., 300., 0.5]);
    motion(&mut a, [PAGE_W - 100., 300., 0.5]);
    assert!(
        a.snap_target.is_none(),
        "Padded selection must stay on paper"
    );
    release(&mut a, [PAGE_W - 100., 300., 0.5]);
    let saved = a.page().unwrap().payload.clone();
    motion(&mut a, [500., 500., 0.5]);
    assert_eq!(a.page().unwrap().payload, saved);
    assert!(a.snap_target.is_none());
}

#[test]
fn every_polygon_side_midpoint_accepts_an_arrow_endpoint() {
    for kind in [Shape::Rectangle, Shape::Square, Shape::Diamond] {
        let target = shape(kind, (350., 300.), (550., 500.));
        for edge in target.points.windows(2) {
            let midpoint = [
                (edge[0][0] + edge[1][0]) / 2.,
                (edge[0][1] + edge[1][1]) / 2.,
                0.5,
            ];
            let (_dir, mut a) = fixture(vec![
                shape(Shape::Arrow, (100., 150.), (200., 200.)),
                target.clone(),
            ]);
            a.act(Action::Magnet);
            a.render();
            tap(&mut a, [150., 175., 0.5]);
            begin(&mut a, [200., 200., 0.5]);
            release(&mut a, [midpoint[0] - 4., midpoint[1] + 3., 0.5]);
            near(
                diagram::endpoints(&a.page().unwrap().payload.strokes[0]).unwrap()[1],
                midpoint,
            );
        }
    }
}
