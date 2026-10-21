//! Per-document local name parsing, editable previews and rule-based name generation.
use super::workspace::{accessible_checkbox, accessible_command, command, muted_text, section};
use super::{App, InspectorTab, Message};
use crate::canvas::layered::canvas;
use crate::canvas::{Camera, Edit, MoleculeCanvas, PreviewStyles, Tool};
use iced::widget::{Space, column, container, row, scrollable, text};
mod aliases;
mod import_dock;
pub use aliases::{AliasAction, AliasTarget};
mod molecule_label;
mod preview;
use iced::{Element, Length, Task};
use reshiki::accessibility::{button, text_input};
use reshiki::{
    document::{Document, Point},
    editing,
    engine::{LocalEngine, Request},
    naming::{self, Record},
};

const PANEL_INTRO: &str = "Read and generate names on this computer.";
const NAME_HINT: &str = "Ambiguous names and unsupported stereo are rejected.";
const PREVIEW_HINT: &str = "Drag atoms to arrange the layout.";
const STRUCTURE_HINT: &str = "Neutral C, N, O and halogen molecules, up to 64 heavy atoms.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket {
    epoch: u64,
    revision: u64,
    serial: u64,
}
impl Ticket {
    fn is_current(self, revision: u64, epoch: u64) -> bool {
        self.revision == revision && self.epoch == epoch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Request(Ticket),
    Preview(Ticket),
    Caption(Ticket),
    Import { ticket: Ticket, add_caption: bool },
}
impl Pending {
    fn ticket(self) -> Ticket {
        match self {
            Self::Request(ticket)
            | Self::Preview(ticket)
            | Self::Caption(ticket)
            | Self::Import { ticket, .. } => ticket,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Name(String),
    Cancel,
    Resolve,
    Generate,
    ShowMoleculeName(Vec<u64>),
    HideMoleculeName(Vec<u64>),
    Smiles(String),
    UpdatePreview,
    RestorePreview,
    PreviewEdit(Edit),
    Acknowledge(bool),
    Insert,
    InsertName,
    AddNameBelow(bool),
    PreviewOpen(bool),
    DetailsOpen(bool),
    CopyName,
    Caption,
    Alias(AliasAction),
    /// Selects this molecule and opens the Chemical names inspector.
    OpenPanel(Vec<u64>),
    Finished(Ticket, Box<Result<Outcome, naming::Error>>),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeKind {
    Retry,
    Unsupported,
    /// The name parsed to a salt or mixture.
    Disconnected,
    Failed,
    Info,
}

#[derive(Debug, Clone)]
struct Notice {
    kind: NoticeKind,
    detail: String,
}
impl Notice {
    fn new(kind: NoticeKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    fn summary(&self) -> &str {
        match self.kind {
            NoticeKind::Retry | NoticeKind::Info => &self.detail,
            NoticeKind::Unsupported => "Unsupported or ambiguous name. Try a more specific name.",
            NoticeKind::Disconnected => {
                "Salts and mixtures aren't supported yet. Draw the ions, or paste SMILES under Import → Structure."
            }
            NoticeKind::Failed => "Could not import this name. See Details.",
        }
    }
}
impl From<naming::Error> for Notice {
    fn from(error: naming::Error) -> Self {
        match error {
            naming::Error::Rejected(detail) => Self::new(NoticeKind::Unsupported, detail),
            naming::Error::Disconnected(detail) => Self::new(NoticeKind::Disconnected, detail),
            naming::Error::Worker(detail) => Self::new(NoticeKind::Failed, detail),
            naming::Error::Cancelled(detail) => Self::new(NoticeKind::Retry, detail),
        }
    }
}

pub(super) struct State {
    name: String,
    local_cancel: Option<naming::Cancel>,
    pending: Option<Pending>,
    serial: u64,
    candidates: Vec<Record>,
    preview: Option<Preview>,
    smiles: String,
    acknowledged: bool,
    structure: Option<(Ticket, Record)>,
    notice: Option<Notice>,
    label_target: Option<Vec<u64>>,
    add_name_below: bool,
    preview_open: bool,
    details_open: bool,
    import_alias: Option<usize>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            name: String::new(),
            local_cancel: None,
            pending: None,
            serial: 0,
            candidates: vec![],
            preview: None,
            smiles: String::new(),
            acknowledged: false,
            structure: None,
            notice: None,
            label_target: None,
            add_name_below: true,
            preview_open: false,
            details_open: false,
            import_alias: None,
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(cancel) = &self.local_cancel {
            cancel.stop();
        }
    }
}
impl State {
    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn stop_request(&mut self) {
        if let Some(cancel) = self.local_cancel.take() {
            cancel.stop();
        }
        self.serial = self.serial.wrapping_add(1);
        self.pending = None;
        self.label_target = None;
    }

    pub(super) fn invalidate_name(&mut self) {
        self.stop_request();
        self.candidates.clear();
        self.preview = None;
        self.notice = None;
        self.acknowledged = false;
        self.import_alias = None;
    }
    fn set_preview(&mut self, preview: Preview) {
        self.smiles = preview.identity.clone();
        self.acknowledged = false;
        self.import_alias = None;
        self.preview = Some(preview);
    }
}

async fn prepare_preview(
    engine: LocalEngine,
    record: Record,
    smiles: String,
) -> Result<Preview, naming::Error> {
    let identity = naming::canonical_smiles(&smiles).map_err(naming::Error::Rejected)?;
    let response = engine
        .request(Request::import_smiles(&smiles))
        .await
        .map_err(naming::Error::Worker)?;
    let document = response
        .document
        .ok_or_else(|| naming::Error::Worker("The parser returned no editable drawing".into()))?;
    let actual = naming::document_identity(&document)
        .map_err(naming::Error::Rejected)?
        .smiles;
    naming::verify_identity(&identity, &actual).map_err(naming::Error::Rejected)?;
    Ok(Preview::new(record, document, response.warnings, identity))
}

async fn resolve_preview(
    engine: LocalEngine,
    name: String,
    cancel: naming::Cancel,
) -> Result<Outcome, naming::Error> {
    let record = naming::resolve_name(&name, cancel).await?;
    let preview = prepare_preview(engine, record.clone(), record.smiles.clone()).await?;
    Ok(Outcome::Names(vec![record], Some(Box::new(preview))))
}

async fn generate_for_atoms(
    document: Document,
    atoms: Vec<u64>,
    cancel: naming::Cancel,
) -> Result<Outcome, naming::Error> {
    let identity =
        reshiki::task::spawn_blocking(move || naming::selected_identity(&document, &atoms))
            .await
            .map_err(|error| naming::Error::Worker(error.to_string()))?
            .map_err(naming::Error::Rejected)?;
    naming::generate_name(&identity, cancel)
        .await
        .map(Outcome::Structure)
}

impl App {
    fn name_preview_styles(&self) -> PreviewStyles<'_> {
        PreviewStyles {
            bond_drawing: self.tab.bond_drawing,
            chain_drawing: self.tab.chain_drawing,
            arrow_preset: self.tab.arrow_style,
            arrow_style: &self.tab.arrows.style,
            orbital_phase: self.tab.orbital_phase,
            graphic_style: &self.tab.graphic_style,
            bracket_sides: self.tab.bracket_sides,
        }
    }

    fn naming_ticket(&mut self) -> Ticket {
        self.tab.naming.serial = self.tab.naming.serial.wrapping_add(1);
        let ticket = Ticket {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            serial: self.tab.naming.serial,
        };
        self.tab.naming.pending = Some(Pending::Request(ticket));
        self.tab.naming.label_target = None;
        self.tab.naming.notice = None;
        ticket
    }

    fn naming_preview_ticket(&mut self) -> Ticket {
        let ticket = self.naming_ticket();
        self.tab.naming.pending = Some(Pending::Preview(ticket));
        ticket
    }

    fn begin_naming(
        &mut self,
        pending: impl FnOnce(Ticket) -> Pending,
    ) -> (Ticket, naming::Cancel) {
        let ticket = self.naming_ticket();
        self.tab.naming.pending = Some(pending(ticket));
        let cancel = naming::Cancel::default();
        self.tab.naming.local_cancel = Some(cancel.clone());
        (ticket, cancel)
    }

    fn naming_finished(&mut self, ticket: Ticket, result: Result<Outcome, naming::Error>) {
        if self.tab.naming.pending.map(Pending::ticket) != Some(ticket)
            || ticket.epoch != self.tab.file_epoch
        {
            return;
        }
        let caption = matches!(self.tab.naming.pending, Some(Pending::Caption(_)));
        let import_caption = match self.tab.naming.pending {
            Some(Pending::Import { add_caption, .. }) => Some(add_caption),
            _ => None,
        };
        let target = self.tab.naming.label_target.take();
        self.tab.naming.pending = None;
        self.tab.naming.local_cancel = None;
        if ticket.revision != self.tab.revision {
            self.tab.naming.notice = Some(Notice::new(
                NoticeKind::Retry,
                "The drawing changed. Try again.",
            ));
            if caption {
                self.status = "The molecule changed while naming. Show its name again".into();
                self.error = true;
            }
            return;
        }
        match result {
            Err(error) => {
                let notice = Notice::from(error);
                if caption {
                    self.status = notice.detail.clone();
                    self.error = true;
                }
                self.tab.naming.notice = Some(notice);
            }
            Ok(Outcome::Names(records, preview)) => {
                self.tab.naming.candidates = records;
                if let Some(preview) = preview {
                    self.tab.naming.set_preview(*preview);
                    if let Some(caption) = import_caption {
                        self.insert_import_preview(caption);
                    }
                } else if import_caption.is_some() {
                    self.tab.naming.notice = Some(Notice::new(
                        NoticeKind::Failed,
                        "No supported structure was returned.",
                    ));
                }
            }
            Ok(Outcome::Preview(preview)) => self.tab.naming.set_preview(*preview),
            Ok(Outcome::Structure(record)) => {
                if caption {
                    if let Some(atoms) = target {
                        self.place_molecule_name(&atoms, record);
                    }
                } else {
                    self.tab.naming.structure = Some((ticket, record));
                }
            }
        }
    }

    pub(super) fn naming_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Name(name) => {
                self.tab.naming.invalidate_name();
                self.tab.naming.name = name;
            }
            Action::Cancel => {
                self.tab.naming.stop_request();
                self.tab.naming.notice = Some(Notice::new(
                    NoticeKind::Retry,
                    "Local name parsing was cancelled",
                ));
            }
            Action::Resolve => {
                if self.tab.naming.name.trim().is_empty() || self.tab.naming.pending.is_some() {
                    return Task::none();
                }
                let (ticket, cancel) = self.begin_naming(Pending::Request);
                let name = self.tab.naming.name.clone();
                self.tab.naming.preview = None;
                self.tab.naming.candidates.clear();
                let engine = self.engine.clone();
                return Task::perform(resolve_preview(engine, name, cancel), move |result| {
                    Message::Naming(Action::Finished(ticket, Box::new(result)))
                });
            }
            Action::Generate => {
                if self.tab.naming.pending.is_some() {
                    return Task::none();
                }
                let (ticket, cancel) = self.begin_naming(Pending::Request);
                self.tab.naming.structure = None;
                let document = self.tab.doc.clone();
                let selected = self.tab.selected.clone();
                return Task::perform(
                    generate_for_atoms(document, selected, cancel),
                    move |result| Message::Naming(Action::Finished(ticket, Box::new(result))),
                );
            }
            Action::ShowMoleculeName(atoms) => return self.show_molecule_name(atoms),
            Action::HideMoleculeName(atoms) => self.hide_molecule_name(&atoms),
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
            Action::Finished(ticket, result) => self.naming_finished(ticket, *result),
            Action::PreviewEdit(edit) => self.edit_name_preview(edit),
            Action::Acknowledge(value) => self.tab.naming.acknowledged = value,
            Action::Insert => self.insert_name_preview(),
            Action::InsertName => return self.insert_name_import(),
            Action::AddNameBelow(value) => self.tab.naming.add_name_below = value,
            Action::PreviewOpen(value) => {
                self.tab.naming.preview_open = value;
                if value && self.tab.naming.preview.is_none() {
                    return self.naming_action(Action::Resolve);
                }
            }
            Action::DetailsOpen(value) => {
                self.tab.naming.details_open = value;
                if value
                    && self.tab.naming.preview.is_none()
                    && !self.tab.naming.name.trim().is_empty()
                {
                    return self.naming_action(Action::Resolve);
                }
            }
            Action::CopyName => {
                if let Some((_, record)) = &self.tab.naming.structure
                    && let Some(name) = &record.systematic_name
                {
                    return iced::clipboard::write(name.clone());
                }
            }
            Action::Caption => self.insert_name_caption(None),
            Action::Alias(action) => return self.naming_alias_action(action),
            Action::OpenPanel(atoms) => {
                // Structure → name works on the selection.
                let select = self.update(Message::Canvas(Edit::Select(atoms)));
                let open = self.update(Message::Inspector(InspectorTab::Names));
                return Task::batch([select, open]);
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
            } => crate::canvas::rotate(&mut preview.document, &ids, pivot, scale, rotation),
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

    pub(super) fn naming_can_show_molecule(&self) -> bool {
        self.naming_can_edit() && self.tab.naming.pending.is_none()
    }

    pub(super) fn context_name_target(&self) -> Option<Vec<u64>> {
        let hit = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.hit.as_deref());
        molecule_label::target(&self.tab.doc, hit.unwrap_or(&[]), &self.tab.selected)
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
            self.tab.naming.notice = Some(Notice::new(
                NoticeKind::Retry,
                "Apply the edited SMILES to the preview before insertion",
            ));
            return;
        }
        let actual = naming::document_identity(&preview.document)
            .and_then(|i| naming::verify_identity(&preview.identity, &i.smiles));
        if let Err(error) = actual {
            self.tab.naming.notice = Some(Notice::new(NoticeKind::Unsupported, error));
            return;
        }
        let document = preview.document.clone();
        let title = preview.record.title.clone();
        let before = self.tab.doc.clone();
        let Some(selected) = self.insert_beside_drawing(&document) else {
            self.tab.doc = before;
            self.error = true;
            self.status = format!("Could not insert {title}");
            return;
        };
        self.tab.selected = selected;
        self.changed(before);
        self.tool = Tool::Select;
        self.fit();
        self.status = "Inserted editable name result · Undo removes the insertion".into();
        self.error = false;
    }

