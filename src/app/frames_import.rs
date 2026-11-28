//! Choose the job or geometry to open from a calculation or a multi-frame
//! file. An XYZ or SD file can also be kept as a conformer ensemble.
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

mod calculation_stereo;

/// Geometry used when opening a whole job.
pub(super) fn job_step(job: &qcread::Job) -> Option<&qcread::Step> {
    job.result_step().or_else(|| job.steps.last())
}

pub(super) fn result_step_index(steps: &[qcread::Step], result: &CalcResult) -> Option<usize> {
    steps
        .iter()
        .rposition(|step| step.frame.hash() == result.frame)
}

pub(super) fn molecule(frame: &Frame) -> String {
    const DIGITS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
    super::xtb::formula(&frame.numbers)
        .chars()
        .map(|c| {
            c.to_digit(10)
                .and_then(|digit| DIGITS.get(digit as usize))
                .copied()
                .unwrap_or(c)
        })
        .collect()
}

const KCAL_PER_HARTREE: f64 = 627.509_474_063;
const SHEET_WIDTH: f32 = 1100.;
const SHEET_HEIGHT: f32 = 640.;
/// The sheet previews frames up to this many atoms.
const PREVIEW_ATOMS: usize = 500;

/// Which row opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    First,
    Last,
    Lowest,
    /// A job or frame by its index, picked in the list.
    Frame(usize),
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Choose(Choice),
    /// Move the picked frame by this many rows.
    Move(isize),
    Keep(bool),
    Add2d(bool),
    /// Draw the lowest-energy frame faint behind the previewed one.
    Overlay(bool),
    /// Turn the preview by a drag of this many pixels.
    Turn(f32, f32),
    Import,
    Close,
    /// Ask about the next file opened together with the last one.
    Next,
}

/// A file with a choice of jobs or frames, read.
#[derive(Clone, Debug)]
pub(crate) struct File {
    name: String,
    file: QcFile,
    /// The source hash for an XYZ or SD file if it is kept as an ensemble.
    sha256: Option<String>,
    /// The SD file's text, to open the chosen record with its own chemistry.
    sdf: Option<String>,
    /// Each step's energy, found once: a step without its own energy is
    /// matched to the result by hashing geometries.
    step_energies: std::sync::OnceLock<Vec<Option<f64>>>,
}

/// The open "Choose what to import" sheet.
pub(crate) struct Dialog {
    path: PathBuf,
    file: File,
    choice: Choice,
    keep: bool,
    add_2d: bool,
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
        sha256: Some(sha256),
        sdf,
        step_energies: Default::default(),
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
    pub(super) fn calculation(name: String, file: QcFile) -> Self {
        Self {
            name,
            file,
            sha256: None,
            sdf: None,
            step_energies: Default::default(),
        }
    }

    fn calculation_output(&self) -> bool {
        self.file.format.has_results()
    }

    fn job_rows(&self) -> bool {
        self.file.jobs.len() > 1
    }

    fn count(&self) -> usize {
        if self.job_rows() {
            self.file.jobs.len()
        } else {
            self.steps().len()
        }
    }

    fn step(&self, index: usize) -> Option<&qcread::Step> {
        if self.job_rows() {
            job_step(self.file.jobs.get(index)?)
        } else {
            self.steps().get(index)
        }
    }

    fn result_step_index(&self) -> Option<usize> {
        let [job] = self.file.jobs.as_slice() else {
            return None;
        };
        let result = job
            .result
            .as_ref()
            .filter(|result| super::files::has_calculated_results(result))?;
        result_step_index(&job.steps, result)
    }

    fn subtitle(&self) -> String {
        let name = &self.name;
        let format = self.file.format.name();
        let program = self.file.program.as_ref().map(|program| {
            super::files::program_label(program, self.file.program_version.as_deref())
        });
        let count = self.count();
        if self.calculation_output() {
            let kind = if self.job_rows() { "jobs" } else { "steps" };
            format!(
                "{name} · {} · {count} {kind}",
                program.as_deref().unwrap_or(format)
            )
        } else {
            let atoms = self
                .steps()
                .first()
                .map_or(0, |step| step.frame.numbers.len());
            let mut subtitle = format!("{name} · {format} · {count} frames · {atoms} atoms");
            if let Some(program) = program {
                subtitle.push_str(&format!(" · written by {program}"));
            }
            subtitle
        }
    }

    fn steps(&self) -> &[qcread::Step] {
        if self.job_rows() {
            return &[];
        }
        self.file
            .jobs
            .first()
            .map_or(&[], |job| job.steps.as_slice())
    }

