//! Selection-focused properties and task-based export controls.
use super::workspace::{command, hover_hint, keyed_command, muted_text};
use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use iced::widget::{button, checkbox, column, container, row, text, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Subscription, Task};
use reshiki::{
    bonds::{BondPreset, DoublePosition},
    document::Document,
    engine::{Analysis, ChemistryEngine, Request},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Bonds,
    BondDirection,
    Atoms,
    Transform,
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
    #[cfg(windows)]
    Emf,
}
impl FigureFormat {
    /// Menu order: vector formats, then raster.
    const ALL: &'static [Self] = &[
        Self::Svg,
        Self::Pdf,
        #[cfg(windows)]
        Self::Emf,
        Self::Png,
    ];
    fn kind(self) -> &'static str {
        match self {
            Self::Png => "Raster",
            _ => "Vector",
        }
    }
    fn code(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Svg => "svg",
            Self::Png => "png",
            #[cfg(windows)]
            Self::Emf => "emf",
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::Pdf => "Vector figure at its physical publication size.",
            Self::Svg => "Editable vector artwork for layout and illustration.",
            Self::Png => {
                "Up to 1200 dpi. Large drawings use a lower resolution; physical size is preserved."
            }
            #[cfg(windows)]
            Self::Emf => {
                "Vector picture for Microsoft Office at its physical size. Text becomes outlines."
            }
        }
    }
}
impl std::fmt::Display for FigureFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pdf => "PDF",
            Self::Svg => "SVG",
            Self::Png => "PNG",
            #[cfg(windows)]
            Self::Emf => "EMF for Office",
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
    /// Open or close the figure format menu.
    FigureMenu(bool),
    Chemical(ChemicalFormat),
    ChemicalMenu(bool),
    RefreshProperties,
    Centroid,
    Attachment(reshiki::attachments::Kind),
    DepthBonds,
    RingArc,
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
    figure_menu: bool,
    chemical: ChemicalFormat,
    chemical_menu: bool,
    pending: Option<PropertyKey>,
    properties: Option<(PropertyKey, Result<Analysis, String>)>,
}
impl State {
    pub(super) fn menu_open(&self) -> bool {
        self.figure_menu || self.chemical_menu
    }
    pub(super) fn close_menu(&mut self) {
        self.figure_menu = false;
        self.chemical_menu = false;
    }
    #[cfg(test)]
    pub(super) fn expanded(&self, section: Section) -> Option<bool> {
        self.expanded.get(&section).copied()
    }
    pub(super) fn update(&mut self, action: Action) {
        match action {
            Action::Section(section, expanded) => {
                self.expanded.insert(section, expanded);
            }
            Action::Figure(format) => {
                self.figure = format;
                self.figure_menu = false;
            }
            Action::FigureMenu(open) => {
                self.figure_menu = open;
                self.chemical_menu = false;
            }
            Action::Chemical(format) => {
                self.chemical = format;
                self.chemical_menu = false;
            }
            Action::ChemicalMenu(open) => {
                self.chemical_menu = open;
                self.figure_menu = false;
            }
            Action::RefreshProperties
            | Action::PropertiesCalculated(..)
            | Action::Centroid
            | Action::Attachment(_)
            | Action::RingArc
            | Action::DepthBonds => {}
        }
    }
}
fn card<'a>(body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(body)
        .width(Length::Fill)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::WHITE.into()),
                    border: Border {
                        color: Color::from_rgb8(218, 225, 224),
                        width: 1.,
                        radius: 8.into(),
                    },
                    ..Default::default()
                },
            )
        })
        .into()
}
impl App {
    fn property_key(&self) -> Option<PropertyKey> {
        if self.tab.selected.is_empty() {
            return None;
        }
        let ids: HashSet<_> = self
            .tab
            .doc
            .expand_abbreviation_selection(&self.tab.selected)
            .into_iter()
            .collect();
        Some(PropertyKey {
            revision: self.tab.revision,
            epoch: self.tab.file_epoch,
            atoms: self
                .tab
                .doc
                .atoms
                .iter()
                .filter(|a| ids.contains(&a.id))
                .map(|a| a.id)
                .collect(),
        })
    }

