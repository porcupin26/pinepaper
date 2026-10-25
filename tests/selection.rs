use pinepaper::{
    app::{Action, App},
    diagram::{self, Shape},
    model::*,
    render::Box2,
};
fn fixture(strokes: Vec<Stroke>) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    let mut page = Payload::blank("Grid");
    page.strokes = strokes;
    let id = a
        .store
        .create("Select test", "notebook", None, vec![page], None)
        .unwrap();
    a.open(&id).unwrap();
    a.resize(702, 936, 2);
    a.act(Action::Tool("select".into()));
    a.render();
    (dir, a)
}
fn touch(a: &mut App, p: Point) {
    let q = a.view.screen(p);
    a.down(q[0], q[1], 0.5, false);
    a.up(q[0], q[1]);
    a.render();
}
fn drag(a: &mut App, start: Point, end: Point, finger: bool) {
    let start = a.view.screen(start);
    let end = a.view.screen(end);
    a.down(start[0], start[1], 0.5, finger);
    a.render();
    a.motion(end[0], end[1], 0.5);
    a.up(end[0], end[1]);
}
fn ink(x: f64, y: f64) -> Stroke {
    Stroke {
        points: vec![[x, y, 0.5], [x + 100., y + 40., 0.5]],
        ..Stroke::default()
    }
}
#[test]
fn select_moves_shapes_from_their_interior_and_preserves_noop_history() {
    for shape in [
        Shape::Rectangle,
        Shape::Square,
        Shape::Circle,
        Shape::Ellipse,
        Shape::Diamond,
    ] {
        let s = shape.stroke([150., 200., 0.5], [350., 400., 0.5], 2.4, 0.07);
        let (_dir, mut a) = fixture(vec![s.clone()]);
        assert!(a
            .hits
            .iter()
            .any(|(_, action)| *action == Action::Tool("select".into())));
        touch(&mut a, [250., 300., 0.5]);
        assert_eq!(a.selection.strokes, vec![0]);
        assert!(a.selected_endpoints().is_none());
        assert!(!a.action_available(&Action::Undo));
        drag(&mut a, [250., 300., 0.5], [300., 340., 0.5], false);
        assert!(
            !a.redraw,
            "Movement should damage the affected region, not rebuild the UI"
        );
        let moved = &a.page().unwrap().payload.strokes[0];
        assert!((moved.points[0][0] - s.points[0][0] - 50.).abs() < 1e-8);
        assert!((moved.points[0][1] - s.points[0][1] - 40.).abs() < 1e-8);
        a.act(Action::Undo);
        assert!(a.action_available(&Action::Redo));
        a.render();
        touch(&mut a, [250., 300., 0.5]);
        touch(&mut a, [650., 800., 0.5]);
        assert!(a.selection.strokes.is_empty());
        assert!(a.action_available(&Action::Redo));
        a.act(Action::Redo);
        assert!(a.action_available(&Action::Undo));
    }
}
#[test]
fn select_picks_ink_before_shape_interiors_and_text_in_visual_order() {
    let rectangle = Shape::Rectangle.stroke([100., 100., 0.5], [500., 500., 0.5], 2., 0.07);
    let (_dir, mut a) = fixture(vec![ink(200., 200.), rectangle]);
    touch(&mut a, [250., 220., 0.5]);
    assert_eq!(a.selection.strokes, vec![0]);
    a.pages[0].payload.strokes.push(ink(200., 200.));
    a.redraw = true;
    a.render();
    touch(&mut a, [250., 220., 0.5]);
    assert_eq!(a.selection.strokes, vec![2]);
    a.pages[0].payload.texts.push(Text {
        text: "Move this\nsecond line".into(),
        x: 200.,
        y: 200.,
        size: 22.,
        font: "Sans".into(),
        color: 0.07,
        extra: Default::default(),
    });
    a.redraw = true;
    a.render();
    touch(&mut a, [240., 210., 0.5]);
    assert!(a.selection.strokes.is_empty());
    assert_eq!(a.selection.texts, vec![0]);
    drag(&mut a, [240., 210., 0.5], [290., 260., 0.5], false);
    assert!((a.page().unwrap().payload.texts[0].x - 250.).abs() < 1e-8);
    assert!((a.page().unwrap().payload.texts[0].y - 250.).abs() < 1e-8);
    a.act(Action::DeleteSelection);
    assert!(a.page().unwrap().payload.texts.is_empty());
    a.act(Action::Undo);
    assert_eq!(a.page().unwrap().payload.texts.len(), 1);
}
#[test]
fn endpoints_drag_independently_regenerate_arrowheads_and_preserve_identity() {
    for shape in [Shape::Line, Shape::Arrow, Shape::DoubleArrow] {
        let mut s = shape.stroke([150., 300., 0.5], [450., 300., 0.5], 4., 0.45);
        s.extra
            .insert("pinepaper_object_id".into(), serde_json::json!("stable-id"));
        s.extra
            .insert("future-field".into(), serde_json::json!({"keep":true}));
        let (dir, mut a) = fixture(vec![s.clone()]);
        touch(&mut a, [300., 300., 0.5]);
        assert_eq!(a.selected_endpoints().unwrap(), [s.points[0], s.points[1]]);
        for (end, target) in [(0, [100., 180., 0.5]), (1, [520., 450., 0.5])] {
            let original = a.page().unwrap().payload.strokes[0].clone();
            let ends = diagram::endpoints(&original).unwrap();
            drag(&mut a, ends[end], target, false);
            let result = &a.page().unwrap().payload.strokes[0];
            assert_eq!(diagram::endpoints(result).unwrap()[1 - end], ends[1 - end]);
            let updated = diagram::endpoints(result).unwrap()[end];
            assert!((updated[0] - target[0]).hypot(updated[1] - target[1]) < 1e-8);
            assert_eq!(result.extra, s.extra);
            assert_eq!(result.width, s.width);
            assert_eq!(result.color, s.color);
            let geometry = shape.stroke(result.points[0], result.points[1], s.width, s.color);
            assert_eq!(result.points, geometry.points);
            a.act(Action::Undo);
            assert_eq!(a.page().unwrap().payload.strokes[0], original);
            a.act(Action::Redo);
            a.render();
            let center = [
                (result_point(&a, 0)[0] + result_point(&a, 1)[0]) / 2.,
                (result_point(&a, 0)[1] + result_point(&a, 1)[1]) / 2.,
                0.5,
            ];
            touch(&mut a, center);
        }
        a.saver.flush().unwrap();
        let id = a.doc.as_ref().unwrap().id.clone();
        let expected = serde_json::to_value(&a.store.pages(&id).unwrap()[0].payload).unwrap();
        pinepaper::document::export_pdf(&dir.path().join("edited-arrow.pdf"), &a.pages, None)
            .unwrap();
        a.open(&id).unwrap();
        assert_eq!(
            serde_json::to_value(&a.page().unwrap().payload).unwrap(),
            expected
        );
    }
}
fn result_point(a: &App, index: usize) -> Point {
    a.page().unwrap().payload.strokes[0].points[index]
}
#[test]
fn body_moves_both_endpoints_fingers_work_and_handles_follow_rotation() {
    let (_dir, mut a) = fixture(vec![Shape::DoubleArrow.stroke(
        [180., 300., 0.5],
        [480., 300., 0.5],
        2.4,
        0.07,
    )]);
    assert!(!a.prefs.finger_draw);
    drag(&mut a, [330., 300., 0.5], [370., 350., 0.5], true);
    assert_eq!(
        a.selected_endpoints().unwrap(),
        [[220., 350., 0.5], [520., 350., 0.5]]
    );
    for (w, h, scale, fit) in [
        (936, 702, 1, true),
        (702, 936, 2, false),
        (936, 702, 2, false),
    ] {
        a.fit = fit;
        a.resize(w, h, scale);
        a.render();
        let ends = a.selected_endpoints().unwrap();
        let q = a.view.screen(ends[1]);
        // Grab near the edge of the screen-sized handle, not just its exact center.
        a.down(q[0] + 9., q[1], 0.5, false);
        a.render();
        a.up(q[0] + 29., q[1] + 20.);
        let next = a.selected_endpoints().unwrap();
        assert_eq!(next[0], ends[0]);
        assert!((next[1][0] - ends[1][0] - 20. / a.view.zoom).abs() < 1e-8);
        assert!((next[1][1] - ends[1][1] - 20. / a.view.zoom).abs() < 1e-8);
        a.render();
    }
}
#[test]
fn edit_preview_preserves_overlap_tone_and_matches_full_render_without_ghosts() {
    let mut highlight = ink(200., 260.);
    highlight.tool = "highlighter".into();
    highlight.width = 4.;
    let (_dir, mut a) = fixture(vec![
        Shape::Arrow.stroke([150., 300., 0.5], [450., 300., 0.5], 4., 0.45),
        highlight,
    ]);
    a.prefs.page_contrast = 1.4;
    a.prefs.page_gamma = 0.8;
    a.redraw = true;
    a.render();
    touch(&mut a, [380., 300., 0.5]);
    let p = a.view.screen([450., 300., 0.5]);
    a.down(p[0], p[1], 0.5, false);
    a.render();
    for point in [
        [450., 340., 0.5],
        [600., 480., 0.5],
        [280., 180., 0.5],
        [450., 300., 0.5],
    ] {
        let q = a.view.screen(point);
        a.damage = None;
        a.motion(q[0], q[1], 0.5);
        assert!(!a.redraw);
        let damage = a.damage.unwrap();
        assert!(damage.x >= a.view.clip.x as usize * 2);
        assert!(damage.y >= a.view.clip.y as usize * 2);
        let preview = a.surface.pixels.clone();
        a.redraw = true;
        a.render();
        assert!(
            preview == a.surface.pixels,
            "Partial preview must match a complete repaint"
        );
    }
    a.up(p[0], p[1]);
    assert!(
        !a.action_available(&Action::Undo),
        "A drag back to the original geometry is a no-op"
    );
    // New ink after the partial repaint must still use the correct untinted background.
    a.act(Action::Tool("pen".into()));
    a.render();
    drag(&mut a, [100., 500., 0.5], [600., 500., 0.5], false);
    let preview = a.surface.pixels.clone();
    a.redraw = true;
    a.render();
    let canvas = a.surface.rect(a.view.clip);
    for y in canvas.y..canvas.bottom {
        let row = y * a.surface.width + canvas.x..y * a.surface.width + canvas.right;
        assert!(
            preview[row.clone()] == a.surface.pixels[row],
            "New ink must retain correct page contrast after object editing"
        );
    }
}
#[test]
fn empty_taps_and_hover_never_create_marks_or_pan_the_page() {
    let (_dir, mut a) = fixture(vec![ink(150., 200.)]);
    let before = a.page().unwrap().payload.clone();
    let q = a.view.screen([200., 220., 0.5]);
    a.motion(q[0], q[1], 0.);
    assert!(a.selection.strokes.is_empty());
    touch(&mut a, [200., 220., 0.5]);
    let blank = a.view.screen([600., 800., 0.5]);
    a.down(blank[0], blank[1], 0.5, true);
    a.motion(blank[0] - 100., blank[1] - 100., 0.5);
    a.up(blank[0] - 100., blank[1] - 100.);
    assert!(a.selection.strokes.is_empty());
    assert_eq!(a.page().unwrap().payload, before);
    assert_eq!(a.scroll, 0.);
    a.render();
    a.surface
        .repaint_page_region(a.view, &before, None, Box2::new(-500., -500., 1., 1.));
    // Empty clipping must be harmless.
}

