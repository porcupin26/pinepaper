use pinepaper::{
    app::{Action, App},
    diagram::{self, ResizeHandle as Handle, Shape},
    model::*,
    render::Box2,
};

fn fixture(shape: Shape) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    let mut page = Payload::blank("Grid");
    let mut stroke = shape.stroke([180., 200., 0.5], [380., 400., 0.5], 2.4, 0.35);
    stroke
        .extra
        .insert("pinepaper_object_id".into(), serde_json::json!("resize-id"));
    stroke
        .extra
        .insert("future-field".into(), serde_json::json!({"keep":true}));
    page.strokes.push(stroke);
    let id = a
        .store
        .create("Resize", "notebook", None, vec![page], None)
        .unwrap();
    a.open(&id).unwrap();
    a.resize(702, 936, 2);
    select(&mut a);
    (dir, a)
}
fn select(a: &mut App) {
    a.act(Action::Tool("select".into()));
    a.render();
    let p = a.view.screen([280., 300., 0.5]);
    a.down(p[0], p[1], 0.5, false);
    a.up(p[0], p[1]);
    a.render();
}
fn handle(a: &App, h: Handle) -> Point {
    a.selected_resize_handles()
        .into_iter()
        .find(|(kind, _)| *kind == h)
        .unwrap()
        .1
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}

#[test]
fn resize_handles_keep_opposite_edges_fixed_and_constrain_shapes_to_paper() {
    for uniform in [false, true] {
        for h in Handle::ALL {
            let (sx, sy) = h.axes();
            if uniform && (sx == 0 || sy == 0) {
                continue;
            }
            let b = Box2::new(180., 200., 200., 200.);
            let n = h.resized(b, sx as f64 * 40., sy as f64 * 20., uniform, 4.);
            if sx < 0 {
                near(n.x + n.w, b.x + b.w);
            } else {
                near(n.x, b.x);
            }
            if sy < 0 {
                near(n.y + n.h, b.y + b.h);
            } else {
                near(n.y, b.y);
            }
            if uniform {
                near(n.w, 230.);
                near(n.h, 230.);
            } else {
                near(n.w, if sx != 0 { 240. } else { 200. });
                near(n.h, if sy != 0 { 220. } else { 200. });
            }
            for distance in [-10000., 10000.] {
                let n = h.resized(b, sx as f64 * distance, sy as f64 * distance, uniform, 4.);
                assert!(
                    n.x >= 0.
                        && n.y >= 0.
                        && n.x + n.w <= PAGE_W + 1e-8
                        && n.y + n.h <= PAGE_H + 1e-8
                );
                assert!(n.w >= 4. && n.h >= 4.);
                if uniform {
                    near(n.w, n.h);
                }
            }
        }
    }
    // Tiny/degenerate legacy shapes can return to their original geometry exactly.
    let b = Box2::new(0., 0., 0., 200.);
    assert_eq!(Handle::SouthEast.resized(b, 0., 0., false, 4.), b);
    assert!(Handle::East.resized(b, 30., 0., false, 4.).w > 0.);
}

#[test]
fn all_closed_shapes_resize_from_corners_preserving_style_identity_and_undo() {
    for shape in [
        Shape::Rectangle,
        Shape::Square,
        Shape::Ellipse,
        Shape::Circle,
        Shape::Diamond,
    ] {
        let (_dir, mut a) = fixture(shape);
        let uniform = matches!(shape, Shape::Square | Shape::Circle);
        assert_eq!(
            a.selected_resize_handles().len(),
            if uniform { 4 } else { 8 }
        );
        assert!(a.selected_endpoints().is_none());
        let original = a.page().unwrap().payload.strokes[0].clone();
        let p = a.view.screen(handle(&a, Handle::SouthEast));
        a.down(p[0], p[1], 0.5, true);
        a.render();
        a.queue_motion(p[0] + 45., p[1] + 25., 0.5);
        a.render();
        assert!(!a.redraw);
        a.up(p[0] + 60., p[1] + 40.); // Final position wins over the preceding preview.
        let resized = a.page().unwrap().payload.strokes[0].clone();
        let b = diagram::bounds(&resized).unwrap();
        near(b.x, 180.);
        near(b.y, 200.);
        near(b.w, 200. + if uniform { 50. } else { 60. } / a.view.zoom);
        near(b.h, 200. + if uniform { 50. } else { 40. } / a.view.zoom);
        assert_eq!(resized.extra, original.extra);
        assert_eq!(resized.width, original.width);
        assert_eq!(resized.color, original.color);
        assert_eq!(resized.pen, original.pen);
        a.act(Action::Undo);
        assert_eq!(a.page().unwrap().payload.strokes[0], original);
        a.act(Action::Redo);
        assert_eq!(a.page().unwrap().payload.strokes[0], resized);
        a.saver.flush().unwrap();
        let id = a.doc.as_ref().unwrap().id.clone();
        let saved: Payload =
            serde_json::from_str(&serde_json::to_string(&a.page().unwrap().payload).unwrap())
                .unwrap();
        assert_eq!(a.store.pages(&id).unwrap()[0].payload, saved);
    }
}

