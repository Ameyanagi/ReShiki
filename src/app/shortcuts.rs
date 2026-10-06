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

#[cfg(test)]
mod atom_target_tests;
#[cfg(test)]
mod ring_tool_tests;

#[derive(Debug, Clone)]
pub enum Action {
    ReactionCopy,
    SelectRecent,
    SelectRing(u8),
    FixedLength,
    FixedAngles,
    Rulers,
    Crosshair,
    Nudge(f32, f32),
    Join,
    CopyText(&'static str),
}

/// The shortcut of a menu or context row command, as `keys` shows it.
pub(super) fn label(message: &Message) -> Option<String> {
    binding(message).map(|(mods, key)| keys(mods, key))
}

/// A shortcut as menus, tooltips and Help show it: key symbols on macOS
/// (⇧R, ⌘G, ⌥⌘←, ↩, ⌫) and words elsewhere (Shift+R, Ctrl+G, Ctrl+Alt+Left,
/// Enter, Delete). An empty key gives the modifiers alone, as in "⌥ drag".
/// `Modifiers::COMMAND` is ⌘ on macOS and Ctrl elsewhere.
pub(super) fn keys(mods: Modifiers, key: &str) -> String {
    let mac = cfg!(target_os = "macos");
    let key = match key {
        "Enter" if mac => "↩",
        "Delete" if mac => "⌫",
        "Left" if mac => "←",
        "Right" if mac => "→",
        "Up" if mac => "↑",
        "Down" if mac => "↓",
        key => key,
    };
    if mac {
        [
            (mods.control(), "⌃"),
            (mods.alt(), "⌥"),
            (mods.shift(), "⇧"),
            (mods.logo(), "⌘"),
        ]
        .into_iter()
        .filter_map(|(held, symbol)| held.then_some(symbol))
        .chain([key])
        .collect()
    } else {
        [
            (mods.control(), "Ctrl"),
            (mods.alt(), "Alt"),
            (mods.shift(), "Shift"),
        ]
        .into_iter()
        .filter_map(|(held, name)| held.then_some(name))
        .chain((!key.is_empty()).then_some(key))
        .collect::<Vec<_>>()
        .join("+")
    }
}

/// `keys` as rich text spans, with the macOS key symbols in Lucida Grande:
/// the interface font lacks ⌥, ⌫ and ↩ and draws ⇧ as a hairline.
pub(super) fn spans(keys: &str) -> Vec<iced::widget::text::Span<'static>> {
    let mut spans = vec![];
    let mut rest = keys;
    while let Some(first) = rest.chars().next() {
        let symbols = symbol(first);
        let end = rest.find(|c| symbol(c) != symbols).unwrap_or(rest.len());
        let run = iced::widget::span(rest[..end].to_owned());
        spans.push(if symbols {
            run.font(iced::Font::with_name("Lucida Grande"))
        } else {
            run
        });
        rest = &rest[end..];
    }
    spans
}

/// A macOS key symbol that `spans` draws in the shortcut font.
pub(super) fn symbol(c: char) -> bool {
    matches!(c, '⌘' | '⇧' | '⌥' | '⌃' | '⌫' | '↩')
}

