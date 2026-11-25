//! Opening a file that holds several geometries of one molecule (a
//! multi-frame XYZ such as a trajectory or a CREST ensemble, or an SD file
//! of conformers): choose the first, the last, the lowest-energy or any
//! frame, and optionally keep every frame as an ensemble in the results
//! palette's Conformers tab.
use super::energy_plot::{EnergyPlot, Layout};
use super::{
    App, Message,
    files::Prepared,
    help::{Placement, modal},
    workspace::{accessible_checkbox, accessible_command, control, muted_text},
};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::canvas;
use iced::widget::{Column, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Task, mouse};
use reshiki::qcdata::{
    self, CalcResult, Frame, MAX_SAVED_CONFORMERS, Provenance, Source, Status, Trajectory,
};
use reshiki::qcread::{self, Format, QcFile};
use std::path::PathBuf;

const KCAL_PER_HARTREE: f64 = 627.509_474_063;
/// The sheet previews frames up to this many atoms.
const PREVIEW_ATOMS: usize = 500;

/// Which frame opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    First,
    Last,
    Lowest,
    /// A frame by its index, picked in the list.
    Frame(usize),
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Choose(Choice),
    /// Move the picked frame by this many rows.
    Move(isize),
    Keep(bool),
    /// Draw the lowest-energy frame faint behind the previewed one.
    Overlay(bool),
    /// Turn the preview by a drag of this many pixels.
    Turn(f32, f32),
    Import,
    Close,
    /// Ask about the next file opened together with the last one.
    Next,
}

/// A file of several frames of one molecule, read.
#[derive(Clone, Debug)]
pub(crate) struct File {
    name: String,
    file: QcFile,
    sha256: String,
    /// The SD file's text, to open the chosen record with its own chemistry.
    sdf: Option<String>,
}

/// The open "Choose what to import" sheet.
pub(crate) struct Dialog {
    path: PathBuf,
    file: File,
    choice: Choice,
    keep: bool,
    overlay: bool,
    /// The chosen frame as a 3D model, turned as the user turned it.
    preview: Option<reshiki::document::Document>,
}

/// A multi-frame XYZ, or an SD file of 3D conformers (every record the
/// same atoms, charge, spin and title); `None` for anything else, which
/// opens as before: a single frame, a library of molecules, 2D records, a
/// periodic structure, or a file read only in part.
pub(super) fn detect(path: &std::path::Path, contents: &[u8]) -> Option<File> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(extension.as_str(), "xyz" | "extxyz" | "sdf")
        || contents.len() as u64 > super::import::limit(path)
    {
        return None;
    }
    let name = super::import::name(path);
    let file = qcread::read(&name, contents).ok()?;
    let [job] = file.jobs.as_slice() else {
        return None;
    };
    let first = job.steps.first()?;
    let partial = file.warnings.iter().any(|warning| {
        warning.starts_with("Read the first")
            || warning.starts_with("The cell is ignored")
            || warning.ends_with("is 2D (all z = 0)")
    });
    let sdf = file.format == Format::Sdf;
    if partial
        || job.steps.len() < 2
        || job.steps.iter().any(|step| {
            let (a, b) = (&step.frame, &first.frame);
            a.numbers != b.numbers
                || a.isotopes != b.isotopes
                || a.charge != b.charge
                || a.multiplicity != b.multiplicity
                // Records of different molecules can share an atom list;
                // conformer files repeat the molecule's name.
                || sdf && molecule_name(&step.label) != molecule_name(&first.label)
        })
    {
        return None;
    }
    use sha2::Digest;
    let sha256 = sha2::Sha256::digest(contents)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let sdf = sdf.then(|| String::from_utf8_lossy(contents).into_owned());
    Some(File {
        name,
        file,
        sha256,
        sdf,
    })
}