    fn insert_name_caption(&mut self, alias: Option<usize>) {
        if !self.naming_can_show_molecule() {
            return;
        }
        let Some((ticket, record)) = self.tab.naming.structure.clone() else {
            return;
        };
        if !ticket.is_current(self.tab.revision, self.tab.file_epoch) {
            const DETAIL: &str =
                "The drawing changed. Generate its name again before inserting a caption";
            self.tab.naming.notice = Some(Notice::new(NoticeKind::Retry, DETAIL));
            return;
        }
        let current = naming::selected_identity(&self.tab.doc, &self.tab.selected);
        if !current.is_ok_and(|i| i.smiles == record.canonical_smiles) {
            const DETAIL: &str =
                "The selection changed. Select the named molecule before inserting a caption";
            self.tab.naming.notice = Some(Notice::new(NoticeKind::Retry, DETAIL));
            return;
        }
        let atoms = editing::analysis_atoms(&self.tab.doc, &self.tab.selected);
        if let Some(index) = alias {
            let Some(alias) = record.synonyms.get(index) else {
                return;
            };
            self.place_molecule_caption(&atoms, alias.name.clone(), record.canonical_smiles);
        } else {
            self.place_molecule_name(&atoms, record);
        }
    }

    /// The Chemical names inspector, opened from a molecule's right-click
    /// menu: a fixed header above the scrolling panel.
    pub(super) fn naming_inspector(&self) -> Element<'_, Message> {
        let header = column![
            row![
                command("‹ Properties", Message::Inspector(InspectorTab::Properties)),
                Space::new().width(Length::Fill),
                accessible_command("naming.import", "Open SMILES import", "Import")
                    .on_press(Message::Inspector(InspectorTab::Import)),
            ]
            .align_y(iced::Alignment::Center),
            text("Chemical names").size(20),
            text(PANEL_INTRO).size(11).style(muted_text),
        ]
        .spacing(8);
        column![
            container(header).padding([12, 16]),
            scrollable(container(self.naming_panel()).padding(iced::Padding {
                top: 0.,
                right: 16.,
                bottom: 16.,
                left: 16.,
            }))
            .id("inspector-content")
            .height(Length::Fill),
        ]
        .into()
    }

