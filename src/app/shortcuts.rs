use super::{App, Message};
use crate::canvas::Tool;
use iced::Task;
use reshiki::{atom_labels::Carbons, bonds::BondPreset};

impl App {
    /// Called only for unhandled keys; focused editors keep ordinary typing.
    pub(super) fn context_key(&mut self, key: &str) -> Task<Message> {
        let point = self
            .hover
            .filter(|(_, epoch)| *epoch == self.file_epoch)
            .map(|(p, _)| p);
        let hovered_atom = point.and_then(|p| self.doc.nearest(p, 10. / self.camera.zoom));
        let atom = hovered_atom.or_else(|| match self.selected.as_slice() {
            [id] if self.doc.atom(*id).is_some() => Some(*id),
            _ => None,
        });
        let bond = if hovered_atom.is_none() {
            point
                .and_then(|p| reshiki::editing::nearest_bond(&self.doc, p, 7. / self.camera.zoom))
                .and_then(|i| self.doc.bonds.get(i))
                .map(|e| (e.a, e.b))
                .or_else(|| match self.selected.as_slice() {
                    [a, b] => self
                        .doc
                        .bonds
                        .iter()
                        .find(|e| (e.a == *a && e.b == *b) || (e.a == *b && e.b == *a))
                        .map(|e| (e.a, e.b)),
                    _ => None,
                })
        } else {
            None
        };
        if let (Some((a, b)), Some(order)) = (
            bond,
            match key {
                "S" => Some(1),
                "D" => Some(2),
                "T" => Some(3),
                _ => None,
            },
        ) {
            if self
                .doc
                .abbreviations
                .iter()
                .any(|g| g.members.contains(&a) || g.members.contains(&b))
            {
                self.status = "Expand the abbreviation before changing its bonds".into();
                self.error = true;
                return Task::none();
            }
            let before = self.doc.clone();
            let current = self
                .doc
                .bonds
                .iter()
                .find(|e| e.a == a && e.b == b)
                .cloned();
            if order == 2
                && let Some(current) = current
                    .as_ref()
                    .filter(|e| e.order == 2 && e.display != "wavy")
            {
                let position =
                    reshiki::scene::effective_double_position(&self.doc, current).cycled();
                if let Some(e) = self.doc.bonds.iter_mut().find(|e| e.a == a && e.b == b) {
                    e.double_position = position;
                }
                self.changed(before);
                self.status = format!("Double bond: {position}");
            } else {
                let preset = match order {
                    2 => BondPreset::Double,
                    3 => BondPreset::Triple,
                    _ => BondPreset::Single,
                };
                if current
                    .as_ref()
                    .is_none_or(|b| BondPreset::of(b) != Some(preset))
                {
                    self.doc.add_bond(a, b, order, "plain");
                    self.changed(before);
                }
                self.status = format!("{preset} bond");
            }
            self.selected = vec![a, b];
            return Task::none();
        }
        if ["C", "N", "O", "S", "P", "F", "H"].contains(&key) {
            if let Some(id) = atom {
                if self
                    .doc
                    .abbreviations
                    .iter()
                    .any(|g| g.members.contains(&id))
                {
                    self.status = "Expand the abbreviation before changing its atoms".into();
                    self.error = true;
                    return Task::none();
                }
                let before = self.doc.clone();
                let replace = self.doc.atom(id).is_some_and(|a| a.element != key);
                if replace {
                    self.doc.invalidate_chemistry(&[id]);
                }
                if let Some(a) = self.doc.atom_mut(id) {
                    if replace {
                        a.element = key.into();
                        a.explicit_h = 0;
                        a.no_implicit = false;
                        a.charge = 0;
                        a.isotope = 0;
                        a.radical_electrons = 0;
                    }
                    a.display.hydrogens = Some(true);
                    if key == "C" {
                        a.display.carbons = Some(Carbons::All);
                    }
                }
                self.selected = vec![id];
                self.changed(before);
                // Carbon visibility is only an appearance change, but its cached
                // H count may not exist yet (for example after opening a draft).
                self.refresh_due = Some(std::time::Instant::now());
                self.status = format!("{key} label · Hydrogens follow the atom’s valence");
                return Task::none();
            }
            return self.update(Message::Element(key.into()));
        }
        match key {
            "D" => self.update(Message::Tool(Tool::Bond(2))),
            "T" => self.update(Message::Tool(Tool::Text)),
            "A" if self
                .selected
                .iter()
                .filter(|id| self.doc.atom(**id).is_some())
                .count()
                >= 3 =>
            {
                self.update(Message::AromaticDisplay)
            }
            "A" => self.update(Message::Tool(Tool::Arrow)),
            _ => Task::none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Edit;
    use reshiki::document::{Document, Point};

    #[test]
    fn hover_atom_shortcuts_reveal_carbon_replace_elements_and_undo_once() {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.doc.atom_mut(a).unwrap().label_h = 3;
        app.selected = vec![b];
        let original = app.doc.clone();
        app.edit(Edit::Hover(Some(Point::default())));
        let _ = app.context_key("C");
        assert_eq!(app.selected, vec![a]);
        assert_eq!(app.doc.atom(a).unwrap().display.carbons, Some(Carbons::All));
        assert_eq!(app.doc.atom(a).unwrap().label_h, 3);
        assert!(!super::super::chemistry_changed(&original, &app.doc));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.context_key("N");
        assert_eq!(app.doc.atom(a).unwrap().element, "N");
        assert_eq!(app.doc.atom(b).unwrap().element, "C");
        assert!(app.refresh_due.is_some());
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
    }

    #[test]
    fn selected_and_hovered_bond_shortcuts_set_order_and_cycle_only_double_position() {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.selected = vec![a, b];
        for (key, order) in [("D", 2), ("T", 3), ("S", 1)] {
            let _ = app.context_key(key);
            assert_eq!(app.doc.bonds[0].order, order);
        }
        app.selected.clear();
        app.edit(Edit::Hover(Some(Point::new(21., 0.))));
        let _ = app.context_key("D");
        let before = app.doc.clone();
        let _ = app.context_key("D");
        assert_eq!(app.doc.bonds[0].order, 2);
        assert_ne!(
            app.doc.bonds[0].double_position,
            before.bonds[0].double_position
        );
        assert!(!super::super::chemistry_changed(&before, &app.doc));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        app.edit(Edit::Hover(Some(Point::default())));
        let _ = app.context_key("S");
        assert_eq!(app.doc.atom(a).unwrap().element, "S");
        assert_eq!(app.doc.bonds[0].order, 2);
    }

    #[test]
    fn stale_hover_and_empty_cleanup_do_not_modify_the_drawing() {
        let (mut app, _) = App::new();
        app.busy = false;
        app.doc = Document::default();
        app.doc.add_atom("C", Point::default());
        app.edit(Edit::Hover(Some(Point::default())));
        app.file_epoch = app.file_epoch.wrapping_add(1);
        let before = app.doc.clone();
        let _ = app.context_key("O");
        assert_eq!(app.doc, before);
        assert_eq!(app.tool, Tool::Atom);
        let _ = app.update(Message::Clean);
        assert!(!app.busy);
        assert!(app.cleanup.is_none());
        assert!(app.status.contains("Select"));
        assert_eq!(app.doc, before);
    }

    #[tokio::test]
    async fn aromatic_shortcut_commits_once_keeps_hydrogens_and_ignores_late_results() {
        use super::super::{Job, Request};
        let (mut app, _) = App::new();
        app.busy = false;
        app.doc = app
            .engine
            .request(Request::import_smiles("c1cc[nH]c1"))
            .await
            .unwrap()
            .document
            .unwrap();
        app.selected = app.doc.all_ids();
        let original = app.doc.clone();
        let revision = app.revision;
        let _ = app.context_key("A");
        assert!(app.busy);
        let mut request = Request::molecule("aromatic", app.doc.clone());
        request.selected_ids = Some(app.selected.clone());
        let response = app.engine.request(request).await.unwrap();
        let _ = app.update(Message::EngineDone {
            revision,
            kind: Job::AromaticDisplay,
            result: Box::new(Ok(response.clone())),
        });
        assert_eq!(reshiki::aromatic::circles(&app.doc).len(), 1);
        assert!(app.doc.atoms.iter().all(|a| a.label_h == 1));
        assert!(app.refresh_due.is_none());
        assert_eq!(app.selected, original.all_ids());
        let circled = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert!(super::super::same_drawing(&app.doc, &original));
        assert!(!app.history.can_undo());
        let _ = app.update(Message::Redo);
        assert!(super::super::same_drawing(&app.doc, &circled));
        let _ = app.update(Message::EngineDone {
            revision,
            kind: Job::AromaticDisplay,
            result: Box::new(Ok(response)),
        });
        assert!(super::super::same_drawing(&app.doc, &circled));
    }
}
