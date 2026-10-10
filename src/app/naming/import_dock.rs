//! Chemical names share the ordinary Import dock. Insertion resolves and
//! verifies locally in one action; opening the editable preview is optional.
use super::super::workspace::muted_text;
use super::*;
use iced::widget::column;

fn compact_notice(message: &str) -> &str {
    if message.contains("changed")
        || message.contains("cancelled")
        || message.contains("Apply")
        || message.contains("Finish")
        || message.contains("omitted")
    {
        message
    } else if message.contains("could not interpret")
        || message.contains("unsupported")
        || message.contains("does not support")
        || message.contains("disconnected")
    {
        "Unsupported or ambiguous name. Try a more specific name."
    } else {
        "Could not import this name. See Details."
    }
}

impl App {
    pub(in crate::app) fn cancel_name_import(&mut self) {
        if matches!(
            self.tab.naming.pending,
            Some(Pending::Import(..) | Pending::Request(_) | Pending::Preview(_))
        ) {
            let _ = self.naming_action(Action::Cancel);
        }
    }

    pub(super) fn insert_name_import(&mut self) -> Task<Message> {
        if !self.naming_can_edit()
            || self.tab.naming.pending.is_some()
            || self.tab.naming.name.trim().is_empty()
        {
            return Task::none();
        }
        let caption = self.tab.naming.add_name_below;
        if self.tab.naming.preview.is_some() {
            self.insert_import_preview(caption);
            return Task::none();
        }
        let ticket = self.naming_ticket();
        self.tab.naming.pending = Some(Pending::Import(ticket, caption));
        let cancel = naming::Cancel::default();
        self.tab.naming.local_cancel = Some(cancel.clone());
        let engine = self.engine.clone();
        let name = self.tab.naming.name.clone();
        Task::perform(resolve_preview(engine, name, cancel), move |result| {
            Message::Naming(Action::Finished(ticket, Box::new(result)))
        })
    }

    pub(super) fn insert_import_preview(&mut self, caption: bool) {
        if !self.naming_can_edit() || self.tab.naming.pending.is_some() {
            self.tab.naming.notice = Some("Finish the current edit, then try again.".into());
            return;
        }
        let Some(preview) = self.tab.naming.preview.clone() else {
            return;
        };
        if self.tab.naming.smiles != preview.identity {
            self.tab.naming.notice = Some("Apply the edited SMILES before inserting.".into());
            return;
        }
        if let Err(error) = naming::document_identity(&preview.document)
            .and_then(|actual| naming::verify_identity(&preview.identity, &actual.smiles))
        {
            self.tab.naming.notice = Some(error);
            return;
        }
        let mut document = preview.document;
        let omit_caption = caption && preview.identity != preview.record.canonical_smiles;
        if caption && !omit_caption {
            let atoms = document
                .atoms
                .iter()
                .map(|atom| atom.id)
                .collect::<Vec<_>>();
            if let Err(error) = reshiki::molecule_names::show(
                &mut document,
                &atoms,
                preview.record.title.clone(),
                preview.record.canonical_smiles,
                self.tab.caption_format.clone(),
            ) {
                self.tab.naming.notice = Some(error);
                return;
            }
        }
        let center = editing::center(&document, &document.all_ids());
        let offset = if self.tab.doc.all_ids().is_empty() {
            Point::new(
                self.tab.camera.center.x - center.x,
                self.tab.camera.center.y - center.y,
            )
        } else {
            let (_, hi) = self.tab.doc.bounds();
            let (lo, _) = document.bounds();
            Point::new(
                hi.x + self.tab.doc.drawing_style.bond_length_world - lo.x,
                self.tab.camera.center.y - center.y,
            )
        };
        let before = self.tab.doc.clone();
        self.tab.selected = editing::append(&mut self.tab.doc, &document, offset);
        self.changed(before);
        if self.error {
            return;
        }
        self.tool = Tool::Select;
        self.fit();
        self.tab.naming.notice =
            omit_caption.then(|| "Edited structure inserted; original name omitted.".into());
        self.status = if omit_caption {
            "Inserted edited structure · Original name omitted · Undo removes it".into()
        } else {
            format!("Inserted {} · Undo removes it", preview.record.title)
        };
    }

