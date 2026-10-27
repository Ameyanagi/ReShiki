//! Per-query consent for the optional online name lookup.
use super::*;
use iced::widget::column;

impl App {
    pub(super) fn open_online_name_source(&mut self) {
        let Some(url) = self
            .tab
            .naming
            .preview
            .as_ref()
            .and_then(|preview| preview.record.provenance.source_url())
        else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        let result = open::that(&url).map_err(|error| error.to_string());
        #[cfg(target_arch = "wasm32")]
        let result = crate::web::open_url(&url);
        if let Err(error) = result {
            self.tab.naming.notice = Some(Notice::new(NoticeKind::Retry, error));
        }
    }

    pub(super) fn ask_online_name(&mut self) {
        if self.naming_can_edit()
            && self.tab.naming.pending.is_none()
            && !self.tab.naming.name.trim().is_empty()
        {
            self.tab.naming.serial = self.tab.naming.serial.wrapping_add(1);
            self.tab.naming.online_confirmation =
                Some((self.tab.naming.serial, self.tab.naming.name.trim().into()));
        }
    }

    pub(super) fn confirm_online_name(&mut self, serial: u64) -> Task<Message> {
        if self
            .tab
            .naming
            .online_confirmation
            .as_ref()
            .map(|(current, _)| *current)
            != Some(serial)
        {
            return Task::none();
        }
        let (_, name) = self.tab.naming.online_confirmation.take().unwrap();
        if !self.naming_can_edit()
            || self.tab.naming.pending.is_some()
            || name != self.tab.naming.name.trim()
        {
            return Task::none();
        }
        let (ticket, cancel) = self.begin_naming(Pending::Online);
        self.tab.naming.preview = None;
        self.tab.naming.candidates.clear();
        self.tab.naming.preview_open = true;
        let engine = self.engine.clone();
        Task::perform(
            async move {
                let record = naming::resolve_name_online(&name, cancel).await?;
                let preview =
                    prepare_preview(engine, record.clone(), record.smiles.clone()).await?;
                Ok(Outcome::Names(vec![record], Some(Box::new(preview))))
            },
            move |result| Message::Naming(Action::Finished(ticket, Box::new(result))),
        )
    }

    pub(super) fn online_name_controls(&self) -> Element<'_, Message> {
        let state = &self.tab.naming;
        if let Some((serial, name)) = &state.online_confirmation {
            return container(
                column![
                    text("Send this name to PubChem?").size(13),
                    text(name).size(13).wrapping(text::Wrapping::WordOrGlyph),
                    text("This uses the internet and sends the name to pubchem.ncbi.nlm.nih.gov (NCBI). Your drawing is not sent. Review the returned structure before inserting.")
                        .size(11),
                    row![
                        accessible_command("naming.online-dismiss", "Cancel online lookup", "Cancel")
                            .on_press(Message::Naming(Action::DismissOnline)),
                        accessible_command("naming.online-confirm", "Send this name to PubChem", "Send name")
                            .on_press(Message::Naming(Action::ConfirmOnline(*serial))),
                    ].spacing(8),
                ].spacing(8),
            )
            .padding(10)
            .style(container::bordered_box)
            .into();
        }
        let mut content = column![
            accessible_command(
                "naming.online-ask",
                "Confirm an online lookup of this name",
                "Look up online…"
            )
            .on_press_maybe(
                (state.pending.is_none()
                    && !state.name.trim().is_empty()
                    && self.naming_can_edit())
                .then_some(Message::Naming(Action::AskOnline))
            ),
        ]
        .spacing(6);
        if let Some(preview) = &state.preview
            && matches!(
                preview.record.provenance,
                naming::Provenance::PubChem { .. }
            )
        {
            content = content.push(
                accessible_command(
                    "naming.online-source",
                    "Open this PubChem compound record in your browser",
                    preview.record.provenance.label(),
                )
                .on_press(Message::Naming(Action::OpenOnlineSource)),
            );
        }
        content.into()
    }
}

#[cfg(test)]
mod tests;
