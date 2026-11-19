//! Commands for the object under a secondary click, or the current selection.
use super::object_toolbar::Command;
use super::workspace::horizontal_line;
use super::{App, InspectorTab, Message, inspector};
use crate::canvas::Tool;
use iced::widget::{button, column, container, rich_text, row, scrollable, text};
use iced::{Border, Color, Element, Length, Point, Task, keyboard::key::Named};
use reshiki::export::Background as Paper;
use std::borrow::Cow;

mod cascade;
#[cfg(test)]
mod cascade_tests;
use reshiki::{
    bonds::BondPreset,
    chemistry::stereo_lint,
    editing::{Arrange, Transform},
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Page {
    #[default]
    Main,
    Align,
    Bonds,
    Tilt,
    /// Show the selected molecule as 2D, 3D or both.
    Model,
    /// Property lines under the clicked molecule, ticked when shown.
    Properties,
    /// Chemistry, Reference or Illustration, ticked for the current one.
    Purpose,
    Attachments,
    CopyAs,
    /// Copy image on a background for this copy only.
    CopyImageOn,
    // Menus anchored under context row buttons.
    AlignObjects,
    Distribute,
    Order,
    Arrange,
    /// The last n context row commands, folded into ⋯.
    More(usize),
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_parity_tests;
const SHORTCUT_GAP: f32 = 12.;
/// The tick column of rows that show whether they are on.
const TICK_WIDTH: f32 = 16.;
/// The tick and swatch before a background row's label.
const SWATCH_WIDTH: f32 = 32.;
const SCROLLBAR_WIDTH: f32 = 10.;
const SCROLLBAR_GAP: f32 = 4.;

/// A menu row's accessibility id: the command's own id where it has one,
/// otherwise its page and position. Keyboard focus moves by the same id.
fn item_id(page: Page, index: usize, action: &Action) -> String {
    let named = match action {
        Action::Run(message) => match message.as_ref() {
            Message::Naming(super::naming::Action::OpenPanel(_)) => Some("menu-chemical-names"),
            message => super::workspace::keyboard_control_id(message),
        },
        _ => None,
    };
    named.map_or_else(|| format!("menu-{page:?}-{index}"), str::to_owned)
}

/// The shortcut shown right-aligned beside a command.
fn shortcut(action: &Action) -> Option<String> {
    match action {
        Action::Run(message) => super::shortcuts::label(message),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Close,
    /// Toggle a context row menu at this x offset over the canvas.
    Open(Page, f32),
    Page(Page),
    Hover(usize, usize),
    Pointer,
    Activate(usize, usize),
    CloseAfter(usize),
    Key(Named),
    FocusedKey(usize, usize, Named),
    Run(Box<Message>),
    Properties(bool),
    /// Flip the Editable drawing choice of Copy image on; the menu stays open.
    Editable,
}
pub(super) struct State {
    pub position: Point,
    pub page: Page,
    /// Raw hit IDs; `None` denotes a menu opened from the context toolbar.
    pub hit: Option<Vec<u64>>,
    children: Vec<Child>,
    focused: Option<(usize, usize)>,
    keyboard: bool,
    /// Copy image on's Editable drawing choice; `None` follows the default.
    editable: Option<bool>,
}
#[derive(Clone, Copy)]
struct Child {
    page: Page,
    anchor: usize,
}
impl State {
    pub(super) fn new(position: Point, page: Page) -> Self {
        Self {
            position,
            page,
            hit: None,
            children: vec![],
            focused: None,
            keyboard: false,
            editable: None,
        }
    }

    fn page_at(&self, level: usize) -> Option<Page> {
        if level == 0 {
            Some(self.page)
        } else {
            self.children.get(level - 1).map(|child| child.page)
        }
    }
}

#[derive(Clone)]
enum Entry {
    Item {
        label: &'static str,
        action: Action,
        enabled: bool,
        /// A background swatch, and whether it is the Copy default.
        swatch: Option<(Paper, bool)>,
    },
    Separator,
    Hint(&'static str),
}
/// Copy image on's check row.
const EDITABLE: &str = "Editable drawing";
/// Beside the check row while it differs from the Settings default.
const NOT_DEFAULT: &str = "(not default)";
/// The label of a saved color's row, which shows the color's code instead.
const SAVED_COLOR: &str = "Saved color";
/// The text a row shows: its label, or a saved color's code.
fn shown(label: &'static str, swatch: Option<(Paper, bool)>) -> Cow<'static, str> {
    match swatch {
        Some((Paper::Color(rgb), _)) if label == SAVED_COLOR => reshiki::palette::hex(rgb).into(),
        _ => label.into(),
    }
}
impl Entry {
    fn command(label: &'static str, message: Message, enabled: bool) -> Self {
        Self::Item {
            label,
            action: Action::Run(Box::new(message)),
            enabled,
            swatch: None,
        }
    }
    fn page(label: &'static str, page: Page) -> Self {
        Self::Item {
            label,
            action: Action::Page(page),
            enabled: true,
            swatch: None,
        }
    }
}

impl App {
    fn context_entries(&self, page: Page) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        let submenu = Entry::page;
        let atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .any(|a| self.tab.selected.contains(&a.id));
        let bonds = self
            .tab
            .doc
            .bonds
            .iter()
            .any(|b| self.tab.selected.contains(&b.a) && self.tab.selected.contains(&b.b));
        let tilt = crate::canvas::tilt::available(&self.tab.doc, &self.tab.selected);
        let multiple = self.alignment_count() >= 2;
        match page {
            Page::Main if self.tab.selected.is_empty() => vec![
                command("Undo", Message::Undo, self.tab.history.can_undo()),
                command("Redo", Message::Redo, self.tab.history.can_redo()),
                Separator,
                command("Paste", Message::Paste, !self.tab.clipboard_busy),
            ]
            .into_iter()
            .chain(self.copy_image_on_entry())
            .chain([
                submenu("Copy as", Page::CopyAs),
                command(
                    "Select all",
                    Message::SelectAll,
                    !self.tab.doc.all_ids().is_empty(),
                ),
                Separator,
                command("Fit drawing", Message::Fit, true),
            ])
            .collect(),
            Page::Main => self.main_entries(atoms, bonds, tilt),
            Page::CopyAs => self.copy_as_entries(),
            Page::CopyImageOn => self.copy_image_on_entries(),
            Page::Tilt => vec![
                Hint("3D tilt · selected objects"),
                command("Drag to tilt", Message::Tool(Tool::Tilt), tilt),
                Separator,
                command("X −15°", Message::Transform(Transform::TiltX(-15.)), tilt),
                command("X +15°", Message::Transform(Transform::TiltX(15.)), tilt),
                command("Y −15°", Message::Transform(Transform::TiltY(-15.)), tilt),
                command("Y +15°", Message::Transform(Transform::TiltY(15.)), tilt),
                Separator,
                command(
                    "Emphasize front bonds",
                    Message::InspectorAction(inspector::Action::DepthBonds),
                    bonds,
                ),
                Hint("Labels stay upright"),
            ],
            Page::Model => self.model_entries(),
            Page::Properties => self.property_entries(),
            Page::Purpose => reshiki::purpose::Purpose::ALL
                .into_iter()
                .map(|p| Entry::command(p.label(), Message::Purpose(p), true))
                .collect(),
            Page::Align => self.align_entries(atoms, multiple),
            Page::Bonds => self.bond_entries(bonds),
            // Two objects can already make a lasting row or column.
            Page::AlignObjects => [
                self.arrange_entries(&[&Command::HORIZONTAL, &Command::VERTICAL]),
                self.layout_entries(),
            ]
            .concat(),
            Page::Distribute => {
                let mut entries = self.arrange_entries(&[&Command::DISTRIBUTE]);
                entries.extend(self.layout_entries());
                entries
            }
            Page::Order => self.arrange_entries(&[&Command::ORDER]),
            Page::Arrange => [
                self.arrange_entries(&[&Command::HORIZONTAL, &Command::VERTICAL]),
                [
                    self.arrange_entries(&[&Command::DISTRIBUTE]),
                    self.layout_entries(),
                ]
                .concat(),
                self.arrange_entries(&[&Command::ORDER]),
                self.arrange_entries(&[&Command::TRANSFORM]),
            ]
            .join(&Separator),
            Page::More(folded) => {
                let commands = self.context_commands();
                let shown = commands.len().saturating_sub(folded);
                commands
                    .into_iter()
                    .skip(shown)
                    .map(|c| command(c.menu, c.message, c.enabled))
                    .collect()
            }
            Page::Attachments => vec![
                Hint("Attach to selected atoms"),
                command(
                    "Multi-center attachment",
                    Message::InspectorAction(inspector::Action::Attachment(
                        reshiki::attachments::Kind::MultiCenter,
                    )),
                    true,
                ),
                command(
                    "Variable attachment",
                    Message::InspectorAction(inspector::Action::Attachment(
                        reshiki::attachments::Kind::Variable,
                    )),
                    true,
                ),
                Separator,
                command(
                    "Drawing centroid",
                    Message::InspectorAction(inspector::Action::Centroid),
                    true,
                ),
                Hint("Multi-center: all selected atoms. Variable: one of the selected atoms."),
            ],
        }
    }

    /// The selection menu, in groups: edit the clicked atom, name or select
    /// its molecule, connect and reshape, generic structures, the clipboard,
    /// then Properties and Delete, pinned in the footer.
    fn main_entries(&self, atoms: bool, bonds: bool, tilt: bool) -> Vec<Entry> {
        let command = Entry::command;
        [
            self.atom_entries(atoms),
            self.molecule_entries(atoms),
            self.structure_entries(atoms, bonds, tilt),
            self.generic_entries(),
            if self.selected_text_boxes() {
                vec![command(
                    "Format as chemical name",
                    Message::FormatChemicalName,
                    true,
                )]
            } else {
                vec![]
            },
            self.purpose_entries(atoms),
            [
                command("Cut", Message::Copy(true), true),
                command("Copy", Message::Copy(false), true),
            ]
            .into_iter()
            .chain(self.copy_image_on_entry())
            .chain([
                Entry::page("Copy as", Page::CopyAs),
                command("Paste", Message::Paste, !self.tab.clipboard_busy),
                command("Duplicate", Message::Duplicate, true),
            ])
            .collect(),
            vec![Entry::Item {
                label: "Properties…",
                action: Action::Properties(false),
                enabled: true,
                swatch: None,
            }],
            vec![command("Delete", Message::Delete, true)],
        ]
        .into_iter()
        .filter(|group| !group.is_empty())
        .collect::<Vec<_>>()
        .join(&Entry::Separator)
    }

    /// What the selected molecules and arrows are for, and what takes part in
    /// the selected arrow's reaction.
    fn purpose_entries(&self, atoms: bool) -> Vec<Entry> {
        let arrows: Vec<_> = self
            .tab
            .doc
            .arrows
            .iter()
            .filter(|a| self.tab.selected.contains(&a.id))
            .collect();
        let mut entries = vec![];
        // A molecule's Purpose is in its own group.
        if !atoms && !arrows.is_empty() {
            entries.push(Entry::page("Purpose", Page::Purpose));
        }
        if let [arrow] = arrows.as_slice() {
            entries.push(Entry::command(
                "Preview reaction",
                Message::Reaction(super::reactions::Action::Preview),
                arrow.purpose.is_chemistry()
                    && reshiki::arrows::Preset::from_kind(&arrow.kind)
                        .reaction_role_refusal()
                        .is_none(),
            ));
            let ready = self
                .tab
                .doc
                .reactions
                .iter()
                .any(|r| r.arrow == arrow.id && r.ready());
            entries.push(Entry::command(
                "Color by origin",
                Message::Reaction(super::reactions::Action::ColorByOrigin),
                ready,
            ));
        }
        entries
    }

    /// The clicked atom's label and reaction map.
    fn atom_entries(&self, atoms: bool) -> Vec<Entry> {
        let command = Entry::command;
        let mut entries = vec![];
        if !atoms {
            return entries;
        }
        if self.atom_text_target().is_some() {
            entries.push(command(
                "Edit atom label…",
                Message::AtomText(super::atom_text::Action::Begin(None)),
                true,
            ));
        }
        if let Some(atom) = self.mapping_atom() {
            entries.push(command(
                "Edit atom map…",
                Message::Mapping(super::reaction_mapping::Action::Open),
                true,
            ));
            if self.tab.doc.atom(atom).is_some_and(|a| a.map_num != 0) {
                entries.push(command(
                    "Clear atom map",
                    Message::Mapping(super::reaction_mapping::Action::Clear),
                    true,
                ));
            }
        }
        let selected_atoms = self
            .tab
            .selected
            .iter()
            .filter(|id| self.tab.doc.atom(**id).is_some())
            .count();
        let eligible = reshiki::depiction::eligible_atoms(&self.tab.doc, &self.tab.selected);
        if !eligible.is_empty() {
            entries.push(command(
                "Draw as charge-separated",
                Message::DrawChargeSeparated(eligible),
                true,
            ));
        }
        if selected_atoms > 1 {
            entries.push(command(
                "Abbreviate selection…",
                Message::AtomText(super::atom_text::Action::ContractSelection),
                true,
            ));
        }
        entries
    }

    /// Name the clicked molecule, or select all of it.
    fn molecule_entries(&self, atoms: bool) -> Vec<Entry> {
        let command = Entry::command;
        let mut entries = vec![];
        if !atoms {
            return entries;
        }
        if let Some(target) = self.context_name_target() {
            // One molecule's name is the Name row of its property caption. A
            // salt, or a name caption from an older drawing, keeps its own caption.
            let caption = reshiki::molecule_names::label(&self.tab.doc, &target).is_some();
            let row = self.context_property_target().as_ref() == Some(&target);
            let shown = caption
                || row
                    && reshiki::linked_properties::shown(
                        &self.tab.doc,
                        &target,
                        reshiki::linked_properties::Property::Name,
                    );
            entries.push(command(
                if shown {
                    "Hide chemical name"
                } else {
                    "Show chemical name"
                },
                if caption {
                    Message::Naming(super::naming::Action::HideMoleculeName(target.clone()))
                } else if row {
                    Message::Naming(super::naming::Action::NameRow(target.clone()))
                } else {
                    Message::Naming(super::naming::Action::ShowMoleculeName(target.clone()))
                },
                row && !caption || self.naming_can_show_molecule(),
            ));
            entries.push(command(
                "Chemical names…",
                Message::Naming(super::naming::Action::OpenPanel(target)),
                true,
            ));
        }
        if self.context_property_target().is_some() {
            entries.push(Entry::page("Show properties", Page::Properties));
        }
        let connected: Vec<_> = reshiki::editing::groups(&self.tab.doc, &self.tab.doc.all_ids())
            .into_iter()
            .filter(|ids| {
                ids.iter()
                    .any(|id| self.tab.selected.contains(id) && self.tab.doc.atom(*id).is_some())
            })
            .flatten()
            .collect();
        if connected.len() != self.tab.selected.len()
            || connected.iter().any(|id| !self.tab.selected.contains(id))
        {
            entries.push(command(
                "Select molecule",
                Message::Canvas(crate::canvas::Edit::Select(connected)),
                true,
            ));
        }
        entries.push(Entry::page("Purpose", Page::Purpose));
        entries
    }

    fn context_property_target(&self) -> Option<Vec<u64>> {
        let hit = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.hit.as_deref());
        self.property_target(hit.unwrap_or(&[]))
    }

    /// Choosing "Show properties" itself, by click, Enter or Space, shows
    /// Mol. Wt. when the molecule has no caption yet.
    fn show_default_property(&mut self, page: Page) {
        if page == Page::Properties
            && let Some(target) = self.context_property_target()
            && reshiki::linked_properties::caption(&self.tab.doc, &target).is_none()
        {
            self.toggle_property(&target, reshiki::linked_properties::Property::Weight);
        }
    }

    /// One row per property, in the Properties panel's order: the name,
    /// then the values.
    fn property_entries(&self) -> Vec<Entry> {
        let Some(target) = self.context_property_target() else {
            return vec![];
        };
        let mut entries: Vec<_> = reshiki::linked_properties::Property::ALL
            .into_iter()
            .map(|p| Entry::command(p.name(), Message::ToggleProperty(target.clone(), p), true))
            .collect();
        entries.insert(1, Entry::Separator);
        entries
    }

    /// Whether a property row is ticked; `None` for other rows.
    fn ticked(&self, action: &Action) -> Option<bool> {
        match action {
            Action::Run(message) => match message.as_ref() {
                Message::ToggleProperty(atoms, property) => Some(
                    reshiki::linked_properties::shown(&self.tab.doc, atoms, *property),
                ),
                Message::Purpose(purpose) => Some(
                    self.tab
                        .selected
                        .iter()
                        .filter_map(|id| reshiki::purpose::of(&self.tab.doc, *id))
                        .all(|p| p == *purpose),
                ),
                _ => None,
            },
            _ => None,
        }
    }

    /// The Structure section's 2D / 3D / Both, with 2D in either layout; the
    /// current view is disabled.
    fn model_entries(&self) -> Vec<Entry> {
        use super::models3d::Action;
        use reshiki::models3d::Display;
        let selected = self.selected_model();
        let model = selected
            .as_ref()
            .and_then(|(_, index)| *index)
            .and_then(|i| self.tab.doc.models.get(i));
        let shown = model.map_or(Display::TwoD, |m| m.display);
        // A generation in flight shows its view, so 2D can cancel it.
        let current = selected
            .and_then(|(atoms, _)| self.pending(&atoms))
            .unwrap_or(shown);
        let enabled = !self.tab.busy && self.tab.optimization.is_none();
        // 2D in 2D still brings back a drawn layout that was flattened.
        let flattened = model.is_some_and(|m| m.drawn.is_some());
        let show = |label, display| {
            Entry::command(
                label,
                Message::Models3d(Action::Show(display)),
                enabled && (display != current || display == Display::TwoD && flattened),
            )
        };
        vec![
            show("2D drawing", Display::TwoD),
            Entry::command(
                "2D from the 3D view",
                Message::Models3d(Action::Flatten),
                enabled && model.is_some(),
            ),
            show("3D model", Display::ThreeD),
            show("2D and 3D", Display::Both),
        ]
    }

    /// Connect and reshape the selection. Move & attach and Coordinate in
    /// place stay in Arrange & transform, so the menu fits a 1280 × 820 window.
    fn structure_entries(&self, atoms: bool, bonds: bool, tilt: bool) -> Vec<Entry> {
        let command = Entry::command;
        let submenu = Entry::page;
        let mut entries = vec![];
        if atoms {
            let molecules = reshiki::reactions::molecules(&self.tab.doc, &self.tab.selected);
            let fischer = reshiki::fischer::marked_centers(&self.tab.doc);
            let in_molecules = |id: &u64| molecules.iter().any(|ids| ids.contains(id));
            if fischer.iter().any(in_molecules) {
                entries.push(command(
                    "Convert to wedge drawing",
                    Message::Cleanup(super::cleanup::Action::ToWedge),
                    !self.tab.busy,
                ));
            } else if self
                .tab
                .doc
                .atoms
                .iter()
                .any(|a| matches!(a.cip_label.as_deref(), Some("R" | "S")) && in_molecules(&a.id))
            {
                entries.push(command(
                    "Convert to Fischer projection",
                    Message::Cleanup(super::cleanup::Action::ToFischer),
                    !self.tab.busy,
                ));
            }
            if let [id] = self.tab.selected.as_slice()
                && self
                    .tab
                    .doc
                    .atom(*id)
                    .is_some_and(|a| a.stereo.is_some() || a.cip_label.is_some())
            {
                entries.push(command(
                    "Invert configuration here",
                    Message::Stereo(super::stereo::Action::Invert),
                    !self.tab.busy,
                ));
            }
            if self.tab.doc.bonds.iter().any(|bond| {
                stereo_lint::stereobond(bond) && molecules.iter().any(|ids| ids.contains(&bond.a))
            }) {
                entries.push(command(
                    "Redraw stereo bonds",
                    Message::Stereo(super::stereo::Action::Redraw),
                    !self.tab.busy,
                ));
            }
            entries.extend(
                self.selected_join_commands()
                    .into_iter()
                    .map(|c| command(c.menu, c.message, c.enabled)),
            );
            if reshiki::ionic::bracket_charge(&self.tab.doc, &self.tab.selected).is_ok() {
                entries.push(command("Bracket ion", Message::BracketIon, true));
            }
        }
        if tilt {
            entries.push(submenu("3D tilt", Page::Tilt));
        }
        if self.selected_model().is_some() {
            entries.push(submenu("Show as", Page::Model));
        }
        if self
            .tab
            .doc
            .atoms
            .iter()
            .any(|atom| self.tab.selected.contains(&atom.id) && !atom.marks.is_empty())
        {
            entries.push(command(
                "Position atom marks",
                Message::Tool(Tool::EditPoints),
                true,
            ));
        }
        if self.tab.doc.atoms.iter().any(|a| {
            self.tab.selected.contains(&a.id)
                && a.attachment.is_some()
                && self.tab.doc.abbreviation(a.id).is_none()
        }) {
            entries.push(command(
                "Move attachment point only",
                Message::Tool(Tool::EditPoints),
                true,
            ));
        }
        entries.push(submenu("Arrange & transform", Page::Align));
        if self.callout_target().is_some() {
            entries.push(command("Add callout", Message::AddCallout, true));
        }
        if bonds {
            entries.push(submenu("Bond appearance", Page::Bonds));
        }
        let real_atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| {
                self.tab.selected.contains(&a.id) && a.element != "*" && a.centroid.is_empty()
            })
            .count();
        if (2..=300).contains(&real_atoms) && real_atoms == self.tab.selected.len() {
            entries.push(submenu("Attachment points", Page::Attachments));
        }
        if self
            .tab
            .doc
            .arrows
            .iter()
            .any(|a| self.tab.selected.contains(&a.id))
        {
            entries.push(command(
                "Reverse arrow",
                Message::ArrowAction(super::arrows::Action::Reverse),
                true,
            ));
            entries.push(command(
                "Straighten arrow",
                Message::ArrowAction(super::arrows::Action::Straighten),
                true,
            ));
            if self.selected_node_arrow().is_some() {
                entries.push(command(
                    "Edit arrow points",
                    Message::Tool(Tool::EditPoints),
                    true,
                ));
            }
        }
        if self.has_cleanup_selection() {
            entries.push(command(
                "Smooth",
                Message::Graphics(super::graphics::Action::Cleanup(
                    super::graphics::Cleanup::Smooth,
                )),
                true,
            ));
            entries.push(command(
                "Symmetrize",
                Message::Graphics(super::graphics::Action::Cleanup(
                    super::graphics::Cleanup::Symmetrise { upright: false },
                )),
                true,
            ));
        }
        entries
    }

    /// Expansion of the clicked component, including its native dependencies.
    fn generic_entries(&self) -> Vec<Entry> {
        let target = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.hit.as_deref())
            .unwrap_or(&self.tab.selected);
        let Ok(scope) = reshiki::generic::component_scope(&self.tab.doc, target) else {
            return vec![];
        };
        if !reshiki::generic::intersects(&self.tab.doc, &scope) {
            return vec![];
        }
        vec![Entry::command(
            super::generic::EXPAND,
            Message::Generic(super::generic::Action::ExpandComponent(target.to_vec())),
            self.generic_expand_enabled(),
        )]
    }

    /// Copy image on ›, where a native clipboard can hold pictures.
    fn copy_image_on_entry(&self) -> Option<Entry> {
        reshiki::clipboard::available().then(|| Entry::page("Copy image on", Page::CopyImageOn))
    }

    /// Whether this Copy image on includes the editable drawing.
    fn copy_editable(&self) -> bool {
        self.context_menu
            .as_ref()
            .and_then(|menu| menu.editable)
            .unwrap_or(self.appearance.export_editable_drawing)
    }

    /// The backgrounds and saved colors for one Copy image; a tick marks the
    /// Copy default. Then whether it includes the editable drawing, and a row
    /// that opens Settings to change the defaults.
    fn copy_image_on_entries(&self) -> Vec<Entry> {
        use crate::appearance::Background;
        let appearance = &self.appearance;
        let default = appearance
            .copy_background
            .paper(appearance.custom_background);
        let enabled = !self.clipboard_working();
        let editable = self.copy_editable();
        let item = |label: &'static str, paper: Paper| Entry::Item {
            label,
            action: Action::Run(Box::new(Message::CopyImageOn(paper, editable))),
            enabled,
            swatch: Some((paper, paper == default)),
        };
        let mut entries: Vec<_> = [
            Background::Transparent,
            Background::Canvas,
            Background::White,
            Background::Black,
        ]
        .into_iter()
        .map(|background| {
            item(
                background.label(),
                background.paper(appearance.custom_background),
            )
        })
        .collect();
        if !appearance.background_colors.is_empty() {
            entries.push(Entry::Separator);
            entries.extend(
                appearance
                    .background_colors
                    .iter()
                    .map(|&rgb| item(SAVED_COLOR, Paper::Color(rgb))),
            );
        }
        entries.push(Entry::Separator);
        entries.push(Entry::Item {
            label: EDITABLE,
            action: Action::Editable,
            enabled,
            swatch: None,
        });
        entries.push(Entry::command(
            "Change the default…",
            Message::Settings(super::settings_page::Action::Open(Some(
                super::settings_page::Section::CopyExport,
            ))),
            true,
        ));
        entries
    }

    fn copy_as_entries(&self) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        use reshiki::clipboard::{CopyFormat, chemical_snapshot, selection_or_drawing};
        let snapshot = selection_or_drawing(&self.tab.doc, &self.tab.selected);
        let chemical = chemical_snapshot(&self.tab.doc, &snapshot);
        let busy = self.clipboard_working();
        let mut entries = vec![Hint(if self.tab.selected.is_empty() {
            "Copy as · whole drawing"
        } else {
            "Copy as · selected objects"
        })];
        let mut reasons = Vec::new();
        for format in CopyFormat::ALL {
            if matches!(
                format,
                CopyFormat::Mol | CopyFormat::Cdxml | CopyFormat::Rxn
            ) {
                entries.push(Separator);
            }
            let reason = if format.is_chemical() {
                match &chemical {
                    Ok(doc) => format.unavailable_reason_for_selection(&self.tab.doc, doc),
                    Err(reason) => Some(*reason),
                }
            } else {
                format.unavailable_reason(&snapshot)
            };
            entries.push(command(
                format.label(),
                Message::CopyAs(format),
                !busy && reason.is_none(),
            ));
            if let Some(reason) = reason
                && !reasons.contains(&reason)
            {
                reasons.push(reason);
            }
        }
        if busy {
            reasons.push("A clipboard operation is already in progress.");
        }
        if !reasons.is_empty() {
            entries.push(Separator);
            entries.extend(reasons.into_iter().map(Hint));
        }
        entries
    }

    fn align_entries(&self, atoms: bool, multiple: bool) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        let mut entries = vec![
            Hint("Rotate & reflect"),
            command(
                "Rotate −30°",
                Message::Transform(Transform::Rotate(-30.)),
                true,
            ),
            command(
                "Rotate +30°",
                Message::Transform(Transform::Rotate(30.)),
                true,
            ),
            command(
                "Flip horizontal",
                Message::Transform(Transform::FlipHorizontal),
                true,
            ),
            command(
                "Flip vertical",
                Message::Transform(Transform::FlipVertical),
                true,
            ),
        ];
        if multiple {
            entries.push(Separator);
            for (label, action) in [
                ("Align left", Arrange::AlignLeft),
                ("Align center", Arrange::AlignHorizontal),
                ("Align right", Arrange::AlignRight),
                ("Align top", Arrange::AlignTop),
                ("Align middle", Arrange::AlignVertical),
                ("Align bottom", Arrange::AlignBottom),
                ("Distribute horizontally", Arrange::DistributeHorizontal),
                ("Distribute vertically", Arrange::DistributeVertical),
            ] {
                entries.push(command(label, Message::Arrange(action), true));
            }
        }
        let group = self.can_group();
        let ungroup = !self
            .tab
            .doc
            .outer_selected_groups(&self.tab.selected)
            .is_empty();
        if atoms || group || ungroup {
            entries.push(Separator);
            if atoms {
                entries.extend(
                    self.selected_attach_commands()
                        .into_iter()
                        .map(|c| command(c.menu, c.message, c.enabled)),
                );
            }
            if group {
                entries.push(command("Group", Message::Group, true));
            }
            if ungroup {
                entries.push(command("Ungroup", Message::Ungroup, true));
            }
        }
        if !self.tab.selected.is_empty() {
            entries.push(command(
                "Wrap in panel",
                Message::Figure(super::figure_panels::Action::Wrap),
                true,
            ));
        }
        entries
    }

    fn bond_entries(&self, bonds: bool) -> Vec<Entry> {
        use Entry::{Hint, Separator};
        let command = Entry::command;
        let mut entries = vec![Hint("Bond appearance")];
        entries.push(command("Bond in front", Message::BondDepth(true), bonds));
        entries.push(command("Bond behind", Message::BondDepth(false), bonds));
        if !reshiki::ionic::eligible_bonds(&self.tab.doc, &self.tab.selected).is_empty() {
            entries.push(command("Make ionic", Message::MakeIonic, true));
        }
        if reshiki::aromatic::charge_circle(&self.tab.doc, &self.tab.selected).is_some() {
            entries.push(command(
                "Center charge in ring",
                Message::CenterRingCharge,
                true,
            ));
        }
        if reshiki::rings::selected_cycle(&self.tab.doc, &self.tab.selected).is_some() {
            entries.push(command(
                "Saturated ↔ Aromatic",
                Message::ToggleSelectedRing,
                true,
            ));
        }
        if !reshiki::ring_fills::selected_cycles(&self.tab.doc, &self.tab.selected).is_empty() {
            entries.push(command(
                "Color ring interior…",
                Message::StyleMenu(super::color_popover::Action::RingColor),
                true,
            ));
            entries.push(command(
                "Clear ring fill",
                Message::ClearRingFill,
                self.tab
                    .doc
                    .ring_fills
                    .iter()
                    .any(|f| f.atoms.iter().all(|id| self.tab.selected.contains(id))),
            ));
        }
        if reshiki::ring_arcs::toggle(&mut self.tab.doc.clone(), &self.tab.selected).is_ok() {
            entries.push(command(
                "Toggle inner ring curve",
                Message::InspectorAction(inspector::Action::RingArc),
                true,
            ));
        }
        entries.push(Separator);
        entries.extend(
            BondPreset::ALL
                .into_iter()
                .map(|preset| command(preset.name(), Message::ApplyBondPreset(preset), bonds)),
        );
        entries
    }

    /// Lasting layouts: rows, columns and grids that keep their gutters.
    fn layout_entries(&self) -> Vec<Entry> {
        use reshiki::layouts::Flow;
        // Like Align, a layout needs two objects.
        if self.alignment_count() < 2 {
            return vec![];
        }
        let mut entries = vec![Entry::Separator, Entry::Hint("Lasting layout")];
        for (label, flow) in [
            ("Row with even gutters", Flow::Row),
            ("Column with even gutters", Flow::Column),
            ("Grid with even gutters", Flow::Grid(2)),
        ] {
            entries.push(Entry::command(
                label,
                Message::Figure(super::figure_panels::Action::Arrange(flow)),
                true,
            ));
        }
        entries
    }

    /// Groups of arrange commands that share one availability rule, or a
    /// single disabled row saying what they need.
    fn arrange_entries(&self, groups: &[&[Command]]) -> Vec<Entry> {
        let mut commands = groups.iter().flat_map(|group| group.iter());
        if let Some(first) = commands
            .next()
            .filter(|c| !c.enabled(self, self.alignment_count()))
        {
            return vec![Entry::Item {
                label: first.unavailable(),
                action: Action::Close,
                enabled: false,
                swatch: None,
            }];
        }
        groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|c| Entry::command(c.name(), c.message(), true))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .join(&Entry::Separator)
    }

    pub(super) fn context_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Close => self.context_menu = None,
            Action::Open(page, x) => {
                let open = self
                    .context_menu
                    .as_ref()
                    .is_some_and(|menu| menu.page == page);
                self.context_menu = (!open).then(|| State::new(Point::new(x, 0.), page));
            }
            Action::Page(Page::Main) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.children.clear();
                    menu.focused = None;
                }
            }
            Action::Page(page) => {
                let target = self.context_menu.as_ref().and_then(|menu| {
                    (0..=menu.children.len()).rev().find_map(|level| {
                        self.context_entries(menu.page_at(level)?)
                            .iter()
                            .position(|entry| {
                                matches!(entry, Entry::Item {
                                action: Action::Page(target), enabled: true, ..
                            } if *target == page)
                            })
                            .map(|index| (level, index))
                    })
                });
                if let Some((level, index)) = target {
                    self.context_hover(level, index, true);
                }
            }
            Action::Hover(level, index) => self.context_hover(level, index, true),
            Action::Pointer => {
                if let Some(menu) = &mut self.context_menu {
                    menu.keyboard = false;
                    menu.focused = None;
                }
            }
            Action::Activate(level, index) => {
                let entry = self
                    .context_menu
                    .as_ref()
                    .and_then(|menu| menu.page_at(level))
                    .and_then(|page| self.context_entries(page).get(index).cloned());
                if let Some(Entry::Item {
                    action,
                    enabled: true,
                    ..
                }) = entry
                {
                    if let Action::Page(page) = action {
                        self.show_default_property(page);
                        self.context_hover(level, index, true);
                    } else {
                        return self.context_action(action);
                    }
                }
            }
            Action::CloseAfter(level) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.children.truncate(level);
                    menu.focused = None;
                    menu.keyboard = false;
                }
            }
            Action::Key(key) => return self.context_key_action(key),
            Action::FocusedKey(level, index, key) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.focused = Some((level, index));
                }
                return self.context_key_action(key);
            }
            Action::Run(message) => {
                self.context_menu = None;
                return self.update(*message);
            }
            Action::Editable => {
                let editable = !self.copy_editable();
                if let Some(menu) = &mut self.context_menu {
                    menu.editable = Some(editable);
                }
            }
            Action::Properties(molecular) => {
                self.context_menu = None;
                if molecular {
                    self.tab.inspector_ui.update(inspector::Action::Section(
                        inspector::Section::Molecule,
                        true,
                    ));
                }
                return self.update(Message::Inspector(InspectorTab::Properties));
            }
        }
        Task::none()
    }

    fn context_hover(&mut self, level: usize, index: usize, open: bool) {
        let entry = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.page_at(level))
            .and_then(|page| self.context_entries(page).get(index).cloned());
        let Some(menu) = &mut self.context_menu else {
            return;
        };
        let Some(Entry::Item {
            action, enabled, ..
        }) = entry
        else {
            return;
        };
        menu.focused = Some((level, index));
        menu.keyboard = false;
        let child = match action {
            Action::Page(page) if enabled && open => Some(page),
            _ => None,
        };
        if menu
            .children
            .get(level)
            .is_some_and(|current| Some(current.page) == child && current.anchor == index)
        {
            return;
        }
        menu.children.truncate(level);
        if let Some(page) = child {
            menu.children.push(Child {
                page,
                anchor: index,
            });
        }
    }

    fn context_key_action(&mut self, key: Named) -> Task<Message> {
        if key == Named::Escape {
            self.context_menu = None;
            return Task::none();
        }
        let Some(menu) = &self.context_menu else {
            return Task::none();
        };
        let level = menu.focused.map_or(menu.children.len(), |(level, _)| level);
        let focused = menu
            .focused
            .filter(|(at, _)| *at == level)
            .map(|(_, index)| index);
        let Some(page) = menu.page_at(level) else {
            return Task::none();
        };
        let entries = self.context_entries(page);
        let enabled: Vec<_> = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                matches!(entry, Entry::Item { enabled: true, .. }).then_some(index)
            })
            .collect();
        match key {
            Named::ArrowDown | Named::ArrowUp | Named::Home | Named::End => {
                let current =
                    focused.and_then(|index| enabled.iter().position(|value| *value == index));
                let next = match key {
                    Named::Home => enabled.first(),
                    Named::End => enabled.last(),
                    Named::ArrowDown => current
                        .map(|at| (at + 1) % enabled.len())
                        .and_then(|at| enabled.get(at))
                        .or_else(|| enabled.first()),
                    _ => current
                        .map(|at| (at + enabled.len() - 1) % enabled.len())
                        .and_then(|at| enabled.get(at))
                        .or_else(|| enabled.last()),
                }
                .copied();
                if let Some(index) = next {
                    self.context_hover(level, index, false);
                    if let Some(menu) = &mut self.context_menu {
                        menu.keyboard = true;
                    }
                }
            }
            Named::ArrowLeft => {
                if let Some(menu) = &mut self.context_menu
                    && let Some(child) = menu.children.pop()
                {
                    menu.focused = Some((menu.children.len(), child.anchor));
                    menu.keyboard = true;
                }
            }
            Named::ArrowRight | Named::Enter | Named::Space => {
                let index = focused.or_else(|| enabled.first().copied());
                if let Some(index) = index {
                    if let Some(Entry::Item {
                        action: Action::Page(child),
                        enabled: true,
                        ..
                    }) = entries.get(index)
                    {
                        let child = *child;
                        if key != Named::ArrowRight {
                            self.show_default_property(child);
                        }
                        let first = self
                            .context_entries(child)
                            .iter()
                            .position(|entry| matches!(entry, Entry::Item { enabled: true, .. }));
                        self.context_hover(level, index, true);
                        if let Some(menu) = &mut self.context_menu {
                            menu.focused = first.map(|index| (level + 1, index));
                            menu.keyboard = true;
                        }
                    } else if key != Named::ArrowRight {
                        return self.context_action(Action::Activate(level, index));
                    }
                }
            }
            _ => {}
        }
        if let Some(menu) = &self.context_menu
            && menu.keyboard
            && let Some((level, index)) = menu.focused
            && let Some(page) = menu.page_at(level)
            && let Some(Entry::Item { action, .. }) = self.context_entries(page).get(index)
        {
            return iced::advanced::widget::operate(reshiki::accessibility::FocusControl::new(
                item_id(page, index, action),
            ))
            .discard()
            .chain(Task::done(Message::Accessibility(
                super::accessibility::Action::Refresh,
            )));
        }
        Task::none()
    }

    fn context_width(&self, page: Page) -> f32 {
        use super::workspace::{font_width, text_width};
        let content = self
            .context_entries(page)
            .iter()
            .filter_map(|entry| match entry {
                Entry::Item {
                    label,
                    action,
                    swatch,
                    ..
                } => Some(
                    text_width(&shown(label, *swatch), 12.)
                        + match (swatch, action) {
                            (Some(_), _) => {
                                SWATCH_WIDTH + SHORTCUT_GAP + text_width("Default", 11.)
                            }
                            (None, Action::Editable) => {
                                SWATCH_WIDTH + SHORTCUT_GAP + text_width(NOT_DEFAULT, 11.)
                            }
                            _ => 0.,
                        }
                        + self
                            .ticked(action)
                            .map_or(0., |_| TICK_WIDTH + SHORTCUT_GAP)
                        + if matches!(action, Action::Page(_)) {
                            24.
                        } else {
                            shortcut(action).map_or(0., |keys| {
                                SHORTCUT_GAP
                                    + super::shortcuts::spans(&keys)
                                        .iter()
                                        .map(|run| match run.font {
                                            Some(font) => font_width(&run.text, 11., font),
                                            None => text_width(&run.text, 11.),
                                        })
                                        .sum::<f32>()
                            })
                        },
                ),
                _ => None,
            })
            .fold(0., f32::max);
        // Keep the measured text width after the scrollbar takes its own space.
        (content + 30. + SCROLLBAR_WIDTH + SCROLLBAR_GAP).max(232.)
    }

    fn context_panel(&self, menu: &State, page: Page, level: usize) -> cascade::Panel<'_> {
        let mut entries = column![].spacing(1);
        let mut footer = column![].spacing(1);
        let mut items = Vec::new();
        let mut submenus = Vec::new();
        if matches!(page, Page::Main) && !self.tab.selected.is_empty() {
            entries = entries.push(
                container(
                    text(self.selection_summary())
                        .size(11)
                        .style(super::workspace::muted_text),
                )
                .padding([5, 10]),
            );
        }
        let all_entries = self.context_entries(page);
        let pinned = if page == Page::Main {
            all_entries
                .iter()
                .position(|entry| {
                    matches!(entry, Entry::Item { action: Action::Run(message), .. }
                    if matches!(message.as_ref(), Message::Delete))
                })
                .map(|index| index - 1) // Include the separator above Delete.
        } else {
            None
        };
        for (index, entry) in all_entries.into_iter().enumerate() {
            let row: Element<'_, Message> = match entry {
                Entry::Item {
                    label,
                    action,
                    enabled,
                    swatch,
                } => {
                    let destructive = matches!(&action, Action::Run(message) if matches!(message.as_ref(), Message::Delete));
                    let label = shown(label, swatch);
                    // Copy image on's check row: on, and whether that is the default.
                    let check = matches!(action, Action::Editable).then(|| {
                        let on = self.copy_editable();
                        (on, on == self.appearance.export_editable_drawing)
                    });
                    let accessible_name = match swatch {
                        Some((_, true)) => format!("{label} (default)"),
                        _ if check.is_some_and(|(_, default)| !default) => {
                            format!("{label} {NOT_DEFAULT}")
                        }
                        _ => label.to_string(),
                    };
                    let hint = matches!(&action, Action::Run(message) if matches!(
                        message.as_ref(),
                        Message::Graphics(super::graphics::Action::Cleanup(
                            super::graphics::Cleanup::Symmetrise { .. }
                        ))
                    ))
                    .then_some(super::graphics::SYMMETRISE_HINT);
                    let label = text(label).size(12).width(Length::Fill);
                    let label = if destructive {
                        label.style(crate::appearance::text_color(Color::from_rgb8(167, 59, 51)))
                    } else {
                        label
                    };
                    let mut content = row![]
                        .spacing(SHORTCUT_GAP)
                        .align_y(iced::Alignment::Center);
                    let ticked = self.ticked(&action);
                    if let Some(ticked) = ticked {
                        content = content.push(
                            text(if ticked { "✓" } else { "" })
                                .size(12)
                                .width(TICK_WIDTH),
                        );
                    }
                    if let Some((paper, default)) = swatch {
                        content = content.push(
                            row![
                                text(if default { "✓" } else { "" }).size(12).width(12),
                                super::settings_page::swatch(paper, 14., 12.),
                            ]
                            .spacing(6)
                            .align_y(iced::Alignment::Center),
                        );
                    }
                    if let Some((on, _)) = check {
                        content = content
                            .push(text(if on { "✓" } else { "" }).size(12).width(SWATCH_WIDTH));
                    }
                    content = content.push(label);
                    if swatch.is_some_and(|(_, default)| default) {
                        content = content
                            .push(text("Default").size(11).style(super::workspace::muted_text));
                    }
                    if check.is_some_and(|(_, default)| !default) {
                        content = content.push(
                            text(NOT_DEFAULT)
                                .size(11)
                                .style(super::workspace::muted_text),
                        );
                    }
                    if matches!(action, Action::Page(_)) {
                        content = content.push(text("›").size(16));
                    } else if let Some(keys) = shortcut(&action) {
                        content = content.push(
                            rich_text(super::shortcuts::spans(&keys))
                                .size(11)
                                .style(super::workspace::muted_text),
                        );
                    }
                    let active = enabled
                        && ((menu.keyboard && menu.focused == Some((level, index)))
                            || menu
                                .children
                                .get(level)
                                .is_some_and(|child| child.anchor == index));
                    if enabled && matches!(action, Action::Page(_)) {
                        submenus.push(index);
                    }
                    let pointer = !menu.keyboard;
                    let id = item_id(page, index, &action);
                    let item = reshiki::accessibility::button(id, accessible_name, content)
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(move |theme: &iced::Theme, status| {
                            let mut style = button::text(theme, status);
                            if active
                                || (pointer
                                    && matches!(
                                        status,
                                        button::Status::Hovered | button::Status::Pressed
                                    ))
                            {
                                style.background = Some(
                                    Color {
                                        a: 0.12,
                                        ..theme.palette().primary
                                    }
                                    .into(),
                                );
                            }
                            style
                        })
                        .on_press_maybe(
                            enabled.then_some(Message::ContextMenu(Action::Activate(level, index))),
                        );
                    let item = match check.map(|(on, _)| on).or(ticked) {
                        Some(on) => item.checked(on),
                        None => item,
                    };
                    items.push(index);
                    let item: Element<'_, Message> = if let Some(hint) = hint {
                        super::workspace::hover_hint(
                            item,
                            hint.to_owned(),
                            iced::widget::tooltip::Position::Right,
                        )
                        .into()
                    } else {
                        item.into()
                    };
                    container(item).id(cascade::row_id(level, index)).into()
                }
                Entry::Separator => horizontal_line(),
                Entry::Hint(label) => {
                    container(text(label).size(11).style(super::workspace::muted_text))
                        .padding([5, 10])
                        .into()
                }
            };
            if pinned.is_some_and(|first| index >= first) {
                footer = footer.push(row);
            } else {
                entries = entries.push(row);
            }
        }
        let scrolling = scrollable(entries)
            .id(cascade::scroll_id(level))
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(SCROLLBAR_WIDTH)
                    .scroller_width(SCROLLBAR_WIDTH)
                    .spacing(SCROLLBAR_GAP),
            ))
            .height(if pinned.is_some() {
                Length::Fill
            } else {
                Length::Shrink
            });
        let body: Element<'_, Message> = if pinned.is_some() {
            column![scrolling, footer].spacing(1).into()
        } else {
            scrolling.into()
        };
        let content = container(body)
            .padding(5)
            .width(self.context_width(page))
            .style(|theme| {
                crate::appearance::container(
                    theme,
                    container::Style {
                        background: Some(Color::WHITE.into()),
                        border: Border {
                            color: Color::from_rgb8(192, 204, 201),
                            width: 1.,
                            radius: 7.into(),
                        },
                        shadow: crate::appearance::surface_shadow(iced::Shadow {
                            color: Color::from_rgba8(20, 40, 35, 0.18),
                            offset: iced::Vector::new(0., 4.),
                            blur_radius: 12.,
                        }),
                        ..Default::default()
                    },
                )
            })
            .into();
        cascade::Panel {
            page,
            content,
            items,
            pinned,
            submenus,
            anchor: level
                .checked_sub(1)
                .and_then(|at| menu.children.get(at))
                .map(|child| child.anchor),
        }
    }

    pub(super) fn with_context_menu<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        // Keep the canvas at the same child path when a menu opens or closes.
        // Replacing the wrapper discards its scene, selection and text caches
        // just before a command's first feedback frame.
        let menu = self.context_menu.as_ref();
        let panels = menu
            .into_iter()
            .flat_map(|menu| {
                (0..=menu.children.len()).filter_map(move |level| {
                    menu.page_at(level)
                        .map(|page| self.context_panel(menu, page, level))
                })
            })
            .collect();
        Element::new(cascade::Cascade::new(base, panels, menu))
    }
}
