//! Application routing for the pointer-independent drawing hotspot.
use super::{App, Message};
use crate::canvas::Tool;
use iced::Task;
use iced::advanced::widget::{Id, Operation, operation::Focusable};
use reshiki::{
    document::{Document, Point},
    hotkeys,
    keyboard_drawing::{Direction, State, Target, connect_atoms},
};

#[derive(Debug, Clone)]
pub enum Action {
    Toggle,
    Leave,
    Navigate(Direction, bool),
    Mark,
    Connect,
}

/// Entering a drawing mode explicitly hands Enter/Space ownership back from
/// its opener button or an inspector field to the application's key router.
struct ReleaseFocus;
impl<T: 'static> Operation<T> for ReleaseFocus {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }
    fn focusable(&mut self, _: Option<&Id>, _: iced::Rectangle, state: &mut dyn Focusable) {
        state.unfocus();
    }
}

impl App {
    pub(super) fn keyboard_drawing_active(&self) -> bool {
        self.tab.keyboard_drawing.active()
            && self.tool.selects()
            && !self.keyboard_drawing_blocked()
    }

    /// All tool setters pass through update_front, including palette actions
    /// that assign a tool directly instead of publishing Message::Tool.
    pub(super) fn sync_keyboard_drawing(&mut self) {
        if self.tool.selects() {
            self.tab.keyboard_drawing.ensure_target(
                &self.tab.doc,
                &self.tab.selected,
                self.tab.camera.center,
                self.tab.file_epoch,
            );
        } else {
            self.tab.keyboard_drawing.suspend();
        }
    }

    fn keyboard_target_at(&self, point: Point) -> Target {
        let atom = reshiki::scene::atom_label_hit(&self.tab.doc, point, 0.)
            .or_else(|| self.tab.doc.nearest(point, 10. / self.tab.camera.zoom));
        if let Some(id) = atom {
            return Target::Atom(id);
        }
        reshiki::editing::nearest_bond(&self.tab.doc, point, 7. / self.tab.camera.zoom)
            .and_then(|index| self.tab.doc.bonds.get(index))
            .map_or(Target::Blank(point), |bond| {
                Target::Bond(bond.a.min(bond.b), bond.a.max(bond.b))
            })
    }

    pub(super) fn keyboard_pointer_hover(&mut self, point: Option<Point>) {
        if !self.keyboard_drawing_active() {
            return;
        }
        if let Some(point) = point {
            let target = self.keyboard_target_at(point);
            self.tab
                .keyboard_drawing
                .pointer_target(point, target, &self.tab.doc, false);
        } else {
            self.tab.keyboard_drawing.pointer_left();
        }
    }

    pub(super) fn keyboard_pointer_selection(&mut self, point: Option<Point>) {
        if !self.keyboard_drawing_active() {
            return;
        }
        let pointed = point
            .map(|point| (point, self.keyboard_target_at(point)))
            .filter(|(_, target)| match target {
                Target::Blank(_) => self.tab.selected.is_empty(),
                _ => target
                    .atoms()
                    .iter()
                    .all(|id| self.tab.selected.contains(id)),
            });
        if let Some((point, target)) = pointed {
            self.tab
                .keyboard_drawing
                .pointer_target(point, target, &self.tab.doc, true);
            return;
        }
        let target = match self.tab.selected.as_slice() {
            [id] if self.tab.doc.atom(*id).is_some() => Target::Atom(*id),
            [a, b] if Target::Bond(*a, *b).point(&self.tab.doc).is_some() => {
                Target::Bond((*a).min(*b), (*a).max(*b))
            }
            _ => self
                .tab
                .selected
                .iter()
                .copied()
                .find(|id| self.tab.doc.atom(*id).is_some())
                .map_or(
                    Target::Blank(point.unwrap_or(self.tab.camera.center)),
                    Target::Atom,
                ),
        };
        let anchor = target
            .point(&self.tab.doc)
            .unwrap_or(self.tab.camera.center);
        self.tab
            .keyboard_drawing
            .pointer_target(anchor, target, &self.tab.doc, true);
    }

    fn keyboard_drawing_blocked(&self) -> bool {
        self.help_open
            || self.updates.open
            || self.assistant.viewed_image.is_some()
            || self.tab.atom_text.is_some()
            || self.tab.inline_text.is_some()
            || self.tab.cleanup.is_some()
            || self.tab.optimization.is_some()
            || self.tab.joining.is_some()
            || self.tab.busy
    }

