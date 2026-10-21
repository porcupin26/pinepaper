use pinepaper::{
    app::{Action, App, Dialog, Screen},
    document,
    model::*,
    render::{Box2, Surface, View},
};

#[test]
fn quick_sheets_reuses_identity_appends_without_losing_ink_and_respects_trash() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.resize(702, 936, 1);
    a.act(Action::QuickSheets);
    let id = a.doc.as_ref().unwrap().id.clone();
    assert_eq!(a.screen, Screen::Editor);
    assert_eq!(a.tool, "pen");
    a.render();
    let p = a.view.screen([150., 180., 0.5]);
    a.down(p[0], p[1], 0.5, false);
    a.motion(p[0] + 40., p[1] + 20., 0.7);
    a.up(p[0] + 40., p[1] + 20.);
    a.act(Action::Library);
    let ink = a.store.pages(&id).unwrap()[0].payload.strokes.clone();
    assert_eq!(ink.len(), 1);
    a.store.rename(&id, "Scratchpad").unwrap();
    a.act(Action::QuickSheets);
    assert_eq!(a.doc.as_ref().unwrap().id, id);
    assert_eq!(a.index, 1);
    assert_eq!(a.pages.len(), 2);
    assert_eq!(a.pages[0].payload.strokes[0].points, ink[0].points);
    assert!(a.pages[1].payload.strokes.is_empty());
    a.act(Action::Library);
    drop(a);
    let mut a = App::new(dir.path().into()).unwrap();
    a.act(Action::QuickSheets);
    assert_eq!(a.doc.as_ref().unwrap().id, id);
    assert_eq!(a.index, 2);
    a.act(Action::Library);
    a.store.trash(&id, true).unwrap();
    a.act(Action::QuickSheets);
    assert_ne!(a.doc.as_ref().unwrap().id, id);
    assert_eq!(a.pages.len(), 1);
    assert!(a.store.document(&id).unwrap().trashed);
    assert_eq!(
        a.store.pages(&id).unwrap()[0].payload.strokes[0].points,
        ink[0].points
    );
}

#[test]
fn library_filters_and_search_find_nested_files_and_preferences_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    let folder = a
        .store
        .create("Projects", "folder", None, vec![], None)
        .unwrap();
    let nested = a
        .store
        .create(
            "Deep notes",
            "notebook",
            Some(&folder),
            vec![Payload::blank("Lined")],
            None,
        )
        .unwrap();
    a.reload_library().unwrap();
    assert_eq!(a.docs.len(), 1);
    a.act(Action::Filter("notebook".into()));
    assert_eq!(a.docs[0].id, nested);
    a.act(Action::LibraryHome);
    a.query = "deep".into();
    a.reload_library().unwrap();
    assert_eq!(a.docs[0].id, nested);
    a.act(Action::ClearSearch);
    assert_eq!(a.docs[0].id, folder);
    a.act(Action::LibraryView);
    a.act(Action::SortBy(3));
    drop(a);
    let a = App::new(dir.path().into()).unwrap();
    assert!(a.library_list);
    assert_eq!(a.sort, 3);
}

#[test]
fn library_scrolling_reaches_every_file_without_covering_fixed_controls() {
    use std::collections::HashSet;
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    for i in 0..12 {
        a.store
            .create(&format!("Folder {i}"), "folder", None, vec![], None)
            .unwrap();
        a.store
            .create(
                &format!("Notes {i}"),
                "notebook",
                None,
                vec![Payload::blank("Lined")],
                None,
            )
            .unwrap();
    }
    a.reload_library().unwrap();
    for (w, h) in [(702, 936), (936, 702)] {
        for list in [false, true] {
            a.library_list = list;
            a.prefs.ui_font_size = 18.;
            a.resize(w, h, 1);
            a.library_offset = 0;
            let mut reached = HashSet::new();
            loop {
                a.redraw = true;
                a.render();
                for (b, action) in &a.hits {
                    assert!(
                        b.x >= 0. && b.y >= 0. && b.x + b.w <= w as f64 && b.y + b.h <= h as f64,
                        "{action:?}: {b:?}"
                    );
                    if let Action::Open(id) = action {
                        assert!(b.y >= 160. && b.y + b.h <= h as f64 - 40.);
                        reached.insert(id.clone());
                    }
                }
                let header = a.surface.pixels[..w * 160].to_vec();
                let hits = a.hits.clone();
                pinepaper::ui::status(&mut a);
                assert_eq!(header, a.surface.pixels[..w * 160]);
                assert_eq!(hits, a.hits);
                let old = a.library_offset;
                a.scroll_by(300.);
                if old == a.library_offset {
                    break;
                }
            }
            assert_eq!(reached.len(), 24);
            a.act(Action::LibraryMenu);
            a.render();
            let panel = a.popup_bounds.unwrap();
            let x = panel.w + 25.;
            a.down(x, 300., 0.5, false);
            a.up(x, 300.);
            assert!(a.dialog.is_none());
            assert_eq!(a.screen, Screen::Library);
        }
    }
}

