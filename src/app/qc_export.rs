//! Export input for Gaussian, ORCA and GAMESS: a dialog that writes another
//! program's input file from the selected molecule's 3D model. Nothing runs
//! here; the text comes from `reshiki::qc_input`.
use super::{
    App, Message,
    help::{Placement, modal},
    workspace::{control, muted_text},
};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{Space, column, container, rich_text, row, scrollable, span, text};
use iced::{Alignment, Border, Color, Element, Font, Length, Task};
use reshiki::models3d::{Display, ForceField};
use reshiki::qc_input::{self, Geometry, Job, Output, Program, Settings, Template};
use reshiki::qcdata::Frame;
use std::path::PathBuf;

use super::xtb::ts::export::Source;

const ACCENT: Color = Color::from_rgb8(47, 122, 95);
const WARNING: Color = Color::from_rgb8(161, 98, 7);
const ERROR: Color = Color::from_rgb8(185, 28, 28);
const METHOD: &str = "qc-export-method";
static NEXT_DIALOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Method,
    Basis,
    Charge,
    Multiplicity,
    Cores,
    Memory,
    Title,
    Extra,
    IrcPoints,
}

#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Close,
    Program(Program),
    Job(Job),
    Template(Choice),
    Field(Field, String),
    /// Show the name field for a new template.
    NameTemplate,
    TemplateName(String),
    SaveTemplate,
    DeleteTemplate,
    /// Make the molecule's 3D model, shown beside the drawing.
    Generate,
    /// Remake a 3D model the drawing has changed since.
    Update,
    Copy,
    Save,
    /// A save finished for the dialog with this id.
    Saved(u64, Result<Option<PathBuf>, String>),
}

/// A template in the menu, shown by its name.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice(Template);

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.name)
    }
}

/// The open dialog. Numbers stay as typed so a half-typed field is not
/// rewritten; they are read when the file is written.
pub(super) struct Dialog {
    /// Tells this dialog's save receipts from an earlier dialog's.
    id: u64,
    atoms: Vec<u64>,
    /// A TS export uses the saved frames rather than the drawing's model.
    source: Option<Source>,
    name: String,
    program: Program,
    job: Job,
    /// The template the fields started from.
    template: Template,
    method: String,
    basis: String,
    charge: String,
    multiplicity: String,
    cores: String,
    memory: String,
    title: String,
    extra: String,
    irc_points: String,
    /// The new template's name while it is being typed.
    template_name: Option<String>,
    message: String,
}

pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    match key {
        Key::Named(Named::Escape) if modifiers.is_empty() => Some(Message::QcExport(Action::Close)),
        Key::Named(Named::Enter) if modifiers.command() => Some(Message::QcExport(Action::Save)),
        _ => None,
    }
}

/// A file stem from a molecule name: letters, digits, `-` and `_`.
fn stem(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim_matches('_');
    if stem.is_empty() {
        "molecule".into()
    } else {
        stem.into()
    }
}

fn force_field(force_field: ForceField) -> &'static str {
    match force_field {
        ForceField::Mmff94 => "MMFF94",
        ForceField::Mmff94s => "MMFF94s",
        ForceField::Uff => "UFF",
    }
}

/// Lines of the preview with their colour: comments muted, keyword and block
/// lines in the accent colour.
fn line_color(program: Program, line: &str) -> Option<Color> {
    let comment = match program {
        Program::Orca => line.starts_with('#'),
        Program::Gaussian | Program::Gamess => line.starts_with('!'),
        Program::Xyz => false,
    };
    let keyword = match program {
        Program::Gaussian => line.starts_with('%') || line.starts_with('#'),
        Program::Orca => {
            line.starts_with(['!', '%', '*', '$']) || line.trim().eq_ignore_ascii_case("end")
        }
        Program::Gamess => line.starts_with(" $"),
        Program::Xyz => false,
    };
    if comment {
        Some(super::workspace::muted())
    } else if keyword {
        Some(ACCENT)
    } else {
        None
    }
}

impl Dialog {
    pub(super) fn new(atoms: Vec<u64>, name: String, charge: i32, multiplicity: u32) -> Self {
        let [default, ..] = qc_input::builtin_templates();
        let mut dialog = Self {
            id: NEXT_DIALOG.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            atoms,
            source: None,
            title: name.clone(),
            name,
            program: Program::Gaussian,
            job: Job::OptFreq,
            template: default.clone(),
            method: String::new(),
            basis: String::new(),
            charge: charge.to_string(),
            multiplicity: multiplicity.to_string(),
            cores: String::new(),
            memory: String::new(),
            extra: String::new(),
            irc_points: "30".into(),
            template_name: None,
            message: String::new(),
        };
        dialog.apply(default);
        dialog
    }

