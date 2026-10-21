//! Client-owned, operation-based document state. Unique ink/text IDs preserve
//! concurrent additions; Lamport registers resolve edits of the same object.
use crate::model::{identifier, Document, Page, Payload, Stroke, Text};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const OBJECT_ID: &str = "pinepaper_object_id";
const MAX_CELLS: usize = 200_000;
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Stamp {
    pub clock: u64,
    pub actor: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub stamp: Stamp,
    pub born: Stamp,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub value: Option<Value>,
}
// Missing means tombstone; explicit JSON null is a legitimate nullable field
// (page parent, heading, body). Default Option deserialization conflates these.
fn present_value<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Change {
    pub key: String,
    pub cell: Cell,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedState {
    pub document: String,
    pub kind: String,
    pub cells: BTreeMap<String, Cell>,
}
#[derive(Clone)]
struct UndoChange {
    key: String,
    value: Option<Value>,
    expected: Stamp,
}
pub struct Replica {
    pub state: SharedState,
    actor: String,
    clock: u64,
    mapping: BTreeMap<String, String>, // wire page ID -> local SQLite page ID
    last_pages: BTreeMap<String, Page>,
    undo: Vec<Vec<UndoChange>>,
    redo: Vec<Vec<UndoChange>>,
}
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn object_id(extra: &BTreeMap<String, Value>) -> Option<&str> {
    extra.get(OBJECT_ID)?.as_str().filter(|id| valid_id(id))
}
fn assign_ids(page: &mut Page) {
    let mut seen = BTreeSet::new();
    for extra in page
        .payload
        .strokes
        .iter_mut()
        .map(|s| &mut s.extra)
        .chain(page.payload.texts.iter_mut().map(|t| &mut t.extra))
    {
        let id = object_id(extra)
            .map(str::to_owned)
            .filter(|id| seen.insert(id.clone()))
            .unwrap_or_else(|| {
                let id = identifier();
                seen.insert(id.clone());
                id
            });
        extra.insert(OBJECT_ID.into(), Value::String(id));
    }
}
impl SharedState {
    pub fn validate(&self) -> Result<()> {
        ensure!(valid_id(&self.document), "Invalid shared document ID");
        ensure!(
            ["notebook", "book", "pdf"].contains(&self.kind.as_str()),
            "Unsupported shared document type"
        );
        ensure!(
            self.cells.len() <= MAX_CELLS,
            "Shared document is too large"
        );
        for (key, cell) in &self.cells {
            validate_change(key, cell)?;
        }
        Ok(())
    }
}
fn validate_change(key: &str, cell: &Cell) -> Result<()> {
    ensure!(
        valid_id(&cell.stamp.actor)
            && valid_id(&cell.born.actor)
            && cell.born <= cell.stamp
            && cell.stamp.clock < (1u64 << 53),
        "Invalid edit clock"
    );
    let parts: Vec<_> = key.split('/').collect();
    if key == "title" {
        ensure!(
            cell.value
                .as_ref()
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 240),
            "Invalid shared title"
        );
        return Ok(());
    }
    ensure!(
        (parts.len() == 3 || parts.len() == 4) && parts[0] == "p" && valid_id(parts[1]),
        "Invalid page key"
    );
    let Some(v) = &cell.value else {
        ensure!(
            parts.len() == 4 && ["s", "t"].contains(&parts[2]),
            "Invalid deletion"
        );
        ensure!(valid_id(parts[3]), "Invalid object ID");
        return Ok(());
    };
    match (parts[2], parts.len()) {
        ("s", 4) => {
            ensure!(valid_id(parts[3]), "Invalid ink ID");
            let stroke: Stroke = serde_json::from_value(v.clone())?;
            ensure!(stroke.points.len() <= 1_000_000, "Stroke is too large");
            let mut p = Payload::blank("Blank");
            p.strokes.push(stroke);
            p.validate()?;
        }
        ("t", 4) => {
            ensure!(valid_id(parts[3]), "Invalid text ID");
            let text: Text = serde_json::from_value(v.clone())?;
            ensure!(
                text.text.len() <= 1_000_000 && text.color.is_finite(),
                "Text is too large"
            );
            let mut p = Payload::blank("Blank");
            p.texts.push(text);
            p.validate()?;
        }
        ("alive" | "bookmarked", 3) => ensure!(v.is_boolean(), "Invalid page flag"),
        ("after", 3) => ensure!(
            v.is_null() || v.as_str().is_some_and(valid_id),
            "Invalid page order"
        ),
        ("template", 3) => ensure!(
            v.as_str().is_some_and(|s| s.len() <= 100),
            "Invalid template"
        ),
        ("body" | "heading", 3) => ensure!(
            v.is_null() || v.as_str().is_some_and(|s| s.len() <= 2_000_000),
            "Page text is too large"
        ),
        ("tags", 3) => {
            let tags: Vec<String> = serde_json::from_value(v.clone())?;
            ensure!(
                tags.len() <= 100 && tags.iter().all(|t| t.len() <= 240),
                "Invalid tags"
            );
        }
        ("extra", 3) => ensure!(
            v.is_object() && v.to_string().len() <= 64 * 1024,
            "Invalid page attributes"
        ),
        _ => bail!("Unknown shared document field"),
    }
    Ok(())
}
impl Replica {
    pub fn from_local(doc: &Document, pages: &mut [Page]) -> Result<Self> {
        let state = SharedState {
            document: identifier(),
            kind: doc.kind.clone(),
            cells: BTreeMap::new(),
        };
        let mut r = Self::empty(state);
        for p in pages.iter() {
            r.mapping.insert(p.id.clone(), p.id.clone());
        }
        r.capture(&doc.title, pages, true)?;
        r.undo.clear();
        Ok(r)
    }
    pub fn from_shared(state: SharedState) -> Result<Self> {
        state.validate()?;
        Ok(Self::empty(state))
    }
    fn empty(state: SharedState) -> Self {
        let clock = state
            .cells
            .values()
            .map(|c| c.stamp.clock)
            .max()
            .unwrap_or(0);
        Self {
            state,
            actor: identifier(),
            clock,
            mapping: BTreeMap::new(),
            last_pages: BTreeMap::new(),
            undo: vec![],
            redo: vec![],
        }
    }
    pub fn title(&self) -> String {
        self.value("title")
            .and_then(Value::as_str)
            .unwrap_or("Shared document")
            .to_string()
    }
    fn value(&self, key: &str) -> Option<&Value> {
        self.state.cells.get(key)?.value.as_ref()
    }
    fn wire_page(&mut self, local: &str) -> String {
        if let Some((wire, _)) = self.mapping.iter().find(|(_, id)| id.as_str() == local) {
            return wire.clone();
        }
        self.mapping.insert(local.into(), local.into());
        local.into()
    }
    fn write(&mut self, key: String, value: Option<Value>) -> (Change, UndoChange) {
        self.clock += 1;
        let stamp = Stamp {
            clock: self.clock,
            actor: self.actor.clone(),
        };
        let old = self.state.cells.get(&key);
        let undo = UndoChange {
            key: key.clone(),
            value: old.and_then(|c| c.value.clone()),
            expected: stamp.clone(),
        };
        let cell = Cell {
            born: old.map(|c| c.born.clone()).unwrap_or_else(|| stamp.clone()),
            stamp,
            value,
        };
        self.state.cells.insert(key.clone(), cell.clone());
        (Change { key, cell }, undo)
    }
    pub fn capture(
        &mut self,
        title: &str,
        pages: &mut [Page],
        complete: bool,
    ) -> Result<Vec<Change>> {
        let mut desired = BTreeMap::<String, Option<Value>>::new();
        desired.insert("title".into(), Some(json!(title)));
        let mut previous: Option<String> = None;
        let mut present = BTreeSet::new();
        for page in pages.iter_mut() {
            assign_ids(page);
            let wire = self.wire_page(&page.id);
            let prefix = format!("p/{wire}");
            present.insert(page.id.clone());
            if !self.state.cells.contains_key(&format!("{prefix}/alive")) {
                desired.insert(format!("{prefix}/alive"), Some(json!(true)));
                desired.insert(format!("{prefix}/after"), Some(json!(previous)));
            }
            previous = Some(wire);
            for (key, value) in [
                ("template", json!(page.payload.template)),
                ("body", json!(page.payload.body)),
                ("heading", json!(page.payload.heading)),
                ("extra", json!(page.payload.extra)),
                ("bookmarked", json!(page.bookmarked)),
                ("tags", json!(page.tags)),
            ] {
                desired.insert(format!("{prefix}/{key}"), Some(value));
            }
            let old = self.last_pages.get(&page.id);
            let old_strokes: HashMap<_, _> = old
                .into_iter()
                .flat_map(|p| p.payload.strokes.iter())
                .filter_map(|s| object_id(&s.extra).map(|id| (id, s)))
                .collect();
            let old_texts: HashMap<_, _> = old
                .into_iter()
                .flat_map(|p| p.payload.texts.iter())
                .filter_map(|t| object_id(&t.extra).map(|id| (id, t)))
                .collect();
            let mut ink_ids = BTreeSet::new();
            let mut text_ids = BTreeSet::new();
            for s in &page.payload.strokes {
                let id = object_id(&s.extra).unwrap();
                ink_ids.insert(id);
                if old_strokes.get(id).is_none_or(|old| *old != s) {
                    desired.insert(format!("{prefix}/s/{id}"), Some(serde_json::to_value(s)?));
                }
            }
            for t in &page.payload.texts {
                let id = object_id(&t.extra).unwrap();
                text_ids.insert(id);
                if old_texts.get(id).is_none_or(|old| *old != t) {
                    desired.insert(format!("{prefix}/t/{id}"), Some(serde_json::to_value(t)?));
                }
            }
            for id in old_strokes.keys().filter(|id| !ink_ids.contains(**id)) {
                desired.insert(format!("{prefix}/s/{id}"), None);
            }
            for id in old_texts.keys().filter(|id| !text_ids.contains(**id)) {
                desired.insert(format!("{prefix}/t/{id}"), None);
            }
            self.last_pages.insert(page.id.clone(), page.clone());
        }
        if complete {
            let removed: Vec<_> = self
                .last_pages
                .keys()
                .filter(|id| !present.contains(*id))
                .cloned()
                .collect();
            for local in removed {
                let wire = self.wire_page(&local);
                desired.insert(format!("p/{wire}/alive"), Some(json!(false)));
                self.last_pages.remove(&local);
            }
        }
        let mut changes = vec![];
        let mut undo = vec![];
        for (key, value) in desired {
            if self.state.cells.get(&key).map(|c| &c.value) == Some(&value) {
                continue;
            }
            // Creation undo hides the page; required metadata is never tombstoned.
            let old_missing = !self.state.cells.contains_key(&key);
            let (change, mut inverse) = self.write(key.clone(), value);
            validate_change(&key, &change.cell)?;
            if old_missing && key.ends_with("/alive") {
                inverse.value = Some(json!(false));
            }
            if !old_missing || key.ends_with("/alive") || key.split('/').count() == 4 {
                undo.push(inverse);
            }
            changes.push(change);
        }
        if !undo.is_empty() {
            self.undo.push(undo);
            self.redo.clear();
            if self.undo.len() > 40 {
                self.undo.remove(0);
            }
        }
        Ok(changes)
    }
    pub fn merge(&mut self, document: &str, changes: &[Change]) -> Result<bool> {
        ensure!(
            document == self.state.document,
            "Room contains a different document"
        );
        ensure!(changes.len() <= MAX_CELLS, "Too many edits");
        for change in changes {
            validate_change(&change.key, &change.cell)?;
        }
        let additional = changes
            .iter()
            .filter(|c| !self.state.cells.contains_key(&c.key))
            .count();
        ensure!(
            self.state.cells.len() + additional <= MAX_CELLS,
            "Shared document is too large"
        );
        let mut changed = false;
        for c in changes {
            self.clock = self.clock.max(c.cell.stamp.clock);
            if self
                .state
                .cells
                .get(&c.key)
                .is_none_or(|old| old.stamp < c.cell.stamp)
            {
                self.state.cells.insert(c.key.clone(), c.cell.clone());
                changed = true;
            }
        }
        Ok(changed)
    }
    pub fn merge_state(&mut self, state: &SharedState) -> Result<bool> {
        state.validate()?;
        ensure!(
            state.kind == self.state.kind,
            "Shared document type changed"
        );
        self.merge(
            &state.document,
            &state
                .cells
                .iter()
                .map(|(key, cell)| Change {
                    key: key.clone(),
                    cell: cell.clone(),
                })
                .collect::<Vec<_>>(),
        )
    }
    pub fn undo(&mut self, redo: bool) -> Vec<Change> {
        let entry = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        let mut changes = vec![];
        let mut inverse = vec![];
        if let Some(entry) = entry {
            for change in entry {
                if self
                    .state
                    .cells
                    .get(&change.key)
                    .is_some_and(|c| c.stamp == change.expected)
                {
                    let (out, back) = self.write(change.key, change.value);
                    changes.push(out);
                    inverse.push(back);
                }
            }
            if !inverse.is_empty() {
                if redo {
                    self.undo.push(inverse);
                } else {
                    self.redo.push(inverse);
                }
            }
        }
        changes
    }
    pub fn pages(&mut self, local_document: &str) -> Result<Vec<Page>> {
        let mut ids: Vec<String> = self
            .state
            .cells
            .keys()
            .filter(|k| k.ends_with("/alive"))
            .map(|k| k.split('/').nth(1).unwrap().to_string())
            .collect();
        ensure!(ids.len() <= 2048, "Too many shared pages");
        // RGA-style ordering. Children of a deleted anchor retain their position;
        // concurrent insertions use birth clocks and actor IDs as a stable tie-break.
        ids.sort_by(|a, b| {
            self.state.cells[&format!("p/{b}/alive")]
                .born
                .cmp(&self.state.cells[&format!("p/{a}/alive")].born)
        });
        let mut ordered = vec![];
        let mut seen = BTreeSet::new();
        fn walk(
            r: &Replica,
            ids: &[String],
            after: Option<&str>,
            seen: &mut BTreeSet<String>,
            out: &mut Vec<String>,
        ) {
            for id in ids {
                let parent = r.value(&format!("p/{id}/after")).and_then(Value::as_str);
                if parent == after && seen.insert(id.clone()) {
                    out.push(id.clone());
                    walk(r, ids, Some(id), seen, out);
                }
            }
        }
        walk(self, &ids, None, &mut seen, &mut ordered);
        for id in &ids {
            if seen.insert(id.clone()) {
                ordered.push(id.clone());
                walk(self, &ids, Some(id), &mut seen, &mut ordered);
            }
        }
        ordered.retain(|id| self.value(&format!("p/{id}/alive")) == Some(&json!(true)));
        let fallback = ordered.is_empty();
        if fallback {
            ordered.push(self.state.document.clone());
        }
        let mut pages = vec![];
        for wire in ordered {
            let local = self
                .mapping
                .entry(wire.clone())
                .or_insert_with(identifier)
                .clone();
            let prefix = format!("p/{wire}");
            let mut payload = Payload::blank(
                self.value(&format!("{prefix}/template"))
                    .and_then(Value::as_str)
                    .unwrap_or("Blank"),
            );
            let mut strokes = vec![];
            let mut texts = vec![];
            if !fallback {
                let mut objects: Vec<_> = self
                    .state
                    .cells
                    .range(format!("{prefix}/")..)
                    .take_while(|(key, _)| key.starts_with(&format!("{prefix}/")))
                    .filter(|(_, c)| c.value.is_some())
                    .collect();
                objects.sort_by(|(ka, a), (kb, b)| a.born.cmp(&b.born).then(ka.cmp(kb)));
                for (key, c) in objects {
                    if key.starts_with(&format!("{prefix}/s/")) {
                        let mut stroke: Stroke = serde_json::from_value(c.value.clone().unwrap())?;
                        stroke
                            .extra
                            .insert(OBJECT_ID.into(), json!(key.rsplit('/').next().unwrap()));
                        strokes.push(stroke);
                    } else if key.starts_with(&format!("{prefix}/t/")) {
                        let mut text: Text = serde_json::from_value(c.value.clone().unwrap())?;
                        text.extra
                            .insert(OBJECT_ID.into(), json!(key.rsplit('/').next().unwrap()));
                        texts.push(text);
                    }
                }
                payload.body = self
                    .value(&format!("{prefix}/body"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                payload.heading = self
                    .value(&format!("{prefix}/heading"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(extra) = self.value(&format!("{prefix}/extra")) {
                    payload.extra = serde_json::from_value(extra.clone())?;
                }
            }
            payload.strokes = strokes;
            payload.texts = texts;
            payload.validate()?;
            let page = Page {
                id: local,
                document: local_document.into(),
                position: pages.len(),
                payload,
                bookmarked: self
                    .value(&format!("{prefix}/bookmarked"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                tags: self
                    .value(&format!("{prefix}/tags"))
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()?
                    .unwrap_or_default(),
            };
            pages.push(page);
        }
        self.last_pages = pages.iter().map(|p| (p.id.clone(), p.clone())).collect();
        Ok(pages)
    }
}
