//! Properties follows the selection; an idle canvas shows drawing settings,
//! while a drawing tool shows the same defaults as its context row.
use super::*;
use iced::widget::column;

/// A request follows its object/atom scope, not the document revision. Caption
/// pins and appearance edits keep that scope; changing targets does not.
pub(super) struct MolecularTarget {
    epoch: u64,
    selection: HashSet<u64>,
    atoms: HashSet<u64>,
}

impl App {
    fn molecular_properties_target(&self) -> MolecularTarget {
        MolecularTarget {
            epoch: self.tab.file_epoch,
            selection: self.tab.selected.iter().copied().collect(),
            atoms: self.property_request_key().atoms.into_iter().collect(),
        }
    }

    pub(in crate::app) fn request_molecular_properties(&mut self) {
        self.tab.inspector_ui.requested_molecule = Some(self.molecular_properties_target());
        self.tab
            .inspector_ui
            .expanded
            .insert(Section::Molecule, true);
        self.tab
            .inspector_ui
            .expanded
            .insert(Section::RequestedMolecule, true);
    }

    pub(in crate::app) fn dismiss_molecular_properties(&mut self) -> Task<Message> {
        self.tab.inspector_ui.clear_requested_molecule();
        super::super::focus_drawing()
    }

    /// Some selection entry points assign IDs directly. Check their final
    /// scope after every message as well as before rendering a requested card.
    pub(in crate::app) fn sync_requested_molecular_properties(&mut self) {
        if self.tab.inspector_ui.requested_molecule.is_some()
            && !self.requested_molecular_properties()
        {
            self.tab.inspector_ui.clear_requested_molecule();
        }
    }

    pub(super) fn properties_selection_summary(&self) -> String {
        let summary = self.selection_summary();
        if let [id] = self.tab.selected.as_slice()
            && let Some(atom) = self.tab.doc.atom(*id)
        {
            format!("{summary} · {}", atom.element)
        } else {
            summary
        }
    }

    pub(super) fn properties_molecular_selection(
        &self,
        selected: &HashSet<u64>,
        has_model: bool,
    ) -> bool {
        self.tab
            .doc
            .atoms
            .iter()
            .any(|atom| selected.contains(&atom.id))
            && (reshiki::reactions::whole_molecules(
                &self.tab.doc,
                &self
                    .tab
                    .doc
                    .expand_abbreviation_selection(&self.tab.selected)
                    .into_iter()
                    .collect(),
            ) || has_model
                || self.requested_molecular_properties())
    }

    pub(super) fn properties_generic_selection(&self) -> bool {
        reshiki::generic::intersects(&self.tab.doc, &self.tab.selected)
    }

    pub(in crate::app) fn requested_molecular_properties(&self) -> bool {
        self.tab
            .inspector_ui
            .requested_molecule
            .as_ref()
            .is_some_and(|target| {
                self.tool == Tool::Select
                    && target.epoch == self.tab.file_epoch
                    && target.selection.len() == self.tab.selected.len()
                    && self
                        .tab
                        .selected
                        .iter()
                        .all(|id| target.selection.contains(id))
                    && if self.tab.selected.is_empty() {
                        target.atoms.len() == self.tab.doc.atoms.len()
                            && self
                                .tab
                                .doc
                                .atoms
                                .iter()
                                .all(|atom| target.atoms.contains(&atom.id))
                    } else {
                        let atoms = self.property_request_key().atoms;
                        target.atoms.len() == atoms.len()
                            && atoms.iter().all(|id| target.atoms.contains(id))
                    }
            })
    }