    pub(in crate::app) fn name_import_panel(&self) -> Element<'_, Message> {
        let state = &self.tab.naming;
        let idle = state.pending.is_none();
        let has_name = !state.name.trim().is_empty();
        let ready = idle
            && has_name
            && self.naming_can_edit()
            && state
                .preview
                .as_ref()
                .is_none_or(|preview| state.smiles == preview.identity);
        let changed = state
            .preview
            .as_ref()
            .is_some_and(|preview| preview.modified);
        let mut content = column![
            text_input("import-name", "Chemical name", "e.g. ethanol", &state.name)
                .size(13)
                .padding(8)
                .on_input(|value| Message::Naming(Action::Name(value)))
                .on_submit_maybe(ready.then_some(Message::Naming(Action::InsertName))),
            button(
                "import-name-caption",
                "Add name below",
                text(if state.add_name_below && !changed {
                    "✓ Add name below"
                } else {
                    "Add name below"
                })
                .size(12)
            )
            .checked(state.add_name_below && !changed)
            .style(super::super::workspace::control(
                state.add_name_below && !changed
            ))
            .on_press_maybe(
                (idle && !changed)
                    .then_some(Message::Naming(Action::AddNameBelow(!state.add_name_below)))
            ),
            row![
                button(
                    "import-name-preview",
                    "Optional editable preview",
                    text(if state.preview_open {
                        "Hide preview"
                    } else {
                        "Preview"
                    })
                    .size(12)
                )
                .expanded(state.preview_open)
                .style(crate::appearance::secondary)
                .on_press_maybe(
                    (idle && has_name)
                        .then_some(Message::Naming(Action::PreviewOpen(!state.preview_open)))
                ),
                button(
                    "import-name-insert",
                    "Insert chemical name as structure",
                    text("Insert").size(12)
                )
                .style(crate::appearance::primary)
                .on_press_maybe(ready.then_some(Message::InsertInput)),
            ]
            .spacing(8),
        ]
        .spacing(10);
        if let Some(notice) = &state.notice {
            content = content.push(
                text(compact_notice(notice))
                    .size(12)
                    .color(self.theme().palette().danger),
            );
        }
        if !idle {
            content = content.push(
                row![
                    text("Working…").size(12),
                    button(
                        "import-name-cancel",
                        "Cancel name import",
                        text("Cancel").size(12)
                    )
                    .on_press(Message::Naming(Action::Cancel)),
                ]
                .spacing(10),
            );
        }
        if state.preview_open
            && let Some(preview) = &state.preview
        {
            content = content.push(self.compact_name_preview(preview));
        }
        content = content.push(
            button(
                "import-name-details",
                "Name import details",
                text(if state.details_open {
                    "Hide details"
                } else {
                    "Details"
                })
                .size(11),
            )
            .expanded(state.details_open)
            .style(super::super::workspace::control(false))
            .on_press(Message::Naming(Action::DetailsOpen(!state.details_open))),
        );
        if state.details_open {
            content = content.push(text("Names are parsed locally with the Rust OPSIN backend. Ambiguous names and unsupported chemical semantics are rejected.").size(11).style(muted_text));
            if let Some(notice) = &state.notice {
                content = content.push(text(notice).size(11));
            }
            if let Some(preview) = &state.preview {
                content = content.push(
                    text(preview.record.provenance.label())
                        .size(11)
                        .style(muted_text),
                );
                for warning in preview.record.warnings.iter().chain(&preview.warnings) {
                    content = content.push(text(warning).size(11));
                }
            }
        }
        content.into()
    }

    fn compact_name_preview<'a>(&'a self, preview: &'a Preview) -> Element<'a, Message> {
        let drawing: Element<'_, Edit> = canvas(preview::Preview(MoleculeCanvas {
            nmr: None,
            optimizer: None,
            keyboard_target: None,
            joining: None,
            hidden_annotation: None,
            bond_drawing: self.tab.bond_drawing,
            chain_drawing: self.tab.chain_drawing,
            doc: &preview.document,
            element: "C",
            selected: &preview.selected,
            tool: Tool::Select,
            camera: preview.camera,
            grid: false,
            guides: Default::default(),
            smart_guides: false,
            ring_size: 6,
            aromatic_ring: false,
            template_connection: Default::default(),
            template: None,
            arrow_preset: self.tab.arrow_style,
            arrow_style: &self.tab.arrows.style,
            arrow_source: None,
            attach_arrow_targets: false,
            orbital_phase: self.tab.orbital_phase,
            phase_flipped: false,
            attach_symbols: false,
            snap_orbitals: false,
            graphic_constrain: false,
            graphic_arc: Default::default(),
            graphic_style: &self.tab.graphic_style,
            graphic_point: None,
            bracket_sides: self.tab.bracket_sides,
        }))
        .width(Length::Fill)
        .height(160)
        .into();
        let state = &self.tab.naming;
        let idle = state.pending.is_none();
        let mut content = column![
            container(drawing.map(|edit| Message::Naming(Action::PreviewEdit(edit))))
                .style(container::bordered_box),
            text(format!(
                "{} atoms · {} bonds",
                preview.document.atoms.len(),
                preview.document.bonds.len()
            ))
            .size(11)
            .style(muted_text),
            text_input(
                "import-name-smiles",
                "Preview SMILES",
                "SMILES",
                &state.smiles
            )
            .size(12)
            .on_input(|value| Message::Naming(Action::Smiles(value))),
            row![
                button(
                    "import-name-apply",
                    "Apply preview SMILES",
                    text("Apply").size(11)
                )
                .on_press_maybe(idle.then_some(Message::Naming(Action::UpdatePreview))),
                button(
                    "import-name-reset",
                    "Reset parsed structure",
                    text("Reset").size(11)
                )
                .on_press_maybe(idle.then_some(Message::Naming(Action::RestorePreview))),
            ]
            .spacing(6),
        ]
        .spacing(8);
        if preview.modified {
            content = content.push(
                text("Edited structure: name below is omitted.")
                    .size(11)
                    .style(muted_text),
            );
        }
        content.into()
    }
}

#[cfg(test)]
mod tests;
