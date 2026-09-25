//! Commands for the object under a secondary click, or the current selection.
use super::workspace::horizontal_line;
use super::{App, InspectorTab, Message, inspector};
use crate::canvas::Tool;
use iced::widget::{Space, button, column, container, mouse_area, opaque, scrollable, stack, text};
use iced::{Border, Color, Element, Length, Point, Task};
use reshiki::{
    bonds::BondPreset,
    editing::{Arrange, Transform},
};

#[derive(Debug, Clone, Copy, Default)]
pub enum Page {
    #[default]
    Main,
    Align,
    Bonds,
    Tilt,
    Attachments,
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
    fn context_commands_follow_the_target_without_modifying_the_drawing() -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let ring = app.doc.all_ids();
        let atom = *ring.first().ok_or("ring")?;
        let arrow = app.doc.next_id();
        app.doc.arrows.push(Arrow::new(
            arrow,
            World::new(100., 100.),
            World::new(160., 100.),
            Default::default(),
            Default::default(),
        ));
        let before = app.doc.clone();
        assert_eq!(
            labels(&app, Page::Main),
            ["Undo", "Redo", "Paste", "Select all", "Fit drawing"]
        );
        app.selected = vec![atom];
        let single = labels(&app, Page::Main);
        assert!(single.contains(&"Edit atom label…  Enter"));
        assert!(!single.contains(&"3D tilt…"));
        assert!(!single.contains(&"Bond appearance…"));
        assert!(single.contains(&"Select molecule"));
        app.selected = ring.clone();
        let molecule = labels(&app, Page::Main);
        assert!(!molecule.contains(&"Select molecule"));
        for expected in [
            "3D tilt…",
            "Arrange & transform…",
            "Bond appearance…",
            "Attachment points…",
        ] {
            assert!(molecule.contains(&expected), "Missing {expected}");
        }
        assert!(!molecule.contains(&"Bond in front"));
        assert!(labels(&app, Page::Bonds).contains(&"Bond in front"));
        assert!(!labels(&app, Page::Align).contains(&"Align middles"));
        app.selected.push(arrow);
        assert!(!labels(&app, Page::Main).contains(&"Attachment points…"));
        assert!(labels(&app, Page::Align).contains(&"Align middles"));
        app.selected = vec![arrow];
        let arrow_items = labels(&app, Page::Main);
        assert!(arrow_items.contains(&"Reverse arrow"));
        assert!(!arrow_items.contains(&"3D tilt…"));
        assert!(!arrow_items.contains(&"Bond appearance…"));
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
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
            app.doc = reshiki::rings::Preset::Regular.document(42., false);
            let ids = app.doc.all_ids();
            let source = app.doc.clone();
            reshiki::editing::append(&mut app.doc, &source, World::new(240., 0.));
            let before = app.doc.clone();
            app.edit(Edit::ContextMenu {
                position: Point::new(20., 20.),
                selected: ids.clone(),
            });
            run_item(&mut app, Page::Main, "3D tilt…")?;
            assert!(matches!(
                app.context_menu.as_ref().map(|s| s.page),
                Some(Page::Tilt)
            ));
            run_item(&mut app, Page::Tilt, "‹ Back")?;
            assert_eq!(app.doc, before);
            assert!(!app.history.can_undo());
            run_item(&mut app, Page::Main, "3D tilt…")?;
            run_item(&mut app, Page::Tilt, label)?;
            let mut expected = before.clone();
            reshiki::projection::tilt(&mut expected, &ids, degrees, around_x);
            assert_eq!(app.doc, expected);
            assert_eq!(app.selected, ids);
            assert!(app.context_menu.is_none());
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, expected);
            app.edit(Edit::ContextMenu {
                position: Point::new(20., 20.),
                selected: ids.clone(),
            });
            run_item(&mut app, Page::Tilt, "Drag to tilt")?;
            assert_eq!(app.tool, Tool::Tilt);
            assert_eq!(app.selected, ids);
            assert_eq!(app.doc, expected);
            assert!(app.context_menu.is_none());
        }
        Ok(())
    }

    #[test]
    fn tilt_drag_is_one_undo_step_and_matches_projection_preview() {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., true);
        let ids = app.doc.all_ids();
        let source = app.doc.clone();
        reshiki::editing::append(&mut app.doc, &source, World::new(240., 0.));
        let before = app.doc.clone();
        app.selected = ids.clone();
        let _ = app.update(Message::Tool(Tool::Tilt));
        assert_eq!(app.selected, ids);
        assert!(!app.history.can_undo());
        let mut preview = before.clone();
        crate::canvas::tilt::apply(&mut preview, &ids, 30., -15.);
        app.edit(Edit::Tilt {
            ids: ids.clone(),
            x: 30.,
            y: -15.,
        });
        assert_eq!(app.doc, preview);
        assert_eq!(app.doc.bonds, before.bonds);
        assert_eq!(app.tool, Tool::Tilt);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, preview);
    }

    #[test]
    fn context_alignment_preserves_molecular_geometry_and_undo_restores_every_group()
    -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let source = app.doc.clone();
        reshiki::editing::append(&mut app.doc, &source, World::new(240., 80.));
        let arrow = Arrow::new(
            app.doc.next_id(),
            World::new(90., -60.),
            World::new(160., -60.),
            Default::default(),
            Default::default(),
        );
        app.doc.arrows.push(arrow);
        let before = app.doc.clone();
        let ids = app.doc.all_ids();
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            selected: ids.clone(),
        });
        assert_eq!(app.alignment_count(), 3);
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::RefreshLabels);
        assert!(
            app.context_menu.is_some(),
            "Background label refresh must leave the menu open"
        );
        let _ = app.context_action(Action::Run(Box::new(Message::Arrange(
            Arrange::AlignVertical,
        ))));
        assert!(app.context_menu.is_none());
        let centers: Vec<_> = reshiki::editing::groups(&app.doc, &ids)
            .iter()
            .map(|ids| {
                let (lo, hi) =
                    reshiki::scene::selection_bounds(&app.doc, ids).ok_or("selection bounds")?;
                Ok::<_, String>((lo.y + hi.y) / 2.)
            })
            .collect::<Result<_, _>>()?;
        let center = centers.first().ok_or("alignment center")?;
        assert!(centers.iter().all(|y| (y - center).abs() < 0.001));
        for bond in &app.doc.bonds {
            let length = |doc: &reshiki::document::Document| -> Result<f32, String> {
                Ok(doc
                    .atom(bond.a)
                    .ok_or("bond start")?
                    .position
                    .distance(doc.atom(bond.b).ok_or("bond end")?.position))
            };
            assert!((length(&app.doc)? - length(&before)?).abs() < 0.001);
        }
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            selected: ids,
        });
        let _ = app.update(Message::Escape);
        assert!(app.context_menu.is_none());
        assert_eq!(app.doc, before);
        Ok(())
    }

    #[tokio::test]
    async fn inserted_examples_keep_existing_objects_and_can_be_removed_in_one_undo()
    -> Result<(), String> {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let before = app.doc.clone();
        let result = app
            .engine
            .request(reshiki::engine::Request::import_smiles("CCO"))
            .await?;
        let _ = app.update(Message::EngineDone {
            revision: app.revision,
            kind: super::super::Job::Insert,
            result: Box::new(Ok(result)),
        });
        assert_eq!(app.doc.atoms.len(), before.atoms.len() + 3);
        assert_eq!(app.selected.len(), 3);
        for atom in &before.atoms {
            assert_eq!(app.doc.atom(atom.id), Some(atom));
        }
        let (_, old_max) = before.bounds();
        assert!(
            app.selected
                .iter()
                .all(|id| app.doc.atom(*id).is_some_and(|a| a.position.x > old_max.x))
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Close,
    Page(Page),
    Run(Box<Message>),
    Properties(bool),
}
pub(super) struct State {
    pub position: Point,
    pub page: Page,
}

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
        let atoms = self.doc.atoms.iter().any(|a| self.selected.contains(&a.id));
        let bonds = self
            .doc
            .bonds
            .iter()
            .any(|b| self.selected.contains(&b.a) && self.selected.contains(&b.b));
        let tilt = crate::canvas::tilt::available(&self.doc, &self.selected);
        let multiple = self.alignment_count() >= 2;
        match page {
            Page::Main if self.selected.is_empty() => vec![
                command("Undo", Message::Undo, self.history.can_undo()),
                command("Redo", Message::Redo, self.history.can_redo()),
                Separator,
                command("Paste", Message::Paste, !self.clipboard_busy),
                command(
                    "Select all",
                    Message::SelectAll,
                    !self.doc.all_ids().is_empty(),
                ),
                Separator,
                command("Fit drawing", Message::Fit, true),
            ],
            Page::Main => {
                let mut entries = vec![];
                if atoms && self.atom_text_target().is_some() {
                    entries.push(command(
                        "Edit atom label…  Enter",
                        Message::AtomText(super::atom_text::Action::Begin(None)),
                        true,
                    ));
                }
                if atoms
                    && self
                        .selected
                        .iter()
                        .filter(|id| self.doc.atom(**id).is_some())
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
                        reshiki::editing::groups(&self.doc, &self.doc.all_ids())
                            .into_iter()
                            .filter(|ids| {
                                ids.iter().any(|id| {
                                    self.selected.contains(id) && self.doc.atom(*id).is_some()
                                })
                            })
                            .flatten()
                            .collect();
                    if connected.len() != self.selected.len()
                        || connected.iter().any(|id| !self.selected.contains(id))
                    {
                        entries.push(command(
                            "Select molecule",
                            Message::Canvas(crate::canvas::Edit::Select(connected)),
                            true,
                        ));
                    }
                }
                if tilt {
                    entries.push(submenu("3D tilt…", Page::Tilt));
                }
                entries.push(submenu("Arrange & transform…", Page::Align));
                if bonds {
                    entries.push(submenu("Bond appearance…", Page::Bonds));
                }
                let real_atoms = self
                    .doc
                    .atoms
                    .iter()
                    .filter(|a| {
                        self.selected.contains(&a.id) && a.element != "*" && a.centroid.is_empty()
                    })
                    .count();
                if (2..=300).contains(&real_atoms) && real_atoms == self.selected.len() {
                    entries.push(submenu("Attachment points…", Page::Attachments));
                }
                if self
                    .doc
                    .arrows
                    .iter()
                    .any(|a| self.selected.contains(&a.id))
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
                entries.push(command("Paste", Message::Paste, !self.clipboard_busy));
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
            Page::Tilt => vec![
                submenu("‹ Back", Page::Main),
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
                    submenu("‹ Back", Page::Main),
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
                    || !self.doc.outer_selected_groups(&self.selected).is_empty()
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
                    if !self.doc.outer_selected_groups(&self.selected).is_empty() {
                        entries.push(command("Ungroup", Message::Ungroup, true));
                    }
                }
                entries
            }
            Page::Bonds => {
                let mut entries = vec![submenu("‹ Back", Page::Main), Hint("Bond appearance")];
                entries.push(command("Bond in front", Message::BondDepth(true), bonds));
                entries.push(command("Bond behind", Message::BondDepth(false), bonds));
                if reshiki::rings::selected_cycle(&self.doc, &self.selected).is_some() {
                    entries.push(command(
                        "Saturated ↔ Aromatic  Shift+R",
                        Message::ToggleSelectedRing,
                        true,
                    ));
                }
                if !reshiki::ring_fills::selected_cycles(&self.doc, &self.selected).is_empty() {
                    entries.push(command(
                        "Color ring interior…",
                        Message::ColorScope(super::typography::ColorScope::Rings),
                        true,
                    ));
                    entries.push(command(
                        "Clear ring fill",
                        Message::ClearRingFill,
                        self.doc
                            .ring_fills
                            .iter()
                            .any(|f| f.atoms.iter().all(|id| self.selected.contains(id))),
                    ));
                }
                if reshiki::ring_arcs::toggle(&mut self.doc.clone(), &self.selected).is_ok() {
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
            Page::Attachments => vec![
                submenu("‹ Back", Page::Main),
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

    pub(super) fn context_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Close => self.context_menu = None,
            Action::Page(page) => {
                let origin = self.context_menu.as_ref().map(|menu| {
                    let (position, _, _) = self.context_geometry(menu);
                    position
                });
                if let Some(menu) = &mut self.context_menu {
                    menu.page = page;
                    if let Some(position) = origin {
                        menu.position = position;
                    }
                }
            }
            Action::Run(message) => {
                self.context_menu = None;
                return self.update(*message);
            }
            Action::Properties(molecular) => {
                self.context_menu = None;
                if molecular {
                    self.inspector_ui.update(inspector::Action::Section(
                        inspector::Section::Molecule,
                        true,
                    ));
                }
                return self.update(Message::Inspector(InspectorTab::Properties));
            }
        }
        Task::none()
    }

    fn context_geometry(&self, menu: &State) -> (Point, f32, f32) {
        let header = usize::from(matches!(menu.page, Page::Main) && !self.selected.is_empty());
        let count = self.context_entries(menu.page).len() + header;
        let width = 232_f32.min((self.viewport.width - 12.).max(1.));
        let height = ((count as f32 * 30.) + 12.).min((self.viewport.height - 12.).max(1.));
        let x = menu
            .position
            .x
            .min((self.viewport.width - width - 6.).max(6.))
            .max(6.);
        let y = menu
            .position
            .y
            .min((self.viewport.height - height - 6.).max(6.))
            .max(6.);
        (Point::new(x, y), width, height)
    }

    pub(super) fn with_context_menu<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(menu) = &self.context_menu else {
            return base;
        };
        let item = |label: String, action: Action, enabled: bool| {
            let destructive = matches!(&action, Action::Run(message) if matches!(message.as_ref(), Message::Delete));
            let label = text(label).size(12);
            let label = if destructive {
                label.color(Color::from_rgb8(167, 59, 51))
            } else {
                label
            };
            button(label)
                .padding([6, 10])
                .width(Length::Fill)
                .style(button::text)
                .on_press_maybe(enabled.then_some(Message::ContextMenu(action)))
        };
        let mut entries = column![].spacing(1);
        if matches!(menu.page, Page::Main) && !self.selected.is_empty() {
            entries = entries.push(
                container(
                    text(self.selection_summary())
                        .size(11)
                        .color(super::workspace::muted()),
                )
                .padding([5, 10]),
            );
        }
        for entry in self.context_entries(menu.page) {
            entries = match entry {
                Entry::Item {
                    label,
                    action,
                    enabled,
                } => entries.push(item(label.into(), action, enabled)),
                Entry::Separator => entries.push(horizontal_line()),
                Entry::Hint(label) => entries.push(
                    container(text(label).size(11).color(super::workspace::muted()))
                        .padding([5, 10]),
                ),
            };
        }
        let (position, width, height) = self.context_geometry(menu);
        let popup = container(scrollable(entries))
            .padding(5)
            .width(width)
            .max_height(height)
            .style(|_| container::Style {
                background: Some(Color::WHITE.into()),
                border: Border {
                    color: Color::from_rgb8(192, 204, 201),
                    width: 1.,
                    radius: 7.into(),
                },
                shadow: super::workspace::surface_shadow(iced::Shadow {
                    color: Color::from_rgba8(20, 40, 35, 0.18),
                    offset: iced::Vector::new(0., 4.),
                    blur_radius: 12.,
                }),
                ..Default::default()
            });
        stack![
            base,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .on_press(Message::ContextMenu(Action::Close))
            .on_right_press(Message::ContextMenu(Action::Close)),
            container(opaque(popup)).padding(iced::Padding {
                left: position.x,
                top: position.y,
                right: 0.,
                bottom: 0.
            })
        ]
        .into()
    }
}
