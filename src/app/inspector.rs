//! Selection-focused properties and task-based export controls.
use super::workspace::{command, muted};
use super::{App, InspectorTab, Message};
use crate::canvas::Tool;
use iced::widget::{button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Border, Color, Element, Length};
use reshiki::{
    bonds::{BondPreset, DoublePosition},
    editing::{Arrange, Transform},
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Bonds,
    BondDirection,
    Atoms,
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
}
#[derive(Default)]
pub(super) struct State {
    expanded: HashMap<Section, bool>,
    figure: FigureFormat,
    chemical: ChemicalFormat,
}
impl State {
    pub(super) fn update(&mut self, action: Action) {
        match action {
            Action::Section(section, expanded) => {
                self.expanded.insert(section, expanded);
            }
            Action::Figure(format) => self.figure = format,
            Action::Chemical(format) => self.chemical = format,
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
        if let Tool::RingPreset(preset) = self.tool {
            body = body.push(card(container(column![
                text(preset.to_string()).size(14),
                crate::canvas::layered::canvas(crate::canvas::DrawingThumbnail(preset.document(self.bond_drawing.length, false))).width(Length::Fill).height(90),
                text("Click to place. Drag to rotate or choose an attachment side. Click an atom to share it, or a bond to fuse.").size(12),
                text("Alt/Option on an atom connects the ring with a new bond. Each placement is one Undo step.").size(11).color(muted()),
                text(if preset == reshiki::rings::Preset::Cyclopentadiene { "Hold Shift to move the double bonds." } else { "Chair projections do not assign stereochemistry. Cleanup may redraw them as regular hexagons." }).size(11).color(muted()),
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
        body = body.push(
            self.inspector_section(
                Section::Molecule,
                "Molecular properties",
                self.analysis
                    .as_ref()
                    .map(|a| a.formula.clone())
                    .unwrap_or_else(|| {
                        format!(
                            "{} atoms · {} bonds",
                            self.doc.atoms.len(),
                            self.doc.bonds.len()
                        )
                    }),
                self.selected.is_empty() && self.tool == Tool::Select && !self.doc.atoms.is_empty(),
                self.molecular_properties(),
            ),
        );
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

    fn molecular_properties(&self) -> Element<'_, Message> {
        let mut body = column![
            text(format!(
                "{} atoms · {} bonds",
                self.doc.atoms.len(),
                self.doc.bonds.len()
            ))
            .size(11)
            .color(muted())
        ]
        .spacing(8);
        if let Some(a) = &self.analysis {
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
        } else {
            body = body.push(
                text("Check the structure to calculate its formula and properties.")
                    .size(12)
                    .color(muted()),
            );
        }
        body.push(
            command("Check structure", Message::Analyze)
                .on_press_maybe((!self.busy).then_some(Message::Analyze)),
        )
        .into()
    }

    fn selection_panel(&self) -> Element<'_, Message> {
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
                    command("Toggle aromatic circle · A", Message::AromaticDisplay)
                        .on_press_maybe((!self.busy).then_some(Message::AromaticDisplay)),
                );
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
        let arrange = column![
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
                command("Center", Message::Arrange(Arrange::AlignVertical)).width(Length::Fill),
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
        body = body.push(self.inspector_section(
            Section::Arrange,
            "Arrange & transform",
            "",
            false,
            arrange,
        ));
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
            ".reshiki · retains the complete drawing",
            true,
            command("Save native document…", Message::SaveAs).width(Length::Fill),
        ))
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