/// Modifiers and key of the commands that have a shortcut in `key_message`,
/// or in `file_shortcuts` for the file commands.
fn binding(message: &Message) -> Option<(Modifiers, &'static str)> {
    let command = Modifiers::COMMAND;
    let shift = command | Modifiers::SHIFT;
    Some(match message {
        Message::New => (command, "N"),
        Message::Open => (command, "O"),
        Message::Save => (command, "S"),
        Message::SaveAs => (shift, "S"),
        Message::Printing(super::printing::Action::Start(reshiki::printing::Scope::Document)) => {
            (command, "P")
        }
        Message::Undo => (command, "Z"),
        Message::Redo => (shift, "Z"),
        Message::Copy(cut) => (command, if *cut { "X" } else { "C" }),
        Message::Paste => (command, "V"),
        Message::CopyImage => (shift, "C"),
        Message::Optimization(super::optimization::Action::Begin) => (shift, "D"),
        Message::SelectAll => (command, "A"),
        Message::InvertSelection => (shift, "A"),
        Message::Group => (command, "G"),
        Message::Ungroup => (shift, "G"),
        Message::Fit => (command, "/"),
        Message::BondDepth(front) => (command, if *front { "]" } else { "[" }),
        Message::Transform(Transform::FlipHorizontal) => (shift, "V"),
        Message::Transform(Transform::FlipVertical) => (shift, "H"),
        Message::Arrange(arrange) => (
            shift | Modifiers::ALT,
            match arrange {
                Arrange::AlignLeft => "L",
                Arrange::AlignVertical => "C",
                Arrange::AlignRight => "R",
                Arrange::AlignTop => "T",
                Arrange::AlignHorizontal => "M",
                Arrange::AlignBottom => "B",
                Arrange::DistributeHorizontal => "H",
                Arrange::DistributeVertical => "V",
            },
        ),
        Message::Delete => (Modifiers::empty(), "Delete"),
        Message::KeyboardDrawing(super::keyboard_drawing::Action::Toggle) => {
            (Modifiers::empty(), "F8")
        }
        Message::Shortcut(Action::SelectRing(size)) => (
            Modifiers::SHIFT,
            match size {
                3 => "3",
                4 => "4",
                5 => "5",
                6 => "6",
                7 => "7",
                8 => "8",
                _ => return None,
            },
        ),
        // Keys for the selected atom or ring, through `App::context_key`.
        Message::AtomText(super::atom_text::Action::Begin(None)) => (Modifiers::empty(), "Enter"),
        Message::ToggleSelectedRing => (Modifiers::SHIFT, "R"),
        _ => return None,
    })
}

