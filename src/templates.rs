//! Built-in molecules and attachment geometry, shared by preview and placement.
mod aromatic;
use crate::document::{Atom, Document, Point};
use crate::editing;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    #[serde(default)]
    pub id: String,
    pub group: String,
    pub name: String,
    #[serde(default)]
    pub smiles: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub document: Document,
    #[serde(default)]
    pub anchor: Anchor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    #[default]
    Auto,
    Atom(u64),
    Bond(u64, u64),
}
impl Anchor {
    pub fn valid(self, doc: &Document) -> bool {
        match self {
            Self::Auto => true,
            Self::Atom(id) => doc.atom(id).is_some(),
            Self::Bond(a, b) => doc
                .bonds
                .iter()
                .any(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a)),
        }
    }
    pub fn point(self, doc: &Document) -> Option<Point> {
        match self {
            Self::Atom(id) => doc.atom(id).map(|a| a.position),
            Self::Bond(a, b) => Some(midpoint(doc.atom(a)?.position, doc.atom(b)?.position)),
            Self::Auto => Some(editing::center(doc, &doc.all_ids())),
        }
    }
}
impl std::fmt::Display for Anchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "Automatic attachment"),
            Self::Atom(id) => write!(f, "Atom {id}"),
            Self::Bond(a, b) => write!(f, "Bond {a}–{b}"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Connection {
    /// Retained for callers that choose attachment from the destination alone.
    Auto,
    #[default]
    Connect,
    ShareAtom,
    FuseBond,
}
impl std::fmt::Display for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Automatic attachment",
            Self::Connect => "Connect with a bond",
            Self::ShareAtom => "Share an atom",
            Self::FuseBond => "Fuse along a bond",
        })
    }
}
impl Connection {
    pub fn hint(self) -> &'static str {
        match self {
            Self::Auto => "Choose a destination atom or bond.",
            Self::Connect => {
                "Choose a template atom, then a drawing atom. A new single bond joins them. Drag to set direction. Shift/Ctrl snaps to 15°; Alt frees the angle."
            }
            Self::ShareAtom => {
                "Choose one atom in each structure. They become a single shared atom."
            }
            Self::FuseBond => {
                "Choose one bond in each structure. Their two atoms become a shared edge. Drag to choose the side."
            }
        }
    }
}