#[test]
fn edge_handles_resize_one_axis_and_keep_screen_sized_grab_offsets_after_rotation() {
    let (_dir, mut a) = fixture(Shape::Rectangle);
    for (w, h, scale, fit) in [
        (702, 936, 2, false),
        (936, 702, 1, true),
        (936, 702, 2, false),
    ] {
        a.fit = fit;
        a.resize(w, h, scale);
        a.render();
        let b = diagram::bounds(&a.page().unwrap().payload.strokes[0]).unwrap();
        let p = a.view.screen(handle(&a, Handle::East));
        a.down(p[0] + 9., p[1], 0.5, false);
        a.render();
        a.up(p[0] + 29., p[1] + 20.);
        let n = diagram::bounds(&a.page().unwrap().payload.strokes[0]).unwrap();
        near(n.x, b.x);
        near(n.y, b.y);
        near(n.h, b.h);
        near(n.w, b.w + 20. / a.view.zoom);
    }
}

#[test]
fn taps_and_out_and_back_resize_do_not_change_reversed_shapes_or_history() {
    let (_dir, mut a) = fixture(Shape::Ellipse);
    a.pages[0].payload.strokes[0] =
        Shape::Ellipse.stroke([380., 400., 0.5], [180., 200., 0.5], 2.4, 0.35);
    select(&mut a);
    let original = a.page().unwrap().payload.clone();
    let p = a.view.screen(handle(&a, Handle::NorthWest));
    a.down(p[0], p[1], 0.5, false);
    a.up(p[0] + 1., p[1] + 1.);
    assert_eq!(a.page().unwrap().payload, original);
    assert!(!a.action_available(&Action::Undo));
    a.down(p[0], p[1], 0.5, false);
    a.render();
    a.queue_motion(p[0] - 30., p[1] - 30., 0.5);
    a.render();
    a.up(p[0], p[1]);
    assert_eq!(a.page().unwrap().payload, original);
    assert!(!a.action_available(&Action::Undo));
}

#[test]
fn resize_preview_preserves_upper_ink_text_tone_and_roundtrips_to_pdf() {
    let (dir, mut a) = fixture(Shape::Ellipse);
    let mut stroke = Shape::Arrow.stroke([220., 260., 0.5], [500., 420., 0.5], 4., 0.2);
    stroke.tool = "highlighter".into();
    a.pages[0].payload.strokes.push(stroke);
    a.pages[0].payload.texts.push(Text {
        text: "Keep overlapping text".into(),
        x: 230.,
        y: 240.,
        size: 20.,
        font: "Sans".into(),
        color: 0.25,
        extra: Default::default(),
    });
    a.prefs.page_contrast = 1.4;
    a.prefs.page_gamma = 0.8;
    a.redraw = true;
    a.render();
    let p = a.view.screen(handle(&a, Handle::SouthEast));
    a.down(p[0], p[1], 0.5, false);
    a.render();
    for (x, y) in [(40., 70.), (-80., -50.), (110., 40.)] {
        a.queue_motion(p[0] + x, p[1] + y, 0.5);
        a.render();
    }
    let preview = a.surface.pixels.clone();
    a.redraw = true;
    a.render();
    assert!(
        preview == a.surface.pixels,
        "Cached resize must match a full repaint"
    );
    a.up(p[0] + 110., p[1] + 40.);
    let result = a.page().unwrap().payload.clone();
    pinepaper::document::export_pdf(&dir.path().join("resized.pdf"), &a.pages, None).unwrap();
    a.saver.flush().unwrap();
    let id = a.doc.as_ref().unwrap().id.clone();
    a.open(&id).unwrap();
    let saved: Payload = serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(a.page().unwrap().payload, saved);
}

#[test]
fn page_edge_handles_stay_reachable_and_tiny_shapes_omit_overlapping_midpoints() {
    let (_dir, mut a) = fixture(Shape::Rectangle);
    a.pages[0].payload.strokes[0] =
        Shape::Rectangle.stroke([0., 0., 0.5], [100., 100., 0.5], 2., 0.3);
    a.redraw = true;
    a.render();
    let p = a.view.screen([50., 50., 0.5]);
    a.down(p[0], p[1], 0.5, false);
    a.up(p[0], p[1]);
    a.render();
    let corner = handle(&a, Handle::NorthWest);
    near(corner[0], 0.);
    near(corner[1], 0.);
    let p = a.view.screen(corner);
    a.down(p[0] - 3., p[1], 0.5, false);
    a.render();
    a.up(p[0] + 17., p[1] + 20.);
    let b = diagram::bounds(&a.page().unwrap().payload.strokes[0]).unwrap();
    near(b.x, 20. / a.view.zoom);
    near(b.y, 20. / a.view.zoom);
    near(b.x + b.w, 100.);
    near(b.y + b.h, 100.);
    a.pages[0].payload.strokes[0] =
        Shape::Rectangle.stroke([260., 280., 0.5], [300., 320., 0.5], 0.5, 0.3);
    select(&mut a);
    assert_eq!(a.selected_resize_handles().len(), 4);
}
