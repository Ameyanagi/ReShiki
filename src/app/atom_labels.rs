use super::*;
use reshiki::atom_labels::{
    self as labels, AxialDescriptors, Carbons, HydrogenPosition, LabelAttachment, Number,
};
use reshiki::naming;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocantsTicket {
    epoch: u64,
    revision: u64,
    serial: u64,
}

struct LocantsJob {
    ticket: LocantsTicket,
    cancel: naming::Cancel,
}

struct LocantsPreview {
    ticket: LocantsTicket,
    numbers: HashMap<u64, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    Drawing,
    Selection,
}
impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Drawing => "Whole drawing",
            Self::Selection => "Selected atoms",
        })
    }
}
pub struct State {
    pub scope: Scope,
    pub seed: String,
    pub number: String,
    pub size: String,
    pub ring_atoms_only: bool,
    locants_serial: u64,
    locants_job: Option<LocantsJob>,
    locants_preview: Option<LocantsPreview>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            scope: Scope::Drawing,
            seed: "1".into(),
            number: String::new(),
            size: "7.5".into(),
            ring_atoms_only: false,
            locants_serial: 0,
            locants_job: None,
            locants_preview: None,
        }
    }
}
impl State {
    pub(super) fn locants_running(&self) -> bool {
        self.locants_job.is_some()
    }

    pub(super) fn locants_replacements(
        &self,
        document: &Document,
        epoch: u64,
        revision: u64,
    ) -> Option<usize> {
        let preview = self.locants_preview.as_ref()?;
        if preview.ticket.epoch != epoch || preview.ticket.revision != revision {
            return None;
        }
        Some(
            document
                .atoms
                .iter()
                .filter(|atom| {
                    preview.numbers.contains_key(&atom.id) && atom.display.number.is_some()
                })
                .count(),
        )
    }