/// Explicit attachment intent, used identically by the hover preview and commit.
pub fn place_with_mode(
    doc: &Document,
    part: &Document,
    point: Point,
    direction: Option<Point>,
    radius: f32,
    anchor: Anchor,
    mode: Connection,
) -> Result<(Document, Vec<u64>), &'static str> {
    if doc.validate().is_err()
        || part.validate().is_err()
        || !point.x.is_finite()
        || !point.y.is_finite()
        || !radius.is_finite()
        || radius <= 0.
        || direction.is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
        || !anchor.valid(part)
    {
        return Err("Invalid template or attachment geometry.");
    }
    let target = doc.nearest(point, radius);
    let bond = target.is_none() && editing::nearest_bond(doc, point, radius).is_some();
    if mode == Connection::Auto || (target.is_none() && !bond) {
        return place_anchored(doc, part, point, direction, radius, anchor);
    }
    match mode {
        Connection::ShareAtom if target.is_none() => {
            return Err("Share an atom: point to a drawing atom.");
        }
        Connection::FuseBond if !bond => return Err("Fuse along a bond: point to a drawing bond."),
        Connection::Connect if bond || matches!(anchor, Anchor::Bond(..)) => {
            return Err("Connect with a bond: choose a template atom and point to a drawing atom.");
        }
        Connection::ShareAtom | Connection::FuseBond => {
            let placed = place_anchored(doc, part, point, direction, radius, anchor);
            if placed.is_err()
                && mode == Connection::FuseBond
                && let Some(result) = aromatic::fuse(doc, part, point, direction, radius, anchor)
            {
                return Ok(result);
            }
            return placed;
        }
        _ => {}
    }
    let id = target.ok_or("Choose a drawing atom to connect.")?;
    let target = doc.atom(id).ok_or("The drawing atom is unavailable.")?;
    let eligible = |d: &Document, a: &Atom| {
        a.stereo.is_none()
            && a.radical_electrons == 0
            && a.explicit_h == 0
            && !a.no_implicit
            && d.abbreviation(a.id).is_none()
            && valence(d, a.id).saturating_add(2) <= capacity(a)
    };
    if !eligible(doc, target) {
        return Err(
            "This atom has no available valence, or needs its abbreviation/stereochemistry expanded first.",
        );
    }
    let lengths: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            let other = if b.a == id {
                b.b
            } else if b.b == id {
                b.a
            } else {
                return None;
            };
            let length = doc.atom(other)?.position.distance(target.position);
            (length > 0.001).then_some(length)
        })
        .collect();
    let length = if lengths.is_empty() {
        crate::style::DEFAULT.bond_length_world
    } else {
        lengths.iter().sum::<f32>() / lengths.len() as f32
    };
    let end = direction
        .filter(|p| p.distance(target.position) > radius)
        .unwrap_or_else(|| editing::bond_extension(doc, target.position, Some(id), 1));
    let angle = (end.y - target.position.y).atan2(end.x - target.position.x);
    let dest = target
        .position
        .offset(length * angle.cos(), length * angle.sin());
    let mut best: Option<(f32, Document, Vec<u64>)> = None;
    for source in &part.atoms {
        if matches!(anchor, Anchor::Atom(chosen) if chosen != source.id) || !eligible(part, source)
        {
            continue;
        }
        let source_length = part
            .bonds
            .iter()
            .filter(|b| b.a == source.id || b.b == source.id)
            .find_map(|b| Some(part.atom(b.a)?.position.distance(part.atom(b.b)?.position)))
            .filter(|n| *n > 0.001)
            .unwrap_or(length);
        // Align the source's open valence with the new bond exactly. Steric
        // scoring chooses among chemically reasonable directions, never arbitrary
        // rotations that distort the angles at the template end of the bond.
        for outward in connection_directions(part, source) {
            let rotation = (angle + std::f32::consts::PI - outward).to_degrees();
            let mut positioned = part.clone();
            let all = positioned.all_ids();
            editing::transform_about(
                &mut positioned,
                &all,
                source.position,
                length / source_length,
                rotation,
            );
            positioned.translate(&all, dest.x - source.position.x, dest.y - source.position.y);
            let mut score = 0.;
            for a in &positioned.atoms {
                for b in &doc.atoms {
                    let distance = a.position.distance(b.position) / length;
                    score +=
                        (0.85 - distance).max(0.).powi(2) * 1000. + 0.05 / (distance + 0.1).powi(2);
                }
            }
            if best.as_ref().is_some_and(|(old, _, _)| score >= *old) {
                continue;
            }
            let mut result = doc.clone();
            let ids = editing::append(&mut result, &positioned, Point::default());
            if ids.len() != all.len() {
                continue;
            }
            let Some(added) = all
                .iter()
                .zip(&ids)
                .find_map(|(a, b)| (*a == source.id).then_some(*b))
            else {
                continue;
            };
            result.add_bond(id, added, 1, "plain");
            result.reconcile_molecule_groups();
            if crate::reactions::reconcile(&mut result).is_ok() && result.validate().is_ok() {
                best = Some((score, result, ids));
            }
        }
    }
    best.map(|(_, doc, ids)| (doc, ids))
        .ok_or("The chosen template atom has no available valence for a new bond.")
}

fn connection_directions(doc: &Document, source: &Atom) -> Vec<f32> {
    use std::f32::consts::{PI, TAU};
    let neighbors: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|b| {
            let other = if b.a == source.id {
                b.b
            } else if b.b == source.id {
                b.a
            } else {
                return None;
            };
            let point = doc.atom(other)?.position;
            (point.distance(source.position) > 0.001).then_some((
                (point.y - source.position.y)
                    .atan2(point.x - source.position.x)
                    .rem_euclid(TAU),
                b.order,
            ))
        })
        .collect();
    match neighbors.as_slice() {
        [] => vec![0.],
        &[(angle, 3 | 6)] => vec![angle + PI],
        &[(angle, _)] => vec![angle + 2. * PI / 3., angle - 2. * PI / 3.],
        _ => {
            let mut angles: Vec<_> = neighbors.iter().map(|(a, _)| *a).collect();
            angles.sort_by(f32::total_cmp);
            let gaps: Vec<_> = angles
                .iter()
                .zip(angles.iter().cycle().skip(1))
                .take(angles.len())
                .enumerate()
                .map(|(i, (&a, &b))| {
                    let gap = b + if i + 1 == angles.len() { TAU } else { 0. } - a;
                    (a + gap / 2., gap)
                })
                .collect();
            let largest = gaps.iter().map(|(_, g)| *g).fold(0., f32::max);
            gaps.into_iter()
                .filter(|(_, g)| largest - g < 0.0001)
                .map(|(a, _)| a)
                .collect()
        }
    }
}

