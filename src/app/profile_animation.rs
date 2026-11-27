//! Playback of a placed energy profile, owned by the active drawing view.
use super::{
    App, Message,
    animation::{PATH_FRAME_SECONDS, Playback, PlaybackMode, PlaybackSpeed},
    document_tab::TabId,
    workspace::accessible_command,
    xtb::{
        self,
        ts::{self, irc::focus, profile::ProfileIrc},
    },
};
use iced::widget::{checkbox, column, row, text};
use iced::{Element, Subscription, Task};
use reshiki::document::Document;
use reshiki::graphics::{EnergyProfile, Graphic};
use reshiki::scene::Primitive;
use std::time::Duration;

mod artwork;
#[cfg(test)]
pub(super) mod tests;
mod timeline;

#[cfg(not(target_arch = "wasm32"))]
pub(super) use artwork::FrameRenderer;
pub use timeline::Motion;
pub(super) use timeline::Timeline;

const READOUT_ID: &str = "energy-profile-timeline";

#[derive(Clone, Debug)]
pub enum Action {
    Toggle,
    Motion(Motion),
    Highlight(bool),
    Step(i32),
    Tick,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Target {
    tab: TabId,
    epoch: u64,
    revision: u64,
    graphic: u64,
}

pub(super) struct Session {
    target: Target,
    paths: Vec<Option<ProfileIrc>>,
    motion: Motion,
    timeline: Timeline,
    position: Playback,
    highlight: bool,
    artwork: artwork::Artwork,
    ink: Vec<Primitive>,
    hidden_picture: Option<(u64, usize)>,
}

impl Session {
    fn new(
        target: Target,
        doc: &Document,
        graphic: &Graphic,
        profile: &EnergyProfile,
        paths: Vec<Option<ProfileIrc>>,
    ) -> Self {
        let motion = Motion::Steps;
        let timeline = Timeline::new(profile, &paths, motion);
        let mut position = Playback::default();
        position.seek(0, timeline.len());
        Self {
            target,
            paths,
            motion,
            timeline,
            position,
            highlight: true,
            artwork: artwork::Artwork::new(doc, graphic),
            ink: vec![],
            hidden_picture: None,
        }
    }

    fn show(&mut self, doc: &Document, graphic: &Graphic) {
        let Some(frame) = self.timeline.frame(self.position.frame) else {
            return;
        };
        let geometry = self.timeline.geometry(frame);
        self.hidden_picture = geometry.as_ref().and_then(|_| {
            let profile = graphic.energy_profile.as_deref()?;
            let slot = self.timeline.picture_slot(profile, frame)?;
            Some((graphic.id, slot))
        });
        self.ink = self.artwork.overlay(
            doc,
            graphic,
            frame.state,
            frame.sample(),
            geometry.as_ref(),
            self.highlight,
        );
    }
}

impl App {
    pub(super) fn selected_energy_profile(&self) -> Option<&Graphic> {
        let [id] = self.tab.selected.as_slice() else {
            return None;
        };
        self.tab
            .doc
            .graphics
            .iter()
            .find(|g| g.id == *id && g.energy_profile.is_some())
    }

    fn profile_animation_target(&self, graphic: &Graphic) -> Target {
        Target {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            graphic: graphic.id,
        }
    }

    fn profile_animation_allowed(&self) -> bool {
        self.tabs.temporary_depth == 0
            && !self.tab.busy
            && !self.tab.drawing_edit_active()
            && !self.tube_bend_active()
    }

    fn current_profile_animation(&self) -> Option<&Session> {
        let session = self.profile_animation.as_ref()?;
        let graphic = self.selected_energy_profile()?;
        (self.profile_animation_allowed()
            && session.target == self.profile_animation_target(graphic))
        .then_some(session)
    }

    pub(super) fn sync_profile_animation(&mut self) {
        if self.current_profile_animation().is_none() {
            self.profile_animation = None;
        }
    }

