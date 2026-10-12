//! Source-labelled aliases share the Import dock and normal linked captions.
use super::*;
use iced::widget::column;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasTarget {
    Import,
    Structure,
}
#[derive(Debug, Clone)]
pub enum AliasAction {
    Copy(AliasTarget, usize),
    UseBelow(Option<usize>),
    Caption(usize),
}

impl App {
    fn alias_record(&self, target: AliasTarget) -> Option<&Record> {
        if self.tab.naming.pending.is_some() {
            return None;
        }
        match target {
            AliasTarget::Import => {
                let preview = self.tab.naming.preview.as_ref()?;
                (!preview.modified && self.tab.naming.smiles == preview.identity)
                    .then_some(&preview.record)
            }
            AliasTarget::Structure => {
                let (ticket, record) = self.tab.naming.structure.as_ref()?;
                ticket
                    .is_current(self.tab.revision, self.tab.file_epoch)
                    .then_some(record)
            }
        }
    }

    pub(super) fn naming_alias_action(&mut self, action: AliasAction) -> Task<Message> {
        match action {
            AliasAction::Copy(target, index) => {
                if let Some(alias) = self
                    .alias_record(target)
                    .and_then(|r| r.synonyms.get(index))
                {
                    return iced::clipboard::write(alias.name.clone());
                }
            }
            AliasAction::UseBelow(index) => {
                if self
                    .alias_record(AliasTarget::Import)
                    .is_some_and(|record| index.is_none_or(|i| record.synonyms.get(i).is_some()))
                {
                    self.tab.naming.import_alias = index;
                    self.tab.naming.add_name_below = true;
                }
            }
            AliasAction::Caption(index) => self.insert_name_caption(Some(index)),
        }
        Task::none()
    }

    pub(super) fn alias_controls<'a>(
        &'a self,
        target: AliasTarget,
        record: &'a Record,
    ) -> Element<'a, Message> {
        let available = self.alias_record(target).is_some();
        let prefix = match target {
            AliasTarget::Import => "import-name",
            AliasTarget::Structure => "naming",
        };
        let mut content = column![text("Curated aliases · offline").size(12)].spacing(6);
        if record.synonyms.is_empty() {
            content =
                content.push(text("No curated aliases available in the bundled catalog.").size(11));
        }
        for (index, alias) in record.synonyms.iter().enumerate() {
            let selected =
                target == AliasTarget::Import && self.tab.naming.import_alias == Some(index);
            content = content.push(
                column![
                    text(&alias.name).size(13),
                    text(alias.source.label()).size(10),
                    row![
                        button(
                            format!("{prefix}-alias-copy-{index}"),
                            format!("Copy alias {}", alias.name),
                            text("Copy").size(11)
                        )
                        .on_press_maybe(available.then_some(
                            Message::Naming(Action::Alias(AliasAction::Copy(target, index)))
                        )),
                        button(
                            format!("{prefix}-alias-caption-{index}"),
                            format!(
                                "{} {}",
                                if target == AliasTarget::Import {
                                    "Use alias below"
                                } else {
                                    "Insert alias caption"
                                },
                                alias.name
                            ),
                            text(if selected {
                                "✓ Use below"
                            } else if target == AliasTarget::Import {
                                "Use below"
                            } else {
                                "Insert caption"
                            })
                            .size(11)
                        )
                        .checked(selected)
                        .on_press_maybe(
                            (available && self.naming_can_edit()).then_some(Message::Naming(
                                Action::Alias(if target == AliasTarget::Import {
                                    AliasAction::UseBelow(Some(index))
                                } else {
                                    AliasAction::Caption(index)
                                })
                            ))
                        ),
                    ]
                    .spacing(6),
                ]
                .spacing(3),
            );
        }
        if target == AliasTarget::Import && self.tab.naming.import_alias.is_some() {
            content = content.push(
                button(
                    "import-name-original-caption",
                    "Use original input name below",
                    text("Use original name below").size(11),
                )
                .on_press_maybe(
                    available
                        .then_some(Message::Naming(Action::Alias(AliasAction::UseBelow(None)))),
                ),
            );
        }
        content.into()
    }

    pub(super) fn generated_name_controls(&self) -> Element<'_, Message> {
        let mut content = column![].spacing(8);
        if let Some((ticket, record)) = &self.tab.naming.structure {
            let current = ticket.is_current(self.tab.revision, self.tab.file_epoch);
            if !current {
                content = content.push(text("Previous name · The drawing changed. Generate the selection name again before using it.").size(12));
            }
            content = content.push(text(record.provenance.label()).size(11));
            content = content.push(text("Generated systematic name · local rules").size(12));
            if let Some(name) = &record.systematic_name {
                content = content.push(text(name).size(15));
            }
            for warning in &record.warnings {
                content = content.push(text(warning).size(11));
            }
            content = content.push(
                row![
                    button(
                        "naming.copy-name",
                        "Copy systematic name",
                        text("Copy name").size(11)
                    )
                    .on_press(Message::Naming(Action::CopyName)),
                    button(
                        "naming.caption",
                        "Insert systematic name caption",
                        text("Insert caption").size(11)
                    )
                    .on_press_maybe(
                        (current && self.naming_can_show_molecule())
                            .then_some(Message::Naming(Action::Caption))
                    ),
                ]
                .spacing(6),
            );
            content = content.push(self.alias_controls(AliasTarget::Structure, record));
        }
        content.into()
    }
}

#[cfg(test)]
mod tests;