    fn property_document(&self, key: &PropertyKey) -> Document {
        let mut part = reshiki::editing::selection(&self.tab.doc, &key.atoms);
        reshiki::atom_labels::clear_computed(&mut part);
        part
    }

    fn property_request_key(&self) -> PropertyKey {
        self.property_key().unwrap_or_else(|| PropertyKey {
            revision: self.tab.revision,
            epoch: self.tab.file_epoch,
            atoms: self.tab.doc.atoms.iter().map(|a| a.id).collect(),
        })
    }

    pub(super) fn property_analysis(&self) -> Option<&Analysis> {
        if self.tab.selected.is_empty() && self.tab.analysis.is_some() {
            return self.tab.analysis.as_ref();
        }
        let key = self.property_request_key();
        self.tab
            .inspector_ui
            .properties
            .as_ref()
            .filter(|(saved, _)| *saved == key)
            .and_then(|(_, result)| result.as_ref().ok())
    }

    pub(super) fn properties_subscription(&self) -> Subscription<Message> {
        if reshiki::attachments::present(&self.tab.doc)
            || !self.inspector_open
            || self.inspector_tab != InspectorTab::Properties
            || self.tab.busy
            || self.tab.erase_stroke
            || self.tab.cleanup.is_some()
            || self.tab.optimization.is_some()
            || self.tab.inspector_ui.pending.is_some()
            || (self.tab.selected.is_empty() && self.tab.analysis.is_some())
        {
            return Subscription::none();
        }
        let key = self.property_request_key();
        if key.atoms.is_empty()
            || self
                .tab
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
            Action::RingArc => {
                let before = self.tab.doc.clone();
                match reshiki::ring_arcs::toggle(&mut self.tab.doc, &self.tab.selected) {
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
                let before = self.tab.doc.clone();
                match reshiki::projection::add_centroid(&mut self.tab.doc, &self.tab.selected) {
                    Ok(id) => {
                        self.tab.selected = vec![id];
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
                let before = self.tab.doc.clone();
                match reshiki::attachments::add(&mut self.tab.doc, &self.tab.selected, kind) {
                    Ok(id) => {
                        self.tab.selected = vec![id];
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
                let before = self.tab.doc.clone();
                reshiki::projection::depth_bonds(&mut self.tab.doc, &self.tab.selected);
                self.changed(before);
                Task::none()
            }
            Action::RefreshProperties => {
                let key = self.property_request_key();
                if key.atoms.is_empty() || self.tab.inspector_ui.pending.is_some() {
                    return Task::none();
                }
                let request = Request::molecule("analyze", self.property_document(&key));
                self.tab.inspector_ui.pending = Some(key.clone());
                self.tab.inspector_ui.properties = None;
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
                if self.tab.inspector_ui.pending.as_ref() == Some(&key) {
                    self.tab.inspector_ui.pending = None;
                    if self.property_request_key() == key {
                        // Never apply fragment labels or hydrogen counts to the original drawing.
                        self.tab.inspector_ui.properties = Some((key, *result));
                    }
                }
                Task::none()
            }
            other => {
                self.tab.inspector_ui.update(other);
                Task::none()
            }
        }
    }

    fn property_summary(&self) -> String {
        let scope = if self.tab.selected.is_empty() {
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
        self.inspector_section_lazy(section, title, summary, expanded, || content)
    }

    fn inspector_section_lazy<'a, Content: Into<Element<'a, Message>>>(
        &'a self,
        section: Section,
        title: &'static str,
        summary: impl Into<String>,
        expanded: bool,
        content: impl FnOnce() -> Content,
    ) -> Element<'a, Message> {
        let expanded = self
            .tab
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
                    .style(muted_text)
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(3);
        if !summary.is_empty() {
            heading = heading.push(text(summary).size(11).style(muted_text));
        }
        let mut body = column![
            reshiki::accessibility::button(
                format!("inspector-section-{section:?}"),
                format!(
                    "{title}: {}",
                    if expanded { "expanded" } else { "collapsed" }
                ),
                heading
            )
            .expanded(expanded)
            .padding(10)
            .width(Length::Fill)
            .style(button::text)
            .on_press(Message::InspectorAction(Action::Section(
                section, !expanded
            )))
        ];
        if expanded {
            body = body.push(container(content()).padding(iced::Padding {
                top: 2.,
                right: 10.,
                bottom: 12.,
                left: 10.,
            }));
        }
        card(body)
    }

    pub(super) fn properties_panel(&self) -> Element<'_, Message> {
        if self.tab.optimization.is_some() {
            return column![
                card(container(self.optimization_panel()).padding(12)),
                text("Apply or cancel the preview to edit other properties.")
                    .size(11)
                    .style(muted_text),
            ]
            .spacing(10)
            .into();
        }
        self.ordinary_properties_panel()
    }

    fn ordinary_properties_panel(&self) -> Element<'_, Message> {
        let selected: HashSet<_> = self.tab.selected.iter().copied().collect();
        let mut body = column![
            text(if self.tab.selected.is_empty() {
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
            .style(muted_text)
        ]
        .spacing(10);
        let color_issues = reshiki::canvas_theme::label_contrast_issues(&self.tab.doc);
        if !color_issues.is_empty() {
            body = body.push(text(format!(
                "{} atom label(s) have low contrast against the canvas or a ring fill. Adjust the label or fill color.", color_issues.len()
            )).size(11).style(muted_text));
        }
        let molecular_first = self.tab.selected.is_empty()
            || self.property_key().is_some_and(|key| !key.atoms.is_empty());
        if molecular_first {
            body = body.push(self.molecular_section());
        }
        if let Tool::RingPreset(preset) = self.tool {
            use reshiki::rings::Preset;
            let details = format!(
                "{} Alt/Option on an atom connects the ring with a new bond. Each placement is one Undo step. {}",
                if preset == Preset::Benzene {
                    "Click an aromatic carbon to attach a phenyl group, or a bond to fuse. Terminal carbons become part of the ring. Drag to choose the direction."
                } else {
                    "Click to place. Drag to rotate or choose an attachment side. Click an atom to share it, or a bond to fuse."
                },
                match preset {
                    Preset::Benzene | Preset::Cyclopentadiene =>
                        "Hold Shift to move the double bonds.",
                    Preset::HaworthFive | Preset::HaworthSix =>
                        "Haworth outlines have a bold front edge. Templates → Carbohydrates contains oxygen scaffolds and defined α/β sugars. These blank outlines do not assign stereochemistry.",
                    _ =>
                        "Chair projections do not assign stereochemistry. Cleanup may redraw them as regular hexagons.",
                }
            );
            body = body.push(card(
                container(
                    column![
                        text(preset.to_string()).size(14),
                        hover_hint(
                            crate::canvas::layered::canvas(crate::canvas::DrawingThumbnail(
                                preset.document(self.tab.bond_drawing.length, false)
                            ))
                            .width(Length::Fill)
                            .height(90),
                            details,
                            tooltip::Position::Left,
                        ),
                        text("Click to place · Click a bond to fuse.")
                            .size(11)
                            .style(muted_text),
                    ]
                    .spacing(8),
                )
                .padding(12),
            ));
        }
        if matches!(self.tool, Tool::Graphic(_))
            || self
                .tab
                .doc
                .graphics
                .iter()
                .any(|g| selected.contains(&g.id) && g.picture.is_none())
        {
            body = body.push(card(container(self.graphic_panel()).padding(12)));
        }
        if self
            .tab
            .doc
            .graphics
            .iter()
            .any(|g| selected.contains(&g.id) && g.picture.is_some())
        {
            body = body.push(card(container(self.picture_panel()).padding(12)));
        }
        if self.tool == Tool::Arrow || self.tab.doc.arrows.iter().any(|a| selected.contains(&a.id))
        {
            body = body.push(card(container(self.arrow_panel()).padding(12)));
        }
        if self.tool == Tool::Text
            || (self.tab.selected.len() == 1
                && self
                    .tab
                    .caption_target
                    .is_some_and(|id| self.tab.selected.contains(&id)))
        {
            body = body.push(card(container(self.text_panel()).padding(12)));
        }
        if !self.tab.selected.is_empty() {
            body = body.push(self.selection_panel(&selected));
        }
        if let Some(error) = &self.tab.chemistry_notice {
            body = body.push(
                text(error)
                    .size(12)
                    .style(crate::appearance::text_color(Color::from_rgb8(182, 66, 61))),
            );
        }
        if !molecular_first {
            body = body.push(self.molecular_section());
        }
        body = body.push(self.inspector_section_lazy(
            Section::Chemistry,
            "Labels & chemistry",
            "",
            false,
            || {
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
                if self.tab.doc.atoms.iter().any(|a| selected.contains(&a.id)) {
                    chemistry.push(self.attachment_points())
                } else {
                    chemistry
                }
            },
        ));
        body.push(
            self.inspector_section(
                Section::DrawingStyle,
                "Drawing style",
                &self.tab.doc.drawing_style.name,
                false,
                column![
                    text(format!(
                        "{} {} pt\nBonds {} pt · Lines {} pt\nPNG up to 1200 dpi",
                        self.tab.doc.drawing_style.font_family,
                        self.tab.doc.drawing_style.font_size_pt,
                        self.tab.doc.drawing_style.bond_length_pt,
                        self.tab.doc.drawing_style.line_width_pt
                    ))
                    .size(12)
                    .style(muted_text),
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
        self.inspector_section_lazy(
            Section::Molecule,
            "Molecular properties",
            self.property_summary(),
            self.opened_section() == Some(Section::Molecule),
            || self.molecular_properties(),
        )
    }

    fn molecular_properties(&self) -> Element<'_, Message> {
        if reshiki::attachments::present(&self.tab.doc) {
            let selected = (!self.tab.selected.is_empty())
                .then(|| reshiki::editing::selection(&self.tab.doc, &self.tab.selected));
            let doc = selected.as_ref().unwrap_or(&self.tab.doc);
            let mut body = column![
                text(reshiki::attachments::ANALYSIS_NOTICE)
                    .size(11)
                    .style(muted_text)
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
                        .style(muted_text),
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
                self.tab
                    .doc
                    .bonds
                    .iter()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                    .count(),
            )
        } else {
            (self.tab.doc.atoms.len(), self.tab.doc.bonds.len())
        };
        let mut body = column![
            text(format!("{} atoms · {} bonds", atoms, bonds))
                .size(11)
                .style(muted_text)
        ]
        .spacing(8);
        if key.is_some()
            && self
                .tab
                .doc
                .bonds
                .iter()
                .any(|b| ids.contains(&b.a) != ids.contains(&b.b))
        {
            body = body.push(
                text("Selected fragment: implicit hydrogens are recalculated at cut bonds.")
                    .size(11)
                    .style(muted_text),
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
                        text(label).size(11).style(muted_text).width(Length::Fill),
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
            return body.push(text(if key.is_some() { "Select atoms or bonds to calculate their properties. Clear the selection to use the whole drawing." } else { "Draw or import a molecule to calculate its properties." }).size(12).style(muted_text)).into();
        } else if let Some((_, Err(error))) = self
            .tab
            .inspector_ui
            .properties
            .as_ref()
            .filter(|(saved, _)| *saved == self.property_request_key())
        {
            body = body.push(
                text(error)
                    .size(12)
                    .style(crate::appearance::text_color(Color::from_rgb8(182, 66, 61))),
            );
        } else {
            body = body.push(
                text(if key.is_some() {
                    "Calculating selection…"
                } else {
                    "Calculating molecular properties…"
                })
                .size(12)
                .style(muted_text),
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
                (!self.tab.busy && self.tab.inspector_ui.pending.is_none())
                    .then_some(Message::InspectorAction(Action::RefreshProperties)),
            ),
        )
        .into()
    }

    pub(super) fn alignment_count(&self) -> usize {
        if self.tab.selected.len() <= 1 {
            return self.tab.selected.len();
        }
        reshiki::editing::groups(&self.tab.doc, &self.tab.selected).len()
    }

    fn has_selected_ring(&self) -> bool {
        if self.tab.selected.len() < 3 {
            return false;
        }
        // Ring detection does linear atom lookups for each selected ID. Reject
        // large chemical selections before doing that work, while retaining
        // selections that include extra nonchemical objects or dummy points.
        let selected: HashSet<_> = self.tab.selected.iter().copied().collect();
        let ring_atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| selected.contains(&a.id) && a.centroid.is_empty() && a.element != "*")
            .take(9)
            .count();
        (3..=8).contains(&ring_atoms)
            && reshiki::rings::selected_cycle(&self.tab.doc, &self.tab.selected).is_some()
    }

    /// The one section a selection opens. Sections the user opened or
    /// closed keep that state.
    fn opened_section(&self) -> Option<Section> {
        if self.tab.selected.is_empty() {
            return (self.tool == Tool::Select && !self.tab.doc.atoms.is_empty())
                .then_some(Section::Molecule);
        }
        let selected: HashSet<_> = self.tab.selected.iter().copied().collect();
        if self
            .tab
            .doc
            .bonds
            .iter()
            .any(|b| selected.contains(&b.a) && selected.contains(&b.b))
        {
            return Some(Section::Bonds);
        }
        let atoms: Vec<_> = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| selected.contains(&a.id))
            .collect();
        (atoms.len() == 1
            || atoms
                .iter()
                .any(|a| !a.marks.is_empty() || a.radical_electrons != 0))
        .then_some(Section::Atoms)
    }

    /// Centroids, dummy atoms and semantic attachment points for the
    /// selected atoms; the rules match the right-click menu.
    fn attachment_points(&self) -> Element<'_, Message> {
        let atoms = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| {
                self.tab.selected.contains(&a.id) && a.element != "*" && a.centroid.is_empty()
            })
            .count();
        let centroid = (2..=300).contains(&atoms);
        let attachment = centroid && atoms == self.tab.selected.len();
        let item = |label, message: Message, enabled: bool, hint: &'static str, reason| {
            hover_hint(
                command(label, message.clone())
                    .on_press_maybe(enabled.then_some(message))
                    .width(Length::Fill),
                if enabled { hint } else { reason },
                tooltip::Position::Top,
            )
        };
        let kind = |kind| Message::InspectorAction(Action::Attachment(kind));
        column![
            text("Attachment points").size(11).style(muted_text),
            row![
                item(
                    "Add centroid",
                    Message::InspectorAction(Action::Centroid),
                    centroid,
                    "A point at the center of the selected atoms · Draw a dashed contact from it",
                    "Select at least two atoms",
                ),
                hover_hint(
                    command("Dummy atom (*)", Message::Element("*".into())).width(Length::Fill),
                    "Draw * wildcard atoms with the Atom tool",
                    tooltip::Position::Top,
                ),
            ]
            .spacing(4),
            row![
                item(
                    "Multi-center",
                    kind(reshiki::attachments::Kind::MultiCenter),
                    attachment,
                    "Attaches to all selected atoms · Then draw a bond from * to the metal or substituent",
                    "Select only the target atoms (2–300)",
                ),
                item(
                    "Variable",
                    kind(reshiki::attachments::Kind::Variable),
                    attachment,
                    "Attaches to one of the selected positions · Then draw a bond from * to the substituent",
                    "Select only the target atoms (2–300)",
                ),
            ]
            .spacing(4),
        ]
        .spacing(3)
        .padding(iced::Padding::ZERO.top(6))
        .into()
    }

    fn transform_section(&self) -> Element<'_, Message> {
        self.inspector_section_lazy(Section::Transform, "Transform", "", false, || {
            self.numeric_transform_panel()
        })
    }

    fn selection_panel(&self, selected: &HashSet<u64>) -> Element<'_, Message> {
        let opened = self.opened_section();
        let atoms: Vec<_> = self
            .tab
            .doc
            .atoms
            .iter()
            .filter(|a| selected.contains(&a.id))
            .collect();
        let bonds: Vec<_> = self
            .tab
            .doc
            .bonds
            .iter()
            .filter(|b| selected.contains(&b.a) && selected.contains(&b.b))
            .collect();
        let mut body = column![].spacing(10);
        if self.atom_text_target().is_some() {
            body = body.push(
                keyed_command(
                    "Edit atom label…",
                    Message::AtomText(super::atom_text::Action::Begin(None)),
                )
                .width(Length::Fill),
            );
        }
        if let Some(first) = bonds.first() {
            let preset = BondPreset::of(first)
                .filter(|p| bonds.iter().all(|b| BondPreset::of(b) == Some(*p)));
            let mut controls = column![
                text("Style").size(11).style(muted_text),
                crate::appearance::pick_list(BondPreset::ALL, preset, Message::ApplyBondPreset)
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
                    .push(text("Second line placement").size(11).style(muted_text))
                    .push(
                        crate::appearance::pick_list(
                            DoublePosition::ALL,
                            position,
                            Message::BondPosition,
                        )
                        .placeholder("Mixed positions")
                        .text_size(12)
                        .padding(7)
                        .width(Length::Fill),
                    );
            }
            controls = controls
                .push(text("Color").size(11).style(muted_text))
                .push(
                    row![
                        crate::appearance::text_input("#000000", &self.tab.bond_color_input)
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
                    row![
                        hover_hint(
                            command("Aromatic circle", Message::AromaticDisplay)
                                .on_press_maybe((!self.tab.busy).then_some(Message::AromaticDisplay))
                                .width(Length::Fill),
                            "Toggle the aromatic circle",
                            tooltip::Position::Top,
                        ),
                        hover_hint(
                            command("Inner ring curve", Message::InspectorAction(Action::RingArc))
                                .width(Length::Fill),
                            "Toggle the inner ring curve · Select consecutive ring atoms for a partial curve, or the whole ring for a circle. Bond orders stay unchanged.",
                            tooltip::Position::Top,
                        ),
                    ]
                    .spacing(6),
                );
            }
            if self.has_selected_ring() {
                controls = controls.push(
                    keyed_command("Saturated ↔ Aromatic", Message::ToggleSelectedRing)
                        .width(Length::Fill),
                );
            }
            body = body.push(self.inspector_section(
                Section::Bonds,
                "Bond appearance",
                "",
                opened == Some(Section::Bonds),
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
                    crate::appearance::text_input("Isotope mass", &self.tab.isotope)
                        .on_input(Message::Isotope)
                        .on_submit(Message::ApplyIsotope)
                        .size(12)
                        .padding(7),
                    command("Set", Message::ApplyIsotope)
                ]
                .spacing(6),
                row![
                    text("Unpaired electrons").size(11).width(Length::Fill),
                    crate::appearance::pick_list([0u8, 1, 2], count, Message::AtomRadical)
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
            body = body.push(self.inspector_section(
                Section::Atoms,
                "Atom details",
                "Charge, isotope & electron marks",
                opened == Some(Section::Atoms),
                controls,
            ));
        }
        body = body.push(self.transform_section());
        body = body.push(self.inspector_section_lazy(
            Section::Groups,
            "Grouping & frames",
            "",
            false,
            || {
                let groups = self.tab.doc.outer_selected_groups(&self.tab.selected);
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
                    crate::appearance::pick_list(
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
                        .tab
                        .doc
                        .groups
                        .iter()
                        .filter(|g| groups.contains(&g.id))
                        .all(|g| g.integral);
                    grouping = grouping.push(hover_hint(
                        checkbox(integral)
                            .label("Integral group")
                            .size(14)
                            .text_size(12)
                            .on_toggle(Message::IntegralGroup),
                        "Integral groups stay whole with Option/Alt-click. Ungroup releases them.",
                        tooltip::Position::Top,
                    ));
                }
                grouping
            },
        ));
        body.push(
            button(
                text("Delete selection")
                    .size(12)
                    .style(crate::appearance::text_color(Color::from_rgb8(174, 57, 52))),
            )
            .style(button::text)
            .padding(7)
            .on_press(Message::Delete),
        )
        .into()
    }

    /// The figure format, chosen from a menu grouped into vector and raster.
    fn figure_menu(&self, figure: FigureFormat) -> Element<'_, Message> {
        let open = self.tab.inspector_ui.figure_menu;
        let anchor = reshiki::accessibility::button(
            "export-figure-format",
            format!("Figure format: {figure}"),
            row![
                text(format!("{} · {figure}", figure.kind()))
                    .size(12)
                    .width(Length::Fill),
                super::workspace::caret(9.)
            ]
            .align_y(Alignment::Center),
        )
        .padding(8)
        .width(Length::Fill)
        .style(crate::appearance::secondary)
        .expanded(open)
        .on_press(Message::InspectorAction(Action::FigureMenu(!open)));
        let popup = open.then(|| {
            let mut items = column![].spacing(1);
            for kind in ["Vector", "Raster"] {
                items =
                    items.push(container(text(kind).size(11).style(muted_text)).padding([5, 10]));
                for &format in FigureFormat::ALL.iter().filter(|f| f.kind() == kind) {
                    items = items.push(
                        reshiki::accessibility::button(
                            format!("figure-format-{}", format.code()),
                            format.to_string(),
                            text(format.to_string()).size(12),
                        )
                        .checked(format == figure)
                        .width(Length::Fill)
                        .padding([6, 10])
                        .style(super::workspace::control(format == figure))
                        .on_press(Message::InspectorAction(Action::Figure(format))),
                    );
                }
            }
            container(items)
                .width(Length::Fill)
                .padding(5)
                .style(super::color_popover::surface)
                .into()
        });
        Element::new(
            super::popover::popover(
                anchor,
                popup,
                Message::InspectorAction(Action::FigureMenu(false)),
            )
            .fit_anchor(),
        )
    }

    fn chemical_menu(&self, chemical: ChemicalFormat) -> Element<'_, Message> {
        let open = self.tab.inspector_ui.chemical_menu;
        let anchor = super::popover::choice_anchor(
            "export-chemical-format",
            format!("Structure format: {chemical}"),
            chemical.to_string(),
            12.,
            8,
            open,
            Message::InspectorAction(Action::ChemicalMenu(!open)),
        );
        let popup = open.then(|| {
            let items = ChemicalFormat::ALL.into_iter().map(|format| {
                reshiki::accessibility::button(
                    format!("chemical-format-{}", format.code()),
                    format.to_string(),
                    text(format.to_string()).size(12),
                )
                .checked(format == chemical)
                .width(Length::Fill)
                .padding([6, 10])
                .style(super::workspace::control(format == chemical))
                .on_press(Message::InspectorAction(Action::Chemical(format)))
                .into()
            });
            container(column(items))
                .width(Length::Fill)
                .padding(5)
                .style(super::color_popover::surface)
                .into()
        });
        Element::new(
            super::popover::popover(
                anchor,
                popup,
                Message::InspectorAction(Action::ChemicalMenu(false)),
            )
            .fit_anchor(),
        )
    }