#[test]
fn button_labels_are_centered_at_both_display_scales() {
    use pinepaper::render::{BLACK, WHITE};
    for scale in [1, 2] {
        for size in [16., 20., 24.] {
            for label in ["All", "Notebooks", "Books", "PDFs", "Apply"] {
                let mut s = Surface::new();
                s.resize(200, 80, scale);
                s.pixels.fill(WHITE);
                let b = Box2::new(20., 18., 160., 44.);
                s.centered_label(label, b, size, BLACK);
                let mut bounds = (s.width, s.height, 0, 0);
                for (i, color) in s.pixels.iter().enumerate() {
                    if color & 255 < 128 {
                        let (x, y) = (i % s.width, i / s.width);
                        bounds.0 = bounds.0.min(x);
                        bounds.1 = bounds.1.min(y);
                        bounds.2 = bounds.2.max(x + 1);
                        bounds.3 = bounds.3.max(y + 1);
                    }
                }
                assert!(bounds.2 > bounds.0, "Missing label: {label}");
                let cx = (bounds.0 + bounds.2) as f64 / 2.;
                let cy = (bounds.1 + bounds.3) as f64 / 2.;
                assert!((cx - 100. * scale as f64).abs() <= 1.5, "{label}: x={cx}");
                assert!((cy - 40. * scale as f64).abs() <= 1.5, "{label}: y={cy}");
            }
        }
    }
}

#[test]
fn sqlite_compatibility_save_order_backup_and_folders() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let a = s.create("A", "folder", None, vec![], None).unwrap();
    let b = s.create("B", "folder", Some(&a), vec![], None).unwrap();
    assert!(s.move_to(&a, Some(&b)).is_err());
    let mut p = Payload::blank("Lined");
    p.extra.insert(
        "future-extension".into(),
        serde_json::json!({"preserve":true}),
    );
    let id = s
        .create("My note", "notebook", Some(&b), vec![p], None)
        .unwrap();
    let mut page = s.pages(&id).unwrap().remove(0);
    let mut saver = Saver::start(dir.path().into()).unwrap();
    for n in 0..20 {
        page.payload.strokes.push(Stroke {
            points: vec![[n as f64, 10., 0.7]],
            ..Stroke::default()
        });
        saver.save(&page).unwrap();
    }
    saver.flush().unwrap();
    let reloaded = s.pages(&id).unwrap();
    assert_eq!(reloaded[0].payload.strokes.len(), 20);
    assert_eq!(
        reloaded[0].payload.extra["future-extension"]["preserve"],
        true
    );
    s.add_page(&id, 0, "Grid").unwrap();
    assert_eq!(s.pages(&id).unwrap()[1].position, 1);
    s.delete_page(&s.pages(&id).unwrap()[1]).unwrap();
    assert!(s.delete_page(&page).is_err());
    s.trash(&b, true).unwrap();
    s.trash(&id, true).unwrap();
    s.trash(&id, false).unwrap();
    assert!(s.document(&id).unwrap().parent.is_none());
    std::fs::write(dir.path().join("files/source.txt"), "book").unwrap();
    let zip = dir.path().join("backup.zip");
    s.backup(&zip).unwrap();
    let mut z = zip::ZipArchive::new(std::fs::File::open(zip).unwrap()).unwrap();
    assert!(z.by_name("files/source.txt").is_ok());
    let restore = tempfile::tempdir().unwrap();
    z.extract(restore.path()).unwrap();
    let s = Store::open(restore.path()).unwrap();
    assert_eq!(s.pages(&id).unwrap()[0].payload.strokes.len(), 20);
}

