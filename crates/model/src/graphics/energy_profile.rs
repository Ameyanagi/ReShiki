//! A placed reaction energy profile, with owned geometries and vector artwork.
use reshiki_qcdata::{Frame, HARTREE_TO_EV, KCAL_PER_HARTREE};
use serde::{Deserialize, Serialize};

use super::{BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand};
use crate::{
    document::{Annotation, Document, Point, text_angle},
    models3d::{self, Display, ForceField, Model3d, Placed},
    palette::{Color, Hue, Row},
    scene::Primitive,
    scientific::Part,
    style::{self, DrawingStyle},
    typography::{TextFormat, TextStyle},
};

/// The same display unit used by result plots and placed figures.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyUnit {
    #[default]
    KcalPerMol,
    KjPerMol,
    Ev,
}
impl EnergyUnit {
    pub const ALL: [Self; 3] = [Self::KcalPerMol, Self::KjPerMol, Self::Ev];
    pub fn label(self) -> &'static str {
        match self {
            Self::KcalPerMol => "kcal/mol",
            Self::KjPerMol => "kJ/mol",
            Self::Ev => "eV",
        }
    }
    pub fn convert_kcal(self, energy: f64) -> f64 {
        match self {
            Self::KcalPerMol => energy,
            Self::KjPerMol => energy * 4.184,
            Self::Ev => energy / KCAL_PER_HARTREE * HARTREE_TO_EV,
        }
    }
    pub fn number(self, energy: f64) -> String {
        let (digits, half_digit) = if self == Self::Ev {
            (2, 0.005)
        } else {
            (1, 0.05)
        };
        let value = self.convert_kcal(energy);
        let value = if value.abs() < half_digit { 0. } else { value };
        format!("{value:.digits$}").replace('-', "−")
    }
    pub fn format(self, energy: f64) -> String {
        format!("{} {}", self.number(energy), self.label())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyProfileKind {
    #[default]
    Electronic,
    Gibbs,
}
impl EnergyProfileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Electronic => "ΔE",
            Self::Gibbs => "ΔG",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyProfileModels {
    #[default]
    AtLevels,
    RowAbove,
    Off,
}
impl EnergyProfileModels {
    pub const ALL: [Self; 3] = [Self::AtLevels, Self::RowAbove, Self::Off];
}
impl std::fmt::Display for EnergyProfileModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AtLevels => "At each level",
            Self::RowAbove => "In a row above",
            Self::Off => "Off",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyProfileStateKind {
    Reactant,
    TransitionState,
    Intermediate,
    Product,
}

/// Bond indices belong to the owned frame, never to drawing IDs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyProfileGeometry {
    pub frame: Frame,
    pub bonds: Vec<[usize; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forming: Vec<[usize; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaking: Vec<[usize; 2]>,
}
impl EnergyProfileGeometry {
    pub fn validate(&self) -> Result<(), String> {
        self.frame.validate().map_err(|e| e.to_string())?;
        let count = self.frame.numbers.len();
        if count == 0
            || count > 4096
            || !self.frame.drawing_atoms.is_empty()
            || self
                .frame
                .positions_angstrom
                .iter()
                .flatten()
                .any(|v| v.abs() > 1e6)
            || self.bonds.len() + self.forming.len() + self.breaking.len() > count * 12
            || self
                .bonds
                .iter()
                .chain(&self.forming)
                .chain(&self.breaking)
                .any(|&[a, b]| a == b || a >= count || b >= count)
        {
            return Err("Invalid energy profile geometry".into());
        }
        Ok(())
    }
    fn heap_bytes(&self) -> usize {
        self.frame.heap_bytes()
            + std::mem::size_of_val(self.bonds.as_slice())
            + std::mem::size_of_val(self.forming.as_slice())
            + std::mem::size_of_val(self.breaking.as_slice())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyProfileState {
    pub kind: EnergyProfileStateKind,
    pub label: String,
    /// Relative energy on the profile's common reference, in kcal/mol.
    pub energy_kcal: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<EnergyProfileGeometry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyProfileSample {
    /// Normalized coordinate of one step, from 0 to 1, with its TS at 0.5.
    pub x: f64,
    pub energy_kcal: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyProfileStep {
    /// State indices of the two minima, in reaction order.
    pub from: usize,
    pub to: usize,
    pub transition: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub irc: Vec<EnergyProfileSample>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyProfile {
    /// Source reaction arrow, when this figure follows the scheme.
    pub source: Option<u64>,
    /// Renumber-stable reaction identity supplied by the calculation workflow.
    pub source_key: String,
    pub linked: bool,
    pub method: String,
    pub energy_kind: EnergyProfileKind,
    pub unit: EnergyUnit,
    pub states: Vec<EnergyProfileState>,
    pub steps: Vec<EnergyProfileStep>,
    pub models: EnergyProfileModels,
    /// A shared molecular view, independent of the figure's affine frame.
    pub rotation: [f32; 4],
}
impl EnergyProfile {
    pub fn new(
        source: u64,
        source_key: String,
        method: String,
        states: Vec<EnergyProfileState>,
        steps: Vec<EnergyProfileStep>,
    ) -> Self {
        Self {
            source: Some(source),
            source_key,
            linked: true,
            method,
            energy_kind: EnergyProfileKind::Electronic,
            unit: EnergyUnit::default(),
            states,
            steps,
            models: EnergyProfileModels::default(),
            rotation: [1., 0., 0., 0.],
        }
    }
    pub fn unlink(&mut self) {
        self.source = None;
        self.linked = false;
    }
    /// Replace calculation data while retaining the user's molecular view and placement.
    /// The caller retains the Graphic frame and its line/text style.
    pub fn relink(&mut self, mut fresh: Self) {
        fresh.models = self.models;
        fresh.rotation = self.rotation;
        *self = fresh;
    }
    pub fn natural_size(&self, style: &DrawingStyle) -> (f32, f32) {
        let layout = Layout::new(self, style);
        (layout.width, layout.height)
    }
    pub(crate) fn heap_bytes(&self) -> usize {
        self.source_key.len()
            + self.method.len()
            + std::mem::size_of_val(self.states.as_slice())
            + std::mem::size_of_val(self.steps.as_slice())
            + self
                .states
                .iter()
                .map(|s| {
                    s.label.len()
                        + s.geometry
                            .as_ref()
                            .map_or(0, EnergyProfileGeometry::heap_bytes)
                })
                .sum::<usize>()
            + self
                .steps
                .iter()
                .map(|s| std::mem::size_of_val(s.irc.as_slice()))
                .sum::<usize>()
    }
    pub(super) fn validate(&self, g: &Graphic) -> Result<(), String> {
        let key_valid = self.source_key.len() == 64
            && self
                .source_key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if (!self.source_key.is_empty() || self.linked) && !key_valid
            || self.source.is_some_and(|id| id == 0 || id == u64::MAX)
            || self.method.len() > 128
            || self.method.chars().any(char::is_control)
            || !(2..=65).contains(&self.states.len())
            || self
                .states
                .first()
                .is_none_or(|s| s.kind != EnergyProfileStateKind::Reactant || s.energy_kcal != 0.)
            || self
                .states
                .last()
                .is_none_or(|s| s.kind != EnergyProfileStateKind::Product)
            || self.states.iter().any(|s| {
                !valid_energy(s.energy_kcal)
                    || s.label.len() > 256
                    || s.label.chars().any(char::is_control)
            })
        {
            return Err("Invalid energy profile states or source".into());
        }
        let minima: Vec<_> = self
            .states
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (s.kind != EnergyProfileStateKind::TransitionState).then_some(i))
            .collect();
        if self.states.iter().enumerate().any(|(i, s)| {
            i > 0
                && i + 1 < self.states.len()
                && !matches!(
                    s.kind,
                    EnergyProfileStateKind::TransitionState | EnergyProfileStateKind::Intermediate
                )
        }) || self.steps.len() != minima.len().saturating_sub(1)
            || self.steps.iter().zip(minima.windows(2)).any(|(s, pair)| {
                let &[from, to] = pair else {
                    return true;
                };
                s.from != from
                    || s.to != to
                    || match s.transition {
                        Some(ts) => {
                            ts != from + 1
                                || to != ts + 1
                                || self.states.get(ts).is_none_or(|s| {
                                    s.kind != EnergyProfileStateKind::TransitionState
                                })
                        }
                        None => to != from + 1,
                    }
                    || s.irc.len() > 16384
                    || s.irc.iter().any(|p| {
                        !p.x.is_finite()
                            || !(0. ..=1.).contains(&p.x)
                            || !valid_energy(p.energy_kcal)
                    })
                    || s.irc.windows(2).any(|p| matches!(p, [a,b] if a.x >= b.x))
            })
            || self.steps.iter().map(|s| s.irc.len()).sum::<usize>() > 65536
        {
            return Err("Invalid energy profile steps or IRC".into());
        }
        for geometry in self.states.iter().filter_map(|s| s.geometry.as_ref()) {
            geometry.validate()?;
        }
        let norm = self.rotation.iter().map(|v| v * v).sum::<f32>();
        if self.rotation.iter().any(|v| !v.is_finite())
            || (norm - 1.).abs() > 1e-3
            || self.heap_bytes() > 64 << 20
        {
            return Err("Invalid energy profile molecular view or size".into());
        }
        let x = g.axis_x.x.hypot(g.axis_x.y);
        let y = g.axis_y.x.hypot(g.axis_y.y);
        let det = g.frame_determinant();
        if !x.is_finite()
            || !y.is_finite()
            || x <= 1e-3
            || y <= 1e-3
            || !det.is_finite()
            || f64::from(det.abs()) <= f64::from(x) * f64::from(y) * 1e-6
            || !g.frame_corners().into_iter().all(Point::is_finite)
        {
            return Err("An energy profile needs a finite frame with two independent axes".into());
        }
        Ok(())
    }
    pub(crate) fn draw(
        &self,
        g: &Graphic,
        style: &DrawingStyle,
        source: &Document,
    ) -> (Vec<Part>, Vec<Annotation>, Vec<Primitive>) {
        let layout = Layout::new(self, style);
        let (parts, labels) = layout.ink(g);
        let side = layout.picture_world_side(g);
        let pictures = layout
            .pictures
            .iter()
            .filter_map(|slot| {
                let geometry = self.states.get(slot.state)?.geometry.as_ref()?;
                geometry_picture(
                    geometry,
                    self.rotation,
                    layout.world(g, slot.centre),
                    side,
                    source,
                )
            })
            .flatten()
            .collect();
        (parts, labels, pictures)
    }
}
fn valid_energy(v: f64) -> bool {
    v.is_finite() && v.abs() <= 1e12
}

impl Graphic {
    pub fn energy_profile(
        id: u64,
        origin: Point,
        size: Point,
        profile: EnergyProfile,
        style: GraphicStyle,
    ) -> Self {
        let mut g = Self::dragged(
            id,
            GraphicKind::EnergyProfile,
            origin,
            origin.offset(size.x, size.y),
            style,
            BracketSides::Both,
            false,
        );
        g.origin = origin;
        g.axis_x = Point::new(size.x, 0.);
        g.axis_y = Point::new(0., size.y);
        g.energy_profile = Some(Box::new(profile));
        g
    }
    /// Publication vector paths and editable labels, without molecular pictures.
    pub fn energy_profile_parts(&self, style: &DrawingStyle) -> (Vec<Part>, Vec<Annotation>) {
        self.energy_profile
            .as_deref()
            .map_or_else(Default::default, |p| Layout::new(p, style).ink(self))
    }
    /// The same vector strokes, labels, and owned molecular artwork the canvas draws.
    pub fn energy_profile_scene(
        &self,
        style: &DrawingStyle,
        source: &Document,
    ) -> (Vec<Part>, Vec<Annotation>, Vec<Primitive>) {
        self.energy_profile
            .as_deref()
            .map_or_else(Default::default, |p| p.draw(self, style, source))
    }
}

struct Label {
    text: String,
    centre: Point,
    size: f32,
}
struct StateSlot {
    point: Point,
}
struct PictureSlot {
    state: usize,
    centre: Point,
}
struct Layout {
    width: f32,
    height: f32,
    font: TextStyle,
    states: Vec<StateSlot>,
    labels: Vec<Label>,
    pictures: Vec<PictureSlot>,
    lines: Vec<Vec<Point>>,
    curves: Vec<Vec<PathCommand>>,
    bar: f32,
    picture_side: f32,
    low: f64,
    high: f64,
    top: f32,
    bottom: f32,
}
impl Layout {
    fn new(profile: &EnergyProfile, style: &DrawingStyle) -> Self {
        let font = style.text_style();
        let em = font.size();
        let tick_size = em * 0.82;
        let width_of = |text: &str, size| style::styled_text_width(text, size, &font);
        let energies = profile
            .states
            .iter()
            .map(|s| profile.unit.convert_kcal(s.energy_kcal))
            .chain(
                profile
                    .steps
                    .iter()
                    .flat_map(|s| &s.irc)
                    .map(|s| profile.unit.convert_kcal(s.energy_kcal)),
            );
        let (low, high, ticks) = nice_scale(energies);
        let tick_texts: Vec<_> = ticks.iter().map(|&v| tick_text(v)).collect();
        let axis_title = if profile.method.is_empty() {
            format!("{} ({})", profile.energy_kind.label(), profile.unit.label())
        } else {
            format!(
                "{} ({}) · {}",
                profile.energy_kind.label(),
                profile.unit.label(),
                profile.method
            )
        };
        let axis = em * 1.4
            + tick_texts
                .iter()
                .map(|s| width_of(s, tick_size))
                .fold(em, f32::max);
        let names: Vec<_> = profile
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if s.label.is_empty() {
                    default_label(s.kind, i)
                } else {
                    s.label.clone()
                }
            })
            .collect();
        let values: Vec<_> = profile
            .states
            .iter()
            .map(|s| profile.unit.number(s.energy_kcal))
            .collect();
        let picture_side = style.bond_length_world * 2.2;
        let has_models = profile.models != EnergyProfileModels::Off
            && profile.states.iter().any(|s| s.geometry.is_some());
        let bar = names
            .iter()
            .chain(&values)
            .map(|s| width_of(s, em))
            .fold(em * 2.7, f32::max)
            .max(if has_models { picture_side } else { 0. })
            + em * 0.7;
        // Text and models fit within a level's horizontal span. Connecting
        // curves only occupy the gaps, so even nearly flat paths stay clear.
        let slot = bar + em * 2.7;
        let left = axis + em + bar / 2.;
        let width = (left + slot * profile.states.len().saturating_sub(1) as f32 + bar / 2. + em)
            .max(axis + em * 8.)
            .max(width_of(&axis_title, tick_size) + em * 2.);
        let row = has_models && profile.models == EnergyProfileModels::RowAbove;
        let at_levels = has_models && profile.models == EnergyProfileModels::AtLevels;
        let extra = if at_levels {
            picture_side + em * 0.6
        } else {
            0.
        };
        let row_height = if row { picture_side + em } else { 0. };
        let header = em * 2.5;
        let top = header + row_height + em * 2.1 + extra;
        let bottom = top + em * 9.;
        let height = bottom + em * 4.3 + extra;
        let factor = profile.unit.convert_kcal(1.);
        let mut layout = Self {
            width,
            height,
            font: font.clone(),
            states: vec![],
            labels: vec![],
            pictures: vec![],
            lines: vec![],
            curves: vec![],
            bar,
            picture_side,
            low: low / factor,
            high: high / factor,
            top,
            bottom,
        };
        layout.labels.push(Label {
            text: axis_title.clone(),
            centre: Point::new(em + width_of(&axis_title, tick_size) / 2., em * 0.75),
            size: tick_size,
        });
        layout.lines.push(vec![
            Point::new(axis, header + row_height),
            Point::new(axis, bottom + extra + em * 2.5),
        ]);
        for (&tick, text) in ticks.iter().zip(tick_texts) {
            let y = layout.energy_y(tick / factor);
            layout
                .lines
                .push(vec![Point::new(axis - em * 0.25, y), Point::new(axis, y)]);
            layout.labels.push(Label {
                centre: Point::new(axis - em * 0.5 - width_of(&text, tick_size) / 2., y),
                text,
                size: tick_size,
            });
        }
        layout.labels.push(Label {
            text: "Reaction coordinate".into(),
            centre: Point::new((axis + width) / 2., height - em * 0.7),
            size: tick_size,
        });
        for (i, state) in profile.states.iter().enumerate() {
            let x = left + i as f32 * slot;
            let y = layout.energy_y(state.energy_kcal);
            let up = state.kind == EnergyProfileStateKind::TransitionState;
            let sign = if up { -1. } else { 1. };
            layout.states.push(StateSlot {
                point: Point::new(x, y),
            });
            layout.labels.push(Label {
                text: names.get(i).cloned().unwrap_or_default(),
                centre: Point::new(x, y + sign * em * 1.05),
                size: em,
            });
            layout.labels.push(Label {
                text: values.get(i).cloned().unwrap_or_default(),
                centre: Point::new(x, y - sign * em * 1.05),
                size: em * 0.9,
            });
            if has_models && state.geometry.is_some() {
                let cy = if row {
                    header + picture_side / 2.
                } else {
                    y + sign * (em * 2. + picture_side / 2.)
                };
                layout.pictures.push(PictureSlot {
                    state: i,
                    centre: Point::new(x, cy),
                });
            }
        }
        for step in &profile.steps {
            if !step.irc.is_empty() {
                let mut samples = step.irc.clone();
                // State bars remain exact anchors. Scalar IRC samples between
                // them keep their assigned (possibly irregular) coordinates.
                samples.push(EnergyProfileSample {
                    x: 0.,
                    energy_kcal: profile.states.get(step.from).map_or(0., |s| s.energy_kcal),
                });
                samples.push(EnergyProfileSample {
                    x: 1.,
                    energy_kcal: profile.states.get(step.to).map_or(0., |s| s.energy_kcal),
                });
                if let Some(ts) = step.transition.and_then(|i| profile.states.get(i)) {
                    samples.push(EnergyProfileSample {
                        x: 0.5,
                        energy_kcal: ts.energy_kcal,
                    });
                }
                samples.sort_by(|a, b| a.x.total_cmp(&b.x));
                // Prefer the appended exact state when coordinates coincide.
                samples.reverse();
                samples.dedup_by(|a, b| a.x == b.x);
                samples.reverse();
                let mut path = vec![];
                for pair in samples.windows(2) {
                    let [a, b] = pair else { continue };
                    let second = (a.x + b.x) / 2. > 0.5;
                    let Some(pa) = layout.irc_point(step, *a, second) else {
                        continue;
                    };
                    let Some(pb) = layout.irc_point(step, *b, second) else {
                        continue;
                    };
                    path.push(PathCommand::Move(pa));
                    path.push(PathCommand::Line(pb));
                }
                layout.curves.push(path);
            } else {
                let indices: Vec<_> = std::iter::once(step.from)
                    .chain(step.transition)
                    .chain(std::iter::once(step.to))
                    .collect();
                let mut path = vec![];
                for pair in indices.windows(2) {
                    let [a, b] = pair else { continue };
                    let Some(a) = layout.states.get(*a) else {
                        continue;
                    };
                    let Some(b) = layout.states.get(*b) else {
                        continue;
                    };
                    let a = a.point.offset(bar / 2., 0.);
                    let b = b.point.offset(-bar / 2., 0.);
                    let mx = (a.x + b.x) / 2.;
                    path.push(PathCommand::Move(a));
                    path.push(PathCommand::Cubic(
                        Point::new(mx, a.y),
                        Point::new(mx, b.y),
                        b,
                    ));
                }
                layout.curves.push(path);
            }
        }
        layout
    }
    fn energy_y(&self, energy: f64) -> f32 {
        self.top + (self.bottom - self.top) * ((self.high - energy) / (self.high - self.low)) as f32
    }
    fn irc_point(
        &self,
        step: &EnergyProfileStep,
        sample: EnergyProfileSample,
        second: bool,
    ) -> Option<Point> {
        let from = self.states.get(step.from)?.point;
        let to = self.states.get(step.to)?.point;
        let energy = sample.energy_kcal;
        // Reserve the full level spans, including their labels, around the
        // exact R/TS/P anchors. Each branch is drawn only in the free gap.
        let (a, b, t) = if let Some(ts) = step.transition.and_then(|i| self.states.get(i)) {
            if !second {
                (from, ts.point, sample.x * 2.)
            } else {
                (ts.point, to, (sample.x - 0.5) * 2.)
            }
        } else {
            (from, to, sample.x)
        };
        let left = a.x + self.bar / 2.;
        let right = b.x - self.bar / 2.;
        let t = t as f32;
        Some(Point::new(
            left * (1. - t) + right * t,
            self.energy_y(energy),
        ))
    }
    fn world(&self, g: &Graphic, p: Point) -> Point {
        let (x, y) = (p.x / self.width, p.y / self.height);
        g.origin.offset(
            x * g.axis_x.x + y * g.axis_y.x,
            x * g.axis_x.y + y * g.axis_y.y,
        )
    }
    fn text_scale(&self, g: &Graphic) -> f32 {
        (g.axis_x.x.hypot(g.axis_x.y) / self.width).min(g.axis_y.x.hypot(g.axis_y.y) / self.height)
    }
    fn picture_world_side(&self, g: &Graphic) -> f32 {
        let span = |a: Point| f64::from(a.x.abs()) + f64::from(a.y.abs());
        let den =
            (f64::from(self.width) * span(g.axis_y)).max(f64::from(self.height) * span(g.axis_x));
        (f64::from(self.picture_side) * f64::from(g.frame_determinant().abs()) / den) as f32
    }
    fn annotation(&self, g: &Graphic, label: &Label) -> Annotation {
        let mut a = Annotation {
            id: g.id,
            position: Point::default(),
            text: label.text.clone(),
            format: TextFormat {
                style: TextStyle {
                    size_pt: label.size * style::DEFAULT.points_per_world() * self.text_scale(g),
                    color: g.style.stroke,
                    ..self.font.clone()
                },
                ..TextFormat::default()
            },
            angle: text_angle(g.axis_x, g.axis_y),
        };
        let centre = self.world(g, label.centre);
        let (w, h) = a.size();
        a.position = centre.offset(-w / 2., -h / 2.);
        a
    }
    fn ink(&self, g: &Graphic) -> (Vec<Part>, Vec<Annotation>) {
        let mut parts: Vec<_> = self
            .lines
            .iter()
            .map(|p| {
                line_part(
                    g,
                    p.iter()
                        .enumerate()
                        .map(|(i, &p)| {
                            if i == 0 {
                                PathCommand::Move(self.world(g, p))
                            } else {
                                PathCommand::Line(self.world(g, p))
                            }
                        })
                        .collect(),
                    0.7,
                )
            })
            .collect();
        parts.extend(self.curves.iter().map(|p| {
            line_part(
                g,
                p.iter().map(|p| p.map(|p| self.world(g, p))).collect(),
                0.8,
            )
        }));
        parts.extend(self.states.iter().map(|s| {
            let mut part = line_part(
                g,
                vec![
                    PathCommand::Move(self.world(g, s.point.offset(-self.bar / 2., 0.))),
                    PathCommand::Line(self.world(g, s.point.offset(self.bar / 2., 0.))),
                ],
                1.4,
            );
            part.style.flat_caps = true;
            part
        }));
        (
            parts,
            self.labels.iter().map(|l| self.annotation(g, l)).collect(),
        )
    }
}
fn default_label(kind: EnergyProfileStateKind, index: usize) -> String {
    match kind {
        EnergyProfileStateKind::Reactant => "R".into(),
        EnergyProfileStateKind::TransitionState => format!("TS{}‡", index.div_ceil(2)),
        EnergyProfileStateKind::Intermediate => format!("I{}", index / 2),
        EnergyProfileStateKind::Product => "P".into(),
    }
}
fn line_part(g: &Graphic, commands: Vec<PathCommand>, weight: f32) -> Part {
    Part {
        commands,
        style: GraphicStyle {
            stroke: g.style.stroke,
            width_pt: g.style.width_pt * weight,
            pattern: g.style.pattern,
            flat_caps: g.style.flat_caps,
            ..GraphicStyle::default()
        },
        filled: false,
    }
}

/// Round in the displayed unit, including eV and small near-flat energy spans.
fn nice_scale(values: impl Iterator<Item = f64>) -> (f64, f64, Vec<f64>) {
    let (mut low, mut high) = values
        .filter(|v| v.is_finite())
        .fold((0_f64, 0_f64), |(lo, hi), v| (lo.min(v), hi.max(v)));
    if high - low < 1e-9 {
        low -= 0.5;
        high += 0.5;
    }
    let raw = (high - low) / 4.;
    let magnitude = 10_f64.powf(raw.log10().floor());
    let fraction = raw / magnitude;
    let step = magnitude
        * if fraction <= 1. {
            1.
        } else if fraction <= 2. {
            2.
        } else if fraction <= 5. {
            5.
        } else {
            10.
        };
    let low = (low / step).floor() * step;
    let high = (high / step).ceil() * step;
    let count = ((high - low) / step).round() as usize;
    (
        low,
        high,
        (0..=count.min(12)).map(|i| low + i as f64 * step).collect(),
    )
}
fn tick_text(value: f64) -> String {
    if value.abs() < 1e-10 {
        return "0".into();
    }
    let text = if value.abs() >= 1e6 || value.abs() < 1e-4 {
        format!("{value:.1e}")
    } else {
        format!("{value:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .into()
    };
    text.replace('-', "−")
}

fn geometry_picture(
    geometry: &EnergyProfileGeometry,
    rotation: [f32; 4],
    centre: Point,
    side: f32,
    source: &Document,
) -> Option<Vec<Primitive>> {
    if !side.is_finite() || side <= 0. {
        return None;
    }
    let mut doc = Document {
        canvas_theme: source.canvas_theme,
        color_theme: source.color_theme,
        custom_theme: source.custom_theme.clone(),
        drawing_style: source.drawing_style.clone(),
        ..Document::default()
    };
    let positions: Vec<_> = geometry
        .frame
        .positions_angstrom
        .iter()
        .map(|p| p.map(|v| v as f32))
        .collect();
    let atoms = geometry
        .frame
        .numbers
        .iter()
        .zip(&positions)
        .map(|(&z, &xyz)| {
            Some(Placed {
                id: doc.add_atom(
                    crate::editing::ELEMENTS.get(usize::from(z).checked_sub(1)?)?,
                    Point::new(xyz[0] * 30., xyz[1] * 30.),
                ),
                xyz,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let changed = |a: usize, b: usize| {
        geometry
            .forming
            .iter()
            .chain(&geometry.breaking)
            .any(|&[x, y]| (x == a && y == b) || (x == b && y == a))
    };
    for &[a, b] in &geometry.bonds {
        if !changed(a, b) {
            doc.add_bond(atoms.get(a)?.id, atoms.get(b)?.id, 1, "plain");
        }
    }
    let mut model = Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s);
    model.display = Display::ThreeD;
    model.rotation = rotation;
    model.size = side;
    model.perspective = 0.;
    model.depth_fade = 0.16;
    let (lo, hi) = models3d::frame(&doc, &model)?;
    model.offset = Point::new(centre.x - (lo.x + hi.x) / 2., centre.y - (lo.y + hi.y) / 2.);
    let projected = models3d::project(&doc, &model, &positions)?;
    let mut out = vec![];
    for (pairs, hue, crossed) in [
        (&geometry.forming, Hue::Green, false),
        (&geometry.breaking, Hue::Red, true),
    ] {
        for &[a, b] in pairs {
            let (a, b) = (*projected.get(a)?, *projected.get(b)?);
            let style = GraphicStyle {
                stroke: Color::Palette(hue, Row::Strong),
                width_pt: source.drawing_style.line_width_pt * 0.85,
                pattern: LinePattern::Dashed,
                ..GraphicStyle::default()
            };
            out.push(Primitive::Path {
                commands: vec![PathCommand::Move(a), PathCommand::Line(b)],
                style: style.clone(),
                filled: false,
            });
            if crossed {
                let c = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
                let d = side * 0.035;
                out.push(Primitive::Path {
                    commands: vec![
                        PathCommand::Move(c.offset(-d, -d)),
                        PathCommand::Line(c.offset(d, d)),
                        PathCommand::Move(c.offset(-d, d)),
                        PathCommand::Line(c.offset(d, -d)),
                    ],
                    style: GraphicStyle {
                        pattern: LinePattern::Solid,
                        ..style
                    },
                    filled: false,
                });
            }
        }
    }
    doc.models.push(model);
    out.extend(models3d::primitives(&doc));
    Some(out)
}

#[cfg(test)]
mod tests;
