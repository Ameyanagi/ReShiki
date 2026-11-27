//! Reusable transport controls for reaction-path animations.
use crate::app::Message;
use crate::app::animation::{PathSample, PlaybackMode, PlaybackSpeed};
use crate::app::workspace::accessible_command;
use crate::appearance::EnergyUnit;
use iced::widget::{column, row, slider, text};
use iced::{Element, Length};

pub(crate) struct Actions {
    pub toggle: Message,
    pub seek: fn(usize) -> Message,
    pub step: fn(i32) -> Message,
    pub mode: fn(PlaybackMode) -> Message,
    pub speed: fn(PlaybackSpeed) -> Message,
}

pub(crate) struct State {
    pub frame: usize,
    pub len: usize,
    pub playing: bool,
    pub active: bool,
    pub enabled: bool,
    pub initial_label: &'static str,
    pub mode: PlaybackMode,
    pub speed: PlaybackSpeed,
}

pub(crate) fn view(
    prefix: &'static str,
    state: State,
    sample: PathSample,
    unit: EnergyUnit,
    actions: Actions,
    chart: Element<'static, Message>,
) -> Element<'static, Message> {
    let label = if state.playing {
        "Pause"
    } else if state.active {
        "Play"
    } else {
        state.initial_label
    };
    let play = accessible_command(prefix, label, label)
        .on_press_maybe(state.enabled.then_some(actions.toggle));
    let mode = reshiki::accessibility::select(
        format!("{prefix}-mode"),
        "Playback mode",
        PlaybackMode::ALL.to_vec(),
        state.mode.to_string(),
        actions.mode,
        crate::appearance::pick_list(PlaybackMode::ALL, Some(state.mode), actions.mode)
            .text_size(12)
            .padding(6),
    );
    let speed = reshiki::accessibility::select(
        format!("{prefix}-speed"),
        "Playback speed",
        PlaybackSpeed::ALL.to_vec(),
        state.speed.to_string(),
        actions.speed,
        crate::appearance::pick_list(PlaybackSpeed::ALL, Some(state.speed), actions.speed)
            .text_size(12)
            .padding(6),
    );
    let transport = row![play, mode, speed]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .wrap();
    let readout = format!(
        "{} · {}",
        sample.readout(unit, state.frame, state.len),
        sample.direction()
    );
    let mut timeline = column![chart, text(readout).size(12)].spacing(6);
    if state.enabled && state.len > 1 {
        let seek = actions.seek;
        timeline = timeline.push(
            slider(
                0..=(state.len - 1) as u32,
                state.frame as u32,
                move |frame| seek(frame as usize),
            )
            .step(1_u32)
            .width(Length::Fill),
        );
    }
    column![
        super::focus::scope(
            format!("{prefix}-path"),
            !state.playing && state.enabled,
            actions.step,
            timeline
        ),
        transport
    ]
    .spacing(6)
    .into()
}
