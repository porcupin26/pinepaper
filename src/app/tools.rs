use super::*;

impl App {
    pub fn is_quick_sheets(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| {
            d.kind == "notebook"
                && !d.trashed
                && self
                    .store
                    .setting::<Option<String>>("quick_sheets", None)
                    .as_ref()
                    == Some(&d.id)
        })
    }
    pub fn action_available(&self, action: &Action) -> bool {
        let notebook =
            self.doc.as_ref().is_some_and(|d| d.kind == "notebook") && self.page().is_some();
        match action {
            Action::AddPage | Action::TemplatePanel | Action::Template(_) => {
                notebook && self.page_selection.is_none()
            }
            Action::DeletePage => notebook && self.pages.len() > 1,
            Action::ClearPage => {
                notebook
                    && self.page().is_some_and(|p| {
                        !p.payload.strokes.is_empty() || !p.payload.texts.is_empty()
                    })
            }
            Action::DeleteSelection => {
                !self.selection.strokes.is_empty() || !self.selection.texts.is_empty()
            }
            Action::Previous => self.index > 0,
            Action::Next => self.index + 1 < self.pages.len(),
            Action::Undo | Action::Redo => {
                let redo = *action == Action::Redo;
                if let Some(s) = &self.sharing {
                    return s.replica.as_ref().is_some_and(|r| r.can_undo(redo));
                }
                self.page().is_some_and(|p| {
                    self.histories.get(&p.id).is_some_and(|h| {
                        if redo {
                            h.can_redo(&p.payload)
                        } else {
                            h.can_undo(&p.payload)
                        }
                    })
                })
            }
            Action::MakeNotebook => self.is_quick_sheets(),
            Action::NameNotebook => self
                .page_selection
                .as_ref()
                .is_some_and(|ids| !ids.is_empty()),
            _ => true,
        }
    }
    pub(super) fn update_tooltip(&mut self, now: Instant) {
        if self.tip_until.is_some_and(|until| now >= until) {
            self.tool_tip = None;
            self.tip_until = None;
            self.redraw = true;
        }
        if self.long_pressed
            || self.dialog.is_some()
            || self.screen == Screen::Library
            || now.duration_since(self.press_at) < Duration::from_millis(550)
        {
            return;
        }
        if let Some(action) = &self.press {
            if let Some(label) = ui::action_label(action) {
                if let Some((bounds, _)) = self.hits.iter().find(|(_, a)| a == action) {
                    self.tool_tip = Some((*bounds, label.into()));
                    self.long_pressed = true;
                    self.redraw = true;
                }
            }
        }
    }
    pub(super) fn remember_changed(&mut self, original: &Payload) {
        if self.sharing.is_none() {
            if let Some(p) = self.page() {
                if &p.payload != original {
                    self.histories
                        .entry(p.id.clone())
                        .or_default()
                        .push(original);
                }
            }
        }
    }
    pub(super) fn preview_diagram(
        &mut self,
        old: &Stroke,
        new: &Stroke,
        background: &SurfaceSnapshot,
    ) {
        let v = self.view;
        let mut bounds = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in old.points.iter().chain(&new.points) {
            let q = v.screen(*p);
            bounds.0 = bounds.0.min(q[0]);
            bounds.1 = bounds.1.min(q[1]);
            bounds.2 = bounds.2.max(q[0]);
            bounds.3 = bounds.3.max(q[1]);
        }
        let pad = old.width.max(new.width) * v.zoom + 4.;
        let x = (bounds.0 - pad).max(v.clip.x);
        let y = (bounds.1 - pad).max(v.clip.y);
        let right = (bounds.2 + pad).min(v.clip.x + v.clip.w);
        let bottom = (bounds.3 + pad).min(v.clip.y + v.clip.h);
        let rect = self
            .surface
            .rect(Box2::new(x, y, (right - x).max(0.), (bottom - y).max(0.)));
        self.surface.restore(background, rect);
        self.surface.stroke(v, new);
        self.dirty(rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hold_labels_do_not_activate_and_hover_or_drag_does_not_label() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = App::new(dir.path().into()).unwrap();
        a.act(Action::New("notebook".into()));
        for (w, h) in [(702, 936), (936, 702)] {
            a.resize(w, h, 1);
            a.render();
            a.motion(34., 126., 0.);
            a.update_tooltip(Instant::now() + Duration::from_secs(1));
            assert!(a.tool_tip.is_none());
            a.down(34., 126., 0.5, false);
            a.update_tooltip(a.press_at + Duration::from_millis(551));
            assert_eq!(a.tool_tip.as_ref().unwrap().1, "Pen");
            a.render();
            a.up(34., 126.);
            assert!(a.dialog.is_none());
            assert!(a.page().unwrap().payload.strokes.is_empty());
            a.update_tooltip(Instant::now() + Duration::from_secs(2));
            assert!(a.tool_tip.is_none());
            a.down(34., 126., 0.5, false);
            a.motion(34., 170., 0.5);
            a.update_tooltip(a.press_at + Duration::from_secs(1));
            assert!(a.tool_tip.is_none());
            a.up(34., 170.);
            a.down(34., 126., 0.5, false);
            a.up(34., 126.);
            assert!(matches!(a.dialog, Some(Dialog::PenPanel)));
            a.act(Action::Close);
        }
    }
}
