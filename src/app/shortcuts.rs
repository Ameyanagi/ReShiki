use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use iced::{
    Task,
    keyboard::{Key, Modifiers, key::Named},
};
use reshiki::{
    bonds::DoublePosition,
    document::{Document, Point},
    editing::{self, Arrange, Transform},
    hotkeys,
};

#[derive(Debug, Clone)]
pub enum Action {
    FixedLength,
    FixedAngles,
    Rulers,
    Crosshair,
    Nudge(f32, f32),
    Join,
    CopyText(&'static str),
    Copied {
        epoch: u64,
        revision: u64,
        result: Result<String, String>,
    },
}

/// Called only after focused widgets have had an opportunity to capture the key.
/// Character hotkeys use the actual modified character (including Caps Lock).
pub(super) fn key_message(key: &Key, modified: &Key, mods: Modifiers) -> Option<Message> {
    if super::help::is_shortcut(key, mods) {
        return Some(Message::ToggleHelp);
    }
    if mods.command() {
        let Key::Character(c) = key else {
            return matches!(key, Key::Named(Named::Enter)).then_some(Message::InlineText(
                super::inline_text::Action::Finish(true),
            ));
        };
        let c = c.to_ascii_lowercase();
        if mods.alt() && mods.shift() {
            return Some(Message::Arrange(match c.as_str() {
                "l" => Arrange::AlignLeft,
                "c" => Arrange::AlignVertical,
                "r" => Arrange::AlignRight,
                "t" => Arrange::AlignTop,
                "m" => Arrange::AlignHorizontal,
                "b" => Arrange::AlignBottom,
                "h" => Arrange::DistributeHorizontal,
                "v" => Arrange::DistributeVertical,
                _ => return None,
            }));
        }
        if mods.alt() {
            return Some(match c.as_str() {
                "k" => Message::AromaticDisplay,
                "c" => Message::Shortcut(Action::CopyText("smiles")),
                "o" => Message::Shortcut(Action::CopyText("mol")),
                "p" => Message::Paste,
                "x" => Message::Shortcut(Action::Crosshair),
                _ => return None,
            });
        }
        if mods.shift() {
            return Some(match c.as_str() {
                "z" => Message::Redo,
                "a" => Message::InvertSelection,
                "g" => Message::Ungroup,
                "k" => Message::Clean,
                "h" => Message::Transform(Transform::FlipVertical),
                "v" => Message::Transform(Transform::FlipHorizontal),
                "d" => Message::Duplicate,
                "e" => Message::Inspector(InspectorTab::Export),
                "c" => Message::CopyImage,
                _ => return None,
            });
        }
        return Some(match c.as_str() {
            "z" => Message::Undo,
            "y" if cfg!(windows) => Message::Redo,
            "a" => Message::SelectAll,
            "g" => Message::Group,
            "c" => Message::Copy(false),
            "x" => Message::Copy(true),
            "v" => Message::Paste,
            "d" => Message::Shortcut(Action::CopyText("cdxml")),
            "j" => Message::Shortcut(Action::Join),
            "i" => Message::ToggleImport,
            "l" => Message::Shortcut(Action::FixedLength),
            "e" => Message::Shortcut(Action::FixedAngles),
            "/" => Message::Fit,
            ";" => Message::Shortcut(Action::Rulers),
            "[" => Message::BondDepth(false),
            "]" => Message::BondDepth(true),
            _ => return None,
        });
    }
    if mods.control() || mods.logo() {
        return None;
    }
    if mods.alt() {
        return Some(match (key, mods.shift()) {
            (Key::Named(Named::ArrowUp), true) => Message::Transform(Transform::TiltX(-12.)),
            (Key::Named(Named::ArrowDown), true) => Message::Transform(Transform::TiltX(12.)),
            (Key::Named(Named::ArrowLeft), true) => Message::Transform(Transform::TiltY(12.)),
            (Key::Named(Named::ArrowRight), true) => Message::Transform(Transform::TiltY(-12.)),
            (Key::Named(Named::ArrowUp), false) => Message::Transform(Transform::Rotate(-15.)),
            (Key::Named(Named::ArrowDown), false) => Message::Transform(Transform::Rotate(15.)),
            (Key::Named(Named::ArrowLeft), false) => Message::Transform(Transform::Rotate(-1.)),
            (Key::Named(Named::ArrowRight), false) => Message::Transform(Transform::Rotate(1.)),
            (Key::Character(c), false) if cfg!(windows) && c.eq_ignore_ascii_case("k") => {
                Message::AromaticDisplay
            }
            _ => return None,
        });
    }
    let step = if mods.shift() { 10. } else { 1. };
    Some(match modified {
        Key::Character(c) => Message::ContextKey(c.to_string()),
        Key::Named(Named::Space) => Message::Tool(Tool::Select),
        Key::Named(Named::Delete | Named::Backspace) => Message::Delete,
        Key::Named(Named::Enter) => Message::ContextKey("Enter".into()),
        Key::Named(Named::Escape) => Message::Escape,
        Key::Named(Named::ArrowLeft) => Message::Shortcut(Action::Nudge(-step, 0.)),
        Key::Named(Named::ArrowRight) => Message::Shortcut(Action::Nudge(step, 0.)),
        Key::Named(Named::ArrowUp) => Message::Shortcut(Action::Nudge(0., -step)),
        Key::Named(Named::ArrowDown) => Message::Shortcut(Action::Nudge(0., step)),
        _ => return None,
    })
}

impl App {
    pub(super) fn shortcut_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::FixedLength => {
                return self.update(Message::FixedLength(!self.bond_drawing.fixed_length));
            }
            Action::FixedAngles => {
                return self.update(Message::FixedAngles(!self.bond_drawing.fixed_angles));
            }
            Action::Rulers => return self.update(Message::Rulers(!self.guides.rulers)),
            Action::Crosshair => return self.update(Message::Crosshair(!self.guides.crosshair)),
            Action::Nudge(x, y) => {
                let before = self.doc.clone();
                self.doc.translate(&self.selected, x, y);
                self.changed(before);
            }
            Action::Join => {
                let result = self.join_shortcut();
                self.commit_hotkey(result, "Joined selected attachment sites");
            }
            Action::CopyText(format) => {
                if self.selected.is_empty() {
                    self.status = "Select a structure to copy".into();
                    return Task::none();
                }
                let engine = self.engine.clone();
                let mut request = super::Request::molecule(
                    "export",
                    editing::selection(&self.doc, &self.selected),
                );
                request.format = Some(format.into());
                let (epoch, revision) = (self.file_epoch, self.revision);
                return Task::perform(
                    async move {
                        engine
                            .request(request)
                            .await?
                            .output
                            .ok_or_else(|| "No text was exported".into())
                    },
                    move |result| {
                        Message::Shortcut(Action::Copied {
                            epoch,
                            revision,
                            result,
                        })
                    },
                );
            }
            Action::Copied {
                epoch,
                revision,
                result,
            } => {
                if epoch != self.file_epoch || revision != self.revision {
                    self.status = "Drawing changed; copy the structure again".into();
                    return Task::none();
                }
                match result {
                    Ok(text) => {
                        self.status = "Structure text copied".into();
                        self.error = false;
                        return iced::clipboard::write(text);
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
        }
        Task::none()
    }

    fn join_shortcut(&self) -> Result<(Document, Vec<u64>), String> {
        use reshiki::{
            joining::Prepared,
            templates::{Anchor, Connection},
        };
        if let [source, target] = self.selected.as_slice() {
            let prepared = Prepared::new(&self.doc, &[*source])?;
            let point = prepared
                .base
                .atom(*target)
                .ok_or("Select attachment atoms from two separate fragments")?
                .position;
            return prepared.place(
                point,
                None,
                1.,
                Anchor::Atom(*source),
                Connection::ShareAtom,
            );
        }
        if self.selected.len() == 4 {
            let bonds: Vec<_> = self
                .doc
                .bonds
                .iter()
                .filter(|b| self.selected.contains(&b.a) && self.selected.contains(&b.b))
                .collect();
            if let [source, target] = bonds.as_slice() {
                let prepared = Prepared::new(&self.doc, &[source.a, source.b])?;
                let a = prepared
                    .base
                    .atom(target.a)
                    .ok_or("Choose bonds from separate fragments")?
                    .position;
                let b = prepared
                    .base
                    .atom(target.b)
                    .ok_or("Choose bonds from separate fragments")?
                    .position;
                return prepared.place(
                    Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.),
                    None,
                    1.,
                    Anchor::Bond(source.a, source.b),
                    Connection::FuseBond,
                );
            }
        }
        Err(
            "Select two atoms, or the four endpoints of two bonds, from separate fragments to join"
                .into(),
        )
    }