    pub(super) fn keyboard_drawing_action(&mut self, action: Action) -> Task<Message> {
        if matches!(action, Action::Leave)
            || matches!(action, Action::Toggle) && self.tab.keyboard_drawing.enabled()
        {
            self.tab.keyboard_drawing.leave();
            self.status = "Keyboard drawing off".into();
            self.error = false;
            return Task::none();
        }
        if self.keyboard_drawing_blocked() {
            return Task::none();
        }
        self.sync_keyboard_drawing();
        self.tab
            .keyboard_drawing
            .reconcile(&self.tab.doc, self.tab.file_epoch);
        match action {
            Action::Toggle => {
                self.tool = Tool::Select;
                self.palette = None;
                self.assistant.menu = None;
                self.context_menu = None;
                self.close_style_menu();
                self.tab.keyboard_drawing.enter(
                    &self.tab.doc,
                    &self.tab.selected,
                    self.tab.camera.center,
                    self.tab.file_epoch,
                );
                self.status = format!("Keyboard drawing · {}", State::hint());
                self.error = false;
                return iced::advanced::widget::operate(ReleaseFocus);
            }
            Action::Navigate(direction, same_kind) if self.keyboard_drawing_active() => {
                let moved =
                    self.tab
                        .keyboard_drawing
                        .move_target(&self.tab.doc, direction, same_kind);
                self.status = if moved {
                    format!(
                        "Keyboard drawing · {}",
                        self.tab.keyboard_drawing.active_label(&self.tab.doc)
                    )
                } else {
                    "No drawing hotspot in that direction".into()
                };
                self.error = false;
            }
            Action::Mark if self.keyboard_drawing_active() => {
                if let Target::Atom(id) = self.tab.keyboard_drawing.target() {
                    self.tab.keyboard_drawing.mark(id);
                    self.status =
                        format!("Marked atom {id} · Navigate to another atom, then ] to connect");
                    self.error = false;
                } else {
                    self.status = "Navigate to an atom, then [ to mark it".into();
                    self.error = false;
                }
            }
            Action::Connect if self.keyboard_drawing_active() => {
                let target = self.tab.keyboard_drawing.target();
                let result = self
                    .tab
                    .keyboard_drawing
                    .marked()
                    .zip(match target {
                        Target::Atom(id) => Some(id),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        "Mark an atom with [, then navigate to another atom and press ]".to_string()
                    })
                    .and_then(|(a, b)| connect_atoms(&self.tab.doc, a, b));
                if self.commit_hotkey(
                    result.map(|doc| (doc, target.atoms())),
                    "Connected marked atom · Undo restores the open chain",
                ) {
                    self.tab.keyboard_drawing.clear_mark();
                    self.tab.keyboard_drawing.set_target(target, &self.tab.doc);
                }
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn keyboard_context_key(&mut self, key: &str) -> Task<Message> {
        self.sync_keyboard_drawing();
        if self.keyboard_drawing_blocked() {
            return Task::none();
        }
        self.tab
            .keyboard_drawing
            .reconcile(&self.tab.doc, self.tab.file_epoch);
        if !self.keyboard_drawing_active() {
            return Task::none();
        }
        match key {
            "[" => return self.keyboard_drawing_action(Action::Mark),
            "]" => return self.keyboard_drawing_action(Action::Connect),
            _ => {}
        }
        let target = self.tab.keyboard_drawing.target();
        if let Target::Blank(point) = target {
            if key == "a" && self.aromatic_display_selection() {
                return self.update(Message::AromaticDisplay);
            }
            if let Some(result) =
                begin_drawing(&self.tab.doc, point, key, self.tab.bond_drawing.length)
            {
                match result {
                    Ok(StartedDrawing {
                        document: doc,
                        active,
                        selected,
                    }) => {
                        if self.commit_hotkey(Ok((doc, selected)), "Keyboard drawing started") {
                            self.tab.keyboard_drawing.set_target(active, &self.tab.doc);
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            } else {
                return self.empty_context_key(key);
            }
            return Task::none();
        }
        self.context_key_at(key, target)
    }
}

struct StartedDrawing {
    document: Document,
    active: Target,
    selected: Vec<u64>,
}

/// Seed and edit in one transaction: unsupported or rejected keys never leave
/// an extra carbon in the document or create an Undo entry.
fn begin_drawing(
    doc: &Document,
    point: Point,
    key: &str,
    length: f32,
) -> Option<Result<StartedDrawing, String>> {
    if hotkeys::atom_label(key).is_none()
        && ![
            "d", "+", "-", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "z", "K", "k", "v",
            "u", "a", "j", "J",
        ]
        .contains(&key)
    {
        return None;
    }
    Some((|| {
        doc.validate()?;
        if !point.x.is_finite() || !point.y.is_finite() || !length.is_finite() || length <= 0. {
            return Err("Invalid keyboard drawing position or bond length".into());
        }
        doc.next_id()
            .checked_add(1)
            .ok_or("Object ID limit exceeded")?;
        if doc.atoms.len() >= 100_000 {
            return Err("Atom limit exceeded".into());
        }
        if doc.nearest(point, length * 0.15).is_some() {
            return Err(
                "The drawing position overlaps an atom · Use arrows to navigate to it".into(),
            );
        }
        let mut seeded = doc.clone();
        let id = seeded.add_atom("C", point);
        if let Some(result) = hotkeys::atom_edit(&seeded, id, key, length) {
            return result.map(|(document, focus)| StartedDrawing {
                document,
                active: Target::Atom(focus),
                selected: vec![focus],
            });
        }
        if let Some(result) = hotkeys::ring_edit(&seeded, Some(id), None, key, length) {
            return result.map(|(document, selected)| StartedDrawing {
                document,
                active: Target::Atom(id),
                selected,
            });
        }
        Err("No drawing shortcut for that key".into())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Edit;

    fn app() -> App {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        app.tab.busy = false;
        app.tab.camera.center = Point::new(137., -83.);
        app.tab.bond_drawing.length = 42.;
        app.sync_keyboard_drawing();
        app
    }

    #[test]
    fn blank_letters_and_growth_do_not_need_hover_and_restore_cursor_with_history() {
        let mut app = app();
        assert!(
            app.keyboard_drawing_active(),
            "Select enables keyboard drawing by default"
        );
        let _ = app.context_key("n");
        let Target::Atom(start) = app.tab.keyboard_drawing.target() else {
            panic!("Active atom");
        };
        assert_eq!(app.tab.doc.atom(start).unwrap().element, "N");
        assert_eq!(
            app.tab.doc.atom(start).unwrap().position,
            Point::new(137., -83.)
        );
        assert!(app.tab.hover.is_none());
        let _ = app.context_key("1");
        let Target::Atom(end) = app.tab.keyboard_drawing.target() else {
            panic!("Growth endpoint");
        };
        assert_ne!(start, end);
        assert_eq!(app.tab.doc.atoms.len(), 2);
        let grown = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc.atoms.len(), 1);
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(start));
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, grown);
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(end));
        assert!(app.tab.hover.is_none());
    }

    #[test]
    fn new_and_loaded_tabs_enable_keyboard_drawing_and_keep_other_tabs_opted_out() {
        let mut app = app();
        let _ = app.context_key("n");
        let first_tab = app.tab.id;
        let _ = app.update(Message::KeyboardDrawing(Action::Toggle));
        assert!(!app.tab.keyboard_drawing.enabled());
        let _ = app.update(Message::New);
        assert_ne!(app.tab.id, first_tab);
        assert!(app.keyboard_drawing_active());
        assert!(matches!(
            app.tab.keyboard_drawing.target(),
            Target::Blank(_)
        ));
        let _ = app.context_key("6");
        assert_eq!(app.tab.doc.atoms.len(), 6);
        assert!(app.tab.hover.is_none());
        let _ = app.update(Message::Tabs(super::super::tabs::Action::Cycle(false)));
        assert_eq!(app.tab.id, first_tab);
        assert!(!app.tab.keyboard_drawing.enabled());

        let (mut loaded, _) = App::new();
        loaded.tab.busy = false;
        let atom = loaded.tab.doc.add_atom("O", Point::new(137., -83.));
        loaded.tab.file_epoch += 1;
        loaded.sync_keyboard_drawing();
        assert!(loaded.keyboard_drawing_active());
        assert_eq!(loaded.tab.keyboard_drawing.target(), Target::Atom(atom));
        let _ = loaded.context_key("n");
        assert_eq!(loaded.tab.doc.atom(atom).unwrap().element, "N");
        assert!(loaded.tab.hover.is_none());
    }

    #[test]
    fn f8_opt_out_survives_tool_changes_and_restores_classic_arrow_nudging() {
        use super::super::shortcuts::Action as Shortcut;
        let mut app = app();
        let _ = app.context_key("1");
        let endpoint = app.tab.selected[0];
        let _ = app.update(Message::KeyboardDrawing(Action::Toggle));
        let _ = app.update(Message::Tool(Tool::Text));
        let _ = app.update(Message::Tool(Tool::Lasso));
        assert!(!app.tab.keyboard_drawing.enabled());
        assert!(!app.keyboard_drawing_active());
        let before = app.tab.doc.atom(endpoint).unwrap().position;
        let _ = app.update(Message::Shortcut(Shortcut::Nudge(1., 0.)));
        assert_eq!(
            app.tab.doc.atom(endpoint).unwrap().position,
            before.offset(1., 0.)
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc.atom(endpoint).unwrap().position, before);
        let _ = app.update(Message::KeyboardDrawing(Action::Toggle));
        assert_eq!(app.tool, Tool::Select);
        assert!(app.keyboard_drawing_active());
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(endpoint));
        let graph = app.tab.doc.clone();
        let _ = app.update(Message::Shortcut(Shortcut::Nudge(-1., 0.)));
        assert!(matches!(
            app.tab.keyboard_drawing.target(),
            Target::Bond(_, _)
        ));
        assert_eq!(app.tab.doc, graph);
    }

    #[test]
    fn drawing_tools_clear_hotspot_and_mark_then_selection_resumes_without_reenabling() {
        let mut app = app();
        let _ = app.context_key("1");
        let atom = app.tab.selected[0];
        for tool in [
            Tool::Text,
            Tool::Bond(1),
            Tool::StyledBond(reshiki::bonds::BondPreset::Double),
            Tool::Atom,
            Tool::Erase,
            Tool::Ring,
            Tool::Template,
            Tool::Arrow,
            Tool::Tilt,
            Tool::EditPoints,
            Tool::Wedge,
            Tool::Hash,
            Tool::Wavy,
            Tool::Chain(reshiki::chains::ChainMode::Straight),
            Tool::RingPreset(reshiki::rings::Preset::Benzene),
            Tool::Graphic(reshiki::graphics::GraphicKind::Ellipse),
        ] {
            app.tab
                .keyboard_drawing
                .set_target(Target::Atom(atom), &app.tab.doc);
            app.tab.keyboard_drawing.mark(atom);
            let _ = app.update(Message::Tool(tool));
            assert!(app.tab.keyboard_drawing.enabled(), "{tool:?}");
            assert!(!app.keyboard_drawing_active(), "{tool:?}");
            assert!(
                matches!(app.tab.keyboard_drawing.target(), Target::Blank(_)),
                "{tool:?}"
            );
            assert_eq!(app.tab.keyboard_drawing.marked(), None, "{tool:?}");
            assert_eq!(
                app.tab.keyboard_drawing.marker_point(&app.tab.doc),
                None,
                "{tool:?}"
            );
            let graph = app.tab.doc.clone();
            let _ = app.keyboard_drawing_action(Action::Navigate(Direction::Left, false));
            assert_eq!(app.tab.doc, graph, "{tool:?}");
            app.tab.selected = vec![atom];
            let _ = app.update(Message::Tool(Tool::Select));
            assert!(app.keyboard_drawing_active(), "{tool:?}");
            assert_eq!(
                app.tab.keyboard_drawing.target(),
                Target::Atom(atom),
                "{tool:?}"
            );
        }
        for message in [
            Message::Element("O".into()),
            Message::Palette(super::super::palettes::Action::Ring(5, false)),
            Message::Labels(super::super::atom_labels::Action::PositionIndicators),
        ] {
            let _ = app.update(message);
            assert!(!app.keyboard_drawing_active());
            assert_eq!(app.tab.keyboard_drawing.marked(), None);
            assert!(matches!(
                app.tab.keyboard_drawing.target(),
                Target::Blank(_)
            ));
            app.tab.selected = vec![atom];
            let _ = app.update(Message::Tool(Tool::Lasso));
            assert!(app.keyboard_drawing_active());
            assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(atom));
        }
    }

    #[test]
    fn nonselection_tools_use_ordinary_context_instead_of_suspended_hotspot() {
        let mut app = app();
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.selected = vec![b];
        app.tab
            .keyboard_drawing
            .set_target(Target::Atom(a), &app.tab.doc);
        let _ = app.update(Message::Tool(Tool::Text));
        let _ = app.context_key("n");
        assert_eq!(app.tab.doc.atom(a).unwrap().element, "C");
        assert_eq!(app.tab.doc.atom(b).unwrap().element, "N");
        assert!(!app.keyboard_drawing_active());
        assert!(matches!(
            app.tab.keyboard_drawing.target(),
            Target::Blank(_)
        ));
        let _ = app.update(Message::Escape);
        assert_eq!(app.tool, Tool::Select);
        assert!(app.keyboard_drawing_active());
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    }

    #[test]
    fn unassigned_atom_and_bond_hotkeys_choose_tools_and_suspend_the_hotspot() {
        for bond_target in [false, true] {
            for (key, tool) in [
                ("t", Tool::Text),
                ("X", Tool::Chain(reshiki::chains::ChainMode::Straight)),
                ("T", Tool::Graphic(reshiki::graphics::GraphicKind::Brackets)),
                (
                    "G",
                    Tool::Graphic(reshiki::graphics::GraphicKind::Orbital(
                        reshiki::scientific::OrbitalKind::P,
                    )),
                ),
            ] {
                let mut app = app();
                let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
                let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
                app.tab.doc.add_bond(a, b, 1, "plain");
                app.tab.selected = vec![a];
                let target = if bond_target {
                    Target::Bond(a, b)
                } else {
                    Target::Atom(a)
                };
                app.tab.keyboard_drawing.set_target(target, &app.tab.doc);
                app.tab.keyboard_drawing.mark(a);
                let drawing = app.tab.doc.clone();
                let _ = app.context_key(key);
                assert_eq!(app.tool, tool, "{target:?}: {key}");
                assert!(app.tab.keyboard_drawing.enabled(), "{target:?}: {key}");
                assert!(!app.keyboard_drawing_active(), "{target:?}: {key}");
                assert!(
                    matches!(app.tab.keyboard_drawing.target(), Target::Blank(_)),
                    "{target:?}: {key}"
                );
                assert_eq!(app.tab.keyboard_drawing.marked(), None, "{target:?}: {key}");
                assert_eq!(
                    app.tab.keyboard_drawing.marker_point(&app.tab.doc),
                    None,
                    "{target:?}: {key}"
                );
                assert_eq!(app.tab.doc, drawing, "{target:?}: {key}");
                assert!(!app.tab.history.can_undo(), "{target:?}: {key}");
            }
        }
    }

    #[test]
    fn contextual_chemistry_keeps_priority_over_tool_aliases() {
        let mut app = app();
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab
            .keyboard_drawing
            .set_target(Target::Atom(a), &app.tab.doc);
        let _ = app.context_key("b");
        assert_eq!(
            app.tool,
            Tool::Select,
            "Atom b replaces with Br instead of choosing the Bond tool"
        );
        assert_eq!(app.tab.doc.atom(a).unwrap().element, "Br");
        assert!(app.keyboard_drawing_active());
        let _ = app.update(Message::Undo);
        app.tab
            .keyboard_drawing
            .set_target(Target::Bond(a, b), &app.tab.doc);
        let _ = app.context_key("b");
        assert_eq!(
            app.tool,
            Tool::Select,
            "Bond b applies Bold instead of choosing the Bond tool"
        );
        assert_eq!(
            reshiki::bonds::BondPreset::of(&app.tab.doc.bonds[0]),
            Some(reshiki::bonds::BondPreset::Bold)
        );
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Bond(a, b));
        assert!(app.keyboard_drawing_active());
    }

    #[test]
    fn escape_returns_to_selection_without_opting_out() {
        let mut app = app();
        let _ = app.context_key("n");
        let graph = app.tab.doc.clone();
        let _ = app.update(Message::Escape);
        assert!(app.tab.keyboard_drawing.enabled());
        assert!(app.keyboard_drawing_active());
        assert_eq!(app.tab.doc, graph);
        let _ = app.update(Message::Tool(Tool::Bond(1)));
        assert!(!app.keyboard_drawing_active());
        let _ = app.update(Message::Escape);
        assert_eq!(app.tool, Tool::Select);
        assert!(app.keyboard_drawing_active());
        let _ = app.update(Message::KeyboardDrawing(Action::Toggle));
        let _ = app.update(Message::Escape);
        assert!(!app.tab.keyboard_drawing.enabled());
        assert_eq!(app.tab.doc, graph);
    }

    #[test]
    fn mouse_motion_and_click_transfer_hotspot_but_keyboard_selection_does_not() {
        use super::super::shortcuts::Action as Shortcut;
        let mut app = app();
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        let pointer = app.tab.doc.atom(a).unwrap().position;
        let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer))));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
        let graph = app.tab.doc.clone();
        let _ = app.update(Message::Shortcut(Shortcut::Nudge(1., 0.)));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Bond(a, b));
        let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer))));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Bond(a, b));
        let _ = app.context_key("g");
        assert_eq!(app.tab.selected, vec![a, b]);
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Bond(a, b));
        assert_eq!(app.tab.doc, graph);
        let _ = app.update(Message::Canvas(Edit::Select(vec![a])));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
        let _ = app.update(Message::Shortcut(Shortcut::Nudge(10., 0.)));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
        let _ = app.update(Message::Canvas(Edit::Hover(Some(pointer.offset(0.5, 0.)))));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a));
        let blank = Point::new(-200., -200.);
        let _ = app.update(Message::Canvas(Edit::Hover(Some(blank))));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Blank(blank));
        let _ = app.update(Message::Canvas(Edit::Hover(None)));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Blank(blank));
        let _ = app.context_key("o");
        let Target::Atom(oxygen) = app.tab.keyboard_drawing.target() else {
            panic!("Blank pointer seed");
        };
        assert_eq!(app.tab.doc.atom(oxygen).unwrap().position, blank);
        assert_eq!(app.tab.doc.atom(oxygen).unwrap().element, "O");
        assert_eq!(app.tab.doc.bonds, graph.bonds);
    }

    #[test]
    fn exact_pointer_selection_targets_bond_and_blank_after_gesture_clears_hover() {
        let mut app = app();
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        let c = app.tab.doc.add_atom("C", Point::new(84., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.add_bond(b, c, 1, "plain");
        let _ = app.update(Message::Canvas(Edit::Hover(None)));
        let bond = Point::new(21., 0.);
        let _ = app.update(Message::Canvas(Edit::SelectAt(vec![a, b, c], bond)));
        assert_eq!(app.tab.selected, vec![a, b, c]);
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Bond(a, b));
        let _ = app.context_key("2");
        assert_eq!(app.tab.doc.bonds[0].order, 2);
        assert_eq!(app.tab.doc.bonds[1].order, 1);
        let blank = Point::new(-120., 73.);
        let _ = app.update(Message::Canvas(Edit::Hover(None)));
        let _ = app.update(Message::Canvas(Edit::SelectAt(vec![], blank)));
        assert!(app.tab.selected.is_empty());
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Blank(blank));
        let _ = app.context_key("n");
        let Target::Atom(nitrogen) = app.tab.keyboard_drawing.target() else {
            panic!("Click seed");
        };
        assert_eq!(app.tab.doc.atom(nitrogen).unwrap().position, blank);
        assert_eq!(app.tab.doc.atom(nitrogen).unwrap().element, "N");
    }

    #[test]
    fn blank_hotspot_preserves_selected_aromatic_display_action_and_atom_hotspot_attaches() {
        let mut app = app();
        app.tab.doc = reshiki::rings::Preset::Benzene.document(42., true);
        app.tab.selected = app.tab.doc.all_ids();
        let ring = app.tab.doc.clone();
        let _ = app.update(Message::Canvas(Edit::Hover(Some(Point::new(400., 400.)))));
        assert!(matches!(
            app.tab.keyboard_drawing.target(),
            Target::Blank(_)
        ));
        let task = app.context_key("a");
        assert!(
            task.units() > 0,
            "Use the existing aromatic display chemistry job"
        );
        assert!(app.tab.busy);
        assert_eq!(app.tab.doc, ring, "No unrelated ring is seeded");

        let (mut explicit, _) = App::new();
        explicit.tab.busy = false;
        explicit.tab.doc = ring;
        explicit.tab.selected = explicit.tab.doc.all_ids();
        let anchor = explicit.tab.doc.add_atom("C", Point::new(300., 0.));
        explicit
            .tab
            .keyboard_drawing
            .set_target(Target::Atom(anchor), &explicit.tab.doc);
        let before_atoms = explicit.tab.doc.atoms.len();
        let task = explicit.context_key("a");
        assert_eq!(task.units(), 0);
        assert!(!explicit.tab.busy);
        assert!(!explicit.error, "{}", explicit.status);
        assert!(explicit.tab.doc.atoms.len() > before_atoms);
        assert_eq!(explicit.tab.keyboard_drawing.target(), Target::Atom(anchor));
    }

    #[test]
    fn stationary_hover_cannot_steal_the_hotspot_and_arrows_return_for_branching() {
        let mut app = app();
        let _ = app.context_key("1");
        let Target::Atom(end) = app.tab.keyboard_drawing.target() else {
            panic!("Endpoint");
        };
        let first_bond = app.tab.doc.bonds[0].clone();
        let start = if first_bond.a == end {
            first_bond.b
        } else {
            first_bond.a
        };
        let unrelated = app.tab.doc.add_atom("O", Point::new(500., 500.));
        let pointer = app.tab.doc.atom(start).unwrap().position;
        app.edit(Edit::Hover(Some(pointer)));
        let _ = app.keyboard_drawing_action(Action::Navigate(Direction::Right, true));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(end));
        app.edit(Edit::Hover(Some(pointer)));
        let _ = app.context_key("n");
        assert_eq!(app.tab.doc.atom(end).unwrap().element, "N");
        assert_eq!(app.tab.doc.atom(unrelated).unwrap().element, "O");
        let _ = app.keyboard_drawing_action(Action::Navigate(Direction::Left, false));
        assert!(matches!(
            app.tab.keyboard_drawing.target(),
            Target::Bond(_, _)
        ));
        let _ = app.keyboard_drawing_action(Action::Navigate(Direction::Left, false));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(start));
        let _ = app.context_key("0");
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(start));
        assert_eq!(
            app.tab
                .doc
                .bonds
                .iter()
                .filter(|bond| bond.a == start || bond.b == start)
                .count(),
            2
        );
    }

    #[test]
    fn rings_keep_the_attachment_target_and_bond_targets_fuse_without_mouse() {
        let mut app = app();
        let _ = app.context_key("3");
        let Target::Atom(atom) = app.tab.keyboard_drawing.target() else {
            panic!("Ring attachment");
        };
        assert_eq!(app.tab.doc.atoms.len(), 6);
        assert_eq!(app.tab.selected.len(), 6);
        let ring = app.tab.doc.clone();
        let _ = app.keyboard_drawing_action(Action::Navigate(Direction::Left, false));
        let target = app.tab.keyboard_drawing.target();
        assert!(matches!(target, Target::Bond(_, _)));
        let _ = app.context_key("6");
        assert_eq!(app.tab.doc.atoms.len(), 10);
        assert_eq!(app.tab.keyboard_drawing.target(), target);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, ring);
        assert_eq!(app.tab.keyboard_drawing.target(), target);
        app.tab
            .keyboard_drawing
            .set_target(Target::Atom(atom), &app.tab.doc);
        let _ = app.context_key("Enter");
        assert!(
            app.tab.atom_text.is_some(),
            "Whole-ring selection must not contract the ring"
        );
    }

    #[test]
    fn ring_closure_is_atomic_and_rejects_full_valence() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::new(0., 0.));
        let b = doc.add_atom("C", Point::new(42., 0.));
        let c = doc.add_atom("C", Point::new(21., 36.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, 1, "plain");
        let before = doc.clone();
        let closed = connect_atoms(&doc, a, c).unwrap();
        assert_eq!(closed.atoms, before.atoms);
        assert_eq!(closed.bonds.len(), 3);
        assert!(connect_atoms(&closed, a, c).is_err());
        doc.atom_mut(a).unwrap().element = "F".into();
        assert!(connect_atoms(&doc, a, c).is_err());
        assert_eq!(doc.bonds.len(), 2);
    }

    #[test]
    fn mark_connect_and_undo_restore_graph_and_marked_target() {
        let mut app = app();
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        let c = app.tab.doc.add_atom("C", Point::new(21., 36.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.add_bond(b, c, 1, "plain");
        let before = app.tab.doc.clone();
        app.tab
            .keyboard_drawing
            .set_target(Target::Atom(a), &app.tab.doc);
        let _ = app.context_key("[");
        app.tab
            .keyboard_drawing
            .set_target(Target::Atom(c), &app.tab.doc);
        let _ = app.context_key("]");
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.atoms.len(), 3);
        assert_eq!(app.tab.doc.bonds.len(), 3);
        assert_eq!(app.tab.keyboard_drawing.marked(), None);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(c));
        assert_eq!(app.tab.keyboard_drawing.marked(), Some(a));
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc.bonds.len(), 3);
        assert_eq!(app.tab.keyboard_drawing.marked(), None);
    }

    #[test]
    fn unsupported_blank_key_and_modal_draft_leave_drawing_and_history_unchanged() {
        let mut app = app();
        let before = app.tab.doc.clone();
        let _ = app.context_key("t");
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        assert_eq!(app.tool, Tool::Text);
        assert!(!app.keyboard_drawing_active());
        let _ = app.update(Message::Tool(Tool::Select));
        let _ = app.context_key("n");
        let Target::Atom(id) = app.tab.keyboard_drawing.target() else {
            panic!("Active atom");
        };
        let _ = app.context_key("Enter");
        let before = app.tab.doc.clone();
        let target = app.tab.keyboard_drawing.target();
        let _ = app.context_key("1");
        let _ = app.keyboard_drawing_action(Action::Navigate(Direction::Right, false));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.keyboard_drawing.target(), target);
        assert!(app.tab.atom_text.is_some());
        let _ = app.update(Message::AtomText(super::super::atom_text::Action::Cancel));
        assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(id));
    }

    #[test]
    fn ignored_key_dispatcher_keeps_modifier_commands_and_default_nudge() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let raw = Key::Named(Named::F8);
        assert!(matches!(
            super::super::shortcuts::key_message(&raw, &raw, Modifiers::empty()),
            Some(Message::KeyboardDrawing(Action::Toggle))
        ));
        assert!(super::super::shortcuts::key_message(&raw, &raw, Modifiers::ALT).is_none());
        let mut app = app();
        let _ = app.context_key("1");
        let before = app.tab.doc.clone();
        let active = app.tab.keyboard_drawing.target();
        let left = Key::Named(Named::ArrowLeft);
        let message =
            super::super::shortcuts::key_message(&left, &left, Modifiers::empty()).unwrap();
        let _ = app.update(message);
        assert_ne!(app.tab.keyboard_drawing.target(), active);
        assert_eq!(app.tab.doc, before);
        let _ = app.keyboard_drawing_action(Action::Leave);
        app.tab.selected = app.tab.doc.all_ids();
        let message =
            super::super::shortcuts::key_message(&left, &left, Modifiers::empty()).unwrap();
        let _ = app.update(message);
        assert_ne!(app.tab.doc, before);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let command = Modifiers::COMMAND;
        let right = Key::Named(Named::ArrowRight);
        assert!(matches!(
            super::super::shortcuts::key_message(&right, &right, command | Modifiers::SHIFT),
            Some(Message::Shortcut(
                super::super::shortcuts::Action::ReactionCopy
            ))
        ));
        assert!(matches!(
            super::super::shortcuts::key_message(&left, &left, Modifiers::ALT | Modifiers::SHIFT),
            Some(Message::Transform(reshiki::editing::Transform::TiltY(_)))
        ));
    }

    #[tokio::test]
    #[ignore = "Opt-in real renderer semantics and compact keyboard bar check"]
    async fn keyboard_bar_publishes_native_actions_and_fits_compact_canvas() {
        use iced::advanced::{
            layout,
            renderer::Headless,
            widget::{Operation, Tree, operation},
        };
        use iced::keyboard::{
            self, Key, Modifiers,
            key::{Code, Named, Physical},
        };
        use iced::widget::{Space, column};
        use iced::{Element, Event, Length, Rectangle, Size, mouse};
        use reshiki::accessibility::{
            Activate, Collect, Node, Role, Snapshot,
            tree::{NativeTree, Request},
        };

        fn view(app: &App) -> Element<'_, Message> {
            reshiki::accessibility::focus_scope(
                app.with_context_menu(
                    column![app.context_bar(), Space::new().height(Length::Fill)]
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into(),
                ),
            )
        }

        struct BarUi {
            renderer: iced::Renderer,
            cache: iced_runtime::user_interface::Cache,
            size: Size,
        }
        impl BarUi {
            async fn new(width: f32) -> Self {
                Self {
                    renderer: <iced::Renderer as Headless>::new(
                        iced::Font::with_name(reshiki::style::ui_font_family()),
                        iced::Pixels(16.),
                        None,
                    )
                    .await
                    .unwrap(),
                    cache: iced_runtime::user_interface::Cache::new(),
                    size: Size::new(width, 512.),
                }
            }

            fn operate<T: 'static>(&mut self, app: &App, operation: &mut dyn Operation<T>) {
                let mut ui = iced_runtime::UserInterface::build(
                    view(app),
                    self.size,
                    std::mem::take(&mut self.cache),
                    &mut self.renderer,
                );
                ui.operate(&self.renderer, &mut operation::black_box(operation));
                self.cache = ui.into_cache();
            }

            fn snapshot(&mut self, app: &App) -> Snapshot {
                let mut collect = Collect::new(Rectangle::with_size(self.size));
                self.operate(app, &mut collect);
                let snapshot = collect.snapshot().clone();
                assert!(snapshot.duplicate_ids.is_empty());
                snapshot
            }

            fn activate(&mut self, app: &App, id: &str) -> Option<Message> {
                let mut activate = Activate::<Message>::new(id);
                self.operate(app, &mut activate);
                match activate.finish() {
                    operation::Outcome::Some(message) => Some(message),
                    operation::Outcome::None => None,
                    _ => panic!("{id}: activation must produce one current action"),
                }
            }

            fn event(&mut self, app: &App, named: Named) -> (iced::event::Status, Vec<Message>) {
                let event = Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(named),
                    modified_key: Key::Named(named),
                    physical_key: Physical::Code(if named == Named::Tab {
                        Code::Tab
                    } else {
                        Code::Enter
                    }),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::empty(),
                    text: None,
                    repeat: false,
                });
                let mut ui = iced_runtime::UserInterface::build(
                    view(app),
                    self.size,
                    std::mem::take(&mut self.cache),
                    &mut self.renderer,
                );
                let mut messages = Vec::new();
                let (_, statuses) = ui.update(
                    &[event],
                    mouse::Cursor::Unavailable,
                    &mut self.renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut messages,
                );
                self.cache = ui.into_cache();
                (statuses[0], messages)
            }

            fn row(&mut self, app: &App) -> Snapshot {
                assert!(app.context_menu.is_none());
                let mut bar = app.context_bar();
                let mut tree = Tree::new(bar.as_widget());
                let node = bar.as_widget_mut().layout(
                    &mut tree,
                    &self.renderer,
                    &layout::Limits::new(Size::ZERO, self.size),
                );
                assert!(
                    (node.size().height - 46.).abs() < 0.01,
                    "{} px: one 46px context row, got {:?}",
                    self.size.width,
                    node.size()
                );
                let snapshot = self.snapshot(app);
                let indicator = snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == "keyboard-done")
                    .unwrap();
                let active = app.tab.keyboard_drawing.active_label(&app.tab.doc);
                assert_eq!(indicator.role, Role::ToggleButton);
                assert_eq!(indicator.checked, Some(true));
                assert_eq!(indicator.value.as_deref(), Some(active.as_str()));
                assert!(indicator.name.contains(&active));
                if let Some(marked) = app.tab.keyboard_drawing.marked() {
                    assert!(indicator.name.contains(&format!("Marked atom {marked}")));
                }
                for (index, control) in snapshot.nodes.iter().enumerate() {
                    assert_visible(control, self.size);
                    assert!(
                        control.bounds.y >= 0. && control.bounds.y + control.bounds.height <= 46.01,
                        "{} leaves the single row: {control:?}",
                        control.id
                    );
                    assert!(
                        (control.bounds.center_y() - 23.).abs() < 0.5,
                        "{} wraps onto another line: {control:?}",
                        control.id
                    );
                    for other in snapshot.nodes.iter().skip(index + 1) {
                        assert!(
                            control.bounds.intersection(&other.bounds).is_none(),
                            "{} overlaps {}",
                            control.id,
                            other.id
                        );
                    }
                }
                snapshot
            }

            fn expose(&mut self, app: &mut App, id: &str) -> bool {
                if app.context_menu.is_some() {
                    let _ = app.update(Message::ContextMenu(
                        super::super::context_menu::Action::Close,
                    ));
                }
                let snapshot = self.snapshot(app);
                if snapshot.nodes.iter().any(|node| node.id == id) {
                    return false;
                }
                let more = self
                    .activate(app, "context-overflow")
                    .expect("Folded keyboard control has a real More anchor");
                assert!(matches!(
                    more,
                    Message::ContextMenu(super::super::context_menu::Action::Open(
                        super::super::context_menu::Page::More(_),
                        _
                    ))
                ));
                let _ = app.update(more);
                assert!(app.context_menu.is_some());
                let popup = self.snapshot(app);
                assert!(
                    popup.nodes.iter().any(|node| node.id == id),
                    "{id} missing from actual More popup: {popup:?}"
                );
                true
            }

            fn control(
                &mut self,
                app: &mut App,
                id: &str,
                enabled: bool,
                hint: Option<&str>,
            ) -> (bool, Option<Message>) {
                let folded = self.expose(app, id);
                let snapshot = self.snapshot(app);
                let control = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                assert_eq!(
                    control.role,
                    if id == "keyboard-done" {
                        Role::ToggleButton
                    } else {
                        Role::Button
                    }
                );
                assert_eq!(control.enabled, enabled, "{id}: {control:?}");
                if let Some(hint) = hint {
                    assert_eq!(
                        control.name, hint,
                        "{id}: inline and folded controls keep the same state-specific explanation"
                    );
                }
                assert_visible(control, self.size);
                let mut native = NativeTree::default();
                let native_tree = native
                    .update(&snapshot, "ReShiki", Rectangle::with_size(self.size), 2.)
                    .unwrap();
                let native_id = native_tree
                    .nodes
                    .iter()
                    .find(|(_, node)| node.author_id() == Some(id))
                    .unwrap()
                    .0;
                assert_eq!(
                    native.resolve(&accesskit::ActionRequest {
                        action: accesskit::Action::Click,
                        target_tree: accesskit::TreeId::ROOT,
                        target_node: native_id,
                        data: None,
                    }),
                    enabled.then(|| Request::Activate(id.into()))
                );
                let message = self.activate(app, id);
                if enabled {
                    match (&message, id, folded) {
                        (Some(Message::KeyboardDrawing(Action::Mark)), "keyboard-mark", false)
                        | (
                            Some(Message::KeyboardDrawing(Action::Connect)),
                            "keyboard-connect",
                            false,
                        )
                        | (Some(Message::KeyboardDrawing(Action::Leave)), "keyboard-done", false)
                        | (
                            Some(Message::ContextMenu(
                                super::super::context_menu::Action::Activate(_, _),
                            )),
                            "keyboard-mark" | "keyboard-connect",
                            true,
                        ) => {}
                        _ => panic!("{id}: wrong live dispatch: {message:?}"),
                    }
                } else {
                    assert!(
                        message.is_none(),
                        "{id}: a disabled native control must publish no action"
                    );
                }
                (folded, message)
            }

            fn tab_enter(&mut self, app: &mut App, id: &str) -> Message {
                self.expose(app, id);
                let count = self.snapshot(app).nodes.len();
                for _ in 0..=count {
                    let (status, messages) = self.event(app, Named::Tab);
                    assert_eq!(status, iced::event::Status::Captured);
                    assert!(
                        messages.is_empty(),
                        "Tab only changes real focus: {messages:?}"
                    );
                    let snapshot = self.snapshot(app);
                    if snapshot
                        .nodes
                        .iter()
                        .any(|node| node.id == id && node.focused)
                    {
                        let (status, mut messages) = self.event(app, Named::Enter);
                        assert_eq!(status, iced::event::Status::Captured);
                        assert_eq!(
                            messages.len(),
                            1,
                            "Enter dispatches exactly one current action: {messages:?}"
                        );
                        return messages.remove(0);
                    }
                }
                panic!("Tab cannot reach the enabled {id}");
            }
        }

        fn assert_visible(control: &Node, size: Size) {
            let visible = control
                .visible_bounds
                .expect("Current control must be visible");
            assert!(
                (visible.width - control.bounds.width).abs() < 0.5
                    && (visible.height - control.bounds.height).abs() < 0.5,
                "{} clips at {} px: {control:?}",
                control.id,
                size.width
            );
            let viewport = Rectangle::with_size(size);
            assert!(
                viewport.contains(control.bounds.position())
                    && viewport.contains(iced::Point::new(
                        control.bounds.x + control.bounds.width,
                        control.bounds.y + control.bounds.height
                    )),
                "{} leaves the viewport: {control:?}",
                control.id
            );
        }

        fn triangle() -> (App, u64, u64, u64) {
            let mut app = app();
            let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
            let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
            let c = app.tab.doc.add_atom("C", Point::new(21., 36.));
            app.tab.doc.add_bond(a, b, 1, "plain");
            app.tab.doc.add_bond(b, c, 1, "plain");
            app.tab.selected = vec![a];
            app.tab
                .keyboard_drawing
                .set_target(Target::Atom(a), &app.tab.doc);
            (app, a, b, c)
        }

        let mut saw_folded_mark = false;
        let mut saw_folded_connect = false;
        for width in [636., 884.] {
            let mut ui = BarUi::new(width).await;
            for state in [
                "blank",
                "atom",
                "bond",
                "marked same",
                "marked bond",
                "marked different",
                "long label",
            ] {
                let (mut app, a, b, c) = triangle();
                match state {
                    "blank" => {
                        app.tab.doc = Document::default();
                        app.tab.selected.clear();
                        app.tab
                            .keyboard_drawing
                            .set_target(Target::Blank(app.tab.camera.center), &app.tab.doc);
                    }
                    "bond" | "marked bond" => {
                        app.tab.selected = vec![a, b];
                        app.tab
                            .keyboard_drawing
                            .set_target(Target::Bond(a, b), &app.tab.doc);
                        if state == "marked bond" {
                            app.tab.keyboard_drawing.mark(a);
                        }
                    }
                    "marked same" => app.tab.keyboard_drawing.mark(a),
                    "marked different" => {
                        app.tab.selected = vec![c];
                        app.tab
                            .keyboard_drawing
                            .set_target(Target::Atom(c), &app.tab.doc);
                        app.tab.keyboard_drawing.mark(a);
                    }
                    "long label" => {
                        let label = "W".repeat(32);
                        app.tab
                            .doc
                            .abbreviations
                            .push(reshiki::abbreviations::Abbreviation {
                                label_style: None,
                                label_color_override: false,
                                highlight: None,
                                label,
                                reverse_label: String::new(),
                                anchor: b,
                                members: vec![a, b],
                                alignment: Default::default(),
                            });
                        app.tab.doc.validate().unwrap();
                        app.tab.selected = vec![a, b];
                        app.tab
                            .keyboard_drawing
                            .set_target(Target::Atom(b), &app.tab.doc);
                        app.tab.keyboard_drawing.mark(b);
                    }
                    _ => {}
                }
                let drawing = app.tab.doc.clone();
                let target = app.tab.keyboard_drawing.target();
                let marked = app.tab.keyboard_drawing.marked();
                let row = ui.row(&app);
                if state == "long label" {
                    assert!(
                        row.nodes
                            .iter()
                            .find(|node| node.id == "keyboard-done")
                            .unwrap()
                            .value
                            .as_ref()
                            .unwrap()
                            .contains(&"W".repeat(32)),
                        "Full long label remains in native value despite compact visible summary"
                    );
                }
                let commands = app.context_commands();
                for (id, enabled) in [
                    ("keyboard-mark", matches!(target, Target::Atom(_))),
                    (
                        "keyboard-connect",
                        matches!(target, Target::Atom(atom) if marked.is_some_and(|other| other != atom)),
                    ),
                ] {
                    let command = commands
                        .iter()
                        .find(|command| {
                            super::super::workspace::keyboard_control_id(&command.message)
                                == Some(id)
                        })
                        .unwrap();
                    assert_eq!(command.enabled, enabled, "{state}: {id}");
                    if !enabled {
                        let explanation = if id == "keyboard-mark" {
                            "Choose an atom"
                        } else if marked.is_none() {
                            "Mark an atom"
                        } else if !matches!(target, Target::Atom(_)) {
                            "Choose an atom"
                        } else {
                            "Choose a different atom"
                        };
                        assert!(
                            command.hint.contains(explanation),
                            "{state}: {}",
                            command.hint
                        );
                    }
                    let (folded, _) = ui.control(&mut app, id, enabled, Some(command.hint));
                    if id == "keyboard-mark" {
                        saw_folded_mark |= folded;
                    } else {
                        saw_folded_connect |= folded;
                    }
                }
                let _ = ui.control(&mut app, "keyboard-done", true, None);
                let _ = app.update(Message::ContextMenu(
                    super::super::context_menu::Action::Close,
                ));
                assert_eq!(
                    app.tab.doc, drawing,
                    "{width}/{state}: inspecting native actions does not edit the drawing"
                );
                assert_eq!(app.tab.keyboard_drawing.target(), target);
                assert_eq!(app.tab.keyboard_drawing.marked(), marked);
            }

            for keyboard in [false, true] {
                let (mut app, a, _, c) = triangle();
                let before = app.tab.doc.clone();
                let mark = if keyboard {
                    ui.tab_enter(&mut app, "keyboard-mark")
                } else {
                    ui.control(&mut app, "keyboard-mark", true, None).1.unwrap()
                };
                let _ = app.update(mark);
                assert_eq!(app.tab.keyboard_drawing.marked(), Some(a));
                assert_eq!(app.tab.doc, before);
                assert!(app.context_menu.is_none());
                app.tab.selected = vec![c];
                app.tab
                    .keyboard_drawing
                    .set_target(Target::Atom(c), &app.tab.doc);
                let connect = if keyboard {
                    ui.tab_enter(&mut app, "keyboard-connect")
                } else {
                    ui.control(&mut app, "keyboard-connect", true, None)
                        .1
                        .unwrap()
                };
                let _ = app.update(connect);
                assert_eq!(app.tab.doc.atoms.len(), 3);
                assert_eq!(app.tab.doc.bonds.len(), 3);
                assert_eq!(app.tab.keyboard_drawing.marked(), None);
                assert!(app.context_menu.is_none());
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, before);
                assert_eq!(app.tab.keyboard_drawing.marked(), Some(a));
                let _ = app.update(Message::Redo);
                assert_eq!(app.tab.doc.bonds.len(), 3);
                assert_eq!(app.tab.keyboard_drawing.marked(), None);
                let done = if keyboard {
                    ui.tab_enter(&mut app, "keyboard-done")
                } else {
                    ui.control(&mut app, "keyboard-done", true, None).1.unwrap()
                };
                let _ = app.update(done);
                assert!(!app.tab.keyboard_drawing.enabled());
                assert_eq!(app.tab.doc.bonds.len(), 3);
            }
        }
        assert!(
            saw_folded_mark,
            "Exercise Mark through the actual responsive More popup"
        );
        assert!(
            saw_folded_connect,
            "Exercise Connect through the actual responsive More popup"
        );
    }
}
