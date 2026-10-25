//! The Python 0.2 library schema is deliberately retained. No destructive migration.
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

pub const PAGE_W: f64 = 760.;
pub const PAGE_H: f64 = 1040.;
pub const TEMPLATES: [&str; 6] = ["Lined", "Blank", "Dots", "Grid", "Checklist", "Music"];
pub const PENS: [&str; 6] = [
    "Ballpoint",
    "Fineliner",
    "Marker",
    "Pencil",
    "Mechanical",
    "Calligraphy",
];
pub const BRUSHES: [&str; 4] = ["Round", "Flat", "Soft", "Dry"];
pub const BRUSH_SIZES: [(f64, &str); 4] =
    [(3., "Fine"), (8., "Medium"), (16., "Broad"), (28., "Wide")];
// Keep the historical highlighter width multiplier for saved document compatibility.
pub const HIGHLIGHTER_SIZES: [(f64, &str); 4] =
    [(2., "Fine"), (4., "Medium"), (6., "Broad"), (9., "Wide")];
pub const PEN_SIZES: [(f64, &str); 3] = [(1.4, "Fine"), (2.4, "Medium"), (4., "Bold")];
pub type Point = [f64; 3];
fn pen() -> String {
    "Fineliner".into()
}
fn tool() -> String {
    "pen".into()
}
fn width() -> f64 {
    2.4
}
fn color() -> f64 {
    0.07
}
fn size() -> f64 {
    25.
}
fn font() -> String {
    "Sans".into()
}
fn one() -> u32 {
    1
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    #[serde(default = "tool")]
    pub tool: String,
    #[serde(default = "pen")]
    pub pen: String,
    #[serde(default = "width")]
    pub width: f64,
    #[serde(default = "color")]
    pub color: f64,
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for Stroke {
    fn default() -> Self {
        Self {
            tool: tool(),
            pen: pen(),
            width: width(),
            color: color(),
            points: vec![],
            extra: BTreeMap::new(),
        }
    }
}
impl Stroke {
    pub fn brush() -> Self {
        Self {
            tool: "brush".into(),
            pen: "Round".into(),
            width: 8.,
            ..Self::default()
        }
    }
    pub fn highlighter() -> Self {
        Self {
            tool: "highlighter".into(),
            width: 4.,
            ..Self::default()
        }
    }
    /// Conservative geometric radius for selection and erasing, in page units.
    pub fn max_radius(&self) -> f64 {
        if self.tool == "highlighter" {
            (self.width * 7.).max(14.) / 2.
        } else if self.tool == "brush" {
            self.width * 1.75 / 2.
        } else {
            self.width
                * match self.pen.as_str() {
                    "Mechanical" => 0.35,
                    "Marker" => 1.25,
                    "Ballpoint" => 0.75,
                    "Pencil" => 0.7,
                    "Calligraphy" => 1.875,
                    _ => 0.5,
                }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
    pub x: f64,
    pub y: f64,
    #[serde(default = "size")]
    pub size: f64,
    #[serde(default = "color")]
    pub color: f64,
    #[serde(default = "font")]
    pub font: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    #[serde(default = "one")]
    pub version: u32,
    pub template: String,
    #[serde(default)]
    pub strokes: Vec<Stroke>,
    #[serde(default)]
    pub texts: Vec<Text>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Payload {
    pub fn blank(template: &str) -> Self {
        Self {
            version: 1,
            template: template.into(),
            strokes: vec![],
            texts: vec![],
            body: None,
            heading: None,
            extra: BTreeMap::new(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        for s in &self.strokes {
            if !s.width.is_finite()
                || !(0.1..=100.).contains(&s.width)
                || !s.color.is_finite()
                || s.points.iter().flatten().any(|v| !v.is_finite())
            {
                bail!("Invalid ink coordinates");
            }
        }
        for t in &self.texts {
            if !t.x.is_finite()
                || !t.y.is_finite()
                || !t.size.is_finite()
                || !(1. ..=300.).contains(&t.size)
            {
                bail!("Invalid text coordinates");
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Page {
    pub id: String,
    pub document: String,
    pub position: usize,
    pub payload: Payload,
    pub bookmarked: bool,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub parent: Option<String>,
    pub source: Option<String>,
    pub current_page: usize,
    pub trashed: bool,
}
fn document(row: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get("id")?,
        title: row.get("title")?,
        kind: row.get("kind")?,
        parent: row.get("parent")?,
        source: row.get("source")?,
        current_page: row.get::<_, u32>("current_page")? as usize,
        trashed: row.get("trashed")?,
    })
}
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
pub fn identifier() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
pub fn default_root() -> PathBuf {
    std::env::var_os("PINEPAPER_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
                })
                .join("pinepaper")
        })
}
fn connect(path: &Path) -> Result<Connection> {
    let db = Connection::open(path)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
    Ok(db)
}
pub struct Store {
    pub root: PathBuf,
    pub db: Connection,
}
impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("files"))?;
        fs::create_dir_all(root.join("exports"))?;
        let db = connect(&root.join("library.sqlite3"))?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS documents (id TEXT PRIMARY KEY,title TEXT NOT NULL,kind TEXT NOT NULL,parent TEXT REFERENCES documents(id),source TEXT,created REAL NOT NULL,updated REAL NOT NULL,opened REAL NOT NULL DEFAULT 0,current_page INTEGER NOT NULL DEFAULT 0,trashed INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE IF NOT EXISTS pages (id TEXT PRIMARY KEY,document TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,position INTEGER NOT NULL,payload TEXT NOT NULL,bookmarked INTEGER NOT NULL DEFAULT 0,tags TEXT NOT NULL DEFAULT '[]');
        CREATE INDEX IF NOT EXISTS pages_document ON pages(document,position);
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
        Ok(Self { root, db })
    }
    /// Atomically persist a materialized shared document in this client's library.
    pub fn save_shared(&self, doc: &Document, pages: &[Page]) -> Result<()> {
        anyhow::ensure!(
            !pages.is_empty() && pages.len() <= 2048,
            "Invalid shared page count"
        );
        let tx = self.db.unchecked_transaction()?;
        tx.execute("INSERT INTO documents(id,title,kind,parent,source,created,updated,opened,current_page,trashed) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET title=excluded.title,updated=excluded.updated,current_page=excluded.current_page", params![doc.id,doc.title,doc.kind,doc.parent,doc.source,now(),now(),now(),doc.current_page as i64,doc.trashed])?;
        tx.execute("DELETE FROM pages WHERE document=?", [&doc.id])?;
        for page in pages {
            anyhow::ensure!(page.document == doc.id, "Page belongs to another document");
            page.payload.validate()?;
            tx.execute("INSERT INTO pages(id,document,position,payload,bookmarked,tags) VALUES(?,?,?,?,?,?)",params![page.id,doc.id,page.position as i64,serde_json::to_string(&page.payload)?,page.bookmarked,serde_json::to_string(&page.tags)?])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn setting<T: serde::de::DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.db
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(default)
    }
    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO settings VALUES(?,?)",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
    pub fn document(&self, id: &str) -> Result<Document> {
        Ok(self
            .db
            .query_row("SELECT * FROM documents WHERE id=?", [id], document)?)
    }
    pub fn documents(
        &self,
        parent: Option<&str>,
        kind: &str,
        query: &str,
        sort: usize,
        trash: bool,
    ) -> Result<Vec<Document>> {
        let order = [
            "opened DESC,updated DESC",
            "title COLLATE NOCASE",
            "created DESC",
            "updated DESC",
            "kind,title COLLATE NOCASE",
        ][sort.min(4)];
        let mut stmt = self.db.prepare(&format!("SELECT * FROM documents WHERE trashed=? AND (? OR ?!='' OR ?!='' OR parent IS ?) AND (?='' OR kind=?) ORDER BY {order}"))?;
        let rows = stmt.query_map(
            params![trash, trash, kind, query, parent, kind, kind],
            document,
        )?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|d| d.title.to_lowercase().contains(&query.to_lowercase()))
            .collect())
    }
    pub fn folders(&self) -> Result<Vec<Document>> {
        Ok(self.db.prepare("SELECT * FROM documents WHERE kind='folder' AND trashed=0 ORDER BY title COLLATE NOCASE")?.query_map([], document)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn create(
        &mut self,
        title: &str,
        kind: &str,
        parent: Option<&str>,
        pages: Vec<Payload>,
        source: Option<&str>,
    ) -> Result<String> {
        let title: String = title.trim().chars().take(240).collect();
        if title.is_empty() {
            bail!("Please enter a name");
        }
        if !["notebook", "book", "pdf", "folder"].contains(&kind) {
            bail!("Unknown document kind");
        }
        if kind != "folder" && pages.is_empty() {
            bail!("A document needs at least one page");
        }
        let id = identifier();
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO documents(id,title,kind,parent,source,created,updated) VALUES(?,?,?,?,?,?,?)", params![id, title, kind, parent, source, now(), now()])?;
        for (i, p) in pages.iter().enumerate() {
            p.validate()?;
            tx.execute(
                "INSERT INTO pages(id,document,position,payload) VALUES(?,?,?,?)",
                params![identifier(), id, i as i64, serde_json::to_string(p)?],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }
    pub fn move_pages_to_notebook(
        &mut self,
        source: &str,
        selected: &[String],
        title: &str,
    ) -> Result<String> {
        let doc = self.document(source)?;
        let title: String = title.trim().chars().take(240).collect();
        let pages = self.pages(source)?;
        let chosen: std::collections::BTreeSet<_> = selected.iter().collect();
        anyhow::ensure!(
            doc.kind == "notebook" && !doc.trashed,
            "Open an active notebook first"
        );
        anyhow::ensure!(!title.is_empty(), "Please enter a name");
        anyhow::ensure!(
            !chosen.is_empty()
                && chosen.len() == selected.len()
                && chosen.iter().all(|id| pages.iter().any(|p| &p.id == *id)),
            "Some selected pages are no longer available"
        );
        let id = identifier();
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO documents(id,title,kind,parent,created,updated) VALUES(?,?,'notebook',?,?,?)",
            params![id,title,doc.parent,now(),now()])?;
        let (mut moved, mut remaining) = (0, 0);
        for page in &pages {
            let (target, position) = if chosen.contains(&page.id) {
                let position = moved;
                moved += 1;
                (&id, position)
            } else {
                let position = remaining;
                remaining += 1;
                (&doc.id, position)
            };
            tx.execute(
                "UPDATE pages SET document=?,position=? WHERE id=?",
                params![target, position as i64, page.id],
            )?;
        }
        if remaining == 0 {
            tx.execute(
                "INSERT INTO pages(id,document,position,payload) VALUES(?,?,0,?)",
                params![
                    identifier(),
                    source,
                    serde_json::to_string(&Payload::blank("Blank"))?
                ],
            )?;
            remaining = 1;
        }
        tx.execute(
            "UPDATE documents SET current_page=?,updated=? WHERE id=?",
            params![doc.current_page.min(remaining - 1) as i64, now(), source],
        )?;
        tx.commit()?;
        Ok(id)
    }
    pub fn pages(&self, id: &str) -> Result<Vec<Page>> {
        let mut stmt = self.db.prepare("SELECT id,document,position,payload,bookmarked,tags FROM pages WHERE document=? ORDER BY position")?;
        let rows = stmt.query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)? as usize,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, document, position, payload, bookmarked, tags) = row?;
            let payload: Payload = serde_json::from_str(&payload)
                .context("Could not read page; original data was not changed")?;
            payload.validate()?;
            result.push(Page {
                id,
                document,
                position,
                payload,
                bookmarked,
                tags: serde_json::from_str(&tags)?,
            });
        }
        Ok(result)
    }
    pub fn save(&mut self, id: &str, p: &Payload) -> Result<()> {
        p.validate()?;
        let data = serde_json::to_string(p)?;
        let tx = self.db.transaction()?;
        if tx.execute("UPDATE pages SET payload=? WHERE id=?", params![data, id])? != 1 {
            bail!("Page no longer exists");
        }
        tx.execute(
            "UPDATE documents SET updated=? WHERE id=(SELECT document FROM pages WHERE id=?)",
            params![now(), id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn add_page(&mut self, doc: &str, after: usize, template: &str) -> Result<()> {
        if self.document(doc)?.kind != "notebook" {
            bail!("Only notebooks can add pages");
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE pages SET position=position+1 WHERE document=? AND position>?",
            params![doc, after as i64],
        )?;
        tx.execute(
            "INSERT INTO pages(id,document,position,payload) VALUES(?,?,?,?)",
            params![
                identifier(),
                doc,
                (after + 1) as i64,
                serde_json::to_string(&Payload::blank(template))?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn delete_page(&mut self, p: &Page) -> Result<()> {
        if self.document(&p.document)?.kind != "notebook" || self.pages(&p.document)?.len() < 2 {
            bail!("Keep at least one page; imported pages cannot be deleted");
        }
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM pages WHERE id=?", [&p.id])?;
        tx.execute(
            "UPDATE pages SET position=position-1 WHERE document=? AND position>?",
            params![p.document, p.position as i64],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn meta(&self, p: &Page) -> Result<()> {
        self.db.execute(
            "UPDATE pages SET bookmarked=?,tags=? WHERE id=?",
            params![p.bookmarked, serde_json::to_string(&p.tags)?, p.id],
        )?;
        Ok(())
    }
    pub fn open_at(&self, id: &str, page: usize) -> Result<()> {
        self.db.execute(
            "UPDATE documents SET opened=?,current_page=? WHERE id=?",
            params![now(), page as i64, id],
        )?;
        Ok(())
    }
    pub fn rename(&self, id: &str, title: &str) -> Result<()> {
        let title: String = title.trim().chars().take(240).collect();
        if title.is_empty() {
            bail!("Name cannot be empty");
        }
        self.db.execute(
            "UPDATE documents SET title=?,updated=? WHERE id=?",
            params![title, now(), id],
        )?;
        Ok(())
    }
    pub fn move_to(&self, id: &str, parent: Option<&str>) -> Result<()> {
        let mut cursor = parent.map(str::to_owned);
        let mut seen = std::collections::HashSet::new();
        while let Some(ref p) = cursor {
            if p == id || !seen.insert(p.clone()) {
                bail!("A folder cannot contain itself");
            }
            let d = self.document(p)?;
            if d.kind != "folder" || d.trashed {
                bail!("Choose an available folder");
            }
            cursor = d.parent;
        }
        self.db.execute(
            "UPDATE documents SET parent=? WHERE id=?",
            params![parent, id],
        )?;
        Ok(())
    }
    pub fn trash(&self, id: &str, trash: bool) -> Result<()> {
        self.db.execute(
            "UPDATE documents SET trashed=? WHERE id=?",
            params![trash, id],
        )?;
        if !trash {
            if let Some(p) = self.document(id)?.parent {
                if self.document(&p)?.trashed {
                    self.move_to(id, None)?;
                }
            }
        }
        Ok(())
    }
    pub fn source_path(&self, d: &Document) -> Result<PathBuf> {
        let source = d.source.as_deref().context("Document has no source file")?;
        let path = Path::new(source);
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            bail!("Invalid source path");
        }
        Ok(self.root.join(path))
    }
    pub fn copy_source(&self, path: &Path) -> Result<String> {
        let name = format!(
            "files/{}.{}",
            identifier(),
            path.extension().and_then(|s| s.to_str()).unwrap_or("bin")
        );
        let mut temp = tempfile::NamedTempFile::new_in(self.root.join("files"))?;
        std::io::copy(&mut fs::File::open(path)?, &mut temp)?;
        temp.as_file().sync_all()?;
        temp.persist(self.root.join(&name))?;
        Ok(name)
    }
    pub fn backup(&self, path: &Path) -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let dbfile = tmp.path().join("library.sqlite3");
        self.db.backup("main", &dbfile, None)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        {
            let mut zip = zip::ZipWriter::new(output.as_file_mut());
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("library.sqlite3", opts)?;
            zip.write_all(&fs::read(dbfile)?)?;
            for file in fs::read_dir(self.root.join("files"))? {
                let file = file?;
                if file.file_type()?.is_file()
                    && !file.file_name().to_string_lossy().ends_with(".part")
                {
                    zip.start_file(
                        format!("files/{}", file.file_name().to_string_lossy()),
                        opts,
                    )?;
                    std::io::copy(&mut fs::File::open(file.path())?, &mut zip)?;
                }
            }
            zip.finish()?;
        }
        output.as_file().sync_all()?;
        output.persist(path)?;
        Ok(())
    }
    pub fn has_documents(&self) -> bool {
        self.db
            .query_row("SELECT id FROM documents LIMIT 1", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .ok()
            .flatten()
            .is_some()
    }
}

enum SaveJob {
    Page(String, Payload),
    Barrier(mpsc::Sender<Result<(), String>>),
}
pub struct Saver {
    tx: mpsc::Sender<SaveJob>,
    pub events: mpsc::Receiver<Result<(), String>>,
    pub pending: usize,
}
impl Saver {
    pub fn start(root: PathBuf) -> Result<Self> {
        let mut store = Store::open(root)?;
        let (tx, rx) = mpsc::channel();
        let (out, events) = mpsc::channel();
        thread::spawn(move || {
            let mut failures = BTreeMap::new();
            while let Ok(job) = rx.recv() {
                match job {
                    SaveJob::Page(id, payload) => {
                        let result = store.save(&id, &payload).map_err(|e| e.to_string());
                        if result.is_err() {
                            failures.insert(id.clone(), payload);
                        } else {
                            failures.remove(&id);
                        }
                        let _ = out.send(result);
                    }
                    SaveJob::Barrier(reply) => {
                        let mut error = None;
                        failures.retain(|id, p| match store.save(id, p) {
                            Ok(()) => false,
                            Err(e) => {
                                error = Some(e.to_string());
                                true
                            }
                        });
                        let _ = reply.send(error.map_or(Ok(()), Err));
                    }
                }
            }
        });
        Ok(Self {
            tx,
            events,
            pending: 0,
        })
    }
    pub fn save(&mut self, p: &Page) -> Result<()> {
        self.tx
            .send(SaveJob::Page(p.id.clone(), p.payload.clone()))?;
        self.pending += 1;
        Ok(())
    }
    pub fn flush(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.tx.send(SaveJob::Barrier(tx))?;
        rx.recv()?.map_err(anyhow::Error::msg)
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Payload>,
    redo: Vec<Payload>,
}
impl History {
    pub fn can_undo(&self, current: &Payload) -> bool {
        self.undo.iter().any(|p| p != current)
    }
    pub fn can_redo(&self, current: &Payload) -> bool {
        self.redo.iter().any(|p| p != current)
    }
    pub fn push(&mut self, p: &Payload) {
        self.undo.push(p.clone());
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, p: &mut Payload) {
        while let Some(prev) = self.undo.pop() {
            if prev != *p {
                self.redo.push(std::mem::replace(p, prev));
                break;
            }
        }
    }
    pub fn redo(&mut self, p: &mut Payload) {
        while let Some(next) = self.redo.pop() {
            if next != *p {
                self.undo.push(std::mem::replace(p, next));
                break;
            }
        }
    }
}

pub fn distance(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let t = if dx * dx + dy * dy > 0. {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0., 1.)
    } else {
        0.
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}
pub fn inside(p: Point, polygon: &[Point]) -> bool {
    let mut hit = false;
    if polygon.len() < 3 {
        return false;
    }
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            hit = !hit;
        }
    }
    hit
}
pub fn erase(p: &mut Payload, a: Point, b: Point) -> bool {
    let before = p.strokes.len();
    p.strokes.retain(|s| {
        let r = 10. + s.max_radius();
        if s.points.iter().any(|&p| distance(p, a, b) <= r) {
            return false;
        }
        !s.points.windows(2).any(|v| {
            let c = v[0];
            let d = v[1];
            let cross = |u: Point, v: Point, w: Point| {
                (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0])
            };
            distance(a, c, d) <= r
                || distance(b, c, d) <= r
                || (cross(a, b, c) * cross(a, b, d) < 0. && cross(c, d, a) * cross(c, d, b) < 0.)
        })
    });
    before != p.strokes.len()
}