    fn step_energy(&self, step: &qcread::Step) -> Option<f64> {
        step.energy_hartree.or_else(|| {
            let job = self.file.jobs.first()?;
            let result = job.result.as_ref()?;
            // Gaussian can repeat the converged geometry after its last SCF
            // energy. Both rows describe the same evaluated structure.
            (step.frame.hash() == result.frame).then_some(())?;
            job.result_step()
                .and_then(|step| step.energy_hartree)
                .or(result.energy_hartree)
        })
    }

    /// Each step's energy, in step order.
    fn step_energies(&self) -> &[Option<f64>] {
        self.step_energies
            .get_or_init(|| self.steps().iter().map(|s| self.step_energy(s)).collect())
    }

    /// Every frame's energy, when the file gives one for each.
    fn energies(&self) -> Option<Vec<f64>> {
        if self.job_rows() {
            return None;
        }
        self.step_energies().iter().copied().collect()
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
        Some(
            if !self.calculation_output() && (self.sdf.is_some() || sorted) {
                Layout::Levels
            } else {
                Layout::Line
            },
        )
    }
}

impl Dialog {
    fn new(path: PathBuf, file: File) -> Self {
        let ensemble = file.layout() == Some(Layout::Levels);
        // Opening an SD record preserves its chemistry unless keeping the ensemble is requested.
        let keep = ensemble && file.file.format == Format::Xyz;
        let choice = if file.job_rows() {
            Choice::Frame(file.file.default_job().unwrap_or(0))
        } else if ensemble {
            Choice::Lowest
        } else if let Some(step) = file.result_step_index().filter(|&i| i + 1 < file.count()) {
            Choice::Frame(step)
        } else {
            Choice::Last
        };
        let add_2d = file.calculation_output();
        let mut dialog = Self {
            path,
            file,
            choice,
            keep,
            add_2d,
            overlay: false,
            preview: None,
        };
        dialog.refresh();
        dialog
    }