#[test]
fn fractional_handle_centers_cannot_invert_the_circle_clamp_range() {
    use pinepaper::render::{Surface, BLACK};
    let mut surface = Surface::new();
    surface.resize(702, 936, 2);
    for x in [498.00000000000006, 500.00000000000006, 400.99999999999994] {
        for y in [498.00000000000006, 500.00000000000006] {
            for radius in [5., 7.] {
                surface.rounded(Box2::new(x, y, radius * 2., radius * 2.), radius, BLACK);
            }
        }
    }
    assert!(surface.pixels.contains(&BLACK));
}

#[test]
fn queued_edit_uses_latest_endpoint_and_cached_layers_preserve_text_and_pdf() {
    let arrow = Shape::Arrow.stroke([150., 300., 0.5], [450., 300., 0.5], 4., 0.45);
    let (_dir, mut a) = fixture(vec![ink(100., 120.), arrow, ink(280., 330.)]);
    a.pages[0].payload.texts.push(Text {
        text: "Overlapping text\nand a second line".into(),
        x: 280.,
        y: 300.,
        size: 22.,
        font: "Sans".into(),
        color: 0.3,
        extra: Default::default(),
    });
    a.pdf = Some(image::RgbImage::from_fn(50, 70, |x, y| {
        image::Rgb([180 + x as u8, 180 + y as u8, 210])
    }));
    a.prefs.page_contrast = 1.4;
    a.prefs.page_gamma = 0.8;
    a.redraw = true;
    a.render();
    touch(&mut a, [210., 300., 0.5]);
    assert_eq!(a.selection.strokes, vec![1]);
    let start = a.view.screen([450., 300., 0.5]);
    a.down(start[0], start[1], 0.5, false);
    a.render();
    let before = a.surface.pixels.clone();
    for i in 0..100 {
        a.queue_motion(start[0] + 80., start[1] + i as f64, 0.5);
    }
    assert!(a.surface.pixels == before);
    a.render();
    // Two further visible previews exercise reuse of the same cached background.
    for p in [[500., 400., 0.5], [380., 250., 0.5]] {
        let q = a.view.screen(p);
        a.queue_motion(q[0], q[1], 0.5);
        a.render();
    }
    let preview = a.surface.pixels.clone();
    a.redraw = true;
    a.render();
    assert!(
        preview == a.surface.pixels,
        "Cached PDF, tone and foreground text must match a full repaint"
    );
    a.queue_motion(start[0] + 10., start[1] + 10., 0.5);
    let release = a.view.screen([520., 440., 0.5]);
    a.up(release[0], release[1]);
    let end = a.selected_endpoints().unwrap()[1];
    assert!((end[0] - 520.).abs() < 1e-8 && (end[1] - 440.).abs() < 1e-8);
    let saved = a.page().unwrap().payload.clone();
    a.render();
    assert_eq!(
        a.page().unwrap().payload,
        saved,
        "Pending motion cannot leak past release"
    );
    a.act(Action::Undo);
    assert_eq!(
        a.page().unwrap().payload.strokes[1].points[1],
        [450., 300., 0.5]
    );
    a.render();
    touch(&mut a, [210., 300., 0.5]);
    a.down(start[0], start[1], 0.5, false);
    a.queue_motion(start[0] + 40., start[1] + 60., 0.5);
    a.act(Action::Undo); // Shortcut while the pen is still down.
    a.render();
    a.up(start[0] + 90., start[1] + 80.);
    a.render();
    assert_eq!(
        a.page().unwrap().payload.strokes[1].points[1],
        [450., 300., 0.5],
        "Undo during a drag must finish it and prevent a stale edit after release"
    );
}
