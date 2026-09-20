//! Chemical marks and orbital diagrams. Attached marks use the atom's actual
//! charge/radical state; free symbols and orbitals are drawing objects.
use crate::{
    document::{Atom, Point},
    graphics::{Graphic, GraphicKind, GraphicStyle, PathCommand},
    style::DEFAULT,
};
use serde::{Deserialize, Serialize};

macro_rules! choices {
    ($name:ident { $first:ident => $label:literal $(,$variant:ident => $text:literal)* $(,)? }) => {
        #[derive(Debug,Clone,Copy,Default,PartialEq,Eq,Serialize,Deserialize)]
        #[serde(rename_all="snake_case")]
        pub enum $name {#[default] $first,$($variant),*}
        impl $name {pub const ALL:&'static [Self]=&[Self::$first,$(Self::$variant),*];}
        impl std::fmt::Display for $name {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str(match self {Self::$first=>$label,$(Self::$variant=>$text),*})}}
    }
}
choices!(SymbolKind {CirclePlus=>"Circled plus",CircleMinus=>"Circled minus",Plus=>"Plus",Minus=>"Minus",Radical=>"Radical",Diradical=>"Diradical",LonePair=>"Lone pair",LonePairBar=>"Lone pair bar",RadicalCation=>"Radical cation",RadicalAnion=>"Radical anion",HydrogenDot=>"H-dot",HydrogenDash=>"H-dash",Diamond=>"Attachment diamond",Star=>"Attachment star",Wavy=>"Wavy attachment",Bead=>"Polymer bead",Dagger=>"Dagger",DoubleDagger=>"Double dagger"});
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
            Some(if phase == Phase::Solid {
                style.fill.unwrap_or(style.stroke)
            } else {
                style
                    .fill
                    .unwrap_or(style.stroke)
                    .map(|c| ((c as u16 + 255 * 2) / 3) as u8)
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
    use PathCommand::*;
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
        let mut v = vec![Move(p(x - 0.3, 0.)), Line(p(x + 0.3, 0.))];
        if positive {
            v.extend([Move(p(x, -0.3)), Line(p(x, 0.3))]);
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
        if matches!(kind, SymbolKind::CirclePlus | SymbolKind::CircleMinus) {
            add(circle(0., 0., 0.52, 0.52), false);
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
            add(circle(-0.2, 0., 0.1, 0.1), true);
            add(circle(0.2, 0., 0.1, 0.1), true);
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
                return vec![Part {
                    commands: self.commands(),
                    style: self.style.clone(),
                    filled: self.filled(),
                }];
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
    let default_size = atom.text_style.as_ref().map_or(10., |s| s.size_pt) * 0.75;
    let style = GraphicStyle {
        stroke: atom.text_style.as_ref().map_or([0; 3], |s| s.color),
        ..Default::default()
    };
    let mut out = vec![];
    for mark in &atom.marks {
        let size = DEFAULT.world(mark.size_pt.unwrap_or(default_size));
        let angle = mark.angle.to_radians();
        let (kind, amount) = match mark.kind {
            MarkKind::Charge | MarkKind::CircledCharge if atom.charge != 0 => (
                match (mark.kind == MarkKind::CircledCharge, atom.charge > 0) {
                    (true, true) => SymbolKind::CirclePlus,
                    (true, false) => SymbolKind::CircleMinus,
                    (false, true) => SymbolKind::Plus,
                    _ => SymbolKind::Minus,
                },
                atom.charge.unsigned_abs().min(255) as u8,
            ),
            MarkKind::Radical if atom.radical_electrons > 0 => (
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
            _ => continue,
        };
        let mut parts = symbol_parts(kind, amount, &style);
        if mark.kind == MarkKind::RadicalIon
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
/// Charges and radicals are chemical edits; lone pairs are explicit annotations.
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
            atom.marks.retain(|m|!m.kind.charge());
            if matches!(kind,SymbolKind::CirclePlus|SymbolKind::CircleMinus) {MarkKind::CircledCharge} else {MarkKind::Charge}
        }
        SymbolKind::Radical|SymbolKind::Diradical=>{atom.radical_electrons=if kind==SymbolKind::Radical {1} else {2};atom.marks.retain(|m|!m.kind.radical());MarkKind::Radical}
        SymbolKind::RadicalCation|SymbolKind::RadicalAnion=>{atom.radical_electrons=1;atom.charge=if kind==SymbolKind::RadicalCation {1} else {-1};atom.marks.retain(|m|!m.kind.radical()&&!m.kind.charge());MarkKind::RadicalIon}
        SymbolKind::LonePair=>MarkKind::LonePair,SymbolKind::LonePairBar=>MarkKind::LonePairBar,
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
    atom.marks.push(AtomMark {
        kind: mark,
        offset,
        angle: 0.,
        size_pt: None,
    });
    Ok(())
}

#[derive(Clone)]
pub struct Drawing {
    pub kind: GraphicKind,
    pub style: GraphicStyle,
    pub phase: Phase,
    pub flipped: bool,
    pub attach: bool,
}
impl Drawing {
    pub fn place(
        &self,
        doc: &mut crate::document::Document,
        start: Point,
        end: Point,
        constrain: bool,
        radius: f32,
    ) -> Result<u64, String> {
        if let GraphicKind::Symbol(kind) = self.kind
            && self.attach
            && let Some(id) = doc.nearest(start, radius)
        {
            let atom = doc
                .atom_mut(id)
                .ok_or("The symbol's attachment atom is unavailable")?;
            let offset = if start.distance(end) < 3. {
                {
                    let angle = (-90. - 90. * (atom.marks.len() % 4) as f32).to_radians();
                    let label_size = atom.text_style.as_ref().map_or(10., |s| s.size_pt);
                    let radius =
                        DEFAULT.world(label_size * (1.0 + 0.8 * (atom.marks.len() / 4) as f32));
                    Point::new(radius * angle.cos(), radius * angle.sin())
                }
            } else {
                Point::new(end.x - atom.position.x, end.y - atom.position.y)
            };
            attach(atom, kind, offset)?;
            if !matches!(kind, SymbolKind::LonePair | SymbolKind::LonePairBar) {
                doc.invalidate_chemistry(&[id]);
            }
            return Ok(id);
        }
        let origin = if matches!(self.kind, GraphicKind::Orbital(_)) {
            doc.nearest(start, radius)
                .and_then(|id| doc.atom(id))
                .map_or(start, |a| a.position)
        } else {
            start
        };
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