/// Esc cancels, ⌘Return imports (Return stays with the focused control),
/// ↑/↓ pick the frame above or below.
pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    let action = match key {
        Key::Named(Named::Enter) if modifiers.command() => Action::Import,
        _ if !modifiers.is_empty() => return None,
        Key::Named(Named::Escape) => Action::Close,
        Key::Named(Named::ArrowUp) => Action::Move(-1),
        Key::Named(Named::ArrowDown) => Action::Move(1),
        _ => return None,
    };
    Some(Message::FramesImport(action))
}

impl File {
    fn steps(&self) -> &[qcread::Step] {
        self.file
            .jobs
            .first()
            .map_or(&[], |job| job.steps.as_slice())
    }

    /// Every frame's energy, when the file gives one for each.
    fn energies(&self) -> Option<Vec<f64>> {
        self.steps().iter().map(|s| s.energy_hartree).collect()
    }

    /// The lowest-energy frame.
    fn lowest(&self) -> Option<usize> {
        self.energies()?
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
    }

    /// How the energy diagram draws this file, `None` without energies: an
    /// SD file or frames already sorted by energy (CREST, ReShiki's export)
    /// are an ensemble of levels; frames in any other order are an ordered
    /// trajectory (an optimization, a scan, MD), drawn against frame.
    fn layout(&self) -> Option<Layout> {
        let energies = self.energies()?;
        let sorted = energies.windows(2).all(|pair| pair.first() <= pair.get(1));
        Some(if self.sdf.is_some() || sorted {
            Layout::Levels
        } else {
            Layout::Line
        })
    }
}

impl Dialog {
    pub(super) fn new(path: PathBuf, file: File) -> Self {
        // An ensemble opens its lowest member; a trajectory its last frame.
        let choice = match file.layout() {
            Some(Layout::Levels) => Choice::Lowest,
            _ => Choice::Last,
        };
        let mut dialog = Self {
            path,
            file,
            choice,
            keep: false,
            overlay: false,
            preview: None,
        };
        dialog.refresh();
        dialog
    }

    /// Build the preview of the chosen frame, keeping the view's turn, with
    /// the lowest-energy frame superposed behind it when asked.
    fn refresh(&mut self) {
        let index = self.index();
        let steps = self.file.steps();
        let Some(step) = steps.get(index) else {
            return;
        };
        // Bonds are perceived for the preview; large frames open without one.
        if step.frame.numbers.len() > PREVIEW_ATOMS {
            self.preview = None;
            return;
        }
        let rotation = self
            .preview
            .as_ref()
            .and_then(|doc| doc.models.first())
            .map(|model| model.rotation);
        let Ok((mut doc, _)) = drawing(&step.frame) else {
            self.preview = None;
            return;
        };
        let lowest = self
            .file
            .lowest()
            .and_then(|i| steps.get(i))
            .filter(|_| self.overlay);
        if let Some(model) = doc.models.first_mut() {
            if let Some(rotation) = rotation {
                model.rotation = rotation;
            }
            if let Some(lowest) = lowest.filter(|lowest| !std::ptr::eq(*lowest, step))
                && let Some(ghost) = super::xtb::superposed(&step.frame, &lowest.frame)
            {
                model.overlay = vec![ghost.iter().map(|p| p.map(|x| x as f32)).collect()];
            }
        }
        self.preview = Some(doc);
    }

    /// The frame `choice` names.
    fn index(&self) -> usize {
        let last = self.file.steps().len().saturating_sub(1);
        match self.choice {
            Choice::First => 0,
            Choice::Last => last,
            Choice::Lowest => self.file.lowest().unwrap_or(0),
            Choice::Frame(index) => index.min(last),
        }
    }
}