    pub(super) fn with_source(mut self, source: Source, job: Job) -> Self {
        self.source = Some(source);
        self.job = job;
        self
    }

    fn apply(&mut self, template: Template) {
        self.method = template.method.clone();
        self.basis = template.basis.clone();
        self.job = template.job;
        self.cores = template.cores.to_string();
        self.memory = template.memory_gb.to_string();
        self.extra = template.extra.clone();
        self.template = template;
    }

    fn field(&mut self, field: Field) -> &mut String {
        match field {
            Field::Method => &mut self.method,
            Field::Basis => &mut self.basis,
            Field::Charge => &mut self.charge,
            Field::Multiplicity => &mut self.multiplicity,
            Field::Cores => &mut self.cores,
            Field::Memory => &mut self.memory,
            Field::Title => &mut self.title,
            Field::Extra => &mut self.extra,
            Field::IrcPoints => &mut self.irc_points,
        }
    }

    fn settings(&self) -> Result<Settings, String> {
        // XYZ disables the number fields and writes none of them.
        let xyz = self.program == Program::Xyz;
        let number = |text: &str, what: &str| -> Result<u32, String> {
            match text.trim().parse() {
                Ok(value) => Ok(value),
                Err(_) if xyz => Ok(1),
                Err(_) => Err(format!("{what} must be a whole number.")),
            }
        };
        let charge = match self.charge.trim().parse() {
            Ok(charge) => charge,
            Err(_) if xyz => 0,
            Err(_) => return Err("Charge must be a whole number.".into()),
        };
        Ok(Settings {
            program: self.program,
            job: self.job,
            method: self.method.clone(),
            basis: self.basis.clone(),
            charge,
            multiplicity: number(&self.multiplicity, "Multiplicity")?,
            cores: number(&self.cores, "Cores")?,
            memory_gb: number(&self.memory, "Memory")?,
            title: self.title.clone(),
            extra: self.extra.clone(),
            // A hidden field never blocks the file.
            irc_points: if self.job == Job::Irc {
                number(&self.irc_points, "IRC points")?
            } else {
                30
            },
            neb_images: 12,
        })
    }

    fn file_name(&self) -> String {
        format!("{}.{}", stem(&self.name), self.program.extension())
    }
}

/// What the molecule's 3D model gives the dialog.
enum Model {
    Ready {
        frame: Frame,
        label: String,
        stale: bool,
    },
    Generating,
    Missing,
    Unsupported(String),
}

impl App {
    fn input_templates(&self) -> Vec<Template> {
        qc_input::builtin_templates()
            .into_iter()
            .chain(self.appearance.input_templates.iter().cloned())
            .collect()
    }

    /// The drawn molecule of the selection, or of the drawing's only 3D model
    /// when nothing is selected. Its charge and spin come from the drawing as
    /// it is now, even when the model is older.
    fn export_molecule(&self) -> Option<Vec<u64>> {
        let doc = &self.tab.doc;
        let seed = if self.tab.selected.is_empty() {
            match doc.models.as_slice() {
                [model] => model.linked_ids(doc).first().copied(),
                _ => None,
            }
        } else {
            reshiki::models3d::selected_molecule(doc, &self.tab.selected)?
                .first()
                .copied()
        }?;
        // A model can join several drawn molecules (ions read from a file):
        // its atoms all go into the file, so they all count for the charge.
        let mut atoms = reshiki::molecule_names::component(doc, seed);
        if let Some(model) = doc.models.iter().find(|model| model.contains(seed)) {
            for id in model.linked_ids(doc) {
                if !atoms.contains(&id) {
                    atoms.push(id);
                }
            }
        }
        Some(atoms)
    }