    pub(super) fn export_panel(&self) -> Element<'_, Message> {
        let figure = self.tab.inspector_ui.figure;
        let chemical = self.tab.inspector_ui.chemical;
        let mut figures = column![
            self.figure_menu(figure),
            text(figure.description()).size(12).style(muted_text),
            reshiki::accessibility::button(
                "export-figure",
                format!("Export {} figure", figure.code().to_uppercase()),
                text(if self.figure_exporting {
                    "Exporting…".into()
                } else {
                    format!("Export {}…", figure.code().to_uppercase())
                })
                .size(13)
            )
            .padding(10)
            .width(Length::Fill)
            .on_press_maybe(
                (!self.tab.busy && !self.figure_exporting)
                    .then_some(Message::Export(figure.code()))
            ),
        ]
        .spacing(9);
        if reshiki::clipboard::available() {
            figures = figures
                .push(
                    reshiki::accessibility::button(
                        "export-copy-image",
                        "Copy image",
                        text(super::workspace::keyed("Copy image", &Message::CopyImage)).size(12),
                    )
                    .padding([7, 9])
                    .style(super::workspace::control(false))
                    .on_press_maybe((!self.tab.clipboard_busy).then_some(Message::CopyImage))
                    .width(Length::Fill),
                )
                .push(
                    text(if self.tab.selected.is_empty() {
                        "Copy image uses the full drawing."
                    } else {
                        "Copy image uses the selected objects."
                    })
                    .size(11)
                    .style(muted_text),
                );
        }
        let mut body = column![
            text("Export").size(18),
            text("File exports use the full drawing.")
                .size(12)
                .style(muted_text),
            self.inspector_section(
                Section::ExportFigure,
                "Figure",
                &self.tab.doc.drawing_style.name,
                true,
                figures
            ),
            self.inspector_section(
                Section::ExportChemical,
                "Structure & exchange",
                "MOL · SMILES · InChI · CDXML",
                false,
                column![
                    self.chemical_menu(chemical),
                    text(chemical.description()).size(12).style(muted_text),
                    reshiki::accessibility::button(
                        "export-chemical",
                        format!("Export {} structure", chemical.code().to_uppercase()),
                        text(format!("Export {}…", chemical.code().to_uppercase())).size(13)
                    )
                    .padding(10)
                    .width(Length::Fill)
                    .on_press_maybe((!self.tab.busy).then_some(Message::Export(chemical.code()))),
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
        if self.tab.doc.page_layout.is_some() {
            pages = pages.push(
                command(
                    "PDF · all pages",
                    Message::Pages(super::pages::Action::Export),
                )
                .on_press_maybe(
                    (!self.figure_exporting)
                        .then_some(Message::Pages(super::pages::Action::Export)),
                ),
            );
        }
        if reshiki::printing::available() {
            pages = pages.push(
                keyed_command(
                    "Print…",
                    Message::Printing(super::printing::Action::Start(
                        reshiki::printing::Scope::Document,
                    )),
                )
                .on_press_maybe(self.printing.active.is_none().then_some(Message::Printing(
                    super::printing::Action::Start(reshiki::printing::Scope::Document),
                )))
                .width(Length::Fill),
            );
            if !self.tab.selected.is_empty() {
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
            self.tab.doc.page_layout.is_some(),
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
    pub(super) fn toggle_inspector(&mut self) -> Task<Message> {
        self.inspector_open = !self.inspector_open;
        if self.inspector_open && self.inspector_tab == InspectorTab::Assistant {
            return self.assistant_action(super::assistant::Action::Open);
        }
        Task::none()
    }
    /// Returns the Import tab's focus task. `None` lets update_inner's
    /// inspector reveal apply.
    pub(super) fn show_inspector_tab(&mut self, tab: InspectorTab) -> Option<Task<Message>> {
        if tab != InspectorTab::ThemeGenerator {
            self.theme_library.editor = None;
        }
        if tab != InspectorTab::DrawingStyle {
            self.tab.styles.editor = None;
        }
        if tab == InspectorTab::Labels {
            let atoms: Vec<_> = self
                .tab
                .doc
                .atoms
                .iter()
                .filter(|a| self.tab.selected.contains(&a.id))
                .collect();
            self.tab.labels.scope = if atoms.is_empty() {
                super::atom_labels::Scope::Drawing
            } else {
                super::atom_labels::Scope::Selection
            };
            self.tab.labels.number = if let [atom] = atoms.as_slice() {
                atom.display
                    .number
                    .as_ref()
                    .map(|n| n.text.clone())
                    .unwrap_or_default()
            } else {
                String::new()
            };
        }
        self.inspector_tab = tab;
        self.inspector_open = true;
        if tab == InspectorTab::Import {
            self.help_open = false;
            return Some(iced::widget::operation::focus(super::import::INPUT));
        }
        None
    }
}

#[cfg(test)]
mod tests;