impl App {
    pub(super) fn frames_import_action(&mut self, action: Action) -> Task<Message> {
        let Some(dialog) = self.frames_import.as_mut() else {
            if matches!(action, Action::Next)
                && let Some((path, file)) = self.frames_queue.pop_front()
            {
                self.status = format!(
                    "{} has several frames: choose which to open",
                    super::import::name(&path)
                );
                self.error = false;
                self.frames_import = Some(Dialog::new(path, file));
            }
            return Task::none();
        };
        match action {
            // A sheet is up: the next file asks once it is answered.
            Action::Next => {}
            Action::Choose(Choice::Lowest) if dialog.file.energies().is_none() => {}
            Action::Choose(choice) => {
                let before = dialog.index();
                dialog.choice = choice;
                if dialog.index() != before {
                    dialog.refresh();
                }
            }
            Action::Move(step) => {
                let count = dialog.file.steps().len() as isize;
                let before = dialog.index();
                let index = (before as isize + step).clamp(0, count - 1) as usize;
                dialog.choice = Choice::Frame(index);
                if index != before {
                    dialog.refresh();
                }
            }
            Action::Keep(keep) => dialog.keep = keep && dialog.file.energies().is_some(),
            Action::Overlay(on) => {
                dialog.overlay = on;
                dialog.refresh();
            }
            Action::Turn(dx, dy) => {
                if let Some(model) = dialog
                    .preview
                    .as_mut()
                    .and_then(|doc| doc.models.first_mut())
                {
                    model.rotation = reshiki::models3d::turn(model.rotation, dx, dy, false);
                }
            }
            Action::Close => {
                self.frames_import = None;
                self.status = "Nothing opened".into();
                self.error = false;
                // Keyboard focus goes back to the drawing, then the next file asks.
                return iced::advanced::widget::operate(super::keyboard_drawing::ReleaseFocus)
                    .chain(Task::done(Message::FramesImport(Action::Next)));
            }
            Action::Import => {
                let Some(dialog) = self.frames_import.take() else {
                    return Task::none();
                };
                // The next file asks once this one has opened (an SD record
                // opens a moment later), so tabs open in the order chosen.
                return self
                    .frames_import_open(dialog)
                    .chain(iced::advanced::widget::operate(
                        super::keyboard_drawing::ReleaseFocus,
                    ))
                    .chain(Task::done(Message::FramesImport(Action::Next)));
            }
        }
        Task::none()
    }

    /// Open the chosen frame: an SD record with its own chemistry, else a 3D
    /// model from the coordinates, with the ensemble kept when asked.
    fn frames_import_open(&mut self, dialog: Dialog) -> Task<Message> {
        let index = dialog.index();
        let count = dialog.file.steps().len();
        let path = dialog.path.clone();
        if let (Some(text), false) = (&dialog.file.sdf, dialog.keep)
            && let Some(record) = record(text, index)
        {
            let prepared = Ok(Prepared::Import {
                format: "sdf",
                contents: record,
                loss: Default::default(),
            });
            return Task::perform(
                async move {
                    let result = super::files::records(&path, prepared).await;
                    Some((path, result))
                },
                Message::FilePrepared,
            );
        }
        let Some(step) = dialog.file.steps().get(index) else {
            return Task::none();
        };
        let (drawing, mut warnings) = match drawing(&step.frame) {
            Ok(opened) => opened,
            Err(error) => {
                self.status = error;
                self.error = true;
                return Task::none();
            }
        };
        let frame = &step.frame;
        let mut omitted = Vec::new();
        if frame.charge != 0 {
            omitted.push(format!("total charge {:+}", frame.charge));
        }
        if frame.multiplicity != 1 {
            omitted.push(format!("spin multiplicity {}", frame.multiplicity));
        }
        let not_drawn = if omitted.is_empty() {
            String::new()
        } else {
            format!("Not drawn: {}", omitted.join(", "))
        };
        if !not_drawn.is_empty() {
            warnings.insert(0, not_drawn.clone());
        }
        warnings.insert(
            0,
            format!(
                "Frame {} of {count} from the {} file",
                index + 1,
                dialog.file.file.format.name()
            ),
        );
        let task = self.file_prepared(Some((
            path,
            Ok(Prepared::Records(Box::new(super::import::Batch::one(
                drawing, warnings,
            )))),
        )));
        if self.error || !dialog.keep {
            return task;
        }
        let result = ensemble(&mut self.tab.doc, &dialog.file, index);
        match result {
            Ok(kept) => {
                self.status = format!(
                    "Frame {} of {count} opened · {kept} kept as an ensemble",
                    index + 1
                );
                if !not_drawn.is_empty() {
                    self.status.push_str(&format!(" · {not_drawn}"));
                }
                self.error = false;
                task.chain(Task::done(Message::Xtb(super::xtb::Action::Palette(
                    super::xtb::palette::Action::Show,
                ))))
            }
            Err(reason) => {
                self.status = format!(
                    "Frame {} of {count} · Ensemble not kept: {reason}",
                    index + 1
                );
                self.error = false;
                task
            }
        }
    }