    /// Name → structure, then structure → name for the selection.
    pub(super) fn naming_panel(&self) -> Element<'_, Message> {
        let state = &self.tab.naming;
        let idle = state.pending.is_none();
        let mut name = column![
            section("NAME → STRUCTURE"),
            text_input(
                "naming.name",
                "Chemical name",
                "Chemical name, e.g. ethanol",
                &state.name
            )
            .on_input(|s| Message::Naming(Action::Name(s)))
            .on_submit(Message::Naming(Action::Resolve))
            .size(13)
            .padding(8),
            text(NAME_HINT).size(11).style(muted_text),
            accessible_command("naming.resolve", "Parse name locally", "Parse name")
                .on_press_maybe(
                    (idle && !state.name.trim().is_empty())
                        .then_some(Message::Naming(Action::Resolve))
                ),
        ]
        .spacing(10);
        if let Some(notice) = &state.notice {
            name = name.push(
                text(&notice.detail)
                    .size(12)
                    .color(self.theme().palette().danger),
            );
        }
        if !idle {
            let mut working = row![text("Working…").size(12)]
                .spacing(10)
                .align_y(iced::Alignment::Center);
            if state.local_cancel.is_some() {
                working = working.push(
                    accessible_command("naming.cancel", "Cancel local naming", "Cancel")
                        .on_press(Message::Naming(Action::Cancel)),
                );
            }
            name = name.push(working);
        }
        if let Some(preview) = &state.preview {
            name = name.push(self.naming_preview(preview));
        }
        let structure = column![
            section("STRUCTURE → NAME"),
            text(STRUCTURE_HINT).size(11).style(muted_text),
            accessible_command(
                "naming.generate",
                "Generate selected structure name locally",
                "Generate selected name",
            )
            .on_press_maybe(idle.then_some(Message::Naming(Action::Generate))),
            self.generated_name_controls(),
        ]
        .spacing(10);
        column![name, structure].spacing(24).into()
    }

    /// The parsed name's editable preview, its SMILES and the insertion.
    fn naming_preview<'a>(&'a self, preview: &'a Preview) -> Element<'a, Message> {
        let state = &self.tab.naming;
        let idle = state.pending.is_none();
        let needs_ack = !preview.record.warnings.is_empty() || !preview.warnings.is_empty();
        let mut content = column![
            text(preview.record.provenance.label())
                .size(11)
                .style(muted_text),
            text(&preview.record.title).size(14),
            self.name_preview_canvas(preview, 180.),
            text(PREVIEW_HINT).size(11).style(muted_text),
            text_input(
                "naming.smiles",
                "Editable preview SMILES",
                "Preview SMILES",
                &state.smiles,
            )
            .on_input(|s| Message::Naming(Action::Smiles(s)))
            .size(12)
            .padding(7),
            row![
                accessible_command("naming.update-preview", "Update preview", "Update")
                    .on_press_maybe(idle.then_some(Message::Naming(Action::UpdatePreview))),
                accessible_command(
                    "naming.restore-preview",
                    "Restore parsed structure",
                    "Restore"
                )
                .on_press_maybe(idle.then_some(Message::Naming(Action::RestorePreview))),
            ]
            .spacing(4),
        ]
        .spacing(8);
        if preview.modified {
            content = content.push(
                text("Edited structure: the name may not match.")
                    .size(11)
                    .style(muted_text),
            );
        }
        content = content.push(
            text(format!(
                "{} atoms · {} bonds",
                preview.document.atoms.len(),
                preview.document.bonds.len()
            ))
            .size(11)
            .style(muted_text),
        );
        for warning in preview.record.warnings.iter().chain(&preview.warnings) {
            content = content.push(text(warning).size(11));
        }
        if needs_ack {
            content = content.push(accessible_checkbox(
                "naming.acknowledge",
                "I reviewed these warnings and the structure",
                state.acknowledged,
                Some(Message::Naming(Action::Acknowledge(!state.acknowledged))),
            ));
        }
        let ready = idle
            && self.naming_can_edit()
            && (!needs_ack || state.acknowledged)
            && state.smiles == preview.identity;
        content
            .push(row![
                Space::new().width(Length::Fill),
                // Right-aligned, as Insert is in the Import dock.
                button(
                    "naming.insert",
                    "Insert editable structure",
                    text("Insert").size(12)
                )
                .padding([6, 12])
                .style(crate::appearance::primary)
                .on_press_maybe(ready.then_some(Message::Naming(Action::Insert))),
            ])
            .into()
    }

    /// The bordered, editable preview canvas shared by this panel and the
    /// Import dock, which differ only in its height.
    fn name_preview_canvas<'a>(
        &'a self,
        preview: &'a Preview,
        height: f32,
    ) -> Element<'a, Message> {
        let drawing: Element<'a, Edit> = canvas(preview::Preview(MoleculeCanvas::preview(
            &preview.document,
            &preview.selected,
            preview.camera,
            self.name_preview_styles(),
        )))
        .width(Length::Fill)
        .height(height)
        .into();
        container(drawing.map(|edit| Message::Naming(Action::PreviewEdit(edit))))
            .style(container::bordered_box)
            .into()
    }
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
