//! Quiet library chrome with a separately clipped, continuously scrollable body.
use super::*;
use crate::model::Document;

struct Layout {
    margin: f64,
    toolbar: f64,
    body: Box2,
    cols: usize,
    card_w: f64,
    card_h: f64,
    folder_cols: usize,
    folder_h: f64,
    height: f64,
}
impl Layout {
    fn new(a: &App) -> Self {
        let w = a.surface.logical_w;
        let margin = if w < 760. { 28. } else { 36. };
        let toolbar = if w < 620. { 118. } else { 72. };
        let top = toolbar + 88. + if a.query.is_empty() { 0. } else { 44. };
        let body = Box2::new(
            margin,
            top,
            w - margin * 2.,
            (a.surface.logical_h - top - 40.).max(0.),
        );
        let cols = ((body.w + 24.) / 210.).floor().max(1.) as usize;
        let card_w = (body.w - (cols - 1) as f64 * 24.) / cols as f64;
        let card_h = card_w * PAGE_H / PAGE_W + 74.;
        let folder_cols = ((body.w + 20.) / 210.).floor().max(1.) as usize;
        let folders = a.docs.iter().filter(|d| d.kind == "folder").count();
        let files = a.docs.len() - folders;
        let folder_h = if folders == 0 {
            0.
        } else {
            folders.div_ceil(folder_cols) as f64 * 48. + 22.
        };
        let height = folder_h
            + if a.library_list {
                files as f64 * 108.
            } else {
                files.div_ceil(cols) as f64 * card_h
            };
        Self {
            margin,
            toolbar,
            body,
            cols,
            card_w,
            card_h,
            folder_cols,
            folder_h,
            height,
        }
    }
    fn max_scroll(&self) -> f64 {
        (self.height - self.body.h).max(0.)
    }
}
pub(super) fn scroll_max(a: &App) -> f64 {
    Layout::new(a).max_scroll()
}

fn plain(a: &mut App, icon: Icon, label: &str, b: Box2, action: Action) {
    icon.draw(
        &mut a.surface,
        Box2::new(b.x, b.y + (b.h - 22.) / 2., 22., 22.),
        BLACK,
    );
    a.surface.label(
        label,
        Box2::new(b.x + 30., b.y, b.w - 30., b.h),
        a.prefs.ui_font_size,
        BLACK,
    );
    a.hits.push((b, action));
}
fn clipped_hit(a: &mut App, b: Box2, viewport: Box2, action: Action) {
    let x = b.x.max(viewport.x);
    let y = b.y.max(viewport.y);
    let right = (b.x + b.w).min(viewport.x + viewport.w);
    let bottom = (b.y + b.h).min(viewport.y + viewport.h);
    if right > x && bottom > y {
        a.hits
            .push((Box2::new(x, y, right - x, bottom - y), action));
    }
}
fn folder_icon(a: &mut App, x: f64, y: f64) {
    a.surface.fill(Box2::new(x, y, 11., 3.), BLACK);
    a.surface.fill(Box2::new(x, y + 5., 23., 17.), BLACK);
}
fn preview(a: &mut App, d: &Document, b: Box2) -> usize {
    a.surface.fill(b, WHITE);
    a.surface.border(b, LINE, 1.);
    let Ok(pages) = a.store.pages(&d.id) else {
        return 0;
    };
    if let Some(p) = pages.first() {
        let zoom = ((b.w - 14.) / PAGE_W).min((b.h - 14.) / PAGE_H);
        let clip = a.surface.clip;
        let left = (b.x + 1.).max(clip.x);
        let top = (b.y + 1.).max(clip.y);
        let view = View {
            x: b.x + (b.w - PAGE_W * zoom) / 2.,
            y: b.y + 7.,
            zoom,
            clip: Box2::new(
                left,
                top,
                (b.x + b.w - 1.).min(clip.x + clip.w) - left,
                (b.y + b.h - 1.).min(clip.y + clip.h) - top,
            ),
        };
        if d.kind == "pdf" {
            if let Ok(path) = a.store.source_path(d) {
                a.request_thumbnail(&d.id, 0, path);
            }
        }
        let thumbnail = a.thumbnails.get(&(d.id.clone(), 0));
        let missing = d.kind == "pdf" && thumbnail.is_none();
        a.surface.page(view, &p.payload, thumbnail, true);
        if missing {
            a.surface.label(
                "PDF",
                Box2::new(b.x + 16., b.y + 20., b.w - 32., 30.),
                18.,
                GRAY,
            );
        }
    }
    pages.len()
}
fn meta(kind: &str, pages: usize) -> String {
    let kind = match kind {
        "notebook" => "Notebook",
        "book" => "Ebook",
        _ => "PDF",
    };
    format!(
        "{pages} {} · {kind}",
        if pages == 1 { "page" } else { "pages" }
    )
}