/// Called only after focused widgets have had an opportunity to capture the key.
/// Character hotkeys use the modified character (including Caps Lock), except
/// Shift+digit ring selectors, which use the digit before Shift adds punctuation.
pub(super) fn key_message(key: &Key, modified: &Key, mods: Modifiers) -> Option<Message> {
    if super::help::is_shortcut(key, mods) {
        return Some(Message::ToggleHelp);
    }
    if mods.is_empty() && matches!(key, Key::Named(Named::F8)) {
        return Some(Message::KeyboardDrawing(
            super::keyboard_drawing::Action::Toggle,
        ));
    }
    if mods.command() {
        if mods.shift() && !mods.alt() && matches!(key, Key::Named(Named::ArrowRight)) {
            return Some(Message::Shortcut(Action::ReactionCopy));
        }
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
                "k" => Message::Cleanup(super::cleanup::Action::Begin),
                "h" => Message::Transform(Transform::FlipVertical),
                "v" => Message::Transform(Transform::FlipHorizontal),
                "d" => Message::Optimization(super::optimization::Action::Begin),
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
            "i" => Message::Inspector(InspectorTab::Import),
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
    if mods == Modifiers::SHIFT
        && let Key::Character(digit) = key
        && let [digit @ b'3'..=b'8'] = digit.as_bytes()
    {
        return Some(Message::Shortcut(Action::SelectRing(*digit - b'0')));
    }
    let step = if mods.shift() { 10. } else { 1. };
    Some(match modified {
        Key::Character(c) => Message::ContextKey(c.to_string()),
        Key::Named(Named::Space) if mods.is_empty() => Message::Shortcut(Action::SelectRecent),
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
            Action::ReactionCopy => self.reaction_copy_shortcut(),
            Action::SelectRecent => self.select_recent_shortcut(),
            Action::SelectRing(size) => {
                if (3..=8).contains(&size) {
                    return self
                        .update(Message::Palette(super::palettes::Action::Ring(size, false)));
                }
            }
            Action::FixedLength => {
                return self.update(Message::FixedLength(!self.tab.bond_drawing.fixed_length));
            }
            Action::FixedAngles => {
                return self.update(Message::FixedAngles(!self.tab.bond_drawing.fixed_angles));
            }
            Action::Rulers => return self.update(Message::Rulers(!self.guides.rulers)),
            Action::Crosshair => return self.update(Message::Crosshair(!self.guides.crosshair)),
            Action::Nudge(x, y) => {
                self.sync_keyboard_drawing();
                if self.tool.selects()
                    && self.tab.keyboard_drawing.active()
                    && let Some(direction) = reshiki::keyboard_drawing::Direction::from_delta(x, y)
                {
                    return self.keyboard_drawing_action(
                        super::keyboard_drawing::Action::Navigate(
                            direction,
                            x.abs().max(y.abs()) > 1.,
                        ),
                    );
                }
                let before = self.tab.doc.clone();
                let ids = if self.tool == Tool::EditPoints {
                    self.tab.selected.clone()
                } else {
                    reshiki::attachments::movement_selection(&self.tab.doc, &self.tab.selected)
                };
                self.tab.doc.translate(&ids, x, y);
                self.changed(before);
            }
            Action::Join => {
                let result = self.join_shortcut();
                self.commit_hotkey(result, "Joined selected attachment sites");
            }
            Action::CopyText(format) => {
                if self.tab.selected.is_empty() {
                    self.status = "Select a structure to copy".into();
                    return Task::none();
                }
                if let Some(format) = reshiki::clipboard::CopyFormat::ALL
                    .into_iter()
                    .find(|candidate| candidate.code() == format)
                {
                    return self.copy_as(format);
                }
                self.status = "Unsupported copy format".into();
                self.error = true;
            }
        }
        Task::none()
    }

    fn join_shortcut(&self) -> Result<(Document, Vec<u64>), String> {
        use reshiki::{
            joining::Prepared,
            templates::{Anchor, Connection},
        };
        if let [source, target] = self.tab.selected.as_slice() {
            let prepared = Prepared::new(&self.tab.doc, &[*source])?;
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
        if self.tab.selected.len() == 4 {
            let bonds: Vec<_> = self
                .tab
                .doc
                .bonds
                .iter()
                .filter(|b| self.tab.selected.contains(&b.a) && self.tab.selected.contains(&b.b))
                .collect();
            if let [source, target] = bonds.as_slice() {
                let prepared = Prepared::new(&self.tab.doc, &[source.a, source.b])?;
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

    pub(super) fn commit_hotkey(
        &mut self,
        result: Result<(Document, Vec<u64>), String>,
        status: &str,
    ) -> bool {
        match result {
            Ok((doc, selected)) => {
                let before = std::mem::replace(&mut self.tab.doc, doc);
                let previous_selection = std::mem::replace(&mut self.tab.selected, selected);
                self.error = false;
                self.changed(before);
                if self.error {
                    self.tab.selected = previous_selection;
                    return false;
                }
                self.status = status.into();
                self.sync_typography();
                self.sync_bonds();
                true
            }
            Err(error) => {
                self.status = error;
                self.error = true;
                false
            }
        }
    }

    pub(super) fn context_key(&mut self, key: &str) -> Task<Message> {
        self.sync_keyboard_drawing();
        if self.tool.selects() && self.tab.keyboard_drawing.active() {
            return self.keyboard_context_key(key);
        }
        let point = self
            .tab
            .hover
            .filter(|(_, epoch)| *epoch == self.tab.file_epoch)
            .map(|(p, _)| p);
        let hovered_atom = point.and_then(|p| {
            let radius = 10. / self.tab.camera.zoom;
            // The visible label owns its hydrogens, isotope and charge even
            // when their glyphs extend beyond the atom's center hit radius.
            // Do not pad the label here: nearby bonds must remain targetable.
            reshiki::scene::atom_label_hit(&self.tab.doc, p, 0.)
                .or_else(|| self.tab.doc.nearest(p, radius))
        });
        let hovered_bond = if hovered_atom.is_none() {
            point
                .and_then(|p| editing::nearest_bond(&self.tab.doc, p, 7. / self.tab.camera.zoom))
                .and_then(|i| self.tab.doc.bonds.get(i))
                .map(|b| (b.a, b.b))
        } else {
            None
        };
        let atom = hovered_atom.or_else(|| {
            if hovered_bond.is_none() {
                match self.tab.selected.as_slice() {
                    [id] if self.tab.doc.atom(*id).is_some() => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        });
        let bond = hovered_bond.or_else(|| {
            if atom.is_none() {
                match self.tab.selected.as_slice() {
                    [a, b]
                        if self
                            .tab
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
        self.context_key_resolved(
            key,
            atom,
            bond,
            false,
            hovered_atom.is_some() || hovered_bond.is_some(),
        )
    }

    /// The keyboard mode supplies its hotspot directly, without synthesizing
    /// hover events or relying on the selection created by the previous edit.
    pub(super) fn context_key_at(
        &mut self,
        key: &str,
        target: reshiki::keyboard_drawing::Target,
    ) -> Task<Message> {
        let (atom, bond) = match target {
            reshiki::keyboard_drawing::Target::Atom(id) => (Some(id), None),
            reshiki::keyboard_drawing::Target::Bond(a, b) => (None, Some((a, b))),
            reshiki::keyboard_drawing::Target::Blank(_) => (None, None),
        };
        self.context_key_resolved(key, atom, bond, true, false)
    }

    fn context_key_resolved(
        &mut self,
        key: &str,
        atom: Option<u64>,
        bond: Option<(u64, u64)>,
        explicit: bool,
        pointed: bool,
    ) -> Task<Message> {
        // Hover attachment wins even when placement automatically selected the
        // previous ring. Only an unpointed selection gives `a` a display action.
        if key == "a" && !explicit && !pointed && self.aromatic_display_selection() {
            return self.update(Message::AromaticDisplay);
        }
        if key == "g" {
            if let Some(id) = atom {
                self.edit(crate::canvas::Edit::Select(vec![id]));
            } else if let Some((a, b)) = bond {
                self.edit(crate::canvas::Edit::Select(vec![a, b]));
            }
            return Task::none();
        }
        if ["/", "?", "=", "Enter"].contains(&key) {
            if key == "Enter"
                && !explicit
                && self
                    .tab
                    .selected
                    .iter()
                    .filter(|id| self.tab.doc.atom(**id).is_some())
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
                self.edit(crate::canvas::Edit::Select(vec![id]));
                if ["=", "Enter"].contains(&key) {
                    return self
                        .update(Message::AtomText(super::atom_text::Action::Begin(Some(id))));
                }
            } else if let Some((a, b)) = bond {
                self.edit(crate::canvas::Edit::Select(vec![a, b]));
            }
            return self.update(Message::Inspector(InspectorTab::Properties));
        }
        if let Some((a, b)) = bond {
            if let Some(preset) = hotkeys::bond_preset(key) {
                if self
                    .tab
                    .doc
                    .abbreviations
                    .iter()
                    .any(|g| g.members.contains(&a) || g.members.contains(&b))
                {
                    self.status = "Expand the abbreviation before changing its bonds".into();
                    self.error = true;
                    return Task::none();
                }
                self.tab.selected = vec![a, b];
                // Repeated 2 cycles placement while keeping chemical order intact.
                if key == "2"
                    && let Some(current) = self.tab.doc.bonds.iter().find(|e| {
                        ((e.a == a && e.b == b) || (e.a == b && e.b == a))
                            && reshiki::bonds::BondPreset::of(e)
                                == Some(reshiki::bonds::BondPreset::Double)
                    })
                {
                    return self.update(Message::BondPosition(
                        reshiki::scene::effective_double_position(&self.tab.doc, current).cycled(),
                    ));
                }
                let result =
                    hotkeys::bond_edit(&self.tab.doc, a, b, preset).map(|doc| (doc, vec![a, b]));
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
                self.tab.selected = vec![a, b];
                return self.update(Message::BondPosition(position));
            }
            if key == "f" {
                self.tab.selected = vec![a, b];
                return self.update(Message::BondDepth(true));
            }
        }
        if let Some(id) = atom
            && let Some(result) =
                hotkeys::atom_edit(&self.tab.doc, id, key, self.tab.bond_drawing.length)
        {
            let focus = result.as_ref().ok().map(|(_, focus)| *focus);
            let result = result.map(|(doc, focus)| (doc, vec![focus]));
            if self.commit_hotkey(result, "Atom shortcut applied") {
                self.tab.labels_dirty = true;
                if explicit {
                    if let Some(focus) = focus {
                        self.tab.keyboard_drawing.set_target(
                            reshiki::keyboard_drawing::Target::Atom(focus),
                            &self.tab.doc,
                        );
                    }
                } else {
                    // Continue ordinary hover growth until the pointer moves again.
                    self.tab.hover = self
                        .tab
                        .selected
                        .first()
                        .and_then(|id| self.tab.doc.atom(*id))
                        .map(|a| (a.position, self.tab.file_epoch));
                }
            }
            return Task::none();
        }
        if (atom.is_some() || bond.is_some())
            && let Some(result) =
                hotkeys::ring_edit(&self.tab.doc, atom, bond, key, self.tab.bond_drawing.length)
        {
            self.commit_hotkey(result, "Ring attached");
            return Task::none();
        }
        self.empty_context_key(key)
    }

    /// Keys without a contextual chemistry action keep their ordinary drawing
    /// tool behavior, including Text, chain and graphic shortcuts.
    pub(super) fn empty_context_key(&mut self, key: &str) -> Task<Message> {
        if ["/", "?", "=", "Enter"].contains(&key) {
            return self.update(Message::Inspector(InspectorTab::Properties));
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
                Some(Tool::RingPreset(reshiki::rings::Preset::Benzene))
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

    pub(super) fn aromatic_display_selection(&self) -> bool {
        reshiki::rings::selected_cycle(&self.tab.doc, &self.tab.selected).is_some()
            && self.tab.doc.bonds.iter().any(|bond| {
                self.tab.selected.contains(&bond.a)
                    && self.tab.selected.contains(&bond.b)
                    && matches!(bond.order, 2 | 4)
            })
    }
}

// An independent diagram elsewhere on the page must not block a selected ring's display edit.
// Keep validation for the complete selected molecule, and preserve the rest of the document.
pub(super) async fn aromatic_selection(
    engine: reshiki::engine::LocalEngine,
    mut request: reshiki::engine::Request,
) -> Result<reshiki::engine::Response, String> {
    use reshiki::engine::ChemistryEngine;
    let Some(original) = request.document.as_mut() else {
        return engine.execute(request).await;
    };
    original.validate()?;
    let selected = request.selected_ids.as_deref().unwrap_or_default();
    let atoms: Vec<_> = original.atoms.iter().map(|a| a.id).collect();
    let scope: Vec<_> = reshiki::editing::groups(original, &atoms)
        .into_iter()
        .filter(|g| g.iter().any(|id| selected.contains(id)))
        .flatten()
        .collect();
    if scope.is_empty() || scope.len() == atoms.len() {
        return engine.execute(request).await;
    }
    let fragment = reshiki::editing::selection(original, &scope);
    let mut original = std::mem::replace(original, fragment);
    let mut response = engine.execute(request).await?;
    let edited = response
        .document
        .ok_or("The ring edit returned no drawing")?;
    for atom in &mut original.atoms {
        if let Some(new) = edited.atom(atom.id) {
            *atom = new.clone();
        }
    }
    for bond in &mut original.bonds {
        if let Some(new) = edited
            .bonds
            .iter()
            .find(|b| (b.a == bond.a && b.b == bond.b) || (b.a == bond.b && b.b == bond.a))
        {
            *bond = new.clone();
        }
    }
    original.validate()?;
    response.document = Some(original);
    response.analysis = None; // A fragment's formula is not the whole drawing's formula.
    Ok(response)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod compatibility_tests;