    pub(super) fn with_frames_import<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(dialog) = &self.frames_import else {
            return base;
        };
        let send = Message::FramesImport;
        let file = &dialog.file;
        let steps = file.steps();
        let energies = file.energies();
        let index = dialog.index();
        let atoms = steps.first().map_or(0, |s| s.frame.numbers.len());
        let mut subtitle = format!(
            "{} · {} · {} frames · {atoms} atoms",
            file.name,
            file.file.format.name(),
            steps.len()
        );
        if let Some(program) = &file.file.program {
            subtitle.push_str(&format!(" · written by {program}"));
        }
        let choices = [
            (Choice::First, "First"),
            (Choice::Last, "Last"),
            (Choice::Lowest, "Lowest energy"),
        ];
        let mut quick = row(choices.map(|(choice, name)| {
            let active = dialog.choice == choice;
            let enabled = choice != Choice::Lowest || energies.is_some();
            reshiki::accessibility::button(
                format!("frames-{}", name.to_lowercase().replace(' ', "-")),
                name,
                text(name).size(12),
            )
            .padding([6, 12])
            .checked(active)
            .style(control(active))
            .on_press_maybe(enabled.then(|| send(Action::Choose(choice))))
            .into()
        }))
        .spacing(4)
        .align_y(Alignment::Center);
        let picked = matches!(dialog.choice, Choice::Frame(_));
        quick = quick.push(
            container(
                text(if picked {
                    "Picked in the list"
                } else {
                    "or pick a frame below"
                })
                .size(12)
                .style(muted_text),
            )
            .padding([0, 8]),
        );

        let lowest = energies
            .as_ref()
            .map(|e| e.iter().copied().fold(f64::INFINITY, f64::min));
        let cell = |content: String, width: Length| text(content).size(12).width(width);
        let header = container(
            row![
                cell("#".into(), Length::Fixed(40.)),
                cell("Comment".into(), Length::Fill),
                cell("Energy (Eh)".into(), Length::Fixed(120.)),
                cell("ΔE (kcal/mol)".into(), Length::Fixed(100.)),
            ]
            .spacing(10),
        )
        .padding([2, 8]);
        let rows = Column::with_children(steps.iter().enumerate().map(|(i, step)| {
            let active = i == index;
            let comment: String = step
                .label
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect();
            let energy = step.energy_hartree;
            let delta = energy
                .zip(lowest)
                .map(|(e, low)| (e - low) * KCAL_PER_HARTREE);
            reshiki::accessibility::button(
                format!("frames-row-{i}"),
                format!("Frame {}", i + 1),
                row![
                    cell((i + 1).to_string(), Length::Fixed(40.)),
                    cell(comment, Length::Fill),
                    cell(
                        energy.map_or_else(|| "—".into(), |e| minus(format!("{e:.6}"))),
                        Length::Fixed(120.)
                    ),
                    cell(
                        delta.map_or_else(|| "—".into(), |d| format!("{d:.2}")),
                        Length::Fixed(100.)
                    ),
                ]
                .spacing(10),
            )
            .padding([3, 8])
            .width(Length::Fill)
            .checked(active)
            .style(control(active))
            .on_press(send(Action::Choose(Choice::Frame(i))))
            .into()
        }))
        .spacing(1);
        let list = scrollable(rows).id("frames-import-list").height(280);