    fn open_qc_export(&mut self) -> Task<Message> {
        let Some(atoms) = self.export_molecule() else {
            self.status = "Select one molecule to export as calculation input.".into();
            self.error = true;
            return Task::none();
        };
        let doc = &self.tab.doc;
        let (charge, unpaired) = doc
            .atoms
            .iter()
            .filter(|atom| atoms.contains(&atom.id))
            .fold((0, 0u32), |(charge, unpaired), atom| {
                let radicals = if atom.singlet {
                    0
                } else {
                    u32::from(atom.radical_electrons)
                };
                (charge + atom.charge, unpaired + radicals)
            });
        let name = reshiki::molecule_names::label(doc, &atoms)
            .map(|name| name.text.clone())
            .or_else(|| self.export_stem())
            .unwrap_or_else(|| "molecule".into());
        let dialog = Dialog::new(atoms, name, charge, unpaired + 1);
        self.show_qc_export(dialog)
    }

    pub(super) fn show_qc_export(&mut self, dialog: Dialog) -> Task<Message> {
        self.qc_export = Some(dialog);
        self.context_menu = None;
        self.palette = None;
        iced::widget::operation::focus(METHOD)
    }

    pub(super) fn qc_export_action(&mut self, action: Action) -> Task<Message> {
        if let Action::Open = action {
            return self.open_qc_export();
        }
        if let Action::Saved(id, result) = action {
            // A receipt for an earlier dialog only reports in the status bar.
            let ours = self
                .qc_export
                .as_ref()
                .is_some_and(|dialog| dialog.id == id);
            match result {
                Ok(None) => return Task::none(),
                Err(error) if ours => {
                    if let Some(dialog) = &mut self.qc_export {
                        dialog.message = error;
                    }
                    return Task::none();
                }
                Ok(Some(path)) => {
                    (self.status, self.error) = (format!("Saved {}", path.display()), false)
                }
                Err(error) => (self.status, self.error) = (error, true),
            }
            if !ours {
                return Task::none();
            }
            self.qc_export = None;
            return iced::advanced::widget::operate(super::keyboard_drawing::ReleaseFocus);
        }
        let output = self.qc_output();
        let [default, ..] = qc_input::builtin_templates();
        let Some(dialog) = &mut self.qc_export else {
            return Task::none();
        };
        dialog.message.clear();
        match action {
            Action::Open | Action::Saved(..) => {}
            Action::Close => {
                self.qc_export = None;
                return iced::advanced::widget::operate(super::keyboard_drawing::ReleaseFocus);
            }
            Action::Program(program) => {
                dialog.program = program;
                if let Some(only) = dialog.template.only.filter(|&only| only != program) {
                    dialog.message = format!(
                        "{} is for {} only; switched to {}.",
                        dialog.template.name,
                        only.label(),
                        default.name
                    );
                    dialog.apply(default);
                }
                if program != Program::Xyz && !program.jobs().contains(&dialog.job) {
                    dialog.message = format!(
                        "{} does not offer {}; switched to Transition state.",
                        program.label(),
                        dialog.job.label()
                    );
                    dialog.job = Job::TransitionState;
                }
            }
            Action::Job(job) => dialog.job = job,
            Action::Template(choice) => {
                dialog.apply(choice.0);
            }
            Action::Field(field, value) => *dialog.field(field) = value,
            Action::NameTemplate => {
                dialog.template_name = Some(String::new());
                return iced::widget::operation::focus("qc-export-template-name");
            }
            Action::TemplateName(name) => dialog.template_name = Some(name),
            Action::SaveTemplate => {
                let name = dialog.template_name.take().unwrap_or_default();
                let name = name.trim();
                if name.is_empty() {
                    return Task::none();
                }
                let template = match dialog.settings() {
                    Ok(settings) => Template {
                        name: name.into(),
                        method: settings.method,
                        basis: settings.basis,
                        job: settings.job,
                        cores: settings.cores,
                        memory_gb: settings.memory_gb,
                        extra: settings.extra,
                        only: None,
                    },
                    Err(error) => {
                        dialog.message = error;
                        return Task::none();
                    }
                };
                let saved = &mut self.appearance.input_templates;
                match saved.iter_mut().find(|t| t.name == template.name) {
                    Some(existing) => *existing = template.clone(),
                    None => saved.push(template.clone()),
                }
                dialog.template = template;
                dialog.message = match self.appearance.save() {
                    Ok(()) => format!("Saved the template \u{201c}{name}\u{201d}"),
                    Err(error) => format!("Could not save the template: {error}"),
                };
            }
            Action::DeleteTemplate => {
                let saved = &mut self.appearance.input_templates;
                let Some(index) = saved.iter().position(|t| *t == dialog.template) else {
                    return Task::none();
                };
                let removed = saved.remove(index);
                dialog.template = default;
                dialog.message = match self.appearance.save() {
                    Ok(()) => format!("Deleted the template \u{201c}{}\u{201d}", removed.name),
                    Err(error) => format!("Could not save the templates: {error}"),
                };
            }
            Action::Generate => {
                // The model is made for the selection; select the molecule.
                self.tab.selected = dialog.atoms.clone();
                return self.models3d_action(super::models3d::Action::Show(Display::Both));
            }
            Action::Update => {
                self.tab.selected = dialog.atoms.clone();
                return self.models3d_action(super::models3d::Action::Update);
            }
            Action::Copy => match output {
                Ok(output) => {
                    dialog.message = "Copied the file text".into();
                    return iced::clipboard::write(output.main.text);
                }
                Err(error) => dialog.message = error,
            },
            Action::Save => match output {
                Ok(output) => return save(output, dialog.program, dialog.id),
                Err(error) => dialog.message = error,
            },
        }
        Task::none()
    }