    fn cancel_locants(&mut self) {
        if let Some(job) = self.locants_job.take() {
            job.cancel.stop();
        }
        self.locants_preview = None;
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel_locants();
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Scope(Scope),
    Carbons(Carbons),
    Hydrogens(bool),
    Charges(bool),
    Position(HydrogenPosition),
    Attachment(LabelAttachment),
    Stereo(bool),
    RomanStereoParentheses(bool),
    AxialDescriptors(AxialDescriptors),
    Seed(String),
    Number,
    IupacLocants,
    RingAtomsOnly(bool),
    CancelLocants,
    ReplaceLocants,
    LocantsFinished(LocantsTicket, Box<Result<HashMap<u64, String>, String>>),
    ClearNumbers,
    Text(String),
    ApplyText,
    Size(String),
    ApplySize,
    ToolbarStyle,
    PositionIndicators,
    ResetPositions,
    ResetOverrides,
}

async fn generate_locant_numbers(
    document: Document,
    atoms: Vec<u64>,
    ring_atoms_only: bool,
    cancel: naming::Cancel,
) -> Result<HashMap<u64, String>, String> {
    let (identity, selection) = reshiki::task::spawn_blocking(move || {
        let identity = naming::selected_identity(&document, &atoms)?;
        let ids = reshiki::editing::analysis_atoms(&document, &atoms);
        Ok::<_, String>((identity, reshiki::editing::selection(&document, &ids)))
    })
    .await
    .map_err(|error| error.to_string())??;
    let record = naming::generate_name(&identity, cancel)
        .await
        .map_err(|error| error.to_string())?;
    reshiki::task::spawn_blocking(move || {
        let locants = record
            .locants
            .ok_or("The generated name has no parent atom locants")?;
        filter_locant_numbers(
            &selection,
            locants.for_document(&selection),
            ring_atoms_only,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The selection's parent locants, optionally only on ring atoms.
fn filter_locant_numbers(
    document: &Document,
    mut numbers: HashMap<u64, String>,
    ring_atoms_only: bool,
) -> Result<HashMap<u64, String>, String> {
    if ring_atoms_only {
        let molecule =
            reshiki::chemistry::document::prepare(document).map_err(|error| error.to_string())?;
        let rings: HashSet<_> = molecule
            .state
            .rings
            .atoms
            .iter()
            .flatten()
            .filter_map(|&index| molecule.ids.get(index).copied())
            .collect();
        numbers.retain(|id, _| rings.contains(id));
    }
    if numbers.is_empty() {
        return Err(if ring_atoms_only {
            "The selected molecule has no ring atoms with parent locants"
        } else {
            "The selected molecule has no mapped parent atom locants"
        }
        .into());
    }
    Ok(numbers)
}

impl App {
    pub(super) fn invalidate_locant_numbering(&mut self) {
        let current = |ticket: LocantsTicket| {
            ticket.epoch == self.tab.file_epoch && ticket.revision == self.tab.revision
        };
        if self
            .tab
            .labels
            .locants_job
            .as_ref()
            .is_some_and(|job| !current(job.ticket))
            || self
                .tab
                .labels
                .locants_preview
                .as_ref()
                .is_some_and(|preview| !current(preview.ticket))
        {
            self.tab.labels.cancel_locants();
        }
    }

    pub(super) fn label_ids(&self) -> Vec<u64> {
        self.tab
            .doc
            .atoms
            .iter()
            .filter(|a| {
                self.tab.labels.scope == Scope::Drawing || self.tab.selected.contains(&a.id)
            })
            .map(|a| a.id)
            .collect()
    }
    pub(super) fn label_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::IupacLocants => return self.number_by_iupac_locants(),
            Action::RingAtomsOnly(value) => {
                let pending = self.tab.labels.locants_job.is_some()
                    || self.tab.labels.locants_preview.is_some();
                self.tab.labels.cancel_locants();
                self.tab.labels.ring_atoms_only = value;
                if pending {
                    self.status = "IUPAC locant numbering cancelled".into();
                    self.error = false;
                }
            }
            Action::CancelLocants => {
                self.tab.labels.cancel_locants();
                self.status = "IUPAC locant numbering cancelled".into();
                self.error = false;
            }
            Action::ReplaceLocants => {
                if let Some(preview) = self.tab.labels.locants_preview.take() {
                    self.apply_locant_numbers(preview);
                }
            }
            Action::LocantsFinished(ticket, result) => {
                if self.tab.labels.locants_job.as_ref().map(|job| job.ticket) != Some(ticket) {
                    return Task::none();
                }
                self.tab.labels.locants_job = None;
                if !self.locants_ticket_current(ticket) {
                    return Task::none();
                }
                match *result {
                    Ok(numbers) => {
                        let preview = LocantsPreview { ticket, numbers };
                        if preview.numbers.keys().any(|id| {
                            self.tab
                                .doc
                                .atom(*id)
                                .is_some_and(|atom| atom.display.number.is_some())
                        }) {
                            self.tab.labels.locants_preview = Some(preview);
                            self.status =
                                "Review the existing atom numbers before replacing them".into();
                            self.error = false;
                        } else {
                            self.apply_locant_numbers(preview);
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            action => {
                if let Err(error) = self.change_labels(action) {
                    self.status = error;
                    self.error = true;
                }
            }
        }
        Task::none()
    }

    fn number_by_iupac_locants(&mut self) -> Task<Message> {
        if self.tab.labels.locants_running() {
            return Task::none();
        }
        self.tab.labels.cancel_locants();
        self.tab.labels.locants_serial = self.tab.labels.locants_serial.wrapping_add(1);
        let ticket = LocantsTicket {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            serial: self.tab.labels.locants_serial,
        };
        let cancel = naming::Cancel::default();
        self.tab.labels.locants_job = Some(LocantsJob {
            ticket,
            cancel: cancel.clone(),
        });
        self.status = "Generating IUPAC parent locants locally…".into();
        self.error = false;
        Task::perform(
            generate_locant_numbers(
                self.tab.doc.clone(),
                self.tab.selected.clone(),
                self.tab.labels.ring_atoms_only,
                cancel,
            ),
            move |result| Message::Labels(Action::LocantsFinished(ticket, Box::new(result))),
        )
    }

    fn locants_ticket_current(&mut self, ticket: LocantsTicket) -> bool {
        if ticket.epoch != self.tab.file_epoch || ticket.revision != self.tab.revision {
            self.status = "The drawing changed. Request IUPAC locants again".into();
            self.error = true;
            return false;
        }
        true
    }

    fn apply_locant_numbers(&mut self, preview: LocantsPreview) {
        if !self.locants_ticket_current(preview.ticket) {
            return;
        }
        if !self.naming_can_edit() {
            self.status = "Finish the current edit, then request IUPAC locants again".into();
            self.error = true;
            return;
        }
        let before = self.tab.doc.clone();
        let count = preview.numbers.len();
        for atom in &mut self.tab.doc.atoms {
            if let Some(locant) = preview.numbers.get(&atom.id) {
                atom.display.number = Some(Number {
                    text: naming::locant_text(locant),
                    offset: None,
                    style: labels::number_style(),
                });
            }
        }
        self.error = false;
        self.changed(before);
        if !self.error {
            self.status = format!(
                "IUPAC locants applied to {count} atoms · Undo restores the previous numbers"
            );
        }
    }

    fn change_labels(&mut self, action: Action) -> Result<(), String> {
        let before = self.tab.doc.clone();
        let ids = self.label_ids();
        match action {
            Action::IupacLocants
            | Action::RingAtomsOnly(_)
            | Action::CancelLocants
            | Action::ReplaceLocants
            | Action::LocantsFinished(..) => return Ok(()),
            Action::Scope(scope) => self.tab.labels.scope = scope,
            Action::Seed(s) => self.tab.labels.seed = s,
            Action::Text(s) => self.tab.labels.number = s,
            Action::Size(s) => self.tab.labels.size = s,
            Action::PositionIndicators => {
                self.tab.selected = ids;
                self.tool = Tool::EditPoints;
                self.status =
                    "Drag a number, atom map or stereochemistry handle · Escape finishes".into();
            }
            Action::Carbons(value) => {
                if self.tab.labels.scope == Scope::Drawing {
                    self.tab.doc.atom_labels.carbons = value;
                }
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.carbons =
                        (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
            }
            Action::Hydrogens(value) => {
                if self.tab.labels.scope == Scope::Drawing {
                    self.tab.doc.atom_labels.hydrogens = value;
                }
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.hydrogens =
                        (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
            }
            Action::Charges(show) => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.hide_charge = !show;
                }
            }
            Action::Position(value) => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.hydrogen_position = value;
                }
            }
            Action::Attachment(value) => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id) && labels::free_text(a, &before))
                {
                    a.display.label_attachment = value;
                }
            }
            Action::Stereo(value) => {
                if self.tab.labels.scope == Scope::Drawing {
                    self.tab.doc.atom_labels.stereo = value;
                }
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.stereo.show =
                        (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
                for b in self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.show = (self.tab.labels.scope == Scope::Selection).then_some(value);
                }
                self.tab.labels_dirty = true;
            }
            Action::RomanStereoParentheses(value) => {
                self.tab.doc.atom_labels.roman_stereo_parentheses = value;
            }
            Action::AxialDescriptors(value) => self.tab.doc.atom_labels.axial_descriptors = value,
            Action::Number => {
                let sequence = labels::sequence(&self.tab.labels.seed, ids.len())?;
                for (id, text) in ids.iter().zip(sequence) {
                    let atom = self
                        .tab
                        .doc
                        .atom_mut(*id)
                        .ok_or("The numbered atom is no longer available")?;
                    atom.display.number = Some(Number {
                        text,
                        offset: None,
                        style: labels::number_style(),
                    });
                }
            }
            Action::ClearNumbers => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.number = None;
                }
            }
            Action::ApplyText => {
                if ids.len() != 1 {
                    return Err("Select one atom to edit its number".into());
                }
                let id = ids
                    .first()
                    .copied()
                    .ok_or("Select one atom to edit its number")?;
                let a = self
                    .tab
                    .doc
                    .atom_mut(id)
                    .ok_or("The numbered atom is no longer available")?;
                let mut display = a.display.clone();
                display
                    .number
                    .get_or_insert_with(|| Number {
                        text: String::new(),
                        offset: None,
                        style: labels::number_style(),
                    })
                    .text = self.tab.labels.number.trim().into();
                display.validate()?;
                a.display = display;
            }
            Action::ApplySize | Action::ToolbarStyle => {
                let size = if matches!(action, Action::ApplySize) {
                    Some(
                        self.tab
                            .labels
                            .size
                            .parse::<f32>()
                            .ok()
                            .filter(|x| x.is_finite() && (4.0..=144.).contains(x))
                            .ok_or("Enter an indicator size from 4 to 144 pt")?,
                    )
                } else {
                    None
                };
                let mut style = self.current_text_style().clone();
                style.script = reshiki::typography::Script::Normal;
                style.formula = false;
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    for s in std::iter::once(&mut a.display.stereo.style)
                        .chain(std::iter::once(&mut a.display.mapping.style))
                        .chain(a.display.number.iter_mut().map(|n| &mut n.style))
                    {
                        if let Some(size) = size {
                            s.size_pt = size;
                        } else {
                            *s = style.clone();
                        }
                    }
                }
                for b in self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    if let Some(size) = size {
                        b.indicator.style.size_pt = size;
                    } else {
                        b.indicator.style = style.clone();
                    }
                }
            }
            Action::ResetPositions => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.stereo.offset = None;
                    a.display.mapping.offset = None;
                    if let Some(n) = &mut a.display.number {
                        n.offset = None;
                    }
                }
                for b in self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.offset = None;
                }
            }
            Action::ResetOverrides => {
                for a in self
                    .tab
                    .doc
                    .atoms
                    .iter_mut()
                    .filter(|a| ids.contains(&a.id))
                {
                    a.display.carbons = None;
                    a.display.hide_charge = false;
                    a.display.hydrogens = None;
                    a.display.hydrogen_position = HydrogenPosition::Auto;
                    a.display.label_attachment = LabelAttachment::Auto;
                    a.display.stereo.show = None;
                    a.display.mapping.show = None;
                }
                for b in self
                    .tab
                    .doc
                    .bonds
                    .iter_mut()
                    .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                {
                    b.indicator.show = None;
                }
            }
        }
        self.changed(before);
        Ok(())
    }

    /// Which glyph of the text labels in scope meets their bonds; None when
    /// the scope has no text label.
    pub(super) fn label_attachment_picker<'a>(
        &self,
        atoms: &[&reshiki::document::Atom],
    ) -> Option<Element<'a, Message>> {
        let labeled: Vec<_> = atoms
            .iter()
            .filter(|a| labels::free_text(a, &self.tab.doc))
            .collect();
        let current = labeled.first()?.display.label_attachment;
        let mut choices: Vec<_> = LabelAttachment::SIDES
            .into_iter()
            .map(|value| AttachmentChoice(value, value.to_string()))
            .collect();
        if let [atom] = labeled.as_slice()
            && let Some(text) = &atom.display.variable
        {
            choices.extend(text.chars().zip(0..32u8).map(|(c, index)| {
                let value = LabelAttachment::Character(index);
                AttachmentChoice(value, format!("{value}: {c}"))
            }));
        }
        let selected = labeled
            .iter()
            .all(|a| a.display.label_attachment == current)
            .then(|| {
                choices
                    .iter()
                    .find(|choice| choice.0 == current)
                    .cloned()
                    .unwrap_or_else(|| AttachmentChoice(current, current.to_string()))
            });
        Some(
            crate::appearance::pick_list(choices, selected, |choice: AttachmentChoice| {
                Message::Labels(Action::Attachment(choice.0))
            })
            .placeholder("Mixed")
            .text_size(12)
            .width(iced::Length::Fill)
            .into(),
        )
    }
}

/// A bond-attachment menu entry: flush sides, then each character of one label.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentChoice(LabelAttachment, String);
impl std::fmt::Display for AttachmentChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}
