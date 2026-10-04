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
            Action::Navigate(direction, same_kind) if self.tab.keyboard_drawing.enabled() => {
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
            Action::Mark if self.tab.keyboard_drawing.enabled() => {
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
            Action::Connect if self.tab.keyboard_drawing.enabled() => {
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
        if self.keyboard_drawing_blocked() {
            return Task::none();
        }
        self.tab
            .keyboard_drawing
            .reconcile(&self.tab.doc, self.tab.file_epoch);
        if !self.tab.keyboard_drawing.enabled() {
            return Task::none();
        }
        match key {
            "[" => return self.keyboard_drawing_action(Action::Mark),
            "]" => return self.keyboard_drawing_action(Action::Connect),
            _ => {}
        }
        let target = self.tab.keyboard_drawing.target();
        if let Target::Blank(point) = target {
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
                self.status =
                    "Type an atom letter, 1 for a bond, or 3/6/7 for a ring to begin".into();
                self.error = false;
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
        let _ = app.keyboard_drawing_action(Action::Toggle);
        app
    }

    #[test]
    fn blank_letters_and_growth_do_not_need_hover_and_restore_cursor_with_history() {
        let mut app = app();
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
        app.edit(Edit::Hover(Some(Point::new(500., 500.))));
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
            Layout, layout,
            renderer::Headless,
            widget::{Operation, Tree, operation},
        };
        use iced::{Rectangle, Size};
        use reshiki::accessibility::{
            Activate, Collect, Role,
            tree::{NativeTree, Request},
        };

        fn check_bar(
            app: &App,
            renderer: &iced::Renderer,
            size: Size,
            mark: bool,
            connect: bool,
        ) -> Vec<Message> {
            let viewport = Rectangle::with_size(size);
            let mut bar = app.context_bar();
            let mut tree = Tree::new(bar.as_widget());
            let layout = bar.as_widget_mut().layout(
                &mut tree,
                renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            let mut collect = Collect::new(viewport);
            bar.as_widget_mut().operate(
                &mut tree,
                Layout::new(&layout),
                renderer,
                &mut operation::black_box(&mut collect),
            );
            let snapshot = collect.snapshot();
            assert!(snapshot.duplicate_ids.is_empty());
            assert_eq!(snapshot.nodes.len(), 3);
            let active = app.tab.keyboard_drawing.active_label(&app.tab.doc);
            let mut native = NativeTree::default();
            let native_tree = native.update(snapshot, "ReShiki", viewport, 2.).unwrap();
            let mut messages = Vec::new();
            for (id, enabled) in [
                ("keyboard-mark", mark),
                ("keyboard-connect", connect),
                ("keyboard-done", true),
            ] {
                let control = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                assert_eq!(control.role, Role::Button);
                assert_eq!(control.enabled, enabled, "{id}");
                assert!(control.name.contains(&active), "{id}: {}", control.name);
                let visible = control.visible_bounds.unwrap();
                assert!(
                    (visible.width - control.bounds.width).abs() < 0.5
                        && (visible.height - control.bounds.height).abs() < 0.5,
                    "{id} clips at {} px: {control:?}",
                    size.width
                );
                assert!(
                    viewport.contains(iced::Point::new(control.bounds.x, control.bounds.y))
                        && viewport.contains(iced::Point::new(
                            control.bounds.x + control.bounds.width,
                            control.bounds.y + control.bounds.height
                        )),
                    "{id}: {control:?}"
                );
                let native_id = native_tree
                    .nodes
                    .iter()
                    .find(|(_, node)| node.author_id() == Some(id))
                    .unwrap()
                    .0;
                let request = accesskit::ActionRequest {
                    action: accesskit::Action::Click,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: native_id,
                    data: None,
                };
                assert_eq!(
                    native.resolve(&request),
                    enabled.then(|| Request::Activate(id.into()))
                );
                let mut activate = Activate::<Message>::new(id);
                bar.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&layout),
                    renderer,
                    &mut operation::black_box(&mut activate),
                );
                match activate.finish() {
                    operation::Outcome::Some(message @ Message::KeyboardDrawing(_)) if enabled => {
                        assert!(matches!(
                            (&message, id),
                            (Message::KeyboardDrawing(Action::Mark), "keyboard-mark")
                                | (
                                    Message::KeyboardDrawing(Action::Connect),
                                    "keyboard-connect"
                                )
                                | (Message::KeyboardDrawing(Action::Leave), "keyboard-done")
                        ));
                        messages.push(message);
                    }
                    operation::Outcome::None if !enabled => {}
                    _ => panic!("{id} must expose exactly its current enabled action"),
                }
            }
            for (index, control) in snapshot.nodes.iter().enumerate() {
                for other in snapshot.nodes.iter().skip(index + 1) {
                    assert!(
                        control.bounds.intersection(&other.bounds).is_none(),
                        "{} overlaps {}",
                        control.id,
                        other.id
                    );
                }
            }
            let done = snapshot
                .nodes
                .iter()
                .find(|node| node.id == "keyboard-done")
                .unwrap();
            if let Some(marked) = app.tab.keyboard_drawing.marked() {
                assert!(
                    done.name.contains(&format!("Marked atom {marked}")),
                    "{}",
                    done.name
                );
            }
            messages
        }

        let renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        for width in [636., 884.] {
            let size = Size::new(width, 512.);
            let mut app = app();
            let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
            let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
            let c = app.tab.doc.add_atom("C", Point::new(21., 36.));
            app.tab.doc.add_bond(a, b, 1, "plain");
            app.tab.doc.add_bond(b, c, 1, "plain");
            let before = app.tab.doc.clone();
            check_bar(&app, &renderer, size, false, false);
            app.tab
                .keyboard_drawing
                .set_target(Target::Atom(a), &app.tab.doc);
            let actions = check_bar(&app, &renderer, size, true, false);
            let mark = actions
                .into_iter()
                .find(|message| matches!(message, Message::KeyboardDrawing(Action::Mark)))
                .unwrap();
            let _ = app.update(mark);
            assert_eq!(app.tab.keyboard_drawing.marked(), Some(a));
            assert_eq!(app.tab.doc, before);
            check_bar(&app, &renderer, size, true, false);
            app.tab
                .keyboard_drawing
                .set_target(Target::Bond(a, b), &app.tab.doc);
            check_bar(&app, &renderer, size, false, false);
            app.tab
                .keyboard_drawing
                .set_target(Target::Atom(c), &app.tab.doc);
            let actions = check_bar(&app, &renderer, size, true, true);
            let connect = actions
                .into_iter()
                .find(|message| matches!(message, Message::KeyboardDrawing(Action::Connect)))
                .unwrap();
            let _ = app.update(connect);
            assert_eq!(app.tab.doc.atoms.len(), 3);
            assert_eq!(app.tab.doc.bonds.len(), 3);
            assert_eq!(app.tab.keyboard_drawing.marked(), None);
            let actions = check_bar(&app, &renderer, size, true, false);
            let done = actions
                .into_iter()
                .find(|message| matches!(message, Message::KeyboardDrawing(Action::Leave)))
                .unwrap();
            let _ = app.update(done);
            assert!(!app.tab.keyboard_drawing.enabled());
        }
    }
}
