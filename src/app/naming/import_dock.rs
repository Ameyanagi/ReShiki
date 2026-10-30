//! Chemical names share the ordinary Import dock. Insertion resolves and
//! verifies locally in one action; opening the editable preview is optional.
use super::super::workspace::{accessible_checkbox, accessible_command, muted_text};
use super::*;
use iced::widget::{Space, column};

const DETAILS: &str = "Names are parsed locally by default. Online lookup sends a name to PubChem only after you confirm. Ambiguous results and unsupported chemistry are rejected.";

impl App {
    pub(in crate::app) fn cancel_name_import(&mut self) {
        self.tab.naming.online_confirmation = None;
        if matches!(
            self.tab.naming.pending,
            Some(
                Pending::Import { .. }
                    | Pending::Request(_)
                    | Pending::Generate(_)
                    | Pending::Preview(_)
                    | Pending::Online(_)
            )
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
        let (ticket, cancel) = self.begin_naming(|ticket| Pending::Import {
            ticket,
            add_caption: caption,
        });
        let engine = self.engine.clone();
        let name = self.tab.naming.name.clone();
        Task::perform(resolve_preview(engine, name, cancel), move |result| {
            Message::Naming(Action::Finished(ticket, Box::new(result)))
        })
    }

    pub(super) fn insert_import_preview(&mut self, caption: bool) {
        if !self.naming_can_edit() || self.tab.naming.pending.is_some() {
            self.tab.naming.notice = Some(Notice::new(
                NoticeKind::Retry,
                "Finish the current edit, then try again.",
            ));
            return;
        }
        let Some(preview) = self.tab.naming.preview.clone() else {
            return;
        };
        if self.tab.naming.smiles != preview.identity {
            self.tab.naming.notice = Some(Notice::new(
                NoticeKind::Retry,
                "Apply the edited SMILES before inserting.",
            ));
            return;
        }
        if let Err(error) = naming::document_identity(&preview.document)
            .and_then(|actual| naming::verify_identity(&preview.identity, &actual.smiles))
        {
            self.tab.naming.notice = Some(Notice::new(NoticeKind::Unsupported, error));
            return;
        }
        let mut document = preview.document;
        let omit_caption = caption && preview.identity != preview.record.canonical_smiles;
        let caption_text = if let Some(index) = self.tab.naming.import_alias {
            let Some(alias) = preview.record.synonyms.get(index) else {
                self.tab.naming.notice = Some(Notice::new(
                    NoticeKind::Retry,
                    "The alias choice changed. Choose its name again.",
                ));
                return;
            };
            alias.name.clone()
        } else {
            preview.record.title.clone()
        };
        if caption && !omit_caption {
            let atoms = document
                .atoms
                .iter()
                .map(|atom| atom.id)
                .collect::<Vec<_>>();
            if let Err(error) = reshiki::molecule_names::show(
                &mut document,
                &atoms,
                caption_text,
                preview.record.canonical_smiles,
                self.tab.caption_format.clone(),
            ) {
                self.tab.naming.notice = Some(Notice::new(NoticeKind::Failed, error));
                return;
            }
        }
        let before = self.tab.doc.clone();
        let Some(selected) = self.insert_beside_drawing(&document) else {
            self.tab.doc = before;
            self.error = true;
            let title = &preview.record.title;
            self.status = format!("Could not insert {title}");
            return;
        };
        self.tab.selected = selected;
        self.changed(before);
        if self.error {
            return;
        }
        self.tool = Tool::Select;
        self.fit();
        self.tab.naming.notice = omit_caption.then(|| {
            Notice::new(
                NoticeKind::Info,
                "Edited structure inserted; original name omitted.",
            )
        });
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
        let caption = state.add_name_below && !changed;
        let mut content = column![
            text_input(
                "import-name",
                "Chemical name",
                "Chemical name, e.g. ethanol",
                &state.name
            )
            .size(13)
            .padding(8)
            .on_input(|value| Message::Naming(Action::Name(value)))
            .on_submit_maybe(ready.then_some(Message::Naming(Action::InsertName))),
            accessible_checkbox(
                "import-name-caption",
                "Add name below",
                caption,
                (idle && !changed)
                    .then_some(Message::Naming(Action::AddNameBelow(!state.add_name_below))),
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
                .padding([6, 10])
                .style(crate::appearance::secondary)
                .on_press_maybe(
                    (idle && has_name)
                        .then_some(Message::Naming(Action::PreviewOpen(!state.preview_open)))
                ),
                Space::new().width(Length::Fill),
                // Right-aligned, as Insert is in Structure mode.
                button(
                    "import-name-insert",
                    "Insert chemical name as structure",
                    text("Insert").size(12)
                )
                .padding([6, 12])
                .style(crate::appearance::primary)
                .on_press_maybe(ready.then_some(Message::InsertInput)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(10);
        if let Some(index) = state.import_alias
            && let Some(alias) = state
                .preview
                .as_ref()
                .and_then(|p| p.record.synonyms.get(index))
        {
            content = content.push(
                text(format!("Name below: {}", alias.name))
                    .size(11)
                    .style(muted_text),
            );
        }
        if let Some(notice) = &state.notice
            && notice.source == NoticeSource::Parse
        {
            content = content.push(
                text(notice.summary())
                    .size(12)
                    .color(self.theme().palette().danger),
            );
        }
        content = content.push(self.online_name_controls());
        if !idle {
            content = content.push(
                row![
                    text(self.naming_working_label()).size(12),
                    accessible_command("import-name-cancel", "Cancel name import", "Cancel")
                        .on_press(Message::Naming(Action::Cancel)),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
            );
        }
        if state.preview_open
            && let Some(preview) = &state.preview
        {
            content = content.push(self.compact_name_preview(preview));
        }
        content = content.push(
            accessible_command(
                "import-name-details",
                "Name import details",
                if state.details_open {
                    "▾ Details"
                } else {
                    "▸ Details"
                },
            )
            .expanded(state.details_open)
            .on_press(Message::Naming(Action::DetailsOpen(!state.details_open))),
        );
        if state.details_open {
            content = content.push(self.name_import_details(idle && has_name));
        }
        content.into()
    }

    /// Details: what the parser does, its provenance, aliases and the
    /// generated name of the selection.
    fn name_import_details(&self, can_resolve: bool) -> Element<'_, Message> {
        let state = &self.tab.naming;
        let idle = state.pending.is_none();
        let mut content = column![text(DETAILS).size(11).style(muted_text)].spacing(10);
        if state.preview.is_none() {
            content = content.push(
                accessible_command(
                    "import-name-resolve",
                    "Resolve chemical name without inserting",
                    "Resolve name",
                )
                .on_press_maybe(can_resolve.then_some(Message::Naming(Action::Resolve))),
            );
        }
        if let Some(notice) = &state.notice
            && notice.source == NoticeSource::Parse
        {
            content = content.push(text(&notice.detail).size(11));
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
            if preview.modified {
                content = content.push(
                    text("Edited preview · Aliases need the original name")
                        .size(11)
                        .style(muted_text),
                );
            } else {
                content = content.push(self.alias_controls(AliasTarget::Import, &preview.record));
            }
        }
        content = content.push(
            accessible_command(
                "import-name-generate",
                "Generate selected structure name locally",
                "Generate selected name",
            )
            .on_press_maybe(
                (idle && !self.tab.selected.is_empty())
                    .then_some(Message::Naming(Action::Generate)),
            ),
        );
        if let Some(notice) = &state.notice
            && notice.source == NoticeSource::Generate
        {
            content = content.push(
                text(&notice.detail)
                    .size(11)
                    .color(self.theme().palette().danger),
            );
        }
        content.push(self.generated_name_controls()).into()
    }

    fn compact_name_preview<'a>(&'a self, preview: &'a Preview) -> Element<'a, Message> {
        let state = &self.tab.naming;
        let idle = state.pending.is_none();
        let mut content = column![
            self.name_preview_canvas(preview, 160.),
            text(crate::app::workspace::atom_bond_counts(
                preview.document.atoms.len(),
                preview.document.bonds.len(),
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
            .padding(7)
            .on_input(|value| Message::Naming(Action::Smiles(value))),
            row![
                accessible_command("import-name-apply", "Apply preview SMILES", "Apply")
                    .on_press_maybe(idle.then_some(Message::Naming(Action::UpdatePreview))),
                accessible_command("import-name-reset", "Reset parsed structure", "Reset")
                    .on_press_maybe(idle.then_some(Message::Naming(Action::RestorePreview))),
            ]
            .spacing(4),
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
