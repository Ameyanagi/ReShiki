//! Desktop export for the IRC and normal-mode previews.
use super::{
    App, Message,
    animation::{PATH_FRAME_SECONDS, PathSample, PlaybackMode},
    gates::Gate,
    help::{Placement, modal},
    models3d::thumbnail::{AnimationPlot, AnimationRenderer},
    workspace::muted_text,
};
use crate::appearance::EnergyUnit;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{column, progress_bar, row, text};
use iced::{Alignment, Element, Length, Subscription, Task};
use reshiki::document::Document;
use reshiki::qcdata::Trajectory;
use std::ops::ControlFlow::{Break, Continue};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::time::Duration;

mod encode;

const ACTION_ID: &str = "gif-export-save";
const CANCEL_ID: &str = "gif-export-cancel";
const FPS: usize = 20;
static NEXT_DIALOG: AtomicU64 = AtomicU64::new(0);

pub(super) fn button(id: &'static str, action: Action, enabled: bool) -> Element<'static, Message> {
    super::workspace::accessible_command(id, "Export the animation as a GIF", "Export GIF…")
        .on_press_maybe(enabled.then_some(Message::GifExport(action)))
        .into()
}

pub(super) struct Source {
    pub(super) name: String,
    pub(super) doc: Document,
    pub(super) model: usize,
    pub(super) motion: Motion,
    pub(super) mode: PlaybackMode,
    pub(super) speed: f32,
}

pub(super) enum Motion {
    Mode(Vec<Vec<[f32; 3]>>),
    Irc {
        trajectory: Box<Trajectory>,
        samples: Vec<PathSample>,
        ts_index: usize,
        unit: EnergyUnit,
    },
}

impl Source {
    fn sequence(&self) -> Vec<usize> {
        match &self.motion {
            Motion::Irc {
                samples, ts_index, ..
            } => encode::irc_frames(samples.len(), *ts_index, self.mode, self.speed),
            Motion::Mode(frames) => {
                let count = (FPS as f32 / self.speed).round() as usize;
                // One period wraps directly from the last sample to the first.
                (0..count).map(|i| i * frames.len() / count).collect()
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Options {
    size: u16,
    transparent: bool,
    plot: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            size: 480,
            transparent: false,
            plot: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    OpenIrc,
    OpenMode,
    Size(u16),
    Transparent(bool),
    Plot(bool),
    Save,
    Picked(u64, Option<PathBuf>),
    Progress,
    Finished(u64, Result<Option<PathBuf>, String>),
    Cancel,
}

enum Stage {
    Options,
    Picking,
    Rendering(Arc<Work>, usize),
}

pub(super) struct Dialog {
    id: u64,
    source: Arc<Source>,
    options: Options,
    stage: Stage,
}

impl Drop for Dialog {
    fn drop(&mut self) {
        if let Stage::Rendering(work, _) = &self.stage {
            work.cancel();
        }
    }
}

const RENDERING: u8 = 0;
const CANCELED: u8 = 1;
const SAVING: u8 = 2;

#[derive(Default)]
struct Work {
    state: AtomicU8,
    completed: AtomicUsize,
}

impl Work {
    fn cancel(&self) -> bool {
        self.state
            .compare_exchange(RENDERING, CANCELED, Relaxed, Relaxed)
            .is_ok()
    }
}

pub(super) fn modal_key(key: &Key, modifiers: Modifiers) -> Option<Message> {
    match key {
        Key::Named(Named::Escape) if modifiers.is_empty() => {
            Some(Message::GifExport(Action::Cancel))
        }
        Key::Named(Named::Enter) if modifiers.command() => Some(Message::GifExport(Action::Save)),
        _ => None,
    }
}

impl App {
    pub(super) fn gif_export_gate(&mut self, message: Message) -> Gate {
        if self.gif_export.is_some() {
            match message {
                Message::GifExport(action) => return Break(self.gif_export_action(action)),
                Message::Escape => return Break(self.gif_export_action(Action::Cancel)),
                _ if !super::updates::background(&message)
                    && !self.answers_save_dialog(&message) =>
                {
                    return Break(Task::none());
                }
                _ => {}
            }
        }
        Continue(message)
    }

    pub(super) fn gif_export_subscription(&self) -> Subscription<Message> {
        if self
            .gif_export
            .as_ref()
            .is_some_and(|dialog| matches!(dialog.stage, Stage::Rendering(..)))
        {
            iced::time::every(Duration::from_millis(100))
                .map(|_| Message::GifExport(Action::Progress))
        } else {
            Subscription::none()
        }
    }

    pub(super) fn gif_export_action(&mut self, action: Action) -> Task<Message> {
        if matches!(action, Action::OpenIrc | Action::OpenMode) {
            if self.gif_export.is_some() {
                return Task::none();
            }
            let source = match action {
                Action::OpenIrc => self.xtb_irc_gif_source(),
                _ => self.xtb_vibration_gif_source(),
            };
            if let Some(source) = source {
                self.context_menu = None;
                self.gif_export = Some(Dialog {
                    id: NEXT_DIALOG.fetch_add(1, Relaxed),
                    source: Arc::new(source),
                    options: Options::default(),
                    stage: Stage::Options,
                });
                return iced::widget::operation::focus(ACTION_ID);
            }
            self.status = "GIF export requires saved frames and a matching 3D model.".into();
            self.error = true;
            return Task::none();
        }
        let Some(dialog) = &mut self.gif_export else {
            return Task::none();
        };
        match action {
            Action::Cancel => {
                if let Stage::Rendering(work, _) = &dialog.stage
                    && !work.cancel()
                {
                    return Task::none();
                }
                self.gif_export = None;
                return super::focus_drawing();
            }
            Action::Size(size) if matches!(dialog.stage, Stage::Options) => {
                if [480, 720, 1080].contains(&size) {
                    dialog.options.size = size;
                }
            }
            Action::Transparent(value) if matches!(dialog.stage, Stage::Options) => {
                dialog.options.transparent = value;
            }
            Action::Plot(value) if matches!(dialog.stage, Stage::Options) => {
                dialog.options.plot = value && matches!(dialog.source.motion, Motion::Irc { .. });
            }
            Action::Save if matches!(dialog.stage, Stage::Options) => {
                dialog.stage = Stage::Picking;
                let id = dialog.id;
                let name = encode::file_name(&dialog.source.name);
                return Task::perform(
                    async move { super::files::save_path("Export animated GIF", &name, "gif").await },
                    move |path| Message::GifExport(Action::Picked(id, path)),
                );
            }
            Action::Picked(id, path)
                if id == dialog.id && matches!(dialog.stage, Stage::Picking) =>
            {
                let Some(path) = path else {
                    self.gif_export = None;
                    return super::focus_drawing();
                };
                let work = Arc::new(Work::default());
                let source = Arc::clone(&dialog.source);
                let options = dialog.options;
                let sequence = source.sequence();
                dialog.stage = Stage::Rendering(Arc::clone(&work), sequence.len());
                return Task::batch([
                    iced::widget::operation::focus(CANCEL_ID),
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                render(&source, options, &sequence, &work, &path)
                                    .map(|saved| saved.then_some(path))
                            })
                            .await
                            .map_err(|e| format!("The GIF render stopped. {e}"))?
                        },
                        move |result| Message::GifExport(Action::Finished(id, result)),
                    ),
                ]);
            }
            Action::Finished(id, result) if id == dialog.id => {
                self.gif_export = None;
                match result {
                    Ok(Some(path)) => {
                        self.status = format!("Saved {}", path.display());
                        self.error = false;
                    }
                    Err(error) => {
                        self.status = format!("Could not export the animation. {error}");
                        self.error = true;
                    }
                    Ok(None) => {}
                }
                return super::focus_drawing();
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn with_gif_export<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(dialog) = &self.gif_export else {
            return base;
        };
        let cancel = Message::GifExport(Action::Cancel);
        let mut content = column![text("Export animated GIF").size(16)].spacing(12);
        let cancel_available = match &dialog.stage {
            Stage::Options => {
                let options = dialog.options;
                if matches!(dialog.source.motion, Motion::Irc { .. }) {
                    content = content.push(choice(
                        "Content",
                        &[
                            (
                                "gif-model",
                                "3D model only",
                                !options.plot,
                                Action::Plot(false),
                            ),
                            (
                                "gif-plot",
                                "Model + energy plot",
                                options.plot,
                                Action::Plot(true),
                            ),
                        ],
                    ));
                }
                content = content
                    .push(choice(
                        "Size",
                        &[
                            (480, "480 px", "gif-480"),
                            (720, "720 px", "gif-720"),
                            (1080, "1080 px", "gif-1080"),
                        ]
                        .map(|(n, label, id)| (id, label, n == options.size, Action::Size(n))),
                    ))
                    .push(choice(
                        "Background",
                        &[
                            (
                                "gif-white",
                                "White",
                                !options.transparent,
                                Action::Transparent(false),
                            ),
                            (
                                "gif-transparent",
                                "Transparent",
                                options.transparent,
                                Action::Transparent(true),
                            ),
                        ],
                    ))
                    .push(
                        text(format!("{} · {FPS} fps", dialog.source.mode))
                            .size(12)
                            .style(muted_text),
                    );
                true
            }
            Stage::Picking => {
                content = content.push(text("Choose where to save the GIF.").size(12));
                false
            }
            Stage::Rendering(work, total) => {
                let done = work.completed.load(Relaxed);
                let saving = work.state.load(Relaxed) == SAVING;
                content = content
                    .push(
                        text(if saving {
                            "Saving GIF…".into()
                        } else {
                            format!("Rendering frames · {done} / {total}")
                        })
                        .size(12),
                    )
                    .push(progress_bar(0. ..=(*total).max(1) as f32, done as f32).girth(4));
                !saving
            }
        };
        let cancel_button =
            reshiki::accessibility::button(CANCEL_ID, "Cancel", text("Cancel").size(12))
                .padding([6, 12])
                .style(crate::appearance::secondary)
                .on_press_maybe(cancel_available.then(|| cancel.clone()));
        let save = matches!(dialog.stage, Stage::Options).then(|| {
            reshiki::accessibility::button(ACTION_ID, "Save GIF…", text("Save GIF…").size(12))
                .padding([6, 12])
                .style(crate::appearance::primary)
                .on_press(Message::GifExport(Action::Save))
        });
        content =
            content.push(row![iced::widget::space::horizontal(), cancel_button, save].spacing(6));
        modal(base, content.into(), 420., 315., Placement::Fit, cancel)
    }
}

fn choice(
    label: &'static str,
    choices: &[(&'static str, &'static str, bool, Action)],
) -> Element<'static, Message> {
    let mut row = row![text(label).size(12).width(90)]
        .spacing(6)
        .align_y(Alignment::Center);
    for (id, title, active, action) in choices {
        row = row.push(
            reshiki::accessibility::button(*id, *title, text(*title).size(12))
                .checked(*active)
                .padding([5, 8])
                .style(super::workspace::control(*active))
                .on_press(Message::GifExport(action.clone())),
        );
    }
    row.width(Length::Fill).into()
}

fn render(
    source: &Source,
    options: Options,
    sequence: &[usize],
    work: &Work,
    path: &Path,
) -> Result<bool, String> {
    if work.state.load(Relaxed) == CANCELED {
        return Ok(false);
    }
    let mut renderer = AnimationRenderer::new(
        &source.doc,
        source.model,
        u32::from(options.size),
        options.transparent,
    )?;
    match &source.motion {
        Motion::Mode(frames) => {
            renderer.fit(frames)?;
            encode::save(path, options, source.mode, sequence, work, |_, index| {
                let positions = frames.get(index).ok_or("Missing normal-mode frame")?;
                renderer.render(positions, None)
            })
        }
        Motion::Irc {
            trajectory,
            samples,
            unit,
            ..
        } => {
            let width = trajectory.numbers.len() * 3;
            if width == 0 || trajectory.positions_angstrom.len() != width * samples.len() {
                return Err("The IRC geometries are incomplete.".into());
            }
            let geometries: Vec<_> = trajectory.positions_angstrom.chunks_exact(width).collect();
            let frames: Vec<Vec<[f32; 3]>> = geometries
                .iter()
                .map(|xyz| {
                    xyz.as_chunks::<3>()
                        .0
                        .iter()
                        .map(|p| p.map(|x| x as f32))
                        .collect()
                })
                .collect();
            renderer.fit(&frames)?;
            let points: Vec<_> = samples
                .iter()
                .map(|sample| [sample.s, unit.convert_kcal(sample.energy_kcal)])
                .collect();
            let y_label = format!("ΔE ({})", unit.label());
            encode::save(
                path,
                options,
                source.mode,
                sequence,
                work,
                |frame, index| {
                    let geometry = geometries.get(index).ok_or("Missing IRC geometry")?;
                    let positions = frames.get(index).ok_or("Missing IRC frame")?;
                    renderer.set_irc_topology(&trajectory.numbers, geometry)?;
                    let sample = samples.get(index).ok_or("Missing IRC energy")?;
                    let readout = sample.readout(*unit, frame, sequence.len());
                    let plot = if options.plot {
                        Some(AnimationPlot {
                            points: &points,
                            marker: *points.get(index).ok_or("Missing IRC point")?,
                            x_label: "s (amu½·bohr)",
                            y_label: &y_label,
                            readout: &readout,
                            direction: sample.direction(),
                        })
                    } else {
                        None
                    };
                    renderer.render(positions, plot.as_ref())
                },
            )
        }
    }
}
