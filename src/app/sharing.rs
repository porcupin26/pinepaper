use super::*;
use crate::exchange::{self, Command, Event, Replica, Session};
impl App {
    pub fn sharing_label(&self) -> Option<String> {
        self.sharing.as_ref().map(|s| {
            if s.online && s.replica.is_some() {
                format!("{} · {} online · saved locally", s.code, s.peers.len() + 1)
            } else {
                s.status.clone()
            }
        })
    }
    pub(super) fn start_sharing(&mut self, join: Option<String>) -> Result<()> {
        self.finish();
        self.flush()?;
        anyhow::ensure!(
            self.sharing.is_none(),
            "Leave the current sharing session first"
        );
        let (replica, local_document, source) = if join.is_none() {
            let d = self.doc.as_ref().context("Open a document to share")?;
            let source = if d.kind == "pdf" {
                Some(self.store.source_path(d)?)
            } else {
                None
            };
            if let Some(path) = &source {
                anyhow::ensure!(
                    path.metadata()?.len() <= 64 * 1024 * 1024,
                    "Shared PDFs must be at most 64 MiB"
                );
            }
            let replica = Replica::from_local(d, &mut self.pages)?;
            // Save assigned object IDs before advertising the document.
            self.store.save_shared(d, &self.pages)?;
            (Some(replica), Some(d.id.clone()), source)
        } else {
            (None, None, None)
        };
        let link = exchange::Link::start(&self.prefs.exchange_url, join, self.store.root.clone())?;
        self.sharing = Some(Session {
            link,
            replica,
            local_document,
            source,
            code: String::new(),
            peers: vec![],
            status: "Connecting…".into(),
            online: false,
            pending: vec![],
        });
        self.histories.clear();
        self.dialog = Some(Dialog::Sharing);
        Ok(())
    }
    pub(super) fn capture_shared(&mut self, complete: bool) -> Result<()> {
        let Some(s) = &mut self.sharing else {
            return Ok(());
        };
        let Some(d) = &self.doc else {
            return Ok(());
        };
        if s.local_document.as_deref() != Some(&d.id) {
            return Ok(());
        }
        let Some(r) = &mut s.replica else {
            return Ok(());
        };
        let pages = if complete {
            self.pages.as_mut_slice()
        } else {
            self.pages
                .get_mut(self.index..self.index + 1)
                .context("No current page")?
        };
        let changes = r.capture(&d.title, pages, complete)?;
        if !changes.is_empty() {
            if complete {
                self.saver.flush()?;
                self.store.save_shared(d, &self.pages)?;
            }
            s.link.send(Command::Ops {
                document: r.state.document.clone(),
                changes,
            });
        }
        Ok(())
    }
    fn send_snapshot(&self, to: Option<String>) {
        if let Some(s) = &self.sharing {
            if let Some(r) = &s.replica {
                s.link.send(Command::Snapshot {
                    to,
                    state: r.state.clone(),
                    source: s.source.clone(),
                });
            }
        }
    }
    pub(super) fn materialize_shared(&mut self) -> Result<()> {
        self.flush()?;
        let s = self.sharing.as_mut().context("No sharing session")?;
        let r = s.replica.as_mut().context("Waiting for document")?;
        let id = s.local_document.as_ref().context("Missing local copy")?;
        let old_page = self.pages.get(self.index).map(|p| p.id.clone());
        let mut doc = self.store.document(id)?;
        let pages = r.pages(id)?;
        let index = old_page
            .and_then(|id| pages.iter().position(|p| p.id == id))
            .unwrap_or(self.index)
            .min(pages.len() - 1);
        doc.title = r.title();
        doc.current_page = index;
        self.store.save_shared(&doc, &pages)?;
        self.doc = Some(doc);
        self.pages = pages;
        self.index = index;
        self.selection = Selection::default();
        self.histories.clear();
        self.reload_library()?;
        self.redraw = true;
        Ok(())
    }
    pub(super) fn shared_undo(&mut self, redo: bool) -> Result<()> {
        if let Some(s) = &mut self.sharing {
            if let Some(r) = &mut s.replica {
                let changes = r.undo(redo);
                if !changes.is_empty() {
                    s.link.send(Command::Ops {
                        document: r.state.document.clone(),
                        changes,
                    });
                    self.materialize_shared()?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn poll_exchange(&mut self) {
        // A remote erase/reorder must never invalidate pen, lasso or text indices.
        // The bounded worker queue forces a full resync if a long edit fills it.
        if self.gesture.is_some() || matches!(self.dialog, Some(Dialog::Input(_))) {
            return;
        }
        for _ in 0..16 {
            let event = self
                .sharing
                .as_ref()
                .and_then(|s| s.link.events.try_recv().ok());
            let Some(event) = event else {
                break;
            };
            if let Err(e) = self.exchange_event(event) {
                self.sharing = None;
                self.notice = format!("Sharing stopped: {e}. Local copy kept.");
                self.redraw = true;
                break;
            }
            self.redraw = true;
        }
    }
    fn exchange_event(&mut self, event: Event) -> Result<()> {
        let Some(s) = &mut self.sharing else {
            return Ok(());
        };
        match event {
            Event::Connected { code, peers } => {
                s.code = code;
                s.peers = peers;
                s.online = true;
                s.status = if s.replica.is_some() {
                    "Connected"
                } else {
                    "Receiving document…"
                }
                .into();
                if let Some(peer) = s.peers.first() {
                    s.link.send(Command::NeedState(peer.clone()));
                }
                if s.replica.is_some() && !s.peers.is_empty() {
                    self.send_snapshot(None);
                }
            }
            Event::Joined(peer) => {
                if !s.peers.contains(&peer) {
                    s.peers.push(peer);
                }
            }
            Event::Left(peer) => {
                s.peers.retain(|p| p != &peer);
                // A departing participant may have supplied an older in-flight
                // snapshot while the newcomer was joining. Reconcile with a
                // remaining participant even if we already received a replica.
                if let Some(peer) = s.peers.first() {
                    s.link.send(Command::NeedState(peer.clone()));
                } else if s.replica.is_none() {
                    s.status = "Waiting for a participant with a document…".into();
                }
            }
            Event::NeedState(peer) => self.send_snapshot(Some(peer)),
            Event::Status(message) => {
                s.online = false;
                s.status = message;
            }
            Event::Ended(message) => {
                self.sharing = None;
                self.notice = message;
                if matches!(self.dialog, Some(Dialog::Sharing)) {
                    self.dialog =
                        Some(Dialog::Message("Sharing ended".into(), self.notice.clone()));
                }
            }
            Event::Ops { document, changes } => {
                if let Some(r) = &mut s.replica {
                    if r.merge(&document, &changes)? {
                        self.materialize_shared()?;
                    }
                } else {
                    anyhow::ensure!(
                        s.pending.len() < 128,
                        "Too many edits before the document arrived; join again"
                    );
                    s.pending.push((document, changes));
                }
            }
            Event::Snapshot { state, source } => {
                if let Some(r) = &mut s.replica {
                    if r.merge_state(&state)? {
                        self.materialize_shared()?;
                    }
                } else {
                    let mut replica = Replica::from_shared(state)?;
                    for (document, changes) in s.pending.drain(..) {
                        replica.merge(&document, &changes)?;
                    }
                    let id = identifier();
                    let doc = Document {
                        id: id.clone(),
                        title: replica.title(),
                        kind: replica.state.kind.clone(),
                        parent: None,
                        source,
                        current_page: 0,
                        trashed: false,
                    };
                    let pages = replica.pages(&id)?;
                    self.saver.flush()?;
                    self.store.save_shared(&doc, &pages)?;
                    s.local_document = Some(id.clone());
                    s.source = if doc.kind == "pdf" {
                        Some(self.store.source_path(&doc)?)
                    } else {
                        None
                    };
                    s.replica = Some(replica);
                    s.status = "Connected".into();
                    self.doc = Some(doc);
                    self.pages = pages;
                    self.histories.clear();
                    self.screen = Screen::Editor;
                    self.hidden = false;
                    self.fit = false;
                    self.dialog = None;
                    self.go(0)?;
                    self.reload_library()?;
                    // Share merged early edits as well, ensuring all peers converge.
                    self.send_snapshot(None);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[track_caller]
    fn pump(apps: &mut [&mut App], check: impl Fn(&[&mut App]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            for app in apps.iter_mut() {
                app.tick();
            }
            if check(apps) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Timed out: {:?}",
                apps.iter()
                    .map(|a| (
                        a.notice.clone(),
                        a.sharing_label(),
                        a.pages
                            .iter()
                            .map(|p| (p.payload.strokes.len(), p.tags.clone(), p.bookmarked))
                            .collect::<Vec<_>>()
                    ))
                    .collect::<Vec<_>>()
            );
            thread::sleep(Duration::from_millis(15));
        }
    }
    fn ready(app: &App) -> bool {
        app.sharing
            .as_ref()
            .is_some_and(|s| s.online && s.replica.is_some())
    }
    fn same(a: &App, b: &App) -> bool {
        a.pages.len() == b.pages.len()
            && a.pages.iter().zip(&b.pages).all(|(a, b)| {
                a.payload == b.payload && a.tags == b.tags && a.bookmarked == b.bookmarked
            })
    }
    fn draw(app: &mut App, x: f64) {
        app.pages[0].payload.strokes.push(Stroke {
            points: vec![[x, 20., 0.7], [x + 4., 30., 0.6]],
            ..Stroke::default()
        });
        app.save().unwrap();
    }
    #[test]
    #[ignore = "Start pinepaper-exchange and set PINEPAPER_EXCHANGE_TEST_URL"]
    fn live_three_clients_merge_reconnect_and_keep_local_copies() {
        let url = std::env::var("PINEPAPER_EXCHANGE_TEST_URL").expect("Set test relay URL");
        let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let mut a = App::new(dirs[0].path().into()).unwrap();
        let mut b = App::new(dirs[1].path().into()).unwrap();
        let mut c = App::new(dirs[2].path().into()).unwrap();
        for app in [&mut a, &mut b, &mut c] {
            app.prefs.exchange_url = url.clone();
        }
        let id = a
            .store
            .create(
                "Together",
                "notebook",
                None,
                vec![Payload::blank("Lined")],
                None,
            )
            .unwrap();
        a.open(&id).unwrap();
        a.start_sharing(None).unwrap();
        pump(&mut [&mut a], |apps| ready(apps[0]));
        if let Ok(path) = std::env::var("PINEPAPER_EXCHANGE_TEST_ARTIFACTS") {
            let path = PathBuf::from(path);
            std::fs::create_dir_all(&path).unwrap();
            for (w, h, name) in [(702, 936, "portrait"), (936, 702, "landscape")] {
                a.resize(w, h, 1);
                a.render();
                a.surface
                    .save_png(&path.join(format!("sharing-{name}.png")))
                    .unwrap();
            }
        }
        let code = a.sharing.as_ref().unwrap().code.clone();
        b.start_sharing(Some(code.clone())).unwrap();
        pump(&mut [&mut a, &mut b], |apps| ready(apps[1]));
        draw(&mut a, 10.);
        draw(&mut b, 50.);
        pump(&mut [&mut a, &mut b], |apps| {
            apps[0].pages[0].payload.strokes.len() == 2 && same(apps[0], apps[1])
        });
        a.act(Action::Undo);
        pump(&mut [&mut a, &mut b], |apps| {
            apps[0].pages[0].payload.strokes.len() == 1 && same(apps[0], apps[1])
        });
        assert_eq!(b.pages[0].payload.strokes[0].points[0][0], 50.);
        a.act(Action::Redo);
        a.act(Action::AddPage);
        a.act(Action::Bookmark);
        a.act(Action::Template("Grid".into()));
        pump(&mut [&mut a, &mut b], |apps| {
            apps[1].pages.len() == 2 && same(apps[0], apps[1])
        });
        // Don't apply incoming data while text indices are in use.
        b.input(
            "Test edit",
            String::new(),
            InputPurpose::Text(None, [100., 100., 0.]),
            false,
        );
        a.index = 0;
        draw(&mut a, 70.);
        for _ in 0..30 {
            a.tick();
            b.tick();
            thread::sleep(Duration::from_millis(15));
        }
        assert_eq!(b.pages[0].payload.strokes.len(), 2);
        b.act(Action::Close);
        pump(&mut [&mut a, &mut b], |apps| {
            apps[1].pages[0].payload.strokes.len() == 3 && same(apps[0], apps[1])
        });
        // Replace A's transport while retaining its replica: local/offline edits
        // and B's edits must survive full-state reconciliation on rejoin.
        a.sharing.as_mut().unwrap().link =
            exchange::Link::start(&url, Some(code.clone()), a.store.root.clone()).unwrap();
        draw(&mut a, 90.);
        draw(&mut b, 110.);
        pump(&mut [&mut a, &mut b], |apps| {
            apps[0].pages[0].payload.strokes.len() == 5 && same(apps[0], apps[1])
        });
        a.act(Action::LeaveSharing);
        c.start_sharing(Some(code.clone())).unwrap();
        pump(&mut [&mut b, &mut c], |apps| {
            ready(apps[1]) && same(apps[0], apps[1])
        });
        let b_id = b.doc.as_ref().unwrap().id.clone();
        let c_id = c.doc.as_ref().unwrap().id.clone();
        b.act(Action::LeaveSharing);
        c.act(Action::LeaveSharing);
        assert_eq!(b.store.pages(&b_id).unwrap()[0].payload.strokes.len(), 5);
        assert_eq!(c.store.pages(&c_id).unwrap()[0].payload.strokes.len(), 5);
        thread::sleep(Duration::from_millis(100));
        c.start_sharing(Some(code)).unwrap();
        pump(&mut [&mut c], |apps| apps[0].sharing.is_none());
        assert!(c.notice.contains("Room has ended"));
        // The original PDF is supplied by a participant, saved locally and can
        // itself be relayed by the newcomer after the creator has left.
        let pdf = dirs[0].path().join("original.pdf");
        document::export_pdf(&pdf, &a.pages, None).unwrap();
        let pdf_id = document::import(&mut a.store, &pdf, None).unwrap();
        a.open(&pdf_id).unwrap();
        a.start_sharing(None).unwrap();
        pump(&mut [&mut a], |apps| ready(apps[0]));
        let pdf_code = a.sharing.as_ref().unwrap().code.clone();
        b.start_sharing(Some(pdf_code.clone())).unwrap();
        pump(&mut [&mut a, &mut b], |apps| ready(apps[1]));
        let original = std::fs::read(&pdf).unwrap();
        assert_eq!(
            std::fs::read(b.store.source_path(b.doc.as_ref().unwrap()).unwrap()).unwrap(),
            original
        );
        a.act(Action::LeaveSharing);
        c.start_sharing(Some(pdf_code)).unwrap();
        pump(&mut [&mut b, &mut c], |apps| ready(apps[1]));
        assert_eq!(
            std::fs::read(c.store.source_path(c.doc.as_ref().unwrap()).unwrap()).unwrap(),
            original
        );
        draw(&mut b, 130.);
        pump(&mut [&mut b, &mut c], |apps| {
            apps[1].pages[0].payload.strokes.len() == 1 && same(apps[0], apps[1])
        });
        draw(&mut b, 150.);
        b.act(Action::LeaveSharing);
        pump(&mut [&mut c], |apps| {
            apps[0].pages[0].payload.strokes.len() == 2
        });
        c.act(Action::LeaveSharing);

        assert_eq!(c.store.pages(&c_id).unwrap()[0].payload.strokes.len(), 5);
    }
}