    fn commit_hotkey(&mut self, result: Result<(Document, Vec<u64>), String>, status: &str) {
        match result {
            Ok((doc, selected)) => {
                let before = std::mem::replace(&mut self.doc, doc);
                self.selected = selected;
                self.changed(before);
                self.status = status.into();
                self.error = false;
                self.sync_typography();
                self.sync_bonds();
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }

    pub(super) fn context_key(&mut self, key: &str) -> Task<Message> {
        let point = self
            .hover
            .filter(|(_, epoch)| *epoch == self.file_epoch)
            .map(|(p, _)| p);
        let hovered_atom = point.and_then(|p| self.doc.nearest(p, 10. / self.camera.zoom));
        let hovered_bond = if hovered_atom.is_none() {
            point
                .and_then(|p| editing::nearest_bond(&self.doc, p, 7. / self.camera.zoom))
                .and_then(|i| self.doc.bonds.get(i))
                .map(|b| (b.a, b.b))
        } else {
            None
        };
        let atom = hovered_atom.or_else(|| {
            if hovered_bond.is_none() {
                match self.selected.as_slice() {
                    [id] if self.doc.atom(*id).is_some() => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        });
        let bond = hovered_bond.or_else(|| {
            if atom.is_none() {
                match self.selected.as_slice() {
                    [a, b]
                        if self
                            .doc
                            .bonds
                            .iter()
                            .any(|e| (e.a == *a && e.b == *b) || (e.a == *b && e.b == *a)) =>
                    {
                        Some((*a, *b))
                    }
                    _ => None,
                }
            } else {
                None
            }
        });
        if key == "g" {
            if let Some(id) = atom {
                self.selected = vec![id];
            } else if let Some((a, b)) = bond {
                self.selected = vec![a, b];
            }
            return Task::none();
        }
        if ["/", "?", "=", "Enter"].contains(&key) {
            if key == "Enter"
                && self
                    .selected
                    .iter()
                    .filter(|id| self.doc.atom(**id).is_some())
                    .count()
                    > 1
            {
                return self.update(Message::AtomText(if self.atom_text_target().is_some() {
                    super::atom_text::Action::Begin(None)
                } else {
                    super::atom_text::Action::ContractSelection
                }));
            }
            if let Some(id) = atom {
                self.selected = vec![id];
                if ["=", "Enter"].contains(&key) {
                    return self
                        .update(Message::AtomText(super::atom_text::Action::Begin(Some(id))));
                }
            } else if let Some((a, b)) = bond {
                self.selected = vec![a, b];
            }
            return self.update(Message::Inspector(InspectorTab::Properties));
        }
        if let Some((a, b)) = bond {
            if let Some(preset) = hotkeys::bond_preset(key) {
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
                self.selected = vec![a, b];
                // Repeated 2 cycles placement while keeping chemical order intact.
                if key == "2"
                    && let Some(current) = self.doc.bonds.iter().find(|e| {
                        ((e.a == a && e.b == b) || (e.a == b && e.b == a))
                            && reshiki::bonds::BondPreset::of(e)
                                == Some(reshiki::bonds::BondPreset::Double)
                    })
                {
                    return self.update(Message::BondPosition(
                        reshiki::scene::effective_double_position(&self.doc, current).cycled(),
                    ));
                }
                let result =
                    hotkeys::bond_edit(&self.doc, a, b, preset).map(|doc| (doc, vec![a, b]));
                self.commit_hotkey(result, &format!("{preset} bond"));
                return Task::none();
            }
            let position = match key {
                "l" => Some(DoublePosition::Left),
                "c" => Some(DoublePosition::Center),
                "r" => Some(DoublePosition::Right),
                _ => None,
            };
            if let Some(position) = position {
                self.selected = vec![a, b];
                return self.update(Message::BondPosition(position));
            }
            if key == "f" {
                self.selected = vec![a, b];
                return self.update(Message::BondDepth(true));
            }
        }
        if let Some(id) = atom
            && let Some(result) = hotkeys::atom_edit(&self.doc, id, key, self.bond_drawing.length)
        {
            let result = result.map(|(doc, focus)| (doc, vec![focus]));
            self.commit_hotkey(result, "Atom shortcut applied");
            if !self.error {
                self.refresh_due = Some(std::time::Instant::now());
                // Continue growth at its new endpoint until the pointer moves again.
                self.hover = self
                    .selected
                    .first()
                    .and_then(|id| self.doc.atom(*id))
                    .map(|a| (a.position, self.file_epoch));
            }
            return Task::none();
        }
        if (atom.is_some() || bond.is_some())
            && let Some(result) =
                hotkeys::ring_edit(&self.doc, atom, bond, key, self.bond_drawing.length)
        {
            self.commit_hotkey(result, "Ring attached");
            return Task::none();
        }
        // No contextual action: select a drawing tool. Preserve useful nonconflicting aliases.
        let tool = match key {
            " " | "v" => Some(Tool::Select),
            "l" => Some(Tool::Lasso),
            "x" | "b" | "1" => Some(Tool::Bond(1)),
            "2" => Some(Tool::Bond(2)),
            "3" => Some(Tool::Bond(3)),
            "4" => Some(Tool::StyledBond(reshiki::bonds::BondPreset::Quadruple)),
            "X" => Some(Tool::Chain(reshiki::chains::ChainMode::Straight)),
            "r" => Some(Tool::Ring),
            "R" => return self.update(Message::ToggleAromaticRing),
            "j" => {
                self.ring_size = 6;
                self.aromatic_ring = true;
                Some(Tool::Ring)
            }
            "J" => Some(Tool::RingPreset(reshiki::rings::Preset::Cyclopentadiene)),
            "a" | "e" => Some(Tool::Arrow),
            "t" => Some(Tool::Text),
            "T" => Some(Tool::Graphic(reshiki::graphics::GraphicKind::Brackets)),
            "E" => Some(Tool::Graphic(reshiki::graphics::GraphicKind::Symbol(
                reshiki::scientific::SymbolKind::CirclePlus,
            ))),
            "G" => Some(Tool::Graphic(reshiki::graphics::GraphicKind::Orbital(
                reshiki::scientific::OrbitalKind::P,
            ))),
            _ => None,
        };
        if let Some(tool) = tool {
            return self.update(Message::Tool(tool));
        }
        if let Some((label, reshiki::atom_text::Mode::Auto)) = hotkeys::atom_label(key) {
            return self.update(Message::Element(label.into()));
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Edit;
    use reshiki::atom_labels::Carbons;
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
        let _ = app.context_key("c");
        assert_eq!(app.selected, vec![a]);
        assert_eq!(app.doc.atom(a).unwrap().display.carbons, Some(Carbons::All));
        assert_eq!(app.doc.atom(a).unwrap().label_h, 3);
        assert!(!super::super::chemistry_changed(&original, &app.doc));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.context_key("n");
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
        for (key, order) in [("2", 2), ("3", 3), ("1", 1)] {
            let _ = app.context_key(key);
            assert_eq!(app.doc.bonds[0].order, order);
        }
        app.selected.clear();
        app.edit(Edit::Hover(Some(Point::new(21., 0.))));
        let _ = app.context_key("2");
        let before = app.doc.clone();
        let _ = app.context_key("2");
        assert_eq!(app.doc.bonds[0].order, 2);
        assert_ne!(
            app.doc.bonds[0].double_position,
            before.bonds[0].double_position
        );
        assert!(!super::super::chemistry_changed(&before, &app.doc));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        app.edit(Edit::Hover(Some(Point::default())));
        let _ = app.context_key("s");
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
        let _ = app.context_key("o");
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
        let _ = app.update(Message::AromaticDisplay);
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

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use crate::canvas::Edit;
    use reshiki::bonds::BondPreset;

    fn key(c: &str) -> Key {
        Key::Character(c.into())
    }
    fn primary() -> Modifiers {
        if cfg!(target_os = "macos") {
            Modifiers::LOGO
        } else {
            Modifiers::CTRL
        }
    }

    #[test]
    fn dispatcher_preserves_shift_and_does_not_fall_through_modifier_chords() {
        assert!(
            matches!(key_message(&key("c"), &key("C"), Modifiers::SHIFT), Some(Message::ContextKey(k)) if k == "C")
        );
        assert!(
            matches!(key_message(&key("n"), &key("N"), Modifiers::empty()), Some(Message::ContextKey(k)) if k == "N")
        );
        assert!(matches!(
            key_message(&key("d"), &key("d"), primary()),
            Some(Message::Shortcut(Action::CopyText("cdxml")))
        ));
        assert!(matches!(
            key_message(&key("e"), &key("e"), primary()),
            Some(Message::Shortcut(Action::FixedAngles))
        ));
        assert!(matches!(
            key_message(&key("k"), &key("K"), primary() | Modifiers::SHIFT),
            Some(Message::Clean)
        ));
        assert!(matches!(
            key_message(&key("j"), &key("j"), primary()),
            Some(Message::Shortcut(Action::Join))
        ));
        assert!(key_message(&key("x"), &key("X"), primary() | Modifiers::SHIFT).is_none());
        assert!(key_message(&key("z"), &key("z"), primary() | Modifiers::ALT).is_none());
        assert!(matches!(
            key_message(
                &Key::Named(Named::ArrowLeft),
                &Key::Named(Named::ArrowLeft),
                Modifiers::ALT | Modifiers::SHIFT
            ),
            Some(Message::Transform(Transform::TiltY(_)))
        ));
    }

    #[test]
    fn numeric_hotkeys_distinguish_hovered_bond_atom_and_blank_canvas() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        let initial = app.doc.clone();
        app.selected = vec![a]; // The actual hovered bond must win over this selection.
        app.edit(Edit::Hover(Some(Point::new(21., 0.))));
        let _ = app.context_key("2");
        assert_eq!(app.doc.atoms.len(), 2);
        assert_eq!(app.doc.bonds.first().ok_or("Missing bond")?.order, 2);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, initial);
        app.edit(Edit::Hover(Some(Point::new(42., 0.))));
        let _ = app.context_key("2");
        assert_eq!(app.doc.atoms.len(), 4);
        assert!(app.doc.atoms.iter().any(|a| a.element == "O"));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, initial);
        app.selected.clear();
        app.edit(Edit::Hover(None));
        let _ = app.context_key("2");
        assert_eq!(app.tool, Tool::Bond(2));
        assert_eq!(app.doc, initial);
        Ok(())
    }

    #[test]
    fn every_bond_hotkey_is_undoable_and_preserves_endpoints() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        let original = app.doc.clone();
        for key in ["2", "3", "b", "B", "w", "h", "W", "H", "y", "d", "D"] {
            app.selected = vec![a, b];
            let _ = app.context_key(key);
            app.doc.validate()?;
            let bond = app.doc.bonds.first().ok_or("Missing bond")?;
            assert_eq!((bond.a, bond.b), (a, b));
            assert_eq!(BondPreset::of(bond), hotkeys::bond_preset(key));
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, original, "{key}");
        }
        Ok(())
    }

    #[test]
    fn triple_shortcut_geometry_and_order_undo_and_redo_as_one_edit() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("C", Point::new(0., 0.));
        let b = app.doc.add_atom("C", Point::new(36.373, -21.));
        let c = app.doc.add_atom("C", Point::new(72.746, 0.));
        let d = app.doc.add_atom("C", Point::new(109.119, -21.));
        for (a, b) in [(a, b), (b, c), (c, d)] {
            app.doc.add_bond(a, b, 1, "plain");
        }
        app.selected = vec![b, c];
        app.hover = None;
        let original = app.doc.clone();
        let _ = app.context_key("3");
        assert!(!app.error, "{}", app.status);
        let changed = app.doc.clone();
        assert_ne!(
            changed.atom(a).ok_or("Missing atom")?.position,
            original.atom(a).ok_or("Missing atom")?.position
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, changed);
        Ok(())
    }