    pub(super) fn properties_contrast_notice(
        &self,
        selected: Option<&HashSet<u64>>,
    ) -> Option<Element<'_, Message>> {
        let count = reshiki::canvas_theme::label_contrast_issues(&self.tab.doc)
            .into_iter()
            .filter(|id| selected.is_none_or(|selected| selected.contains(id)))
            .count();
        (count > 0).then(|| {
            hover_hint(
                text(if count == 1 {
                    "1 label has low contrast".to_owned()
                } else {
                    format!("{count} labels have low contrast")
                })
                .size(11)
                .style(muted_text),
                "Against the canvas or a ring fill · Change the label or fill color",
                tooltip::Position::Bottom,
            )
            .into()
        })
    }

    pub(super) fn unselected_properties_panel(&self) -> Element<'_, Message> {
        if !self.tool.selects() {
            return self.next_object_properties_panel();
        }
        let mut body = column![
            text("Nothing selected").size(14),
            super::super::workspace::section("Drawing"),
            self.inspector_section_lazy(
                Section::Chemistry,
                "Labels & chemistry",
                "",
                false,
                || {
                    column![
                        panel_action("Normalize depiction", Message::NormalizeDepiction),
                        panel_action(
                            "Reaction roles…",
                            Message::Reaction(super::super::reactions::Action::Open),
                        ),
                        panel_action(
                            "Atom labels & numbering…",
                            Message::Inspector(InspectorTab::Labels),
                        ),
                        panel_action(
                            "Chemical abbreviations…",
                            Message::Inspector(InspectorTab::Abbreviations),
                        ),
                    ]
                    .spacing(3)
                },
            ),
            self.inspector_section(
                Section::DrawingStyle,
                "Drawing style",
                &self.tab.doc.drawing_style.name,
                false,
                column![
                    text(format!(
                        "{} {} pt\nBonds {} pt · Lines {} pt\nPNG files at {} dpi",
                        self.tab.doc.drawing_style.font_family,
                        self.tab.doc.drawing_style.font_size_pt,
                        self.tab.doc.drawing_style.bond_length_pt,
                        self.tab.doc.drawing_style.line_width_pt,
                        self.appearance.export_dpi.get(),
                    ))
                    .size(12)
                    .style(muted_text),
                    panel_action(
                        "Edit drawing style…",
                        Message::DrawingStyle(super::super::document_styles::Action::Open),
                    ),
                ]
                .spacing(8),
            ),
        ]
        .spacing(10);
        if self.requested_molecular_properties() {
            body = body.push(self.molecular_section(Some(Section::RequestedMolecule)));
        }
        if let Some(section) = self.generic_structure_section() {
            body = body.push(section);
        }
        if let Some(error) = self
            .tab
            .chemistry_notice
            .as_deref()
            .filter(|notice| *notice != reshiki::atom_labels::refresh::GENERIC_NOTICE)
        {
            body = body.push(text(error).size(12).style(muted_text));
        }
        if let Some(notice) = self.properties_contrast_notice(None) {
            body = body.push(notice);
        }
        if !self.tab.stereo_notes.is_empty() {
            body = body.push(card(
                container(self.stereo_note_lines(
                    column![super::super::workspace::section("Stereochemistry")].spacing(5),
                    "stereo",
                ))
                .padding(12),
            ));
        }
        body.into()
    }

    fn next_object_properties_panel(&self) -> Element<'_, Message> {
        let heading = match self.tool {
            tool if tool.bond_preset().is_some() => "Next bond",
            Tool::Chain(_) => "Next chain",
            Tool::Atom => "Next atom",
            Tool::Ring | Tool::RingPreset(_) => "Next ring",
            Tool::Template => "Next template",
            Tool::Arrow => "Next arrow",
            Tool::Text => "Next text",
            Tool::Graphic(_) => "Next graphic",
            Tool::Tilt => "Tilt",
            Tool::StretchBond { .. } => "Stretch bond",
            Tool::EditPoints => "Edit points",
            Tool::Erase => "Erase",
            _ => "Tool settings",
        };
        let (options, hint) = self.property_tool_options();
        let mut body = column![text(heading).size(14)].spacing(10);
        if !options.is_empty() {
            body = body.push(card(container(column(options).spacing(8)).padding(12)));
        }
        match self.tool {
            Tool::Graphic(_) => {
                body = body.push(card(container(self.graphic_panel()).padding(12)));
            }
            Tool::Arrow => {
                body = body.push(card(container(self.arrow_panel()).padding(12)));
            }
            Tool::Text => {
                body = body.push(card(container(self.text_panel()).padding(12)));
            }
            Tool::RingPreset(preset) => {
                body = body.push(self.ring_preset_properties(preset));
            }
            _ => {}
        }
        if self.tool.bond_preset().is_some() || matches!(self.tool, Tool::Chain(_)) {
            body = body.push(
                reshiki::accessibility::button(
                    "properties-reset-bonds",
                    "Reset bond defaults",
                    text("Reset bonds").size(12).width(Length::Fill).center(),
                )
                .padding([6, 8])
                .width(Length::Fill)
                .style(crate::appearance::secondary)
                .on_press(Message::ResetBondDrawing),
            );
        }
        body.push(text(hint).size(11).style(muted_text)).into()
    }

    fn ring_preset_properties(&self, preset: reshiki::rings::Preset) -> Element<'_, Message> {
        use reshiki::rings::Preset;
        let details = format!(
            "{} {} on an atom connects the ring with a new bond. Each placement is one Undo step. {}",
            if preset == Preset::Benzene {
                "Click an aromatic carbon to attach a phenyl group, or a bond to fuse. Terminal carbons become part of the ring. Drag to choose the direction."
            } else {
                "Click to place. Drag to rotate or choose an attachment side. Click an atom to share it, or a bond to fuse."
            },
            crate::platform::alt_key(),
            match preset {
                Preset::Benzene | Preset::Cyclopentadiene => "Hold Shift to move the double bonds.",
                Preset::HaworthFive | Preset::HaworthSix =>
                    "Haworth outlines have a bold front edge. Templates → Carbohydrates contains oxygen scaffolds and defined α/β sugars. These blank outlines do not assign stereochemistry.",
                _ =>
                    "Chair projections do not assign stereochemistry. Cleanup may redraw them as regular hexagons.",
            },
        );
        card(
            container(
                column![
                    text(preset.to_string()).size(14),
                    hover_hint(
                        crate::canvas::layered::canvas(crate::canvas::DrawingThumbnail(
                            preset.document(self.tab.bond_drawing.length, false),
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
        )
    }
}

#[cfg(test)]
mod tests;