pub(super) fn render(a: &mut App) {
    let l = Layout::new(a);
    let w = a.surface.logical_w;
    let font = a.prefs.ui_font_size;
    plain(
        a,
        Icon::Menu,
        "Menu",
        Box2::new(l.margin, 14., 96., 44.),
        Action::LibraryMenu,
    );
    let widths = [font * 3.7 + 38., font * 5.6 + 38., font * 7.2 + 38.];
    let total = widths.iter().sum::<f64>() + 24.;
    let mut x = if w < 620. {
        l.margin
    } else {
        ((w - total) / 2.).max(l.margin + 110.)
    };
    let y = if w < 620. { 66. } else { 14. };
    for ((label, action), width) in [
        ("Folder", Action::New("folder".into())),
        ("Notebook", Action::New("notebook".into())),
        ("Quick sheets", Action::QuickSheets),
    ]
    .into_iter()
    .zip(widths)
    {
        plain(
            a,
            Icon::PlusCircle,
            label,
            Box2::new(x, y, width, 44.),
            action,
        );
        x += width + 12.;
    }
    icon_button(
        a,
        Icon::Search,
        Box2::new(w - l.margin - 44., 14., 44., 44.),
        Action::Search,
        false,
    );
    a.surface
        .fill(Box2::new(l.margin, l.toolbar, l.body.w, 1.), LINE);
    let title = if a.trash {
        "Trash".into()
    } else if let Some(id) = &a.folder {
        a.store
            .document(id)
            .map(|d| d.title)
            .unwrap_or("Folder".into())
    } else {
        match a.filter.as_str() {
            "notebook" => "Notebooks",
            "pdf" => "PDFs",
            "book" => "Ebooks",
            _ => "My files",
        }
        .into()
    };
    let mut title_x = l.margin;
    if a.folder.is_some() && !a.trash {
        icon_button(
            a,
            Icon::Back,
            Box2::new(title_x - 8., l.toolbar + 20., 40., 44.),
            Action::Up,
            false,
        );
        title_x += 40.;
    }
    let sort_w = (font * 9.5 + 32.).max(170.);
    let sort_x = w - l.margin - 52. - sort_w;
    a.surface.label(
        &title,
        Box2::new(
            title_x,
            l.toolbar + 20.,
            (sort_x - title_x - 14.).max(40.),
            44.,
        ),
        font + 6.,
        BLACK,
    );
    let sorts = [
        "Recently opened",
        "Alphabetical",
        "Date created",
        "Last modified",
        "File type",
    ];
    a.surface.label(
        sorts[a.sort.min(4)],
        Box2::new(sort_x, l.toolbar + 20., sort_w - 28., 44.),
        font - 1.,
        BLACK,
    );
    Icon::ChevronDown.draw(
        &mut a.surface,
        Box2::new(sort_x + sort_w - 24., l.toolbar + 33., 18., 18.),
        BLACK,
    );
    a.hits.push((
        Box2::new(sort_x, l.toolbar + 20., sort_w, 44.),
        Action::Sort,
    ));
    icon_button(
        a,
        if a.library_list {
            Icon::Grid
        } else {
            Icon::List
        },
        Box2::new(w - l.margin - 44., l.toolbar + 20., 44., 44.),
        Action::LibraryView,
        false,
    );
    if !a.query.is_empty() {
        a.surface.label(
            &format!("Search: {}", a.query),
            Box2::new(l.margin, l.toolbar + 72., l.body.w - 48., 36.),
            font,
            GRAY,
        );
        icon_button(
            a,
            Icon::Close,
            Box2::new(w - l.margin - 44., l.toolbar + 68., 44., 44.),
            Action::ClearSearch,
            false,
        );
    }
    a.library_offset = a.library_offset.min(l.max_scroll().ceil() as usize);
    let top = l.body.y - a.library_offset as f64;
    a.surface.clip = l.body;
    let docs = a.docs.clone();
    let folder_w = (l.body.w - (l.folder_cols - 1) as f64 * 20.) / l.folder_cols as f64;
    for (i, d) in docs.iter().filter(|d| d.kind == "folder").enumerate() {
        let x = l.margin + (i % l.folder_cols) as f64 * (folder_w + 20.);
        let y = top + (i / l.folder_cols) as f64 * 48.;
        let b = Box2::new(x, y, folder_w, 44.);
        if y + b.h < l.body.y || y > l.body.y + l.body.h {
            continue;
        }
        folder_icon(a, x, y + 11.);
        a.surface.label(
            &d.title,
            Box2::new(x + 33., y, folder_w - 67., 44.),
            font,
            BLACK,
        );
        clipped_hit(a, b, l.body, Action::Open(d.id.clone()));
        Icon::More.draw(
            &mut a.surface,
            Box2::new(x + folder_w - 25., y + 13., 18., 18.),
            GRAY,
        );
        clipped_hit(
            a,
            Box2::new(x + folder_w - 34., y, 34., 44.),
            l.body,
            Action::ItemMenu(d.id.clone()),
        );
    }
    for (i, d) in docs.iter().filter(|d| d.kind != "folder").enumerate() {
        let (x, y, width, height) = if a.library_list {
            (l.margin, top + l.folder_h + i as f64 * 108., l.body.w, 108.)
        } else {
            (
                l.margin + (i % l.cols) as f64 * (l.card_w + 24.),
                top + l.folder_h + (i / l.cols) as f64 * l.card_h,
                l.card_w,
                l.card_h,
            )
        };
        if y + height < l.body.y || y > l.body.y + l.body.h {
            continue;
        }
        let page_box = if a.library_list {
            Box2::new(x, y + 6., 62., 85.)
        } else {
            Box2::new(x, y, width, width * PAGE_H / PAGE_W)
        };
        let count = preview(a, d, page_box);
        let (tx, ty, tw) = if a.library_list {
            (x + 84., y + 20., width - 130.)
        } else {
            (x, y + page_box.h + 4., width - 34.)
        };
        a.surface
            .label(&d.title, Box2::new(tx, ty, tw, 28.), font, BLACK);
        a.surface.label(
            &meta(&d.kind, count),
            Box2::new(tx, ty + 27., tw, 22.),
            11.,
            GRAY,
        );
        clipped_hit(
            a,
            Box2::new(x, y, width, height - 12.),
            l.body,
            Action::Open(d.id.clone()),
        );
        let more = Box2::new(x + width - 34., ty, 34., 44.);
        Icon::More.draw(
            &mut a.surface,
            Box2::new(more.x + 8., more.y + 13., 18., 18.),
            GRAY,
        );
        clipped_hit(a, more, l.body, Action::ItemMenu(d.id.clone()));
        if a.library_list {
            a.surface.fill(Box2::new(x, y + 103., width, 1.), LINE);
        }
    }
    if a.docs.is_empty() {
        let (heading, detail) = if !a.query.is_empty() {
            (
                "No matching files",
                "Try a different name or clear your search.",
            )
        } else if a.trash {
            (
                "The trash is empty",
                "Deleted files can be restored from here.",
            )
        } else {
            (
                "Room for your next idea",
                "Tap Notebook to write, or import a document from Menu.",
            )
        };
        a.surface.label(
            heading,
            Box2::new(l.margin, l.body.y + 56., l.body.w, 42.),
            24.,
            BLACK,
        );
        a.surface.label(
            detail,
            Box2::new(l.margin, l.body.y + 104., l.body.w, 40.),
            font,
            GRAY,
        );
    }
    a.surface.reset_clip();
    if l.max_scroll() > 0. {
        let size = (l.body.h * l.body.h / l.height).max(24.);
        let y = l.body.y + (l.body.h - size) * a.library_offset as f64 / l.max_scroll();
        a.surface.fill(Box2::new(w - 9., y, 3., size), GRAY);
    }
    footer(a);
}

