//! Desktop export for molecular previews and placed energy profiles.
use super::{
    App, Message,
    animation::{PATH_FRAME_SECONDS, PathSample, PlaybackMode},
    gates::Gate,
    help::{Placement, modal},
    models3d::thumbnail::{AnimationPlot, AnimationRenderer},
    profile_animation::{self, FrameRenderer, Timeline},
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
#[cfg(target_os = "macos")]
mod mp4;

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
    Profile {
        id: u64,
        timeline: Timeline,
        motion: profile_animation::Motion,
    },
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
            Motion::Profile { timeline, .. } => timeline.sequence(FPS, self.mode, self.speed),
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Gif,
    #[cfg(target_os = "macos")]
    Mp4,
}

impl Format {
    fn available(_profile: bool) -> &'static [Self] {
        #[cfg(target_os = "macos")]
        if _profile {
            return &[Self::Gif, Self::Mp4];
        }
        &[Self::Gif]
    }

    fn title(self) -> &'static str {
        match self {
            Self::Gif => "GIF",
            #[cfg(target_os = "macos")]
            Self::Mp4 => "MP4",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            #[cfg(target_os = "macos")]
            Self::Mp4 => "mp4",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    OpenIrc,
    OpenMode,
    OpenProfile,
    Format(Format),
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
    format: Format,
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
        if matches!(
            action,
            Action::OpenIrc | Action::OpenMode | Action::OpenProfile
        ) {
            if self.gif_export.is_some() {
                return Task::none();
            }
            let source = match action {
                Action::OpenIrc => self.xtb_irc_gif_source(),
                Action::OpenProfile => self.profile_export_source(),
                _ => self.xtb_vibration_gif_source(),
            };
            if let Some(source) = source {
                self.context_menu = None;
                self.gif_export = Some(Dialog {
                    id: NEXT_DIALOG.fetch_add(1, Relaxed),
                    source: Arc::new(source),
                    options: Options::default(),
                    format: Format::default(),
                    stage: Stage::Options,
                });
                return iced::widget::operation::focus(ACTION_ID);
            }
            self.status = if matches!(action, Action::OpenProfile) {
                "Select an energy profile to export."
            } else {
                "GIF export requires saved frames and a matching 3D model."
            }
            .into();
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
            Action::Format(format) if matches!(dialog.stage, Stage::Options) => {
                if Format::available(matches!(dialog.source.motion, Motion::Profile { .. }))
                    .contains(&format)
                {
                    dialog.format = format;
                    dialog.options.transparent &= format == Format::Gif;
                }
            }
            Action::Transparent(value)
                if matches!(dialog.stage, Stage::Options) && dialog.format == Format::Gif =>
            {
                dialog.options.transparent = value;
            }
            Action::Plot(value) if matches!(dialog.stage, Stage::Options) => {
                dialog.options.plot = value && matches!(dialog.source.motion, Motion::Irc { .. });
            }
            Action::Save if matches!(dialog.stage, Stage::Options) => {
                dialog.stage = Stage::Picking;
                let id = dialog.id;
                let format = dialog.format;
                let name = encode::file_name(&dialog.source.name, format.extension());
                let title = if matches!(dialog.source.motion, Motion::Profile { .. }) {
                    "Export animation"
                } else {
                    "Export animated GIF"
                };
                return Task::perform(
                    async move { super::files::save_path(title, &name, format.extension()).await },
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
                let format = dialog.format;
                dialog.stage = Stage::Rendering(Arc::clone(&work), sequence.len());
                return Task::batch([
                    iced::widget::operation::focus(CANCEL_ID),
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                render(&source, options, format, &sequence, &work, &path)
                                    .map(|saved| saved.then_some(path))
                            })
                            .await
                            .map_err(|e| format!("The animation render stopped. {e}"))?
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
        let profile = matches!(dialog.source.motion, Motion::Profile { .. });
        let formats = Format::available(profile);
        let format = dialog.format.title();
        let title = if profile {
            "Export animation"
        } else {
            "Export animated GIF"
        };
        let mut content = column![text(title).size(16)].spacing(12);
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
                if formats.len() > 1 {
                    let choices: Vec<_> = formats
                        .iter()
                        .map(|&item| {
                            (
                                item.extension(),
                                item.title(),
                                dialog.format == item,
                                Action::Format(item),
                            )
                        })
                        .collect();
                    content = content.push(choice("Format", &choices));
                }
                content = content.push(choice(
                    "Size",
                    &[
                        (480, "480 px", "gif-480"),
                        (720, "720 px", "gif-720"),
                        (1080, "1080 px", "gif-1080"),
                    ]
                    .map(|(n, label, id)| (id, label, n == options.size, Action::Size(n))),
                ));
                if dialog.format == Format::Gif {
                    content = content.push(choice(
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
                    ));
                } else {
                    content = content.push(row![
                        text("Background").size(12).width(90),
                        text("White").size(12)
                    ]);
                }
                let summary = match &dialog.source.motion {
                    Motion::Profile { motion, .. } => {
                        format!("{motion} · {} · {FPS} fps", dialog.source.mode)
                    }
                    _ => format!("{} · {FPS} fps", dialog.source.mode),
                };
                content = content.push(text(summary).size(12).style(muted_text));
                true
            }
            Stage::Picking => {
                content =
                    content.push(text(format!("Choose where to save the {format}.")).size(12));
                false
            }
            Stage::Rendering(work, total) => {
                let done = work.completed.load(Relaxed);
                let saving = work.state.load(Relaxed) == SAVING;
                content = content
                    .push(
                        text(if saving {
                            format!("Saving {format}…")
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
            let label = format!("Save {format}…");
            reshiki::accessibility::button(ACTION_ID, label.clone(), text(label).size(12))
                .padding([6, 12])
                .style(crate::appearance::primary)
                .on_press(Message::GifExport(Action::Save))
        });
        content =
            content.push(row![iced::widget::space::horizontal(), cancel_button, save].spacing(6));
        modal(
            base,
            content.into(),
            420.,
            if profile { 345. } else { 315. },
            Placement::Fit,
            cancel,
        )
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
    format: Format,
    sequence: &[usize],
    work: &Work,
    path: &Path,
) -> Result<bool, String> {
    if work.state.load(Relaxed) == CANCELED {
        return Ok(false);
    }
    let model_renderer = || {
        AnimationRenderer::new(
            &source.doc,
            source.model,
            u32::from(options.size),
            options.transparent,
        )
    };
    match &source.motion {
        Motion::Profile { id, timeline, .. } => {
            let mut renderer =
                FrameRenderer::new(&source.doc, *id, options.size, options.transparent)?;
            let render = |_, index| {
                let frame = timeline
                    .frame(index)
                    .ok_or("Missing energy profile frame")?;
                let geometry = timeline.geometry(frame);
                renderer.render(frame.state, frame.sample(), geometry.as_ref(), true)
            };
            match format {
                Format::Gif => encode::save(path, options, source.mode, sequence, work, render),
                #[cfg(target_os = "macos")]
                Format::Mp4 => mp4::save(path, options, sequence, work, render),
            }
        }
        Motion::Mode(frames) => {
            let mut renderer = model_renderer()?;
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
            let mut renderer = model_renderer()?;
            let first = geometries.first().ok_or("The animation has no frames")?;
            renderer.set_irc_topology(&trajectory.numbers, first)?;
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

#[cfg(test)]
mod profile_tests;
