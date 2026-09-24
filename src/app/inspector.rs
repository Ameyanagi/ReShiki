//! Selection-focused properties and task-based export controls.
use super::workspace::{command, muted};
use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task};
use reshiki::{
    bonds::{BondPreset, DoublePosition},
    document::Document,
    editing::{Arrange, Transform},
    engine::{Analysis, ChemistryEngine, Request},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Bonds,
    BondDirection,
    Atoms,
    AtomColors,
    Arrange,
    Groups,
    Molecule,
    Chemistry,
    DrawingStyle,
    ArrowGeometry,
    ExportFigure,
    ExportChemical,
    ExportPages,
    ExportNative,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FigureFormat {
    #[default]
    Pdf,
    Svg,
    Png,
}
impl FigureFormat {
    const ALL: [Self; 3] = [Self::Pdf, Self::Svg, Self::Png];
    fn code(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Png => "png",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Pdf => "Vector figure at its physical publication size.",
            Self::Svg => "Editable vector artwork for layout and illustration.",
            Self::Png => "High-resolution image at 1200 dpi.",
        }
    }
}
impl std::fmt::Display for FigureFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pdf => "PDF · vector",
            Self::Svg => "SVG · editable vector",
            Self::Png => "PNG · 1200 dpi",
        })
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChemicalFormat {
    #[default]
    Mol,
    Smiles,
    Inchi,
    Cdxml,
}
impl ChemicalFormat {
    const ALL: [Self; 4] = [Self::Mol, Self::Smiles, Self::Inchi, Self::Cdxml];
    fn code(self) -> &'static str {
        match self {
            Self::Mol => "mol",
            Self::Smiles => "smiles",
            Self::Inchi => "inchi",
            Self::Cdxml => "cdxml",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Mol => "Molecular structure and coordinates for chemistry software.",
            Self::Smiles => "A text representation of the molecular structure.",
            Self::Inchi => "A standardized molecular identifier.",
            Self::Cdxml => "An editable drawing interchange file.",
        }
    }
}
impl std::fmt::Display for ChemicalFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Mol => "MOL structure",
            Self::Smiles => "SMILES text",
            Self::Inchi => "InChI identifier",
            Self::Cdxml => "CDXML drawing",
        })
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Section(Section, bool),
    Figure(FigureFormat),
    Chemical(ChemicalFormat),
    RefreshProperties,
    Centroid,
    Attachment(reshiki::attachments::Kind),
    DepthBonds,
    RingArc,
    OpenAtomColors,
    ColorElement(String),
    ColorWholeDrawing(bool),
    ColorHex(String),
    ApplyAtomColor,
    PropertiesCalculated(PropertyKey, Box<Result<Analysis, String>>),
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyKey {
    revision: u64,
    epoch: u64,
    atoms: Vec<u64>,
}
#[derive(Default)]
pub(super) struct State {
    expanded: HashMap<Section, bool>,
    figure: FigureFormat,
    chemical: ChemicalFormat,
    pending: Option<PropertyKey>,
    properties: Option<(PropertyKey, Result<Analysis, String>)>,
    color_element: Option<String>,
    color_whole_drawing: bool,
    color_hex: String,
}
impl State {
    pub(super) fn update(&mut self, action: Action) {
        match action {
            Action::Section(section, expanded) => {
                self.expanded.insert(section, expanded);
            }
            Action::Figure(format) => self.figure = format,
            Action::Chemical(format) => self.chemical = format,
            Action::ColorElement(element) => self.color_element = Some(element),
            Action::ColorWholeDrawing(value) => self.color_whole_drawing = value,
            Action::ColorHex(value) => self.color_hex = value,
            Action::RefreshProperties
            | Action::PropertiesCalculated(..)
            | Action::Centroid
            | Action::Attachment(_)
            | Action::RingArc
            | Action::OpenAtomColors
            | Action::ApplyAtomColor
            | Action::DepthBonds => {}
        }
    }
}
fn card<'a>(body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(body)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            border: Border {
                color: Color::from_rgb8(218, 225, 224),
                width: 1.,
                radius: 8.into(),
            },
            ..Default::default()
        })
        .into()
}
impl App {
    fn property_key(&self) -> Option<PropertyKey> {
        if self.selected.is_empty() {
            return None;
        }
        let ids: HashSet<_> = self
            .doc
            .expand_abbreviation_selection(&self.selected)
            .into_iter()
            .collect();
        Some(PropertyKey {
            revision: self.revision,
            epoch: self.file_epoch,
            atoms: self
                .doc
                .atoms
                .iter()
                .filter(|a| ids.contains(&a.id))
                .map(|a| a.id)
                .collect(),
        })
    }

    fn property_document(&self, key: &PropertyKey) -> Document {
        let mut part = reshiki::editing::selection(&self.doc, &key.atoms);
        reshiki::atom_labels::clear_computed(&mut part);
        part
    }

    pub(super) fn property_analysis(&self) -> Option<&Analysis> {
        let Some(key) = self.property_key() else {
            return self.analysis.as_ref();
        };
        self.inspector_ui
            .properties
            .as_ref()
            .filter(|(saved, _)| *saved == key)
            .and_then(|(_, result)| result.as_ref().ok())
    }

    pub(super) fn properties_subscription(&self) -> Subscription<Message> {
        if reshiki::attachments::present(&self.doc)
            || !self.inspector_open
            || self.inspector_tab != InspectorTab::Properties
            || self.busy
            || self.erase_stroke
            || self.cleanup.is_some()
            || self.inspector_ui.pending.is_some()
        {
            return Subscription::none();
        }
        let Some(key) = self.property_key().filter(|key| !key.atoms.is_empty()) else {
            return Subscription::none();
        };
        if self
            .inspector_ui
            .properties
            .as_ref()
            .is_some_and(|(saved, _)| *saved == key)
        {
            return Subscription::none();
        }
        // Changing the selection restarts the delay, so dragging does not queue chemistry jobs.
        iced::time::every(std::time::Duration::from_millis(350))
            .with(key)
            .map(|_| Message::InspectorAction(Action::RefreshProperties))
    }