static BUNDLED: LazyLock<Result<Vec<Template>, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/templates.json"))
        .map_err(|e| format!("Bundled templates could not be read: {e}"))
});
pub fn builtin_error() -> Option<&'static str> {
    BUNDLED.as_ref().err().map(String::as_str)
}
pub static LIBRARY: LazyLock<Vec<Template>> = LazyLock::new(|| {
    let mut templates = BUNDLED.as_ref().cloned().unwrap_or_default();
    for preset in [
        crate::rings::Preset::ChairUp,
        crate::rings::Preset::ChairDown,
    ] {
        templates.push(Template {
            id: String::new(),
            group: "Conformers".into(),
            name: format!("Cyclohexane · {preset}"),
            smiles: "C1CCCCC1".into(),
            keywords: vec!["chair".into(),"cyclohexane".into()],
            note: "Chair projection with equal bond lengths. It does not assign 3D geometry or stereochemistry.".into(),
            document: preset.document(crate::style::DEFAULT.bond_length_world, false),
            anchor: Anchor::Auto,
        });
    }
    for t in &mut templates {
        t.id = format!("builtin:{}/{}", t.group, t.name);
    }
    templates
});

fn midpoint(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

fn compatible(a: &Atom, b: &Atom) -> bool {
    a.element == b.element
        && a.charge == b.charge
        && a.radical_electrons == 0
        && b.radical_electrons == 0
        && a.marks.is_empty()
        && b.marks.is_empty()
        && a.isotope == b.isotope
        && a.explicit_h == 0
        && b.explicit_h == 0
        && !a.no_implicit
        && !b.no_implicit
        && a.stereo.is_none()
        && b.stereo.is_none()
        && a.map_num == 0
        && b.map_num == 0
}

pub(crate) fn valence(doc: &Document, id: u64) -> u32 {
    doc.bonds
        .iter()
        .filter(|b| b.a == id || b.b == id)
        .map(|b| match b.order {
            0 => 0,
            4 | 7 => 3,
            5 => {
                if b.b == id {
                    2
                } else {
                    0
                }
            }
            6 => 8,
            n => 2 * n as u32,
        })
        .sum()
}

pub(crate) fn capacity(atom: &Atom) -> u32 {
    match (atom.element.as_str(), atom.charge) {
        ("C", 0) => 8,
        ("N", 0) => 6,
        ("N", 1) => 8,
        ("O" | "S", 0) => 4,
        _ => 0,
    }
}

/// Pure operation: incompatible targets return an error without changing the drawing.
/// Bond attachment shares two atoms; atom attachment shares one. The destination's
/// bond order, atom identity, and all existing coordinates are retained.
pub fn place(
    doc: &Document,
    part: &Document,
    point: Point,
    direction: Option<Point>,
    radius: f32,
) -> Result<(Document, Vec<u64>), &'static str> {
    place_anchored(doc, part, point, direction, radius, Anchor::Auto)
}

