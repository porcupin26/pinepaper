//! CPU preview timings at PineNote resolution; uses a temporary, disposable library.
//! cargo run --release --example interaction_bench
use pinepaper::{
    app::{Action, App},
    diagram::Shape,
    model::*,
};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut app = App::new(dir.path().into())?;
    app.resize(702, 936, 2);
    app.act(Action::New("notebook".into()));
    for crowded in [false, true] {
        app.pages[0].payload = Payload::blank("Grid");
        if crowded {
            for row in 0..80 {
                app.pages[0].payload.strokes.push(Stroke {
                    points: (0..120)
                        .map(|n| {
                            [
                                90. + n as f64 * 4.,
                                120. + row as f64 * 9. + (n as f64 * 0.7).sin() * 3.,
                                0.5,
                            ]
                        })
                        .collect(),
                    ..Stroke::default()
                });
            }
        }
        app.act(Action::Shape(Shape::Arrow));
        app.render();
        let start = [100., 180., 0.5];
        let end = [600., 850., 0.5];
        let p = app.view.screen(start);
        app.down(p[0], p[1], 0.5, false);
        measure(&mut app, "arrow", crowded, |i| {
            [
                end[0] + (i as f64 * 0.1).sin() * 30.,
                end[1] - i as f64 * 0.5,
                0.5,
            ]
        });
        let p = app.view.screen(end);
        app.up(p[0], p[1]);
        app.act(Action::Tool("select".into()));
        app.render();
        let middle = [(start[0] + end[0]) / 2., (start[1] + end[1]) / 2., 0.5];
        let p = app.view.screen(middle);
        app.down(p[0], p[1], 0.5, false);
        app.render();
        measure(&mut app, "move", crowded, |i| {
            [
                middle[0] + 25. * (i as f64 * 0.12).sin(),
                middle[1] + i as f64 * 0.5,
                0.5,
            ]
        });
        app.up(p[0], p[1]);
        app.act(Action::Undo);
        app.render();
        app.act(Action::Shape(Shape::Ellipse));
        app.render();
        let start = app.view.screen([180., 200., 0.5]);
        let end = app.view.screen([480., 600., 0.5]);
        app.down(start[0], start[1], 0.5, false);
        app.up(end[0], end[1]);
        app.act(Action::Tool("select".into()));
        app.render();
        let center = app.view.screen([330., 400., 0.5]);
        app.down(center[0], center[1], 0.5, false);
        app.up(center[0], center[1]);
        app.render();
        // Select by the outline on crowded pages (ink wins over empty interiors).
        if app.selected_resize_handles().is_empty() {
            let edge = app.view.screen([480., 400., 0.5]);
            app.down(edge[0], edge[1], 0.5, false);
            app.up(edge[0], edge[1]);
            app.render();
        }
        let handle = app
            .selected_resize_handles()
            .into_iter()
            .find(|(kind, _)| *kind == pinepaper::diagram::ResizeHandle::SouthEast)
            .unwrap()
            .1;
        let p = app.view.screen(handle);
        app.down(p[0], p[1], 0.5, false);
        app.render();
        measure(&mut app, "resize", crowded, |i| {
            [
                handle[0] + 30. * (i as f64 * 0.12).sin(),
                handle[1] + i as f64 * 0.5,
                0.5,
            ]
        });
        app.up(p[0], p[1]);
    }
    app.saver.flush()?;
    Ok(())
}
fn measure(app: &mut App, action: &str, crowded: bool, point: impl Fn(usize) -> Point) {
    let mut times = Vec::new();
    for i in 0..100 {
        let p = app.view.screen(point(i));
        let start = Instant::now();
        app.motion(p[0], p[1], 0.5);
        app.render();
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{action} crowded={crowded}: median={:.2}ms p95={:.2}ms",
        times[50], times[95]
    );
}
