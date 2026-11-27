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
use iced::time::Instant;
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
    Tick { deadline: Instant, now: Instant },
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
    deadline: Instant,
    speed: PlaybackSpeed,
}

impl Session {
    fn new(
        target: Target,
        doc: &Document,
        graphic: &Graphic,
        profile: &EnergyProfile,
        paths: Vec<Option<ProfileIrc>>,
        preference: Motion,
    ) -> Self {
        let motion = preference.preferred(
            paths.iter().any(Option::is_some),
            crate::platform::reduced_motion(),
        );
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
            deadline: Instant::now(),
            speed: PlaybackSpeed::Normal,
        }
    }

    fn hold(&self) -> Duration {
        self.timeline
            .frame(self.position.frame)
            .map_or(Duration::ZERO, |frame| {
                Duration::from_secs_f32(frame.seconds / self.speed.factor())
            })
    }

    fn start_hold(&mut self, speed: PlaybackSpeed, now: Instant) {
        self.speed = speed;
        self.deadline = now + self.hold();
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
        } else if let Some(session) = &mut self.profile_animation
            && session.speed != self.appearance.playback_speed
        {
            session.start_hold(self.appearance.playback_speed, Instant::now());
        }
        if self
            .profile_animation
            .as_ref()
            .is_some_and(|session| session.motion == Motion::Smooth)
        {
            self.reduce_profile_animation(crate::platform::reduced_motion());
        }
    }

    fn reduce_profile_animation(&mut self, reduced_motion: bool) {
        if reduced_motion
            && let Some(session) = &mut self.profile_animation
            && session.motion == Motion::Smooth
            && let Some(graphic) = self
                .tab
                .doc
                .graphics
                .iter()
                .find(|g| g.id == session.target.graphic)
            && let Some(profile) = graphic.energy_profile.as_deref()
        {
            let state = session
                .timeline
                .frame(session.position.frame)
                .map_or(0, |frame| frame.state);
            let playing = session.position.playing;
            session.motion = Motion::Steps;
            session.timeline = Timeline::new(profile, &session.paths, Motion::Steps);
            session.position.seek(state, session.timeline.len());
            session.position.playing = playing && session.timeline.len() > 1;
            session.start_hold(self.appearance.playback_speed, Instant::now());
            session.show(&self.tab.doc, graphic);
        }
    }

    pub(super) fn retain_transformed_profile_animation(&mut self, before: &Document) {
        let Some(id) = self.current_profile_animation().map(|s| s.target.graphic) else {
            return;
        };
        let doc = &mut self.tab.doc;
        let Some((index, graphic)) = doc
            .graphics
            .iter_mut()
            .enumerate()
            .find(|(_, g)| g.id == id)
        else {
            return;
        };
        let Some(old) = before.graphics.get(index) else {
            return;
        };
        let frame = |g: &Graphic| (g.origin, g.axis_x, g.axis_y, g.depth);
        let (current, previous) = (frame(graphic), frame(old));
        if current == previous {
            return;
        }
        // Compare the reconciled drawing with the profile's old frame in place.
        (
            graphic.origin,
            graphic.axis_x,
            graphic.axis_y,
            graphic.depth,
        ) = previous;
        let transform_only = *doc == *before;
        let Some(graphic) = doc.graphics.get_mut(index) else {
            return;
        };
        (
            graphic.origin,
            graphic.axis_x,
            graphic.axis_y,
            graphic.depth,
        ) = current;
        if transform_only
            && let Some(session) = &mut self.profile_animation
            && let Some(graphic) = doc.graphics.get(index)
        {
            session.target.revision = self.tab.revision.wrapping_add(1);
            session.show(doc, graphic);
        }
    }

    pub(super) fn pause_profile_animation(&mut self) {
        if let Some(session) = &mut self.profile_animation {
            session.position.playing = false;
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
        // The deadline restarts equal-duration desktop timers. Checking it again
        // on receipt also rejects WASM's immediate tick and queued older ticks.
        iced::time::every(session.hold())
            .with(session.deadline)
            .map(|(deadline, _)| {
                Message::ProfileAnimation(Action::Tick {
                    deadline,
                    now: Instant::now(),
                })
            })
    }

    pub(super) fn tick_profile_animation(&mut self, deadline: Instant, now: Instant) {
        self.sync_profile_animation();
        let Some(session) = &mut self.profile_animation else {
            return;
        };
        if session.deadline != deadline || now < deadline {
            return;
        }
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
            session.start_hold(self.appearance.playback_speed, now.max(Instant::now()));
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
                self.appearance.profile_motion,
            )
        });
        let len = session.timeline.len();
        let mut restart_hold = false;
        match action {
            Action::Toggle => {
                session.position.toggle(self.appearance.playback_mode, len);
                restart_hold = session.position.playing;
            }
            Action::Motion(motion) => {
                if motion == Motion::Steps
                    || !crate::platform::reduced_motion()
                        && session.paths.iter().any(Option::is_some)
                {
                    session.motion = motion;
                    session.timeline = Timeline::new(profile, &session.paths, motion);
                    let playing = session.position.playing;
                    session.position.seek(0, session.timeline.len());
                    session.position.playing = playing && session.timeline.len() > 1;
                    restart_hold = true;
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
                restart_hold = true;
            }
            _ => {}
        }
        session.show(&self.tab.doc, graphic);
        if let Action::Motion(motion) = action
            && session.motion == motion
        {
            self.appearance.profile_motion = motion;
            let _ = self.appearance.save();
        }
        if restart_hold {
            session.start_hold(self.appearance.playback_speed, Instant::now());
        }
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
        let smooth = !crate::platform::reduced_motion()
            && active.map_or_else(
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
        let motion = active.map_or(self.appearance.profile_motion.available(smooth), |s| {
            s.motion.available(smooth)
        });
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
        self.profile_export_source_with_motion(crate::platform::reduced_motion())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn profile_export_source_with_motion(
        &mut self,
        reduced_motion: bool,
    ) -> Option<super::gif_export::Source> {
        use super::gif_export::{Motion as ExportMotion, Source};
        self.sync_profile_animation();
        self.reduce_profile_animation(reduced_motion);
        if !self.profile_animation_allowed() {
            return None;
        }
        if let Some(session) = &mut self.profile_animation {
            session.position.playing = false;
        }
        let graphic = self.selected_energy_profile()?;
        let profile = graphic.energy_profile.as_deref()?;
        let (timeline, motion, highlight) = self.current_profile_animation().map_or_else(
            || {
                let paths = self.energy_profile_paths(profile);
                let motion = self
                    .appearance
                    .profile_motion
                    .preferred(paths.iter().any(Option::is_some), reduced_motion);
                (Timeline::new(profile, &paths, motion), motion, true)
            },
            |s| (s.timeline.clone(), s.motion, s.highlight),
        );
        Some(Source {
            name: "energy-profile".into(),
            doc: self.tab.doc.clone(),
            model: 0,
            motion: ExportMotion::Profile {
                id: graphic.id,
                timeline,
                motion,
                highlight,
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