pub fn place_anchored(
    doc: &Document,
    part: &Document,
    point: Point,
    direction: Option<Point>,
    radius: f32,
    anchor: Anchor,
) -> Result<(Document, Vec<u64>), &'static str> {
    if doc.validate().is_err() || part.validate().is_err() {
        return Err("The drawing or template is invalid.");
    }
    if !point.x.is_finite()
        || !point.y.is_finite()
        || !radius.is_finite()
        || radius <= 0.
        || direction.is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err("Invalid attachment geometry.");
    }
    if !anchor.valid(part) || part.all_ids().is_empty() {
        return Err("Choose an attachment point in a nonempty template.");
    }
    let atom = (!part.atoms.is_empty())
        .then(|| doc.nearest(point, radius))
        .flatten();
    let bond = atom
        .is_none()
        .then(|| {
            (!part.atoms.is_empty())
                .then(|| editing::nearest_bond(doc, point, radius))
                .flatten()
        })
        .flatten();
    if atom.is_none() && bond.is_none() {
        let mut result = doc.clone();
        let center = anchor
            .point(part)
            .ok_or("The source attachment point is unavailable")?;
        let mut positioned = part.clone();
        if let Some(direction) = direction.filter(|p| p.distance(point) > radius) {
            let angle = (direction.y - point.y)
                .atan2(direction.x - point.x)
                .to_degrees();
            let ids = positioned.all_ids();
            editing::transform_about(&mut positioned, &ids, center, 1., angle);
        }
        let ids = editing::append(
            &mut result,
            &positioned,
            Point::new(point.x - center.x, point.y - center.y),
        );
        if ids.len() != part.all_ids().len() {
            return Err("The template could not be inserted.");
        }
        return Ok((result, ids));
    }
    if (atom.is_some() && matches!(anchor, Anchor::Bond(..)))
        || (bond.is_some() && matches!(anchor, Anchor::Atom(..)))
    {
        return Err("Match the chosen source atom to an atom, or source bond to a bond.");
    }

    let mut best: Option<(f32, Document, Vec<u64>)> = None;
    let mut consider =
        |source: &[u64], target: &[u64], origin: Point, dest: Point, scale: f32, angle: f32| {
            let mut positioned = part.clone();
            let all = positioned.all_ids();
            editing::transform_about(&mut positioned, &all, origin, scale, angle.to_degrees());
            positioned.translate(&all, dest.x - origin.x, dest.y - origin.y);
            let length = crate::style::DEFAULT.bond_length_world * scale;
            let mut score = 0.0;
            let mut center = Point::default();
            let mut count = 0;
            for a in &positioned.atoms {
                if source.contains(&a.id) {
                    continue;
                }
                center = center.offset(a.position.x, a.position.y);
                count += 1;
                for other in &doc.atoms {
                    let distance = a.position.distance(other.position) / length;
                    score += (0.75 - distance).max(0.0).powi(2) * 1000.0;
                    score += 0.05 / (distance + 0.1).powi(2);
                }
            }
            // A single atom or edge can be shared too. Existing-fragment joining
            // then removes the duplicate while retaining its captions and groups.
            if count == 0 {
                center = dest;
            } else {
                center.x /= count as f32;
                center.y /= count as f32;
            }
            if let Some(direction) = direction.filter(|p| p.distance(dest) > radius) {
                let aim = (direction.y - dest.y).atan2(direction.x - dest.x);
                let actual = (center.y - dest.y).atan2(center.x - dest.x);
                score += (1.0 - (aim - actual).cos()) * 10000.0;
            }
            if best
                .as_ref()
                .is_some_and(|(previous, _, _)| score >= *previous)
            {
                return;
            }
            let mut result = doc.clone();
            let ids = editing::append(&mut result, &positioned, Point::default());
            if ids.len() != part.all_ids().len() {
                return;
            }
            let added: HashMap<_, _> = part
                .all_ids()
                .into_iter()
                .zip(ids.iter().copied())
                .collect();
            let mapping: HashMap<_, _> = source
                .iter()
                .zip(target)
                .filter_map(|(a, b)| added.get(a).map(|id| (*id, *b)))
                .collect();
            let mapped = |id: u64| mapping.get(&id).copied().unwrap_or(id);
            let mut affected = target.to_vec();
            affected.extend(mapping.keys());
            result.invalidate_chemistry(&affected);
            result.atoms.retain(|a| !mapping.contains_key(&a.id));
            result
                .bonds
                .retain(|b| !(mapping.contains_key(&b.a) && mapping.contains_key(&b.b)));
            for b in &mut result.bonds {
                b.a = mapped(b.a);
                b.b = mapped(b.b);
                for id in &mut b.stereo_atoms {
                    *id = mapped(*id);
                }
            }
            // The templates contain no stereo at attachment sites. Retain remote stereo.
            for a in &mut result.atoms {
                if let Some(stereo) = &mut a.stereo {
                    for id in &mut stereo.neighbors {
                        *id = mapped(*id);
                    }
                }
            }
            for group in &mut result.groups {
                for id in &mut group.members {
                    *id = mapped(*id);
                }
                group.members.sort_unstable();
                group.members.dedup();
            }
            for group in &mut result.abbreviations {
                group.anchor = mapped(group.anchor);
                for id in &mut group.members {
                    *id = mapped(*id);
                }
                group.members.sort_unstable();
                group.members.dedup();
            }
            result.reconcile_molecule_groups();
            if crate::reactions::reconcile(&mut result).is_err() || result.validate().is_err() {
                return;
            }
            best = Some((score, result, ids.into_iter().map(mapped).collect()));
        };

    if let Some(index) = bond {
        let target = doc
            .bonds
            .get(index)
            .ok_or("The target bond is unavailable")?;
        if target.display != "plain" || target.stereo.is_some() || !matches!(target.order, 1 | 2) {
            return Err(
                "Choose a plain single or double bond; Clean up converts aromatic bond orders.",
            );
        }
        let saturated = part
            .bonds
            .iter()
            .all(|b| b.order == 1 && b.display == "plain");
        let ta = doc
            .atom(target.a)
            .ok_or("The target bond has a missing atom")?;
        let tb = doc
            .atom(target.b)
            .ok_or("The target bond has a missing atom")?;
        for source in &part.bonds {
            if let Anchor::Bond(a, b) = anchor
                && !((source.a == a && source.b == b) || (source.a == b && source.b == a))
            {
                continue;
            }
            // A saturated ring may inherit an existing double edge, creating a
            // cycloalkene. Unsaturated fragments require the same source order
            // to avoid silently adding/removing unsaturation elsewhere.
            if (source.order != target.order && !(saturated && target.order == 2))
                || source.display != "plain"
                || source.stereo.is_some()
            {
                continue;
            }
            for (a, b) in [(source.a, source.b), (source.b, source.a)] {
                let sa = part.atom(a).ok_or("The template bond has a missing atom")?;
                let sb = part.atom(b).ok_or("The template bond has a missing atom")?;
                let shared = 2 * source.order as u32;
                if !compatible(sa, ta)
                    || !compatible(sb, tb)
                    || valence(part, a) + valence(doc, ta.id) - shared > capacity(ta)
                    || valence(part, b) + valence(doc, tb.id) - shared > capacity(tb)
                {
                    continue;
                }
                let source_length = sa.position.distance(sb.position);
                let target_length = ta.position.distance(tb.position);
                if source_length < 0.001 || target_length < 0.001 {
                    continue;
                }
                let angle = (tb.position.y - ta.position.y).atan2(tb.position.x - ta.position.x)
                    - (sb.position.y - sa.position.y).atan2(sb.position.x - sa.position.x);
                consider(
                    &[a, b],
                    &[ta.id, tb.id],
                    midpoint(sa.position, sb.position),
                    midpoint(ta.position, tb.position),
                    target_length / source_length,
                    angle,
                );
            }
        }
    } else if let Some(id) = atom {
        let target = doc.atom(id).ok_or("The attachment atom is unavailable")?;
        let neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == id {
                    doc.atom(b.b)
                } else if b.b == id {
                    doc.atom(b.a)
                } else {
                    None
                }
            })
            .collect();
        let length = if neighbors.is_empty() {
            crate::style::DEFAULT.bond_length_world
        } else {
            neighbors
                .iter()
                .map(|a| a.position.distance(target.position))
                .sum::<f32>()
                / neighbors.len() as f32
        };
        if length < 0.001 {
            return Err("Choose an atom with nonzero bond lengths.");
        }
        for source in &part.atoms {
            if let Anchor::Atom(id) = anchor
                && source.id != id
            {
                continue;
            }
            if !compatible(source, target)
                || valence(part, source.id) + valence(doc, id) > capacity(target)
            {
                continue;
            }
            let anchor_bond = part
                .bonds
                .iter()
                .find(|b| b.a == source.id || b.b == source.id);
            let Some(b) = anchor_bond else {
                consider(
                    &[source.id],
                    &[id],
                    source.position,
                    target.position,
                    1.,
                    0.,
                );
                continue;
            };
            let source_length = part
                .atom(b.a)
                .ok_or("The template bond has a missing atom")?
                .position
                .distance(
                    part.atom(b.b)
                        .ok_or("The template bond has a missing atom")?
                        .position,
                );
            if source_length < 0.001 {
                continue;
            }
            for step in 0..24 {
                consider(
                    &[source.id],
                    &[id],
                    source.position,
                    target.position,
                    length / source_length,
                    step as f32 * std::f32::consts::TAU / 24.0,
                );
            }
        }
    }
    best.map(|(_, doc, ids)| (doc, ids)).ok_or(
        "No compatible attachment: match elements and an eligible bond, with room for the new bonds.",
    )
}