    fn qc_model(&self, dialog: &Dialog) -> Model {
        if let Some(source) = &dialog.source {
            return Model::Ready {
                frame: (*source.frame).clone(),
                label: source.label(),
                stale: false,
            };
        }
        let doc = &self.tab.doc;
        let Some((index, model)) = doc
            .models
            .iter()
            .enumerate()
            .find(|(_, model)| dialog.atoms.iter().any(|id| model.contains(*id)))
        else {
            return if self.pending(&dialog.atoms).is_some() {
                Model::Generating
            } else {
                Model::Missing
            };
        };
        let Some(points) = model.points(doc) else {
            return Model::Missing;
        };
        let mut numbers = Vec::with_capacity(points.len());
        for (symbol, _) in &points {
            match reshiki::editing::ELEMENTS.iter().position(|e| e == symbol) {
                Some(i) => numbers.push(i as u8 + 1),
                None => {
                    return Model::Unsupported(format!(
                        "The 3D model has an atom no input format can name ({symbol})."
                    ));
                }
            }
        }
        let mut drawing_atoms = model.geometry_sources();
        drawing_atoms.resize(points.len(), None);
        let isotopes = drawing_atoms
            .iter()
            .map(|id| {
                id.and_then(|id| doc.atom(id))
                    .and_then(|atom| u16::try_from(atom.isotope).ok())
                    .unwrap_or(0)
            })
            .collect();
        let settings = dialog.settings().ok();
        let frame = Frame {
            numbers,
            positions_angstrom: points.iter().map(|(_, xyz)| xyz.map(f64::from)).collect(),
            isotopes,
            masses_amu: vec![],
            charge: settings.as_ref().map_or(0, |s| s.charge),
            multiplicity: settings.as_ref().map_or(1, |s| s.multiplicity),
            drawing_atoms,
        };
        let label = if model.is_file_derived() {
            format!("Current 3D model · {} atoms · from the file", points.len())
        } else {
            format!(
                "Current 3D model · {} atoms · {}",
                points.len(),
                force_field(model.force_field)
            )
        };
        Model::Ready {
            frame,
            label,
            stale: self.model_stale(index),
        }
    }

    /// The files the dialog would write, or why it cannot.
    pub(super) fn qc_output(&self) -> Result<Output, String> {
        let dialog = self.qc_export.as_ref().ok_or_else(String::new)?;
        let settings = dialog.settings()?;
        let generator = format!("ReShiki {}", env!("CARGO_PKG_VERSION"));
        if let Some(source) = &dialog.source {
            return source.output(&settings, &stem(&dialog.name), &generator);
        }
        let frame = match self.qc_model(dialog) {
            Model::Ready { stale: true, .. } => {
                return Err(
                    "The drawing changed since this 3D model was made: update the model first."
                        .into(),
                );
            }
            Model::Ready { frame, .. } => frame,
            _ => return Err("The molecule needs a 3D model first.".into()),
        };
        let geometry = Geometry {
            frame: &frame,
            reactant: None,
            product: None,
            result: None,
        };
        qc_input::write(&settings, &geometry, &stem(&dialog.name), &generator)
            .map_err(|error| error.to_string())
    }

