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

mod animation;

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
    /// Saved chemistry key for a later arrow, resolved from the shared intermediate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub irc: Vec<EnergyProfileSample>,
}

fn valid_source_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
    pub const MAX_STATES: usize = 65;

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
    /// States with their plotted role and any additional authored name.
    pub fn states_with_labels(
        &self,
    ) -> impl Iterator<Item = (&EnergyProfileState, String, Option<&str>)> {
        self.states
            .iter()
            .enumerate()
            .scan(0, move |intermediates, (index, state)| {
                let (role, short) = match state.kind {
                    EnergyProfileStateKind::Reactant => ("R".into(), "R"),
                    EnergyProfileStateKind::TransitionState if self.steps.len() == 1 => {
                        ("TS‡".into(), "TS1‡")
                    }
                    EnergyProfileStateKind::TransitionState => {
                        let step = self
                            .steps
                            .iter()
                            .position(|step| step.transition == Some(index));
                        (
                            step.map_or_else(|| "TS‡".into(), |step| format!("TS{}‡", step + 1)),
                            "TS‡",
                        )
                    }
                    EnergyProfileStateKind::Intermediate => {
                        *intermediates += 1;
                        (format!("I{intermediates}"), "I")
                    }
                    EnergyProfileStateKind::Product => ("P".into(), "P"),
                };
                let name = (state.label != role && state.label != short && !state.label.is_empty())
                    .then_some(state.label.as_str());
                Some((state, role, name))
            })
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
                .map(|s| {
                    s.source_key.as_ref().map_or(0, String::len)
                        + std::mem::size_of_val(s.irc.as_slice())
                })
                .sum::<usize>()
    }
    pub(super) fn validate(&self, g: &Graphic) -> Result<(), String> {
        if (!self.source_key.is_empty() || self.linked) && !valid_source_key(&self.source_key)
            || self.source.is_some_and(|id| id == 0 || id == u64::MAX)
            || self.method.len() > 128
            || self.method.chars().any(char::is_control)
            || !(2..=Self::MAX_STATES).contains(&self.states.len())
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
                    || s.source_key
                        .as_deref()
                        .is_some_and(|key| !valid_source_key(key))
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
        self.draw_without_picture(g, style, source, None)
    }
    fn draw_without_picture(
        &self,
        g: &Graphic,
        style: &DrawingStyle,
        source: &Document,
        skip: Option<usize>,
    ) -> (Vec<Part>, Vec<Annotation>, Vec<Primitive>) {
        let mut layout = Layout::with_frame(self, style, Some(g), Some(source));
        let (parts, labels) = layout.ink(g);
        let side = layout.picture_world_side(g);
        let scale = side / layout.picture_side;
        if !scale.is_finite() || scale <= 0. {
            return (parts, labels, vec![]);
        }
        let change_bond_width = source.drawing_style.line_width_pt * 0.85;
        let outline_width = (side / 400.).clamp(0.6, 1.1);
        let pictures = std::mem::take(&mut layout.pictures)
            .into_iter()
            .filter(|slot| Some(slot.state) != skip)
            .flat_map(|slot| {
                let centre = layout.world(g, slot.centre);
                slot.artwork.into_iter().map(move |mut primitive| {
                    place_picture_ink(
                        &mut primitive,
                        centre,
                        slot.ink_centre,
                        scale,
                        change_bond_width,
                        outline_width,
                    );
                    primitive
                })
            })
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
            .map_or_else(Default::default, |p| {
                Layout::with_frame(p, style, Some(self), None).ink(self)
            })
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
    /// View-only playback leaves this model slot for the animated overlay.
    pub fn energy_profile_scene_without_picture(
        &self,
        style: &DrawingStyle,
        source: &Document,
        state: usize,
    ) -> (Vec<Part>, Vec<Annotation>, Vec<Primitive>) {
        self.energy_profile
            .as_deref()
            .map_or_else(Default::default, |p| {
                p.draw_without_picture(self, style, source, Some(state))
            })
    }
}

