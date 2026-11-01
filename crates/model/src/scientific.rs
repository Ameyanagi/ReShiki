//! Chemical marks and orbital diagrams. Charge/radical marks use the atom's
//! chemical state; partial charges and lone pairs are attached annotations.
use crate::{
    document::{Atom, Point},
    graphics::{Graphic, GraphicKind, GraphicStyle, PathCommand},
    style::DEFAULT,
};
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;

mod label_halo;
mod mark_placement;
#[cfg(test)]
mod mark_placement_tests;
#[cfg(test)]
mod orbital_tests;
#[cfg(test)]
mod symbol_tests;
pub(crate) use label_halo::orbital_halos;
pub use label_halo::{LABEL_HALO_PT, OrbitalHalos, label_halo_radius};

/// The atom labels orbitals reach, from the document's own label ink.
/// Editable exchange and hit-testing stack them as the scene does.
pub fn orbital_label_halos(doc: &crate::document::Document) -> OrbitalHalos {
    let opacity = crate::rear_opacity::Paint::new(doc);
    orbital_halos(doc, &doc.graphics, |atom| {
        if doc.atom_visible(atom.id) && opacity.atom(atom.id) > 0. {
            crate::scene::atom_label_ink_boxes(atom, doc)
        } else {
            Vec::new()
        }
    })
}