#[test]
fn all_nibs_live_pixels_match_full_repaint_including_toned_overlapping_highlights() {
    let styles = PENS
        .into_iter()
        .flat_map(|pen| [(pen, "pen"), (pen, "highlighter")])
        .chain(BRUSHES.into_iter().map(|pen| (pen, "brush")));
    for (pen, tool) in styles {
        let mut p = Payload::blank("Grid");
        let mut c = Surface::new();
        c.resize(380, 520, 1);
        let view = View {
            x: 0.,
            y: 0.,
            zoom: 0.5,
            clip: Box2::new(0., 0., 380., 520.),
        };
        c.page(view, &p, None, true);
        c.tone(view.clip, 1.35, 1.2);
        for offset in [0., 3.] {
            let stroke = Stroke {
                pen: pen.into(),
                tool: tool.into(),
                width: if tool == "brush" { 16. } else { 3. },
                color: 0.4,
                points: (0..120)
                    .map(|i| {
                        [
                            100. + i as f64 * 2.,
                            200. + offset + (i as f64 / 12.).sin() * 30.,
                            i as f64 / 120.,
                        ]
                    })
                    .collect(),
                extra: Default::default(),
            };
            c.begin_stroke();
            for i in 0..stroke.points.len() {
                let before = c.pixels.clone();
                let r = c.segment(
                    view,
                    stroke.points[i.saturating_sub(1)],
                    stroke.points[i],
                    &stroke,
                );
                for (j, (&a, &b)) in before.iter().zip(&c.pixels).enumerate() {
                    if a != b {
                        assert!(
                            j % c.width >= r.x
                                && j % c.width < r.right
                                && j / c.width >= r.y
                                && j / c.width < r.bottom
                        );
                    }
                }
            }
            p.strokes.push(stroke);
        }
        let live = c.pixels.clone();
        c.clear_tone();
        c.page(view, &p, None, true);
        c.tone(view.clip, 1.35, 1.2);
        assert_eq!(live, c.pixels, "{pen}/{tool}");
    }
}

#[test]
fn eraser_hits_sparse_crossings_and_lasso_polygon() {
    let mut p = Payload::blank("Blank");
    p.strokes.push(Stroke {
        points: vec![[0., 0., 0.5], [200., 200., 0.5]],
        ..Stroke::default()
    });
    assert!(erase(&mut p, [0., 200., 0.], [200., 0., 0.]));
    assert!(p.strokes.is_empty());
    let poly = [
        [0., 0., 0.],
        [100., 0., 0.],
        [100., 100., 0.],
        [0., 100., 0.],
    ];
    assert!(inside([50., 50., 0.], &poly));
    assert!(!inside([150., 50., 0.], &poly));
}

#[test]
fn input_persistence_undo_redo_rotation_and_draft() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    let id = a
        .store
        .create(
            "笔記",
            "notebook",
            None,
            vec![Payload::blank("Lined")],
            None,
        )
        .unwrap();
    a.open(&id).unwrap();
    a.resize(936, 702, 2);
    a.render();
    let v = a.view;
    let start = v.screen([100., 200., 0.5]);
    a.motion(start[0], start[1], 0.5);
    assert!(a.page().unwrap().payload.strokes.is_empty());
    a.down(start[0], start[1], 0.4, false);
    for i in 1..241 {
        let p = v.screen([
            100. + i as f64,
            200. + (i as f64 / 16.).sin() * 20.,
            0.2 + i as f64 / 500.,
        ]);
        a.motion(p[0], p[1], p[2]);
    }
    a.up(start[0] + 240. * v.zoom, start[1]);
    a.saver.flush().unwrap();
    assert_eq!(
        a.store.pages(&id).unwrap()[0].payload.strokes[0]
            .points
            .len(),
        241
    );
    let points = a.page().unwrap().payload.strokes[0].points.clone();
    a.act(Action::Undo);
    assert!(a.page().unwrap().payload.strokes.is_empty());
    a.act(Action::Redo);
    assert_eq!(a.page().unwrap().payload.strokes[0].points, points);
    a.act(Action::Tags);
    a.act(Action::Key("Deutsch, français, żółć".into()));
    a.resize(702, 936, 2);
    a.render();
    if let Some(Dialog::Input(i)) = &a.dialog {
        assert_eq!(i.value, "Deutsch, français, żółć");
        assert!(i.keyboard);
    } else {
        panic!("draft lost");
    }
    a.act(Action::Accept);
    assert_eq!(a.page().unwrap().tags.len(), 3);
    assert_eq!(a.page().unwrap().payload.strokes[0].points, points);
    a.saver.flush().unwrap();
}

