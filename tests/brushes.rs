use pinepaper::{
    app::{Action, App, Dialog},
    exchange::replica::Replica,
    model::*,
    render::{Box2, Surface, View, WHITE},
};
fn tap(a: &mut App, action: Action) {
    let b = a.hits.iter().find(|(_, x)| *x == action).unwrap().0;
    a.down(b.x + b.w / 2., b.y + b.h / 2., 0.5, false);
    a.up(b.x + b.w / 2., b.y + b.h / 2.);
    a.render();
}
#[test]
fn tool_popovers_keep_independent_settings_and_fit_both_orientations() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.act(Action::New("notebook".into()));
    a.act(Action::Pen("Pencil".into()));
    a.act(Action::Width(1.4));
    let pen = a.nib.clone();
    for (w, h) in [(702, 936), (936, 702)] {
        a.resize(w, h, 2);
        a.render();
        assert!(a.hits.iter().any(|(_, x)| *x == Action::AddPage));
        for tool in ["highlighter", "brush"] {
            tap(&mut a, Action::Tool(tool.into()));
            assert!(a.dialog.is_none());
            tap(&mut a, Action::Tool(tool.into()));
            assert!(matches!(a.dialog, Some(Dialog::PenPanel)));
            let bounds = a.popup_bounds.unwrap();
            assert!(bounds.w < w as f64 * 0.6 && bounds.h < h as f64 * 0.65);
            for (b, _) in &a.hits {
                assert!(b.x >= 0. && b.y >= 0. && b.x + b.w <= w as f64 && b.y + b.h <= h as f64);
            }
            let sizes = if tool == "brush" {
                BRUSH_SIZES
            } else {
                HIGHLIGHTER_SIZES
            };
            if tool == "brush" {
                for name in BRUSHES {
                    tap(&mut a, Action::Pen(name.into()));
                }
                tap(&mut a, Action::Color(0.45));
            }
            for (size, _) in sizes {
                tap(&mut a, Action::Width(size));
            }
            assert!(matches!(a.dialog, Some(Dialog::PenPanel)));
            // Dismissal must not leak a mark through to the notebook.
            a.down(w as f64 - 20., h as f64 - 80., 0.5, false);
            a.up(w as f64 - 20., h as f64 - 80.);
            a.render();
            assert!(a.dialog.is_none());
            assert!(a.page().unwrap().payload.strokes.is_empty());
            assert_eq!(a.nib, pen);
        }
    }
    assert_eq!(a.highlighter.width, 9.);
    assert_eq!(a.brush.pen, "Dry");
    assert_eq!(a.brush.width, 28.);
    assert_eq!(a.brush.color, 0.45);
    drop(a);
    let a = App::new(dir.path().into()).unwrap();
    assert_eq!(a.nib, pen);
    assert_eq!(a.highlighter.width, 9.);
    assert_eq!(a.brush.pen, "Dry");
    assert_eq!(a.brush.width, 28.);
    assert_eq!(a.brush.color, 0.45);
}
fn rendered(stroke: Stroke) -> Surface {
    let mut s = Surface::new();
    s.resize(380, 520, 1);
    s.stroke(
        View {
            x: 0.,
            y: 0.,
            zoom: 0.5,
            clip: Box2::new(0., 0., 380., 520.),
        },
        &stroke,
    );
    s
}
fn sample(tool: &str, pen: &str, width: f64) -> Stroke {
    Stroke {
        tool: tool.into(),
        pen: pen.into(),
        width,
        color: 0.,
        points: vec![[100., 200., 0.1], [500., 200., 1.]],
        extra: Default::default(),
    }
}
#[test]
fn brushes_have_distinct_profiles_pressure_response_and_growing_sizes() {
    let mut images = vec![];
    for name in BRUSHES {
        let s = rendered(sample("brush", name, 28.));
        let column = |x| {
            (80..120)
                .filter(|&y| (s.pixels[y * s.width + x] & 255) < 250)
                .count()
        };
        assert!(
            column(235) > column(65),
            "{name} should widen with pressure"
        );
        assert!(
            images.iter().all(|old| old != &s.pixels),
            "{name} must have its own appearance"
        );
        images.push(s.pixels);
        let mut previous = 0;
        for (width, _) in BRUSH_SIZES {
            let s = rendered(sample("brush", name, width));
            let ink = s.pixels.iter().filter(|&&p| p != WHITE).count();
            assert!(ink > previous, "{name} widths must visibly increase");
            previous = ink;
        }
    }
}
#[test]
fn highlighter_sizes_are_distinct_and_retracing_one_stroke_keeps_transparency() {
    let mut previous = 0;
    for (width, _) in HIGHLIGHTER_SIZES {
        let mut stroke = sample("highlighter", "Fineliner", width);
        let s = rendered(stroke.clone());
        let ink = s.pixels.iter().filter(|&&p| p != WHITE).count();
        assert!(ink > previous);
        assert!(s.pixels.iter().all(|p| p & 255 >= 210));
        previous = ink;
        stroke.points.extend([stroke.points[0], stroke.points[1]]);
        assert_eq!(s.pixels, rendered(stroke).pixels);
    }
    // Nib family never changed the appearance of legacy highlighter strokes.
    assert_eq!(
        rendered(sample("highlighter", "Pencil", 2.4)).pixels,
        rendered(sample("highlighter", "Fineliner", 2.4)).pixels
    );
}
#[test]
fn brush_strokes_save_undo_export_and_share_with_their_type_and_size() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.act(Action::New("notebook".into()));
    a.resize(702, 936, 2);
    for (i, name) in BRUSHES.into_iter().enumerate() {
        a.act(Action::Tool("brush".into()));
        a.act(Action::Pen(name.into()));
        a.act(Action::Width(BRUSH_SIZES[i].0));
        a.act(Action::Close);
        a.render();
        let p = a.view.screen([150., 200. + i as f64 * 80., 0.5]);
        a.down(p[0], p[1], 0.1, false);
        for step in 1..40 {
            a.queue_motion(
                p[0] + step as f64 * 4.,
                p[1] + (step as f64 * 0.2).sin() * 10.,
                0.1 + step as f64 / 45.,
            );
        }
        a.up(p[0] + 156., p[1]);
        assert!(!a.redraw, "Drawing must stay on the incremental ink path");
        assert_eq!(a.page().unwrap().payload.strokes[i].tool, "brush");
        assert_eq!(a.page().unwrap().payload.strokes[i].pen, name);
    }
    let expected = a.page().unwrap().payload.clone();
    a.act(Action::Undo);
    assert_eq!(a.page().unwrap().payload.strokes.len(), 3);
    a.act(Action::Redo);
    assert_eq!(a.page().unwrap().payload, expected);
    a.saver.flush().unwrap();
    let doc = a.doc.as_ref().unwrap().clone();
    a.open(&doc.id).unwrap();
    let persisted: Payload =
        serde_json::from_str(&serde_json::to_string(&expected).unwrap()).unwrap();
    assert_eq!(a.page().unwrap().payload, persisted);
    pinepaper::document::export_pdf(&dir.path().join("brushes.pdf"), &a.pages, None).unwrap();
    let replica = Replica::from_local(&doc, &mut a.pages).unwrap();
    let mut peer = Replica::from_shared(
        serde_json::from_str(&serde_json::to_string(&replica.state).unwrap()).unwrap(),
    )
    .unwrap();
    let pages = peer.pages("peer").unwrap();
    for source in &a.pages[0].payload.strokes {
        let target = pages[0]
            .payload
            .strokes
            .iter()
            .find(|s| s.extra == source.extra)
            .unwrap();
        assert_eq!(source.tool, target.tool);
        assert_eq!(source.pen, target.pen);
        assert_eq!(source.width, target.width);
        assert_eq!(source.extra, target.extra);
    }
    // Wide strokes remain selectable/erasable near their visible edge.
    let mut p = Payload::blank("Blank");
    let mut stroke = sample("brush", "Round", 28.);
    stroke.points = vec![[100., 200., 1.], [500., 200., 1.]];
    p.strokes.push(stroke);
    assert!(erase(&mut p, [300., 221., 0.5], [300., 221., 0.5]));
}