    #[test]
    fn group_shortcuts_are_atomic_and_modal_editors_block_them() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("N", Point::default());
        let b = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        let original = app.doc.clone();
        app.selected = vec![b];
        let _ = app.update(Message::ContextKey("y".into()));
        assert_eq!(app.doc.abbreviation(b).ok_or("Missing Boc")?.label, "Boc");
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::AtomText(super::super::atom_text::Action::Begin(
            Some(b),
        )));
        let _ = app.update(Message::ContextKey("2".into()));
        let _ = app.update(Message::Shortcut(Action::Join));
        assert_eq!(app.doc, original);
        assert!(app.atom_text.is_some());
        Ok(())
    }

    #[test]
    fn join_merges_sites_instead_of_adding_an_extra_bond_and_undo_restores_all()
    -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        let a = app.doc.add_atom("C", Point::new(0., 0.));
        let b = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        let c = app.doc.add_atom("C", Point::new(150., 0.));
        let d = app.doc.add_atom("O", Point::new(192., 0.));
        app.doc.add_bond(c, d, 1, "plain");
        app.selected = vec![b, c];
        let original = app.doc.clone();
        let _ = app.update(Message::Shortcut(Action::Join));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.doc.atoms.len(), 3);
        assert_eq!(app.doc.bonds.len(), 2);
        app.doc.validate()?;
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        // A failed attempt cannot lose either fragment.
        app.selected = vec![a, d];
        let _ = app.update(Message::Shortcut(Action::Join));
        assert!(app.error);
        assert_eq!(app.doc, original);
        Ok(())
    }
}