#[test]
fn keyboard_unicode_selection_and_editing() {
    let mut i = pinepaper::app::Input {
        title: String::new(),
        value: "aéłz".into(),
        cursor: 3,
        anchor: 1,
        purpose: pinepaper::app::InputPurpose::Search,
        keyboard: true,
        mode: 0,
        shift: 0,
        multiline: false,
    };
    i.insert("ö", false);
    assert_eq!(i.value, "aöz");
    i.insert("", true);
    assert_eq!(i.value, "az");
    assert_eq!(i.cursor, 1);
}

#[test]
fn epub_spine_order_and_exported_pdf_is_readable() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("test.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&epub).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for(name,body)in[("META-INF/container.xml","<container><rootfiles><rootfile full-path='OEBPS/book.opf'/></rootfiles></container>"),("OEBPS/book.opf","<package><metadata><title>Test</title></metadata><manifest><item id='b' href='b.xhtml' media-type='application/xhtml+xml'/><item id='a' href='a.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='a'/><itemref idref='b'/></spine></package>"),("OEBPS/a.xhtml","<html><body><p>First chapter</p></body></html>"),("OEBPS/b.xhtml","<html><body><p>Second chapter</p></body></html>")]{zip.start_file(name,opts).unwrap();zip.write_all(body.as_bytes()).unwrap();}
    zip.finish().unwrap();
    let (title, pages) = document::epub(&epub).unwrap();
    assert_eq!(title, "Test");
    assert!(pages[0].body.as_deref().unwrap().starts_with("First"));
    assert!(pages[1].body.as_deref().unwrap().starts_with("Second"));
    let mut store = Store::open(dir.path().join("library")).unwrap();
    let id = document::import(&mut store, &epub, None).unwrap();
    let pdf = dir.path().join("output.pdf");
    document::export_pdf(&pdf, &store.pages(&id).unwrap(), None).unwrap();
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-1.4"));
    if std::process::Command::new("pdfinfo")
        .arg("-v")
        .output()
        .is_ok()
    {
        assert_eq!(document::pdf_count(&pdf).unwrap(), 2);
        let image = document::pdf_page(&pdf, 0).unwrap();
        assert!(image.width() > 1000);
        let mut app = App::new(dir.path().join("preview-library")).unwrap();
        let pdf_id = document::import(&mut app.store, &pdf, None).unwrap();
        app.reload_library().unwrap();
        app.resize(936, 702, 2);
        app.render();
        let placeholder = app.surface.pixels.clone();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !app.thumbnails.contains_key(&(pdf_id.clone(), 0)) {
            assert!(
                std::time::Instant::now() < deadline,
                "PDF preview did not load"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
            app.tick();
        }
        app.render();
        assert_ne!(
            app.surface.pixels, placeholder,
            "PDF preview never reached the screen"
        );
        app.open(&pdf_id).unwrap();
        app.act(Action::Overview);
        app.render();
        while !app.thumbnails.contains_key(&(pdf_id.clone(), 1)) {
            assert!(
                std::time::Instant::now() < deadline,
                "Second PDF preview did not load"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
            app.tick();
        }
    }
}

#[test]
fn controls_stay_within_portrait_and_landscape_at_largest_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    a.prefs.ui_font_size = 18.;
    a.prefs.keyboard_key_height = 44.;
    let id = a
        .store
        .create(
            "A notebook",
            "notebook",
            None,
            vec![Payload::blank("Lined")],
            None,
        )
        .unwrap();
    for (w, h) in [(702, 936), (936, 702)] {
        a.resize(w, h, 2);
        for screen in [Screen::Library, Screen::Editor, Screen::Overview] {
            a.open(&id).unwrap();
            a.screen = screen;
            a.redraw = true;
            a.render();
            for (b, _) in &a.hits {
                assert!(
                    b.x >= 0. && b.y >= 0. && b.x + b.w <= w as f64 && b.y + b.h <= h as f64,
                    "{screen:?}: {b:?}"
                );
            }
        }
        a.act(Action::Tags);
        a.render();
        for (b, _) in &a.hits {
            assert!(
                b.x >= 0. && b.y >= 0. && b.x + b.w <= w as f64 && b.y + b.h <= h as f64,
                "keyboard: {b:?}"
            );
        }
        a.act(Action::Close);
    }
}

#[test]
fn notebook_creation_and_popovers_keep_drawing_safe() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = App::new(dir.path().into()).unwrap();
    let folder = a
        .store
        .create("Work", "folder", None, vec![], None)
        .unwrap();
    a.folder = Some(folder.clone());
    a.tool = "text".into();
    a.resize(936, 702, 2);
    a.render();
    a.act(Action::Create);
    a.render();
    let tap = |a: &mut App, action: Action| {
        let (b, _) = a.hits.iter().find(|(_, act)| act == &action).unwrap();
        let (x, y) = (b.x + b.w / 2., b.y + b.h / 2.);
        a.down(x, y, 0.5, false);
        a.up(x, y);
        a.render();
    };
    tap(&mut a, Action::New("notebook".into()));
    assert!(a.dialog.is_none());
    assert_eq!(a.screen, Screen::Editor);
    assert_eq!(a.tool, "pen");
    assert_eq!(
        a.doc.as_ref().unwrap().parent.as_deref(),
        Some(folder.as_str())
    );
    assert_eq!(a.doc.as_ref().unwrap().title, "Notebook 1");
    let id = a.doc.as_ref().unwrap().id.clone();
    let p = a.view.screen([200., 200., 0.5]);
    a.down(p[0], p[1], 0.5, false);
    a.motion(p[0] + 40., p[1] + 20., 0.7);
    a.up(p[0] + 40., p[1] + 20.);
    a.saver.flush().unwrap();
    let ink = a.page().unwrap().payload.strokes[0].points.clone();
    a.prefs.ui_font_size = 18.;
    for (w, h) in [(936, 702), (702, 936), (936, 702)] {
        a.resize(w, h, 2);
        a.render();
        tap(&mut a, Action::Tool("pen".into()));
        assert!(matches!(a.dialog, Some(Dialog::PenPanel)));
        tap(&mut a, Action::Pen("Pencil".into()));
        tap(&mut a, Action::Width(4.));
        tap(&mut a, Action::Color(0.45));
        assert!(matches!(a.dialog, Some(Dialog::PenPanel)));
        assert_eq!(a.nib.pen, "Pencil");
        assert_eq!(a.nib.width, 4.);
        assert_eq!(a.nib.color, 0.45);
        let card = a.popup_bounds.unwrap();
        assert!(card.w < w as f64 * 0.6 && card.h < h as f64 * 0.65);
        for (b, _) in &a.hits {
            assert!(
                b.x >= 0. && b.y >= 0. && b.x + b.w <= w as f64 && b.y + b.h <= h as f64,
                "{b:?}"
            );
        }
        // Outside contact closes the popup but cannot pass through into the canvas.
        let (x, y) = (w as f64 - 45., h as f64 - 80.);
        a.down(x, y, 0.5, false);
        a.up(x, y);
        a.render();
        assert!(a.dialog.is_none());
        assert_eq!(a.page().unwrap().payload.strokes.len(), 1);
        assert_eq!(a.page().unwrap().payload.strokes[0].points, ink);
        tap(&mut a, Action::Menu);
        assert!(matches!(a.dialog, Some(Dialog::Menu(..))));
        a.act(Action::Close);
        a.render();
    }
    tap(&mut a, Action::Rename(id.clone()));
    a.act(Action::SelectAll);
    a.act(Action::Key("Meeting notes".into()));
    a.act(Action::Accept);
    assert_eq!(a.doc.as_ref().unwrap().title, "Meeting notes");
    assert_eq!(a.store.document(&id).unwrap().title, "Meeting notes");
    a.act(Action::New("notebook".into()));
    let first_default = a.doc.as_ref().unwrap().title.clone();
    a.act(Action::New("notebook".into()));
    assert_ne!(first_default, a.doc.as_ref().unwrap().title);
    assert_eq!(
        a.store.pages(&id).unwrap()[0].payload.strokes[0].points,
        ink
    );
}
