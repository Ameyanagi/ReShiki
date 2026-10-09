//! Per-document, opt-in online naming workflow and locally editable previews.
use super::{App, InspectorTab, Message};
use crate::canvas::layered::canvas;
use crate::canvas::{Camera, Edit, MoleculeCanvas, Tool};
use iced::widget::{column, container, row, text};
mod preview;
use iced::{Element, Length, Task};
use reshiki::accessibility::{button, text_input};
use reshiki::{
    document::{Annotation, Document, Point},
    editing,
    engine::{LocalEngine, Request},
    naming::{self, NameSource, Record, Service},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket {
    epoch: u64,
    revision: u64,
    serial: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Request(Ticket),
    Preview(Ticket),
}
impl Pending {
    fn ticket(self) -> Ticket {
        match self {
            Self::Request(ticket) | Self::Preview(ticket) => ticket,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Name(String),
    Source(NameSource),
    Consent(bool),
    StructureConsent(bool),
    Resolve,
    Lookup,
    Choose(usize),
    Smiles(String),
    UpdatePreview,
    RestorePreview,
    PreviewEdit(Edit),
    Acknowledge(bool),
    Insert,
    CopyName,
    Caption,
    OpenSource,
    Finished(Ticket, Box<Result<Outcome, String>>),
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Names(Vec<Record>, Option<Box<Preview>>),
    Preview(Box<Preview>),
    Structure(Record),
}

#[derive(Debug, Clone)]
pub struct Preview {
    record: Record,
    document: Document,
    warnings: Vec<String>,
    selected: Vec<u64>,
    camera: Camera,
    /// The graph the preview is allowed to insert, separate from source text.
    identity: String,
    modified: bool,
}
impl Preview {
    fn new(record: Record, document: Document, warnings: Vec<String>, identity: String) -> Self {
        let (lo, hi) = document.bounds();
        Self {
            modified: identity != record.canonical_smiles,
            record,
            document,
            warnings,
            identity,
            selected: vec![],
            camera: Camera {
                center: Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
                zoom: (260. / (hi.x - lo.x + 50.).max(100.))
                    .min(140. / (hi.y - lo.y + 50.).max(100.))
                    .min(1.5),
            },
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    name: String,
    source: NameSource,
    consent: bool,
    structure_consent: bool,
    pending: Option<Pending>,
    serial: u64,
    candidates: Vec<Record>,
    preview: Option<Preview>,
    smiles: String,
    acknowledged: bool,
    structure: Option<(Ticket, Record)>,
    notice: Option<String>,
}
impl State {
    fn invalidate_name(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        self.pending = None;
        self.candidates.clear();
        self.preview = None;
        self.notice = None;
        self.acknowledged = false;
        self.consent = false;
    }
    fn set_preview(&mut self, preview: Preview) {
        self.smiles = preview.identity.clone();
        self.acknowledged = false;
        self.preview = Some(preview);
    }
}

async fn prepare_preview(
    engine: LocalEngine,
    record: Record,
    smiles: String,
) -> Result<Preview, String> {
    let identity = naming::canonical_smiles(&smiles)?;
    let response = engine.request(Request::import_smiles(&smiles)).await?;
    let document = response
        .document
        .ok_or("The parser returned no editable drawing")?;
    let actual = naming::document_identity(&document)?.smiles;
    naming::verify_identity(&identity, &actual)?;
    Ok(Preview::new(record, document, response.warnings, identity))
}

impl App {
    fn naming_ticket(&mut self) -> Ticket {
        self.tab.naming.serial = self.tab.naming.serial.wrapping_add(1);
        let ticket = Ticket {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            serial: self.tab.naming.serial,
        };
        self.tab.naming.pending = Some(Pending::Request(ticket));
        self.tab.naming.notice = None;
        ticket
    }

    fn naming_preview_ticket(&mut self) -> Ticket {
        let ticket = self.naming_ticket();
        self.tab.naming.pending = Some(Pending::Preview(ticket));
        ticket
    }

    pub(super) fn naming_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Name(name) => {
                self.tab.naming.invalidate_name();
                self.tab.naming.name = name;
            }
            Action::Source(source) => {
                self.tab.naming.invalidate_name();
                self.tab.naming.source = source;
            }
            Action::Consent(value) => self.tab.naming.consent = value,
            Action::StructureConsent(value) => self.tab.naming.structure_consent = value,
            Action::Resolve => {
                if !self.tab.naming.consent || self.tab.naming.pending.is_some() {
                    return Task::none();
                }
                let ticket = self.naming_ticket();
                let name = self.tab.naming.name.clone();
                let source = self.tab.naming.source;
                self.tab.naming.consent = false;
                self.tab.naming.preview = None;
                self.tab.naming.candidates.clear();
                let engine = self.engine.clone();
                return Task::perform(
                    async move {
                        let records = Service::new()?.resolve_name(&name, source).await?;
                        let preview = if let [record] = records.as_slice() {
                            Some(Box::new(
                                prepare_preview(engine, record.clone(), record.smiles.clone())
                                    .await?,
                            ))
                        } else {
                            None
                        };
                        Ok(Outcome::Names(records, preview))
                    },
                    move |result| Message::Naming(Action::Finished(ticket, Box::new(result))),
                );
            }
            Action::Lookup => {
                if !self.tab.naming.structure_consent || self.tab.naming.pending.is_some() {
                    return Task::none();
                }
                let ticket = self.naming_ticket();
                self.tab.naming.structure_consent = false;
                self.tab.naming.structure = None;
                let document = self.tab.doc.clone();
                let selected = self.tab.selected.clone();
                return Task::perform(
                    async move {
                        let identity = tokio::task::spawn_blocking(move || {
                            naming::selected_identity(&document, &selected)
                        })
                        .await
                        .map_err(|e| e.to_string())??;
                        Service::new()?
                            .lookup_structure(identity)
                            .await
                            .map(Outcome::Structure)
                    },
                    move |result| Message::Naming(Action::Finished(ticket, Box::new(result))),
                );
            }
            Action::Choose(index) => {
                if self.tab.naming.pending.is_some() {
                    return Task::none();
                }
                let Some(record) = self.tab.naming.candidates.get(index).cloned() else {
                    return Task::none();
                };
                let ticket = self.naming_preview_ticket();
                let engine = self.engine.clone();
                return Task::perform(
                    async move {
                        prepare_preview(engine, record.clone(), record.smiles.clone())
                            .await
                            .map(|p| Outcome::Preview(Box::new(p)))
                    },
                    move |result| Message::Naming(Action::Finished(ticket, Box::new(result))),
                );
            }
            Action::Smiles(smiles) => {
                let state = &mut self.tab.naming;
                if state.smiles != smiles && matches!(state.pending, Some(Pending::Preview(_))) {
                    // A local rebuild must never overwrite newer text or make
                    // its older chemical identity insertable after this edit.
                    state.serial = state.serial.wrapping_add(1);
                    state.pending = None;
                    state.notice = None;
                }
                state.smiles = smiles;
            }
            Action::UpdatePreview | Action::RestorePreview => {
                if self.tab.naming.pending.is_some() {
                    return Task::none();
                }
                let Some(preview) = &self.tab.naming.preview else {
                    return Task::none();
                };
                let record = preview.record.clone();
                let smiles = if matches!(action, Action::RestorePreview) {
                    record.smiles.clone()
                } else {
                    self.tab.naming.smiles.clone()
                };
                let ticket = self.naming_preview_ticket();
                let engine = self.engine.clone();
                return Task::perform(
                    async move {
                        prepare_preview(engine, record, smiles)
                            .await
                            .map(|p| Outcome::Preview(Box::new(p)))
                    },
                    move |result| Message::Naming(Action::Finished(ticket, Box::new(result))),
                );
            }
            Action::Finished(ticket, result) => {
                if self.tab.naming.pending.map(Pending::ticket) != Some(ticket)
                    || ticket.epoch != self.tab.file_epoch
                {
                    return Task::none();
                }
                self.tab.naming.pending = None;
                if ticket.revision != self.tab.revision {
                    self.tab.naming.notice =
                        Some("The drawing changed while naming. Request the result again".into());
                    return Task::none();
                }
                match *result {
                    Err(error) => self.tab.naming.notice = Some(error),
                    Ok(Outcome::Names(records, preview)) => {
                        self.tab.naming.candidates = records;
                        if let Some(preview) = preview {
                            self.tab.naming.set_preview(*preview);
                        }
                    }
                    Ok(Outcome::Preview(preview)) => self.tab.naming.set_preview(*preview),
                    Ok(Outcome::Structure(record)) => {
                        self.tab.naming.structure = Some((ticket, record))
                    }
                }
            }
            Action::PreviewEdit(edit) => self.edit_name_preview(edit),
            Action::Acknowledge(value) => self.tab.naming.acknowledged = value,
            Action::Insert => self.insert_name_preview(),
            Action::CopyName => {
                if let Some((_, record)) = &self.tab.naming.structure
                    && let Some(name) = &record.systematic_name
                {
                    return iced::clipboard::write(name.clone());
                }
            }
            Action::Caption => self.insert_name_caption(),
            Action::OpenSource => {
                if let Some((_, record)) = &self.tab.naming.structure
                    && let Err(error) = open::that(record.provenance.url())
                {
                    self.tab.naming.notice = Some(format!("Could not open source: {error}"));
                }
            }
        }
        Task::none()
    }

    fn edit_name_preview(&mut self, edit: Edit) {
        if self.tab.naming.pending.is_some() {
            return;
        }
        let Some(preview) = &mut self.tab.naming.preview else {
            return;
        };
        match edit {
            Edit::Select(ids) | Edit::SelectAt(ids, _) => preview.selected = ids,
            Edit::Move(ids, dx, dy) => preview.document.translate(&ids, dx, dy),
            Edit::Transform {
                ids,
                pivot,
                scale,
                rotation,
            } => editing::transform_about(&mut preview.document, &ids, pivot, scale, rotation),
            Edit::ScaleAxes { ids, pivot, x, y } => {
                editing::scale_axes_about(&mut preview.document, &ids, pivot, x, y)
            }
            Edit::Pan(dx, dy) => preview.camera.center = preview.camera.center.offset(-dx, -dy),
            Edit::Zoom(factor, at) => {
                let old = preview.camera.zoom;
                preview.camera.zoom = (old * factor).clamp(0.05, 5.);
                let ratio = old / preview.camera.zoom;
                preview.camera.center = Point::new(
                    at.x + (preview.camera.center.x - at.x) * ratio,
                    at.y + (preview.camera.center.y - at.y) * ratio,
                );
            }
            _ => {}
        }
    }

    fn naming_can_edit(&self) -> bool {
        !self.tab.busy
            && self.pending.is_none()
            && self.tab.cleanup.is_none()
            && self.tab.inline_text.is_none()
            && self.tab.atom_text.is_none()
            && self.tab.joining.is_none()
            && self.tab.optimization.is_none()
    }

    fn insert_name_preview(&mut self) {
        if !self.naming_can_edit() || self.tab.naming.pending.is_some() {
            return;
        }
        let Some(preview) = &self.tab.naming.preview else {
            return;
        };
        if (!preview.record.warnings.is_empty() || !preview.warnings.is_empty())
            && !self.tab.naming.acknowledged
        {
            return;
        }
        if self.tab.naming.smiles != preview.identity {
            self.tab.naming.notice =
                Some("Apply the edited SMILES to the preview before insertion".into());
            return;
        }
        let actual = naming::document_identity(&preview.document)
            .and_then(|i| naming::verify_identity(&preview.identity, &i.smiles));
        if let Err(error) = actual {
            self.tab.naming.notice = Some(error);
            return;
        }
        let document = preview.document.clone();
        let before = self.tab.doc.clone();
        let center = editing::center(&document, &document.all_ids());
        let offset = if self.tab.doc.atoms.is_empty() {
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
        self.tab.selected = editing::append(&mut self.tab.doc, &document, offset);
        self.changed(before);
        self.tool = Tool::Select;
        self.fit();
        self.status = "Inserted editable name result · Undo removes the insertion".into();
        self.error = false;
    }

    fn insert_name_caption(&mut self) {
        if !self.naming_can_edit() {
            return;
        }
        let Some((ticket, record)) = &self.tab.naming.structure else {
            return;
        };
        if ticket.revision != self.tab.revision || ticket.epoch != self.tab.file_epoch {
            self.tab.naming.notice = Some(
                "The drawing changed. Look up its name again before inserting a caption".into(),
            );
            return;
        }
        let Some(name) = record.systematic_name.clone() else {
            return;
        };
        let selected = editing::selection(&self.tab.doc, &self.tab.selected);
        let current = naming::selected_identity(&self.tab.doc, &self.tab.selected);
        if !current.is_ok_and(|i| i.smiles == record.canonical_smiles) {
            self.tab.naming.notice = Some(
                "The selection changed. Select the named molecule before inserting a caption"
                    .into(),
            );
            return;
        }
        let (lo, hi) = selected.bounds();
        let before = self.tab.doc.clone();
        let id = self.tab.doc.next_id();
        let mut format = self.tab.caption_format.clone();
        format.spans.clear();
        format.style.formula = false;
        format.alignment = reshiki::typography::TextAlign::Center;
        self.tab.doc.annotations.push(Annotation {
            id,
            text: name,
            position: Point::new((lo.x + hi.x) / 2., lo.y - 24.),
            format,
        });
        self.changed(before);
        self.status = "Inserted source systematic name as a caption · Undo removes it".into();
    }

    pub(super) fn naming_panel(&self) -> Element<'_, Message> {
        let state = &self.tab.naming;
        let idle = state.pending.is_none();
        let mut content = column![
            row![text("Chemical names").size(18), button("naming.import", "Open SMILES import", text("Import")).on_press(Message::Inspector(InspectorTab::Import))].spacing(12),
            text("Online tools · Review the source interpretation before use.").size(12),
            text("Name → structure").size(15),
            row![button("naming.source.opsin", "OPSIN systematic name parser", text("OPSIN").size(12)).checked(state.source == NameSource::Opsin).on_press(Message::Naming(Action::Source(NameSource::Opsin))), button("naming.source.pubchem", "PubChem common name lookup", text("PubChem").size(12)).checked(state.source == NameSource::PubChem).on_press(Message::Naming(Action::Source(NameSource::PubChem)))].spacing(6),
            text_input("naming.name", "Chemical name", "Chemical name, e.g. ethanol", &state.name).on_input(|s| Message::Naming(Action::Name(s))).on_submit(Message::Naming(Action::Resolve)).size(13),
            text(match state.source {
                NameSource::Opsin => "OPSIN parses supported systematic nomenclature on EMBL-EBI's server. Common names may require PubChem.",
                NameSource::PubChem => "PubChem finds exact names in its database. Results are source interpretations; an alias may have multiple meanings.",
            }).size(12),
            button("naming.consent.name", match state.source { NameSource::Opsin => "Send this name to EMBL-EBI", NameSource::PubChem => "Send this name to NCBI PubChem" }, text(match state.source { NameSource::Opsin => "Send this name to EMBL-EBI", NameSource::PubChem => "Send this name to NCBI PubChem" }).size(12)).checked(state.consent).style(super::workspace::control(state.consent)).on_press(Message::Naming(Action::Consent(!state.consent))),
            button("naming.resolve", "Resolve name online", text("Resolve name online")).on_press_maybe((idle && state.consent && !state.name.trim().is_empty()).then_some(Message::Naming(Action::Resolve))),
        ].spacing(10);
        if let Some(notice) = &state.notice {
            content = content.push(text(notice).size(12).color(self.theme().palette().danger));
        }
        if !idle {
            content = content.push(text("Working…").size(12));
        }
        if state.candidates.len() > 1 {
            content =
                content.push(text("Ambiguous source name · Choose an interpretation").size(13));
            for (index, candidate) in state.candidates.iter().enumerate() {
                content = content.push(
                    button(
                        format!("naming.candidate.{index}"),
                        format!(
                            "Choose {} from {}",
                            candidate.title,
                            candidate.provenance.label()
                        ),
                        text(format!(
                            "{} · {}",
                            candidate.title,
                            candidate.provenance.label()
                        ))
                        .size(12),
                    )
                    .on_press_maybe(idle.then_some(Message::Naming(Action::Choose(index)))),
                );
                if let Some(name) = &candidate.systematic_name {
                    content = content.push(text(name).size(11));
                }
            }
        }
        if let Some(preview) = &state.preview {
            content = content.push(text(preview.record.provenance.label()).size(12));
            content = content.push(text(&preview.record.title).size(14));
            let drawing: Element<'_, Edit> = canvas(preview::Preview(MoleculeCanvas {
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
                orbital_phase: self.tab.orbital_phase,
                phase_flipped: false,
                attach_symbols: false,
                graphic_constrain: false,
                graphic_arc: Default::default(),
                graphic_style: &self.tab.graphic_style,
                bracket_sides: self.tab.bracket_sides,
            }))
            .width(Length::Fill)
            .height(180)
            .into();
            let drawing = drawing.map(|edit| Message::Naming(Action::PreviewEdit(edit)));
            content = content.push(container(drawing).style(container::bordered_box));
            content = content.push(text("Editable preview · Drag atoms to adjust layout. Edit SMILES to change chemistry.").size(12));
            content = content.push(
                text_input(
                    "naming.smiles",
                    "Editable preview SMILES",
                    "Preview SMILES",
                    &state.smiles,
                )
                .on_input(|s| Message::Naming(Action::Smiles(s)))
                .size(12),
            );
            content = content.push(
                row![
                    button(
                        "naming.update-preview",
                        "Update preview",
                        text("Update preview")
                    )
                    .on_press_maybe(idle.then_some(Message::Naming(Action::UpdatePreview))),
                    button(
                        "naming.restore-preview",
                        "Restore source",
                        text("Restore source")
                    )
                    .on_press_maybe(idle.then_some(Message::Naming(Action::RestorePreview))),
                ]
                .spacing(6),
            );
            if preview.modified {
                content = content.push(text("Edited chemical identity · The original source name may no longer describe this preview.").size(12));
            }
            content = content.push(text(format!("{} atoms · {} bonds · Specified stereo is retained; unspecified stereo remains unspecified.", preview.document.atoms.len(), preview.document.bonds.len())).size(12));
            let warnings = preview.record.warnings.iter().chain(&preview.warnings);
            for warning in warnings {
                content = content.push(text(warning).size(12));
            }
            let needs_ack = !preview.record.warnings.is_empty() || !preview.warnings.is_empty();
            if needs_ack {
                content = content.push(
                    button(
                        "naming.acknowledge",
                        "I reviewed these warnings and the structure",
                        text("I reviewed these warnings and the structure").size(12),
                    )
                    .checked(state.acknowledged)
                    .style(super::workspace::control(state.acknowledged))
                    .on_press(Message::Naming(Action::Acknowledge(!state.acknowledged))),
                );
            }
            content = content.push(
                button(
                    "naming.insert",
                    "Insert editable structure",
                    text("Insert editable structure"),
                )
                .on_press_maybe(
                    (idle
                        && self.naming_can_edit()
                        && (!needs_ack || state.acknowledged)
                        && state.smiles == preview.identity)
                        .then_some(Message::Naming(Action::Insert)),
                ),
            );
        }
        content = content.push(text("Structure → name").size(15));
        content = content.push(text("Select a complete connected molecule. PubChem returns its source systematic name and synonyms after an exact graph/stereo/isotope check. This is database lookup; novel structures may have no result.").size(12));
        content = content.push(
            button(
                "naming.consent.structure",
                "Send selected molecular SMILES to NCBI PubChem",
                text("Send selected molecular SMILES to NCBI PubChem").size(12),
            )
            .checked(state.structure_consent)
            .style(super::workspace::control(state.structure_consent))
            .on_press(Message::Naming(Action::StructureConsent(
                !state.structure_consent,
            ))),
        );
        content = content.push(
            button(
                "naming.lookup",
                "Look up selected structure online",
                text("Look up selected structure online"),
            )
            .on_press_maybe(
                (idle && state.structure_consent).then_some(Message::Naming(Action::Lookup)),
            ),
        );
        if let Some((ticket, record)) = &state.structure {
            if ticket.revision != self.tab.revision {
                content = content.push(text("Previous lookup · The drawing changed. Look up the selection again before using its name.").size(12));
            }
            content = content.push(text(record.provenance.label()).size(12));
            content = content.push(text("Systematic name · PubChem lookup").size(12));
            if let Some(name) = &record.systematic_name {
                content = content.push(text(name).size(15));
            }
            content = content.push(text(format!("Source title: {}", record.title)).size(12));
            if !record.synonyms.is_empty() {
                content = content.push(
                    text(format!(
                        "Source synonyms (may include registry identifiers):\n{}",
                        record.synonyms.join("\n")
                    ))
                    .size(12),
                );
            }
            for warning in &record.warnings {
                content = content.push(text(warning).size(12));
            }
            content = content.push(
                row![
                    button("naming.copy-name", "Copy name", text("Copy name"))
                        .on_press(Message::Naming(Action::CopyName)),
                    button("naming.caption", "Insert caption", text("Insert caption"))
                        .on_press_maybe(
                            (ticket.revision == self.tab.revision && self.naming_can_edit())
                                .then_some(Message::Naming(Action::Caption))
                        ),
                    button("naming.source-record", "Source", text("Source"))
                        .on_press(Message::Naming(Action::OpenSource)),
                ]
                .spacing(6),
            );
        }
        content.into()
    }
}

#[cfg(test)]
mod tests;
