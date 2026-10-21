//! Monochrome, touch-sized controls. Widgets never participate in the live ink path.
use crate::{
    app::{Action, App, Dialog, Input, Screen, Settings, SliderSetting, SwitchSetting},
    icons::Icon,
    model::{Stroke, PAGE_H, PAGE_W, PENS},
    render::{Box2, View, BACK, BLACK, GRAY, LINE, WHITE},
};

fn button_chrome(a: &mut App, b: Box2, action: Action, selected: bool) -> u32 {
    let fill = if selected { BLACK } else { WHITE };
    let radius = if matches!(&action, Action::Filter(_)) {
        b.h / 2.
    } else {
        10.
    };
    a.surface
        .rounded(b, radius, if selected { BLACK } else { LINE });
    a.surface.rounded(
        Box2::new(b.x + 1., b.y + 1., b.w - 2., b.h - 2.),
        radius - 1.,
        fill,
    );
    a.hits.push((b, action));
    if selected {
        WHITE
    } else {
        BLACK
    }
}
fn button(a: &mut App, label: &str, b: Box2, action: Action, selected: bool) {
    let color = button_chrome(a, b, action, selected);
    a.surface.centered_label(
        label,
        Box2::new(b.x + 6., b.y, b.w - 12., b.h),
        a.prefs.ui_font_size,
        color,
    );
}
fn icon_button(a: &mut App, icon: Icon, b: Box2, action: Action, selected: bool) {
    if selected {
        a.surface
            .rounded(Box2::new(b.x + 4., b.y + 2., b.w - 8., b.h - 4.), 4., BLACK);
    }
    a.hits.push((b, action));
    let color = if selected { WHITE } else { BLACK };
    let size = 22_f64.min(b.w - 8.).min(b.h - 8.);
    icon.draw(
        &mut a.surface,
        Box2::new(b.x + (b.w - size) / 2., b.y + (b.h - size) / 2., size, size),
        color,
    );
}
fn icon_label_button(
    a: &mut App,
    icon: Icon,
    label: &str,
    b: Box2,
    action: Action,
    selected: bool,
) {
    let color = button_chrome(a, b, action, selected);
    icon.draw(
        &mut a.surface,
        Box2::new(b.x + 10., b.y + (b.h - 22.) / 2., 22., 22.),
        color,
    );
    a.surface.label(
        label,
        Box2::new(b.x + 42., b.y, b.w - 48., b.h),
        a.prefs.ui_font_size,
        color,
    );
}
fn fullscreen_icon(a: &App) -> Icon {
    if a.fullscreen {
        Icon::Contract
    } else {
        Icon::Expand
    }
}
fn menu_icon(action: &Action) -> Option<Icon> {
    Some(match action {
        Action::New(kind) if kind == "folder" => Icon::Folder,
        Action::New(_) => Icon::Book,
        Action::Import | Action::Export | Action::Backup => Icon::Download,
        Action::Settings | Action::ExchangeUrl => Icon::Settings,
        Action::Sharing | Action::StartSharing | Action::JoinSharing => Icon::Share,
        Action::Overview | Action::TemplatePanel => Icon::Grid,
        Action::Bookmark => Icon::Bookmark,
        Action::Tags => Icon::Tag,
        Action::AddPage => Icon::Plus,
        Action::DeletePage | Action::Trash(_, true) | Action::DeleteSelection => Icon::Trash,
        Action::Trash(_, false) => Icon::Undo,
        Action::ClearPage => Icon::Eraser,
        Action::Rename(_) | Action::PenPanel | Action::Pen(_) => Icon::Pen,
        Action::Move(_) | Action::MoveTo(_, _) => Icon::Folder,
        Action::Fit => Icon::Expand,
        Action::Refresh => Icon::Refresh,
        Action::Help => Icon::Book,
        Action::TrashView => Icon::Trash,
        Action::SortBy(_) => Icon::Sort,
        Action::Template(_) => Icon::Grid,
        _ => return None,
    })
}
fn text(a: &mut App, s: &str, x: f64, y: f64, size: f64, c: u32, w: f64) {
    a.surface.text(s, x, y, size, c, w, false);
}
fn top(a: &mut App, title: &str) {
    let w = a.surface.logical_w;
    a.surface.fill(Box2::new(0., 0., w, 34.), WHITE);
    a.surface.label(
        title,
        Box2::new(18., 0., (w - 300.).max(70.), 34.),
        13.,
        GRAY,
    );
    let b = Box2::new(
        (w - 276.).max(120.),
        0.,
        (w - 18. - (w - 276.).max(120.)).max(1.),
        34.,
    );
    let wifi_x = w - 248.;
    let level = a.status.signal.unwrap_or(0);
    for n in 0..4 {
        let height = 4. + n as f64 * 4.;
        a.surface.fill(
            Box2::new(wifi_x + n as f64 * 5., 24. - height, 3., height),
            if a.status.wifi == "Connected" && level as usize > n * 25 {
                BLACK
            } else {
                LINE
            },
        );
    }
    a.surface.label(
        if a.status.wifi == "Connected" {
            "Wi-Fi"
        } else {
            &a.status.wifi
        },
        Box2::new(wifi_x + 28., 0., 104., 34.),
        12.,
        BLACK,
    );
    let x = w - 112.;
    let battery = Box2::new(x, 10., 27., 14.);
    a.surface.border(battery, BLACK, 1.);
    a.surface.fill(Box2::new(x + 27., 14., 3., 6.), BLACK);
    if let Some(percent) = a.status.battery {
        a.surface.fill(
            Box2::new(x + 2., 12., 23. * percent as f64 / 100., 10.),
            BLACK,
        );
    }
    let label = a
        .status
        .battery
        .map(|v| format!("{}{}%", if a.status.charging { "ϟ" } else { "" }, v))
        .unwrap_or("--".into());
    a.surface
        .label(&label, Box2::new(x + 36., 0., 72., 34.), 12., BLACK);
    a.hits.push((b, Action::Status));
    a.surface.fill(Box2::new(0., 33., w, 1.), LINE);
}
pub fn status(a: &mut App) {
    if a.surface.width == 0 {
        return;
    }
    let title = "PINEPAPER";
    let old_len = a.hits.len();
    top(a, title);
    a.hits.truncate(old_len);
    let r = a.surface.rect(Box2::new(0., 0., a.surface.logical_w, 34.));
    a.dirty(r);
    if a.screen == Screen::Editor && a.dialog.is_none() {
        footer(a);
        let r = a.surface.rect(Box2::new(
            0.,
            a.surface.logical_h - 34.,
            a.surface.logical_w,
            34.,
        ));
        a.dirty(r);
    }
}
pub fn render(a: &mut App) {
    a.surface.clear_tone();
    a.surface.reset_clip();
    a.surface.pixels.fill(WHITE);
    a.hits.clear();
    a.popup_bounds = None;
    match a.screen {
        Screen::Library => library(a),
        Screen::Editor => editor(a),
        Screen::Overview => overview(a),
    }
    if let Some(dialog) = a.dialog.clone() {
        a.hits.clear();
        dialog_render(a, dialog);
    }
    if a.busy {
        a.hits.clear();
        let w = a.surface.logical_w;
        let h = a.surface.logical_h;
        a.surface
            .fill(Box2::new(20., h / 2. - 38., w - 40., 76.), WHITE);
        a.surface
            .border(Box2::new(20., h / 2. - 38., w - 40., 76.), BLACK, 2.);
        text(a, &a.notice.clone(), 40., h / 2. - 12., 18., BLACK, w - 80.);
    }
    a.surface.reset_clip();
}
fn library(a: &mut App) {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    top(a, "PINEPAPER");
    let title = if a.trash {
        "Trash".into()
    } else if let Some(id) = &a.folder {
        a.store
            .document(id)
            .map(|d| d.title)
            .unwrap_or("Folder".into())
    } else {
        "My files".into()
    };
    icon_button(
        a,
        if a.folder.is_some() {
            Icon::Back
        } else {
            Icon::Menu
        },
        Box2::new(16., 44., 44., 42.),
        if a.folder.is_some() {
            Action::Up
        } else {
            Action::LibraryMenu
        },
        false,
    );
    text(a, &title, 74., 54., 23., BLACK, (w - 440.).max(100.));
    let mut x = (w - 354.).max(200.);
    for (icon, bw, action) in [
        (Icon::Search, 70., Action::Search),
        (fullscreen_icon(a), 54., Action::Fullscreen),
        (Icon::Settings, 76., Action::Settings),
    ] {
        icon_button(a, icon, Box2::new(x, 44., bw, 42.), action, false);
        x += bw + 8.;
    }
    icon_label_button(
        a,
        Icon::Plus,
        "Create",
        Box2::new(x, 44., 100., 42.),
        Action::Create,
        true,
    );
    let mut x = 16.;
    for (label, kind, bw) in [
        ("All", "", 48.),
        ("Notebooks", "notebook", 104.),
        ("Books", "book", 68.),
        ("PDFs", "pdf", 60.),
    ] {
        button(
            a,
            label,
            Box2::new(x, 100., bw, 36.),
            Action::Filter(kind.into()),
            a.filter == kind,
        );
        x += bw + 8.;
    }
    icon_label_button(
        a,
        Icon::Sort,
        ["Recent", "Name", "Created"][a.sort],
        Box2::new(w - 140., 100., 124., 36.),
        Action::Sort,
        false,
    );
    if !a.query.is_empty() {
        text(
            a,
            &format!("Search: {}", a.query),
            16.,
            145.,
            13.,
            GRAY,
            w - 40.,
        );
    }
    let cols = ((w - 24.) / 190.).floor().max(2.) as usize;
    let cw = (w - 32. - (cols - 1) as f64 * 18.) / cols as f64;
    let ch = (cw * 1.16 + 64.).min(316.);
    let top = if a.query.is_empty() { 154. } else { 174. };
    let rows = ((h - top - 48.) / ch).floor().max(1.) as usize;
    let docs: Vec<_> = a
        .docs
        .iter()
        .skip(a.library_offset)
        .take(cols * rows)
        .cloned()
        .collect();
    for (i, d) in docs.iter().enumerate() {
        let x = 16. + (i % cols) as f64 * (cw + 18.);
        let y = top + (i / cols) as f64 * ch;
        let ph = ch - 68.;
        let b = Box2::new(x, y, cw, ph);
        a.surface.fill(b, WHITE);
        a.surface.border(b, LINE, 1.);
        if d.kind == "folder" {
            let size = (cw * 0.45).min(80.);
            Icon::Folder.draw(
                &mut a.surface,
                Box2::new(x + (cw - size) / 2., y + (ph - size) / 2., size, size),
                BLACK,
            );
        } else if let Ok(pages) = a.store.pages(&d.id) {
            if let Some(p) = pages.first() {
                let zoom = ((cw - 16.) / PAGE_W).min((ph - 16.) / PAGE_H);
                let v = View {
                    x: x + (cw - PAGE_W * zoom) / 2.,
                    y: y + 8.,
                    zoom,
                    clip: Box2::new(x + 4., y + 4., cw - 8., ph - 8.),
                };
                if d.kind == "pdf" {
                    if let Ok(path) = a.store.source_path(d) {
                        a.request_thumbnail(&d.id, 0, path);
                    }
                }
                let preview = a.thumbnails.get(&(d.id.clone(), 0));
                a.surface.page(v, &p.payload, preview, true);
                if d.kind == "pdf" && preview.is_none() {
                    text(a, "PDF", x + 14., y + 14., 18., GRAY, cw - 28.);
                    text(a, &d.title, x + 14., y + 52., 14., BLACK, cw - 28.);
                }
            }
        }
        a.hits
            .push((Box2::new(x, y, cw, ch - 12.), Action::Open(d.id.clone())));
        a.surface.label(
            &d.title,
            Box2::new(x, y + ph + 7., cw - 40., 22.),
            14.,
            BLACK,
        );
        text(
            a,
            match d.kind.as_str() {
                "notebook" => "Notebook",
                "book" => "Book",
                "folder" => "Folder",
                _ => "PDF",
            },
            x,
            y + ph + 31.,
            11.,
            GRAY,
            cw - 44.,
        );
        icon_button(
            a,
            Icon::More,
            Box2::new(x + cw - 36., y + ph + 8., 36., 34.),
            Action::ItemMenu(d.id.clone()),
            false,
        );
    }
    if a.docs.is_empty() {
        text(
            a,
            if a.trash {
                "Nothing in the trash"
            } else {
                "Your library starts here"
            },
            26.,
            top + 70.,
            25.,
            BLACK,
            w - 52.,
        );
        text(
            a,
            "Create a notebook or import a PDF, EPUB or text file.",
            26.,
            top + 115.,
            16.,
            GRAY,
            w - 52.,
        );
    }
    a.surface.fill(Box2::new(0., h - 44., w, 44.), WHITE);
    button(
        a,
        if a.trash { "My files" } else { "Trash" },
        Box2::new(16., h - 39., 76., 32.),
        Action::TrashView,
        false,
    );
    button(
        a,
        "Backup",
        Box2::new(100., h - 39., 82., 32.),
        Action::Backup,
        false,
    );
    button(
        a,
        "Quit",
        Box2::new(190., h - 39., 64., 32.),
        Action::Quit,
        false,
    );
    text(
        a,
        &format!("{} items · scroll to browse", a.docs.len()),
        (w - 238.).max(270.),
        h - 28.,
        12.,
        GRAY,
        225.,
    );
}
fn footer(a: &mut App) {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    a.surface.fill(Box2::new(0., h - 34., w, 34.), WHITE);
    a.surface.fill(Box2::new(0., h - 34., w, 1.), LINE);
    let n = a.pages.len().max(1);
    let progress = (a.index + 1) as f64 / n as f64;
    a.surface
        .fill(Box2::new(0., h - 2., w * progress, 2.), BLACK);
    let msg = if !a.notice.is_empty() {
        a.notice.clone()
    } else if a.saver.pending > 0 {
        "Saving…".into()
    } else if let Some(label) = a.sharing_label() {
        label
    } else {
        "Saved".into()
    };
    a.surface.label(
        &msg,
        Box2::new(18., h - 32., (w - 240.).max(40.), 28.),
        12.,
        GRAY,
    );
    a.surface.label(
        &format!("{} / {}    {:.0}%", a.index + 1, n, 100. * progress),
        Box2::new(w - 166., h - 32., 150., 28.),
        12.,
        BLACK,
    );
    // Status updates repaint this footer too, so navigation must be redrawn here.
    Icon::Back.draw(
        &mut a.surface,
        Box2::new(w - 232., h - 30., 24., 24.),
        BLACK,
    );
    Icon::Next.draw(
        &mut a.surface,
        Box2::new(w - 196., h - 30., 24., 24.),
        BLACK,
    );
}
fn editor(a: &mut App) {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    let header = if a.hidden { 40. } else { 94. };
    let left = if a.hidden { 0. } else { 68. };
    let clip = Box2::new(left, header, w - left, h - header - 34.);
    a.surface.fill(clip, BACK);
    let zoom = if a.fit {
        ((w - left - 24.) / PAGE_W).min((h - header - 48.) / PAGE_H)
    } else {
        (w - left - 24.) / PAGE_W
    };
    let zoom = zoom.max(0.01);
    a.scroll = a.scroll.clamp(0., (PAGE_H * zoom - clip.h).max(0.));
    let v = View {
        x: left + (w - left - PAGE_W * zoom) / 2.,
        y: if a.fit {
            header + (clip.h - PAGE_H * zoom) / 2.
        } else {
            header + 4. - a.scroll
        },
        zoom,
        clip,
    };
    a.view = v;
    if let Some(p) = a.pages.get(a.index) {
        a.surface.page(v, &p.payload, a.pdf.as_ref(), true);
    }
    if let Some(s) = a.live_stroke().cloned() {
        a.surface.stroke(v, &s);
    }
    a.surface
        .tone(clip, a.prefs.page_contrast, a.prefs.page_gamma);
    if let Some(b) = a.selection.bounds {
        a.surface.border(
            Box2::new(
                v.x + b.x * v.zoom,
                v.y + b.y * v.zoom,
                b.w * v.zoom,
                b.h * v.zoom,
            ),
            GRAY,
            1.,
        );
    }
    top(a, "PINEPAPER");
    if !a.hidden {
        a.surface.fill(Box2::new(0., 93., w, 1.), LINE);
        a.surface.fill(Box2::new(67., 94., 1., h - 128.), LINE);
    }
    if a.hidden {
        icon_button(
            a,
            Icon::Menu,
            Box2::new(6., 42., 44., 42.),
            Action::Tools,
            false,
        );
    } else {
        icon_button(
            a,
            Icon::Back,
            Box2::new(10., 42., 44., 42.),
            Action::Library,
            false,
        );
        let title = a.doc.as_ref().map(|d| d.title.clone()).unwrap_or_default();
        a.surface.label(
            &title,
            Box2::new(66., 42., (w - 418.).max(80.), 42.),
            18.,
            BLACK,
        );
        if let Some(d) = &a.doc {
            a.hits.push((
                Box2::new(66., 42., (w - 418.).max(80.), 42.),
                Action::Rename(d.id.clone()),
            ));
        }
        let mark = a.page().is_some_and(|p| p.bookmarked);
        let mut x = w - 332.;
        for (icon, bw, act, sel) in [
            (Icon::Bookmark, 42., Action::Bookmark, mark),
            (Icon::Grid, 66., Action::Overview, false),
            (fullscreen_icon(a), 54., Action::Fullscreen, false),
            (Icon::Hide, 54., Action::Tools, false),
            (Icon::More, 42., Action::Menu, false),
        ] {
            icon_button(a, icon, Box2::new(x, 42., bw, 42.), act, sel);
            x += bw + 8.;
        }
        let mut y = 104.;
        for (icon, tool) in [
            (Icon::Pen, "pen"),
            (Icon::Highlighter, "highlighter"),
            (Icon::Eraser, "eraser"),
            (Icon::Lasso, "lasso"),
            (Icon::Text, "text"),
        ] {
            icon_button(
                a,
                icon,
                Box2::new(6., y, 56., 44.),
                Action::Tool(tool.into()),
                a.tool == tool,
            );
            y += 50.;
        }
        a.surface.fill(Box2::new(17., y + 5., 34., 1.), LINE);
        y += 12.;
        for (icon, act) in [
            (Icon::Undo, Action::Undo),
            (Icon::Redo, Action::Redo),
            (Icon::Trash, Action::DeleteSelection),
        ] {
            icon_button(a, icon, Box2::new(6., y, 56., 42.), act, false);
            y += 48.;
        }
        if y + 45. < h - 38. {
            icon_button(
                a,
                Icon::Plus,
                Box2::new(6., y, 56., 42.),
                Action::AddPage,
                false,
            );
        }
    }
    footer(a);
    let b = Box2::new(w - 234., h - 32., 28., 28.);
    a.hits.push((b, Action::Previous));
    let b = Box2::new(w - 198., h - 32., 28., 28.);
    a.hits.push((b, Action::Next));
}
fn overview(a: &mut App) {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    top(a, "PAGE OVERVIEW");
    icon_label_button(
        a,
        Icon::Back,
        "Back",
        Box2::new(16., 44., 94., 42.),
        Action::Page(a.index),
        false,
    );
    let title = a.doc.as_ref().map(|d| d.title.clone()).unwrap_or_default();
    a.surface.label(
        &title,
        Box2::new(126., 44., (w - 422.).max(60.), 42.),
        20.,
        BLACK,
    );
    icon_label_button(
        a,
        Icon::Grid,
        "Template",
        Box2::new(w - 262., 44., 142., 42.),
        Action::TemplatePanel,
        false,
    );
    icon_button(
        a,
        fullscreen_icon(a),
        Box2::new(w - 106., 44., 56., 42.),
        Action::Fullscreen,
        false,
    );
    let mut x = 16.;
    for tab in ["Pages", "Bookmarks", "Tags"] {
        let bw = if tab == "Bookmarks" { 118. } else { 80. };
        button(
            a,
            tab,
            Box2::new(x, 102., bw, 38.),
            Action::OverviewTab(tab.into()),
            a.overview_tab == tab,
        );
        x += bw + 8.;
    }
    let mut start = 160.;
    if a.overview_tab == "Tags" {
        let tags: std::collections::BTreeSet<_> = a
            .pages
            .iter()
            .flat_map(|p| p.tags.iter().cloned())
            .collect();
        let mut x = 16.;
        for tag in tags {
            let width = (tag.chars().count() as f64 * 8. + 24.).min(160.);
            if x + width > w - 16. {
                x = 16.;
                start += 40.;
            }
            button(
                a,
                &tag,
                Box2::new(x, start, width, 32.),
                Action::TagFilter(tag.clone()),
                a.tag.as_ref() == Some(&tag),
            );
            x += width + 8.;
        }
        start += 48.;
    }
    let cols = ((w - 32.) / 175.).floor().max(2.) as usize;
    let cw = (w - 32. - (cols - 1) as f64 * 18.) / cols as f64;
    let ch = (cw * 1.25 + 36.).min(290.);
    let rows = ((h - start - 40.) / ch).floor().max(1.) as usize;
    let pages: Vec<_> = a
        .pages
        .iter()
        .enumerate()
        .filter(|(_, p)| match a.overview_tab.as_str() {
            "Bookmarks" => p.bookmarked,
            "Tags" => a.tag.as_ref().is_some_and(|t| p.tags.contains(t)),
            _ => true,
        })
        .map(|(i, p)| (i, p.clone()))
        .collect();
    for (n, (i, p)) in pages
        .iter()
        .skip(a.overview_offset)
        .take(cols * rows)
        .enumerate()
    {
        let x = 16. + (n % cols) as f64 * (cw + 18.);
        let y = start + (n / cols) as f64 * ch;
        let ph = ch - 40.;
        let b = Box2::new(x, y, cw, ph);
        let zoom = ((cw - 12.) / PAGE_W).min((ph - 12.) / PAGE_H);
        if let Some(d) = a.doc.clone() {
            if d.kind == "pdf" {
                if let Ok(path) = a.store.source_path(&d) {
                    a.request_thumbnail(&d.id, *i, path);
                }
            }
        }
        a.surface.page(
            View {
                x: x + (cw - PAGE_W * zoom) / 2.,
                y: y + 6.,
                zoom,
                clip: b,
            },
            &p.payload,
            a.doc
                .as_ref()
                .and_then(|d| a.thumbnails.get(&(d.id.clone(), *i)))
                .or(if *i == a.index { a.pdf.as_ref() } else { None }),
            true,
        );
        a.surface.border(
            b,
            if *i == a.index { BLACK } else { LINE },
            if *i == a.index { 2. } else { 1. },
        );
        text(
            a,
            &format!("{}{}", i + 1, if *i == a.index { " · current" } else { "" },),
            x,
            y + ph + 8.,
            12.,
            BLACK,
            cw - 24.,
        );
        if p.bookmarked {
            Icon::Bookmark.draw(
                &mut a.surface,
                Box2::new(x + cw - 20., y + ph + 6., 18., 18.),
                BLACK,
            );
        }
        a.hits
            .push((Box2::new(x, y, cw, ch - 6.), Action::Page(*i)));
    }
    if pages.len().saturating_sub(a.overview_offset) < cols * rows
        && a.overview_tab == "Pages"
        && a.doc.as_ref().is_some_and(|d| d.kind == "notebook")
    {
        let n = pages.len().saturating_sub(a.overview_offset);
        let x = 16. + (n % cols) as f64 * (cw + 18.);
        let y = start + (n / cols) as f64 * ch;
        let b = Box2::new(x, y, cw, ch - 40.);
        button_chrome(a, b, Action::AddPage, false);
        Icon::Plus.draw(
            &mut a.surface,
            Box2::new(x + cw / 2. - 16., y + b.h / 2. - 28., 32., 32.),
            BLACK,
        );
        a.surface.label(
            "Add page",
            Box2::new(x + cw / 2. - 38., y + b.h / 2. + 14., 90., 28.),
            14.,
            BLACK,
        );
    }
    if pages.is_empty() {
        text(
            a,
            if a.overview_tab == "Tags" {
                "Add tags from the page menu, then select a tag above."
            } else {
                "Bookmark pages to find them here."
            },
            24.,
            start + 40.,
            18.,
            GRAY,
            w - 48.,
        );
    }
    text(
        a,
        &format!("{} pages · scroll to browse", a.pages.len()),
        18.,
        h - 28.,
        12.,
        GRAY,
        w - 36.,
    );
}
fn popover(a: &mut App, title: &str, width: f64, height: f64, beside: bool) -> Box2 {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    let width = width.min(w - 32.);
    let height = height.min(h - 64.);
    let anchor = if beside {
        Box2::new(6., 104., 56., 44.)
    } else {
        a.popup_anchor.unwrap_or(Box2::new(w - 60., 42., 44., 42.))
    };
    let x = if beside {
        anchor.x + anchor.w + 10.
    } else if anchor.x > w / 2. {
        anchor.x + anchor.w - width
    } else {
        anchor.x
    };
    let y = if beside {
        anchor.y
    } else if anchor.y + anchor.h + height + 8. <= h - 16. {
        anchor.y + anchor.h + 8.
    } else {
        anchor.y - height - 8.
    };
    let b = Box2::new(
        x.clamp(16., (w - width - 16.).max(16.)),
        y.clamp(42., (h - height - 16.).max(42.)),
        width,
        height,
    );
    a.surface.clear_tone();
    a.surface.rounded(b, 6., GRAY);
    a.surface
        .rounded(Box2::new(b.x + 1., b.y + 1., b.w - 2., b.h - 2.), 5., WHITE);
    // The outside tap belongs to the popup: dismiss it without drawing underneath.
    a.hits.push((Box2::new(0., 0., w, h), Action::Close));
    a.hits.push((b, Action::None));
    a.popup_bounds = Some(b);
    a.surface.label(
        title,
        Box2::new(b.x + 16., b.y + 8., b.w - 72., 38.),
        17.,
        BLACK,
    );
    icon_button(
        a,
        Icon::Close,
        Box2::new(b.x + b.w - 48., b.y + 4., 44., 44.),
        Action::Close,
        false,
    );
    b
}
fn menu_popup(a: &mut App, title: &str, items: &[(String, Action)]) {
    let capacity = (((a.surface.logical_h - 170.) / 48.).floor() as usize).clamp(1, 8);
    let count = items.len().min(capacity);
    let scroll = items.len() > capacity;
    a.menu_offset = a.menu_offset.min(items.len().saturating_sub(count));
    let b = popover(
        a,
        title,
        352.,
        58. + count as f64 * 48. + if scroll { 42. } else { 0. },
        false,
    );
    for (i, (label, act)) in items.iter().skip(a.menu_offset).take(count).enumerate() {
        let row = Box2::new(b.x + 10., b.y + 50. + i as f64 * 48., b.w - 20., 44.);
        let selected = match act {
            Action::SortBy(sort) => *sort == a.sort,
            Action::Template(t) => a.page().is_some_and(|p| &p.payload.template == t),
            _ => false,
        };
        if selected {
            a.surface.rounded(row, 4., BLACK);
        }
        let color = if selected { WHITE } else { BLACK };
        if let Some(icon) = menu_icon(act) {
            icon.draw(
                &mut a.surface,
                Box2::new(row.x + 10., row.y + 11., 22., 22.),
                color,
            );
        }
        a.surface.label(
            label,
            Box2::new(row.x + 44., row.y, row.w - 50., row.h),
            a.prefs.ui_font_size,
            color,
        );
        a.hits.push((row, act.clone()));
    }
    if scroll {
        let y = b.y + b.h - 42.;
        icon_button(
            a,
            Icon::Back,
            Box2::new(b.x + 10., y, 44., 38.),
            Action::MenuScroll(-(capacity as isize)),
            false,
        );
        icon_button(
            a,
            Icon::Next,
            Box2::new(b.x + b.w - 54., y, 44., 38.),
            Action::MenuScroll(capacity as isize),
            false,
        );
        a.surface.label(
            &format!(
                "{}–{} / {}",
                a.menu_offset + 1,
                a.menu_offset + count,
                items.len()
            ),
            Box2::new(b.x + 64., y, b.w - 128., 38.),
            12.,
            GRAY,
        );
    }
}
fn pen_popup(a: &mut App) {
    let b = popover(a, "Pen", 368., 404., true);
    text(a, "PEN TYPE", b.x + 16., b.y + 54., 11., GRAY, b.w - 32.);
    let cw = (b.w - 40.) / 2.;
    for (i, name) in PENS.iter().enumerate() {
        let row = Box2::new(
            b.x + 16. + (i % 2) as f64 * (cw + 8.),
            b.y + 76. + (i / 2) as f64 * 48.,
            cw,
            42.,
        );
        let selected = a.nib.pen == *name;
        if selected {
            a.surface.rounded(row, 4., BLACK);
        }
        a.hits.push((row, Action::Pen((*name).into())));
        let ink = Stroke {
            pen: (*name).into(),
            width: 1.4,
            color: if selected { 1. } else { 0.07 },
            points: (0..24)
                .map(|n| {
                    [
                        row.x + 8. + n as f64,
                        row.y + 22. - (n as f64 / 10.).sin() * 4.,
                        0.5,
                    ]
                })
                .collect(),
            ..Stroke::default()
        };
        a.surface.stroke(
            View {
                x: 0.,
                y: 0.,
                zoom: 1.,
                clip: row,
            },
            &ink,
        );
        a.surface.label(
            name,
            Box2::new(row.x + 40., row.y, row.w - 44., row.h),
            a.prefs.ui_font_size.min(14.),
            if selected { WHITE } else { BLACK },
        );
    }
    a.surface
        .fill(Box2::new(b.x + 16., b.y + 224., b.w - 32., 1.), LINE);
    text(a, "THICKNESS", b.x + 16., b.y + 238., 11., GRAY, b.w - 32.);
    let cw = (b.w - 48.) / 3.;
    for (i, (value, label)) in [(1.4, "Fine"), (2.4, "Medium"), (4., "Bold")]
        .iter()
        .enumerate()
    {
        let row = Box2::new(b.x + 16. + i as f64 * (cw + 8.), b.y + 260., cw, 42.);
        let color = button_chrome(a, row, Action::Width(*value), a.nib.width == *value);
        let center = (row.x + 15., row.y + row.h / 2.);
        a.surface
            .line(center, center, value * 2., (color & 255) as f64 / 255.);
        a.surface.label(
            label,
            Box2::new(row.x + 30., row.y, row.w - 34., row.h),
            13.,
            color,
        );
    }
    text(a, "COLOUR", b.x + 16., b.y + 320., 11., GRAY, b.w - 32.);
    let cw = (b.w - 56.) / 4.;
    for (i, (value, label)) in [
        (0.07, "Black"),
        (0.45, "Grey"),
        (0.72, "Light"),
        (0.95, "White"),
    ]
    .iter()
    .enumerate()
    {
        let row = Box2::new(b.x + 16. + i as f64 * (cw + 8.), b.y + 342., cw, 44.);
        let selected = (a.nib.color - value).abs() < 0.02;
        a.surface
            .rounded(row, 4., if selected { BLACK } else { LINE });
        a.surface.rounded(
            Box2::new(row.x + 1., row.y + 1., row.w - 2., row.h - 2.),
            3.,
            WHITE,
        );
        let center = (row.x + 15., row.y + row.h / 2.);
        a.surface.line(center, center, 13., 0.);
        a.surface.line(center, center, 11., *value);
        a.surface.label(
            label,
            Box2::new(row.x + 28., row.y, row.w - 30., row.h),
            12.,
            BLACK,
        );
        if selected {
            a.surface.fill(
                Box2::new(row.x + 5., row.y + row.h - 4., row.w - 10., 2.),
                BLACK,
            );
        }
        a.hits.push((row, Action::Color(*value)));
    }
}
fn dialog_render(a: &mut App, d: Dialog) {
    match &d {
        Dialog::Menu(title, items) => return menu_popup(a, title, items),
        Dialog::PenPanel => return pen_popup(a),
        Dialog::Input(i) => return input(a, i),
        _ => (),
    }
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    a.surface.fill(Box2::new(0., 34., w, h - 34.), BACK);
    top(a, "PINEPAPER");
    let margin = 16.;
    let card = Box2::new(margin, 48., w - margin * 2., h - 64.);
    a.surface.fill(card, WHITE);
    a.surface.border(card, LINE, 1.);
    match d {
        Dialog::Menu(..) | Dialog::PenPanel | Dialog::Input(_) => unreachable!(),
        Dialog::Settings(s) => settings(a, &s),
        Dialog::Sharing => sharing(a),
        Dialog::Message(title, body) => {
            text(a, &title, 32., 70., 24., BLACK, w - 64.);
            text(a, &body, 32., 124., 17., BLACK, w - 64.);
            button(
                a,
                "Done",
                Box2::new(w - 142., h - 80., 108., 44.),
                Action::Close,
                true,
            );
        }
        Dialog::Confirm(title, _) => {
            text(a, &title, 32., 88., 22., BLACK, w - 64.);
            button(
                a,
                "Cancel",
                Box2::new(32., h - 88., 120., 44.),
                Action::Close,
                false,
            );
            button(
                a,
                "Confirm",
                Box2::new(w - 158., h - 88., 124., 44.),
                Action::Confirm,
                true,
            );
        }
        Dialog::Files {
            dir,
            entries,
            offset,
        } => {
            text(a, "Import document", 32., 68., 23., BLACK, w - 190.);
            button(
                a,
                "Cancel",
                Box2::new(w - 120., 58., 88., 42.),
                Action::Close,
                false,
            );
            icon_label_button(
                a,
                Icon::Back,
                "Parent",
                Box2::new(32., 112., 116., 40.),
                Action::FileUp,
                false,
            );
            button(
                a,
                "Type path",
                Box2::new(w - 148., 112., 116., 40.),
                Action::ChooseFile,
                false,
            );
            a.surface.label(
                &dir.display().to_string(),
                Box2::new(32., 166., w - 64., 30.),
                14.,
                GRAY,
            );
            let count = ((h - 230.) / 50.).floor().max(1.) as usize;
            for (i, (name, path, folder)) in
                entries.into_iter().skip(offset).take(count).enumerate()
            {
                icon_label_button(
                    a,
                    if folder { Icon::Folder } else { Icon::Book },
                    &name,
                    Box2::new(32., 208. + i as f64 * 50., w - 64., 44.),
                    if folder {
                        Action::Browse(path)
                    } else {
                        Action::File(path)
                    },
                    false,
                );
            }
            text(
                a,
                "Scroll to browse · PDF, EPUB, TXT",
                32.,
                h - 34.,
                12.,
                GRAY,
                w - 64.,
            );
        }
    }
}
fn input(a: &mut App, i: &Input) {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    let kh = if i.keyboard {
        a.prefs.keyboard_key_height * 4. + 74.
    } else {
        0.
    };
    let cw = (if i.multiline { 560_f64 } else { 520_f64 }).min(w - 32.);
    let ch = (if i.multiline { 326_f64 } else { 218_f64 }).min(h - kh - 58.);
    let card = Box2::new((w - cw) / 2., 42. + (h - kh - 42. - ch) / 2., cw, ch);
    a.surface.clear_tone();
    a.surface.rounded(card, 6., GRAY);
    a.surface.rounded(
        Box2::new(card.x + 1., card.y + 1., card.w - 2., card.h - 2.),
        5.,
        WHITE,
    );
    a.popup_bounds = Some(card);
    // Editing a name or note is explicit: outside touches do not discard the draft.
    a.hits.push((Box2::new(0., 0., w, h), Action::None));
    a.surface.label(
        &i.title,
        Box2::new(card.x + 20., card.y + 12., cw - 40., 36.),
        20.,
        BLACK,
    );
    let bottom = card.y + ch - 58.;
    button(
        a,
        "Cancel",
        Box2::new(card.x + cw - 200., bottom, 84., 42.),
        Action::Close,
        false,
    );
    let action = if i.title.starts_with("Replace existing") {
        Action::Key("overwrite".into())
    } else {
        Action::Accept
    };
    button(
        a,
        "Save",
        Box2::new(card.x + cw - 104., bottom, 84., 42.),
        action,
        true,
    );
    let field = Box2::new(card.x + 20., card.y + 58., cw - 40., ch - 150.);
    a.surface.border(field, BLACK, 1.);
    a.hits.push((field, Action::ShowKeyboard));
    let selected = i.anchor != i.cursor;
    if selected {
        a.surface.fill(
            Box2::new(field.x + 4., field.y + 4., field.w - 8., field.h - 8.),
            BACK,
        );
    }
    let mut chars: Vec<_> = i.value.chars().collect();
    chars.insert(i.cursor.min(chars.len()), '│');
    let value: String = chars.into_iter().collect();
    let old = a.surface.clip;
    a.surface.clip = Box2::new(field.x + 12., field.y + 12., field.w - 24., field.h - 24.);
    text(
        a,
        if value.is_empty() {
            "Type here"
        } else {
            &value
        },
        field.x + 12.,
        field.y + 12.,
        18.,
        BLACK,
        field.w - 24.,
    );
    a.surface.clip = old;
    button(
        a,
        if i.keyboard {
            "Hide keyboard"
        } else {
            "Show keyboard"
        },
        Box2::new(card.x + 20., bottom, 152., 42.),
        Action::Keyboard,
        false,
    );
    if !a.notice.is_empty() {
        a.surface.label(
            &a.notice,
            Box2::new(card.x + 20., field.y + field.h + 4., cw - 40., 26.),
            12.,
            GRAY,
        );
    }
    if i.keyboard {
        keyboard(a, i, h - kh);
    }
}
fn keyboard(a: &mut App, i: &Input, y: f64) {
    let w = a.surface.logical_w;
    let keyh = a.prefs.keyboard_key_height;
    a.surface
        .fill(Box2::new(0., y, w, a.surface.logical_h - y), BACK);
    let mut x = 8.;
    for (label, bw, action) in [
        ("All", 48., Action::SelectAll),
        ("Copy", 62., Action::Copy),
        ("Paste", 68., Action::Paste),
        ("←", 46., Action::Left),
        ("→", 46., Action::Right),
        ("Hide", 62., Action::Keyboard),
    ] {
        let b = Box2::new(x, y + 6., bw, 32.);
        match action {
            Action::Left => icon_button(a, Icon::Back, b, action, false),
            Action::Right => icon_button(a, Icon::Next, b, action, false),
            _ => button(a, label, b, action, false),
        }
        x += bw + 6.;
    }
    let rows: Vec<&str> = if i.mode == 1 {
        vec!["1234567890", "@#€£$%&*()", "-_/+:;!?\"'"]
    } else if i.mode == 2 {
        vec!["äöüßéèêëáà", "ąęłńóśźżçñ", "âôûîïœæøåß"]
    } else {
        match a.prefs.keyboard_layout.as_str() {
            "QWERTZ" => vec!["qwertzuiop", "asdfghjkl", "yxcvbnm"],
            "AZERTY" => vec!["azertyuiop", "qsdfghjklm", "wxcvbn"],
            _ => vec!["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        }
    };
    for (r, row) in rows.into_iter().enumerate() {
        let chars: Vec<_> = row.chars().collect();
        let count = chars.len() + if r == 2 { 2 } else { 0 };
        let kw = (w - 16. - (count - 1) as f64 * 5.) / count as f64;
        let mut x = 8.;
        let yy = y + 44. + r as f64 * (keyh + 4.);
        if r == 2 {
            button(
                a,
                if i.shift == 2 { "CAPS" } else { "Shift" },
                Box2::new(x, yy, kw, keyh),
                Action::Shift,
                i.shift > 0,
            );
            x += kw + 5.;
        }
        for c in chars {
            let key = if i.shift > 0 {
                c.to_uppercase().to_string()
            } else {
                c.to_string()
            };
            button(
                a,
                &key,
                Box2::new(x, yy, kw, keyh),
                Action::Key(key.clone()),
                false,
            );
            x += kw + 5.;
        }
        if r == 2 {
            icon_button(
                a,
                Icon::Backspace,
                Box2::new(x, yy, kw, keyh),
                Action::Backspace,
                false,
            );
        }
    }
    let yy = y + 44. + 3. * (keyh + 4.);
    let unit = (w - 36.) / 10.;
    let mut x = 8.;
    for (label, units, action) in [
        (
            if i.mode == 0 { "123" } else { "ABC" },
            1.5,
            Action::KeyMode(if i.mode == 0 { 1 } else { 0 }),
        ),
        ("áöł", 1.2, Action::KeyMode(2)),
        (",", 0.8, Action::Key(",".into())),
        ("Space", 4., Action::Key(" ".into())),
        (".", 0.8, Action::Key(".".into())),
        ("Enter", 1.7, Action::Key("Enter".into())),
    ] {
        button(
            a,
            label,
            Box2::new(x, yy, unit * units - 4., keyh),
            action,
            false,
        );
        x += unit * units;
    }
}
fn sharing(a: &mut App) {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    text(a, "Share a document", 32., 70., 24., BLACK, w - 64.);
    let active = a.sharing.as_ref().map(|s| {
        (
            s.code.clone(),
            s.status.clone(),
            s.online,
            s.peers.len() + 1,
            s.replica.is_some(),
        )
    });
    if let Some((code, status, online, count, ready)) = active {
        text(
            a,
            if code.is_empty() {
                "Connecting…"
            } else {
                "Invite others with this code"
            },
            32.,
            132.,
            17.,
            GRAY,
            w - 64.,
        );
        a.surface.centered_label(
            if code.is_empty() { "·····" } else { &code },
            Box2::new(32., 172., w - 64., 90.),
            58.,
            BLACK,
        );
        let label = if online && ready {
            format!(
                "{count} participant{} online",
                if count == 1 { "" } else { "s" }
            )
        } else {
            status
        };
        text(a, &label, 32., 284., 17., BLACK, w - 64.);
        text(a,"Everyone can edit. Your copy saves on this device. The room ends when the last participant leaves.",32.,356.,16.,GRAY,w-64.);
        button(
            a,
            "Leave sharing",
            Box2::new(32., h - 148., w - 64., 44.),
            Action::LeaveSharing,
            false,
        );
    } else {
        let url = a.prefs.exchange_url.clone();
        text(
            a,
            if url.is_empty() {
                "Set your exchange server URL to begin."
            } else {
                &url
            },
            32.,
            132.,
            16.,
            GRAY,
            w - 64.,
        );
        button(
            a,
            "Edit server URL",
            Box2::new(32., 206., w - 64., 44.),
            Action::ExchangeUrl,
            false,
        );
        let share = !url.is_empty() && a.doc.is_some();
        button(
            a,
            "Share current document",
            Box2::new(32., 276., w - 64., 48.),
            if share {
                Action::StartSharing
            } else {
                Action::None
            },
            share,
        );
        button(
            a,
            "Join with a code",
            Box2::new(32., 344., w - 64., 48.),
            Action::JoinSharing,
            false,
        );
        text(a,"Open a document to share it, or enter someone’s five-character code to receive your own editable copy. Share the code only with people you want to invite.",32.,424.,16.,GRAY,w-64.);
    }
    button(
        a,
        "Done",
        Box2::new(w - 152., h - 80., 120., 44.),
        Action::Close,
        true,
    );
}
fn settings(a: &mut App, s: &Settings) {
    let w = a.surface.logical_w;
    let h = a.surface.logical_h;
    text(a, "Settings", 32., 68., 23., BLACK, w - 200.);
    button(
        a,
        "Done",
        Box2::new(w - 116., 58., 84., 42.),
        Action::Close,
        false,
    );
    let tabw = (w - 96.) / 5.;
    for (i, name) in ["Light", "Page", "Refresh", "Keyboard", "Sharing"]
        .iter()
        .enumerate()
    {
        button(
            a,
            name,
            Box2::new(32. + i as f64 * (tabw + 8.), 112., tabw, 40.),
            Action::SettingTab(i),
            s.tab == i,
        );
    }
    let content_hits_start = a.hits.len();
    let content = Box2::new(32., 170., w - 64., h - 288.);
    let old = a.surface.clip;
    a.surface.clip = content;
    let mut y = 178. - s.scroll;
    let note = match s.tab {
        0 => "Brightness and warmth change immediately across the tablet.",
        1 => "Page appearance and input. Contrast does not alter saved ink or exports.",
        2 => "Writing mode applies to this window. Driver, dither and refresh affect the tablet.",
        4 => "Connect to your exchange server to share documents live.",
        _ => "Built-in touch keyboard. It works inside fullscreen.",
    };
    text(a, note, 36., y, 14., GRAY, w - 80.);
    y += 64.;
    if !s.loaded {
        text(a, "Reading hardware…", 36., y, 18., GRAY, w - 80.);
    } else {
        match s.tab {
            0 => {
                slider_row(a, "Brightness", SliderSetting::Brightness, &mut y, s);
                slider_row(a, "Warmth", SliderSetting::Warmth, &mut y, s);
                let bw = (w - 88.) / 4.;
                for (i, (name, b, warm)) in [
                    ("Off", 0., 50.),
                    ("Soft warm", 20., 85.),
                    ("Neutral", 35., 50.),
                    ("Bright cool", 75., 0.),
                ]
                .iter()
                .enumerate()
                {
                    button(
                        a,
                        name,
                        Box2::new(36. + i as f64 * (bw + 4.), y, bw - 4., 42.),
                        if s.before.cool.is_some() {
                            Action::LightPreset(*b, *warm)
                        } else {
                            Action::None
                        },
                        s.light_preset_active(*b, *warm),
                    );
                }
                y += 64.;
                if s.before.cool.is_none() {
                    text(
                        a,
                        "Front-light controls are unavailable on this computer.",
                        36.,
                        y,
                        15.,
                        GRAY,
                        w - 80.,
                    );
                }
            }
            1 => {
                slider_row(a, "Page contrast", SliderSetting::Contrast, &mut y, s);
                slider_row(a, "Midtone darkness", SliderSetting::Gamma, &mut y, s);
                slider_row(a, "Interface text", SliderSetting::Font, &mut y, s);
                switch_row(a, "Fullscreen", SwitchSetting::Fullscreen, &mut y, s);
                switch_row(a, "Hide pen pointer", SwitchSetting::Cursor, &mut y, s);
                switch_row(a, "Finger drawing", SwitchSetting::Finger, &mut y, s);
            }
            2 => {
                let bw = (w - 88.) / 4.;
                for (i, name) in ["Writing", "Balanced", "Reading", "Images"]
                    .iter()
                    .enumerate()
                {
                    button(
                        a,
                        name,
                        Box2::new(36. + i as f64 * (bw + 4.), y, bw - 4., 42.),
                        Action::Profile(name.to_string()),
                        s.profile_active(name),
                    );
                }
                y += 60.;
                let parts: Vec<_> = s.prefs.display_hint.split('|').collect();
                choice_row(
                    a,
                    "Page grayscale",
                    match parts.first().copied() {
                        Some("Y1") => "2 tones",
                        Some("Y2") => "4 tones",
                        _ => "16 tones",
                    },
                    "depth",
                    &mut y,
                );
                choice_row(
                    a,
                    "Gray conversion",
                    if parts.get(1) == Some(&"D") {
                        "Dither"
                    } else {
                        "Threshold"
                    },
                    "conversion",
                    &mut y,
                );
                switch_row(
                    a,
                    "Cleanup after drawing",
                    SwitchSetting::Cleanup,
                    &mut y,
                    s,
                );
                choice_row(
                    a,
                    "Driver speed · tablet",
                    match s.hardware.driver {
                        Some(1) => "Fast",
                        Some(_) => "Normal",
                        None => "Unavailable",
                    },
                    "driver",
                    &mut y,
                );
                choice_row(
                    a,
                    "Dither pattern · tablet",
                    match s.hardware.dither {
                        Some(0) => "Bayer",
                        Some(1) => "Blue noise 16",
                        Some(_) => "Blue noise 32",
                        None => "Unavailable",
                    },
                    "dither",
                    &mut y,
                );
                slider_row(a, "Cleanup delay · frames", SliderSetting::Delay, &mut y, s);
                slider_row(
                    a,
                    "Full refresh every N pages",
                    SliderSetting::Periodic,
                    &mut y,
                    s,
                );
                button(
                    a,
                    "Full refresh now",
                    Box2::new(36., y, 220., 42.),
                    if s.before.driver.is_some() {
                        Action::Refresh
                    } else {
                        Action::None
                    },
                    false,
                );
            }
            4 => {
                text(a, "Exchange server", 36., y, 19., BLACK, w - 80.);
                y += 40.;
                text(
                    a,
                    if s.prefs.exchange_url.is_empty() {
                        "No server configured"
                    } else {
                        &s.prefs.exchange_url
                    },
                    36.,
                    y,
                    16.,
                    GRAY,
                    w - 80.,
                );
                y += 76.;
                button(
                    a,
                    "Edit server URL",
                    Box2::new(36., y, w - 72., 44.),
                    Action::ExchangeUrl,
                    false,
                );
                y += 70.;
                button(
                    a,
                    "Share or join a document",
                    Box2::new(36., y, w - 72., 44.),
                    Action::Sharing,
                    false,
                );
                y += 70.;
                text(a,"Everyone uses the same server and a five-character code. Documents save on each device. At least one participant must stay connected for others to join.",36.,y,16.,GRAY,w-80.);
            }
            _ => {
                switch_row(
                    a,
                    "Show keyboard on focus",
                    SwitchSetting::Keyboard,
                    &mut y,
                    s,
                );
                choice_row(
                    a,
                    "Letter layout",
                    &s.prefs.keyboard_layout,
                    "layout",
                    &mut y,
                );
                slider_row(a, "Key height", SliderSetting::KeyHeight, &mut y, s);
                text(a,"Shift once for capitals, twice for Caps Lock. Numbers, symbols and German, French and Polish accents are included. Select all, Copy, Paste and arrow keys work in every text field.",36.,y+8.,16.,GRAY,w-80.);
            }
        }
    }
    a.surface.clip = old;
    let content_hits = a.hits.split_off(content_hits_start);
    a.hits.extend(
        content_hits
            .into_iter()
            .filter(|(b, _)| b.y >= content.y && b.y + b.h <= content.y + content.h),
    );
    a.surface
        .fill(Box2::new(18., h - 112., w - 36., 94.), WHITE);
    a.surface.fill(Box2::new(32., h - 112., w - 64., 1.), LINE);
    a.surface.label(
        if let Some(error) = &s.error {
            error
        } else if s.light_busy || s.display_busy {
            "Updating settings…"
        } else if !a.notice.is_empty() {
            &a.notice
        } else {
            "Changes take effect immediately and save automatically."
        },
        Box2::new(32., h - 108., w - 64., 30.),
        12.,
        GRAY,
    );
    button(
        a,
        "Reset this panel",
        Box2::new(32., h - 70., 170., 42.),
        Action::ResetSettings,
        false,
    );
    button(
        a,
        "Done",
        Box2::new(w - 218., h - 70., 186., 42.),
        Action::Close,
        true,
    );
}
fn switch_row(a: &mut App, title: &str, kind: SwitchSetting, y: &mut f64, s: &Settings) {
    let w = a.surface.logical_w;
    let on = kind.value(s);
    a.surface
        .label(title, Box2::new(36., *y, w - 228., 44.), 16., BLACK);
    a.surface.centered_label(
        if on { "On" } else { "Off" },
        Box2::new(w - 178., *y, 56., 44.),
        14.,
        if on { BLACK } else { GRAY },
    );
    let b = Box2::new(w - 116., *y + 3., 76., 38.);
    a.surface.rounded(b, 19., if on { BLACK } else { LINE });
    if !on {
        a.surface
            .rounded(Box2::new(b.x + 1., b.y + 1., b.w - 2., b.h - 2.), 18., BACK);
    }
    let thumb = Box2::new(b.x + if on { 40. } else { 4. }, b.y + 4., 30., 30.);
    a.surface.rounded(thumb, 15., if on { WHITE } else { GRAY });
    if !on {
        a.surface
            .rounded(Box2::new(thumb.x + 1., thumb.y + 1., 28., 28.), 14., WHITE);
    }
    a.hits.push((
        Box2::new(b.x - 4., *y - 2., b.w + 8., 48.),
        Action::Switch(kind),
    ));
    *y += 64.;
}
fn slider_row(a: &mut App, title: &str, kind: SliderSetting, y: &mut f64, s: &Settings) {
    let w = a.surface.logical_w;
    let enabled = kind.enabled(s);
    let value = kind.value(s);
    let label = if !enabled {
        "Unavailable".into()
    } else {
        match kind {
            SliderSetting::Brightness | SliderSetting::Warmth => format!("{value:.0}%"),
            SliderSetting::Contrast | SliderSetting::Gamma => format!("{value:.2}"),
            SliderSetting::Font => format!("{value:.0} px"),
            SliderSetting::KeyHeight => if value > 40. { "Large" } else { "Standard" }.into(),
            SliderSetting::Periodic if value == 0. => "Off".into(),
            SliderSetting::Periodic => format!(
                "Every {value:.0} page{}",
                if value == 1. { "" } else { "s" }
            ),
            _ => format!("{value:.0}"),
        }
    };
    let x = w * 0.55;
    let bounds = Box2::new(x, *y, w - x - 40., 48.);
    let color = if enabled { BLACK } else { GRAY };
    a.surface
        .label(title, Box2::new(36., *y, x - 52., 44.), 16., BLACK);
    a.surface
        .centered_label(&label, Box2::new(x, *y - 3., bounds.w, 22.), 14., color);
    let rail = Box2::new(x + 16., *y + 28., bounds.w - 32., 6.);
    a.surface.rounded(rail, 3., LINE);
    let fill = rail.w * kind.fraction(s);
    if enabled {
        a.surface
            .rounded(Box2::new(rail.x, rail.y, fill, rail.h), 3., BLACK);
    }
    let thumb = Box2::new(rail.x + fill - 13., rail.y - 10., 26., 26.);
    a.surface.rounded(thumb, 13., color);
    a.surface
        .rounded(Box2::new(thumb.x + 2., thumb.y + 2., 22., 22.), 11., WHITE);
    if enabled {
        a.hits.push((bounds, Action::Slider(kind)));
    }
    *y += 64.;
}
fn choice_row(a: &mut App, title: &str, value: &str, key: &str, y: &mut f64) {
    let w = a.surface.logical_w;
    a.surface.label(
        title,
        Box2::new(36., *y, (w - 300.).max(90.), 44.),
        16.,
        BLACK,
    );
    button(
        a,
        value,
        Box2::new(w - 250., *y, 210., 42.),
        Action::Choice(key.into()),
        false,
    );
    *y += 64.;
}