    pub(super) fn profile_animation_overlay(&self) -> Option<crate::canvas::ProfileAnimation<'_>> {
        let session = self.current_profile_animation()?;
        if session.ink.is_empty() || !std::ptr::eq(self.display_document(), &self.tab.doc) {
            return None;
        }
        Some(crate::canvas::ProfileAnimation {
            hidden_picture: session.hidden_picture,
            ink: &session.ink,
        })
    }

    pub(super) fn profile_animation_subscription(&self) -> Subscription<Message> {
        let Some(session) = self
            .current_profile_animation()
            .filter(|s| s.position.playing)
        else {
            return Subscription::none();
        };
        let Some(frame) = session.timeline.frame(session.position.frame) else {
            return Subscription::none();
        };
        let hold = frame.seconds / self.appearance.playback_speed.factor();
        iced::time::every(Duration::from_secs_f32(hold))
            .map(|_| Message::ProfileAnimation(Action::Tick))
    }

    pub(super) fn tick_profile_animation(&mut self) {
        self.sync_profile_animation();
        let Some(session) = &mut self.profile_animation else {
            return;
        };
        if session
            .position
            .advance(self.appearance.playback_mode, session.timeline.len())
            && let Some(graphic) = self
                .tab
                .doc
                .graphics
                .iter()
                .find(|g| g.id == session.target.graphic)
        {
            session.show(&self.tab.doc, graphic);
        }
    }

    pub(super) fn profile_animation_action(&mut self, action: Action) -> Task<Message> {
        if matches!(action, Action::Toggle | Action::Step(_))
            && self.selected_energy_profile().is_some()
        {
            self.xtb_end_preview();
        }
        self.sync_profile_animation();
        if !self.profile_animation_allowed() {
            return Task::none();
        }
        let previous = self.profile_animation.take();
        let Some(graphic) = self.selected_energy_profile() else {
            return Task::none();
        };
        let Some(profile) = graphic.energy_profile.as_deref() else {
            return Task::none();
        };
        let target = self.profile_animation_target(graphic);
        let mut session = previous.unwrap_or_else(|| {
            Session::new(
                target,
                &self.tab.doc,
                graphic,
                profile,
                self.energy_profile_paths(profile),
            )
        });
        let len = session.timeline.len();
        match action {
            Action::Toggle => session.position.toggle(self.appearance.playback_mode, len),
            Action::Motion(motion) => {
                if motion == Motion::Steps || session.paths.iter().any(Option::is_some) {
                    session.motion = motion;
                    session.timeline = Timeline::new(profile, &session.paths, motion);
                    let playing = session.position.playing;
                    session.position.seek(0, session.timeline.len());
                    session.position.playing = playing && session.timeline.len() > 1;
                }
            }
            Action::Highlight(value) => session.highlight = value,
            Action::Step(direction) if !session.position.playing => {
                session.position.seek(
                    session
                        .position
                        .frame
                        .saturating_add_signed(direction as isize),
                    len,
                );
            }
            _ => {}
        }
        session.show(&self.tab.doc, graphic);
        let reveal = matches!(action, Action::Toggle)
            && session.position.playing
            && (!self.inspector_open || self.inspector_tab != super::InspectorTab::Properties);
        self.profile_animation = Some(session);
        if reveal {
            return self
                .show_inspector_tab(super::InspectorTab::Properties)
                .unwrap_or_else(Task::none);
        }
        Task::none()
    }

