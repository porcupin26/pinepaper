mod sharing;
use crate::{
    document,
    model::*,
    platform::{self, Hardware, Prefs, Status},
    render::{Box2, Rect, Surface, View},
    ui,
};
use anyhow::{Context, Result};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Library,
    Editor,
    Overview,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Library,
    LibraryHome,
    LibraryView,
    ClearSearch,
    QuickSheets,
    Open(String),
    Up,
    Filter(String),
    Sort,
    SortBy(usize),
    LibraryMenu,
    MenuScroll(isize),
    Search,
    TrashView,
    Create,
    New(String),
    Import,
    Backup,
    Settings,
    Sharing,
    StartSharing,
    JoinSharing,
    LeaveSharing,
    ExchangeUrl,
    Fullscreen,
    Quit,
    Tools,
    Tool(String),
    PenPanel,
    Pen(String),
    Width(f64),
    Color(f64),
    Undo,
    Redo,
    DeleteSelection,
    Overview,
    OverviewTab(String),
    TagFilter(String),
    Page(usize),
    Previous,
    Next,
    AddPage,
    Bookmark,
    Menu,
    TemplatePanel,
    Template(String),
    Tags,
    Export,
    DeletePage,
    ClearPage,
    Rename(String),
    Move(String),
    MoveTo(String, Option<String>),
    Trash(String, bool),
    ItemMenu(String),
    Fit,
    Status,
    Help,
    Close,
    Confirm,
    Accept,
    Keyboard,
    ShowKeyboard,
    Key(String),
    Shift,
    KeyMode(usize),
    SelectAll,
    Copy,
    Paste,
    Left,
    Right,
    Backspace,
    SettingTab(usize),
    Setting(String, f64),
    Slider(SliderSetting),
    SetSlider(SliderSetting, f64),
    Switch(SwitchSetting),
    Choice(String),
    Profile(String),
    LightPreset(f64, f64),
    ResetSettings,
    Refresh,
    Browse(PathBuf),
    File(PathBuf),
    ChooseFile,
    FileUp,
}
#[derive(Clone, Debug)]
pub enum InputPurpose {
    ExchangeUrl,
    JoinCode,
    Create(String),
    Rename(String),
    Search,
    Tags,
    Text(Option<usize>, Point),
    Import,
    Export,
    Backup,
}
#[derive(Clone, Debug)]
pub struct Input {
    pub title: String,
    pub value: String,
    pub cursor: usize,
    pub anchor: usize,
    pub purpose: InputPurpose,
    pub keyboard: bool,
    pub mode: usize,
    pub shift: u8,
    pub multiline: bool,
}
impl Input {
    pub fn insert(&mut self, text: &str, back: bool) {
        if self.title.starts_with("Replace existing") {
            self.title = "Save file".into();
        }
        let mut chars: Vec<_> = self.value.chars().collect();
        let mut a = self.cursor.min(self.anchor).min(chars.len());
        let b = self.cursor.max(self.anchor).min(chars.len());
        if back && a == b {
            a = a.saturating_sub(1);
        }
        chars.splice(a..b, text.chars());
        self.value = chars.into_iter().collect();
        self.cursor = a + text.chars().count();
        self.anchor = self.cursor;
    }
}
#[derive(Clone, Debug)]
pub struct Settings {
    pub tab: usize,
    pub prefs: Prefs,
    pub hardware: Hardware,
    pub before: Hardware,
    pub brightness: f64,
    pub warmth: f64,
    pub loaded: bool,
    pub display_busy: bool,
    pub light_busy: bool,
    pub error: Option<String>,
    pub scroll: f64,
}
impl Settings {
    pub fn light_preset_active(&self, brightness: f64, warmth: f64) -> bool {
        self.loaded
            && self.before.cool.is_some()
            && self.before.warm.is_some()
            // Readback is quantized to the front light's integer channel levels.
            && (self.brightness - brightness).abs() < 0.5
            && (self.warmth - warmth).abs() < 0.5
    }
    pub fn profile_active(&self, name: &str) -> bool {
        let (hint, driver, delay) = display_profile(name);
        self.loaded
            && self.prefs.display_hint == hint
            && self.hardware.driver == Some(driver)
            && self.hardware.dither == Some(2)
            && self.hardware.delay == Some(delay)
            && self.hardware.hint.as_deref() == Some(hint)
    }
}
fn display_profile(name: &str) -> (&'static str, u32, u32) {
    match name {
        "Writing" => ("Y1|T|R", 1, 350),
        "Images" => ("Y4|D|R", 0, 250),
        "Reading" => ("Y4|T|R", 0, 150),
        _ => ("Y4|T|R", 0, 200),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderSetting {
    Brightness,
    Warmth,
    Contrast,
    Gamma,
    Font,
    Delay,
    Periodic,
    KeyHeight,
}
impl SliderSetting {
    pub fn range(self) -> (f64, f64, f64) {
        match self {
            Self::Brightness | Self::Warmth => (0., 100., 1.),
            Self::Contrast => (1., 2.5, 0.01),
            Self::Gamma => (0.6, 1.8, 0.01),
            Self::Font => (14., 18., 2.),
            Self::Delay => (25., 400., 25.),
            Self::Periodic => (0., 20., 1.),
            Self::KeyHeight => (38., 44., 6.),
        }
    }
    pub fn value(self, s: &Settings) -> f64 {
        match self {
            Self::Brightness => s.brightness,
            Self::Warmth => s.warmth,
            Self::Contrast => s.prefs.page_contrast,
            Self::Gamma => s.prefs.page_gamma,
            Self::Font => s.prefs.ui_font_size,
            Self::Delay => s.hardware.delay.unwrap_or(25) as f64,
            Self::Periodic => s.prefs.refresh_pages as f64,
            Self::KeyHeight => s.prefs.keyboard_key_height,
        }
    }
    pub fn enabled(self, s: &Settings) -> bool {
        s.loaded
            && match self {
                Self::Brightness | Self::Warmth => {
                    s.before.cool.is_some() && s.before.warm.is_some()
                }
                Self::Delay => s.before.delay.is_some(),
                _ => true,
            }
    }
    pub fn snap(self, value: f64) -> f64 {
        let (min, max, step) = self.range();
        let value = min + ((value.clamp(min, max) - min) / step).round() * step;
        // Decimal settings should persist their displayed value, not float noise.
        ((value * 100.).round() / 100.).clamp(min, max)
    }
    pub fn fraction(self, s: &Settings) -> f64 {
        let (min, max, _) = self.range();
        ((self.value(s) - min) / (max - min)).clamp(0., 1.)
    }
    pub fn at(self, bounds: Box2, x: f64) -> f64 {
        let (min, max, _) = self.range();
        let fraction = ((x - bounds.x - 16.) / (bounds.w - 32.)).clamp(0., 1.);
        self.snap(min + fraction * (max - min))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchSetting {
    Fullscreen,
    Cursor,
    Finger,
    Keyboard,
    Cleanup,
}
impl SwitchSetting {
    pub fn value(self, s: &Settings) -> bool {
        match self {
            Self::Fullscreen => s.prefs.start_fullscreen,
            Self::Cursor => s.prefs.hide_cursor,
            Self::Finger => s.prefs.finger_draw,
            Self::Keyboard => s.prefs.keyboard_auto,
            Self::Cleanup => s.prefs.display_hint.split('|').nth(2) != Some("r"),
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Fullscreen => "fullscreen",
            Self::Cursor => "cursor",
            Self::Finger => "finger",
            Self::Keyboard => "keyboard",
            Self::Cleanup => "cleanup",
        }
    }
}
#[derive(Clone)]
struct ControlDrag {
    action: Action,
    bounds: Box2,
}
#[derive(Clone, Debug)]
pub enum Dialog {
    Sharing,
    Menu(String, Vec<(String, Action)>),
    PenPanel,
    Input(Input),
    Settings(Box<Settings>),
    Message(String, String),
    Confirm(String, Action),
    Files {
        dir: PathBuf,
        entries: Vec<(String, PathBuf, bool)>,
        offset: usize,
    },
}
#[derive(Clone, Default)]
pub struct Selection {
    pub strokes: Vec<usize>,
    pub texts: Vec<usize>,
    pub bounds: Option<Box2>,
}
enum Gesture {
    Ink(Stroke),
    Erase(Point),
    Lasso(Vec<Point>),
    Move(Point, Payload),
    Text,
}
enum Work {
    Import(PathBuf, Option<String>),
    Export(PathBuf, Vec<Page>, Option<PathBuf>),
    Backup(PathBuf),
    Pdf(String, usize, PathBuf),
    Hardware,
    Lights(f64, f64),
    Display(Hardware),
    Refresh,
}
enum Done {
    Imported(String),
    Exported(PathBuf),
    BackedUp(PathBuf),
    Pdf(String, usize, image::RgbImage),
    Hardware(Hardware),
    Lights(Hardware, Option<String>),
    Display(Hardware, Option<String>),
    Refreshed,
    Error(String),
    Thumb(String, usize, Option<image::RgbImage>),
}

pub struct App {
    pub surface: Surface,
    pub sharing: Option<crate::exchange::Session>,
    pub store: Store,
    pub saver: Saver,
    pub prefs: Prefs,
    pub screen: Screen,
    pub dialog: Option<Dialog>,
    pub popup_anchor: Option<Box2>,
    pub popup_bounds: Option<Box2>,
    pub menu_offset: usize,
    pub hits: Vec<(Box2, Action)>,
    pub docs: Vec<Document>,
    pub folder: Option<String>,
    pub filter: String,
    pub query: String,
    pub sort: usize,
    pub trash: bool,
    pub library_offset: usize,
    pub library_list: bool,
    pub doc: Option<Document>,
    pub pages: Vec<Page>,
    pub index: usize,
    pub hidden: bool,
    pub tool: String,
    pub nib: Stroke,
    pub scroll: f64,
    pub fit: bool,
    pub view: View,
    pub selection: Selection,
    pub overview_tab: String,
    pub tag: Option<String>,
    pub overview_offset: usize,
    pub status: Status,
    pub notice: String,
    pub saving_error: bool,
    pub running: bool,
    pub fullscreen: bool,
    pub fullscreen_changed: bool,
    pub redraw: bool,
    pub damage: Option<Rect>,
    pub busy: bool,
    pub pdf: Option<image::RgbImage>,
    pub thumbnails: HashMap<(String, usize), image::RgbImage>,
    thumb_jobs: mpsc::Sender<(String, usize, PathBuf)>,
    thumb_requested: std::collections::HashSet<(String, usize)>,
    activation: std::os::unix::net::UnixListener,
    histories: HashMap<String, History>,
    gesture: Option<Gesture>,
    press: Option<Action>,
    key_feedback: Vec<(Box2, Instant)>,
    control_drag: Option<ControlDrag>,
    press_start: (f64, f64),
    last_pointer: (f64, f64),
    finger: bool,
    palm_until: Instant,
    jobs: mpsc::Sender<Work>,
    done: mpsc::Receiver<Done>,
    status_rx: mpsc::Receiver<Status>,
    page_opens: usize,
    pub last_save: Instant,
    clipboard: String,
    light_pending: Option<(f64, f64)>,
    light_in_flight: bool,
    display_pending: Option<Hardware>,
    display_in_flight: bool,
    close_after_settings: bool,
    quit_after_settings: bool,
}
impl App {
    pub fn new(root: PathBuf) -> Result<Self> {
        let store = Store::open(&root)?;
        if store.has_documents() && !store.setting("rust_backup_created", false) {
            let path = root
                .join("exports")
                .join(format!("before-rust-{}.zip", now() as u64));
            store.backup(&path)?;
            store.set("rust_backup_created", &true)?;
        }
        let socket = root.join("instance.sock");
        if socket.exists() {
            std::fs::remove_file(&socket)?;
        }
        let activation = std::os::unix::net::UnixListener::bind(&socket)?;
        activation.set_nonblocking(true)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let prefs = Prefs::load(&store);
        let saver = Saver::start(root.clone())?;
        let (jobs, rx) = mpsc::channel();
        let (out, done) = mpsc::channel();
        let (thumb_jobs, thumb_rx) = mpsc::channel::<(String, usize, PathBuf)>();
        let thumb_out = out.clone();
        thread::spawn(move || {
            while let Ok((id, index, path)) = thumb_rx.recv() {
                let img = document::pdf_page(&path, index)
                    .ok()
                    .map(|img| image::imageops::thumbnail(&img, 304, 416));
                if thumb_out.send(Done::Thumb(id, index, img)).is_err() {
                    break;
                }
            }
        });
        thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let result = (|| -> Result<Done> {
                    Ok(match job {
                        Work::Import(p, parent) => {
                            let mut store = Store::open(&root)?;
                            Done::Imported(document::import(&mut store, &p, parent.as_deref())?)
                        }
                        Work::Export(path, pages, source) => {
                            document::export_pdf(&path, &pages, source.as_deref())?;
                            Done::Exported(path)
                        }
                        Work::Backup(path) => {
                            Store::open(&root)?.backup(&path)?;
                            Done::BackedUp(path)
                        }
                        Work::Pdf(id, i, path) => Done::Pdf(id, i, document::pdf_page(&path, i)?),
                        Work::Hardware => Done::Hardware(Hardware::read()),
                        Work::Lights(b, w) => match Hardware::apply_lights(b, w) {
                            Ok(h) => Done::Lights(h, None),
                            Err(e) => Done::Lights(Hardware::read_lights(), Some(format!("{e:#}"))),
                        },
                        Work::Display(h) => match h.apply_display() {
                            Ok(actual) => Done::Display(actual, None),
                            Err(e) => Done::Display(
                                Hardware::read().display_only(),
                                Some(format!("{e:#}")),
                            ),
                        },
                        Work::Refresh => {
                            platform::refresh()?;
                            Done::Refreshed
                        }
                    })
                })();
                if out
                    .send(result.unwrap_or_else(|e| Done::Error(format!("{e:#}"))))
                    .is_err()
                {
                    break;
                }
            }
        });
        let fullscreen = prefs.start_fullscreen;
        let library_list = store.setting("library_list", false);
        let library_sort = store.setting::<usize>("library_sort", 0).min(4);
        let mut app = Self {
            sharing: None,
            surface: Surface::new(),
            store,
            saver,
            prefs,
            screen: Screen::Library,
            dialog: None,
            popup_anchor: None,
            popup_bounds: None,
            menu_offset: 0,
            hits: vec![],
            docs: vec![],
            folder: None,
            filter: String::new(),
            query: String::new(),
            sort: library_sort,
            trash: false,
            library_offset: 0,
            library_list,
            doc: None,
            pages: vec![],
            index: 0,
            hidden: false,
            tool: "pen".into(),
            nib: Stroke::default(),
            scroll: 0.,
            fit: false,
            view: View {
                x: 0.,
                y: 0.,
                zoom: 1.,
                clip: Box2::default(),
            },
            selection: Selection::default(),
            overview_tab: "Pages".into(),
            tag: None,
            overview_offset: 0,
            status: Status::default(),
            notice: String::new(),
            saving_error: false,
            running: true,
            fullscreen,
            fullscreen_changed: true,
            redraw: true,
            damage: None,
            busy: false,
            pdf: None,
            thumbnails: HashMap::new(),
            thumb_jobs,
            thumb_requested: Default::default(),
            activation,
            histories: HashMap::new(),
            gesture: None,
            press: None,
            key_feedback: Vec::new(),
            control_drag: None,
            press_start: (0., 0.),
            last_pointer: (0., 0.),
            finger: false,
            palm_until: Instant::now(),
            jobs,
            done,
            status_rx: platform::status_worker(),
            page_opens: 0,
            last_save: Instant::now(),
            clipboard: String::new(),
            light_pending: None,
            light_in_flight: false,
            display_pending: None,
            display_in_flight: false,
            close_after_settings: false,
            quit_after_settings: false,
        };
        app.nib = app.store.setting("pen", Stroke::default());
        app.reload_library()?;
        Ok(app)
    }
    pub fn page(&self) -> Option<&Page> {
        self.pages.get(self.index)
    }
    pub fn hit(&self, x: f64, y: f64) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|(b, _)| b.contains(x, y))
            .map(|(_, a)| a.clone())
    }
    pub fn resize(&mut self, w: usize, h: usize, scale: i32) {
        self.finish();
        self.key_feedback.clear();
        self.surface.resize(w, h, scale);
        self.redraw = true;
    }
    pub fn render(&mut self) {
        if self.redraw {
            ui::render(self);
            self.damage = Some(self.surface.full_rect());
            self.redraw = false;
        }
    }
    pub fn dirty(&mut self, r: Rect) {
        if r.x < r.right && r.y < r.bottom {
            self.damage = Some(self.damage.map_or(r, |d| d.union(r)));
        }
    }
    pub fn reload_library(&mut self) -> Result<()> {
        self.docs = self.store.documents(
            self.folder.as_deref(),
            &self.filter,
            &self.query,
            self.sort,
            self.trash,
        )?;
        Ok(())
    }
    fn input(&mut self, title: &str, value: String, purpose: InputPurpose, multiline: bool) {
        self.key_feedback.clear();
        let n = value.chars().count();
        self.dialog = Some(Dialog::Input(Input {
            title: title.into(),
            value,
            cursor: n,
            anchor: n,
            purpose,
            keyboard: self.prefs.keyboard_auto,
            mode: 0,
            shift: 0,
            multiline,
        }));
    }
    pub fn fail(&mut self, e: impl std::fmt::Display) {
        self.dialog = Some(Dialog::Message(
            "Could not complete action".into(),
            e.to_string(),
        ));
        self.redraw = true;
    }
    fn menu(&mut self, title: &str, items: Vec<(&str, Action)>) {
        self.dialog = Some(Dialog::Menu(
            title.into(),
            items.into_iter().map(|(s, a)| (s.into(), a)).collect(),
        ));
    }
    pub fn act(&mut self, action: Action) {
        let keyboard_mode = self.keyboard_mode();
        if self.dialog.is_none() {
            self.popup_anchor = self
                .hits
                .iter()
                .rev()
                .find(|(_, a)| *a == action)
                .map(|(b, _)| *b);
        }
        let previous_menu = match &self.dialog {
            Some(Dialog::Menu(title, _)) => Some(title.clone()),
            _ => None,
        };
        if let Err(e) = self.action(action) {
            self.fail(e);
        }
        if self.keyboard_mode() != keyboard_mode {
            self.key_feedback.clear();
        }
        if let Some(Dialog::Menu(title, _)) = &self.dialog {
            if previous_menu.as_ref() != Some(title) {
                self.menu_offset = 0;
            }
        }
        self.redraw = true;
    }
    fn action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::None => {}
            Action::Library => {
                self.flush()?;
                self.screen = Screen::Library;
                self.dialog = None;
                self.reload_library()?;
            }
            Action::LibraryHome => {
                self.flush()?;
                self.folder = None;
                self.filter.clear();
                self.query.clear();
                self.trash = false;
                self.library_offset = 0;
                self.screen = Screen::Library;
                self.dialog = None;
                self.reload_library()?;
            }
            Action::LibraryView => {
                self.library_list = !self.library_list;
                self.store.set("library_list", &self.library_list)?;
                self.library_offset = 0;
                self.dialog = None;
            }
            Action::ClearSearch => {
                self.query.clear();
                self.library_offset = 0;
                self.reload_library()?;
            }
            Action::Open(id) => self.open(&id)?,
            Action::Up => {
                self.folder = self
                    .folder
                    .as_ref()
                    .map(|id| self.store.document(id))
                    .transpose()?
                    .and_then(|d| d.parent);
                self.library_offset = 0;
                self.reload_library()?;
            }
            Action::Filter(kind) => {
                self.filter = kind;
                self.folder = None;
                self.trash = false;
                self.screen = Screen::Library;
                self.dialog = None;
                self.library_offset = 0;
                self.reload_library()?;
            }
            Action::Sort => {
                self.menu(
                    "Sort files",
                    vec![
                        ("Recently opened", Action::SortBy(0)),
                        ("Alphabetical", Action::SortBy(1)),
                        ("Date created", Action::SortBy(2)),
                        ("Last modified", Action::SortBy(3)),
                        ("File type", Action::SortBy(4)),
                    ],
                );
            }
            Action::SortBy(sort) => {
                self.sort = sort.min(4);
                self.store.set("library_sort", &self.sort)?;
                self.library_offset = 0;
                self.dialog = None;
                self.reload_library()?;
            }
            Action::LibraryMenu => self.menu(
                "Library",
                vec![
                    ("My files", Action::LibraryHome),
                    ("Notebooks", Action::Filter("notebook".into())),
                    ("PDFs", Action::Filter("pdf".into())),
                    ("Ebooks", Action::Filter("book".into())),
                    ("Import PDF / EPUB / text", Action::Import),
                    ("Join shared document", Action::JoinSharing),
                    ("Settings", Action::Settings),
                    (
                        if self.fullscreen {
                            "Leave fullscreen"
                        } else {
                            "Fullscreen"
                        },
                        Action::Fullscreen,
                    ),
                    ("Back up library", Action::Backup),
                    (
                        if self.trash {
                            "Return from trash"
                        } else {
                            "Trash"
                        },
                        Action::TrashView,
                    ),
                    ("About & help", Action::Help),
                    ("Quit PinePaper", Action::Quit),
                ],
            ),
            Action::MenuScroll(delta) => {
                if let Some(Dialog::Menu(_, items)) = &self.dialog {
                    self.menu_offset = self
                        .menu_offset
                        .saturating_add_signed(delta)
                        .min(items.len().saturating_sub(1));
                }
            }
            Action::Search => self.input(
                "Search library",
                self.query.clone(),
                InputPurpose::Search,
                false,
            ),
            Action::TrashView => {
                self.trash = !self.trash;
                self.dialog = None;
                self.library_offset = 0;
                self.reload_library()?;
            }
            Action::Create => self.menu(
                "Create",
                vec![
                    ("Notebook", Action::New("notebook".into())),
                    ("Folder", Action::New("folder".into())),
                    ("Join shared document", Action::JoinSharing),
                    ("Import PDF / EPUB / text", Action::Import),
                ],
            ),
            Action::QuickSheets => {
                self.flush()?;
                let existing = self
                    .store
                    .setting::<Option<String>>("quick_sheets", None)
                    .and_then(|id| self.store.document(&id).ok())
                    .filter(|d| d.kind == "notebook" && !d.trashed);
                if let Some(doc) = existing {
                    self.open(&doc.id)?;
                    self.index = self.pages.len() - 1;
                    self.action(Action::AddPage)?;
                } else {
                    let id = self.store.create(
                        "Quick sheets",
                        "notebook",
                        None,
                        vec![Payload::blank("Blank")],
                        None,
                    )?;
                    self.store.set("quick_sheets", &Some(&id))?;
                    self.open(&id)?;
                }
                self.tool = "pen".into();
            }
            Action::New(kind) if kind == "notebook" => {
                self.flush()?;
                let docs = self
                    .store
                    .documents(self.folder.as_deref(), "", "", 0, false)?;
                let mut number = 1;
                let title = loop {
                    let title = format!("Notebook {number}");
                    if !docs.iter().any(|d| d.title.eq_ignore_ascii_case(&title)) {
                        break title;
                    }
                    number += 1;
                };
                let id = self.store.create(
                    &title,
                    "notebook",
                    self.folder.as_deref(),
                    vec![Payload::blank("Lined")],
                    None,
                )?;
                self.open(&id)?;
                self.tool = "pen".into();
                self.notice = "Tap the title to rename".into();
            }
            Action::New(kind) => self.input(
                if kind == "folder" {
                    "New folder"
                } else {
                    "New notebook"
                },
                String::new(),
                InputPurpose::Create(kind),
                false,
            ),
            Action::Import => self.browse(
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "/".into()),
            )?,
            Action::Backup => self.input(
                "Back up library to ZIP",
                self.store
                    .root
                    .join("exports")
                    .join(format!("pinepaper-{}.zip", now() as u64))
                    .display()
                    .to_string(),
                InputPurpose::Backup,
                false,
            ),
            Action::Sharing => {
                self.dialog = Some(Dialog::Sharing);
            }
            Action::StartSharing => {
                self.start_sharing(None)?;
            }
            Action::JoinSharing => {
                anyhow::ensure!(
                    self.sharing.is_none(),
                    "Leave the current sharing session first"
                );
                if self.prefs.exchange_url.is_empty() {
                    self.input(
                        "Exchange server URL",
                        String::new(),
                        InputPurpose::ExchangeUrl,
                        false,
                    );
                } else {
                    self.input(
                        "Join · five-character code",
                        String::new(),
                        InputPurpose::JoinCode,
                        false,
                    );
                }
            }
            Action::LeaveSharing => {
                self.finish();
                self.flush()?;
                self.sharing = None;
                self.histories.clear();
                self.notice = "Sharing ended · local copy kept".into();
                self.dialog = None;
            }
            Action::ExchangeUrl => self.input(
                "Exchange server URL",
                self.prefs.exchange_url.clone(),
                InputPurpose::ExchangeUrl,
                false,
            ),
            Action::Settings => {
                let h = Hardware::default();
                let mut prefs = self.prefs.clone();
                prefs.start_fullscreen = self.fullscreen;
                self.dialog = Some(Dialog::Settings(Box::new(Settings {
                    tab: 0,
                    prefs,
                    hardware: h.clone(),
                    before: h,
                    brightness: 0.,
                    warmth: 50.,
                    loaded: false,
                    display_busy: false,
                    light_busy: false,
                    error: None,
                    scroll: 0.,
                })));
                self.jobs.send(Work::Hardware)?;
            }
            Action::Fullscreen => {
                self.fullscreen = !self.fullscreen;
                self.fullscreen_changed = true;
                if let Some(Dialog::Settings(s)) = &mut self.dialog {
                    s.prefs.start_fullscreen = self.fullscreen;
                    self.live_settings()?;
                }
            }
            Action::Quit => {
                if self.light_in_flight || self.display_in_flight {
                    self.quit_after_settings = true;
                    return Ok(());
                }
                self.finish();
                self.flush()?;
                self.running = false;
            }
            Action::Tools => self.hidden = !self.hidden,
            Action::Tool(t) => {
                if self.tool == t && t == "pen" {
                    self.action(Action::PenPanel)?;
                } else {
                    self.tool = t;
                    self.selection = Selection::default();
                }
            }
            Action::PenPanel => {
                self.dialog = Some(Dialog::PenPanel);
            }
            Action::Pen(p) => {
                self.nib.pen = p;
                self.store.set("pen", &self.nib)?;
            }
            Action::Width(w) => {
                self.nib.width = w;
                self.store.set("pen", &self.nib)?;
            }
            Action::Color(c) => {
                self.nib.color = c;
                self.store.set("pen", &self.nib)?;
            }
            Action::Undo | Action::Redo => {
                if self.sharing.as_ref().is_some_and(|s| s.replica.is_some()) {
                    return self.shared_undo(action == Action::Redo);
                }
                if let Some(p) = self.pages.get_mut(self.index) {
                    let h = self.histories.entry(p.id.clone()).or_default();
                    if action == Action::Undo {
                        h.undo(&mut p.payload);
                    } else {
                        h.redo(&mut p.payload);
                    }
                    self.selection = Selection::default();
                    self.save()?;
                }
            }
            Action::DeleteSelection => {
                self.push();
                if let Some(p) = self.pages.get_mut(self.index) {
                    let mut i = 0;
                    p.payload.strokes.retain(|_| {
                        let keep = !self.selection.strokes.contains(&i);
                        i += 1;
                        keep
                    });
                    let mut i = 0;
                    p.payload.texts.retain(|_| {
                        let keep = !self.selection.texts.contains(&i);
                        i += 1;
                        keep
                    });
                    self.selection = Selection::default();
                    self.save()?;
                }
            }
            Action::Overview => {
                self.screen = Screen::Overview;
                self.overview_offset = 0;
            }
            Action::OverviewTab(t) => {
                self.overview_tab = t;
                self.tag = None;
                self.overview_offset = 0;
            }
            Action::TagFilter(t) => {
                self.tag = Some(t);
                self.overview_offset = 0;
            }
            Action::Page(i) => {
                self.go(i)?;
                self.screen = Screen::Editor;
            }
            Action::Previous => {
                if self.index > 0 {
                    self.go(self.index - 1)?;
                }
            }
            Action::Next => {
                if self.index + 1 < self.pages.len() {
                    self.go(self.index + 1)?;
                }
            }
            Action::AddPage => {
                self.flush()?;
                let d = self.doc.as_ref().context("Open a notebook first")?;
                let template = self
                    .page()
                    .map(|p| p.payload.template.clone())
                    .unwrap_or("Lined".into());
                self.store.add_page(&d.id, self.index, &template)?;
                self.pages = self.store.pages(&d.id)?;
                self.capture_shared(true)?;
                self.go(self.index + 1)?;
                self.screen = Screen::Editor;
                self.dialog = None;
            }
            Action::Bookmark => {
                if let Some(p) = self.pages.get_mut(self.index) {
                    p.bookmarked = !p.bookmarked;
                    self.store.meta(p)?;
                    self.capture_shared(false)?;
                }
            }
            Action::Menu => {
                let notebook = self.doc.as_ref().is_some_and(|d| d.kind == "notebook");
                let mut items = vec![
                    ("Template", Action::TemplatePanel),
                    ("Page tags", Action::Tags),
                    ("Share document", Action::Sharing),
                    ("Fit page / fit width", Action::Fit),
                    ("Export annotated PDF", Action::Export),
                    ("Settings", Action::Settings),
                ];
                if notebook {
                    items.extend([
                        ("Add page", Action::AddPage),
                        ("Clear this page", Action::ClearPage),
                        ("Delete this page", Action::DeletePage),
                    ]);
                }
                self.menu("Page", items);
            }
            Action::TemplatePanel => {
                self.dialog = Some(Dialog::Menu(
                    "Page template".into(),
                    TEMPLATES
                        .iter()
                        .map(|s| (s.to_string(), Action::Template(s.to_string())))
                        .collect(),
                ));
            }
            Action::Template(t) => {
                self.push();
                if let Some(p) = self.pages.get_mut(self.index) {
                    if self.doc.as_ref().is_some_and(|d| d.kind == "notebook") {
                        p.payload.template = t;
                        self.save()?;
                    }
                }
                self.dialog = None;
            }
            Action::Tags => {
                if let Some(p) = self.page() {
                    self.input(
                        "Page tags · separate with commas",
                        p.tags.join(", "),
                        InputPurpose::Tags,
                        false,
                    );
                }
            }
            Action::Export => {
                if let Some(d) = &self.doc {
                    let name: String = d
                        .title
                        .chars()
                        .map(|c| {
                            if c.is_alphanumeric() || " -_".contains(c) {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect();
                    self.input(
                        "Export annotated PDF",
                        self.store
                            .root
                            .join("exports")
                            .join(format!("{name}.pdf"))
                            .display()
                            .to_string(),
                        InputPurpose::Export,
                        false,
                    );
                }
            }
            Action::DeletePage => {
                self.dialog = Some(Dialog::Confirm(
                    "Delete this page permanently?".into(),
                    Action::Key("delete-page".into()),
                ));
            }
            Action::ClearPage => {
                self.dialog = Some(Dialog::Confirm(
                    "Clear all ink and text on this page? You can undo this.".into(),
                    Action::Key("clear-page".into()),
                ));
            }
            Action::Rename(id) => {
                let title = self.store.document(&id)?.title;
                self.input("Rename item", title, InputPurpose::Rename(id), false);
            }
            Action::Move(id) => {
                let mut items = vec![("My files".into(), Action::MoveTo(id.clone(), None))];
                for d in self.store.folders()? {
                    if d.id != id {
                        items.push((d.title, Action::MoveTo(id.clone(), Some(d.id))));
                    }
                }
                self.dialog = Some(Dialog::Menu("Move to folder".into(), items));
            }
            Action::MoveTo(id, parent) => {
                self.store.move_to(&id, parent.as_deref())?;
                self.dialog = None;
                self.reload_library()?;
            }
            Action::Trash(id, trash) => {
                if self
                    .sharing
                    .as_ref()
                    .is_some_and(|s| s.local_document.as_deref() == Some(&id))
                {
                    self.sharing = None;
                    self.histories.clear();
                }
                self.store.trash(&id, trash)?;
                self.dialog = None;
                self.reload_library()?;
            }
            Action::ItemMenu(id) => {
                let d = self.store.document(&id)?;
                let mut items = vec![
                    ("Rename", Action::Rename(id.clone())),
                    ("Move to folder", Action::Move(id.clone())),
                ];
                if d.trashed {
                    items.push(("Restore", Action::Trash(id, false)));
                } else {
                    items.push(("Move to trash", Action::Trash(id, true)));
                }
                self.menu(&d.title, items);
            }
            Action::Fit => {
                self.fit = !self.fit;
                self.scroll = 0.;
                self.dialog = None;
            }
            Action::Status => {
                self.dialog = Some(
                    Dialog::Message(
                        "Tablet status".into(),
                        format!(
                            "Wi-Fi: {}\nNetwork: {}\nSignal: {}\nBattery: {}\n{}\n\nStatus updates every 15 seconds. Use the system network menu to change connections.",
                            self.status.wifi, self.status.ssid, self.status.signal.map(|
                            v | format!("{v}%")).unwrap_or("Unavailable".into()), self
                            .status.battery.map(| v | format!("{v}%"))
                            .unwrap_or("Unavailable".into()), if self.status.charging {
                            "External power / charging" } else { "On battery" }
                        ),
                    ),
                );
            }
            Action::Help => {
                self.dialog = Some(
                    Dialog::Message(
                        "PinePaper · Rust".into(),
                        "Pen: write on contact. Tap active Pen for nib settings. Lasso: surround ink, then drag it. Fingers scroll; palm gestures are suppressed near the pen.\n\nNotes save automatically after each stroke. Import PDF, EPUB and text from My files. Export PDF or back up the entire library as ZIP.\n\nF11 fullscreen · Ctrl+N new · Ctrl+O import · Ctrl+Z undo · Ctrl+Shift+Z redo · Page Up/Down · Escape tools · Ctrl+Q quit"
                            .into(),
                    ),
                );
            }
            Action::Close => {
                if matches!(& self.dialog, Some(Dialog::Settings(s)) if s.light_busy || s.display_busy)
                {
                    self.close_after_settings = true;
                    return Ok(());
                }
                self.dialog = None;
            }
            Action::Confirm => {
                if let Some(Dialog::Confirm(_, a)) = self.dialog.take() {
                    self.action(a)?;
                }
            }
            Action::Accept => self.accept(false)?,
            Action::Keyboard => {
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.keyboard = !i.keyboard;
                }
            }
            Action::ShowKeyboard => {
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.keyboard = true;
                }
            }
            Action::Key(k) if k == "delete-page" => {
                self.flush()?;
                if let Some(p) = self.page().cloned() {
                    self.store.delete_page(&p)?;
                    self.pages = self.store.pages(&p.document)?;
                    self.capture_shared(true)?;
                    self.go(self.index.min(self.pages.len() - 1))?;
                }
                self.dialog = None;
            }
            Action::Key(k) if k == "clear-page" => {
                self.push();
                if let Some(p) = self.pages.get_mut(self.index) {
                    p.payload.strokes.clear();
                    p.payload.texts.clear();
                    self.save()?;
                }
                self.dialog = None;
            }
            Action::Key(k) if k == "overwrite" => self.accept(true)?,
            Action::Key(k) => {
                if k == "Enter" {
                    if let Some(Dialog::Input(i)) = &mut self.dialog {
                        if i.multiline {
                            i.insert("\n", false);
                        } else {
                            self.accept(false)?;
                        }
                    }
                } else if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.insert(&k, false);
                    if i.shift == 1 {
                        i.shift = 0;
                    }
                }
            }
            Action::Shift => {
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.shift = (i.shift + 1) % 3;
                }
            }
            Action::KeyMode(m) => {
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.mode = m;
                }
            }
            Action::SelectAll => {
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.anchor = 0;
                    i.cursor = i.value.chars().count();
                }
            }
            Action::Copy => {
                if let Some(Dialog::Input(i)) = &self.dialog {
                    self.clipboard = i
                        .value
                        .chars()
                        .skip(i.cursor.min(i.anchor))
                        .take(i.cursor.abs_diff(i.anchor))
                        .collect();
                    let value = self.clipboard.clone();
                    thread::spawn(move || {
                        let _ = platform::clipboard_copy(&value);
                    });
                }
            }
            Action::Paste => {
                let text = platform::clipboard_paste().unwrap_or_else(|_| self.clipboard.clone());
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.insert(&text, false);
                }
            }
            Action::Left | Action::Right => {
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    if action == Action::Left {
                        i.cursor = i.cursor.saturating_sub(1);
                    } else {
                        i.cursor = (i.cursor + 1).min(i.value.chars().count());
                    }
                    i.anchor = i.cursor;
                }
            }
            Action::Backspace => {
                if let Some(Dialog::Input(i)) = &mut self.dialog {
                    i.insert("", true);
                }
            }
            Action::SettingTab(t) => {
                if let Some(Dialog::Settings(s)) = &mut self.dialog {
                    s.tab = t;
                    s.scroll = 0.;
                }
            }
            Action::Setting(k, d) => {
                self.adjust(&k, d);
                if k == "brightness" || k == "warmth" {
                    self.queue_lights()?;
                } else {
                    self.live_settings()?;
                }
            }
            Action::Slider(_) => (), // Pointer capture supplies the absolute value.
            Action::SetSlider(kind, value) => {
                let Some(Dialog::Settings(s)) = &mut self.dialog else {
                    return Ok(());
                };
                if !kind.enabled(s) || !value.is_finite() {
                    return Ok(());
                }
                let value = kind.snap(value);
                if (kind.value(s) - value).abs() < 1e-6 {
                    return Ok(());
                }
                match kind {
                    SliderSetting::Brightness => s.brightness = value,
                    SliderSetting::Warmth => s.warmth = value,
                    SliderSetting::Contrast => s.prefs.page_contrast = value,
                    SliderSetting::Gamma => s.prefs.page_gamma = value,
                    SliderSetting::Font => s.prefs.ui_font_size = value,
                    SliderSetting::Delay => s.hardware.delay = Some(value as u32),
                    SliderSetting::Periodic => s.prefs.refresh_pages = value as usize,
                    SliderSetting::KeyHeight => s.prefs.keyboard_key_height = value,
                }
                if matches!(kind, SliderSetting::Brightness | SliderSetting::Warmth) {
                    s.hardware.lights(s.brightness, s.warmth);
                    self.queue_lights()?;
                } else {
                    self.live_settings()?;
                }
            }
            Action::Switch(kind) => {
                self.choice(kind.key());
                self.live_settings()?;
            }
            Action::Choice(k) => {
                self.choice(&k);
                self.live_settings()?;
            }
            Action::LightPreset(b, w) => {
                if let Some(Dialog::Settings(s)) = &mut self.dialog {
                    if !s.loaded || s.before.cool.is_none() || s.before.warm.is_none() {
                        return Ok(());
                    }
                    s.brightness = b;
                    s.warmth = w;
                    s.hardware.lights(b, w);
                }
                self.queue_lights()?;
            }
            Action::Profile(name) => {
                if let Some(Dialog::Settings(s)) = &mut self.dialog {
                    if !s.loaded {
                        return Ok(());
                    }
                    let (h, m, d) = display_profile(&name);
                    s.hardware.hint = s.before.hint.as_ref().map(|_| h.into());
                    s.prefs.display_hint = h.into();
                    s.hardware.driver = s.before.driver.map(|_| m);
                    s.hardware.delay = s.before.delay.map(|_| d);
                    s.hardware.dither = s.before.dither.map(|_| 2);
                }
                self.live_settings()?;
            }
            Action::ResetSettings => {
                let tab = if let Some(Dialog::Settings(s)) = &self.dialog {
                    s.tab
                } else {
                    0
                };
                if tab == 0 {
                    return self.action(Action::LightPreset(0., 50.));
                } else if tab == 2 {
                    return self.action(Action::Profile("Balanced".into()));
                } else if let Some(Dialog::Settings(s)) = &mut self.dialog {
                    let p = Prefs::default();
                    if tab == 1 {
                        s.prefs.page_contrast = p.page_contrast;
                        s.prefs.page_gamma = p.page_gamma;
                        s.prefs.ui_font_size = p.ui_font_size;
                        s.prefs.start_fullscreen = p.start_fullscreen;
                        s.prefs.hide_cursor = p.hide_cursor;
                        s.prefs.finger_draw = p.finger_draw;
                    } else if tab == 4 {
                        s.prefs.exchange_url.clear();
                    } else {
                        s.prefs.keyboard_layout = p.keyboard_layout;
                        s.prefs.keyboard_key_height = p.keyboard_key_height;
                        s.prefs.keyboard_auto = p.keyboard_auto;
                    }
                }
                self.live_settings()?;
            }
            Action::Refresh => {
                self.jobs.send(Work::Refresh)?;
                self.notice = "Refreshing display…".into();
            }
            Action::Browse(path) => self.browse(path)?,
            Action::File(path) => self.input(
                "Import file",
                path.display().to_string(),
                InputPurpose::Import,
                false,
            ),
            Action::ChooseFile => self.input(
                "Import PDF, EPUB or text path",
                String::new(),
                InputPurpose::Import,
                false,
            ),
            Action::FileUp => {
                if let Some(Dialog::Files { dir, .. }) = &self.dialog {
                    let p = dir.parent().unwrap_or(dir).to_path_buf();
                    self.browse(p)?;
                }
            }
        }
        Ok(())
    }
    fn browse(&mut self, dir: PathBuf) -> Result<()> {
        let mut entries = vec![];
        for e in std::fs::read_dir(&dir)? {
            let e = e?;
            let is_dir = e.file_type()?.is_dir();
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.')
                && (is_dir
                    || ["pdf", "epub", "txt", "md"].contains(
                        &e.path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase()
                            .as_str(),
                    ))
            {
                entries.push((name, e.path(), is_dir));
            }
        }
        entries.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
        self.dialog = Some(Dialog::Files {
            dir,
            entries,
            offset: 0,
        });
        Ok(())
    }
    fn accept(&mut self, overwrite: bool) -> Result<()> {
        let Some(Dialog::Input(input)) = self.dialog.clone() else {
            return Ok(());
        };
        let text = input.value.trim().to_string();
        match input.purpose {
            InputPurpose::ExchangeUrl => {
                if !text.is_empty() {
                    crate::exchange::transport::endpoint(&text)?;
                }
                self.prefs.exchange_url = text;
                self.prefs.save(&self.store)?;
                self.dialog = Some(Dialog::Sharing);
            }
            InputPurpose::JoinCode => {
                let code = crate::exchange::transport::room_code(&text)?;
                self.start_sharing(Some(code))?;
            }
            InputPurpose::Create(kind) => {
                let pages = if kind == "folder" {
                    vec![]
                } else {
                    vec![Payload::blank("Lined")]
                };
                let id = self
                    .store
                    .create(&text, &kind, self.folder.as_deref(), pages, None)?;
                self.dialog = None;
                if kind == "folder" {
                    self.reload_library()?;
                } else {
                    self.open(&id)?;
                }
            }
            InputPurpose::Rename(id) => {
                self.store.rename(&id, &text)?;
                if let Some(d) = &mut self.doc {
                    if d.id == id {
                        d.title = self.store.document(&id)?.title;
                    }
                }
                self.capture_shared(false)?;
                self.dialog = None;
                self.reload_library()?;
            }
            InputPurpose::Search => {
                self.query = text;
                self.library_offset = 0;
                self.reload_library()?;
                self.dialog = None;
            }
            InputPurpose::Tags => {
                if let Some(p) = self.pages.get_mut(self.index) {
                    let mut tags = vec![];
                    for s in text.split(',') {
                        let s: String = s.trim().chars().take(60).collect();
                        if !s.is_empty() && !tags.contains(&s) {
                            tags.push(s);
                        }
                    }
                    p.tags = tags;
                    self.store.meta(p)?;
                    self.capture_shared(false)?;
                }
                self.dialog = None;
            }
            InputPurpose::Text(idx, pos) => {
                self.push();
                if let Some(p) = self.pages.get_mut(self.index) {
                    if let Some(i) = idx {
                        if text.is_empty() {
                            p.payload.texts.remove(i);
                        } else {
                            p.payload.texts[i].text = input.value;
                        }
                    } else if !text.is_empty() {
                        p.payload.texts.push(Text {
                            text: input.value,
                            x: pos[0],
                            y: pos[1],
                            size: 25.,
                            color: 0.08,
                            font: "Sans".into(),
                            extra: BTreeMap::new(),
                        });
                    }
                    self.save()?;
                }
                self.dialog = None;
            }
            InputPurpose::Import => {
                self.flush()?;
                self.jobs.send(Work::Import(
                    platform::expand_path(&text),
                    self.folder.clone(),
                ))?;
                self.busy = true;
                self.dialog = None;
                self.notice = "Importing document…".into();
            }
            purpose @ (InputPurpose::Export | InputPurpose::Backup) => {
                let path = platform::expand_path(&text);
                if path.exists() && !overwrite {
                    self.notice =
                        "File exists. Tap Save again to replace it, or change the path.".into();
                    if let Some(Dialog::Input(i)) = &mut self.dialog {
                        i.title = "Replace existing file? Save again to confirm".into();
                    }
                    return Ok(());
                }
                self.flush()?;
                if matches!(purpose, InputPurpose::Export) {
                    let source = self
                        .doc
                        .as_ref()
                        .map(|d| document::pdf_source(&self.store, d))
                        .transpose()?
                        .flatten();
                    self.jobs
                        .send(Work::Export(path, self.pages.clone(), source))?;
                } else {
                    self.jobs.send(Work::Backup(path))?;
                }
                self.busy = true;
                self.dialog = None;
                self.notice = "Writing file…".into();
            }
        }
        Ok(())
    }
    pub fn open(&mut self, id: &str) -> Result<()> {
        self.flush()?;
        let d = self.store.document(id)?;
        if d.kind == "folder" {
            self.folder = Some(d.id);
            self.trash = false;
            self.filter.clear();
            self.query.clear();
            self.dialog = None;
            self.library_offset = 0;
            self.reload_library()?;
            return Ok(());
        }
        if self
            .sharing
            .as_ref()
            .is_some_and(|s| s.local_document.as_deref() != Some(id))
        {
            self.sharing = None;
        }
        let pages = self.store.pages(id)?;
        if pages.is_empty() {
            anyhow::bail!("Document has no pages");
        }
        self.hidden = d.kind != "notebook";
        self.fit = self.hidden;
        let index = d.current_page.min(pages.len() - 1);
        self.doc = Some(d);
        self.pages = pages;
        self.screen = Screen::Editor;
        self.dialog = None;
        self.histories.clear();
        self.go(index)?;
        Ok(())
    }
    pub fn go(&mut self, index: usize) -> Result<()> {
        self.finish();
        self.flush()?;
        self.index = index.min(self.pages.len().saturating_sub(1));
        self.scroll = 0.;
        self.selection = Selection::default();
        self.pdf = None;
        if let Some(d) = &self.doc {
            self.store.open_at(&d.id, self.index)?;
            if d.kind == "pdf" {
                self.jobs.send(Work::Pdf(
                    d.id.clone(),
                    self.index,
                    self.store.source_path(d)?,
                ))?;
                self.notice = "Rendering PDF…".into();
            } else {
                self.notice = String::new();
            }
        }
        self.page_opens += 1;
        if self.prefs.refresh_pages > 0 && self.page_opens.is_multiple_of(self.prefs.refresh_pages)
        {
            let _ = self.jobs.send(Work::Refresh);
        }
        self.redraw = true;
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        self.saver.flush()?;
        self.saving_error = false;
        if self.notice.starts_with("NOT SAVED:") {
            self.notice.clear();
        }
        Ok(())
    }
    fn push(&mut self) {
        if self.sharing.is_some() {
            return;
        }
        if let Some(p) = self.page() {
            let id = p.id.clone();
            let payload = p.payload.clone();
            self.histories.entry(id).or_default().push(&payload);
        }
    }
    fn save(&mut self) -> Result<()> {
        // A sharing limit must never prevent saving the user's own ink.
        if let Err(error) = self.capture_shared(false) {
            self.sharing = None;
            self.notice = format!("Sharing stopped: {error}. Saving locally.");
        }
        if let Some(p) = self.pages.get(self.index) {
            self.saver.save(p)?;
            self.last_save = Instant::now();
        }
        Ok(())
    }
    pub fn request_thumbnail(&mut self, id: &str, index: usize, path: PathBuf) {
        let key = (id.to_string(), index);
        if self.thumb_requested.insert(key) {
            let _ = self.thumb_jobs.send((id.into(), index, path));
        }
    }
    pub fn tick(&mut self) {
        self.expire_key_feedback(Instant::now());
        self.poll_exchange();
        if let Ok((stream, _)) = self.activation.accept() {
            use std::io::Read;
            let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
            let mut data = String::new();
            if stream.take(65536).read_to_string(&mut data).is_ok() {
                if let Ok(path) = serde_json::from_str::<Option<PathBuf>>(&data) {
                    if let Some(path) = path {
                        let _ = self.jobs.send(Work::Import(path, self.folder.clone()));
                        self.busy = true;
                    }
                    self.fullscreen = self.prefs.start_fullscreen;
                    self.fullscreen_changed = true;
                    let fullscreen = self.fullscreen;
                    thread::spawn(move || {
                        let _ = platform::activate(fullscreen);
                    });
                }
            }
        }

        let mut status = false;
        while let Ok(s) = self.status_rx.try_recv() {
            if s != self.status {
                self.status = s;
                status = true;
            }
        }
        while let Ok(event) = self.saver.events.try_recv() {
            self.saver.pending = self.saver.pending.saturating_sub(1);
            if let Err(e) = event {
                self.saving_error = true;
                self.notice = format!("NOT SAVED: {e}. Keep the app open and free disk space.");
            } else if !self.saving_error {
                self.notice = String::new();
            }
            status = true;
        }
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Thumb(id, index, img) => {
                    if let Some(img) = img {
                        self.thumbnails.insert((id, index), img);
                        if self.thumbnails.len() > 100 {
                            self.thumbnails.clear();
                            self.thumb_requested.clear();
                        }
                    }
                    if self.screen == Screen::Editor {
                        continue;
                    }
                }
                Done::Imported(id) => {
                    self.busy = false;
                    if let Err(e) = self.open(&id) {
                        self.fail(e);
                    }
                }
                Done::Exported(p) | Done::BackedUp(p) => {
                    self.busy = false;
                    self.dialog = Some(Dialog::Message(
                        "File saved".into(),
                        p.display().to_string(),
                    ));
                    self.notice = String::new();
                }
                Done::Pdf(id, i, img) => {
                    if self.doc.as_ref().is_some_and(|d| d.id == id) && self.index == i {
                        self.pdf = Some(img);
                        self.notice = String::new();
                    }
                }
                Done::Hardware(h) => {
                    if let Some(Dialog::Settings(s)) = &mut self.dialog {
                        let (b, w) = h.light_controls();
                        s.brightness = b;
                        s.warmth = w;
                        s.hardware = h.clone();
                        s.before = h;
                        s.loaded = true;
                    }
                }
                Done::Display(h, error) => {
                    self.display_in_flight = false;
                    if self
                        .display_pending
                        .as_ref()
                        .is_some_and(|pending| h.matches_requested(pending))
                    {
                        self.display_pending = None;
                    }
                    if error.is_some() {
                        self.display_pending = None;
                        self.close_after_settings = false;
                        self.quit_after_settings = false;
                        if let Some(hint) = &h.hint {
                            self.prefs.display_hint = hint.clone();
                        }
                        if let Err(e) = self.prefs.save(&self.store) {
                            self.fail(e);
                        }
                    }
                    if let Some(Dialog::Settings(s)) = &mut self.dialog {
                        s.before.copy_display_from(&h);
                        if self.display_pending.is_none() {
                            s.hardware.copy_display_from(&h);
                        }
                        if error.is_some() {
                            s.prefs.display_hint = self.prefs.display_hint.clone();
                        }
                        if let Some(e) = &error {
                            s.error = Some(format!("Display adjustment failed: {e}"));
                        }
                        s.display_busy = self.display_pending.is_some();
                    }
                    self.notice = error
                        .map(|e| format!("Display adjustment failed: {e}"))
                        .unwrap_or_default();
                    if let Err(e) = self
                        .send_pending_display()
                        .and_then(|_| self.finish_settings())
                    {
                        self.fail(e);
                    }
                }
                Done::Lights(h, error) => {
                    self.light_in_flight = false;
                    if error.is_some() {
                        self.light_pending = None;
                        self.close_after_settings = false;
                        self.quit_after_settings = false;
                    }
                    if let Some(Dialog::Settings(s)) = &mut self.dialog {
                        // Light and display completions only update their own
                        // controls; neither may overwrite a newer adjustment.
                        s.before.cool = h.cool;
                        s.before.warm = h.warm;
                        if self.light_pending.is_none() {
                            s.hardware.cool = h.cool;
                            s.hardware.warm = h.warm;
                        }
                        if error.is_some() {
                            (s.brightness, s.warmth) = h.light_controls();
                        }
                        if let Some(e) = &error {
                            s.error = Some(format!("Light adjustment failed: {e}"));
                        }
                        s.light_busy = self.light_pending.is_some();
                    }
                    self.notice = error
                        .map(|e| format!("Light adjustment failed: {e}"))
                        .unwrap_or_default();
                    if let Err(e) = self
                        .send_pending_lights()
                        .and_then(|_| self.finish_settings())
                    {
                        self.fail(e);
                    }
                }
                Done::Refreshed => self.notice = "Display refreshed".into(),
                Done::Error(e) => {
                    self.busy = false;
                    self.fail(e);
                }
            }
            self.redraw = true;
        }
        if status && self.gesture.is_none() && !self.redraw {
            ui::status(self);
        }
    }
    fn keyboard_mode(&self) -> Option<usize> {
        match &self.dialog {
            Some(Dialog::Input(i)) if i.keyboard => Some(i.mode),
            _ => None,
        }
    }
    pub fn key_flashing(&self, bounds: Box2) -> bool {
        self.key_feedback.iter().any(|(b, _)| *b == bounds)
    }
    fn flash_key(&mut self, bounds: Box2) {
        let until = Instant::now() + Duration::from_millis(500);
        if let Some((_, deadline)) = self.key_feedback.iter_mut().find(|(b, _)| *b == bounds) {
            *deadline = until;
        } else {
            self.key_feedback.push((bounds, until));
        }
        ui::repaint_keys(self, &[bounds]);
    }
    fn expire_key_feedback(&mut self, now: Instant) {
        if self.keyboard_mode().is_none() {
            self.key_feedback.clear();
            return;
        }
        let mut expired = Vec::new();
        self.key_feedback.retain(|(bounds, until)| {
            if now >= *until {
                expired.push(*bounds);
                false
            } else {
                true
            }
        });
        ui::repaint_keys(self, &expired);
    }
    pub fn down(&mut self, x: f64, y: f64, pressure: f64, finger: bool) {
        if self.busy || self.gesture.is_some() {
            return;
        }
        if finger && Instant::now() < self.palm_until {
            return;
        }
        self.finger = finger;
        self.press_start = (x, y);
        self.last_pointer = (x, y);
        self.press = self.hit(x, y);
        if self.keyboard_mode().is_some() {
            if let Some((bounds, action)) = self.hits.iter().rev().find(|(b, _)| b.contains(x, y)) {
                if bounds.y >= self.surface.logical_h - ui::keyboard_height(self)
                    && matches!(
                        action,
                        Action::Key(_)
                            | Action::Shift
                            | Action::KeyMode(_)
                            | Action::Backspace
                            | Action::Left
                            | Action::Right
                            | Action::SelectAll
                            | Action::Copy
                            | Action::Paste
                    )
                {
                    self.flash_key(*bounds);
                }
            }
        }
        self.control_drag = self.hits.iter().rev().find_map(|(bounds, action)| {
            (bounds.contains(x, y) && matches!(action, Action::Slider(_) | Action::Switch(_))).then(
                || ControlDrag {
                    action: action.clone(),
                    bounds: *bounds,
                },
            )
        });
        if self.control_drag.is_some() {
            self.press = None;
            self.drag_slider(x);
            return;
        }
        if self.press.is_some() || self.dialog.is_some() || self.screen != Screen::Editor {
            return;
        }
        if !self.view.contains(x, y) {
            return;
        }
        if finger && !self.prefs.finger_draw {
            return;
        }
        let p = self.view.world(x, y, pressure);
        self.push();
        self.surface.begin_stroke();
        match self.tool.as_str() {
            "pen" | "highlighter" => {
                let mut s = self.nib.clone();
                s.tool = self.tool.clone();
                s.points = vec![p];
                let r = self.surface.segment(self.view, p, p, &s);
                self.dirty(r);
                self.gesture = Some(Gesture::Ink(s));
            }
            "eraser" => {
                if let Some(page) = self.pages.get_mut(self.index) {
                    erase(&mut page.payload, p, p);
                }
                self.gesture = Some(Gesture::Erase(p));
                self.redraw = true;
            }
            "lasso" => {
                if self
                    .selection
                    .bounds
                    .is_some_and(|b| b.contains(p[0], p[1]))
                {
                    if let Some(page) = self.page() {
                        self.gesture = Some(Gesture::Move(p, page.payload.clone()));
                    }
                } else {
                    self.selection = Selection::default();
                    self.gesture = Some(Gesture::Lasso(vec![p]));
                }
            }
            "text" => {
                self.gesture = Some(Gesture::Text);
            }
            _ => (),
        }
    }
    fn drag_slider(&mut self, x: f64) {
        if let Some(ControlDrag {
            action: Action::Slider(kind),
            bounds,
        }) = &self.control_drag
        {
            let (kind, value) = (*kind, kind.at(*bounds, x));
            if let Some(Dialog::Settings(s)) = &self.dialog {
                if (kind.value(s) - value).abs() >= 1e-6 {
                    self.act(Action::SetSlider(kind, value));
                }
            }
        }
    }
    pub fn motion(&mut self, x: f64, y: f64, pressure: f64) {
        let dy = y - self.last_pointer.1;
        self.last_pointer = (x, y);
        if self.control_drag.is_some() {
            self.drag_slider(x);
            return;
        }
        if self.press.is_some() {
            if (x - self.press_start.0).hypot(y - self.press_start.1) > 18. {
                self.press = None;
            }
            return;
        }
        if self.gesture.is_none() {
            if self.finger {
                self.scroll_by(-dy);
            }
            return;
        }
        let mut p = self.view.world(x, y, pressure);
        p[0] = p[0].clamp(0., PAGE_W);
        p[1] = p[1].clamp(0., PAGE_H);
        let mut gesture = self.gesture.take().unwrap();
        match &mut gesture {
            Gesture::Ink(s) => {
                let a = *s.points.last().unwrap();
                if (p[0] - a[0]).hypot(p[1] - a[1]) > 0.015 {
                    s.points.push(p);
                    let r = self.surface.segment(self.view, a, p, s);
                    self.dirty(r);
                }
            }
            Gesture::Erase(a) => {
                if let Some(page) = self.pages.get_mut(self.index) {
                    if erase(&mut page.payload, *a, p) {
                        self.redraw = true;
                    }
                }
                *a = p;
            }
            Gesture::Lasso(poly) => {
                poly.push(p);
                let a = poly[poly.len().saturating_sub(2)];
                let s = Stroke {
                    width: 0.9,
                    color: 0.4,
                    ..Stroke::default()
                };
                self.surface.begin_stroke();
                let r = self.surface.segment(self.view, a, p, &s);
                self.dirty(r);
            }
            Gesture::Move(start, original) => {
                if let Some(page) = self.pages.get_mut(self.index) {
                    page.payload = original.clone();
                    let (dx, dy) = (p[0] - start[0], p[1] - start[1]);
                    for &i in &self.selection.strokes {
                        for q in &mut page.payload.strokes[i].points {
                            q[0] += dx;
                            q[1] += dy;
                        }
                    }
                    for &i in &self.selection.texts {
                        page.payload.texts[i].x += dx;
                        page.payload.texts[i].y += dy;
                    }
                    self.redraw = true;
                }
            }
            Gesture::Text => (),
        }
        self.gesture = Some(gesture);
    }
    pub fn up(&mut self, x: f64, y: f64) {
        let moved = (x - self.press_start.0).hypot(y - self.press_start.1);
        if let Some(drag) = self.control_drag.take() {
            match drag.action {
                Action::Slider(kind) => self.act(Action::SetSlider(kind, kind.at(drag.bounds, x))),
                Action::Switch(kind) => {
                    let dy = (y - self.press_start.1).abs();
                    let dx = x - self.press_start.0;
                    if dx.abs() >= 12. && dy < 32. {
                        if let Some(Dialog::Settings(s)) = &self.dialog {
                            let target = x >= drag.bounds.x + drag.bounds.w / 2.;
                            if kind.value(s) != target {
                                self.act(Action::Switch(kind));
                            }
                        }
                    } else if moved < 18. && drag.bounds.contains(x, y) {
                        self.act(Action::Switch(kind));
                    }
                }
                _ => (),
            }
            self.finger = false;
            return;
        }
        if let Some(a) = self.press.take() {
            if moved < 18. && self.hit(x, y) == Some(a.clone()) {
                self.act(a);
            }
        } else if self.gesture.is_some() {
            self.finish();
        } else if self.screen == Screen::Editor
            && self.dialog.is_none()
            && self.doc.as_ref().is_some_and(|d| d.kind != "notebook")
        {
            let dx = x - self.press_start.0;
            if dx.abs() > 80. {
                self.act(if dx < 0. {
                    Action::Next
                } else {
                    Action::Previous
                });
            } else if moved < 12. {
                if x < self.surface.logical_w * 0.16 {
                    self.act(Action::Previous);
                } else if x > self.surface.logical_w * 0.84 {
                    self.act(Action::Next);
                }
            }
        }
        self.finger = false;
    }
    pub fn pen_near(&mut self) {
        self.palm_until = Instant::now() + Duration::from_millis(700);
        self.finger = false;
    }
    pub fn finish(&mut self) {
        self.control_drag = None;
        self.press = None;
        self.finger = false;
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        match gesture {
            Gesture::Ink(s) => {
                if let Some(p) = self.pages.get_mut(self.index) {
                    p.payload.strokes.push(s);
                }
                if let Err(e) = self.save() {
                    self.fail(e);
                } /* no page rebuild on pen-up */
            }
            Gesture::Erase(_) | Gesture::Move(..) => {
                self.selection_bounds();
                if let Err(e) = self.save() {
                    self.fail(e);
                }
                self.redraw = true;
            }
            Gesture::Lasso(poly) => {
                if let Some(p) = self.page() {
                    self.selection = Selection {
                        strokes: p
                            .payload
                            .strokes
                            .iter()
                            .enumerate()
                            .filter(|(_, s)| s.points.iter().any(|&p| inside(p, &poly)))
                            .map(|(i, _)| i)
                            .collect(),
                        texts: p
                            .payload
                            .texts
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| inside([t.x, t.y, 0.], &poly))
                            .map(|(i, _)| i)
                            .collect(),
                        bounds: None,
                    };
                }
                self.selection_bounds();
                self.redraw = true;
            }
            Gesture::Text => {
                let p = self.view.world(self.press_start.0, self.press_start.1, 0.5);
                let idx = self.page().and_then(|page| {
                    page.payload.texts.iter().position(|t| {
                        Box2::new(
                            t.x,
                            t.y,
                            (t.text.chars().count() as f64 * t.size * 0.6).min(PAGE_W - t.x),
                            t.size * 1.5 * (t.text.lines().count().max(1) as f64),
                        )
                        .contains(p[0], p[1])
                    })
                });
                let value = idx
                    .and_then(|i| self.page().map(|p| p.payload.texts[i].text.clone()))
                    .unwrap_or_default();
                self.input("Write a note", value, InputPurpose::Text(idx, p), true);
                self.redraw = true;
            }
        }
    }
    fn selection_bounds(&mut self) {
        let mut points = vec![];
        if let Some(p) = self.page() {
            for &i in &self.selection.strokes {
                if let Some(s) = p.payload.strokes.get(i) {
                    points.extend(s.points.iter().copied());
                }
            }
            for &i in &self.selection.texts {
                if let Some(t) = p.payload.texts.get(i) {
                    points.extend([
                        [t.x, t.y, 0.],
                        [
                            t.x + (t.text.chars().count() as f64 * t.size * 0.6).min(300.),
                            t.y + t.size * 1.5,
                            0.,
                        ],
                    ]);
                }
            }
        }
        self.selection.bounds = if points.is_empty() {
            None
        } else {
            let x = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min) - 8.;
            let y = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) - 8.;
            let r = points
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max)
                + 8.;
            let b = points
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max)
                + 8.;
            Some(Box2::new(x, y, r - x, b - y))
        };
    }
    pub fn scroll_by(&mut self, dy: f64) {
        if let Some(d) = &mut self.dialog {
            match d {
                Dialog::Files {
                    offset, entries, ..
                } => {
                    *offset = (*offset as isize + if dy > 0. { 1 } else { -1 })
                        .clamp(0, entries.len().saturating_sub(1) as isize)
                        as usize;
                }
                Dialog::Settings(s) => s.scroll = (s.scroll + dy).clamp(0., 800.),
                Dialog::Menu(_, items) => {
                    self.menu_offset = self
                        .menu_offset
                        .saturating_add_signed(if dy > 0. { 1 } else { -1 })
                        .min(items.len().saturating_sub(1));
                }
                _ => (),
            }
            self.redraw = true;
            return;
        }
        match self.screen {
            Screen::Editor => {
                self.scroll = (self.scroll + dy)
                    .clamp(0., (PAGE_H * self.view.zoom - self.view.clip.h).max(0.));
            }
            Screen::Library => {
                self.library_offset = (self.library_offset as f64 + dy)
                    .clamp(0., ui::library_scroll_max(self))
                    .round() as usize;
            }
            Screen::Overview => {
                self.overview_offset = (self.overview_offset as isize
                    + if dy > 0. { 4 } else { -4 })
                .clamp(0, self.pages.len().saturating_sub(1) as isize)
                    as usize;
            }
        }
        self.redraw = true;
    }
    fn queue_lights(&mut self) -> Result<()> {
        if let Some(Dialog::Settings(s)) = &mut self.dialog {
            if s.loaded && s.before.cool.is_some() && s.before.warm.is_some() {
                self.light_pending = Some((s.brightness, s.warmth));
                s.light_busy = true;
                s.error = None;
                self.notice.clear();
                self.send_pending_lights()?;
            }
        }
        Ok(())
    }
    fn send_pending_lights(&mut self) -> Result<()> {
        if !self.light_in_flight {
            if let Some((b, w)) = self.light_pending.take() {
                self.jobs.send(Work::Lights(b, w))?;
                self.light_in_flight = true;
            }
        }
        Ok(())
    }
    fn live_settings(&mut self) -> Result<()> {
        if let Some(Dialog::Settings(s)) = &mut self.dialog {
            if !s.loaded {
                return Ok(());
            }
            s.error = None;
            if self.prefs != s.prefs {
                // One SQLite transaction saves the whole preference change.
                // In-memory controls only change after persistence succeeds.
                s.prefs.save(&self.store)?;
                self.prefs = s.prefs.clone();
                if self.fullscreen != self.prefs.start_fullscreen {
                    self.fullscreen = self.prefs.start_fullscreen;
                    self.fullscreen_changed = true;
                }
            }
            let requested = s.hardware.display_only();
            if self.display_in_flight || requested != s.before.display_only() {
                self.display_pending = Some(requested);
                s.display_busy = true;
                self.notice.clear();
                self.send_pending_display()?;
            }
        }
        Ok(())
    }
    fn send_pending_display(&mut self) -> Result<()> {
        if !self.display_in_flight {
            if let Some(h) = self.display_pending.take() {
                self.jobs.send(Work::Display(h))?;
                self.display_in_flight = true;
            }
        }
        Ok(())
    }
    fn finish_settings(&mut self) -> Result<()> {
        if !self.light_in_flight && !self.display_in_flight {
            if self.quit_after_settings {
                self.quit_after_settings = false;
                self.action(Action::Quit)?;
            }
            if self.close_after_settings {
                self.close_after_settings = false;
                if matches!(self.dialog, Some(Dialog::Settings(_))) {
                    self.dialog = None;
                }
            }
        }
        Ok(())
    }
    fn adjust(&mut self, key: &str, delta: f64) {
        if let Some(Dialog::Settings(s)) = &mut self.dialog {
            if !s.loaded {
                return;
            }
            match key {
                "brightness" if s.before.cool.is_some() => {
                    s.brightness = (s.brightness + delta).clamp(0., 100.);
                    s.hardware.lights(s.brightness, s.warmth);
                }
                "warmth" if s.before.warm.is_some() => {
                    s.warmth = (s.warmth + delta).clamp(0., 100.);
                    s.hardware.lights(s.brightness, s.warmth);
                }
                "contrast" => {
                    s.prefs.page_contrast = (s.prefs.page_contrast + delta).clamp(1., 2.5)
                }
                "gamma" => s.prefs.page_gamma = (s.prefs.page_gamma + delta).clamp(0.6, 1.8),
                "delay" => {
                    s.hardware.delay = s
                        .hardware
                        .delay
                        .map(|v| (v as f64 + delta).clamp(25., 400.) as u32)
                }
                _ => (),
            }
        }
    }
    fn choice(&mut self, key: &str) {
        if let Some(Dialog::Settings(s)) = &mut self.dialog {
            if !s.loaded {
                return;
            }
            let p = &mut s.prefs;
            match key {
                "font" => {
                    p.ui_font_size = if p.ui_font_size >= 18. {
                        14.
                    } else {
                        p.ui_font_size + 2.
                    }
                }
                "fullscreen" => p.start_fullscreen = !p.start_fullscreen,
                "cursor" => p.hide_cursor = !p.hide_cursor,
                "finger" => p.finger_draw = !p.finger_draw,
                "keyboard" => p.keyboard_auto = !p.keyboard_auto,
                "layout" => {
                    p.keyboard_layout = match p.keyboard_layout.as_str() {
                        "QWERTY" => "QWERTZ",
                        "QWERTZ" => "AZERTY",
                        _ => "QWERTY",
                    }
                    .into()
                }
                "keys" => {
                    p.keyboard_key_height = if p.keyboard_key_height == 38. {
                        44.
                    } else {
                        38.
                    }
                }
                "driver" => s.hardware.driver = s.hardware.driver.map(|v| 1 - v),
                "dither" => s.hardware.dither = s.hardware.dither.map(|v| (v + 1) % 3),
                "periodic" => {
                    p.refresh_pages = match p.refresh_pages {
                        0 => 1,
                        1 => 5,
                        5 => 10,
                        10 => 20,
                        _ => 0,
                    }
                }
                "depth" | "conversion" | "cleanup" => {
                    let mut parts: Vec<String> =
                        p.display_hint.split('|').map(str::to_owned).collect();
                    if parts.len() != 3 {
                        parts = vec!["Y1".into(), "T".into(), "R".into()];
                    }
                    match key {
                        "depth" => {
                            parts[0] = match parts[0].as_str() {
                                "Y1" => "Y2",
                                "Y2" => "Y4",
                                _ => "Y1",
                            }
                            .into()
                        }
                        "conversion" => parts[1] = if parts[1] == "T" { "D" } else { "T" }.into(),
                        _ => parts[2] = if parts[2] == "R" { "r" } else { "R" }.into(),
                    }
                    p.display_hint = parts.join("|");
                    s.hardware.hint = s.before.hint.as_ref().map(|_| p.display_hint.clone());
                }
                _ => (),
            }
        }
    }
    pub fn key(&mut self, name: &str, text: &str, ctrl: bool, shift: bool) {
        if name == "F11" {
            self.act(Action::Fullscreen);
            return;
        }
        if self.dialog.is_some() {
            match name {
                "Escape" => self.act(Action::Close),
                "BackSpace" => self.act(Action::Backspace),
                "Left" => self.act(Action::Left),
                "Right" => self.act(Action::Right),
                "Return" => self.act(Action::Key("Enter".into())),
                _ => {
                    if ctrl {
                        match text.to_lowercase().as_str() {
                            "a" => self.act(Action::SelectAll),
                            "c" => self.act(Action::Copy),
                            "v" => self.act(Action::Paste),
                            _ => (),
                        }
                    } else if !text.is_empty() && !text.chars().any(char::is_control) {
                        self.act(Action::Key(text.into()));
                    }
                }
            }
            return;
        }
        if ctrl {
            match text.to_lowercase().as_str() {
                "n" => self.act(Action::New("notebook".into())),
                "o" => self.act(Action::Import),
                "z" => self.act(if shift { Action::Redo } else { Action::Undo }),
                "q" => self.act(Action::Quit),
                _ => (),
            }
        } else {
            match name {
                "Escape" => self.act(Action::Tools),
                "Page_Up" => self.act(Action::Previous),
                "Page_Down" => self.act(Action::Next),
                "Delete" => self.act(Action::DeleteSelection),
                _ => (),
            }
        }
    }
    pub fn live_stroke(&self) -> Option<&Stroke> {
        match &self.gesture {
            Some(Gesture::Ink(s)) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod keyboard_feedback_tests {
    use super::*;
    use crate::render::{BLACK, WHITE};

    #[test]
    fn touch_feedback_preserves_typing_expires_independently_and_only_repaints_keys() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = App::new(dir.path().into()).unwrap();
        for (w, h, finger) in [(702, 936, true), (936, 702, false)] {
            a.resize(w, h, 1);
            a.act(Action::Search);
            a.render();
            let key = |a: &App, s: &str| {
                a.hits
                    .iter()
                    .find(|(_, action)| *action == Action::Key(s.into()))
                    .unwrap()
                    .0
            };
            let q = key(&a, "q");
            let e = key(&a, "e");
            let pixel = |a: &App, b: Box2| {
                a.surface.pixels[(b.y as usize + 8) * a.surface.width + b.x as usize + 8]
            };
            let hits = a.hits.clone();
            a.damage = None;
            a.motion(q.x + 10., q.y + 10., 0.);
            assert_eq!(pixel(&a, q), WHITE, "Hover does not highlight");
            for b in [q, e] {
                a.down(b.x + 10., b.y + 10., 0.5, finger);
                assert_eq!(pixel(&a, b), BLACK, "Feedback starts before release");
                assert_eq!(a.damage, Some(a.surface.rect(b)));
                assert_eq!(a.hits, hits, "Partial repaint must not change hit regions");
                assert!(!a.redraw, "Feedback must not redraw the page");
                a.up(b.x + 10., b.y + 10.);
                a.render();
                a.damage = None;
            }
            let Some(Dialog::Input(input)) = &a.dialog else {
                panic!("Input closed");
            };
            assert_eq!(input.value, "qe");
            let deadline = a.key_feedback[0].1;
            a.key_feedback[1].1 = deadline + Duration::from_millis(100);
            a.expire_key_feedback(deadline - Duration::from_millis(1));
            assert_eq!(pixel(&a, q), BLACK);
            a.expire_key_feedback(deadline);
            assert_eq!(pixel(&a, q), WHITE);
            assert_eq!(pixel(&a, e), BLACK, "Later presses keep their own duration");
            assert_eq!(a.damage, Some(a.surface.rect(q)));
            a.damage = None;
            a.expire_key_feedback(deadline + Duration::from_millis(100));
            assert_eq!(pixel(&a, e), WHITE);
            assert_eq!(a.damage, Some(a.surface.rect(e)));
            assert_eq!(a.hits, hits);
            a.down(q.x + 10., q.y + 10., 0.5, finger);
            a.finish();
            a.act(Action::KeyMode(1));
            assert!(
                a.key_feedback.is_empty(),
                "Do not highlight different keys after a layout change"
            );
            a.act(Action::Close);
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    // Replace the hardware worker with controlled messages: these tests never
    // write the host's lights or display settings.
    fn fixture() -> (
        tempfile::TempDir,
        App,
        mpsc::Receiver<Work>,
        mpsc::Sender<Done>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(dir.path().into()).unwrap();
        let (jobs, requests) = mpsc::channel();
        let (results, done) = mpsc::channel();
        app.jobs = jobs;
        app.done = done;
        app.act(Action::Settings);
        assert!(matches!(requests.try_recv(), Ok(Work::Hardware)));
        results
            .send(Done::Hardware(Hardware {
                cool: Some(51),
                warm: Some(51),
                cool_max: 255,
                warm_max: 255,
                driver: Some(1),
                dither: Some(2),
                delay: Some(200),
                hint: Some("Y1|T|R".into()),
            }))
            .unwrap();
        app.tick();
        (dir, app, requests, results)
    }

    fn light_result(b: f64, w: f64) -> Hardware {
        let mut h = Hardware {
            cool_max: 255,
            warm_max: 255,
            ..Hardware::default()
        };
        h.lights(b, w);
        h
    }

    fn settings(app: &App) -> &Settings {
        let Some(Dialog::Settings(s)) = &app.dialog else {
            panic!("Settings closed unexpectedly")
        };
        s
    }

    fn control(app: &mut App, action: Action) -> Box2 {
        app.render();
        app.hits.iter().find(|(_, a)| *a == action).unwrap().0
    }

    #[test]
    fn sliders_capture_taps_and_drags_save_live_and_ignore_hover_and_cancelled_input() {
        let (_dir, mut app, requests, _results) = fixture();
        for (w, h, finger) in [(702, 936, false), (936, 702, true)] {
            app.resize(w, h, 2);
            app.act(Action::SettingTab(1));
            let b = control(&mut app, Action::Slider(SliderSetting::Contrast));
            let (x, y) = (b.x + b.w / 2., b.y + 31.);
            let before = app.prefs.clone();
            app.motion(x, y, 0.);
            assert_eq!(app.prefs, before, "Hover must not change a slider");
            app.down(x, y, 0.5, finger);
            assert_eq!(app.prefs.page_contrast, 1.75);
            assert_eq!(Prefs::load(&app.store), app.prefs, "Saves before release");
            app.render(); // Hit rectangles are rebuilt while a drag is captured.
            app.motion(b.x + b.w + 100., y + 20., 0.5);
            assert_eq!(app.prefs.page_contrast, 2.5);
            app.motion(b.x - 100., y - 20., 0.5);
            assert_eq!(app.prefs.page_contrast, 1.);
            assert_eq!(settings(&app).scroll, 0., "A slider drag must not scroll");
            app.up(x, y);
            assert_eq!(app.prefs.page_contrast, 1.75);
            app.motion(b.x, y, 0.);
            assert_eq!(app.prefs.page_contrast, 1.75);
            app.down(x, y, 0.5, finger);
            app.finish(); // Touch cancel, pointer leave or rotation.
            app.motion(b.x + b.w, y, 0.5);
            app.up(b.x + b.w, y);
            assert_eq!(app.prefs.page_contrast, 1.75);
            assert_eq!(Prefs::load(&app.store), app.prefs);
        }
        assert!(
            requests.try_recv().is_err(),
            "Local sliders never write hardware"
        );
    }

    #[test]
    fn switches_support_tap_and_swipe_without_hover_or_vertical_drag_activation() {
        let (_dir, mut app, requests, _results) = fixture();
        app.resize(702, 936, 2);
        app.act(Action::SettingTab(1));
        let b = control(&mut app, Action::Switch(SwitchSetting::Finger));
        let (left, right, y) = (b.x + 12., b.x + b.w - 12., b.y + b.h / 2.);
        app.motion(right, y, 0.);
        assert!(!app.prefs.finger_draw);
        app.down(left, y, 0.5, true);
        app.up(left, y);
        assert!(app.prefs.finger_draw);
        app.down(right, y, 0.5, true);
        app.motion(left, y, 0.5);
        app.up(left, y);
        assert!(!app.prefs.finger_draw);
        app.down(left, y, 0.5, true);
        app.motion(right, y, 0.5);
        app.up(right, y);
        assert!(app.prefs.finger_draw);
        app.down(right, y, 0.5, true);
        app.motion(left, y + 80., 0.5);
        app.up(left, y + 80.);
        assert!(app.prefs.finger_draw);
        app.down(right, y, 0.5, true);
        app.finish();
        app.up(right, y);
        assert!(app.prefs.finger_draw);
        assert_eq!(Prefs::load(&app.store), app.prefs);
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn presets_follow_their_dependencies_and_survive_quantized_readback() {
        let (_dir, mut app, requests, results) = fixture();
        for (b, w) in [(0., 50.), (20., 85.), (35., 50.), (75., 0.)] {
            app.act(Action::LightPreset(b, w));
            assert!(settings(&app).light_preset_active(b, w));
            assert!(matches!(requests.try_recv(), Ok(Work::Lights(_, _))));
            results
                .send(Done::Lights(light_result(b, w), None))
                .unwrap();
            app.tick();
            assert!(settings(&app).light_preset_active(b, w));
            app.act(Action::SetSlider(SliderSetting::Contrast, 1.25));
            assert!(
                settings(&app).light_preset_active(b, w),
                "Unrelated control"
            );
            for kind in [SliderSetting::Warmth, SliderSetting::Brightness] {
                let original = kind.value(settings(&app));
                app.act(Action::SetSlider(kind, original + 1.));
                assert!(!settings(&app).light_preset_active(b, w));
                assert!(matches!(requests.try_recv(), Ok(Work::Lights(_, _))));
                app.act(Action::SetSlider(kind, original));
                assert!(settings(&app).light_preset_active(b, w));
                results
                    .send(Done::Lights(light_result(b, w), None))
                    .unwrap();
                app.tick();
                assert!(matches!(requests.try_recv(), Ok(Work::Lights(_, _))));
                results
                    .send(Done::Lights(light_result(b, w), None))
                    .unwrap();
                app.tick();
            }
        }
        for name in ["Writing", "Balanced", "Reading", "Images"] {
            app.act(Action::Profile(name.into()));
            assert!(settings(&app).profile_active(name));
            // Complete the hardware write, then verify that no highlight was latched.
            let Work::Display(h) = requests.try_recv().unwrap() else {
                panic!()
            };
            results.send(Done::Display(h, None)).unwrap();
            app.tick();
            assert!(settings(&app).profile_active(name));
            for key in ["depth", "conversion", "cleanup", "driver", "dither"] {
                app.act(Action::Choice(key.into()));
                assert!(!settings(&app).profile_active(name), "{name}: {key}");
                let Work::Display(h) = requests.try_recv().unwrap() else {
                    panic!()
                };
                results.send(Done::Display(h, None)).unwrap();
                app.tick();
                app.act(Action::Profile(name.into()));
                let Work::Display(h) = requests.try_recv().unwrap() else {
                    panic!()
                };
                results.send(Done::Display(h, None)).unwrap();
                app.tick();
                assert!(settings(&app).profile_active(name));
            }
            let delay = SliderSetting::Delay.value(settings(&app));
            app.act(Action::SetSlider(SliderSetting::Delay, delay + 25.));
            assert!(!settings(&app).profile_active(name));
            let Work::Display(h) = requests.try_recv().unwrap() else {
                panic!()
            };
            results.send(Done::Display(h, None)).unwrap();
            app.tick();
        }
        // Opening settings reconstructs a preset from integer hardware readback.
        app.act(Action::Close);
        app.act(Action::Settings);
        assert!(matches!(requests.try_recv(), Ok(Work::Hardware)));
        results
            .send(Done::Hardware(light_result(35., 50.)))
            .unwrap();
        app.tick();
        assert!(settings(&app).light_preset_active(35., 50.));
        assert!(!settings(&app).profile_active("Writing"));
    }

    #[test]
    fn live_lights_and_display_updates_never_overwrite_each_other() {
        let (_dir, mut app, requests, results) = fixture();
        app.act(Action::Setting("contrast".into(), 0.1));
        app.act(Action::Choice("driver".into()));
        let Work::Display(display) = requests.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(display.cool, None);
        assert_eq!(display.warm, None);
        app.act(Action::Setting("brightness".into(), 5.));
        assert!(matches!(requests.try_recv(), Ok(Work::Lights(25., 50.))));
        app.act(Action::Setting("warmth".into(), 5.));
        app.act(Action::Setting("brightness".into(), 5.));
        assert!(requests.try_recv().is_err());
        results
            .send(Done::Lights(light_result(25., 50.), None))
            .unwrap();
        app.tick();
        assert!(matches!(requests.try_recv(), Ok(Work::Lights(30., 55.))));
        let latest = light_result(30., 55.);
        results.send(Done::Lights(latest.clone(), None)).unwrap();
        app.tick();
        results.send(Done::Display(display, None)).unwrap();
        app.tick();
        let Some(Dialog::Settings(s)) = &app.dialog else {
            panic!()
        };
        assert_eq!(
            (s.hardware.cool, s.hardware.warm),
            (latest.cool, latest.warm)
        );
        assert_eq!((s.before.cool, s.before.warm), (latest.cool, latest.warm));
        assert_eq!((s.hardware.driver, s.before.driver), (Some(0), Some(0)));
        assert!((app.prefs.page_contrast - 1.1).abs() < 1e-6);
        assert_eq!(Prefs::load(&app.store), app.prefs);
    }

    #[test]
    fn all_local_preferences_and_resets_save_without_confirmation() {
        let (_dir, mut app, requests, _results) = fixture();
        let mut actions = vec![
            Action::Setting("contrast".into(), 0.1),
            Action::Setting("gamma".into(), 0.05),
        ];
        actions.extend(
            [
                "font",
                "fullscreen",
                "cursor",
                "finger",
                "keyboard",
                "layout",
                "keys",
                "periodic",
            ]
            .map(|key| Action::Choice(key.into())),
        );
        for action in actions {
            let before = app.prefs.clone();
            app.act(action);
            assert_ne!(app.prefs, before);
            assert_eq!(Prefs::load(&app.store), app.prefs);
            assert_eq!(app.fullscreen, app.prefs.start_fullscreen);
        }
        assert!(requests.try_recv().is_err());
        app.act(Action::SettingTab(1));
        app.act(Action::ResetSettings);
        assert_eq!(app.prefs.page_contrast, 1.);
        assert_eq!(app.prefs.ui_font_size, 14.);
        assert!(app.fullscreen);
        app.act(Action::SettingTab(3));
        app.act(Action::ResetSettings);
        assert_eq!(app.prefs.keyboard_layout, "QWERTY");
        assert_eq!(app.prefs.keyboard_key_height, 38.);
        assert_eq!(Prefs::load(&app.store), app.prefs);
    }

    #[test]
    fn rapid_display_changes_keep_latest_values_and_done_waits_for_both_workers() {
        let (_dir, mut app, requests, results) = fixture();
        app.act(Action::Choice("driver".into()));
        let Work::Display(first) = requests.try_recv().unwrap() else {
            panic!()
        };
        app.act(Action::Choice("driver".into())); // Return to the original while the first write is in flight.
        app.act(Action::Choice("dither".into()));
        app.act(Action::Setting("delay".into(), 25.));
        app.act(Action::Choice("depth".into()));
        app.act(Action::Choice("conversion".into()));
        app.act(Action::Choice("cleanup".into()));
        assert!(requests.try_recv().is_err());
        app.act(Action::LightPreset(35., 50.));
        assert!(matches!(requests.try_recv(), Ok(Work::Lights(35., 50.))));
        app.act(Action::Close);
        results.send(Done::Display(first, None)).unwrap();
        app.tick();
        let Work::Display(latest) = requests.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(latest.driver, Some(1));
        assert_eq!(latest.dither, Some(0));
        assert_eq!(latest.delay, Some(225));
        assert_eq!(latest.hint.as_deref(), Some("Y2|D|r"));
        results.send(Done::Display(latest, None)).unwrap();
        app.tick();
        assert!(app.dialog.is_some());
        results
            .send(Done::Lights(light_result(35., 50.), None))
            .unwrap();
        app.tick();
        assert!(app.dialog.is_none());
        assert_eq!(Prefs::load(&app.store).display_hint, "Y2|D|r");
    }

    #[test]
    fn failed_display_update_restores_its_preferences_without_losing_newer_edits() {
        let (_dir, mut app, requests, results) = fixture();
        let Some(Dialog::Settings(s)) = &app.dialog else {
            panic!()
        };
        let before = s.before.display_only();
        app.act(Action::Profile("Images".into()));
        assert!(matches!(requests.try_recv(), Ok(Work::Display(_))));
        app.act(Action::Choice("font".into()));
        app.act(Action::LightPreset(35., 50.));
        assert!(matches!(requests.try_recv(), Ok(Work::Lights(35., 50.))));
        app.act(Action::Quit);
        results
            .send(Done::Display(
                before.clone(),
                Some("driver write failed; restored".into()),
            ))
            .unwrap();
        results
            .send(Done::Lights(light_result(35., 50.), None))
            .unwrap();
        app.tick();
        let Some(Dialog::Settings(s)) = &app.dialog else {
            panic!()
        };
        assert_eq!(s.hardware.display_only(), before);
        assert!(s.error.as_ref().unwrap().contains("driver write failed"));
        assert_eq!(app.prefs.ui_font_size, 16.);
        assert_eq!(app.prefs.display_hint, "Y1|T|R");
        assert_eq!(Prefs::load(&app.store), app.prefs);
        assert!(app.running);
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn done_waits_for_latest_light_and_failed_writes_remain_visible() {
        let (_dir, mut app, requests, results) = fixture();
        app.act(Action::LightPreset(35., 50.));
        assert!(matches!(requests.try_recv(), Ok(Work::Lights(35., 50.))));
        app.act(Action::Close);
        assert!(app.dialog.is_some());
        results
            .send(Done::Lights(light_result(35., 50.), None))
            .unwrap();
        app.tick();
        assert!(app.dialog.is_none());
        app.act(Action::Settings);
        assert!(matches!(requests.try_recv(), Ok(Work::Hardware)));
        results
            .send(Done::Hardware(light_result(35., 50.)))
            .unwrap();
        app.tick();
        app.act(Action::LightPreset(75., 0.));
        assert!(matches!(requests.try_recv(), Ok(Work::Lights(75., 0.))));
        app.act(Action::LightPreset(20., 85.));
        app.act(Action::Close);
        results
            .send(Done::Lights(
                light_result(35., 50.),
                Some("write failed; restored".into()),
            ))
            .unwrap();
        app.tick();
        assert!(requests.try_recv().is_err());
        let Some(Dialog::Settings(s)) = &app.dialog else {
            panic!()
        };
        assert!(!s.light_busy);
        assert!((s.brightness - 35.).abs() < 0.5);
        assert!(app.notice.contains("write failed"));
    }
}
