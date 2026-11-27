//! Frame-by-frame playback and reaction-coordinate readouts.
use crate::appearance::EnergyUnit;
use reshiki::editing::ELEMENTS;
use reshiki::models3d::{Generated, Model3d, Placed};
use reshiki::qcdata::{IrcEndpoint, IrcPath, KCAL_PER_HARTREE};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlaybackMode {
    #[default]
    Loop,
    BackAndForth,
    Once,
}

impl PlaybackMode {
    pub(crate) const ALL: [Self; 3] = [Self::Loop, Self::BackAndForth, Self::Once];
}

impl fmt::Display for PlaybackMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Loop => "Loop",
            Self::BackAndForth => "Back and forth",
            Self::Once => "Play once",
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlaybackSpeed {
    Half,
    #[default]
    Normal,
    Double,
}

impl PlaybackSpeed {
    pub(crate) const ALL: [Self; 3] = [Self::Half, Self::Normal, Self::Double];

    pub(crate) fn factor(self) -> f32 {
        match self {
            Self::Half => 0.5,
            Self::Normal => 1.0,
            Self::Double => 2.0,
        }
    }
}

impl fmt::Display for PlaybackSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Half => "½×",
            Self::Normal => "1×",
            Self::Double => "2×",
        })
    }
}

pub(crate) const PATH_FRAME_SECONDS: f32 = 0.08;

pub(crate) struct Playback {
    pub(crate) frame: usize,
    pub(crate) playing: bool,
    reverse: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            frame: 0,
            playing: true,
            reverse: false,
        }
    }
}

impl Playback {
    pub(crate) fn seek(&mut self, frame: usize, len: usize) {
        self.frame = frame.min(len.saturating_sub(1));
        self.playing = false;
        self.reverse = false;
    }

    pub(crate) fn toggle(&mut self, mode: PlaybackMode, len: usize) {
        self.playing = !self.playing && len > 1;
        if self.playing && mode == PlaybackMode::Once && self.frame == len - 1 {
            self.frame = 0;
        }
    }