    pub(super) fn with_qc_export<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(dialog) = &self.qc_export else {
            return base;
        };
        let send = |action: Action| Message::QcExport(action);
        let segment = |id: String, label: &'static str, active: bool, message: Option<Message>| {
            reshiki::accessibility::button(id, label, text(label).size(12))
                .checked(active)
                .padding([6, 10])
                .style(control(active))
                .on_press_maybe(message)
        };
        let input =
            |id: &'static str, label: &'static str, field: Field, value: &str, enabled: bool| {
                let mut input = reshiki::accessibility::text_input(id, label, "", value)
                    .style(crate::appearance::input_style)
                    .size(12)
                    .padding([6, 8]);
                if enabled {
                    input = input.on_input(move |value| send(Action::Field(field, value)));
                }
                column![text(label).size(11).style(muted_text), input]
                    .spacing(3)
                    .width(Length::Fill)
            };
        let xyz = dialog.program == Program::Xyz;
        let caption = |label: &'static str| text(label).size(11).style(muted_text).width(70);

        // Program
        let mut programs = row![caption("Program")]
            .spacing(4)
            .align_y(Alignment::Center);
        for program in Program::ALL {
            programs = programs.push(segment(
                format!("qc-export-program-{}", program.label().to_lowercase()),
                program.label(),
                dialog.program == program,
                Some(send(Action::Program(program))),
            ));
        }
        for later in ["Q-Chem", "Turbomole"] {
            programs = programs.push(segment(
                format!("qc-export-program-{later}"),
                later,
                false,
                None,
            ));
        }
        programs = programs.push(text("coming later").size(11).style(muted_text));

        // Template
        let choices: Vec<Choice> = self
            .input_templates()
            .into_iter()
            .filter(|t| t.only.is_none_or(|only| only == dialog.program))
            .map(Choice)
            .collect();
        let current = choices.iter().find(|c| c.0 == dialog.template).cloned();
        let picker = reshiki::accessibility::select(
            "qc-export-template",
            "Template",
            choices.clone(),
            current
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            move |choice| send(Action::Template(choice)),
            crate::appearance::close_on_escape(
                crate::appearance::pick_list(choices, current, move |choice| {
                    send(Action::Template(choice))
                })
                .width(Length::Fill)
                .text_size(12)
                .padding([6, 8]),
            ),
        );
        let user_template = self.appearance.input_templates.contains(&dialog.template);
        let template_actions: Element<'_, Message> = match &dialog.template_name {
            Some(name) => row![
                reshiki::accessibility::text_input(
                    "qc-export-template-name",
                    "Template name",
                    "Template name",
                    name
                )
                .style(crate::appearance::input_style)
                .on_input(move |name| send(Action::TemplateName(name)))
                .on_submit(send(Action::SaveTemplate))
                .size(12)
                .padding([5, 8]),
                reshiki::accessibility::button(
                    "qc-export-template-save",
                    "Save template",
                    text("Save").size(12)
                )
                .padding([5, 9])
                .style(control(false))
                .on_press(send(Action::SaveTemplate)),
            ]
            .spacing(4)
            .into(),
            None => row![
                reshiki::accessibility::button(
                    "qc-export-template-new",
                    "Save as template",
                    text("Save as template…").size(11)
                )
                .padding([3, 0])
                .style(iced::widget::button::text)
                .on_press_maybe((!xyz).then(|| send(Action::NameTemplate))),
                Space::new().width(12),
                reshiki::accessibility::button(
                    "qc-export-template-delete",
                    "Delete template",
                    text("Delete template").size(11)
                )
                .padding([3, 0])
                .style(iced::widget::button::text)
                .on_press_maybe(user_template.then(|| send(Action::DeleteTemplate))),
            ]
            .into(),
        };

        // Job: the plain tasks, then the transition-state tasks.
        let endpoints = dialog.source.as_ref().is_some_and(Source::has_endpoints);
        let job_row = |jobs: &[Job]| {
            let mut buttons = row![].spacing(4);
            for &job in jobs {
                let offered = !xyz && dialog.program.jobs().contains(&job);
                // The pairs of structures come from a transition-state search.
                let needs_path = matches!(job, Job::Qst2 | Job::Qst3 | Job::NebTs);
                buttons = buttons.push(segment(
                    format!("qc-export-job-{job:?}"),
                    job.label(),
                    !xyz && dialog.job == job,
                    (offered && (!needs_path || endpoints)).then(|| send(Action::Job(job))),
                ));
            }
            buttons.wrap()
        };
        let path_jobs: Vec<&str> = [Job::Qst2, Job::Qst3, Job::NebTs]
            .into_iter()
            .filter(|job| dialog.program.jobs().contains(job))
            .map(Job::label)
            .collect();
        let job_note = if xyz {
            "XYZ writes the coordinates only.".to_owned()
        } else if endpoints {
            "Reactant, product and TS guess from this transition-state search.".to_owned()
        } else if path_jobs.is_empty() {
            format!("{} offers no QST2, QST3 or NEB-TS.", dialog.program.label())
        } else {
            format!(
                "{} {} the reactant and product of a transition-state search.",
                path_jobs.join(" and "),
                if path_jobs.len() == 1 {
                    "needs"
                } else {
                    "need"
                }
            )
        };
        let irc = dialog.job == Job::Irc && !xyz;

        let model = self.qc_model(dialog);
        let geometry: Element<'_, Message> = match &model {
            Model::Ready { label, stale, .. } => {
                let mut lines = column![text(label.clone()).size(12)].spacing(2);
                if *stale {
                    lines = lines.push(
                        reshiki::accessibility::button(
                            "qc-export-update",
                            "Update 3D model",
                            text("Update 3D model").size(12),
                        )
                        .padding([5, 9])
                        .style(control(false))
                        .on_press(send(Action::Update)),
                    );
                    lines = lines.push(
                        text("The drawing changed after this model was made; update the 3D model first.")
                            .size(11)
                            .style(crate::appearance::text_color(WARNING)),
                    );
                }
                lines.into()
            }
            Model::Generating => text("Making the 3D model…").size(12).into(),
            Model::Missing => row![
                text("No 3D model yet.").size(12).width(Length::Fill),
                reshiki::accessibility::button(
                    "qc-export-generate",
                    "Generate 3D model",
                    text("Generate 3D model").size(12)
                )
                .padding([5, 9])
                .style(control(false))
                .on_press(send(Action::Generate)),
            ]
            .align_y(Alignment::Center)
            .into(),
            Model::Unsupported(reason) => text(reason.clone())
                .size(12)
                .style(crate::appearance::text_color(ERROR))
                .into(),
        };

        let form = column![
            column![
                text("Template").size(11).style(muted_text),
                picker,
                template_actions
            ]
            .spacing(3),
            row![
                input(METHOD, "Method", Field::Method, &dialog.method, !xyz),
                input(
                    "qc-export-basis",
                    "Basis",
                    Field::Basis,
                    &dialog.basis,
                    !xyz
                ),
            ]
            .spacing(8),
            column![
                text("Task").size(11).style(muted_text),
                job_row(&[Job::SinglePoint, Job::Optimize, Job::OptFreq]),
                job_row(&[
                    Job::TransitionState,
                    Job::Qst2,
                    Job::Qst3,
                    Job::Irc,
                    Job::NebTs
                ]),
                text(job_note).size(11).style(muted_text),
            ]
            .spacing(4),
            row![
                input(
                    "qc-export-charge",
                    "Charge",
                    Field::Charge,
                    &dialog.charge,
                    !xyz
                ),
                input(
                    "qc-export-multiplicity",
                    "Multiplicity",
                    Field::Multiplicity,
                    &dialog.multiplicity,
                    !xyz
                ),
            ]
            .spacing(8),
            text(if dialog.source.is_some() {
                "Charge and multiplicity from the TS result"
            } else {
                "Charge and multiplicity from the drawing"
            })
            .size(11)
            .style(muted_text),
            row![
                input(
                    "qc-export-cores",
                    "Cores",
                    Field::Cores,
                    &dialog.cores,
                    !xyz
                ),
                input(
                    "qc-export-memory",
                    "Memory (GB)",
                    Field::Memory,
                    &dialog.memory,
                    !xyz
                ),
            ]
            .spacing(8),
            irc.then(|| input(
                "qc-export-irc",
                "IRC points (each way)",
                Field::IrcPoints,
                &dialog.irc_points,
                true
            )),
            input(
                "qc-export-title",
                "Title",
                Field::Title,
                &dialog.title,
                true
            ),
            input(
                "qc-export-extra",
                "Extra keywords",
                Field::Extra,
                &dialog.extra,
                !xyz
            ),
            column![text("Geometry").size(11).style(muted_text), geometry].spacing(3),
        ]
        .spacing(10)
        .width(330);

        // Preview
        let output = self.qc_output();
        let file_name = output
            .as_ref()
            .map_or_else(|_| dialog.file_name(), |output| output.main.name.clone());
        let preview: Element<'_, Message> = match &output {
            Ok(output) => {
                let spans: Vec<_> = output
                    .main
                    .text
                    .lines()
                    .map(|line| {
                        let span: text::Span<'_> = span(format!("{line}\n")).font(Font::MONOSPACE);
                        match line_color(dialog.program, line) {
                            Some(color) => span.color(color),
                            None => span,
                        }
                    })
                    .collect();
                rich_text(spans).size(12).into()
            }
            Err(error) => text(error.clone())
                .size(12)
                .style(crate::appearance::text_color(ERROR))
                .into(),
        };
        let preview = container(scrollable(
            container(preview).padding(10).width(Length::Fill),
        ))
        .height(390)
        .width(Length::Fill)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::from_rgb8(248, 250, 250).into()),
                    border: Border {
                        radius: 8.into(),
                        width: 1.,
                        color: Color::from_rgb8(220, 226, 226),
                    },
                    ..Default::default()
                },
            )
        });
        let mut notes = column![].spacing(3);
        if let Ok(output) = &output {
            if !output.companions.is_empty() {
                let also: Vec<_> = output.companions.iter().map(|f| f.name.as_str()).collect();
                notes = notes.push(
                    text(format!("Also written beside it: {}", also.join(", ")))
                        .size(11)
                        .style(muted_text),
                );
            }
            for warning in &output.warnings {
                notes = notes.push(
                    text(warning.clone())
                        .size(11)
                        .style(crate::appearance::text_color(WARNING)),
                );
            }
        }
        let side = column![
            row![
                text("Preview · exact file text")
                    .size(11)
                    .style(muted_text)
                    .width(Length::Fill),
                text(file_name.clone()).size(11).style(muted_text),
            ],
            preview,
            notes,
        ]
        .spacing(6)
        .width(Length::Fill);

        let ready = output.is_ok();
        let companions = output.as_ref().map_or(0, |o| o.companions.len());
        let save_label = match companions {
            0 => format!("Save {file_name}…"),
            1 => format!("Save {file_name} + 1 file…"),
            n => format!("Save {file_name} + {n} files…"),
        };
        let footer = row![
            text(dialog.message.clone())
                .size(12)
                .style(muted_text)
                .width(Length::Fill),
            reshiki::accessibility::button("qc-export-cancel", "Cancel", text("Cancel").size(12))
                .padding([7, 12])
                .style(control(false))
                .on_press(send(Action::Close)),
            reshiki::accessibility::button(
                "qc-export-copy",
                "Copy the file text",
                text("Copy").size(12)
            )
            .padding([7, 12])
            .style(control(false))
            .on_press_maybe(ready.then(|| send(Action::Copy))),
            reshiki::accessibility::button(
                "qc-export-save",
                save_label.clone(),
                text(save_label).size(12)
            )
            .padding([7, 12])
            .style(crate::appearance::primary)
            .on_press_maybe(ready.then(|| send(Action::Save))),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let content = column![
            column![
                text(format!("Export input file · {}", dialog.name)).size(16),
                text("Writes a text input for another program. Nothing runs here.")
                    .size(12)
                    .style(muted_text),
            ]
            .spacing(2),
            programs,
            row![form, side].spacing(18),
            footer,
        ]
        .spacing(14);
        modal(
            base,
            scrollable(content).into(),
            960.,
            760.,
            Placement::Fit,
            send(Action::Close),
        )
    }
}

/// Save the main file through the file dialog and its companions beside it.
fn save(output: Output, program: Program, id: u64) -> Task<Message> {
    Task::perform(
        async move {
            let Output {
                main, companions, ..
            } = output;
            let path = super::files::save_bytes(
                &format!("Export {} input", program.label()),
                &main.name,
                program.extension(),
                main.text.as_bytes(),
                None,
            )
            .await?;
            let mut kept = Vec::new();
            if let Some(directory) = path.as_ref().and_then(|p| p.parent()) {
                for file in companions {
                    let companion = directory.join(&file.name);
                    // Never replace a file the save dialog did not ask about.
                    if companion.exists() {
                        kept.push(file.name);
                    } else {
                        super::files::write(&companion, file.text.as_bytes())?;
                    }
                }
            }
            if !kept.is_empty() {
                return Err(format!(
                    "Saved the input; {} already exists beside it and was not replaced.",
                    kept.join(", ")
                ));
            }
            Ok(path)
        },
        move |result| Message::QcExport(Action::Saved(id, result)),
    )
}

#[cfg(test)]
mod tests;
