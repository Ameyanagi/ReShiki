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
    Coordinate,
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
            connect @ (Action::Connect | Action::Coordinate) if self.keyboard_drawing_active() => {
                let coordinate = matches!(connect, Action::Coordinate);
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
                    .and_then(|(a, b)| {
                        if coordinate {
                            reshiki::templates::coordinate_atoms(&self.tab.doc, a, b)
                        } else {
                            connect_atoms(&self.tab.doc, a, b)
                        }
                    });
                let unchanged = result.as_ref().is_ok_and(|doc| *doc == self.tab.doc);
                if self.commit_hotkey(
                    result.map(|doc| (doc, target.atoms())),
                    if coordinate && unchanged {
                        "This marked donor is already coordinated to that metal"
                    } else if coordinate {
                        "Marked donor → metal contact added in place · Undo removes it"
                    } else {
                        "Connected marked atom · Undo restores the open chain"
                    },
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
            "}" => return self.keyboard_drawing_action(Action::Coordinate),
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
mod tests;