    pub(super) fn profile_animation_controls(
        &self,
        profile: &EnergyProfile,
    ) -> Element<'_, Message> {
        let active = self.current_profile_animation();
        let allowed = self.profile_animation_allowed();
        let playing = active.is_some_and(|s| s.position.playing);
        let label = if playing { "Pause" } else { "Play" };
        let buttons = row![
            accessible_command(
                "energy-profile-play",
                format!("{label} energy profile"),
                label
            )
            .on_press_maybe(allowed.then_some(Message::ProfileAnimation(Action::Toggle)))
        ];
        #[cfg(not(target_arch = "wasm32"))]
        let buttons = buttons.push(
            accessible_command(
                "energy-profile-export",
                "Export energy profile animation",
                "Export animation…",
            )
            .on_press_maybe(
                allowed.then_some(Message::GifExport(super::gif_export::Action::OpenProfile)),
            ),
        );
        let smooth = active.map_or_else(
            || {
                self.energy_profile_paths(profile)
                    .iter()
                    .any(Option::is_some)
            },
            |s| s.paths.iter().any(Option::is_some),
        );
        let motions = if smooth {
            &[Motion::Steps, Motion::Smooth][..]
        } else {
            &[Motion::Steps][..]
        };
        let motion = active.map_or(Motion::Steps, |s| s.motion);
        let repeat = self.appearance.playback_mode;
        let speed = self.appearance.playback_speed;
        let readout = active
            .and_then(|s| s.timeline.frame(s.position.frame))
            .map_or_else(
                || timeline::readout(profile, 0, None, None),
                |frame| frame.readout(profile),
            );
        let motion_control = select(
            "energy-profile-motion",
            "Motion",
            motions.to_vec(),
            motion,
            |motion| Message::ProfileAnimation(Action::Motion(motion)),
        );
        let repeat_control = select(
            "energy-profile-repeat",
            "Repeat",
            PlaybackMode::ALL.to_vec(),
            repeat,
            |mode| Message::Xtb(xtb::Action::Ts(ts::Action::IrcMode(mode))),
        );
        let speed_control = select(
            "energy-profile-speed",
            "Speed",
            PlaybackSpeed::ALL.to_vec(),
            speed,
            |speed| Message::Xtb(xtb::Action::Ts(ts::Action::IrcSpeed(speed))),
        );
        column![
            buttons.spacing(6).wrap().vertical_spacing(4),
            column![
                field("Motion", motion_control),
                field("Repeat", repeat_control),
                field("Speed", speed_control)
            ]
            .spacing(4),
            checkbox(active.is_none_or(|s| s.highlight))
                .label("Highlight on the scheme")
                .text_size(12)
                .on_toggle(|on| Message::ProfileAnimation(Action::Highlight(on))),
            focus::scope(
                READOUT_ID,
                allowed && !playing,
                |d| Message::ProfileAnimation(Action::Step(d)),
                text(readout).size(12)
            ),
        ]
        .spacing(6)
        .into()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn profile_export_source(&mut self) -> Option<super::gif_export::Source> {
        use super::gif_export::{Motion as ExportMotion, Source};
        if !self.profile_animation_allowed() {
            return None;
        }
        if let Some(session) = &mut self.profile_animation {
            session.position.playing = false;
        }
        let graphic = self.selected_energy_profile()?;
        let profile = graphic.energy_profile.as_deref()?;
        let motion = self
            .current_profile_animation()
            .map_or(Motion::Steps, |s| s.motion);
        let timeline = self
            .current_profile_animation()
            .map(|s| s.timeline.clone())
            .unwrap_or_else(|| Timeline::new(profile, &[], Motion::Steps));
        Some(Source {
            name: "energy-profile".into(),
            doc: self.tab.doc.clone(),
            model: 0,
            motion: ExportMotion::Profile {
                id: graphic.id,
                timeline,
                motion,
            },
            mode: self.appearance.playback_mode,
            speed: self.appearance.playback_speed.factor(),
        })
    }
}

fn field<'a>(label: &'static str, content: Element<'a, Message>) -> Element<'a, Message> {
    row![text(label).size(11).width(54), content]
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .into()
}

fn select<T: Clone + PartialEq + ToString + 'static>(
    id: &'static str,
    label: &'static str,
    choices: Vec<T>,
    current: T,
    send: fn(T) -> Message,
) -> Element<'static, Message> {
    reshiki::accessibility::select(
        id,
        label,
        choices.clone(),
        current.to_string(),
        send,
        crate::appearance::pick_list(choices, Some(current), send)
            .text_size(12)
            .padding([4, 6]),
    )
    .into()
}