        let keep = accessible_checkbox(
            "frames-keep-ensemble",
            "Keep all frames as an ensemble (Conformers tab)",
            dialog.keep,
            energies.is_some().then(|| send(Action::Keep(!dialog.keep))),
        );
        let keep_note = if energies.is_none() {
            "Needs an energy for every frame (xtb, CREST and ORCA write them)."
        } else if file.sdf.is_some() {
            "Up to 32 frames, the lowest in energy, are kept. With an ensemble the record opens as a 3D model; bonds are perceived from the coordinates."
        } else {
            "Up to 32 frames, the lowest in energy, are kept with the molecule and saved in the file."
        };
        let message = format!(
            "Frame {} of {} opens{}. ⌘Return imports.",
            index + 1,
            steps.len(),
            match (dialog.choice, energies.as_ref()) {
                (Choice::Lowest, _) => " (lowest energy)".to_owned(),
                (_, Some(energies)) => energies
                    .get(index)
                    .zip(lowest)
                    .map(|(energy, low)| {
                        format!(
                            ", {:.2} kcal/mol above the lowest",
                            (energy - low) * KCAL_PER_HARTREE
                        )
                    })
                    .unwrap_or_default(),
                _ => String::new(),
            }
        );
        let footer = row![
            text(message).size(12).style(muted_text).width(Length::Fill),
            accessible_command("frames-cancel", "Cancel", "Cancel").on_press(send(Action::Close)),
            reshiki::accessibility::button("frames-import", "Import", text("Import").size(12))
                .padding([7, 18])
                .style(crate::appearance::primary)
                .on_press(send(Action::Import)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        // Right: the chosen frame in 3D, then the energy diagram.
        let preview: Element<'_, Message> = match &dialog.preview {
            Some(doc) => iced::widget::canvas(FramePreview(doc))
                .width(Length::Fill)
                .height(250)
                .into(),
            None => text(format!(
                "Frames of up to {PREVIEW_ATOMS} atoms are previewed here."
            ))
            .size(12)
            .style(muted_text)
            .into(),
        };
        let lowest_frame = file.lowest();
        let overlay = accessible_checkbox(
            "frames-overlay-lowest",
            "Overlay lowest energy",
            dialog.overlay,
            lowest_frame.map(|_| send(Action::Overlay(!dialog.overlay))),
        );
        let diagram: Element<'_, Message> = match (file.layout(), &energies, lowest) {
            (Some(layout), Some(energies), Some(low)) => column![
                text(match layout {
                    Layout::Levels => "Ensemble: levels sorted by energy",
                    Layout::Line => "Trajectory: energy against frame",
                })
                .size(11)
                .style(muted_text),
                iced::widget::canvas(EnergyPlot {
                    delta_kcal: energies
                        .iter()
                        .map(|e| (e - low) * KCAL_PER_HARTREE)
                        .collect(),
                    layout,
                    selected: Some(index),
                    notes: vec![],
                    select: Some(|i| Message::FramesImport(Action::Choose(Choice::Frame(i)))),
                })
                .width(Length::Fill)
                .height(130),
            ]
            .spacing(4)
            .into(),
            _ => text("No energies in this file, so no energy diagram.")
                .size(11)
                .style(muted_text)
                .into(),
        };
        let side = column![
            preview,
            overlay,
            text("Drag to turn the model.").size(11).style(muted_text),
            diagram,
        ]
        .spacing(8)
        .width(330);
        let main = column![
            quick,
            column![header, list].spacing(2),
            column![keep, text(keep_note).size(11).style(muted_text)].spacing(4),
        ]
        .spacing(12)
        .width(Length::Fill);
        let mut content = column![
            column![
                text("Choose what to import").size(15),
                text(subtitle).size(12).style(muted_text),
            ]
            .spacing(2),
            row![main, side].spacing(18),
        ]
        .spacing(12);
        for warning in &file.file.warnings {
            content = content.push(text(warning.clone()).size(11).style(muted_text));
        }
        content = content.push(footer);
        modal(
            base,
            container(content).padding(16).into(),
            1100.,
            640.,
            Placement::Fit,
            send(Action::Close),
        )
    }
}

