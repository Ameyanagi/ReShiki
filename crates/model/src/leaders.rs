//! Callout leaders: lines whose ends stay on an atom, a bond, a molecule or
//! another object and follow it when it moves. A leader is an illustration:
//! reaction detection, chemical copy and every exchange format see a line.
use crate::{
    document::{Annotation, Document, Point},
    graphics::{Graphic, GraphicKind, GraphicStyle, PathCommand},
    style::DEFAULT,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Atom {
        atom: u64,
    },
    /// The point `fraction` of the way from atom `a` to atom `b`.
    Bond {
        a: u64,
        b: u64,
        fraction: f32,
    },
    /// The molecule containing `atom`; the leader stops at its edge.
    Molecule {
        atom: u64,
    },
    /// A text, shape or picture; the leader stops at its edge.
    Object {
        id: u64,
    },
}

/// Which ends of a line follow a target.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Leader {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Target>,
}

/// Space between a leader and the edge of a molecule or object.
const GAP_PT: f32 = 2.;
/// Radius of the dot marking an atom or bond.
const DOT_PT: f32 = 1.6;

impl Target {
    fn ids(&self) -> Vec<u64> {
        match *self {
            Self::Atom { atom } | Self::Molecule { atom } => vec![atom],
            Self::Bond { a, b, .. } => vec![a, b],
            Self::Object { id } => vec![id],
        }
    }
    /// A point target, which the leader marks with a dot.
    pub fn is_point(&self) -> bool {
        matches!(self, Self::Atom { .. } | Self::Bond { .. })
    }
    fn exists(&self, doc: &Document) -> bool {
        match self {
            Self::Atom { atom } | Self::Molecule { atom } => doc.atom(*atom).is_some(),
            Self::Bond { a, b, .. } => doc
                .bonds
                .iter()
                .any(|bond| (bond.a, bond.b) == (*a, *b) || (bond.a, bond.b) == (*b, *a)),
            Self::Object { id } => {
                doc.annotations.iter().any(|a| a.id == *id)
                    || doc
                        .graphics
                        .iter()
                        .any(|g| g.id == *id && g.leader.is_none())
            }
        }
    }
    /// Where the leader touches this target when its other end is `toward`.
    fn resolve(&self, doc: &Document, toward: Point) -> Option<Point> {
        let ids = match *self {
            Self::Atom { atom } => return Some(doc.atom(atom)?.position),
            Self::Bond { a, b, fraction } => {
                return Some(doc.atom(a)?.position.lerp(doc.atom(b)?.position, fraction));
            }
            Self::Molecule { atom } => crate::reactions::molecules(doc, &[atom])
                .pop()
                .unwrap_or_default(),
            Self::Object { id } => vec![id],
        };
        let (lo, hi) = crate::scene::selection_bounds(doc, &ids)?;
        let gap = DEFAULT.world(GAP_PT);
        Some(edge(
            Point::new(lo.x - gap, lo.y - gap),
            Point::new(hi.x + gap, hi.y + gap),
            toward,
        ))
    }
    fn remap(&mut self, mapping: &HashMap<u64, u64>) -> bool {
        let map = |id: &mut u64| mapping.get(id).map(|new| *id = *new).is_some();
        match self {
            Self::Atom { atom } | Self::Molecule { atom } => map(atom),
            Self::Bond { a, b, .. } => map(a) && map(b),
            Self::Object { id } => map(id),
        }
    }
}

/// Where the ray from a box's center toward `p` leaves the box.
fn edge(lo: Point, hi: Point, p: Point) -> Point {
    let c = lo.lerp(hi, 0.5);
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    let (hx, hy) = ((hi.x - lo.x) * 0.5, (hi.y - lo.y) * 0.5);
    let scale = [(dx, hx), (dy, hy)]
        .into_iter()
        .filter(|(d, _)| d.abs() > 1e-6)
        .map(|(d, h)| h / d.abs())
        .fold(f32::INFINITY, f32::min);
    if scale.is_finite() && scale < 1. {
        c.offset(dx * scale, dy * scale)
    } else {
        c
    }
}