    pub(crate) fn advance(&mut self, mode: PlaybackMode, len: usize) -> bool {
        if !self.playing {
            return false;
        }
        if len < 2 {
            self.playing = false;
            return false;
        }
        let previous = self.frame;
        let last = len - 1;
        match mode {
            PlaybackMode::Loop => {
                self.reverse = false;
                self.frame = (self.frame + 1) % len;
            }
            PlaybackMode::BackAndForth => {
                if self.frame == last {
                    self.reverse = true;
                } else if self.frame == 0 {
                    self.reverse = false;
                }
                if self.reverse {
                    self.frame -= 1;
                } else {
                    self.frame += 1;
                }
            }
            PlaybackMode::Once => {
                self.reverse = false;
                self.frame = (self.frame + 1).min(last);
                self.playing = self.frame < last;
            }
        }
        previous != self.frame
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PathSample {
    pub(crate) s: f64,
    pub(crate) energy_kcal: f64,
    endpoint: Option<IrcEndpoint>,
}

impl PathSample {
    pub(crate) fn from_irc(path: &IrcPath, reference: f64, frame: usize) -> Option<Self> {
        let [negative, positive] = path.reached;
        Some(Self {
            s: *path.arc_lengths_sqrt_amu_bohr.get(frame)?,
            energy_kcal: (path.trajectory.energies_hartree.get(frame)? - reference)
                * KCAL_PER_HARTREE,
            endpoint: match frame.cmp(&path.ts_index) {
                Ordering::Less => Some(negative),
                Ordering::Equal => None,
                Ordering::Greater => Some(positive),
            },
        })
    }

    pub(crate) fn readout(self, unit: EnergyUnit, frame: usize, len: usize) -> String {
        let s = if self.s.abs() < 0.05 { 0.0 } else { self.s };
        let s = format!("{s:.1}").replace('-', "−");
        format!(
            "s = {s} amu½·bohr · ΔE {} · frame {} / {len}",
            unit.format(self.energy_kcal),
            frame + 1,
        )
    }

    pub(crate) fn direction(self) -> &'static str {
        match self.endpoint {
            Some(IrcEndpoint::Reactant) => "toward reactant",
            Some(IrcEndpoint::Product) => "toward product",
            Some(IrcEndpoint::Other) => "toward another minimum",
            None => "at the transition state",
        }
    }
}

/// Use the IRC covalent graph, including bonds to transferred hydrogens.
pub(crate) fn set_irc_topology(
    model: &mut Model3d,
    numbers: &[u8],
    angstrom: impl Iterator<Item = f64>,
) -> Result<(), String> {
    let count = model.atoms.len() + model.hydrogens.len();
    if numbers.len() != count {
        return Err("The animation's bond graph does not match its atoms".into());
    }
    let bohr: Vec<_> = angstrom
        .map(|x| x / reshiki_optimize::ANGSTROM_PER_BOHR)
        .collect();
    let bonds = reshiki_optimize::ts::bonds(numbers, &bohr).map_err(|e| e.to_string())?;
    let sources = model.frame_sources();
    // The perceived graph and element numbers use calculation frame order.
    // Store the endpoint geometry in that order too, including imported H.
    model.atoms = model
        .frame_positions()
        .into_iter()
        .enumerate()
        .map(|(index, xyz)| Placed {
            id: index as u64,
            xyz,
        })
        .collect();
    model.hydrogens.clear();
    model.frame_atom_order.clear();
    model.generated = Some(Generated {
        sources,
        elements: numbers
            .iter()
            .map(|&z| ELEMENTS.get(usize::from(z) - 1).unwrap_or(&"?").to_string())
            .collect(),
        bonds: bonds.into_iter().map(|(a, b)| [a, b]).collect(),
    });
    model.fingerprint = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_modes_and_restart() {
        for (mode, expected) in [
            (PlaybackMode::Loop, [1, 2, 0, 1, 2]),
            (PlaybackMode::BackAndForth, [1, 2, 1, 0, 1]),
            (PlaybackMode::Once, [1, 2, 2, 2, 2]),
        ] {
            let mut play = Playback::default();
            assert_eq!(
                std::array::from_fn(|_| {
                    play.advance(mode, 3);
                    play.frame
                }),
                expected
            );
            if mode == PlaybackMode::Once {
                assert!(!play.playing);
                play.toggle(mode, 3);
                assert_eq!(play.frame, 0);
                assert!(play.playing);
            }
            play.seek(99, 3);
            assert!(!play.advance(mode, 3));
            assert_eq!(play.frame, 2);
        }
    }

    #[test]
    fn readout_uses_the_energy_unit_and_signed_coordinate() {
        let point = PathSample {
            s: -2.4,
            energy_kcal: 18.6,
            endpoint: Some(IrcEndpoint::Reactant),
        };
        assert_eq!(
            point.readout(EnergyUnit::KcalPerMol, 22, 97),
            "s = −2.4 amu½·bohr · ΔE 18.6 kcal/mol · frame 23 / 97"
        );
        assert!(
            point
                .readout(EnergyUnit::KjPerMol, 22, 97)
                .contains("77.8 kJ/mol")
        );
    }

    #[test]
    fn direction_uses_the_reached_endpoint_on_each_side_of_the_saddle() {
        use IrcEndpoint::{Other, Product, Reactant};

        let mut path = IrcPath {
            trajectory: reshiki::qcdata::Trajectory {
                energies_hartree: vec![0.; 3],
                numbers: vec![1],
                isotopes: vec![],
                masses_amu: vec![1.],
                charge: 0,
                multiplicity: 2,
                positions_angstrom: vec![],
                labels: vec![],
                minimizer_energies_hartree: vec![],
                cluster_sizes: vec![],
                rmsd_to_best_angstrom: vec![],
            },
            arc_lengths_sqrt_amu_bohr: vec![-1., 0., 0.],
            neb_arc_lengths_sqrt_amu_bohr: vec![],
            ts_index: 1,
            reached: [Reactant, Other],
            relaxed_ends: None,
            completed: [true; 2],
        };
        for (reached, expected) in [
            (
                [Reactant, Other],
                ["toward reactant", "toward another minimum"],
            ),
            (
                [Other, Product],
                ["toward another minimum", "toward product"],
            ),
            ([Reactant, Reactant], ["toward reactant", "toward reactant"]),
            ([Product, Product], ["toward product", "toward product"]),
        ] {
            path.reached = reached;
            let direction = |frame| PathSample::from_irc(&path, 0., frame).unwrap().direction();
            assert_eq!([direction(0), direction(2)], expected);
            assert_eq!(direction(1), "at the transition state");
        }
    }
}
