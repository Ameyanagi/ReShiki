//! Commands for the object under a secondary click, or the current selection.
use super::object_toolbar::Command;
use super::workspace::horizontal_line;
use super::{App, InspectorTab, Message, inspector};
use crate::canvas::Tool;
use iced::widget::{button, column, container, rich_text, row, scrollable, text};
use iced::{Border, Color, Element, Length, Point, Task, keyboard::key::Named};

mod cascade;
#[cfg(test)]
mod cascade_tests;
use reshiki::{
    bonds::BondPreset,
    editing::{Arrange, Transform},
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Page {
    #[default]
    Main,
    Align,
    Bonds,
    Tilt,
    Attachments,
    CopyAs,
    // Menus anchored under context row buttons.
    AlignObjects,
    Distribute,
    Order,
    Arrange,
    /// The first n context row commands, folded into ⋯.
    More(usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Edit;
    use reshiki::document::{Arrow, Point as World};

    fn run_item(app: &mut App, page: Page, label: &str) -> Result<(), String> {
        let action = app
            .context_entries(page)
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Item {
                    label: name,
                    action,
                    enabled: true,
                } if name == label => Some(action),
                _ => None,
            })
            .ok_or_else(|| format!("Missing enabled menu item: {label}"))?;
        let _ = app.context_action(action);
        Ok(())
    }

    fn labels(app: &App, page: Page) -> Vec<&'static str> {
        app.context_entries(page)
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Item { label, .. } => Some(label),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn copy_as_menu_names_scope_and_explains_disabled_reactions() -> Result<(), String> {
        use reshiki::clipboard::CopyFormat;
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        let original = app.tab.doc.clone();
        assert!(labels(&app, Page::Main).contains(&"Copy as"));
        assert!(
            app.context_entries(Page::CopyAs)
                .iter()
                .any(|entry| { matches!(entry, Entry::Hint("Copy as · whole drawing")) })
        );
        app.tab.selected = app.tab.doc.all_ids();
        assert!(
            app.context_entries(Page::CopyAs)
                .iter()
                .any(|entry| { matches!(entry, Entry::Hint("Copy as · selected objects")) })
        );
        assert!(app.context_entries(Page::CopyAs).iter().any(|entry| {
            matches!(entry, Entry::Hint(reason) if reason.contains("one complete defined reaction"))
        }));
        for format in [CopyFormat::Mol, CopyFormat::Smiles, CopyFormat::Rxn] {
            assert!(app.context_entries(Page::CopyAs).iter().any(|entry| {
                matches!(entry, Entry::Item { action: Action::Run(message), enabled, .. }
                    if matches!(message.as_ref(), Message::CopyAs(value) if *value == format)
                    && *enabled == (format != CopyFormat::Rxn))
            }));
        }
        app.context_menu = Some(State::new(Point::new(20., 20.), Page::Main));
        let _ = app.context_action(Action::Page(Page::CopyAs));
        assert_eq!(
            app.context_menu
                .as_ref()
                .unwrap()
                .children
                .last()
                .unwrap()
                .page,
            Page::CopyAs
        );
        let _ = app.context_action(Action::Page(Page::Main));
        assert_eq!(
            app.context_menu.as_ref().map(|state| state.page),
            Some(Page::Main)
        );
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        Ok(())
    }

    #[test]
    fn copy_as_menu_and_command_share_inference_and_explicit_selection_guards() {
        use reshiki::clipboard::CopyFormat;
        let (mut app, _) = App::new();
        let reactant = app.tab.doc.add_atom("O", World::default());
        let product = app.tab.doc.add_atom("O", World::new(240., 0.));
        let arrow = app.tab.doc.next_id();
        app.tab.doc.arrows.push(Arrow::new(
            arrow,
            World::new(90., 0.),
            World::new(150., 0.),
            Default::default(),
            Default::default(),
        ));
        app.tab.selected = app.tab.doc.all_ids();
        let enabled = |app: &App, format| {
            app.context_entries(Page::CopyAs).iter().any(|entry| {
                matches!(entry, Entry::Item { action: Action::Run(message), enabled: true, .. }
                if matches!(message.as_ref(), Message::CopyAs(value) if *value == format))
            })
        };
        assert!(enabled(&app, CopyFormat::ChemDoodleReaction));
        assert!(enabled(&app, CopyFormat::Rxn));
        assert!(!enabled(&app, CopyFormat::Smiles));
        let original = app.tab.doc.clone();
        let selection = app.tab.selected.clone();
        assert!(app.copy_as(CopyFormat::ChemDoodleReaction).units() > 0);
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.selected, selection);
        assert!(!app.tab.history.can_undo());
        app.copy_as_busy = false;
        app.tab.clipboard_busy = false;
        app.tab.doc.reactions = vec![
            reshiki::reactions::copy_reaction(&original, &original)
                .unwrap()
                .unwrap(),
        ];
        app.tab.selected = vec![reactant, arrow];
        let explicit = app.tab.doc.clone();
        assert!(!enabled(&app, CopyFormat::Smiles));
        assert!(!enabled(&app, CopyFormat::ChemDoodleReaction));
        assert!(enabled(&app, CopyFormat::Cdxml));
        for format in [CopyFormat::Smiles, CopyFormat::ChemDoodleReaction] {
            assert_eq!(app.copy_as(format).units(), 0);
            assert!(app.error);
            assert!(app.status.contains("complete defined reaction"));
        }
        assert_eq!(app.tab.doc, explicit);
        assert!(!app.tab.history.can_undo());
        app.tab.selected = vec![reactant, product];
        assert!(!enabled(&app, CopyFormat::Smiles));
        assert_eq!(app.copy_as(CopyFormat::Smiles).units(), 0);
        assert!(app.status.contains("participant"));
        app.tab.selected = vec![product];
        assert!(enabled(&app, CopyFormat::Smiles));
    }

    #[test]
    fn row_menus_toggle_and_explain_unavailable_arrange_commands() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        app.tab.selected = app.tab.doc.all_ids();
        let before = app.tab.doc.clone();
        let open = Message::ContextMenu(Action::Open(Page::AlignObjects, 300.));
        let _ = app.update(open.clone());
        assert_eq!(
            app.context_menu.as_ref().map(|m| m.page),
            Some(Page::AlignObjects)
        );
        let _ = app.update(open);
        assert!(
            app.context_menu.is_none(),
            "The same button closes its menu"
        );
        // One molecule is one object: alignment is unavailable and says why.
        assert_eq!(
            labels(&app, Page::Arrange),
            [
                "Align needs 2 objects",
                "Distribute needs 3 objects",
                "Bring to front",
                "Send to back",
                "Flip horizontal",
                "Flip vertical",
                "Rotate 180°"
            ]
        );
        assert!(run_item(&mut app, Page::Arrange, "Align needs 2 objects").is_err());
        assert_eq!(
            labels(&app, Page::More(2)),
            ["Move & attach…", "Group"],
            "⋯ lists the folded commands in row order"
        );
        let _ = app.update(Message::ContextMenu(Action::Open(Page::Arrange, 300.)));
        run_item(&mut app, Page::Arrange, "Flip horizontal")?;
        assert!(app.context_menu.is_none());
        assert_ne!(app.tab.doc, before);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        Ok(())
    }

    #[test]
    fn context_commands_follow_the_target_without_modifying_the_drawing() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        let ring = app.tab.doc.all_ids();
        let atom = *ring.first().ok_or("ring")?;
        let arrow = app.tab.doc.next_id();
        app.tab.doc.arrows.push(Arrow::new(
            arrow,
            World::new(100., 100.),
            World::new(160., 100.),
            Default::default(),
            Default::default(),
        ));
        let before = app.tab.doc.clone();
        assert_eq!(
            labels(&app, Page::Main),
            [
                "Undo",
                "Redo",
                "Paste",
                "Copy as",
                "Select all",
                "Fit drawing"
            ]
        );
        app.tab.selected = vec![atom];
        let single = labels(&app, Page::Main);
        assert!(single.contains(&"Edit atom label…"));
        assert!(!single.contains(&"3D tilt"));
        assert!(!single.contains(&"Bond appearance"));
        assert!(single.contains(&"Select molecule"));
        app.tab.selected = ring.clone();
        let molecule = labels(&app, Page::Main);
        assert!(!molecule.contains(&"Select molecule"));
        for expected in [
            "3D tilt",
            "Arrange & transform",
            "Bond appearance",
            "Attachment points",
        ] {
            assert!(molecule.contains(&expected), "Missing {expected}");
        }
        assert!(!molecule.contains(&"Bond in front"));
        assert!(labels(&app, Page::Bonds).contains(&"Bond in front"));
        assert!(!labels(&app, Page::Align).contains(&"Align middles"));
        app.tab.selected.push(arrow);
        assert!(!labels(&app, Page::Main).contains(&"Attachment points"));
        assert!(labels(&app, Page::Align).contains(&"Align middles"));
        app.tab.selected = vec![arrow];
        let arrow_items = labels(&app, Page::Main);
        assert!(arrow_items.contains(&"Reverse arrow"));
        assert!(!arrow_items.contains(&"3D tilt"));
        assert!(!arrow_items.contains(&"Bond appearance"));
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        Ok(())
    }

    #[test]
    fn context_tilt_routes_all_axes_and_tool_without_losing_selection() -> Result<(), String> {
        for (label, around_x, degrees) in [
            ("X −15°", true, -15.),
            ("X +15°", true, 15.),
            ("Y −15°", false, -15.),
            ("Y +15°", false, 15.),
        ] {
            let (mut app, _) = App::new();
            app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
            let ids = app.tab.doc.all_ids();
            let source = app.tab.doc.clone();
            reshiki::editing::append(&mut app.tab.doc, &source, World::new(240., 0.));
            let before = app.tab.doc.clone();
            app.edit(Edit::ContextMenu {
                position: Point::new(20., 20.),
                selected: ids.clone(),
            });
            run_item(&mut app, Page::Main, "3D tilt")?;
            assert!(matches!(
                app.context_menu
                    .as_ref()
                    .and_then(|s| s.children.last())
                    .map(|child| child.page),
                Some(Page::Tilt)
            ));
            let _ = app.context_action(Action::Page(Page::Main));
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
            run_item(&mut app, Page::Main, "3D tilt")?;
            run_item(&mut app, Page::Tilt, label)?;
            let mut expected = before.clone();
            reshiki::projection::tilt(&mut expected, &ids, degrees, around_x);
            assert_eq!(app.tab.doc, expected);
            assert_eq!(app.tab.selected, ids);
            assert!(app.context_menu.is_none());
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, expected);
            app.edit(Edit::ContextMenu {
                position: Point::new(20., 20.),
                selected: ids.clone(),
            });
            run_item(&mut app, Page::Tilt, "Drag to tilt")?;
            assert_eq!(app.tool, Tool::Tilt);
            assert_eq!(app.tab.selected, ids);
            assert_eq!(app.tab.doc, expected);
            assert!(app.context_menu.is_none());
        }
        Ok(())
    }

    #[test]
    fn tilt_drag_is_one_undo_step_and_matches_projection_preview() {
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., true);
        let ids = app.tab.doc.all_ids();
        let source = app.tab.doc.clone();
        reshiki::editing::append(&mut app.tab.doc, &source, World::new(240., 0.));
        let before = app.tab.doc.clone();
        app.tab.selected = ids.clone();
        let _ = app.update(Message::Tool(Tool::Tilt));
        assert_eq!(app.tab.selected, ids);
        assert!(!app.tab.history.can_undo());
        let mut preview = before.clone();
        crate::canvas::tilt::apply(&mut preview, &ids, 30., -15.);
        app.edit(Edit::Tilt {
            ids: ids.clone(),
            x: 30.,
            y: -15.,
        });
        assert_eq!(app.tab.doc, preview);
        assert_eq!(app.tab.doc.bonds, before.bonds);
        assert_eq!(app.tool, Tool::Tilt);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, preview);
    }

    #[test]
    fn context_alignment_preserves_molecular_geometry_and_undo_restores_every_group()
    -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        let source = app.tab.doc.clone();
        reshiki::editing::append(&mut app.tab.doc, &source, World::new(240., 80.));
        let arrow = Arrow::new(
            app.tab.doc.next_id(),
            World::new(90., -60.),
            World::new(160., -60.),
            Default::default(),
            Default::default(),
        );
        app.tab.doc.arrows.push(arrow);
        let before = app.tab.doc.clone();
        let ids = app.tab.doc.all_ids();
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            selected: ids.clone(),
        });
        assert_eq!(app.alignment_count(), 3);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::InspectorScroll(0.));
        assert!(
            app.context_menu.is_some(),
            "Background inspector updates must leave the menu open"
        );
        let _ = app.context_action(Action::Run(Box::new(Message::Arrange(
            Arrange::AlignVertical,
        ))));
        assert!(app.context_menu.is_none());
        let centers: Vec<_> = reshiki::editing::groups(&app.tab.doc, &ids)
            .iter()
            .map(|ids| {
                let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, ids)
                    .ok_or("selection bounds")?;
                Ok::<_, String>((lo.y + hi.y) / 2.)
            })
            .collect::<Result<_, _>>()?;
        let center = centers.first().ok_or("alignment center")?;
        assert!(centers.iter().all(|y| (y - center).abs() < 0.001));
        for bond in &app.tab.doc.bonds {
            let length = |doc: &reshiki::document::Document| -> Result<f32, String> {
                Ok(doc
                    .atom(bond.a)
                    .ok_or("bond start")?
                    .position
                    .distance(doc.atom(bond.b).ok_or("bond end")?.position))
            };
            assert!((length(&app.tab.doc)? - length(&before)?).abs() < 0.001);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            selected: ids,
        });
        let _ = app.update(Message::Escape);
        assert!(app.context_menu.is_none());
        assert_eq!(app.tab.doc, before);
        Ok(())
    }

    #[tokio::test]
    async fn inserted_examples_keep_existing_objects_and_can_be_removed_in_one_undo()
    -> Result<(), String> {
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        let before = app.tab.doc.clone();
        let result = app
            .engine
            .request(reshiki::engine::Request::import_smiles("CCO"))
            .await?;
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind: super::super::Job::Insert,
            result: Box::new(Ok(result)),
        });
        assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 3);
        assert_eq!(app.tab.selected.len(), 3);
        for atom in &before.atoms {
            assert_eq!(app.tab.doc.atom(atom.id), Some(atom));
        }
        let (_, old_max) = before.bounds();
        assert!(app.tab.selected.iter().all(|id| {
            app.tab
                .doc
                .atom(*id)
                .is_some_and(|a| a.position.x > old_max.x)
        }));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        Ok(())
    }
}
const SHORTCUT_GAP: f32 = 12.;
const SCROLLBAR_WIDTH: f32 = 10.;
const SCROLLBAR_GAP: f32 = 4.;

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
    Activate(usize, usize),
    CloseAfter(usize),
    Key(Named),
    FocusedKey(usize, usize, Named),
    Run(Box<Message>),
    Properties(bool),
}
pub(super) struct State {
    pub position: Point,
    pub page: Page,
    children: Vec<Child>,
    focused: Option<(usize, usize)>,
    keyboard: bool,
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
            children: vec![],
            focused: None,
            keyboard: false,
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
    },
    Separator,
    Hint(&'static str),
}
impl Entry {
    fn command(label: &'static str, message: Message, enabled: bool) -> Self {
        Self::Item {
            label,
            action: Action::Run(Box::new(message)),
            enabled,
        }
    }
    fn page(label: &'static str, page: Page) -> Self {
        Self::Item {
            label,
            action: Action::Page(page),
            enabled: true,
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
                submenu("Copy as", Page::CopyAs),
                command(
                    "Select all",
                    Message::SelectAll,
                    !self.tab.doc.all_ids().is_empty(),
                ),
                Separator,
                command("Fit drawing", Message::Fit, true),
            ],
            Page::Main => {
                let mut entries = vec![];
                if atoms && self.atom_text_target().is_some() {
                    entries.push(command(
                        "Edit atom label…",
                        Message::AtomText(super::atom_text::Action::Begin(None)),
                        true,
                    ));
                }
                if atoms
                    && self
                        .tab
                        .selected
                        .iter()
                        .filter(|id| self.tab.doc.atom(**id).is_some())
                        .count()
                        > 1
                {
                    entries.push(command(
                        "Create group label…",
                        Message::AtomText(super::atom_text::Action::ContractSelection),
                        true,
                    ));
                }
                if atoms {
                    let connected: Vec<_> =
                        reshiki::editing::groups(&self.tab.doc, &self.tab.doc.all_ids())
                            .into_iter()
                            .filter(|ids| {
                                ids.iter().any(|id| {
                                    self.tab.selected.contains(id)
                                        && self.tab.doc.atom(*id).is_some()
                                })
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
                }
                if tilt {
                    entries.push(submenu("3D tilt", Page::Tilt));
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
                if bonds {
                    entries.push(submenu("Bond appearance", Page::Bonds));
                }
                let real_atoms = self
                    .tab
                    .doc
                    .atoms
                    .iter()
                    .filter(|a| {
                        self.tab.selected.contains(&a.id)
                            && a.element != "*"
                            && a.centroid.is_empty()
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
                }
                entries.push(Separator);
                entries.push(command("Cut", Message::Copy(true), true));
                entries.push(command("Copy", Message::Copy(false), true));
                entries.push(submenu("Copy as", Page::CopyAs));
                entries.push(command("Paste", Message::Paste, !self.tab.clipboard_busy));
                entries.push(command("Duplicate", Message::Duplicate, true));
                entries.push(Separator);
                entries.push(Entry::Item {
                    label: "Properties…",
                    action: Action::Properties(false),
                    enabled: true,
                });
                entries.push(command("Delete", Message::Delete, true));
                entries
            }
            Page::CopyAs => {
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
                            Ok(doc) => format.unavailable_reason(doc),
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
                Hint("Labels stay upright. Undo restores the previous view."),
            ],
            Page::Align => {
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
                        ("Align left edges", Arrange::AlignLeft),
                        ("Align horizontal centers", Arrange::AlignHorizontal),
                        ("Align right edges", Arrange::AlignRight),
                        ("Align top edges", Arrange::AlignTop),
                        ("Align middles", Arrange::AlignVertical),
                        ("Align bottom edges", Arrange::AlignBottom),
                        ("Distribute horizontally", Arrange::DistributeHorizontal),
                        ("Distribute vertically", Arrange::DistributeVertical),
                    ] {
                        entries.push(command(label, Message::Arrange(action), true));
                    }
                }
                if atoms
                    || self.can_group()
                    || !self
                        .tab
                        .doc
                        .outer_selected_groups(&self.tab.selected)
                        .is_empty()
                {
                    entries.push(Separator);
                    if atoms {
                        entries.push(command(
                            "Move & attach…",
                            Message::Join(super::joining::Action::Begin),
                            true,
                        ));
                    }
                    if self.can_group() {
                        entries.push(command("Group", Message::Group, true));
                    }
                    if !self
                        .tab
                        .doc
                        .outer_selected_groups(&self.tab.selected)
                        .is_empty()
                    {
                        entries.push(command("Ungroup", Message::Ungroup, true));
                    }
                }
                entries
            }
            Page::Bonds => {
                let mut entries = vec![Hint("Bond appearance")];
                entries.push(command("Bond in front", Message::BondDepth(true), bonds));
                entries.push(command("Bond behind", Message::BondDepth(false), bonds));
                if reshiki::rings::selected_cycle(&self.tab.doc, &self.tab.selected).is_some() {
                    entries.push(command(
                        "Saturated ↔ Aromatic",
                        Message::ToggleSelectedRing,
                        true,
                    ));
                }
                if !reshiki::ring_fills::selected_cycles(&self.tab.doc, &self.tab.selected)
                    .is_empty()
                {
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
                if reshiki::ring_arcs::toggle(&mut self.tab.doc.clone(), &self.tab.selected).is_ok()
                {
                    entries.push(command(
                        "Toggle inner ring curve",
                        Message::InspectorAction(inspector::Action::RingArc),
                        true,
                    ));
                }
                entries.push(Separator);
                entries.extend(
                    BondPreset::ALL.into_iter().map(|preset| {
                        command(preset.name(), Message::ApplyBondPreset(preset), bonds)
                    }),
                );
                entries
            }
            Page::AlignObjects => self.arrange_entries(&[&Command::HORIZONTAL, &Command::VERTICAL]),
            Page::Distribute => self.arrange_entries(&[&Command::DISTRIBUTE]),
            Page::Order => self.arrange_entries(&[&Command::ORDER]),
            Page::Arrange => [
                self.arrange_entries(&[&Command::HORIZONTAL, &Command::VERTICAL]),
                self.arrange_entries(&[&Command::DISTRIBUTE]),
                self.arrange_entries(&[&Command::ORDER]),
                self.arrange_entries(&[&Command::TRANSFORM]),
            ]
            .join(&Separator),
            Page::More(folded) => self
                .context_commands()
                .into_iter()
                .take(folded)
                .map(|c| command(c.menu, c.message, c.enabled))
                .collect(),
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
                    if matches!(action, Action::Page(_)) {
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
                        let first = self
                            .context_entries(*child)
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
        {
            return iced::advanced::widget::operate(reshiki::accessibility::FocusControl::new(
                format!("menu-{page:?}-{index}"),
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
                Entry::Item { label, action, .. } => Some(
                    text_width(label, 12.)
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
        let mut items = Vec::new();
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
        for (index, entry) in self.context_entries(page).into_iter().enumerate() {
            entries = match entry {
                Entry::Item {
                    label,
                    action,
                    enabled,
                } => {
                    let destructive = matches!(&action, Action::Run(message) if matches!(message.as_ref(), Message::Delete));
                    let accessible_name = label;
                    let label = text(label).size(12).width(Length::Fill);
                    let label = if destructive {
                        label.style(crate::appearance::text_color(Color::from_rgb8(167, 59, 51)))
                    } else {
                        label
                    };
                    let mut content = row![label]
                        .spacing(SHORTCUT_GAP)
                        .align_y(iced::Alignment::Center);
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
                        && (menu.focused == Some((level, index))
                            || menu
                                .children
                                .get(level)
                                .is_some_and(|child| child.anchor == index));
                    let item = reshiki::accessibility::button(
                        format!("menu-{page:?}-{index}"),
                        accessible_name,
                        content,
                    )
                    .padding([6, 10])
                    .width(Length::Fill)
                    .style(move |theme: &iced::Theme, status| {
                        let mut style = button::text(theme, status);
                        if active {
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
                    items.push(index);
                    entries.push(container(item).id(cascade::row_id(level, index)))
                }
                Entry::Separator => entries.push(horizontal_line()),
                Entry::Hint(label) => entries.push(
                    container(text(label).size(11).style(super::workspace::muted_text))
                        .padding([5, 10]),
                ),
            };
        }
        let content = container(
            scrollable(entries)
                .id(cascade::scroll_id(level))
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new()
                        .width(SCROLLBAR_WIDTH)
                        .scroller_width(SCROLLBAR_WIDTH)
                        .spacing(SCROLLBAR_GAP),
                ))
                .height(Length::Shrink),
        )
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
        let Some(menu) = &self.context_menu else {
            return base;
        };
        let panels = (0..=menu.children.len())
            .filter_map(|level| {
                menu.page_at(level)
                    .map(|page| self.context_panel(menu, page, level))
            })
            .collect();
        Element::new(cascade::Cascade::new(base, panels, menu))
    }
}