/// The chosen frame's 3D model; a drag turns it.
struct FramePreview<'a>(&'a reshiki::document::Document);

impl canvas::Program<Message> for FramePreview<'_> {
    type State = (
        <crate::canvas::TemplateThumbnail<'static> as canvas::Program<Message>>::State,
        Option<iced::Point>,
    );

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let iced::Event::Mouse(event) = event else {
            return None;
        };
        match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                state.1 = Some(cursor.position_in(bounds)?);
                Some(canvas::Action::capture())
            }
            // Each move's own position: one batch can hold several moves.
            mouse::Event::CursorMoved { position } => {
                let last = state.1?;
                let now = *position - (bounds.position() - iced::Point::ORIGIN);
                state.1 = Some(now);
                Some(
                    canvas::Action::publish(Message::FramesImport(Action::Turn(
                        now.x - last.x,
                        now.y - last.y,
                    )))
                    .and_capture(),
                )
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                state.1 = None;
                None
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match (state.1, cursor.is_over(bounds)) {
            (Some(_), _) => mouse::Interaction::Grabbing,
            (None, true) => mouse::Interaction::Grab,
            _ => mouse::Interaction::default(),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        <crate::canvas::TemplateThumbnail<'_> as canvas::Program<Message>>::draw(
            &crate::canvas::TemplateThumbnail(self.0, None),
            &state.0,
            renderer,
            theme,
            bounds,
            cursor,
        )
    }
}

/// A record title without a trailing conformer number, as exports number
/// them: "water conformer 2" and "water conformer 3" name one molecule.
fn molecule_name(label: &Option<String>) -> &str {
    let label = label.as_deref().unwrap_or("").trim();
    let name = label
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();
    if name.len() == label.len() {
        return label;
    }
    let lower = name.to_ascii_lowercase();
    // Only a "conformer N" suffix is dropped: "compound 101" and "compound
    // 102" are two molecules of a library.
    if lower.ends_with("conformer") {
        name[..name.len() - "conformer".len()].trim_end()
    } else {
        label
    }
}

/// "−76.408953" with a real minus sign.
fn minus(text: String) -> String {
    text.replace('-', "−")
}