    /// Build the preview of the chosen frame, keeping the view's turn, with
    /// the lowest-energy frame superposed behind it when asked.
    fn refresh(&mut self) {
        let Some(step) = self.file.step(self.index()) else {
            self.preview = None;
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
            .overlay
            .then(|| self.file.lowest())
            .flatten()
            .and_then(|i| self.file.step(i));
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

    /// The row `choice` names.
    fn index(&self) -> usize {
        self.choice_index(self.choice)
    }

    fn choice_index(&self, choice: Choice) -> usize {
        let last = self.file.count().saturating_sub(1);
        match choice {
            Choice::First => 0,
            Choice::Last => last,
            Choice::Lowest => self.file.lowest().unwrap_or(0),
            Choice::Frame(index) => index.min(last),
        }
    }
}

impl App {
    pub(super) fn ask_frames_import(&mut self, path: PathBuf, file: File) {
        let kind = if file.job_rows() {
            "jobs"
        } else if file.calculation_output() {
            "steps"
        } else {
            "frames"
        };
        self.status = format!("{} has several {kind}. Choose what to open.", file.name);
        self.error = false;
        self.frames_import = Some(Dialog::new(path, file));
    }

    pub(super) fn frames_import_action(&mut self, action: Action) -> Task<Message> {
        let Some(dialog) = self.frames_import.as_mut() else {
            if matches!(action, Action::Next)
                && let Some((path, file)) = self.frames_queue.pop_front()
            {
                self.ask_frames_import(path, file);
            }
            return Task::none();
        };
        match action {
            // A sheet is up: the next file asks once it is answered.
            Action::Next => {}
            Action::Choose(Choice::Lowest) if dialog.file.energies().is_none() => {}
            Action::Choose(choice) => {
                let before = dialog.index();
                let index = dialog.choice_index(choice);
                if dialog.file.step(index).is_some() {
                    dialog.choice = choice;
                    if index != before {
                        dialog.refresh();
                    }
                }
            }
            Action::Move(step) => {
                let count = dialog.file.count();
                let before = dialog.index();
                let target = (before as isize + step).clamp(0, count as isize - 1) as usize;
                let available = |&i: &usize| dialog.file.step(i).is_some();
                let next = if step < 0 {
                    (0..=target).rev().find(available)
                } else {
                    (target..count).find(available)
                };
                if let Some(index) = next.filter(|index| *index != before) {
                    dialog.choice = Choice::Frame(index);
                    dialog.refresh();
                }
            }
            Action::Keep(keep) => {
                dialog.keep =
                    keep && !dialog.file.calculation_output() && dialog.file.energies().is_some();
            }
            Action::Add2d(add) => dialog.add_2d = add,
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
        let path = dialog.path;
        if dialog.file.calculation_output() {
            let (job, step) = if dialog.file.job_rows() {
                (index, None)
            } else {
                (0, Some(index))
            };
            let file = dialog.file.file;
            let add_2d = dialog.add_2d;
            return Task::perform(
                async move {
                    let result = reshiki::task::spawn_blocking(move || {
                        super::files::prepare_qc_drawing(file, job, step, add_2d)
                    })
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result);
                    Some((path, result))
                },
                Message::FilePrepared,
            );
        }
        let count = dialog.file.count();
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
        let job_rows = file.job_rows();
        let frame_name = if file.calculation_output() {
            "Step"
        } else {
            "Frame"
        };
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
                    "or pick a row below"
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
                cell(frame_name.into(), Length::Fixed(40.)),
                cell("Comment".into(), Length::Fill),
                cell("Energy (Eh)".into(), Length::Fixed(120.)),
                cell("ΔE (kcal/mol)".into(), Length::Fixed(100.)),
            ]
            .spacing(10),
        )
        .padding([2, 8]);
        let result_step = file.result_step_index();
        let rows = Column::with_children(steps.iter().enumerate().map(|(i, step)| {
            let active = i == index;
            let has_results = result_step == Some(i);
            let comment = step_comment(step, has_results);
            let label = if has_results {
                format!("{frame_name} {} · Results", i + 1)
            } else {
                format!("{frame_name} {}", i + 1)
            };
            let energy = file.step_energies().get(i).copied().flatten();
            let delta = energy
                .zip(lowest)
                .map(|(e, low)| (e - low) * KCAL_PER_HARTREE);
            reshiki::accessibility::button(
                format!("frames-row-{i}"),
                label,
                row![
                    cell((i + 1).to_string(), Length::Fixed(40.)),
                    cell(comment, Length::Fill),
                    cell(eh(energy, file.calculation_output()), Length::Fixed(120.)),
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
        let list = if job_rows {
            job_list(file, index)
        } else {
            column![
                header,
                scrollable(rows).id("frames-import-list").height(280)
            ]
            .spacing(2)
            .into()
        };

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
        let message = if job_rows {
            let result_at_earlier_geometry = file
                .file
                .jobs
                .get(index)
                .and_then(|job| job.result.as_ref().zip(job.steps.last()))
                .is_some_and(|(result, last)| result.frame != last.frame.hash());
            let geometry = if result_at_earlier_geometry {
                " at the geometry with results"
            } else {
                ""
            };
            format!(
                "Job {} of {} opens{geometry}. ⌘Return imports.",
                index + 1,
                file.count()
            )
        } else {
            format!(
                "{frame_name} {} of {} opens{}. ⌘Return imports.",
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
            )
        };
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
                    unit: crate::appearance::EnergyUnit::default(),
                    delta_kcal: energies
                        .iter()
                        .map(|e| (e - low) * KCAL_PER_HARTREE)
                        .collect(),
                    layout,
                    selected: Some(index),
                    notes: vec![],
                    select: Some(|i| Message::FramesImport(Action::Choose(Choice::Frame(i)))),
                    steps: vec![],
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
        let mut side = column![
            preview,
            text("Drag to turn the model.").size(11).style(muted_text),
        ]
        .spacing(8)
        .width(330);
        if !job_rows {
            side = side.push(overlay).push(diagram);
        }
        let mut main = column![].spacing(12).width(Length::Fill);
        if !job_rows {
            main = main.push(quick);
        }
        let options: Element<'_, Message> = if file.calculation_output() {
            accessible_checkbox(
                "frames-add-2d",
                "Add 2D structure",
                dialog.add_2d,
                Some(send(Action::Add2d(!dialog.add_2d))),
            )
            .into()
        } else {
            column![keep, text(keep_note).size(11).style(muted_text)]
                .spacing(4)
                .into()
        };
        main = main.push(list).push(options);
        let mut content = column![
            column![
                text("Choose what to import").size(15),
                text(file.subtitle()).size(12).style(muted_text),
            ]
            .spacing(2),
            row![main, side].spacing(18),
        ]
        .spacing(12);
        for warning in &file.file.warnings {
            content = content.push(text(warning.clone()).size(11).style(muted_text));
        }
        modal(
            base,
            sheet_content(content.into(), footer.into()),
            SHEET_WIDTH,
            SHEET_HEIGHT,
            Placement::Fit,
            send(Action::Close),
        )
    }
}

/// Reserve room for both actions even when the table and warnings overflow.
fn sheet_content<'a, R: iced::advanced::text::Renderer + 'a>(
    body: Element<'a, Message, iced::Theme, R>,
    footer: Element<'a, Message, iced::Theme, R>,
) -> Element<'a, Message, iced::Theme, R> {
    container(column![scrollable(body).height(Length::Fill), footer].spacing(12))
        .padding(16)
        .into()
}

fn eh(energy: Option<f64>, calculation: bool) -> String {
    let decimals = if calculation { 9 } else { 6 };
    energy.map_or_else(|| "—".into(), |e| minus(format!("{e:.decimals$}")))
}

fn step_comment(step: &qcread::Step, has_results: bool) -> String {
    let comment: String = step
        .label
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(60)
        .collect();
    if has_results && comment.is_empty() {
        "Results".into()
    } else if has_results {
        format!("Results · {comment}")
    } else {
        comment
    }
}

fn job_cells(job: &qcread::Job) -> [String; 5] {
    let step = job_step(job);
    let energy = job
        .result
        .as_ref()
        .and_then(|result| result.energy_hartree)
        .or_else(|| step.and_then(|step| step.energy_hartree));
    [
        (job.index + 1).to_string(),
        step.map_or_else(|| "—".into(), |step| molecule(&step.frame)),
        job.title(),
        job.completion
            .as_ref()
            .map_or("—", qcread::Completion::name)
            .into(),
        eh(energy, true),
    ]
}

fn job_list(file: &File, selected: usize) -> Element<'static, Message> {
    let columns = |cells: [String; 5]| {
        row(cells
            .into_iter()
            .zip([
                Length::Fixed(38.),
                Length::Fixed(78.),
                Length::Fill,
                Length::Fixed(118.),
                Length::Fixed(116.),
            ])
            .map(|(cell, width)| text(cell).size(12).width(width).into()))
        .spacing(8)
        .align_y(Alignment::Center)
    };
    let header = container(columns(
        ["Job", "Molecule", "Type", "Ended", "Energy (Eh)"].map(str::to_owned),
    ))
    .padding([2, 8]);
    let rows = Column::with_children(file.file.jobs.iter().enumerate().map(|(i, job)| {
        let cells = job_cells(job);
        let label = format!(
            "Job {} · {} · {} · {}",
            cells[0], cells[1], cells[2], cells[3]
        );
        let active = i == selected;
        reshiki::accessibility::button(format!("frames-row-{i}"), label, columns(cells))
            .padding([3, 8])
            .width(Length::Fill)
            .checked(active)
            .style(control(active))
            .on_press_maybe(
                (!job.steps.is_empty())
                    .then_some(Message::FramesImport(Action::Choose(Choice::Frame(i)))),
            )
            .into()
    }))
    .spacing(1);
    column![
        header,
        scrollable(rows).id("frames-import-list").height(280)
    ]
    .spacing(2)
    .into()
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
pub(super) fn drawing(frame: &Frame) -> Result<(reshiki::document::Document, Vec<String>), String> {
    let mut xyz = format!("{}\n\n", frame.numbers.len());
    for (number, [x, y, z]) in frame.numbers.iter().zip(&frame.positions_angstrom) {
        xyz.push_str(&format!("{number} {x} {y} {z}\n"));
    }
    let (mut doc, warnings) = reshiki::crystal::read("xyz", &xyz).map_err(|e| e.to_string())?;
    if let Some(model) = doc
        .models
        .first()
        .filter(|m| m.atoms.len() == frame.numbers.len())
    {
        let isotopes: Vec<_> = model
            .atoms
            .iter()
            .zip(&frame.isotopes)
            .map(|(atom, isotope)| (atom.id, *isotope))
            .collect();
        for (id, isotope) in isotopes {
            if let Some(atom) = doc.atom_mut(id) {
                atom.isotope = u32::from(isotope);
            }
        }
    }
    Ok((doc, warnings))
}

/// Keep the XYZ reader's perceived bonds, but fold ordinary bonded hydrogens
/// into atom labels and keep their original 3D positions.
pub(super) fn fold_calculation_hydrogens(doc: &mut reshiki::document::Document, frame: &Frame) {
    use reshiki::chemistry::{document::drawing_graph, hydrogens};
    use std::collections::{HashMap, HashSet};

    let [model] = doc.models.as_slice() else {
        return;
    };
    if model.atoms.len() != frame.numbers.len() {
        // The XYZ reader can leave overlapping sites out.
        return;
    }
    // Dummy attachment nodes are only a topology proxy for RemoveHs and
    // stereo retention here; no identifier or valence claim is made from it.
    let proxy = if reshiki::attachments::present(doc) {
        reshiki::attachments::interchange_graph(doc).ok()
    } else {
        None
    };
    let Ok(graph) = drawing_graph(proxy.as_ref().unwrap_or(doc)) else {
        return;
    };
    let protected_atoms =
        graph
            .bonds
            .iter()
            .filter(|bond| {
                bond.order != 1
                    || [bond.a, bond.b].into_iter().any(|i| {
                        graph.atoms.get(i).is_some_and(|atom| {
                            reshiki_geometry::is_metal_atomic_number(u32::from(atom.atomic_number))
                        })
                    })
            })
            .flat_map(|bond| [bond.a, bond.b])
            .chain(
                graph.atoms.iter().enumerate().filter_map(|(i, atom)| {
                    (atom.atomic_number == 1 && atom.charge != 0).then_some(i)
                }),
            )
            .collect();
    let mut positions = frame.positions_angstrom.clone();
    let indices: HashMap<_, _> = model
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    for anchor in doc.atoms.iter().skip(positions.len()) {
        let Some(members) = anchor
            .centroid
            .iter()
            .map(|id| frame.positions_angstrom.get(*indices.get(id)?).copied())
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        if members.is_empty() {
            return;
        }
        let sum = members
            .iter()
            .copied()
            .fold([0.; 3], |[a, b, c], [x, y, z]| [a + x, b + y, c + z]);
        positions.push(sum.map(|value| value / members.len() as f64));
    }
    let stereo = calculation_stereo::perceive(&graph, &positions);
    let Ok(folded) = hydrogens::remove(&hydrogens::Input {
        metadata: stereo.metadata,
        directions: stereo.directions,
        unknown_atoms: vec![false; graph.atoms.len()],
        graph,
        annotations: hydrogens::Annotations {
            protected_atoms,
            ..Default::default()
        },
    }) else {
        return;
    };
    let kept = folded.kept_atoms.len();
    let removed = doc.atoms.len() - kept;
    if removed > kept * 4 {
        return;
    }
    let kept_ids: HashSet<_> = folded
        .kept_atoms
        .iter()
        .filter_map(|&i| doc.atoms.get(i).map(|atom| atom.id))
        .collect();
    let parents: HashMap<_, _> = doc
        .bonds
        .iter()
        .filter_map(
            |bond| match (kept_ids.contains(&bond.a), kept_ids.contains(&bond.b)) {
                (true, false) => Some((bond.b, bond.a)),
                (false, true) => Some((bond.a, bond.b)),
                _ => None,
            },
        )
        .collect();
    let sources = model.frame_sources();
    let current_attachments: HashSet<_> = doc
        .science
        .attachments
        .iter()
        .filter(|a| a.atoms == sources && a.freshness(doc) == reshiki::science::Freshness::Current)
        .map(|a| a.id)
        .collect();
    doc.atoms.retain(|atom| kept_ids.contains(&atom.id));
    for (atom, chemistry) in doc.atoms.iter_mut().zip(&folded.graph.atoms) {
        atom.explicit_h = u32::from(chemistry.explicit_hydrogens);
        atom.label_h = atom.explicit_h;
    }
    doc.bonds
        .retain(|b| kept_ids.contains(&b.a) && kept_ids.contains(&b.b));
    if removed > 0
        && let Some(mut model) = doc.models.pop()
    {
        model.fold_file_hydrogens(doc, &parents);
        doc.models.push(model);
    }
    calculation_stereo::draw(doc, &folded.graph, &folded.metadata);
    if !current_attachments.is_empty() {
        let ids: Vec<_> = doc
            .models
            .first()
            .into_iter()
            .flat_map(|m| m.frame_sources())
            .flatten()
            .collect();
        let baseline = reshiki::science::baseline(doc, &ids);
        for attachment in &mut doc.science.attachments {
            if current_attachments.contains(&attachment.id) {
                for atom in &mut attachment.atoms {
                    if atom.is_some_and(|id| parents.contains_key(&id)) {
                        *atom = None;
                    }
                }
                attachment.baseline.clone_from(&baseline);
            }
        }
    }
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
    let mut launch = reshiki::science::launch(doc, first, chosen.charge, chosen.multiplicity)
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
        return Err("the 3D model does not match the chosen frame".into());
    }
    launch.frame = std::sync::Arc::new(chosen.clone());
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
    let lowest = lowest_step.frame.clone();
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
        rmsd_to_best_angstrom: vec![],
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
                file_sha256: file.sha256.clone(),
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
