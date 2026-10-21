use anyhow::{Context, Result};
use pinepaper::{
    app::{Action, App},
    model, wayland,
};
use std::{fs, path::PathBuf};

fn run() -> Result<()> {
    let mut root = model::default_root();
    let mut file = None;
    let mut windowed = false;
    let mut smoke = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => {
                println!("PinePaper {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("PinePaper — native notebooks and reader for Linux Wayland\n\nUsage: pinepaper [--data-dir PATH] [--windowed] [FILE]\n\n--data-dir PATH  Separate library (default: XDG_DATA_HOME/pinepaper)\n--windowed       Start in a normal window; F11 toggles fullscreen\n--smoke PATH     Render and verify a disposable demonstration library\n--version        Show version\n\nPDF rendering requires Poppler tools; EPUB/text are read in Rust.\nSettings and saved notes are compatible with PinePaper 0.2.2.");
                return Ok(());
            }
            "--data-dir" => root = args.next().context("--data-dir needs a path")?.into(),
            "--windowed" => windowed = true,
            "--smoke" => {
                smoke = Some(PathBuf::from(
                    args.next().context("--smoke needs an output directory")?,
                ))
            }
            _ if arg.starts_with('-') => anyhow::bail!("Unknown option: {arg}"),
            _ => file = Some(PathBuf::from(arg)),
        }
    }
    if let Some(path) = smoke {
        return smoke_test(path);
    }
    fs::create_dir_all(&root)?;
    // A process lock prevents simultaneous writers holding stale page snapshots.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".rust-instance.lock"))?;
    if rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_err() {
        use std::io::Write;
        let mut stream = std::os::unix::net::UnixStream::connect(root.join("instance.sock"))
            .context("PinePaper is running but not accepting activation")?;
        let path = file.map(|p| p.canonicalize()).transpose()?;
        stream.write_all(serde_json::to_string(&path)?.as_bytes())?;
        return Ok(());
    }
    let mut app = App::new(root)?;
    if windowed {
        app.fullscreen = false;
    }
    if let Some(path) = file {
        let id = pinepaper::document::import(&mut app.store, &path, None)?;
        app.open(&id)?;
    }
    wayland::run(app)
}
fn smoke_test(output: PathBuf) -> Result<()> {
    use pinepaper::{
        app::{Dialog, InputPurpose, Screen, Settings},
        model::{Payload, Stroke, Text},
        platform::{Hardware, Status},
    };
    fs::create_dir_all(&output)?;
    let tmp = tempfile::tempdir()?;
    let mut a = App::new(tmp.path().to_path_buf())?;
    a.status = Status {
        battery: Some(83),
        charging: true,
        wifi: "Connected".into(),
        ssid: "Test network".into(),
        signal: Some(76),
    };
    let mut page = Payload::blank("Lined");
    page.texts.push(Text {
        text: "Meeting notes\n\nA quiet place to think, read and write.".into(),
        x: 76.,
        y: 50.,
        size: 25.,
        font: "Sans".into(),
        color: 0.08,
        extra: Default::default(),
    });
    page.strokes.push(Stroke {
        points: (0..500)
            .map(|i| [80. + i as f64, 360. + ((i as f64) / 22.).sin() * 40., 0.5])
            .collect(),
        ..Stroke::default()
    });
    let id = a.store.create(
        "Meeting notes",
        "notebook",
        None,
        vec![page.clone(), Payload::blank("Dots"), Payload::blank("Grid")],
        None,
    )?;
    for name in [
        "Projects",
        "Development",
        "Financials",
        "To Read",
        "Planning",
        "Market Analysis",
    ] {
        a.store.create(name, "folder", None, vec![], None)?;
    }
    let mut sketch = Payload::blank("Dots");
    sketch.texts.push(Text {
        text: "Project handover\n\nGoals → sketches → next steps".into(),
        x: 55.,
        y: 70.,
        size: 26.,
        font: "Sans".into(),
        color: 0.08,
        extra: Default::default(),
    });
    for (x, y) in [(85., 300.), (400., 300.), (245., 510.)] {
        sketch.strokes.push(Stroke {
            points: vec![
                [x, y, 0.5],
                [x + 190., y, 0.5],
                [x + 190., y + 110., 0.5],
                [x, y + 110., 0.5],
                [x, y, 0.5],
            ],
            ..Stroke::default()
        });
    }
    sketch.strokes.push(Stroke {
        points: vec![[275., 355., 0.5], [400., 355., 0.5], [340., 510., 0.5]],
        ..Stroke::default()
    });
    a.store
        .create("Project handover", "notebook", None, vec![sketch], None)?;
    a.store.create(
        "Reading journal",
        "notebook",
        None,
        vec![Payload::blank("Checklist")],
        None,
    )?;
    a.store.create("A room of one's own","book",None,pinepaper::document::paginate("A notebook makes room for an unfinished idea. Write it down, return to it, and let it grow.\n\nThe page stays quiet while your thoughts move.","A room of one's own"),None)?;
    a.reload_library()?;
    for (w, h, label) in [(936, 702, "landscape"), (702, 936, "portrait")] {
        a.screen = Screen::Library;
        a.resize(w, h, 2);
        a.render();
        a.surface
            .save_png(&output.join(format!("library-{label}.png")))?;
        a.act(Action::LibraryMenu);
        a.render();
        a.surface
            .save_png(&output.join(format!("library-menu-{label}.png")))?;
        a.act(Action::Close);
        a.act(Action::LibraryView);
        a.render();
        a.surface
            .save_png(&output.join(format!("library-list-{label}.png")))?;
        a.act(Action::LibraryView);
        a.act(Action::Create);
        a.render();
        a.surface
            .save_png(&output.join(format!("create-menu-{label}.png")))?;
        a.act(Action::Close);
        a.open(&id)?;
        a.render();
        a.surface
            .save_png(&output.join(format!("notebook-{label}.png")))?;
        a.act(Action::Tool("diagram".into()));
        a.render();
        a.surface
            .save_png(&output.join(format!("diagram-menu-{label}.png")))?;
        a.act(Action::Shape(pinepaper::diagram::Shape::Arrow));
        a.render();
        let start = a.view.screen([180., 420., 0.5]);
        let end = a.view.screen([500., 490., 0.5]);
        a.down(start[0], start[1], 0.5, false);
        a.motion(end[0], end[1], 0.5);
        a.up(end[0], end[1]);
        a.render();
        a.surface
            .save_png(&output.join(format!("diagram-{label}.png")))?;
        a.act(Action::Tool("select".into()));
        a.render();
        let center = a.view.screen([340., 455., 0.5]);
        a.down(center[0], center[1], 0.5, false);
        a.up(center[0], center[1]);
        a.render();
        a.surface
            .save_png(&output.join(format!("selected-arrow-{label}.png")))?;
        let end = a.view.screen([500., 490., 0.5]);
        let next = a.view.screen([560., 410., 0.5]);
        a.down(end[0], end[1], 0.5, false);
        a.render();
        a.motion(next[0], next[1], 0.5);
        a.up(next[0], next[1]);
        a.render();
        a.surface
            .save_png(&output.join(format!("edited-arrow-{label}.png")))?;
        a.act(Action::Undo); // Endpoint change.
        a.act(Action::Undo); // Original diagram.
        a.act(Action::Shape(pinepaper::diagram::Shape::Rectangle));
        a.render();
        let start = a.view.screen([180., 340., 0.5]);
        let end = a.view.screen([460., 440., 0.5]);
        a.down(start[0], start[1], 0.5, false);
        a.up(end[0], end[1]);
        a.act(Action::Tool("select".into()));
        a.render();
        let center = a.view.screen([320., 390., 0.5]);
        a.down(center[0], center[1], 0.5, false);
        a.up(center[0], center[1]);
        a.render();
        a.surface
            .save_png(&output.join(format!("selected-shape-{label}.png")))?;
        let handle = a
            .selected_resize_handles()
            .into_iter()
            .find(|(kind, _)| *kind == pinepaper::diagram::ResizeHandle::SouthEast)
            .unwrap()
            .1;
        let start = a.view.screen(handle);
        let end = a.view.screen([handle[0] + 70., handle[1] + 40., 0.5]);
        a.down(start[0], start[1], 0.5, false);
        a.render();
        a.queue_motion(end[0], end[1], 0.5);
        a.render();
        a.up(end[0], end[1]);
        a.render();
        a.surface
            .save_png(&output.join(format!("resized-shape-{label}.png")))?;
        a.act(Action::Undo);
        a.act(Action::Undo);
        // Snap preview and release in both orientations, using the normal input path.
        for (shape, from, to) in [
            (
                pinepaper::diagram::Shape::Rectangle,
                [500., 380., 0.5],
                [650., 500., 0.5],
            ),
            (
                pinepaper::diagram::Shape::Arrow,
                [180., 440., 0.5],
                [400., 440., 0.5],
            ),
        ] {
            a.act(Action::Shape(shape));
            a.render();
            let p = a.view.screen(from);
            let q = a.view.screen(to);
            a.down(p[0], p[1], 0.5, false);
            a.up(q[0], q[1]);
        }
        a.act(Action::Tool("select".into()));
        a.act(Action::Magnet);
        a.render();
        let p = a.view.screen([290., 440., 0.5]);
        a.down(p[0], p[1], 0.5, false);
        a.up(p[0], p[1]);
        a.render();
        let p = a.view.screen([400., 440., 0.5]);
        let q = a.view.screen([495., 442., 0.5]);
        a.down(p[0], p[1], 0.5, false);
        a.render();
        a.motion(q[0], q[1], 0.5);
        a.render();
        anyhow::ensure!(a.snap_target.is_some(), "Magnet preview did not snap");
        a.surface
            .save_png(&output.join(format!("magnet-active-{label}.png")))?;
        a.up(q[0], q[1]);
        a.render();
        a.surface
            .save_png(&output.join(format!("magnet-released-{label}.png")))?;
        a.act(Action::Undo);
        a.act(Action::Undo);
        a.act(Action::Undo);
        a.act(Action::Magnet);
        a.store.set("quick_sheets", &Some(&id))?;
        a.act(Action::MakeNotebook);
        a.act(Action::SelectPages(true));
        a.render();
        a.surface
            .save_png(&output.join(format!("move-pages-{label}.png")))?;
        a.act(Action::CancelPageSelection);
        a.act(Action::Tool("pen".into()));
        a.act(Action::PenPanel);
        a.render();
        a.surface
            .save_png(&output.join(format!("pen-menu-{label}.png")))?;
        a.act(Action::Close);
        a.act(Action::Menu);
        a.render();
        a.surface
            .save_png(&output.join(format!("page-menu-{label}.png")))?;
        a.act(Action::Close);
        a.render();
        // Exercise the same app input methods the Wayland tablet protocol calls.
        let before = a.page().unwrap().payload.strokes.len();
        let v = a.view;
        let start = v.screen([100., 450., 0.5]);
        a.down(start[0], start[1], 0.5, false);
        for i in 1..241 {
            let p = v.screen([
                100. + i as f64,
                450. + (i as f64 / 20.).sin() * 35.,
                i as f64 / 300.,
            ]);
            a.motion(p[0], p[1], p[2]);
        }
        a.up(start[0] + 240. * v.zoom, start[1]);
        a.saver.flush()?;
        anyhow::ensure!(
            a.page().unwrap().payload.strokes.len() == before + 1,
            "Input did not persist a stroke"
        );
        a.act(Action::Undo);
        a.saver.flush()?;
        anyhow::ensure!(
            a.page().unwrap().payload.strokes.len() == before,
            "Undo failed"
        );
        a.act(Action::Overview);
        a.render();
        a.surface
            .save_png(&output.join(format!("overview-{label}.png")))?;
        a.act(Action::Page(0));
        a.act(Action::Tags);
        a.act(Action::SelectAll);
        a.act(Action::Key("Deutsch, pensée, żółć".into()));
        a.render();
        a.surface
            .save_png(&output.join(format!("keyboard-{label}.png")))?;
        a.act(Action::Accept);
        anyhow::ensure!(a.page().unwrap().tags.len() == 3, "Tags failed");
        if matches!(a.dialog,Some(Dialog::Input(ref i))if matches!(i.purpose,InputPurpose::Tags)) {
            anyhow::bail!("Tag input did not close");
        }
        let mut hardware = Hardware {
            cool_max: 255,
            warm_max: 255,
            driver: Some(1),
            dither: Some(2),
            delay: Some(350),
            hint: Some("Y1|T|R".into()),
            ..Hardware::default()
        };
        hardware.lights(20., 85.);
        for (tab, name) in ["light", "page", "refresh", "keyboard", "sharing"]
            .iter()
            .enumerate()
        {
            let mut prefs = a.prefs.clone();
            prefs.display_hint = "Y1|T|R".into();
            a.dialog = Some(Dialog::Settings(Box::new(Settings {
                tab,
                prefs,
                hardware: hardware.clone(),
                before: hardware.clone(),
                brightness: 20.,
                warmth: 85.,
                loaded: true,
                display_busy: false,
                light_busy: false,
                error: None,
                scroll: 0.,
            })));
            a.redraw = true;
            a.render();
            a.surface
                .save_png(&output.join(format!("settings-{name}-{label}.png")))?;
            if h < 800 && (tab == 1 || tab == 2) {
                a.scroll_by(if tab == 1 { 150. } else { 280. });
                a.render();
                a.surface
                    .save_png(&output.join(format!("settings-{name}-scrolled-{label}.png")))?;
            }
        }
        a.act(Action::Sharing);
        a.render();
        a.surface
            .save_png(&output.join(format!("sharing-{label}.png")))?;
        a.dialog = None;
    }
    a.saver.flush()?;
    a.store.backup(&output.join("test-library.zip"))?;
    pinepaper::document::export_pdf(&output.join("test-export.pdf"), &a.pages, None)?;
    let reopened = model::Store::open(tmp.path())?;
    anyhow::ensure!(
        reopened.pages(&id)?[0].payload.strokes.len() == 1,
        "Saved page mismatch"
    );
    println!(
        "Smoke checks passed; screenshots, PDF and backup: {}",
        output.display()
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("PinePaper: {error:#}");
        std::process::exit(1);
    }
}