impl Graphic {
    /// The two ends of a line graphic.
    pub fn line_ends(&self) -> [Point; 2] {
        [
            self.origin,
            self.origin.offset(self.axis_x.x, self.axis_x.y),
        ]
    }
    pub(crate) fn set_line_ends(&mut self, [start, end]: [Point; 2]) {
        let (dx, dy) = (end.x - start.x, end.y - start.y);
        self.origin = start;
        self.axis_x = Point::new(dx, dy);
        self.axis_y = Point::new(-dy * 0.5, dx * 0.5);
    }
    /// The filled dot a leader draws on an atom or bond it points at.
    pub(crate) fn leader_dot(&self) -> Option<(Vec<PathCommand>, GraphicStyle)> {
        let leader = self.leader.as_ref()?;
        let [start, end] = self.line_ends();
        let at = match (&leader.end, &leader.start) {
            (Some(target), _) if target.is_point() => end,
            (_, Some(target)) if target.is_point() => start,
            _ => return None,
        };
        let r = DEFAULT.world(DOT_PT.max(self.style.width_pt * 1.5));
        let k = 0.552_284_8 * r;
        let p = |x: f32, y: f32| at.offset(x, y);
        use PathCommand::{Close, Cubic, Move};
        Some((
            vec![
                Move(p(r, 0.)),
                Cubic(p(r, k), p(k, r), p(0., r)),
                Cubic(p(-k, r), p(-r, k), p(-r, 0.)),
                Cubic(p(-r, -k), p(-k, -r), p(0., -r)),
                Cubic(p(k, -r), p(r, -k), p(r, 0.)),
                Close,
            ],
            GraphicStyle {
                fill: Some(self.style.stroke),
                width_pt: 0.,
                ..self.style.clone()
            },
        ))
    }
}

/// Move every leader end onto its target. A target that no longer exists
/// releases its end where it is; a callout whose text is gone goes with it.
pub fn reconcile(doc: &mut Document) {
    if doc.graphics.iter().all(|g| g.leader.is_none()) {
        return;
    }
    let plans: Vec<_> = doc
        .graphics
        .iter()
        .map(|g| {
            let leader = g.leader.as_ref()?;
            let live = |t: &Option<Target>| t.clone().filter(|t| t.exists(doc));
            let (s, e) = (live(&leader.start), live(&leader.end));
            if s.is_none() && matches!(leader.start, Some(Target::Object { .. })) {
                return Some(None);
            }
            let [mut start, mut end] = g.line_ends();
            // An edge depends on where the other end is, so settle twice.
            for _ in 0..2 {
                if let Some(p) = e.as_ref().and_then(|t| t.resolve(doc, start)) {
                    end = p;
                }
                if let Some(p) = s.as_ref().and_then(|t| t.resolve(doc, end)) {
                    start = p;
                }
            }
            Some(Some((Leader { start: s, end: e }, [start, end])))
        })
        .collect();
    let removed = plans.iter().any(|plan| matches!(plan, Some(None)));
    let mut plans = plans.into_iter();
    doc.graphics.retain_mut(|g| match plans.next().flatten() {
        Some(None) => false,
        Some(Some((leader, ends))) => {
            g.leader = (leader != Leader::default()).then_some(leader);
            g.set_line_ends(ends);
            true
        }
        None => true,
    });
    if removed {
        doc.prune_groups();
    }
}

/// Keep leader targets through an ID change; a target left behind releases its end.
pub fn remap(graphic: &mut Graphic, mapping: &HashMap<u64, u64>) {
    if let Some(leader) = graphic.leader.as_mut() {
        for target in [&mut leader.start, &mut leader.end] {
            if target.as_mut().is_some_and(|t| !t.remap(mapping)) {
                *target = None;
            }
        }
        if *leader == Leader::default() {
            graphic.leader = None;
        }
    }
}

/// A leader moved without its target releases that end, like an arrow does.
pub(crate) fn transformed(doc: &mut Document, moved: impl Fn(u64) -> Option<usize>) {
    for g in &mut doc.graphics {
        let Some(unit) = moved(g.id) else {
            continue;
        };
        if let Some(leader) = g.leader.as_mut() {
            for target in [&mut leader.start, &mut leader.end] {
                if target
                    .as_ref()
                    .is_some_and(|t| t.ids().iter().any(|id| moved(*id) != Some(unit)))
                {
                    *target = None;
                }
            }
            if *leader == Leader::default() {
                g.leader = None;
            }
        }
    }
}

