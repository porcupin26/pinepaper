use pinepaper::{
    app::{Action, App, Dialog, Screen},
    diagram::Shape,
    model::*,
};

fn has(a: &App, action: Action) -> bool {
    a.hits.iter().any(|(_, x)| *x == action)
}
fn drag(a: &mut App, start: [f64; 3], end: [f64; 3]) {
    let p = a.view.screen(start);
    let q = a.view.screen(end);
    a.down(p[0], p[1], 0.5, false);
    a.motion(q[0], q[1], 0.5);
    a.up(q[0], q[1]);
}
#[test]
fn curved_arrow_gestures_follow_latest_frame_and_exact_release_without_losing_ink_samples() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.resize(702, 936, 2);
    a.act(Action::New("notebook".into()));
    for shape in [Shape::Line, Shape::Arrow, Shape::DoubleArrow] {
        a.act(Action::Shape(shape));
        a.render();
        let start = [120., 180., 0.5];
        let p = a.view.screen(start);
        a.down(p[0], p[1], 0.5, false);
        let before = a.surface.pixels.clone();
        for i in 0..120 {
            let t = i as f64 / 120. * std::f64::consts::TAU;
            let p = a
                .view
                .screen([340. + 160. * t.cos(), 450. + 170. * t.sin(), 0.5]);
            a.queue_motion(p[0], p[1], 0.5);
        }
        assert!(
            a.surface.pixels == before,
            "Queued geometry must not repaint old samples"
        );
        let latest = [520., 500., 0.5];
        let p = a.view.screen(latest);
        a.queue_motion(p[0], p[1], 0.5);
        a.render();
        assert!(a.surface.pixels != before);
        let ends = pinepaper::diagram::endpoints(a.live_stroke().unwrap()).unwrap();
        assert!((ends[1][0] - latest[0]).abs() < 1e-8);
        // Queue more motion, then release at a different position without a motion frame.
        a.queue_motion(p[0] - 50., p[1] + 30., 0.5);
        let release = [420., 620., 0.5];
        let p = a.view.screen(release);
        a.up(p[0], p[1]);
        let ends = pinepaper::diagram::endpoints(a.page().unwrap().payload.strokes.last().unwrap())
            .unwrap();
        for axis in [0, 1] {
            assert!((ends[0][axis] - start[axis]).abs() < 1e-8);
            assert!((ends[1][axis] - release[axis]).abs() < 1e-8);
        }
        let saved = a.page().unwrap().payload.clone();
        a.queue_motion(650., 850., 0.);
        a.render();
        assert_eq!(
            a.page().unwrap().payload,
            saved,
            "Hover cannot move a released endpoint"
        );
    }
    a.act(Action::Tool("pen".into()));
    a.render();
    let p = a.view.screen([100., 300., 0.5]);
    a.down(p[0], p[1], 0.5, false);
    for i in 1..=100 {
        a.queue_motion(p[0] + i as f64, p[1] + (i as f64).sin(), i as f64 / 100.);
    }
    assert_eq!(
        a.live_stroke().unwrap().points.len(),
        101,
        "Freehand ink keeps every sample"
    );
    a.up(p[0] + 100., p[1]);
}
#[test]
fn commands_follow_document_and_history_without_repainting_ink() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.resize(702, 936, 1);
    a.act(Action::New("notebook".into()));
    a.render();
    for command in [
        Action::Undo,
        Action::Redo,
        Action::Previous,
        Action::Next,
        Action::DeleteSelection,
    ] {
        assert!(!has(&a, command));
    }
    assert!(has(&a, Action::AddPage));
    drag(&mut a, [100., 200., 0.5], [200., 250., 0.5]);
    assert!(!a.redraw, "Pen-up must not rebuild the page");
    assert!(
        has(&a, Action::Undo),
        "Undo must become visible without a full render"
    );
    a.act(Action::Undo);
    a.render();
    assert!(has(&a, Action::Redo));
    assert!(!has(&a, Action::Undo));
    a.act(Action::Tool("lasso".into()));
    a.render();
    drag(&mut a, [400., 400., 0.5], [500., 500., 0.5]);
    a.render();
    assert!(
        has(&a, Action::Redo),
        "Empty selection must not discard redo"
    );
    a.act(Action::Tool("eraser".into()));
    a.render();
    drag(&mut a, [400., 400., 0.5], [500., 500., 0.5]);
    a.render();
    assert!(has(&a, Action::Redo), "Empty erasing must not discard redo");
    let pdf = a.store.root.join("files/test.pdf");
    pinepaper::document::export_pdf(&pdf, &a.pages, None).unwrap();
    for kind in ["pdf", "book"] {
        let id = a
            .store
            .create(
                kind,
                kind,
                None,
                vec![Payload::blank("Blank")],
                if kind == "pdf" {
                    Some("files/test.pdf")
                } else {
                    None
                },
            )
            .unwrap();
        a.open(&id).unwrap();
        a.hidden = false;
        a.render();
        assert!(!has(&a, Action::AddPage));
        a.act(Action::Menu);
        let Some(Dialog::Menu(_, items)) = &a.dialog else {
            panic!()
        };
        for action in [
            Action::AddPage,
            Action::TemplatePanel,
            Action::ClearPage,
            Action::DeletePage,
        ] {
            assert!(!items.iter().any(|(_, x)| *x == action));
        }
        a.act(Action::Close);
        a.act(Action::Overview);
        a.render();
        assert!(!has(&a, Action::TemplatePanel));
    }
}
#[test]
fn diagram_preview_restores_background_and_is_one_undoable_movable_erasable_object() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.resize(702, 936, 2);
    a.act(Action::New("notebook".into()));
    a.prefs.page_contrast = 1.35;
    a.prefs.page_gamma = 0.8;
    a.act(Action::Shape(Shape::Rectangle));
    a.render();
    let start = a.view.screen([100., 200., 0.5]);
    let end = a.view.screen([300., 380., 0.5]);
    let other = a.view.screen([450., 500., 0.5]);
    a.down(start[0], start[1], 0.5, false);
    a.motion(end[0], end[1], 0.2);
    let first = a.surface.pixels.clone();
    a.redraw = true;
    a.render();
    assert_eq!(
        a.surface.pixels, first,
        "Full repaint during a drag must preserve the preview"
    );
    a.motion(other[0], other[1], 0.9);
    a.motion(end[0], end[1], 0.5);
    assert_eq!(
        a.surface.pixels, first,
        "Old preview must be removed even with contrast/gamma"
    );
    a.up(end[0], end[1]);
    assert!(!a.redraw);
    assert_eq!(a.page().unwrap().payload.strokes.len(), 1);
    assert_eq!(
        a.page().unwrap().payload.strokes[0].extra["diagram_shape"],
        "Rectangle"
    );
    a.act(Action::Undo);
    assert!(a.page().unwrap().payload.strokes.is_empty());
    a.act(Action::Redo);
    assert_eq!(a.page().unwrap().payload.strokes.len(), 1);
    a.act(Action::Tool("lasso".into()));
    a.render();
    let path = [
        [80., 180., 0.5],
        [320., 180., 0.5],
        [320., 400., 0.5],
        [80., 400., 0.5],
        [80., 180., 0.5],
    ];
    let q = a.view.screen(path[0]);
    a.down(q[0], q[1], 0.5, false);
    for point in path {
        let q = a.view.screen(point);
        a.motion(q[0], q[1], 0.5);
    }
    a.up(q[0], q[1]);
    assert_eq!(a.selection.strokes, vec![0]);
    a.render();
    drag(&mut a, [200., 300., 0.5], [240., 330., 0.5]);
    assert!((a.page().unwrap().payload.strokes[0].points[0][0] - 140.).abs() < 1e-6);
    a.act(Action::Undo);
    a.act(Action::Tool("eraser".into()));
    a.render();
    drag(&mut a, [90., 300., 0.5], [110., 300., 0.5]);
    assert!(a.page().unwrap().payload.strokes.is_empty());
    a.act(Action::Undo);
    assert_eq!(a.page().unwrap().payload.strokes.len(), 1);
    a.saver.flush().unwrap();
    assert_eq!(
        a.store.pages(&a.doc.as_ref().unwrap().id).unwrap()[0].payload,
        a.page().unwrap().payload
    );
}
#[test]
fn constrained_geometry_is_closed_bounded_and_keeps_equal_sides_in_every_quadrant() {
    for shape in Shape::ALL {
        for end in [
            [20., 30., 0.],
            [500., 30., 0.],
            [20., 700., 0.],
            [900., 1400., 0.],
        ] {
            let s = shape.stroke([200., 300., 0.], end, 2.4, 0.07);
            assert!(s.points.iter().all(|p| p[0] >= 0.
                && p[0] <= PAGE_W
                && p[1] >= 0.
                && p[1] <= PAGE_H
                && p[2] == 0.5));
            if matches!(shape, Shape::Square | Shape::Circle) {
                let extent = |axis: usize| {
                    s.points
                        .iter()
                        .map(|p| p[axis])
                        .fold(f64::NEG_INFINITY, f64::max)
                        - s.points
                            .iter()
                            .map(|p| p[axis])
                            .fold(f64::INFINITY, f64::min)
                };
                assert!((extent(0) - extent(1)).abs() < 1e-8);
            }
            if matches!(
                shape,
                Shape::Rectangle | Shape::Square | Shape::Circle | Shape::Ellipse | Shape::Diamond
            ) {
                let a = s.points[0];
                let b = s.points.last().unwrap();
                assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 1e-8);
            }
        }
    }
    let s = Shape::Arrow.stroke([20., 20., 0.], [100., 20., 0.], 2., 0.07);
    assert!(s.points.iter().any(|p| p[1] < 20.));
    assert!(s.points.iter().any(|p| p[1] > 20.));
}
#[test]
fn moving_pages_preserves_every_field_and_rolls_back_failures() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let folder = store
        .create("Folder", "folder", None, vec![], None)
        .unwrap();
    let source = store
        .create(
            "Quick sheets",
            "notebook",
            Some(&folder),
            vec![
                Payload::blank("Lined"),
                Payload::blank("Dots"),
                Payload::blank("Grid"),
            ],
            None,
        )
        .unwrap();
    let doc = store.document(&source).unwrap();
    let mut original = store.pages(&source).unwrap();
    original[0].payload.strokes.push(Shape::Circle.stroke(
        [20., 20., 0.5],
        [80., 100., 0.5],
        2.,
        0.07,
    ));
    original[0]
        .payload
        .extra
        .insert("future-field".into(), serde_json::json!({"preserve":true}));
    original[0].bookmarked = true;
    original[0].tags = vec!["work".into()];
    original[0].payload.texts.push(Text {
        text: "Keep my text".into(),
        x: 20.,
        y: 130.,
        size: 18.,
        font: "Sans".into(),
        color: 0.07,
        extra: Default::default(),
    });
    store.save_shared(&doc, &original).unwrap();
    let original = store.pages(&source).unwrap();
    for ids in [
        vec![original[0].id.clone(), "missing".into()],
        vec![],
        vec![original[0].id.clone(), original[0].id.clone()],
    ] {
        assert!(store.move_pages_to_notebook(&source, &ids, "New").is_err());
        assert_eq!(
            store.pages(&source).unwrap()[0].payload,
            original[0].payload
        );
    }
    assert!(store
        .move_pages_to_notebook(&source, &[original[0].id.clone()], " ")
        .is_err());
    // Force an error after the destination is inserted, proving transactional rollback.
    let db = rusqlite::Connection::open(dir.path().join("library.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_move BEFORE UPDATE OF document ON pages BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(store
        .move_pages_to_notebook(&source, &[original[0].id.clone()], "Rejected")
        .is_err());
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM documents WHERE title='Rejected'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_move").unwrap();
    let id = store
        .move_pages_to_notebook(
            &source,
            &[original[2].id.clone(), original[0].id.clone()],
            "Project",
        )
        .unwrap();
    let moved = store.pages(&id).unwrap();
    for (p, expected) in moved.iter().zip([&original[0], &original[2]]) {
        assert_eq!(p.id, expected.id);
        assert_eq!(p.payload, expected.payload);
        assert_eq!(p.bookmarked, expected.bookmarked);
        assert_eq!(p.tags, expected.tags);
        assert_eq!(p.document, id);
    }
    assert_eq!(moved[1].position, 1);
    assert_eq!(store.document(&id).unwrap().parent, Some(folder));
    let left = store.pages(&source).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, original[1].id);
    assert_eq!(left[0].position, 0);
    store
        .move_pages_to_notebook(&source, &[left[0].id.clone()], "Last")
        .unwrap();
    let left = store.pages(&source).unwrap();
    assert_eq!(left.len(), 1);
    assert_ne!(left[0].id, original[1].id);
    assert!(left[0].payload.strokes.is_empty());
}
#[test]
fn quick_sheets_selection_moves_pages_then_opens_named_notebook() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.resize(702, 936, 1);
    a.act(Action::QuickSheets);
    a.act(Action::AddPage);
    let source = a.doc.as_ref().unwrap().id.clone();
    let ids: Vec<_> = a.pages.iter().map(|p| p.id.clone()).collect();
    a.act(Action::MakeNotebook);
    a.render();
    assert_eq!(a.screen, Screen::Overview);
    assert_eq!(a.page_selection.as_ref().unwrap().len(), 1);
    assert!(!has(&a, Action::AddPage));
    a.act(Action::SelectPages(false));
    a.render();
    assert!(!has(&a, Action::NameNotebook));
    a.act(Action::SelectPages(true));
    a.act(Action::NameNotebook);
    a.act(Action::SelectAll);
    a.act(Action::Key("My project".into()));
    a.act(Action::Accept);
    assert_eq!(a.screen, Screen::Editor);
    assert!(a.dialog.is_none());
    assert_eq!(a.doc.as_ref().unwrap().title, "My project");
    assert_eq!(
        a.pages.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        ids
    );
    assert_eq!(a.store.pages(&source).unwrap().len(), 1);
    assert_eq!(
        a.store.setting::<Option<String>>("quick_sheets", None),
        Some(source)
    );
}