    pub(super) fn inspector_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::OpenAtomColors => {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Properties;
                self.inspector_ui.expanded.insert(Section::AtomColors, true);
                self.inspector_ui.color_whole_drawing = self.selected.is_empty();
                if self.inspector_ui.color_hex.is_empty() {
                    self.inspector_ui.color_hex = "#205091".into();
                }
                Task::none()
            }
            Action::ApplyAtomColor => {
                let Some(color) = super::graphics::parse_color(&self.inspector_ui.color_hex) else {
                    self.status = "Enter a six-digit hex color, for example #205091".into();
                    self.error = true;
                    return Task::none();
                };
                let before = self.doc.clone();
                let ids = self.atom_color_targets();
                let style = self.doc.drawing_style.text_style();
                for atom in &mut self.doc.atoms {
                    if ids.contains(&atom.id) {
                        atom.text_style.get_or_insert_with(|| style.clone()).color = color;
                    }
                }
                self.changed(before);
                self.status = format!("Colored {} atom labels", ids.len());
                self.error = false;
                Task::none()
            }
            Action::RingArc => {
                let before = self.doc.clone();
                match reshiki::ring_arcs::toggle(&mut self.doc, &self.selected) {
                    Ok(on) => {
                        self.changed(before);
                        self.status = if on {
                            "Inner ring curve added · Bond orders retained"
                        } else {
                            "Inner ring curve removed"
                        }
                        .into();
                        self.error = false;
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
                Task::none()
            }
            Action::Centroid => {
                let before = self.doc.clone();
                match reshiki::projection::add_centroid(&mut self.doc, &self.selected) {
                    Ok(id) => {
                        self.selected = vec![id];
                        self.changed(before);
                        self.status =
                            "Centroid added · Draw a dashed contact from this point".into();
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
                Task::none()
            }
            Action::Attachment(kind) => {
                let before = self.doc.clone();
                match reshiki::attachments::add(&mut self.doc, &self.selected, kind) {
                    Ok(id) => {
                        self.selected = vec![id];
                        self.changed(before);
                        self.status = "Attachment point added · Draw a bond from * to the metal or substituent".into();
                        self.error = false;
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
                Task::none()
            }
            Action::DepthBonds => {
                let before = self.doc.clone();
                reshiki::projection::depth_bonds(&mut self.doc, &self.selected);
                self.changed(before);
                Task::none()
            }
            Action::RefreshProperties => {
                let Some(key) = self.property_key() else {
                    return self.update(Message::Analyze);
                };
                if key.atoms.is_empty() || self.inspector_ui.pending.is_some() {
                    return Task::none();
                }
                let request = Request::molecule("analyze", self.property_document(&key));
                self.inspector_ui.pending = Some(key.clone());
                self.inspector_ui.properties = None;
                let engine = self.engine.clone();
                Task::perform(
                    async move {
                        engine
                            .execute(request)
                            .await?
                            .analysis
                            .ok_or_else(|| "No molecular properties were returned.".into())
                    },
                    move |result| {
                        Message::InspectorAction(Action::PropertiesCalculated(
                            key.clone(),
                            Box::new(result),
                        ))
                    },
                )
            }
            Action::PropertiesCalculated(key, result) => {
                if self.inspector_ui.pending.as_ref() == Some(&key) {
                    self.inspector_ui.pending = None;
                    if self.property_key().as_ref() == Some(&key) {
                        // Never apply fragment labels or hydrogen counts to the original drawing.
                        self.inspector_ui.properties = Some((key, *result));
                    }
                }
                Task::none()
            }
            other => {
                self.inspector_ui.update(other);
                Task::none()
            }
        }
    }

    fn property_summary(&self) -> String {
        let scope = if self.selected.is_empty() {
            "Whole drawing"
        } else {
            "Selection"
        };
        if let Some(a) = self.property_analysis() {
            format!("{scope} · {}", a.formula)
        } else if self.property_key().is_some_and(|key| key.atoms.is_empty()) {
            "No atoms selected".into()
        } else {
            scope.into()
        }
    }

    pub(super) fn inspector_section<'a>(
        &'a self,
        section: Section,
        title: &'static str,
        summary: impl Into<String>,
        expanded: bool,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let expanded = self
            .inspector_ui
            .expanded
            .get(&section)
            .copied()
            .unwrap_or(expanded);
        let summary = summary.into();
        let mut heading = column![
            row![
                text(title).size(13).width(Length::Fill),
                text(if expanded { "−" } else { "+" })
                    .size(16)
                    .color(muted())
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(3);
        if !summary.is_empty() {
            heading = heading.push(text(summary).size(11).color(muted()));
        }
        let mut body = column![
            button(heading)
                .padding(10)
                .width(Length::Fill)
                .style(button::text)
                .on_press(Message::InspectorAction(Action::Section(
                    section, !expanded
                )))
        ];
        if expanded {
            body = body.push(container(content).padding(iced::Padding {
                top: 2.,
                right: 10.,
                bottom: 12.,
                left: 10.,
            }));
        }
        card(body)
    }

    pub(super) fn properties_panel(&self) -> Element<'_, Message> {
        let mut body = column![
            text("Properties").size(18),
            text(if self.selected.is_empty() {
                if self.tool.selects() {
                    "Select an object to edit its properties."
                } else {
                    "Settings apply to the next object you draw."
                }
                .into()
            } else {
                self.selection_summary()
            })
            .size(12)
            .color(muted())
        ]
        .spacing(10);
        body = body.push(self.atom_colors_panel());
        let molecular_first = self.selected.is_empty()
            || self.property_key().is_some_and(|key| !key.atoms.is_empty());
        if molecular_first {
            body = body.push(self.molecular_section());
        }
        if self.alignment_count() >= 2 {
            body = body.push(self.arrangement_panel(true));
        }
        if let Tool::RingPreset(preset) = self.tool {
            body = body.push(card(container(column![
                text(preset.to_string()).size(14),
                crate::canvas::layered::canvas(crate::canvas::DrawingThumbnail(preset.document(self.bond_drawing.length, false))).width(Length::Fill).height(90),
                text("Click to place. Drag to rotate or choose an attachment side. Click an atom to share it, or a bond to fuse.").size(12),
                text("Alt/Option on an atom connects the ring with a new bond. Each placement is one Undo step.").size(11).color(muted()),
                text(match preset {
                    reshiki::rings::Preset::Cyclopentadiene => "Hold Shift to move the double bonds.",
                    reshiki::rings::Preset::HaworthFive | reshiki::rings::Preset::HaworthSix => "Haworth outlines have a bold front edge. Templates → Carbohydrates contains oxygen scaffolds and defined α/β sugars. These blank outlines do not assign stereochemistry.",
                    _ => "Chair projections do not assign stereochemistry. Cleanup may redraw them as regular hexagons.",
                }).size(11).color(muted()),
            ].spacing(8)).padding(12)));
        }
        if matches!(self.tool, Tool::Graphic(_))
            || self
                .doc
                .graphics
                .iter()
                .any(|g| self.selected.contains(&g.id) && g.picture.is_none())
        {
            body = body.push(card(container(self.graphic_panel()).padding(12)));
        }
        if self
            .doc
            .graphics
            .iter()
            .any(|g| self.selected.contains(&g.id) && g.picture.is_some())
        {
            body = body.push(card(container(self.picture_panel()).padding(12)));
        }
        if self.tool == Tool::Arrow
            || self
                .doc
                .arrows
                .iter()
                .any(|a| self.selected.contains(&a.id))
        {
            body = body.push(card(container(self.arrow_panel()).padding(12)));
        }
        if self.tool == Tool::Text
            || (self.selected.len() == 1
                && self
                    .caption_target
                    .is_some_and(|id| self.selected.contains(&id)))
        {
            body = body.push(card(container(self.text_panel()).padding(12)));
        }
        if !self.selected.is_empty() {
            body = body.push(self.selection_panel());
        }
        if let Some(error) = &self.chemistry_notice {
            body = body.push(text(error).size(12).color(Color::from_rgb8(182, 66, 61)));
        }
        if !molecular_first {
            body = body.push(self.molecular_section());
        }
        let chemistry = column![
            command(
                "Reaction roles…",
                Message::Reaction(super::reactions::Action::Open)
            ),
            command(
                "Atom labels & numbering…",
                Message::Inspector(InspectorTab::Labels)
            ),
            command(
                "Chemical abbreviations…",
                Message::Inspector(InspectorTab::Abbreviations)
            ),
        ]
        .spacing(3);
        body = body.push(self.inspector_section(
            Section::Chemistry,
            "Labels & chemistry",
            "",
            false,
            chemistry,
        ));
        body.push(
            self.inspector_section(
                Section::DrawingStyle,
                "Drawing style",
                &self.doc.drawing_style.name,
                false,
                column![
                    text(format!(
                        "{} {} pt\nBonds {} pt · Lines {} pt\nPNG 1200 dpi",
                        self.doc.drawing_style.font_family,
                        self.doc.drawing_style.font_size_pt,
                        self.doc.drawing_style.bond_length_pt,
                        self.doc.drawing_style.line_width_pt
                    ))
                    .size(12)
                    .color(muted()),
                    command(
                        "Edit drawing style…",
                        Message::DrawingStyle(super::document_styles::Action::Open)
                    ),
                ]
                .spacing(8),
            ),
        )
        .into()
    }

    fn molecular_section(&self) -> Element<'_, Message> {
        self.inspector_section(
            Section::Molecule,
            "Molecular properties",
            self.property_summary(),
            self.selected.is_empty() && self.tool == Tool::Select && !self.doc.atoms.is_empty(),
            self.molecular_properties(),
        )
    }

    fn color_elements(&self) -> Vec<String> {
        self.doc
            .atoms
            .iter()
            .filter(|a| a.centroid.is_empty())
            .filter(|a| self.inspector_ui.color_whole_drawing || self.selected.contains(&a.id))
            .map(|a| a.element.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn atom_color_targets(&self) -> Vec<u64> {
        let elements = self.color_elements();
        let element = self
            .inspector_ui
            .color_element
            .as_ref()
            .filter(|e| elements.contains(e))
            .or(elements.first());
        self.doc
            .atoms
            .iter()
            .filter(|a| a.centroid.is_empty() && Some(&a.element) == element)
            .filter(|a| self.inspector_ui.color_whole_drawing || self.selected.contains(&a.id))
            .map(|a| a.id)
            .collect()
    }

    fn atom_colors_panel(&self) -> Element<'_, Message> {
        let elements = self.color_elements();
        let element = self
            .inspector_ui
            .color_element
            .clone()
            .filter(|e| elements.contains(e))
            .or_else(|| elements.first().cloned());
        let count = self.atom_color_targets().len();
        let body = column![
            checkbox(self.inspector_ui.color_whole_drawing)
                .label("Whole drawing")
                .on_toggle(|v| Message::InspectorAction(Action::ColorWholeDrawing(v))),
            text(if self.inspector_ui.color_whole_drawing {
                "Choose an element to color throughout the drawing."
            } else {
                "Only matching atoms in the selection are colored."
            })
            .size(11)
            .color(muted()),
            pick_list(elements, element, |e| Message::InspectorAction(
                Action::ColorElement(e)
            ))
            .placeholder("Select atoms first")
            .width(Length::Fill)
            .text_size(12)
            .padding(7),
            row![
                text_input("#205091", &self.inspector_ui.color_hex)
                    .on_input(|s| Message::InspectorAction(Action::ColorHex(s)))
                    .on_submit(Message::InspectorAction(Action::ApplyAtomColor))
                    .size(12)
                    .padding(7),
                command("Apply", Message::InspectorAction(Action::ApplyAtomColor)).on_press_maybe(
                    (count > 0).then_some(Message::InspectorAction(Action::ApplyAtomColor))
                ),
            ]
            .spacing(6),
            text(format!("{count} matching atoms"))
                .size(11)
                .color(muted()),
        ]
        .spacing(8);
        self.inspector_section(
            Section::AtomColors,
            "Color atoms by element",
            "",
            false,
            body,
        )
    }

    fn molecular_properties(&self) -> Element<'_, Message> {
        if reshiki::attachments::present(&self.doc) {
            let selected = (!self.selected.is_empty())
                .then(|| reshiki::editing::selection(&self.doc, &self.selected));
            let doc = selected.as_ref().unwrap_or(&self.doc);
            let mut body = column![
                text(reshiki::attachments::ANALYSIS_NOTICE)
                    .size(11)
                    .color(muted())
            ]
            .spacing(8);
            if let Ok(composition) = reshiki::attachments::composition(doc) {
                body = body
                    .push(
                        text(format!(
                            "{} · {:.3} g/mol",
                            composition.formula, composition.mass
                        ))
                        .size(14),
                    )
                    .push(
                        text(format!(
                            "{} defined atoms · Attachment points excluded",
                            doc.atoms.iter().filter(|a| a.element != "*").count()
                        ))
                        .size(11)
                        .color(muted()),
                    );
            }
            return container(body).padding(10).into();
        }
        let key = self.property_key();
        let ids: HashSet<_> = key
            .as_ref()
            .map(|key| key.atoms.iter().copied().collect())
            .unwrap_or_default();
        let (atoms, bonds) = if key.is_some() {
            (
                ids.len(),
                self.doc
                    .bonds
                    .iter()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                    .count(),
            )
        } else {
            (self.doc.atoms.len(), self.doc.bonds.len())
        };
        let mut body = column![
            text(format!("{} atoms · {} bonds", atoms, bonds))
                .size(11)
                .color(muted())
        ]
        .spacing(8);
        if key.is_some()
            && self
                .doc
                .bonds
                .iter()
                .any(|b| ids.contains(&b.a) != ids.contains(&b.b))
        {
            body = body.push(
                text("Selected fragment: implicit hydrogens are recalculated at cut bonds.")
                    .size(11)
                    .color(muted()),
            );
        }
        if let Some(a) = self.property_analysis() {
            for (label, value) in [
                ("Weight (g/mol)", format!("{:.3}", a.mass)),
                ("Exact mass (Da)", format!("{:.5}", a.exact_mass)),
                ("cLogP", format!("{:.2}", a.logp)),
                ("TPSA (Å²)", format!("{:.2}", a.tpsa)),
                ("H-bond donors", a.donors.to_string()),
                ("H-bond acceptors", a.acceptors.to_string()),
                ("Rings", a.rings.to_string()),
                ("Unpaired electrons", a.unpaired_electrons.to_string()),
            ] {
                body = body.push(
                    row![
                        text(label).size(11).color(muted()).width(Length::Fill),
                        text(value).size(12)
                    ]
                    .align_y(Alignment::Center),
                );
            }
            body = body
                .push(text("Canonical SMILES").size(12))
                .push(
                    text(if a.smiles.is_empty() {
                        "SMILES cannot represent these hydrogen or partial bonds."
                    } else {
                        &a.smiles
                    })
                    .size(11)
                    .wrapping(text::Wrapping::Glyph),
                )
                .push(
                    command("Copy SMILES", Message::CopySmiles)
                        .on_press_maybe((!a.smiles.is_empty()).then_some(Message::CopySmiles)),
                );
        } else if atoms == 0 {
            return body.push(text(if key.is_some() { "Select atoms or bonds to calculate their properties. Clear the selection to use the whole drawing." } else { "Draw or import a molecule to calculate its properties." }).size(12).color(muted())).into();
        } else if let Some((_, Err(error))) = self
            .inspector_ui
            .properties
            .as_ref()
            .filter(|(saved, _)| Some(saved) == key.as_ref())
        {
            body = body.push(text(error).size(12).color(Color::from_rgb8(182, 66, 61)));
        } else {
            body = body.push(
                text(if key.is_some() {
                    "Calculating selection…"
                } else {
                    "Check the structure to calculate its formula and properties."
                })
                .size(12)
                .color(muted()),
            );
        }
        body.push(
            command(
                if key.is_some() {
                    "Recalculate selection"
                } else {
                    "Check structure"
                },
                Message::InspectorAction(Action::RefreshProperties),
            )
            .on_press_maybe(
                (!self.busy && self.inspector_ui.pending.is_none())
                    .then_some(Message::InspectorAction(Action::RefreshProperties)),
            ),
        )
        .into()
    }

    pub(super) fn alignment_count(&self) -> usize {
        reshiki::editing::groups(&self.doc, &self.selected).len()
    }

    fn arrangement_panel(&self, multiple: bool) -> Element<'_, Message> {
        let mut arrange = column![
            text("Rotate & reflect").size(11).color(muted()),
            row![
                command("↶ 30°", Message::Transform(Transform::Rotate(-30.))).width(Length::Fill),
                command("↷ 30°", Message::Transform(Transform::Rotate(30.))).width(Length::Fill)
            ]
            .spacing(6),
            row![
                command("Flip H", Message::Transform(Transform::FlipHorizontal))
                    .width(Length::Fill),
                command("Flip V", Message::Transform(Transform::FlipVertical)).width(Length::Fill)
            ]
            .spacing(6),
            text("3D tilt").size(11).color(muted()),
            row![
                command("X −15°", Message::Transform(Transform::TiltX(-15.))).width(Length::Fill),
                command("X +15°", Message::Transform(Transform::TiltX(15.))).width(Length::Fill),
            ]
            .spacing(4),
            row![
                command("Y −15°", Message::Transform(Transform::TiltY(-15.))).width(Length::Fill),
                command("Y +15°", Message::Transform(Transform::TiltY(15.))).width(Length::Fill),
            ]
            .spacing(4),
            command(
                "Emphasize front bonds",
                Message::InspectorAction(Action::DepthBonds)
            )
            .width(Length::Fill),
            row![
                command("Add centroid", Message::InspectorAction(Action::Centroid))
                    .width(Length::Fill),
                command("Dummy atom (*)", Message::Element("*".into())).width(Length::Fill),
            ]
            .spacing(4),
            command("Add multi-center attachment", Message::InspectorAction(Action::Attachment(reshiki::attachments::Kind::MultiCenter)))
                .width(Length::Fill),
            command("Add variable attachment", Message::InspectorAction(Action::Attachment(reshiki::attachments::Kind::Variable)))
                .width(Length::Fill),
            text("Select the target atoms, then add a point. Multi-center attaches to all; variable attaches to one of the selected positions.")
                .size(11)
                .color(muted()),
            text("Align horizontally").size(11).color(muted()),
            row![
                command("Left", Message::Arrange(Arrange::AlignLeft)).width(Length::Fill),
                command("Center", Message::Arrange(Arrange::AlignHorizontal)).width(Length::Fill),
                command("Right", Message::Arrange(Arrange::AlignRight)).width(Length::Fill)
            ]
            .spacing(2),
            text("Align vertically").size(11).color(muted()),
            row![
                command("Top", Message::Arrange(Arrange::AlignTop)).width(Length::Fill),
                command("Middles", Message::Arrange(Arrange::AlignVertical)).width(Length::Fill),
                command("Bottom", Message::Arrange(Arrange::AlignBottom)).width(Length::Fill)
            ]
            .spacing(2),
            text("Distribute").size(11).color(muted()),
            row![
                command(
                    "Horizontally",
                    Message::Arrange(Arrange::DistributeHorizontal)
                )
                .width(Length::Fill),
                command("Vertically", Message::Arrange(Arrange::DistributeVertical))
                    .width(Length::Fill)
            ]
            .spacing(2),
            text("Drag a box corner to resize. Drag the top handle to rotate; Shift snaps to 15°.")
                .size(11)
                .color(muted()),
        ]
        .spacing(6);
        if reshiki::rings::selected_cycle(&self.doc, &self.selected).is_some() {
            arrange = arrange.push(
                command(
                    "Saturated ↔ Aromatic · Shift+R",
                    Message::ToggleSelectedRing,
                )
                .width(Length::Fill),
            );
        }
        self.inspector_section(
            Section::Arrange,
            "Arrange & transform",
            if multiple {
                "Align selected molecules, arrows & groups"
            } else {
                ""
            },
            multiple,
            arrange,
        )
    }

    fn selection_panel(&self) -> Element<'_, Message> {
        let multiple = self.alignment_count() >= 2;
        let atoms: Vec<_> = self
            .doc
            .atoms
            .iter()
            .filter(|a| self.selected.contains(&a.id))
            .collect();
        let bonds: Vec<_> = self
            .doc
            .bonds
            .iter()
            .filter(|b| self.selected.contains(&b.a) && self.selected.contains(&b.b))
            .collect();
        let mut body = column![].spacing(10);
        if self.atom_text_target().is_some() {
            body = body.push(
                command(
                    "Edit atom label… · Enter",
                    Message::AtomText(super::atom_text::Action::Begin(None)),
                )
                .width(Length::Fill),
            );
        }
        if let Some(first) = bonds.first() {
            let preset = BondPreset::of(first)
                .filter(|p| bonds.iter().all(|b| BondPreset::of(b) == Some(*p)));
            let mut controls = column![
                text("Style").size(11).color(muted()),
                pick_list(BondPreset::ALL, preset, Message::ApplyBondPreset)
                    .placeholder("Mixed bond styles")
                    .text_size(12)
                    .padding(7)
                    .width(Length::Fill),
            ]
            .spacing(7);
            if bonds.iter().any(|b| [2, 7].contains(&b.order)) {
                let position = bonds
                    .iter()
                    .find(|b| [2, 7].contains(&b.order))
                    .map(|b| b.double_position)
                    .filter(|p| {
                        bonds
                            .iter()
                            .filter(|b| [2, 7].contains(&b.order))
                            .all(|b| b.double_position == *p)
                    });
                controls = controls
                    .push(text("Second line placement").size(11).color(muted()))
                    .push(
                        pick_list(DoublePosition::ALL, position, Message::BondPosition)
                            .placeholder("Mixed positions")
                            .text_size(12)
                            .padding(7)
                            .width(Length::Fill),
                    );
            }
            controls = controls.push(text("Color").size(11).color(muted())).push(
                row![
                    text_input("#000000", &self.bond_color_input)
                        .on_input(Message::BondColor)
                        .on_submit(Message::ApplyBondColor)
                        .size(12)
                        .padding(7),
                    command("Apply", Message::ApplyBondColor),
                ]
                .spacing(6),
            );
            if atoms.len() >= 3 {
                controls = controls.push(
                    command("Toggle aromatic circle", Message::AromaticDisplay)
                        .on_press_maybe((!self.busy).then_some(Message::AromaticDisplay)),
                );
                controls = controls.push(command("Toggle inner ring curve", Message::InspectorAction(Action::RingArc)))
                    .push(text("Select consecutive ring atoms for a partial curve, or the whole ring for a circle. Bond orders stay unchanged.").size(11).color(muted()));
            }
            body = body.push(self.inspector_section(
                Section::Bonds,
                "Bond appearance",
                "",
                true,
                controls,
            ));
            body = body.push(
                self.inspector_section(
                    Section::BondDirection,
                    "Crossings & direction",
                    "",
                    false,
                    column![
                        row![
                            command("In front", Message::BondDepth(true)).width(Length::Fill),
                            command("Behind", Message::BondDepth(false)).width(Length::Fill)
                        ]
                        .spacing(6),
                        command("Reverse bonds", Message::ReverseBonds)
                    ]
                    .spacing(6),
                ),
            );
        }
        if let Some(first) = atoms.first() {
            let count = first.radical_electrons;
            let count = atoms
                .iter()
                .all(|a| a.radical_electrons == count)
                .then_some(count);
            let mut controls = column![
                row![
                    text("Charge").size(12).width(Length::Fill),
                    command("−", Message::Charge(-1)),
                    command("+", Message::Charge(1))
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                row![
                    text_input("Isotope mass", &self.isotope)
                        .on_input(Message::Isotope)
                        .on_submit(Message::ApplyIsotope)
                        .size(12)
                        .padding(7),
                    command("Set", Message::ApplyIsotope)
                ]
                .spacing(6),
                row![
                    text("Unpaired electrons").size(11).width(Length::Fill),
                    pick_list([0u8, 1, 2], count, Message::AtomRadical)
                        .placeholder("Mixed")
                        .text_size(12)
                        .padding(6)
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(8);
            for a in atoms.iter().filter(|a| !a.marks.is_empty()) {
                controls =
                    controls.push(text(format!("{} · positioned marks", a.element)).size(12));
                for (index, mark) in a.marks.iter().enumerate() {
                    controls = controls.push(
                        row![
                            text(match mark.kind {
                                reshiki::scientific::MarkKind::Charge => "Charge",
                                reshiki::scientific::MarkKind::CircledCharge => "Circled charge",
                                reshiki::scientific::MarkKind::Radical => "Radical",
                                reshiki::scientific::MarkKind::RadicalIon => "Radical ion",
                                reshiki::scientific::MarkKind::LonePair => "Lone pair",
                                reshiki::scientific::MarkKind::LonePairBar => "Lone pair bar",
                            })
                            .size(11)
                            .width(Length::Fill),
                            command("Rotate", Message::RotateMark(a.id, index)),
                            command("Remove", Message::RemoveMark(a.id, index))
                        ]
                        .spacing(4),
                    );
                }
                controls = controls.push(command(
                    if self.tool == Tool::EditPoints {
                        "Finish positioning marks"
                    } else {
                        "Position atom marks"
                    },
                    Message::Tool(if self.tool == Tool::EditPoints {
                        Tool::Select
                    } else {
                        Tool::EditPoints
                    }),
                ));
            }
            body = body.push(
                self.inspector_section(
                    Section::Atoms,
                    "Atom details",
                    "Charge, isotope & electron marks",
                    atoms.len() == 1
                        || atoms
                            .iter()
                            .any(|a| !a.marks.is_empty() || a.radical_electrons != 0),
                    controls,
                ),
            );
        }
        if !multiple {
            body = body.push(self.arrangement_panel(false));
        }
        let groups = self.doc.outer_selected_groups(&self.selected);
        let mut grouping = column![
            row![
                command("Group", Message::Group)
                    .on_press_maybe(self.can_group().then_some(Message::Group))
                    .width(Length::Fill),
                command("Ungroup", Message::Ungroup)
                    .on_press_maybe((!groups.is_empty()).then_some(Message::Ungroup))
                    .width(Length::Fill)
            ]
            .spacing(6),
            command("Invert selection", Message::InvertSelection),
            pick_list(
                [
                    reshiki::graphics::GraphicKind::Brackets,
                    reshiki::graphics::GraphicKind::Parentheses,
                    reshiki::graphics::GraphicKind::Braces,
                    reshiki::graphics::GraphicKind::Rectangle,
                    reshiki::graphics::GraphicKind::RoundedRectangle
                ],
                None::<reshiki::graphics::GraphicKind>,
                Message::AddFrame
            )
            .placeholder("Add frame…")
            .text_size(12)
            .padding(7)
            .width(Length::Fill),
        ]
        .spacing(6);
        if !groups.is_empty() {
            let integral = self
                .doc
                .groups
                .iter()
                .filter(|g| groups.contains(&g.id))
                .all(|g| g.integral);
            grouping = grouping
                .push(
                    checkbox(integral)
                        .label("Integral group")
                        .size(14)
                        .text_size(12)
                        .on_toggle(Message::IntegralGroup),
                )
                .push(
                    text(
                        "Integral groups stay whole with Option/Alt-click. Ungroup releases them.",
                    )
                    .size(11)
                    .color(muted()),
                );
        }
        body = body.push(self.inspector_section(
            Section::Groups,
            "Grouping & frames",
            "",
            false,
            grouping,
        ));
        body.push(
            button(
                text("Delete selection")
                    .size(12)
                    .color(Color::from_rgb8(174, 57, 52)),
            )
            .style(button::text)
            .padding(7)
            .on_press(Message::Delete),
        )
        .into()
    }

    pub(super) fn export_panel(&self) -> Element<'_, Message> {
        let figure = self.inspector_ui.figure;
        let chemical = self.inspector_ui.chemical;
        let mut figures = column![
            pick_list(FigureFormat::ALL, Some(figure), |f| {
                Message::InspectorAction(Action::Figure(f))
            })
            .text_size(12)
            .padding(8)
            .width(Length::Fill),
            text(figure.description()).size(12).color(muted()),
            button(text(format!("Export {}…", figure.code().to_uppercase())).size(13))
                .padding(10)
                .width(Length::Fill)
                .on_press_maybe((!self.busy).then_some(Message::Export(figure.code()))),
        ]
        .spacing(9);
        if reshiki::clipboard::available() {
            figures = figures
                .push(
                    command(
                        super::platform_shortcut("Copy image · ⇧⌘C", "Copy image · Ctrl+Shift+C"),
                        Message::CopyImage,
                    )
                    .on_press_maybe((!self.clipboard_busy).then_some(Message::CopyImage))
                    .width(Length::Fill),
                )
                .push(
                    text(if self.selected.is_empty() {
                        "Copy image uses the full drawing."
                    } else {
                        "Copy image uses the selected objects."
                    })
                    .size(11)
                    .color(muted()),
                );
        }
        let mut body = column![
            text("Export").size(18),
            text("File exports use the full drawing.")
                .size(12)
                .color(muted()),
            self.inspector_section(
                Section::ExportFigure,
                "Figure",
                &self.doc.drawing_style.name,
                true,
                figures
            ),
            self.inspector_section(
                Section::ExportChemical,
                "Structure & exchange",
                "MOL · SMILES · InChI · CDXML",
                false,
                column![
                    pick_list(ChemicalFormat::ALL, Some(chemical), |f| {
                        Message::InspectorAction(Action::Chemical(f))
                    })
                    .text_size(12)
                    .padding(8)
                    .width(Length::Fill),
                    text(chemical.description()).size(12).color(muted()),
                    button(text(format!("Export {}…", chemical.code().to_uppercase())).size(13))
                        .padding(10)
                        .width(Length::Fill)
                        .on_press_maybe((!self.busy).then_some(Message::Export(chemical.code()))),
                    command(
                        "Reaction roles & export…",
                        Message::Reaction(super::reactions::Action::Open)
                    ),
                ]
                .spacing(9)
            ),
        ]
        .spacing(10);
        let mut pages = column![command(
            "Page setup…",
            Message::Pages(super::pages::Action::Show)
        )]
        .spacing(6);
        if self.doc.page_layout.is_some() {
            pages = pages.push(command(
                "PDF · all pages",
                Message::Pages(super::pages::Action::Export),
            ));
        }
        if reshiki::printing::available() {
            pages = pages.push(
                command(
                    super::platform_shortcut("Print… · ⌘P", "Print… · Ctrl+P"),
                    Message::Printing(super::printing::Action::Start(
                        reshiki::printing::Scope::Document,
                    )),
                )
                .on_press_maybe(self.printing.active.is_none().then_some(Message::Printing(
                    super::printing::Action::Start(reshiki::printing::Scope::Document),
                )))
                .width(Length::Fill),
            );
            if !self.selected.is_empty() {
                pages = pages.push(
                    command(
                        "Print selection…",
                        Message::Printing(super::printing::Action::Start(
                            reshiki::printing::Scope::Selection,
                        )),
                    )
                    .on_press_maybe(self.printing.active.is_none().then_some(Message::Printing(
                        super::printing::Action::Start(reshiki::printing::Scope::Selection),
                    )))
                    .width(Length::Fill),
                );
            }
        }
        body = body.push(self.inspector_section(
            Section::ExportPages,
            "Pages & printing",
            "",
            self.doc.page_layout.is_some(),
            pages,
        ));
        body.push(self.inspector_section(
            Section::ExportNative,
            "Editable document",
            ".rsk · retains the complete drawing",
            true,
            command("Save native document…", Message::SaveAs).width(Length::Fill),
        ))
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn calculate(app: &mut App) {
        let key = app.property_key().unwrap();
        let doc = app.property_document(&key);
        let _ = app.inspector_action(Action::RefreshProperties);
        let result = app
            .engine
            .execute(Request::molecule("analyze", doc))
            .await
            .unwrap()
            .analysis
            .unwrap();
        let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(Ok(result))));
    }

    #[test]
    fn element_colors_respect_scope_and_undo_without_touching_bonds() {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("Cu", reshiki::document::Point::default());
        let b = app
            .doc
            .add_atom("Cu", reshiki::document::Point::new(40., 0.));
        let n = app
            .doc
            .add_atom("N", reshiki::document::Point::new(80., 0.));
        app.doc.add_bond(b, n, 5, "plain");
        app.doc.atom_mut(a).unwrap().text_style = Some(reshiki::typography::TextStyle {
            italic: true,
            ..Default::default()
        });
        app.selected = vec![a, n];
        let original = app.doc.clone();
        for action in [
            Action::OpenAtomColors,
            Action::ColorElement("Cu".into()),
            Action::ColorHex("#205091".into()),
            Action::ApplyAtomColor,
        ] {
            let _ = app.inspector_action(action);
        }
        assert_eq!(
            app.doc.atom(a).unwrap().text_style.as_ref().unwrap().color,
            [32, 80, 145]
        );
        assert!(app.doc.atom(a).unwrap().text_style.as_ref().unwrap().italic);
        assert_eq!(app.doc.atom(b), original.atom(b));
        assert_eq!(app.doc.atom(n), original.atom(n));
        assert_eq!(app.doc.bonds, original.bonds);
        assert_eq!(app.selected, vec![a, n]);
        assert!(app.history.undo(&mut app.doc));
        assert_eq!(app.doc, original);
        let _ = app.inspector_action(Action::ColorWholeDrawing(true));
        let _ = app.inspector_action(Action::ApplyAtomColor);
        assert_eq!(
            app.doc.atom(b).unwrap().text_style.as_ref().unwrap().color,
            [32, 80, 145]
        );
        assert_eq!(app.doc.atom(n), original.atom(n));
        let colored = app.doc.clone();
        let _ = app.inspector_action(Action::ColorHex("bad".into()));
        let _ = app.inspector_action(Action::ApplyAtomColor);
        assert_eq!(app.doc, colored);
    }

    #[test]
    fn inner_curve_is_one_undo_step_and_keeps_chemical_orders() {
        let (mut app, _) = App::new();
        let ids = reshiki::editing::ring(
            &mut app.doc,
            reshiki::document::Point::default(),
            5,
            false,
            0.,
        );
        app.selected = ids[..3].to_vec();
        let original = app.doc.clone();
        let _ = app.inspector_action(Action::RingArc);
        assert_eq!(app.doc.bonds.iter().filter(|b| b.ring_arc).count(), 2);
        assert!(app.history.undo(&mut app.doc));
        assert_eq!(app.doc, original);
        assert!(app.history.redo(&mut app.doc));
        assert_eq!(reshiki::ring_arcs::render(&app.doc).primitives.len(), 1);
    }

    #[tokio::test]
    async fn selected_properties_use_only_the_fragment_without_changing_the_drawing() {
        let (mut app, _) = App::new();
        let result = app
            .engine
            .execute(Request::import_smiles("CCO.CN"))
            .await
            .unwrap();
        app.doc = result.document.unwrap();
        app.analysis = result.analysis;
        let whole = app.analysis.as_ref().unwrap().formula.clone();
        let before = app.doc.clone();
        app.selected = app.doc.atoms[..3].iter().map(|a| a.id).collect();
        calculate(&mut app).await;
        assert_eq!(app.property_analysis().unwrap().formula, "C2H6O");
        assert_eq!(app.property_analysis().unwrap().smiles, "CCO");
        app.selected.pop();
        assert!(
            app.property_analysis().is_none(),
            "Old results must disappear immediately"
        );
        calculate(&mut app).await;
        assert_eq!(app.property_analysis().unwrap().formula, "C2H6");
        app.selected.clear();
        assert_eq!(app.property_analysis().unwrap().formula, whole);
        app.selected = vec![u64::MAX];
        assert!(
            app.property_analysis().is_none(),
            "Artwork selection must not show whole-drawing values"
        );
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        assert_eq!(app.revision, 0);
    }

    #[tokio::test]
    async fn stale_property_results_cannot_replace_a_new_selection_or_document() {
        let (mut app, _) = App::new();
        let result = app
            .engine
            .execute(Request::import_smiles("CCO"))
            .await
            .unwrap();
        app.doc = result.document.unwrap();
        let analysis = result.analysis.unwrap();
        app.selected = vec![app.doc.atoms[0].id];
        let old = app.property_key().unwrap();
        let _ = app.inspector_action(Action::RefreshProperties);
        app.selected = vec![app.doc.atoms[2].id];
        let _ = app.inspector_action(Action::PropertiesCalculated(
            old,
            Box::new(Ok(analysis.clone())),
        ));
        assert!(app.inspector_ui.pending.is_none());
        assert!(app.property_analysis().is_none());
        let old = app.property_key().unwrap();
        let _ = app.inspector_action(Action::RefreshProperties);
        app.file_epoch += 1;
        let _ = app.inspector_action(Action::PropertiesCalculated(old, Box::new(Ok(analysis))));
        assert!(app.property_analysis().is_none());
        assert!(app.inspector_ui.pending.is_none());
    }

    #[test]
    fn aromatic_shortcut_keeps_tool_size_and_selected_ring_topology() {
        let (mut app, _) = App::new();
        for size in 3..=8 {
            let _ = app.update(Message::RingSize(size));
            let _ = app.update(Message::AromaticRing(false));
            let _ = app.update(Message::ToggleAromaticRing);
            assert_eq!(app.ring_size, size);
            assert!(app.aromatic_ring);
            let _ = app.update(Message::ToggleAromaticRing);
            assert_eq!(app.ring_size, size);
            assert!(!app.aromatic_ring);
        }
        app.selected = reshiki::editing::ring(
            &mut app.doc,
            reshiki::document::Point::default(),
            5,
            false,
            5.,
        );
        app.tool = Tool::Select;
        let before = app.doc.clone();
        let _ = app.update(Message::ToggleAromaticRing);
        assert_eq!(app.doc.atoms.len(), 5);
        assert!(app.doc.bonds.iter().all(|b| b.order == 4));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }

    #[test]
    fn tilted_ring_and_centroid_are_atomic_undoable_edits() {
        let (mut app, _) = App::new();
        app.selected = reshiki::editing::ring(
            &mut app.doc,
            reshiki::document::Point::new(100., 100.),
            5,
            false,
            5.,
        );
        let planar = app.doc.clone();
        let ring = app.selected.clone();
        let _ = app.update(Message::Transform(Transform::TiltX(60.)));
        let tilted = app.doc.clone();
        assert!(tilted.atoms.iter().any(|a| a.depth.abs() > 1.));
        assert_eq!(tilted.bonds, planar.bonds);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, planar);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, tilted);
        app.selected = ring;
        let _ = app.update(Message::InspectorAction(Action::Centroid));
        assert_eq!(app.doc.atoms.len(), 6);
        let centroid = app.selected[0];
        assert_eq!(app.doc.atom(centroid).unwrap().centroid.len(), 5);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, tilted);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc.atom(centroid).unwrap().element, "*");
        app.doc.validate().unwrap();
    }

    #[test]
    fn semantic_attachment_creation_is_undoable() -> Result<(), String> {
        for kind in [
            reshiki::attachments::Kind::MultiCenter,
            reshiki::attachments::Kind::Variable,
        ] {
            let (mut app, _) = App::new();
            app.selected = reshiki::editing::ring(
                &mut app.doc,
                reshiki::document::Point::new(100., 100.),
                6,
                true,
                5.,
            );
            let before = app.doc.clone();
            let _ = app.update(Message::InspectorAction(Action::Attachment(kind)));
            let id = *app.selected.first().ok_or("No point selected")?;
            let point = app.doc.atom(id).ok_or("Missing point")?;
            assert_eq!(point.attachment, Some(kind));
            assert_eq!(point.centroid.len(), 6);
            let after = app.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, after);
            app.doc.validate()?;
        }
        Ok(())
    }

    #[test]
    fn inspector_preferences_preserve_drawing_selection_and_history() {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Default::default());
        let b = app
            .doc
            .add_atom("N", reshiki::document::Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.selected = vec![a, b];
        let before = app.doc.clone();
        let revision = app.revision;
        for action in [
            Action::Section(Section::Atoms, true),
            Action::Section(Section::Arrange, true),
            Action::Figure(FigureFormat::Png),
            Action::Chemical(ChemicalFormat::Cdxml),
        ] {
            let _ = app.update(Message::InspectorAction(action));
        }
        assert_eq!(app.doc, before);
        assert_eq!(app.selected, [a, b]);
        assert_eq!(app.revision, revision);
        assert!(!app.history.can_undo());
        assert_eq!(app.inspector_ui.figure, FigureFormat::Png);
        assert_eq!(app.inspector_ui.chemical, ChemicalFormat::Cdxml);
        let _ = app.properties_panel();
        let _ = app.export_panel();
    }
}