pub fn validate(doc: &Document) -> Result<(), String> {
    for g in doc.graphics.iter() {
        let Some(leader) = &g.leader else {
            continue;
        };
        if g.kind != GraphicKind::Line {
            return Err("Only a line can be a callout leader".into());
        }
        if [&leader.start, &leader.end]
            .into_iter()
            .flatten()
            .any(|t| matches!(t, Target::Bond { fraction, .. } if !(0.0..=1.0).contains(fraction)))
        {
            return Err("A callout leader's bond point must lie on the bond".into());
        }
    }
    Ok(())
}

/// Where a callout's text goes for `target`: up and to the right of it.
pub fn callout_position(doc: &Document, target: &Target) -> Result<Point, String> {
    if !target.exists(doc) {
        return Err("Select an atom, a bond, a molecule or an object for the callout".into());
    }
    let at = match *target {
        Target::Atom { .. } | Target::Bond { .. } => target.resolve(doc, Point::default()),
        Target::Molecule { atom } => {
            let atoms = crate::reactions::molecules(doc, &[atom])
                .pop()
                .unwrap_or_default();
            crate::scene::selection_bounds(doc, &atoms).map(|(lo, hi)| Point::new(hi.x, lo.y))
        }
        Target::Object { id } => {
            crate::scene::selection_bounds(doc, &[id]).map(|(lo, hi)| Point::new(hi.x, lo.y))
        }
    }
    .ok_or("The callout target has no position")?;
    let reach = doc.drawing_style.bond_length_world * 1.6;
    Ok(at.offset(reach, -reach))
}

/// Add the leader from the callout text `text` to `target`.
pub fn attach(doc: &mut Document, text: u64, target: Target) -> Result<(), String> {
    let card = doc
        .annotations
        .iter()
        .find(|a| a.id == text)
        .map(|a| a.position)
        .ok_or("The callout text is missing")?;
    let id = doc.next_id();
    if id == u64::MAX {
        return Err("This drawing has run out of object IDs".into());
    }
    let mut line = Graphic::dragged(
        id,
        GraphicKind::Line,
        card,
        card,
        GraphicStyle {
            width_pt: doc.drawing_style.line_width_pt,
            ..GraphicStyle::default()
        },
        Default::default(),
        false,
    );
    line.layer = 1;
    line.leader = Some(Leader {
        start: Some(Target::Object { id: text }),
        end: Some(target),
    });
    doc.graphics.push(line);
    reconcile(doc);
    Ok(())
}

/// Add a callout to `target`: a text beside it and a leader from the text to
/// the target, in one change. Returns the new text's ID.
pub fn add_callout(doc: &mut Document, target: Target, text: &str) -> Result<u64, String> {
    let position = callout_position(doc, &target)?;
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        angle: 0.,
        id,
        position,
        text: text.into(),
        format: crate::typography::TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    });
    attach(doc, id, target)?;
    Ok(id)
}

/// Follow atoms merged by a join: an end on a merged atom moves to the atom
/// that remains, and an end on a bond that collapsed is released. `mapping`
/// holds every atom before the join.
pub(crate) fn remap_join(doc: &mut Document, mapping: &HashMap<u64, u64>) {
    let bonds: std::collections::HashSet<(u64, u64)> = doc
        .bonds
        .iter()
        .flat_map(|b| [(b.a, b.b), (b.b, b.a)])
        .collect();
    for g in &mut doc.graphics {
        let Some(leader) = g.leader.as_mut() else {
            continue;
        };
        for end in [&mut leader.start, &mut leader.end] {
            let keep = match end.as_mut() {
                Some(Target::Atom { atom } | Target::Molecule { atom }) => {
                    match mapping.get(atom) {
                        Some(new) => {
                            *atom = *new;
                            true
                        }
                        None => false,
                    }
                }
                Some(Target::Bond { a, b, .. }) => {
                    match (mapping.get(a).copied(), mapping.get(b).copied()) {
                        (Some(x), Some(y)) if bonds.contains(&(x, y)) => {
                            (*a, *b) = (x, y);
                            true
                        }
                        _ => false,
                    }
                }
                _ => true,
            };
            if !keep {
                *end = None;
            }
        }
        if *leader == Leader::default() {
            g.leader = None;
        }
    }
}

impl Target {
    /// The IDs this target names, for code that renumbers objects.
    pub(crate) fn ids_mut(&mut self) -> Vec<&mut u64> {
        match self {
            Self::Atom { atom } | Self::Molecule { atom } => vec![atom],
            Self::Bond { a, b, .. } => vec![a, b],
            Self::Object { id } => vec![id],
        }
    }
}

#[cfg(test)]
mod tests;