struct Label {
    text: String,
    centre: Point,
    size: f32,
    angle: f32,
}
impl Label {
    fn new(text: String, centre: Point, size: f32) -> Self {
        Self {
            text,
            centre,
            size,
            angle: 0.,
        }
    }
    fn half_size(&self, font: &TextStyle) -> (f32, f32) {
        let w = style::styled_text_width(&self.text, self.size, font) / 2.;
        let h = self.size * TextFormat::default().line_spacing / 2.;
        if self.angle == 0. { (w, h) } else { (h, w) }
    }
    fn bounds(&self, font: &TextStyle) -> (Point, Point) {
        let (w, h) = self.half_size(font);
        (self.centre.offset(-w, -h), self.centre.offset(w, h))
    }
}
struct PictureSlot {
    state: usize,
    centre: Point,
    half_size: Point,
    ink_centre: Point,
    artwork: Vec<Primitive>,
}
impl PictureSlot {
    fn bounds(&self) -> (Point, Point) {
        let Point { x, y } = self.half_size;
        (self.centre.offset(-x, -y), self.centre.offset(x, y))
    }
}
struct Layout {
    width: f32,
    height: f32,
    min_y: f32,
    font: TextStyle,
    states: Vec<Point>,
    labels: Vec<Label>,
    pictures: Vec<PictureSlot>,
    lines: Vec<Vec<Point>>,
    curves: Vec<Vec<PathCommand>>,
    bar: f32,
    line_width_pt: f32,
    bar_width_pt: f32,
    picture_side: f32,
    low: f64,
    high: f64,
    top: f32,
    bottom: f32,
}
impl Layout {
    fn new(profile: &EnergyProfile, style: &DrawingStyle) -> Self {
        Self::with_frame(profile, style, None, None)
    }
    fn with_frame(
        profile: &EnergyProfile,
        style: &DrawingStyle,
        g: Option<&Graphic>,
        source: Option<&Document>,
    ) -> Self {
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
        let axis_title = format!("{} ({})", profile.energy_kind.label(), profile.unit.label());
        let axis = em * 2.4
            + tick_texts
                .iter()
                .map(|s| width_of(s, tick_size))
                .fold(0., f32::max);
        let captions: Vec<_> = profile
            .states_with_labels()
            .map(|(state, role, name)| {
                (
                    state,
                    format!("{role} {}", profile.unit.number(state.energy_kcal)),
                    name,
                )
            })
            .collect();
        let value_size = em * 0.9;
        let slot = captions
            .iter()
            .map(|(_, value, name)| {
                width_of(value, value_size).max(width_of(name.unwrap_or(""), em))
            })
            .fold(0., f32::max);
        let slots = profile.states.len().max(1) as f32;
        let slot = (slot + em * 1.2)
            .max(em * 9.)
            .max(style.bond_length_world * 6.);
        let plot_left = axis + em * 0.3;
        let plot_width = slot * slots;
        // Keep a gap even in long, multistep profiles.
        let bar = (plot_width * 0.11).min(slot * 0.55);
        let picture_side = bar * 1.5;
        let plot_right = plot_left + plot_width;
        let bottom = (em * 9.).max(width_of(&axis_title, tick_size));
        let factor = profile.unit.convert_kcal(1.);
        let line_width_pt = g.map_or(style.line_width_pt, |g| g.style.width_pt).min(12.);
        let mut layout = Self {
            width: plot_right + em * 0.4,
            height: 0.,
            min_y: 0.,
            font: font.clone(),
            states: vec![],
            labels: vec![],
            pictures: vec![],
            lines: vec![],
            curves: vec![vec![]; profile.states.len().saturating_sub(1)],
            bar,
            line_width_pt,
            bar_width_pt: (2. * line_width_pt).min(12.),
            picture_side,
            low: low / factor,
            high: high / factor,
            top: 0.,
            bottom,
        };
        let highest_tick = layout.energy_y(ticks.last().copied().unwrap_or(high) / factor);
        let lowest_tick = layout.energy_y(ticks.first().copied().unwrap_or(low) / factor);
        let mut title = Label::new(
            axis_title,
            Point::new(em * 0.8, (highest_tick + lowest_tick) / 2.),
            tick_size,
        );
        title.angle = -90.;
        layout.labels.push(title);
        layout.lines.push(vec![
            Point::new(axis, highest_tick),
            Point::new(axis, lowest_tick),
        ]);
        for (&tick, text) in ticks.iter().zip(tick_texts) {
            let y = layout.energy_y(tick / factor);
            layout
                .lines
                .push(vec![Point::new(axis - em * 0.25, y), Point::new(axis, y)]);
            let x = axis - em * 0.5 - width_of(&text, tick_size) / 2.;
            layout
                .labels
                .push(Label::new(text, Point::new(x, y), tick_size));
        }
        layout.states = profile
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Point::new(
                    plot_left + (i as f32 + 0.5) * slot,
                    layout.energy_y(s.energy_kcal),
                )
            })
            .collect();
        for step in &profile.steps {
            if let Some(ts) = step.transition {
                layout.connect(step.from, ts, &step.irc, 0., 0.5);
                layout.connect(ts, step.to, &step.irc, 0.5, 1.);
            } else {
                layout.connect(step.from, step.to, &step.irc, 0., 1.);
            }
        }
        let default_source;
        let source = if let Some(source) = source {
            source
        } else {
            default_source = Document {
                drawing_style: style.clone(),
                ..Document::default()
            };
            &default_source
        };
        let projection = g.and_then(frame_projection);
        let project_half_size = |half: Point| {
            projection.map_or(half, |[x, y]| {
                // The picture renders upright and scales to fit the frame.
                let extent = |row: Point| {
                    let (x, y) = (row.x.abs(), row.y.abs());
                    (x * half.x + y * half.y) / (x + y)
                };
                Point::new(extent(x), extent(y))
            })
        };
        for (i, ((state, value, name), level)) in captions
            .into_iter()
            .zip(layout.states.iter().copied())
            .enumerate()
        {
            let x = level.x;
            let clear = |width, up| layout.clear_y(i, level, width / 2. + em * 0.15, up);
            let mut value = Label::new(value, Point::new(x, 0.), value_size);
            let value_width = width_of(&value.text, value_size);
            value.centre.y = clear(value_width, true) - em * 0.9;
            let mut name = name.map(|text| {
                Label::new(
                    text.into(),
                    Point::new(x, clear(width_of(text, em), false) + em),
                    em,
                )
            });
            if let Some(geometry) = state.geometry.as_ref()
                && profile.models != EnergyProfileModels::Off
                && let Some(artwork) =
                    geometry_picture(geometry, profile.rotation, picture_side, source)
            {
                let (lo, hi) = crate::scene::bounds(&artwork);
                let ink_centre = lo.lerp(hi, 0.5);
                // Scene bounds include a four-point export margin. Keep just
                // enough extra for the bond outline, which those bounds omit.
                let pad = style::DEFAULT.world(4.) - 0.55;
                let half = project_half_size(Point::new(
                    ((hi.x - lo.x) / 2. - pad).min(picture_side / 2.),
                    ((hi.y - lo.y) / 2. - pad).min(picture_side / 2.),
                ));
                let gap = em * 0.25;
                let model_y = if profile.models == EnergyProfileModels::RowAbove {
                    layout.top - em * 1.8 - picture_side / 2.
                } else if state.kind == EnergyProfileStateKind::TransitionState {
                    let (_, h) = value.half_size(&font);
                    value.centre.y = value
                        .centre
                        .y
                        .min(clear(half.x * 2., true) - em * 0.4 + gap + h);
                    value.centre.y - h - gap - half.y
                } else {
                    let label = if let Some(name) = &mut name {
                        name
                    } else {
                        value.centre.y = clear(value_width, false) + em * 0.9;
                        &mut value
                    };
                    let (_, h) = label.half_size(&font);
                    label.centre.y = label
                        .centre
                        .y
                        .max(clear(half.x * 2., false) + em * 0.4 - gap - h);
                    label.centre.y + h + gap + half.y
                };
                layout.pictures.push(PictureSlot {
                    state: i,
                    centre: Point::new(x, model_y),
                    half_size: half,
                    ink_centre,
                    artwork,
                });
            }
            layout.labels.push(value);
            layout.labels.extend(name);
        }
        let caption = (!profile.method.is_empty())
            .then(|| Label::new(profile.method.clone(), Point::default(), em * 0.72));
        let leftmost_tick = axis - em * 0.25;
        let rightmost_bar = plot_left + plot_width - (slot - bar) / 2.;
        layout.fit_frame(g, plot_right, leftmost_tick, rightmost_bar, caption);
        layout
    }
    fn label_and_model_bounds(&self) -> impl Iterator<Item = (Point, Point)> {
        self.labels
            .iter()
            .map(|l| l.bounds(&self.font))
            .chain(self.pictures.iter().map(PictureSlot::bounds))
    }
    fn fit_frame(
        &mut self,
        g: Option<&Graphic>,
        plot_right: f32,
        leftmost_tick: f32,
        rightmost_bar: f32,
        caption: Option<Label>,
    ) {
        const MAX_STROKE_INSET: f32 = 0.25;

        let gap = self.font.size() * 0.3;
        let axis_top = self.lines.first().and_then(|line| line.first()).copied();
        let projection = g
            .and_then(frame_projection)
            .map(|[x, y]| Point::new(x.x.hypot(x.y), y.x.hypot(y.y)));
        if let Some(projection) = projection {
            // Preserve horizontal room for the axis and bars. In height, leave
            // at least half the frame for the plot, text, and model slots.
            let horizontal = (leftmost_tick.min(self.width - rightmost_bar) - gap) / self.width;
            let max_half = (horizontal / projection.x).min(MAX_STROKE_INSET / projection.y);
            self.fit_strokes(max_half);
        }
        let (top, bottom) = self
            .label_and_model_bounds()
            .fold((self.top, self.bottom), |(top, bottom), (a, b)| {
                (top.min(a.y), bottom.max(b.y))
            });
        let bar_half = style::DEFAULT.world(self.bar_width_pt) / 2.;
        let fixed_edges = (top - gap, bottom + gap);
        let stroked_edges = (self.top - gap, self.bottom + gap);
        let bar_half_y = projection.map_or(bar_half, |p| {
            // The local stroke inset grows with the height of the fitted
            // layout. Solve for each possible pair of limiting edges.
            let fraction = (p.y * bar_half).min(MAX_STROKE_INSET);
            let height = (fixed_edges.1 - fixed_edges.0)
                .max(
                    (stroked_edges.1 - fixed_edges.0).max(fixed_edges.1 - stroked_edges.0)
                        / (1. - fraction),
                )
                .max((stroked_edges.1 - stroked_edges.0) / (1. - 2. * fraction));
            fraction * height
        });
        self.min_y = fixed_edges.0.min(stroked_edges.0 - bar_half_y);
        self.height = fixed_edges.1.max(stroked_edges.1 + bar_half_y) - self.min_y;
        if let Some((mut caption, axis_top)) = caption.zip(axis_top) {
            let bar_half_local = Point::new(
                bar_half * projection.map_or(1., |p| self.width * p.x),
                bar_half_y,
            );
            let ratio = self.line_width_pt / self.bar_width_pt;
            let line_half = Point::new(bar_half_local.x * ratio, bar_half_local.y * ratio);
            if !self.place_method(
                &mut caption,
                axis_top,
                plot_right,
                line_half,
                bar_half_local,
            ) {
                // Make a clear top-right corner inside the plot when both
                // corners are busy. Reserve the horizontal stroke inset at
                // the new width, where a world-space stroke takes more room.
                let (w, h) = caption.half_size(&self.font);
                let frame_margin = self.width - plot_right;
                let mut width = self.width + 2. * w + 2. * gap;
                width = projection.map_or(width + bar_half, |p| {
                    width / (1. - (p.x * bar_half).min(MAX_STROKE_INSET))
                });
                self.width = width;
                if let Some(p) = projection {
                    self.fit_strokes((leftmost_tick - gap) / (width * p.x));
                }
                caption.centre = Point::new(width - frame_margin - gap - w, axis_top.y + gap + h);
            }
            self.labels.push(caption);
        }
    }
    fn fit_strokes(&mut self, max_half: f32) {
        const MIN_STROKE_PT: f32 = 0.1;
        let scale = (2. * max_half / style::DEFAULT.world(self.bar_width_pt)).min(1.);
        self.line_width_pt = (self.line_width_pt * scale).max(MIN_STROKE_PT);
        self.bar_width_pt = (self.bar_width_pt * scale).max(MIN_STROKE_PT);
    }
    fn place_method(
        &self,
        caption: &mut Label,
        axis_top: Point,
        plot_right: f32,
        line_half: Point,
        bar_half: Point,
    ) -> bool {
        use crate::atom_labels::segment_hits_rect;

        let (w, h) = caption.half_size(&self.font);
        let em = self.font.size();
        let inset = em * 0.3;
        let clearance = em * 0.12;
        let is_clear = |centre: Point| {
            let (lo, hi) = (
                centre.offset(-w - clearance, -h - clearance),
                centre.offset(w + clearance, h + clearance),
            );
            let hits = |a, b, half: Point| {
                segment_hits_rect(a, b, lo.offset(-half.x, -half.y), hi.offset(half.x, half.y))
            };
            let hits_path = |path: &[Point]| {
                path.windows(2)
                    .any(|p| matches!(p, [a, b] if hits(*a, *b, line_half)))
            };
            self.label_and_model_bounds()
                .all(|(a, b)| a.x >= hi.x || b.x <= lo.x || a.y >= hi.y || b.y <= lo.y)
                && !self.lines.iter().any(|p| hits_path(p))
                && !self
                    .curves
                    .iter()
                    .flat_map(|p| super::flattened(p))
                    .any(|p| hits_path(&p))
                && !self.states.iter().any(|p| {
                    hits(
                        p.offset(-self.bar / 2., 0.),
                        p.offset(self.bar / 2., 0.),
                        bar_half,
                    )
                })
        };
        let y = axis_top.y + inset + h;
        let left = axis_top.x + inset.max(line_half.x + 2. * clearance);
        caption.centre = Point::new(left + w, y);
        if left + 2. * w <= plot_right - inset && is_clear(caption.centre) {
            return true;
        }
        caption.centre = Point::new(plot_right - inset - w, y);
        caption.centre.x - w >= axis_top.x && is_clear(caption.centre)
    }
    fn energy_y(&self, energy: f64) -> f32 {
        self.top + (self.bottom - self.top) * ((self.high - energy) / (self.high - self.low)) as f32
    }
    fn connect(&mut self, from: usize, to: usize, irc: &[EnergyProfileSample], lo: f64, hi: f64) {
        let (Some(&a), Some(&b)) = (self.states.get(from), self.states.get(to)) else {
            return;
        };
        let a = a.offset(self.bar / 2., 0.);
        let b = b.offset(-self.bar / 2., 0.);
        let mut path = vec![PathCommand::Move(a)];
        if irc.is_empty() {
            let mx = (a.x + b.x) / 2.;
            path.push(PathCommand::Cubic(
                Point::new(mx, a.y),
                Point::new(mx, b.y),
                b,
            ));
        } else {
            let trace = irc
                .iter()
                .filter(|s| lo < s.x && s.x < hi)
                .map(|s| ((s.x - lo) / (hi - lo), self.energy_y(s.energy_kcal)));
            blend_irc_ends(&mut path, a, b, trace);
        }
        if let Some(curve) = self.curves.get_mut(from) {
            *curve = path;
        }
    }
    /// The vertical edge of the curve within a label or model's column.
    fn clear_y(&self, state: usize, Point { x, y }: Point, half_width: f32, up: bool) -> f32 {
        if half_width <= self.bar / 2. {
            return y;
        }
        let (left, right) = (x - half_width, x + half_width);
        let extend: fn(f32, f32) -> f32 = if up { f32::min } else { f32::max };
        let start = state.saturating_sub(1);
        let end = (state + 1).min(self.curves.len());
        self.curves
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .flat_map(|curve| super::flattened(curve))
            .fold(y, |edge, path| {
                path.windows(2).fold(edge, |edge, pair| {
                    let [a, b] = pair else { return edge };
                    if b.x < left || a.x > right {
                        return edge;
                    }
                    let y_at = |x| a.y + (b.y - a.y) * ((x - a.x) / (b.x - a.x));
                    let y_left = if a.x < left { y_at(left) } else { a.y };
                    let y_right = if b.x > right { y_at(right) } else { b.y };
                    extend(edge, extend(y_left, y_right))
                })
            })
    }
    fn world(&self, g: &Graphic, p: Point) -> Point {
        let (x, y) = (p.x / self.width, (p.y - self.min_y) / self.height);
        g.origin.offset(
            x * g.axis_x.x + y * g.axis_y.x,
            x * g.axis_x.y + y * g.axis_y.y,
        )
    }
    fn text_scale(&self, g: &Graphic, label: &Label, angle: f32) -> f32 {
        let scale = (g.axis_x.x.hypot(g.axis_x.y) / self.width)
            .min(g.axis_y.x.hypot(g.axis_y.y) / self.height);
        // Text stays upright relative to one frame axis. In a skewed frame,
        // fit its rotated rectangle within the label's local slot.
        frame_projection(g).map_or(scale, |[x, y]| {
            let (w, h) = label.half_size(&self.font);
            let project = |row: Point| {
                let row = row.turned(Point::default(), -angle);
                w * row.x.abs() + h * row.y.abs()
            };
            scale
                .min(w / (self.width * project(x)))
                .min(h / (self.height * project(y)))
        })
    }
    fn picture_world_side(&self, g: &Graphic) -> f32 {
        let span = |a: Point| f64::from(a.x.abs()) + f64::from(a.y.abs());
        let den =
            (f64::from(self.width) * span(g.axis_y)).max(f64::from(self.height) * span(g.axis_x));
        (f64::from(self.picture_side) * f64::from(g.frame_determinant().abs()) / den) as f32
    }
    fn annotation(&self, g: &Graphic, label: &Label) -> Annotation {
        let angle = text_angle(g.axis_x, g.axis_y);
        let mut a = Annotation {
            id: g.id,
            position: Point::default(),
            text: label.text.clone(),
            format: TextFormat {
                style: TextStyle {
                    size_pt: label.size
                        * style::DEFAULT.points_per_world()
                        * self.text_scale(g, label, angle),
                    color: g.style.stroke,
                    ..self.font.clone()
                },
                ..TextFormat::default()
            },
            angle: (angle + label.angle).rem_euclid(360.),
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
                    self.line_width_pt,
                )
            })
            .collect();
        parts.extend(self.curves.iter().filter(|p| !p.is_empty()).map(|p| {
            line_part(
                g,
                p.iter().map(|p| p.map(|p| self.world(g, p))).collect(),
                self.line_width_pt,
            )
        }));
        parts.extend(self.states.iter().map(|s| {
            let mut part = line_part(
                g,
                vec![
                    PathCommand::Move(self.world(g, s.offset(-self.bar / 2., 0.))),
                    PathCommand::Line(self.world(g, s.offset(self.bar / 2., 0.))),
                ],
                self.bar_width_pt,
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

fn blend_irc_ends(
    path: &mut Vec<PathCommand>,
    a: Point,
    b: Point,
    trace: impl Iterator<Item = (f64, f32)>,
) {
    const BLEND: f64 = 0.15;
    let right = 1. - BLEND;
    let x_at = |t: f64| a.x + (b.x - a.x) * t as f32;
    let handle = (b.x - a.x) * (BLEND / 3.) as f32;
    let mut previous = (0., a.y);
    for (t1, y1) in trace.chain(std::iter::once((1., b.y))) {
        let (t0, y0) = previous;
        previous = (t1, y1);
        let on_trace = |t| {
            let y = if t == t1 {
                y1
            } else {
                y0 + (y1 - y0) * ((t - t0) / (t1 - t0)) as f32
            };
            Point::new(x_at(t), y)
        };
        // The inner handles follow the trace tangent and stay within its segment
        // so the joins cannot overshoot the sampled energies.
        if t0 < BLEND && BLEND <= t1 {
            path.push(PathCommand::Cubic(
                a.offset(handle, 0.),
                on_trace(t0.max(BLEND - BLEND / 3.)),
                on_trace(BLEND),
            ));
        }
        if BLEND < t1 && t0 < right {
            path.push(PathCommand::Line(on_trace(t1.min(right))));
        }
        if t0 <= right && right < t1 {
            path.push(PathCommand::Cubic(
                on_trace(t1.min(right + BLEND / 3.)),
                b.offset(-handle, 0.),
                b,
            ));
        }
    }
}
/// Rows of the inverse frame map for world-space offsets.
fn frame_projection(g: &Graphic) -> Option<[Point; 2]> {
    let det = g.frame_determinant();
    (det.is_finite() && det != 0.).then(|| {
        [
            Point::new(g.axis_y.y / det, -g.axis_y.x / det),
            Point::new(-g.axis_x.y / det, g.axis_x.x / det),
        ]
    })
}
fn line_part(g: &Graphic, commands: Vec<PathCommand>, width_pt: f32) -> Part {
    Part {
        commands,
        style: GraphicStyle {
            stroke: g.style.stroke,
            width_pt,
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
            let [x, y, _] = xyz;
            Some(Placed {
                id: doc.add_atom(
                    crate::editing::ELEMENTS.get(usize::from(z).checked_sub(1)?)?,
                    Point::new(x * 30., y * 30.),
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
    let origin = models3d::frame_origin(&doc, &model)?;
    model.offset = Point::new(-origin.x, -origin.y);
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
                // The model fits inside 88% of the slot. The cross extends
                // another 3.5% on each side, leaving room for its stroke.
                width_pt: (source.drawing_style.line_width_pt * 0.85)
                    .min(side * 0.05 * style::DEFAULT.points_per_world()),
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
    out.extend(models3d::primitives(&doc).into_iter().map(|mut primitive| {
        // The shared model renderer uses a minimum outline width. Keep that
        // outline within the slot even when the profile is compressed.
        if let Primitive::Sphere {
            outline: Some(outline),
            ..
        }
        | Primitive::ShadedQuad {
            outline: Some(outline),
            ..
        } = &mut primitive
        {
            outline.width = outline.width.min(side * 0.12);
        }
        primitive
    }));
    (!out.is_empty()).then_some(out)
}

/// Move and scale the measured ball-and-stick artwork into its frame slot.
fn place_picture_ink(
    primitive: &mut Primitive,
    centre: Point,
    ink: Point,
    scale: f32,
    change_bond_width: f32,
    outline_width: f32,
) {
    let place = |p: Point| centre.offset((p.x - ink.x) * scale, (p.y - ink.y) * scale);
    match primitive {
        Primitive::Opacity { primitive, .. } => {
            place_picture_ink(
                primitive,
                centre,
                ink,
                scale,
                change_bond_width,
                outline_width,
            );
        }
        Primitive::Path {
            commands, style, ..
        } => {
            for command in commands {
                *command = command.map(place);
            }
            style.width_pt = (style.width_pt * scale).min(change_bond_width);
        }
        Primitive::Sphere {
            center,
            radius,
            focal,
            outline,
            contact,
            ..
        } => {
            *center = place(*center);
            *radius *= scale;
            *focal = place(*focal);
            if let Some(outline) = outline {
                outline.width = (outline.width * scale).min(outline_width);
            }
            if let Some(contact) = contact {
                for p in [&mut contact.region, &mut contact.rim, &mut contact.seams]
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    *p = place(*p);
                }
            }
        }
        Primitive::ShadedQuad {
            corners,
            cap,
            from,
            to,
            outline,
            ..
        } => {
            for p in corners.iter_mut().chain(cap) {
                *p = place(*p);
            }
            *from = place(*from);
            *to = place(*to);
            if let Some(outline) = outline {
                outline.width = (outline.width * scale).min(outline_width * 0.8);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