/// The `index`-th non-empty record of an SD file, with its delimiter.
fn record(text: &str, index: usize) -> Option<String> {
    let mut records = Vec::new();
    let mut current = String::new();
    for line in text.split_inclusive('\n') {
        current.push_str(line);
        if line.starts_with("$$$$") {
            records.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        records.push(current);
    }
    records.retain(|r| !r.trim().is_empty());
    records.into_iter().nth(index)
}

/// A drawing whose 3D model is `frame`, as program outputs open.
fn drawing(frame: &Frame) -> Result<(reshiki::document::Document, Vec<String>), String> {
    let mut xyz = format!("{}\n\n", frame.numbers.len());
    for (number, [x, y, z]) in frame.numbers.iter().zip(&frame.positions_angstrom) {
        xyz.push_str(&format!("{number} {x} {y} {z}\n"));
    }
    reshiki::crystal::read("xyz", &xyz).map_err(|e| e.to_string())
}

/// Keep up to 32 frames of `file`, the lowest in energy first, as an
/// imported conformer result of the molecule just opened from frame
/// `index`. Returns how many were kept.
fn ensemble(
    doc: &mut reshiki::document::Document,
    file: &File,
    index: usize,
) -> Result<usize, String> {
    let steps = file.steps();
    let energies = file
        .energies()
        .ok_or("the file gives no energy for some frames")?;
    let chosen = &steps.get(index).ok_or("no such frame")?.frame;
    let first = doc.atoms.first().ok_or("the file has no atoms")?.id;
    let launch = reshiki::science::launch(doc, first, chosen.charge, chosen.multiplicity)
        .map_err(|e| e.to_string().to_lowercase())?;
    let same = launch.frame.numbers == chosen.numbers
        && launch.frame.isotopes == chosen.isotopes
        && launch
            .frame
            .positions_angstrom
            .iter()
            .zip(&chosen.positions_angstrom)
            .all(|(a, b)| a.map(|x| x as f32) == b.map(|x| x as f32));
    if !same {
        return Err("the 3D model's atoms are not in the file's order".into());
    }
    // (energy, frame number, step), the lowest first.
    let mut kept: Vec<_> = energies
        .iter()
        .copied()
        .zip(steps)
        .enumerate()
        .map(|(i, (energy, step))| (energy, i, step))
        .collect();
    kept.sort_by(|a, b| a.0.total_cmp(&b.0));
    kept.truncate(MAX_SAVED_CONFORMERS);
    let &(lowest_energy, _, lowest_step) = kept.first().ok_or("the file has no frames")?;
    let lowest = Frame {
        positions_angstrom: lowest_step.frame.positions_angstrom.clone(),
        ..(*launch.frame).clone()
    };
    let label = |i: usize, step: &qcread::Step| {
        step.label
            .clone()
            .unwrap_or_else(|| format!("Frame {}", i + 1))
    };
    let trajectory = Trajectory {
        numbers: lowest.numbers.clone(),
        isotopes: lowest.isotopes.clone(),
        masses_amu: lowest.masses_amu.clone(),
        charge: lowest.charge,
        multiplicity: lowest.multiplicity,
        positions_angstrom: kept
            .iter()
            .flat_map(|(_, _, step)| step.frame.positions_angstrom.iter().flatten().copied())
            .collect(),
        energies_hartree: kept.iter().map(|(energy, ..)| *energy).collect(),
        labels: kept.iter().map(|&(_, i, step)| label(i, step)).collect(),
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    };
    let source = &file.file;
    let result = CalcResult {
        frame: lowest.hash(),
        // The frame the 3D model opened from, which the result is attached to.
        input_frame: Some(launch.frame.hash()),
        task: qcdata::Task::Conformers,
        endpoints: None,
        settings: qcdata::Settings::Imported { keywords: vec![] },
        provenance: Provenance {
            program: source
                .program
                .clone()
                .unwrap_or_else(|| format!("{} file", source.format.name())),
            program_version: source.program_version.clone(),
            method: Some(format!(
                "Ensemble of {} frames from {}",
                steps.len(),
                file.name
            )),
            basis: None,
            source: Source::Imported {
                format: source.format.name().into(),
                file_name: file.name.clone(),
                file_sha256: Some(file.sha256.clone()),
                job: 0,
            },
        },
        status: Status::Converged,
        energy_hartree: Some(lowest_energy),
        energy_terms: Default::default(),
        gradient_hartree_per_bohr: vec![],
        hessian_hartree_per_bohr2: vec![],
        charges: vec![],
        dipole_e_bohr: None,
        bond_orders: vec![],
        orbitals: None,
        vibrations: None,
        thermo: None,
        trajectory: Some(trajectory),
        keep_full: false,
        omitted_bytes: None,
        run: None,
    };
    let kept = kept.len();
    reshiki::science::attach(doc, &launch, vec![lowest], result).map_err(|e| e.to_string())?;
    Ok(kept)
}

#[cfg(test)]
mod tests;