macro_rules! choices {
    ($name:ident { $first:ident => $label:literal $(,$variant:ident => $text:literal)* $(,)? }) => {
        #[derive(Debug,Clone,Copy,Default,PartialEq,Eq,Serialize,Deserialize)]
        #[serde(rename_all="snake_case")]
        pub enum $name {#[default] $first,$($variant),*}
        impl $name {pub const ALL:&'static [Self]=&[Self::$first,$(Self::$variant),*];}
        impl std::fmt::Display for $name {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str(match self {Self::$first=>$label,$(Self::$variant=>$text),*})}}
    }
}
choices!(SymbolKind {CirclePlus=>"Circled plus",CircleMinus=>"Circled minus",Plus=>"Plus",Minus=>"Minus",Radical=>"Radical",Diradical=>"Diradical",LonePair=>"Lone pair",LonePairBar=>"Lone pair bar",RadicalCation=>"Radical cation",RadicalAnion=>"Radical anion",HydrogenDot=>"H-dot",HydrogenDash=>"H-dash",Diamond=>"Attachment diamond",Star=>"Attachment star",Wavy=>"Wavy attachment",Bead=>"Polymer bead",Dagger=>"Dagger",DoubleDagger=>"Double dagger",PartialPositive=>"Partial positive",PartialNegative=>"Partial negative"});
impl SymbolKind {
    pub fn partial_charge_text(self) -> Option<&'static str> {
        match self {
            Self::PartialPositive => Some("δ+"),
            Self::PartialNegative => Some("δ−"),
            _ => None,
        }
    }
}
choices!(OrbitalKind {S=>"s orbital",Sigma=>"σ orbital",Lobe=>"Single lobe",P=>"p orbital",Hybrid=>"sp³ hybrid",Dxy=>"dxy orbital",Dz2=>"dz² orbital"});
choices!(Phase {Solid=>"Solid phase",Open=>"Outline",Shaded=>"Gray phase"});
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Charge,
    CircledCharge,
    Radical,
    RadicalIon,
    LonePair,
    LonePairBar,
    PartialPositive,
    PartialNegative,
}
impl MarkKind {
    pub fn charge(self) -> bool {
        matches!(self, Self::Charge | Self::CircledCharge | Self::RadicalIon)
    }
    pub fn radical(self) -> bool {
        matches!(self, Self::Radical | Self::RadicalIon)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomMark {
    /// Stable within the parent atom; absent on legacy unanchored marks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub kind: MarkKind,
    pub offset: Point,
    #[serde(default)]
    pub angle: f32,
    #[serde(default)]
    pub size_pt: Option<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub commands: Vec<PathCommand>,
    pub style: GraphicStyle,
    pub filled: bool,
}
fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}
fn circle(x: f32, y: f32, rx: f32, ry: f32) -> Vec<PathCommand> {
    use PathCommand::*;
    let k = 0.5522848;
    vec![
        Move(p(x + rx, y)),
        Cubic(p(x + rx, y + k * ry), p(x + k * rx, y + ry), p(x, y + ry)),
        Cubic(p(x - k * rx, y + ry), p(x - rx, y + k * ry), p(x - rx, y)),
        Cubic(p(x - rx, y - k * ry), p(x - k * rx, y - ry), p(x, y - ry)),
        Cubic(p(x + k * rx, y - ry), p(x + rx, y - k * ry), p(x + rx, y)),
        Close,
    ]
}
fn lobe(angle: f32, length: f32, width: f32) -> Vec<PathCommand> {
    use PathCommand::*;
    let convert = |q: Point| {
        p(
            q.x * angle.cos() - q.y * angle.sin(),
            q.x * angle.sin() + q.y * angle.cos(),
        )
    };
    vec![
        Move(p(0., 0.)),
        Cubic(p(length * 0.36, -width), p(length, -width), p(length, 0.)),
        Cubic(p(length, width), p(length * 0.36, width), p(0., 0.)),
        Close,
    ]
    .into_iter()
    .map(|c| c.map(convert))
    .collect()
}
pub fn orbital_parts(
    kind: OrbitalKind,
    phase: Phase,
    flipped: bool,
    style: &GraphicStyle,
) -> Vec<Part> {
    let flipped = flipped
        && !matches!(
            kind,
            OrbitalKind::S | OrbitalKind::Sigma | OrbitalKind::Lobe
        );
    let mut out = vec![];
    let mut add = |commands, positive: bool| {
        let fill = if phase == Phase::Open || positive == flipped {
            None
        } else {
            let color = style.fill.unwrap_or(style.stroke);
            Some(if phase == Phase::Solid {
                color
            } else {
                // Scene documents hold canonical bytes, so this lightens toward the paper.
                crate::palette::Color::Custom(color.rgb().map(|c| ((c as u16 + 255 * 2) / 3) as u8))
            })
        };
        out.push(Part {
            commands,
            style: GraphicStyle {
                fill,
                ..style.clone()
            },
            filled: fill.is_some(),
        });
    };
    let pi = std::f32::consts::PI;
    match kind {
        OrbitalKind::S => add(circle(0., 0., 1., 1.), true),
        OrbitalKind::Sigma => add(circle(0., 0., 1., 0.46), true),
        OrbitalKind::Lobe => add(lobe(0., 1., 0.52), true),
        OrbitalKind::P => {
            add(lobe(0., 1., 0.5), true);
            add(lobe(pi, 1., 0.5), false);
        }
        OrbitalKind::Hybrid => {
            add(lobe(0., 1., 0.5), true);
            add(lobe(pi, 0.42, 0.23), false);
        }
        OrbitalKind::Dxy => {
            for i in 0..4 {
                add(lobe(pi * 0.25 + i as f32 * pi * 0.5, 1., 0.38), i % 2 == 0);
            }
        }
        OrbitalKind::Dz2 => {
            add(lobe(0., 1., 0.4), true);
            add(lobe(pi, 1., 0.4), true);
            add(circle(0., 0., 0.2, 0.65), false);
        }
    }
    out
}
pub fn symbol_parts(kind: SymbolKind, amount: u8, style: &GraphicStyle) -> Vec<Part> {
    glyph_parts(kind, amount, style, false)
}
/// An atom's circled charge keeps ChemDraw's nominal size but draws smaller: radius 0.40
/// and sign arms 0.24 of it. Plain signs and free symbols keep 0.52 and 0.3.
fn glyph_parts(kind: SymbolKind, amount: u8, style: &GraphicStyle, mark: bool) -> Vec<Part> {
    use PathCommand::*;
    if let Some(text) = kind.partial_charge_text() {
        let width = crate::style::text_width(text, 0.85);
        return vec![Part {
            commands: crate::style::outline_text(text, 0.85, p(-width * 0.5, 0.3)),
            style: GraphicStyle {
                fill: Some(style.stroke),
                width_pt: 0.,
                ..style.clone()
            },
            filled: true,
        }];
    }
    let circled = matches!(kind, SymbolKind::CirclePlus | SymbolKind::CircleMinus);
    let (radius, arm) = if mark && circled {
        (0.4, 0.24)
    } else {
        (0.52, 0.3)
    };
    let mut out = vec![];
    let mut add = |commands: Vec<PathCommand>, fill: bool| {
        out.push(Part {
            commands,
            style: GraphicStyle {
                fill: fill.then_some(style.stroke),
                ..style.clone()
            },
            filled: fill,
        })
    };
    let sign = |positive: bool, x: f32| {
        let mut v = vec![Move(p(x - arm, 0.)), Line(p(x + arm, 0.))];
        if positive {
            v.extend([Move(p(x, -arm)), Line(p(x, arm))]);
        }
        v
    };
    let charge = matches!(
        kind,
        SymbolKind::Plus
            | SymbolKind::Minus
            | SymbolKind::CirclePlus
            | SymbolKind::CircleMinus
            | SymbolKind::RadicalCation
            | SymbolKind::RadicalAnion
    );
    if charge {
        let positive = matches!(
            kind,
            SymbolKind::Plus | SymbolKind::CirclePlus | SymbolKind::RadicalCation
        );
        let ion = matches!(kind, SymbolKind::RadicalCation | SymbolKind::RadicalAnion);
        let x = if ion { 0.22 } else { 0. };
        add(sign(positive, x), false);
        if circled {
            add(circle(0., 0., radius, radius), false);
        }
        if ion {
            add(circle(-0.45, 0., 0.095, 0.095), true);
        }
        if amount > 1 {
            add(
                crate::style::outline_text(&amount.to_string(), 0.65, p(-0.75, 0.22)),
                true,
            );
        }
        return out;
    }
    match kind {
        SymbolKind::Radical => add(circle(0., 0., 0.11, 0.11), true),
        SymbolKind::Diradical | SymbolKind::LonePair => {
            let gap = if kind == SymbolKind::Diradical {
                0.45
            } else {
                0.2
            };
            add(circle(-gap, 0., 0.1, 0.1), true);
            add(circle(gap, 0., 0.1, 0.1), true);
        }
        SymbolKind::LonePairBar => add(vec![Move(p(-0.35, 0.)), Line(p(0.35, 0.))], false),
        SymbolKind::HydrogenDot | SymbolKind::HydrogenDash => {
            add(crate::style::outline_text("H", 0.85, p(-0.31, 0.3)), true);
            if kind == SymbolKind::HydrogenDot {
                add(circle(0.5, -0.27, 0.09, 0.09), true);
            } else {
                add(vec![Move(p(0.35, -0.27)), Line(p(0.65, -0.27))], false);
            }
        }
        SymbolKind::Diamond => add(
            vec![
                Move(p(0., -0.55)),
                Line(p(0.37, 0.)),
                Line(p(0., 0.55)),
                Line(p(-0.37, 0.)),
                Close,
            ],
            false,
        ),
        SymbolKind::Star => {
            let mut commands = vec![];
            for i in 0..5 {
                let t = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.;
                commands.extend([Move(p(0., 0.)), Line(p(t.cos() * 0.55, t.sin() * 0.55))]);
            }
            add(commands, false);
        }
        SymbolKind::Wavy => add(
            vec![
                Move(p(-0.55, 0.)),
                Cubic(p(-0.4, -0.45), p(-0.15, 0.45), p(0., 0.)),
                Cubic(p(0.15, -0.45), p(0.4, 0.45), p(0.55, 0.)),
            ],
            false,
        ),
        SymbolKind::Bead => {
            add(circle(0., 0., 0.5, 0.5), false);
            add(
                vec![
                    Move(p(-0.35, -0.35)),
                    Line(p(0.35, 0.35)),
                    Move(p(-0.35, 0.35)),
                    Line(p(0.35, -0.35)),
                ],
                false,
            );
        }
        SymbolKind::Dagger | SymbolKind::DoubleDagger => {
            let mut v = vec![
                Move(p(0., -0.6)),
                Line(p(0., 0.6)),
                Move(p(-0.35, -0.2)),
                Line(p(0.35, -0.2)),
            ];
            if kind == SymbolKind::DoubleDagger {
                v.extend([Move(p(-0.35, 0.2)), Line(p(0.35, 0.2))]);
            }
            add(v, false);
        }
        _ => {}
    }
    out
}
impl Part {
    pub fn map(&mut self, f: impl Fn(Point) -> Point) {
        self.commands = self.commands.iter().map(|c| c.map(&f)).collect();
    }
}
impl Graphic {
    pub fn parts(&self) -> Vec<Part> {
        let mut parts = match self.kind {
            GraphicKind::Symbol(kind) => symbol_parts(kind, 1, &self.style),
            GraphicKind::Orbital(kind) => {
                orbital_parts(kind, self.phase, self.phase_flipped, &self.style)
            }
            _ => {
                return self.path_parts();
            }
        };
        for part in &mut parts {
            part.map(|q| {
                self.origin.offset(
                    self.axis_x.x * q.x + self.axis_y.x * q.y,
                    self.axis_x.y * q.x + self.axis_y.y * q.y,
                )
            });
        }
        parts
    }
}
pub fn mark_parts(atom: &Atom) -> Vec<Part> {
    styled_mark_parts(atom, &DEFAULT)
}
/// Visible attached marks, including the document's ring-charge placement.
pub fn document_mark_parts(atom: &Atom, doc: &crate::document::Document) -> Vec<Part> {
    if atom.marks.iter().any(|mark| mark.kind.charge())
        && crate::aromatic::charge_is_centered(doc, atom)
    {
        let mut shown = atom.clone();
        shown.display.hide_charge = true;
        styled_mark_parts(&shown, &doc.drawing_style)
    } else {
        styled_mark_parts(atom, &doc.drawing_style)
    }
}
/// Whether `mark` draws anything on `atom`; a charge hidden with the
/// atom's charge label, for instance, does not.
pub fn mark_drawn(doc: &crate::document::Document, atom: &Atom, mark: &AtomMark) -> bool {
    let mut single = atom.clone();
    single.marks = vec![mark.clone()];
    !document_mark_parts(&single, doc).is_empty()
}
pub fn styled_mark_parts(atom: &Atom, drawing_style: &crate::style::DrawingStyle) -> Vec<Part> {
    let default_size = atom
        .text_style
        .as_ref()
        .map_or(drawing_style.font_size_pt, |s| s.size_pt)
        * 0.75;
    let style = GraphicStyle {
        stroke: atom
            .text_style
            .as_ref()
            .map_or(Default::default(), |s| s.color),
        width_pt: drawing_style.line_width_pt,
        ..Default::default()
    };
    let mut out = vec![];
    for mark in &atom.marks {
        let size = DEFAULT.world(mark.size_pt.unwrap_or(default_size));
        let angle = mark.angle.to_radians();
        let (kind, amount) = match mark.kind {
            MarkKind::Charge | MarkKind::CircledCharge
                if atom.charge != 0 && !atom.display.hide_charge =>
            {
                (
                    match (mark.kind == MarkKind::CircledCharge, atom.charge > 0) {
                        (true, true) => SymbolKind::CirclePlus,
                        (true, false) => SymbolKind::CircleMinus,
                        (false, true) => SymbolKind::Plus,
                        _ => SymbolKind::Minus,
                    },
                    atom.charge.unsigned_abs().min(255) as u8,
                )
            }
            MarkKind::Radical if atom.radical_electrons > 0 => (
                if atom.radical_electrons == 1 {
                    SymbolKind::Radical
                } else {
                    SymbolKind::Diradical
                },
                1,
            ),
            MarkKind::RadicalIon if atom.display.hide_charge && atom.radical_electrons > 0 => (
                if atom.radical_electrons == 1 {
                    SymbolKind::Radical
                } else {
                    SymbolKind::Diradical
                },
                1,
            ),
            MarkKind::RadicalIon if atom.charge != 0 && atom.radical_electrons > 0 => (
                if atom.charge > 0 {
                    SymbolKind::RadicalCation
                } else {
                    SymbolKind::RadicalAnion
                },
                atom.charge.unsigned_abs().min(255) as u8,
            ),
            MarkKind::LonePair => (SymbolKind::LonePair, 1),
            MarkKind::LonePairBar => (SymbolKind::LonePairBar, 1),
            MarkKind::PartialPositive => (SymbolKind::PartialPositive, 1),
            MarkKind::PartialNegative => (SymbolKind::PartialNegative, 1),
            _ => continue,
        };
        let mut parts = glyph_parts(kind, amount, &style, true);
        if mark.kind == MarkKind::RadicalIon
            && !atom.display.hide_charge
            && atom.radical_electrons == 2
            && let Some(first) = parts.get_mut(1)
        {
            first.commands = circle(-0.45, -0.18, 0.095, 0.095);
            let mut second = first.clone();
            second.commands = circle(-0.45, 0.18, 0.095, 0.095);
            parts.insert(2, second);
        }
        for mut part in parts {
            part.map(|q| {
                atom.position.offset(
                    mark.offset.x + (q.x * angle.cos() - q.y * angle.sin()) * size,
                    mark.offset.y + (q.x * angle.sin() + q.y * angle.cos()) * size,
                )
            });
            out.push(part);
        }
    }
    out
}
/// Charges and radicals are chemical edits; other attached marks are annotations.
pub fn attach(atom: &mut Atom, kind: SymbolKind, offset: Point) -> Result<(), String> {
    let mut candidate = atom.clone();
    attach_inner(&mut candidate, kind, offset)?;
    *atom = candidate;
    Ok(())
}
fn attach_inner(atom: &mut Atom, kind: SymbolKind, offset: Point) -> Result<(), String> {
    let mark=match kind {
        SymbolKind::CirclePlus|SymbolKind::CircleMinus|SymbolKind::Plus|SymbolKind::Minus=>{
            let delta=if matches!(kind,SymbolKind::CirclePlus|SymbolKind::Plus) {1} else {-1};
            atom.charge=atom.charge.saturating_add(delta).clamp(-8,8);
            atom.display.hide_charge=false;
            atom.marks.retain(|m|!m.kind.charge());
            if matches!(kind,SymbolKind::CirclePlus|SymbolKind::CircleMinus) {MarkKind::CircledCharge} else {MarkKind::Charge}
        }
        SymbolKind::Radical|SymbolKind::Diradical=>{atom.radical_electrons=if kind==SymbolKind::Radical {1} else {2};atom.marks.retain(|m|!m.kind.radical());MarkKind::Radical}
        SymbolKind::RadicalCation|SymbolKind::RadicalAnion=>{atom.radical_electrons=1;atom.charge=if kind==SymbolKind::RadicalCation {1} else {-1};atom.marks.retain(|m|!m.kind.radical()&&!m.kind.charge());MarkKind::RadicalIon}
        SymbolKind::LonePair=>MarkKind::LonePair,SymbolKind::LonePairBar=>MarkKind::LonePairBar,
        SymbolKind::PartialPositive=>MarkKind::PartialPositive,SymbolKind::PartialNegative=>MarkKind::PartialNegative,
        _=>return Err("This symbol is a drawing annotation. Use free placement; explicit H bonds carry stereochemistry.".into()),
    };
    if atom.marks.len() >= 12 {
        return Err("An atom can have at most 12 positioned marks".into());
    }
    if mark.charge() || mark.radical() {
        atom.explicit_h = 0;
        atom.no_implicit = false;
    }
    if mark.charge() || mark.radical() {
        atom.label_h = 0;
    }
    let id = next_mark_id(atom)?;
    atom.mark_serial = id;
    atom.marks.push(AtomMark {
        id: Some(id),
        kind: mark,
        offset,
        angle: 0.,
        size_pt: None,
    });
    Ok(())
}

/// Attached marks orbit their atom in these steps (degrees) unless dragged freely.
pub const MARK_ORBIT_STEP: f32 = 15.;

/// The mark dragged to `to`. A plain drag orbits the atom in 15° steps,
/// hugging its label at the click gap and skipping steps where the mark
/// would touch one of the atom's bonds; `free` keeps the exact offset. Lone
/// pairs turn tangent to the atom either way; other marks stay upright.
pub fn orbit_mark(
    doc: &crate::document::Document,
    atom: &Atom,
    mark: &AtomMark,
    to: Point,
    free: bool,
) -> AtomMark {
    // A mark that draws nothing has no shape to orbit with; keep its offset.
    if !mark_drawn(doc, atom, mark) {
        return mark.clone();
    }
    let offset = Point::new(to.x - atom.position.x, to.y - atom.position.y);
    let degrees = offset.y.atan2(offset.x).to_degrees();
    if free {
        return AtomMark {
            offset,
            ..mark_placement::tangent(mark.clone(), degrees)
        };
    }
    let turn = |snap: &(AtomMark, bool)| {
        let snapped = snap.0.offset.y.atan2(snap.0.offset.x).to_degrees();
        let turn = (snapped - degrees).rem_euclid(360.);
        turn.min(360. - turn)
    };
    let snaps = mark_placement::orbit(doc, atom, mark, orbit_snaps());
    // With every step blocked, the nearest one still beats an exact offset.
    snaps
        .iter()
        .filter(|(_, clear)| *clear)
        .min_by(|a, b| turn(a).total_cmp(&turn(b)))
        .or_else(|| snaps.iter().min_by(|a, b| turn(a).total_cmp(&turn(b))))
        .map_or_else(|| mark.clone(), |(mark, _)| mark.clone())
}

fn orbit_snaps() -> impl Iterator<Item = f32> {
    (0..24).map(|i| i as f32 * MARK_ORBIT_STEP)
}

/// The orbit a dragged mark follows, as the runs clear of the atom's bonds,
/// and its available snap positions, in world space. Both come from the
/// placement `orbit_mark` commits, and every snap is a vertex of the orbit.
pub fn mark_orbit_guide(
    doc: &crate::document::Document,
    atom: &Atom,
    mark: &AtomMark,
) -> (Vec<Vec<Point>>, Vec<Point>) {
    if !mark_drawn(doc, atom, mark) {
        return (vec![], vec![]);
    }
    let center = |m: &AtomMark| atom.position.offset(m.offset.x, m.offset.y);
    let directions: Vec<f32> = (0..=120).map(|i| i as f32 * 3.).collect();
    let snaps = directions
        .iter()
        .step_by(5)
        .take(24)
        .copied()
        .collect::<Vec<_>>();
    let mut orbit: Vec<_> = directions
        .iter()
        .copied()
        .zip(mark_placement::orbit(doc, atom, mark, directions.clone()))
        .collect();
    // Where the mark swings fast round a corner, sample more finely.
    let step = doc
        .drawing_style
        .world(doc.drawing_style.font_size_pt * 0.05);
    for _ in 0..3 {
        let between: Vec<f32> = orbit
            .iter()
            .zip(orbit.iter().skip(1))
            .filter(|(a, b)| center(&a.1.0).distance(center(&b.1.0)) > step)
            .map(|(a, b)| 0.5 * (a.0 + b.0))
            .collect();
        if between.is_empty() {
            break;
        }
        orbit.extend(between.iter().copied().zip(mark_placement::orbit(
            doc,
            atom,
            mark,
            between.clone(),
        )));
        orbit.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    let mut runs = vec![Vec::new()];
    for (_, (mark, clear)) in &orbit {
        match (clear, runs.last_mut()) {
            (true, Some(run)) => run.push(center(mark)),
            (false, Some(run)) if !run.is_empty() => runs.push(Vec::new()),
            _ => {}
        }
    }
    runs.retain(|run| run.len() > 1);
    let snaps = orbit
        .iter()
        .filter(|(degrees, (_, clear))| *clear && snaps.contains(degrees))
        .map(|(_, (mark, _))| center(mark))
        .collect();
    (runs, snaps)
}

pub fn next_mark_id(atom: &Atom) -> Result<u64, String> {
    atom.marks
        .iter()
        .filter_map(|m| m.id)
        .max()
        .unwrap_or(0)
        .max(atom.mark_serial)
        .checked_add(1)
        .filter(|id| *id < u64::MAX)
        .ok_or_else(|| "No free positioned-mark ID".into())
}

#[derive(Clone)]
pub struct Drawing {
    pub kind: GraphicKind,
    pub style: GraphicStyle,
    pub phase: Phase,
    pub flipped: bool,
    pub attach: bool,
    pub snap_orbitals: bool,
}
impl Drawing {
    /// Orbital snapping is independent of atom-owned chemical symbols.
    pub fn orbital_target(
        &self,
        doc: &crate::document::Document,
        start: Point,
        radius: f32,
    ) -> Option<u64> {
        (self.snap_orbitals && matches!(self.kind, GraphicKind::Orbital(_)))
            .then(|| doc.nearest(start, radius))
            .flatten()
    }
    /// `constrain` (Shift) snaps a free graphic; an attached mark's drag
    /// instead orbits its atom unless `constrain` frees it.
    pub fn place(
        &self,
        doc: &mut crate::document::Document,
        start: Point,
        end: Point,
        constrain: bool,
        radius: f32,
    ) -> Result<u64, String> {
        self.place_with(doc, start, end, constrain, radius, &OnceCell::new())
    }
    /// [`Self::place`], reusing `hydrogens` for a charge or radical's label
    /// so a drag's previews work out the molecule's chemistry only once.
    pub fn place_with(
        &self,
        doc: &mut crate::document::Document,
        start: Point,
        end: Point,
        constrain: bool,
        radius: f32,
        hydrogens: &OnceCell<Option<u32>>,
    ) -> Result<u64, String> {
        if let GraphicKind::Symbol(kind) = self.kind
            && self.attach
            && let Some(id) = doc.nearest(start, radius)
        {
            let default_label_size = doc.drawing_style.font_size_pt;
            let automatic_pair = if start.distance(end) < 3.
                && matches!(kind, SymbolKind::LonePair | SymbolKind::LonePairBar)
            {
                let atom = doc
                    .atom(id)
                    .ok_or("The symbol's attachment atom is unavailable")?;
                Some(mark_placement::lone_pair(
                    doc,
                    atom,
                    if kind == SymbolKind::LonePair {
                        MarkKind::LonePair
                    } else {
                        MarkKind::LonePairBar
                    },
                ))
            } else {
                None
            };
            let atom = doc
                .atom_mut(id)
                .ok_or("The symbol's attachment atom is unavailable")?;
            let offset = if let Some((offset, _)) = automatic_pair {
                offset
            } else if start.distance(end) < 3. {
                {
                    let angle = (-90. - 90. * (atom.marks.len() % 4) as f32).to_radians();
                    let label_size = atom
                        .text_style
                        .as_ref()
                        .map_or(default_label_size, |s| s.size_pt);
                    let radius =
                        DEFAULT.world(label_size * (1.0 + 0.8 * (atom.marks.len() / 4) as f32));
                    Point::new(radius * angle.cos(), radius * angle.sin())
                }
            } else {
                Point::new(end.x - atom.position.x, end.y - atom.position.y)
            };
            attach(atom, kind, offset)?;
            if let Some((_, angle)) = automatic_pair
                && let Some(mark) = atom.marks.last_mut()
            {
                mark.angle = angle;
            }
            if atom
                .marks
                .last()
                .is_some_and(|m| m.kind.charge() || m.kind.radical())
            {
                doc.invalidate_chemistry(&[id]);
                // Orbit the label the refresh will show, hydrogens included.
                let hydrogens = *hydrogens
                    .get_or_init(|| crate::atom_labels::refresh::label_hydrogens(doc, id));
                if let Some((atom, h)) = doc.atom_mut(id).zip(hydrogens) {
                    atom.label_h = h;
                }
            }
            if start.distance(end) < 3.
                && automatic_pair.is_none()
                && let Some(atom) = doc.atom(id)
                && let Some(mark) = atom.marks.last()
            {
                let offset = mark_placement::charge_click(doc, atom, mark);
                if let Some(mark) = doc.atom_mut(id).and_then(|a| a.marks.last_mut()) {
                    mark.offset = offset;
                }
            }
            // A drag orbits the atom like moving the mark; `constrain` frees it.
            if start.distance(end) >= 3.
                && let Some(atom) = doc.atom(id)
                && let Some(mark) = atom.marks.last()
            {
                let placed = orbit_mark(doc, atom, mark, end, constrain);
                if let Some(mark) = doc.atom_mut(id).and_then(|a| a.marks.last_mut()) {
                    *mark = placed;
                }
            }
            return Ok(id);
        }
        let origin = self
            .orbital_target(doc, start, radius)
            .and_then(|id| doc.atom(id))
            .map_or(start, |a| a.position);
        let id = doc.next_id();
        let mut g = Graphic::dragged(
            id,
            self.kind,
            origin,
            end,
            self.style.clone(),
            crate::graphics::BracketSides::Both,
            constrain,
        );
        g.phase = self.phase;
        g.phase_flipped = self.flipped;
        doc.graphics.push(g);
        Ok(id)
    }
}