pub(super) fn footer(a: &mut App) {
    let w = a.surface.logical_w;
    let y = a.surface.logical_h - 40.;
    a.surface.fill(Box2::new(0., y, w, 40.), WHITE);
    a.surface.label(
        &format!("{} items", a.docs.len()),
        Box2::new(28., y, (w - 300.).max(50.), 40.),
        11.,
        GRAY,
    );
    let x = w - 248.;
    for n in 0..4 {
        let height = 4. + n as f64 * 4.;
        a.surface.fill(
            Box2::new(x + n as f64 * 5., y + 27. - height, 3., height),
            if a.status.wifi == "Connected" && a.status.signal.unwrap_or(0) as usize > n * 25 {
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
        Box2::new(x + 28., y, 98., 40.),
        11.,
        GRAY,
    );
    let bx = w - 112.;
    a.surface
        .border(Box2::new(bx, y + 13., 27., 14.), BLACK, 1.);
    a.surface.fill(Box2::new(bx + 27., y + 17., 3., 6.), BLACK);
    if let Some(p) = a.status.battery {
        a.surface.fill(
            Box2::new(bx + 2., y + 15., 23. * p as f64 / 100., 10.),
            BLACK,
        );
    }
    let label = a
        .status
        .battery
        .map(|p| format!("{}{}%", if a.status.charging { "ϟ" } else { "" }, p))
        .unwrap_or("--".into());
    a.surface
        .label(&label, Box2::new(bx + 36., y, 70., 40.), 11., GRAY);
    a.hits
        .push((Box2::new(x - 8., y, 248., 40.), Action::Status));
}

pub(super) fn sidebar(a: &mut App, items: &[(String, Action)]) {
    let h = a.surface.logical_h;
    let width = 294_f64.min(a.surface.logical_w - 32.);
    let b = Box2::new(0., 0., width, h);
    a.surface.fill(b, WHITE);
    a.surface.fill(Box2::new(width - 1., 0., 1., h), GRAY);
    a.hits
        .push((Box2::new(0., 0., a.surface.logical_w, h), Action::Close));
    a.hits.push((b, Action::None));
    a.popup_bounds = Some(b);
    plain(
        a,
        Icon::Menu,
        "Menu",
        Box2::new(28., 14., 150., 44.),
        Action::Close,
    );
    icon_button(
        a,
        Icon::Close,
        Box2::new(width - 58., 14., 44., 44.),
        Action::Close,
        false,
    );
    a.surface.fill(Box2::new(28., 72., width - 56., 1.), LINE);
    let capacity = ((h - 138.) / 44.).floor().max(1.) as usize;
    let count = items.len().min(capacity);
    a.menu_offset = a.menu_offset.min(items.len().saturating_sub(count));
    for (i, (label, action)) in items.iter().skip(a.menu_offset).take(count).enumerate() {
        let row = Box2::new(14., 90. + i as f64 * 44., width - 28., 44.);
        let selected = match action {
            Action::LibraryHome => a.folder.is_none() && a.filter.is_empty() && !a.trash,
            Action::Filter(kind) => a.filter == *kind && !a.trash,
            Action::TrashView => a.trash,
            _ => false,
        };
        if selected {
            a.surface.rounded(row, 6., BLACK);
        }
        let color = if selected { WHITE } else { BLACK };
        if let Some(icon) = menu_icon(action) {
            icon.draw(
                &mut a.surface,
                Box2::new(row.x + 14., row.y + 11., 22., 22.),
                color,
            );
        }
        a.surface.label(
            label,
            Box2::new(row.x + 54., row.y, row.w - 64., row.h),
            a.prefs.ui_font_size,
            color,
        );
        a.hits.push((row, action.clone()));
    }
    if count < items.len() {
        icon_button(
            a,
            Icon::Back,
            Box2::new(18., h - 44., 44., 40.),
            Action::MenuScroll(-(capacity as isize)),
            false,
        );
        icon_button(
            a,
            Icon::Next,
            Box2::new(width - 62., h - 44., 44., 40.),
            Action::MenuScroll(capacity as isize),
            false,
        );
    }
}
