//! Ordered state holds and sampled movement through the saved IRC.
#[cfg(not(target_arch = "wasm32"))]
use super::PlaybackMode;
use super::{PATH_FRAME_SECONDS, ProfileIrc};
use crate::app::xtb::ts::profile::IrcSample;
use reshiki::graphics::{
    EnergyProfile, EnergyProfileGeometry, EnergyProfileModels, EnergyProfileSample,
    EnergyProfileStateKind as Kind,
};
use serde::{Deserialize, Serialize};

const STATE_SECONDS: f32 = 0.8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    #[default]
    Steps,
    Smooth,
}

impl Motion {
    pub(super) fn available(self, smooth: bool) -> Self {
        if smooth { self } else { Self::Steps }
    }
}

impl std::fmt::Display for Motion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Steps => "Step through states",
            Self::Smooth => "Smooth along IRC",
        })
    }
}

#[derive(Clone, Copy)]
pub(in crate::app) struct Frame {
    pub state: usize,
    pub seconds: f32,
    coordinate: Option<f64>,
    irc: Option<(usize, IrcSample)>,
}

impl Frame {
    pub fn sample(self) -> Option<(usize, EnergyProfileSample)> {
        self.irc.map(|(step, sample)| (step, sample.point))
    }

    pub(super) fn readout(self, profile: &EnergyProfile) -> String {
        readout(
            profile,
            self.state,
            self.sample().map(|(_, p)| p.energy_kcal),
            self.coordinate,
        )
    }
}

#[derive(Clone)]
pub(in crate::app) struct Timeline {
    frames: Vec<Frame>,
    paths: Vec<Option<ProfileIrc>>,
}

impl Timeline {
    pub(in crate::app) fn new(
        profile: &EnergyProfile,
        paths: &[Option<ProfileIrc>],
        motion: Motion,
    ) -> Self {
        let mut states: Vec<_> = (0..profile.states.len())
            .map(|state| {
                vec![Frame {
                    state,
                    seconds: STATE_SECONDS,
                    coordinate: None,
                    irc: None,
                }]
            })
            .collect();
        if motion == Motion::Steps {
            return Self {
                frames: states.into_iter().flatten().collect(),
                paths: vec![],
            };
        }
        for (step_index, (step, path)) in profile.steps.iter().zip(paths).enumerate() {
            let (Some(path), Some(ts)) = (path, step.transition) else {
                continue;
            };
            if let Some(frame) = states.get_mut(step.from).and_then(|s| s.first_mut()) {
                frame.coordinate = path.samples.first().map(|p| p.coordinate);
            }
            if let Some(frame) = states.get_mut(step.to).and_then(|s| s.first_mut()) {
                frame.coordinate = path.samples.last().map(|p| p.coordinate);
            }
            for &sample in &path.samples {
                let x = sample.point.x;
                if x == 0.5 {
                    if let Some(frame) = states.get_mut(ts).and_then(|s| s.first_mut()) {
                        frame.seconds = PATH_FRAME_SECONDS;
                        frame.coordinate = Some(sample.coordinate);
                    }
                    continue;
                }
                let (after, state) = if x < 0.5 {
                    (step.from, if x < 0.25 { step.from } else { ts })
                } else {
                    (ts, if x < 0.75 { ts } else { step.to })
                };
                if let Some(frames) = states.get_mut(after) {
                    frames.push(Frame {
                        state,
                        seconds: PATH_FRAME_SECONDS,
                        coordinate: Some(sample.coordinate),
                        irc: Some((step_index, sample)),
                    });
                }
            }
        }
        Self {
            frames: states.into_iter().flatten().collect(),
            paths: paths.to_vec(),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn frame(&self, i: usize) -> Option<Frame> {
        self.frames.get(i).copied()
    }

    pub fn geometry(&self, frame: Frame) -> Option<EnergyProfileGeometry> {
        let (step, sample) = frame.irc?;
        self.paths.get(step)?.as_ref()?.geometry(sample.frame)
    }

    pub(super) fn picture_slot(&self, profile: &EnergyProfile, frame: Frame) -> Option<usize> {
        if profile.models == EnergyProfileModels::Off {
            return None;
        }
        let (step, _) = frame.irc?;
        let state = profile.steps.get(step)?.transition?;
        profile.states.get(state)?.geometry.as_ref()?;
        Some(state)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn sequence(&self, fps: usize, repeat: PlaybackMode, speed: f32) -> Vec<usize> {
        let forward = 0..self.frames.len();
        let backward = if repeat == PlaybackMode::BackAndForth {
            1..self.frames.len().saturating_sub(1)
        } else {
            0..0
        };
        let mut elapsed = 0.;
        let mut sequence = vec![];
        for index in forward.chain(backward.rev()) {
            let Some(frame) = self.frame(index) else {
                continue;
            };
            elapsed += f64::from(frame.seconds) * fps as f64 / f64::from(speed);
            let count = (elapsed.round() as usize).saturating_sub(sequence.len());
            sequence.extend(std::iter::repeat_n(
                index,
                count.max(usize::from(frame.irc.is_none())),
            ));
        }
        sequence
    }
}

pub(super) fn readout(
    profile: &EnergyProfile,
    index: usize,
    energy: Option<f64>,
    coordinate: Option<f64>,
) -> String {
    let Some(state) = profile.states.get(index) else {
        return String::new();
    };
    let name = match (state.kind, state.label.as_str()) {
        (Kind::Reactant, "" | "R") => "Reactant",
        (Kind::Product, "" | "P") => "Product",
        (Kind::Intermediate, "" | "I") => "Intermediate",
        (Kind::TransitionState, "") => "TS‡",
        (_, label) => label,
    };
    let mut line = format!(
        "{name} · {}",
        profile.unit.format(energy.unwrap_or(state.energy_kcal))
    );
    if let Some(s) = coordinate {
        let s = if s.abs() < 0.05 { 0. } else { s };
        let s = format!("{s:.1}").replace('-', "−");
        line.push_str(&format!(" · s = {s} amu½·bohr"));
    }
    line
}
