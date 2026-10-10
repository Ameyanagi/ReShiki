//! Optional drawing links for mechanism arrows. These references carry no
//! chemical meaning. Stored endpoints are reconciled before history/export.
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, Document, Point},
    scientific::{AtomMark, MarkKind},
    style::DEFAULT,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Atom {
        atom: u64,
    },
    /// `a < b`; fraction is measured from a toward b.
    Bond {
        a: u64,
        b: u64,
        fraction: f32,
    },
    LonePair {
        atom: u64,
        mark: u64,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub target: Target,
    /// Resolved endpoint displacement from the target center. Reconciliation
    /// only increases clearance, so smaller ink cannot pull a fixed end inward.
    pub offset: Point,
    pub direction: Point,
    /// Drawing-space clearance beyond the displayed target ink.
    pub gap: f32,
}
/// A transient hit. Legacy marks acquire an ID only when the arrow is committed.
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    Atom(u64),
    Bond { a: u64, b: u64, fraction: f32 },
    LonePair { atom: u64, mark: AtomMark },
}
fn unit(v: Point) -> Point {
    let n = v.x.hypot(v.y);
    if n > 0.00001 {
        Point::new(v.x / n, v.y / n)
    } else {
        Point::new(0., -1.)
    }
}
fn delta(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn dot(a: Point, b: Point) -> f32 {
    a.x * b.x + a.y * b.y
}
fn lone_pair(kind: MarkKind) -> bool {
    matches!(kind, MarkKind::LonePair | MarkKind::LonePairBar)
}
fn mark_size(doc: &Document, atom: &crate::document::Atom, mark: &AtomMark) -> f32 {
    DEFAULT.world(mark.size_pt.unwrap_or_else(|| {
        atom.text_style
            .as_ref()
            .map_or(doc.drawing_style.font_size_pt, |s| s.size_pt)
            * 0.75
    }))
}
impl Target {
    pub fn center(&self, doc: &Document) -> Option<Point> {
        match *self {
            Self::Atom { atom } => doc.atom(atom).map(|a| a.position),
            Self::Bond { a, b, fraction } => {
                doc.bonds
                    .iter()
                    .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))?;
                let (a, b) = doc.atom(a).zip(doc.atom(b))?;
                Some(a.position.offset(
                    (b.position.x - a.position.x) * fraction,
                    (b.position.y - a.position.y) * fraction,
                ))
            }
            Self::LonePair { atom, mark } => {
                let a = doc.atom(atom)?;
                let m = a
                    .marks
                    .iter()
                    .find(|m| m.id == Some(mark) && lone_pair(m.kind))?;
                Some(a.position.offset(m.offset.x, m.offset.y))
            }
        }
    }
    fn included(&self, ids: &HashSet<u64>) -> bool {
        match *self {
            Self::Atom { atom } | Self::LonePair { atom, .. } => ids.contains(&atom),
            Self::Bond { a, b, .. } => ids.contains(&a) && ids.contains(&b),
        }
    }
    fn remap(&mut self, mapping: &HashMap<u64, u64>) -> bool {
        match self {
            Self::Atom { atom } | Self::LonePair { atom, .. } => {
                let Some(id) = mapping.get(atom) else {
                    return false;
                };
                *atom = *id;
            }
            Self::Bond { a, b, fraction } => {
                let Some((x, y)) = mapping.get(a).zip(mapping.get(b)) else {
                    return false;
                };
                (*a, *b) = (*x, *y);
                if *a > *b {
                    std::mem::swap(a, b);
                    *fraction = 1. - *fraction;
                }
            }
        }
        true
    }
}
impl Anchor {
    fn resolved(&self, doc: &Document, pen: f32) -> Option<Point> {
        let center = self.target.center(doc)?;
        let direction = projection_direction(doc, &self.target, self.direction)?;
        let clearance = clearance(doc, &self.target, direction)?;
        let required = clearance + self.gap + pen;
        let p = center.offset(self.offset.x, self.offset.y);
        let projection = dot(delta(p, center), direction);
        // Leave a subpixel rounding guard only when a correction is needed.
        // Repeated f32 resolution must never walk an endpoint along its normal.
        let correction = if projection < required {
            required - projection + 0.0001
        } else {
            0.
        };
        Some(p.offset(direction.x * correction, direction.y * correction))
    }
    pub fn validate(&self, doc: &Document) -> Result<(), String> {
        if [
            self.offset.x,
            self.offset.y,
            self.direction.x,
            self.direction.y,
            self.gap,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || self.direction.x.hypot(self.direction.y) < 0.00001
            || !(0.0..=1000.).contains(&self.gap)
            || matches!(self.target,Target::Bond{a,b,fraction} if a>=b || !fraction.is_finite() || !(0.0..=1.).contains(&fraction))
            || self.target.center(doc).is_none()
        {
            return Err("Invalid mechanism-arrow attachment".into());
        }
        Ok(())
    }
}
// A bond is a stroke/rail strip rather than a dot. Project against its actual
// normal after an anisotropic stretch; a transformed old normal can become
// nearly parallel to the new bond and cannot certify visible clearance.
fn projection_direction(doc: &Document, target: &Target, hint: Point) -> Option<Point> {
    if let Target::Bond { a, b, .. } = target {
        let (a, b) = doc.atom(*a).zip(doc.atom(*b))?;
        let t = unit(delta(b.position, a.position));
        let n = Point::new(-t.y, t.x);
        Some(if dot(n, hint) >= 0. {
            n
        } else {
            Point::new(-n.x, -n.y)
        })
    } else {
        Some(unit(hint))
    }
}
fn clearance(doc: &Document, target: &Target, direction: Point) -> Option<f32> {
    let center = target.center(doc)?;
    Some(match *target {
        Target::Atom { atom } => {
            let a = doc.atom(atom)?;
            crate::scene::atom_label_ink_boxes(a, doc)
                .into_iter()
                .flat_map(|(lo, hi)| [lo, hi, Point::new(lo.x, hi.y), Point::new(hi.x, lo.y)])
                .map(|p| dot(delta(p, center), direction))
                .fold(doc.drawing_style.line_width() * 0.5, f32::max)
        }
        Target::Bond { a, b, .. } => {
            let bond = doc
                .bonds
                .iter()
                .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))?;
            let style = &doc.drawing_style;
            let stroke = DEFAULT.world(
                if matches!(
                    bond.display.as_str(),
                    "bold" | "wedge" | "hash" | "hashed" | "hollow_wedge"
                ) {
                    style.bold_width_pt
                } else {
                    style.line_width_pt
                },
            ) * 0.5;
            let normal = doc.atom(a).zip(doc.atom(b)).map(|(a, b)| {
                let tangent = unit(delta(b.position, a.position));
                Point::new(-tangent.y, tangent.x)
            })?;
            let rails = match bond.order {
                2 | 3 | 4 | 7 => 1.,
                6 => 1.5,
                _ => 0.,
            };
            let rail_extent = style.bond_length_world * style.bond_spacing_ratio * rails;
            let wave = if bond.display == "wavy" {
                style.line_width() * 1.25
            } else {
                0.
            };
            stroke + dot(normal, direction).abs() * rail_extent.max(wave)
        }
        Target::LonePair { atom, mark } => {
            let a = doc.atom(atom)?;
            let m = a
                .marks
                .iter()
                .find(|m| m.id == Some(mark) && lone_pair(m.kind))?;
            let size = mark_size(doc, a, m);
            let angle = m.angle.to_radians();
            let axis = Point::new(angle.cos(), angle.sin());
            if m.kind == MarkKind::LonePair {
                size * (0.2 * dot(axis, direction).abs() + 0.1)
            } else {
                size * 0.35 * dot(axis, direction).abs() + doc.drawing_style.line_width() * 0.5
            }
        }
    })
}
impl Pick {
    pub fn center(&self, doc: &Document) -> Option<Point> {
        match self {
            Self::Atom(atom) => doc.atom(*atom).map(|a| a.position),
            Self::Bond { a, b, fraction } => Target::Bond {
                a: *a,
                b: *b,
                fraction: *fraction,
            }
            .center(doc),
            Self::LonePair { atom, mark } => doc
                .atom(*atom)
                .filter(|a| a.marks.contains(mark))
                .map(|a| a.position.offset(mark.offset.x, mark.offset.y)),
        }
    }
    pub fn same_target(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bond { a, b, .. }, Self::Bond { a: x, b: y, .. }) => a == x && b == y,
            _ => self == other,
        }
    }
    fn commit(&self, doc: &mut Document) -> Result<Target, String> {
        match self {
            Self::Atom(atom) => Ok(Target::Atom { atom: *atom }),
            Self::Bond { a, b, fraction } => Ok(Target::Bond {
                a: *a,
                b: *b,
                fraction: *fraction,
            }),
            Self::LonePair { atom, mark } => {
                let a = doc.atom_mut(*atom).ok_or("The source atom was removed")?;
                let id = mark
                    .id
                    .map_or_else(|| crate::scientific::next_mark_id(a), Ok)?;
                let m = a
                    .marks
                    .iter_mut()
                    .find(|m| *m == mark)
                    .ok_or("The lone pair changed; choose it again")?;
                m.id = Some(id);
                a.mark_serial = a.mark_serial.max(id);
                Ok(Target::LonePair {
                    atom: *atom,
                    mark: id,
                })
            }
        }
    }
}
/// Hits positioned lone pairs, visible atom labels/vertices, then the visible
/// portion of an existing bond. No absent/imaginary bond is inferred.
pub fn pick(doc: &Document, p: Point, radius: f32) -> Option<Pick> {
    let ink = crate::scene::attachment_ink::AttachmentInk::new(doc);
    for a in doc.atoms.iter().rev().filter(|a| doc.atom_visible(a.id)) {
        for m in a.marks.iter().rev().filter(|m| lone_pair(m.kind)) {
            let center = a.position.offset(m.offset.x, m.offset.y);
            if center.distance(p) <= radius + mark_size(doc, a, m) * 0.3 && ink.mark(a.id) {
                return Some(Pick::LonePair {
                    atom: a.id,
                    mark: m.clone(),
                });
            }
        }
    }
    if let Some(id) = ink
        .label_hit(p, radius * 0.35)
        .or_else(|| ink.nearest_atom(p, radius))
    {
        return Some(Pick::Atom(id));
    }
    doc.bonds
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, b)| doc.bond_visible(b.a, b.b))
        .find_map(|(index, bond)| {
            let (a, b) = if bond.a < bond.b {
                (bond.a, bond.b)
            } else {
                (bond.b, bond.a)
            };
            let (pa, pb) = doc
                .atom(a)
                .zip(doc.atom(b))
                .map(|(a, b)| (a.position, b.position))?;
            let v = delta(pb, pa);
            let length = dot(v, v);
            if length < 0.00001 {
                return None;
            }
            let fraction = dot(delta(p, pa), v) / length;
            let q = pa.offset(v.x * fraction, v.y * fraction);
            // Reject remote candidates before any expensive visibility scene.
            if !(0.0..=1.).contains(&fraction) || q.distance(p) > radius * 0.7 {
                return None;
            }
            let (start, end) = ink.segment(bond)?;
            let from = dot(delta(start, pa), v) / length;
            let to = dot(delta(end, pa), v) / length;
            ((0.0..=1.).contains(&fraction)
                && (from.min(to)..=from.max(to)).contains(&fraction)
                && q.distance(p) <= radius * 0.7
                && ink.label_hit(q, 0.).is_none()
                && ink.bond(index, if bond.a == a { fraction } else { 1. - fraction }))
            .then_some(Pick::Bond { a, b, fraction })
        })
}
// Only new links require displayed ink. Target::center, Anchor::validate and
// reconciliation must continue to retain links to temporarily hidden objects.
fn new_target_visible(
    doc: &Document,
    ink: &crate::scene::attachment_ink::AttachmentInk<'_>,
    target: &Pick,
) -> bool {
    match target {
        Pick::Atom(id) => ink.atom(*id),
        Pick::LonePair { atom, .. } => ink.mark(*atom),
        Pick::Bond { a, b, fraction } => doc
            .bonds
            .iter()
            .enumerate()
            .find(|(_, bond)| (bond.a == *a && bond.b == *b) || (bond.a == *b && bond.b == *a))
            .is_some_and(|(index, bond)| {
                ink.bond(
                    index,
                    if bond.a == *a {
                        *fraction
                    } else {
                        1. - *fraction
                    },
                )
            }),
    }
}
/// Creates one independent cubic. IDs and mark IDs change only after success.
pub fn create(
    doc: &mut Document,
    source: &Pick,
    destination: &Pick,
    preset: Preset,
    style: ArrowStyle,
) -> Result<u64, String> {
    if !matches!(preset, Preset::Curved | Preset::Fishhook) {
        return Err("Attachments require a curved mechanism arrow".into());
    }
    if source.same_target(destination) {
        return Err("Choose a different destination atom, bond, or lone pair".into());
    }
    let (a, b) = source
        .center(doc)
        .zip(destination.center(doc))
        .ok_or("The selected arrow target changed; choose it again")?;
    let ink = crate::scene::attachment_ink::AttachmentInk::new(doc);
    if !new_target_visible(doc, &ink, source) || !new_target_visible(doc, &ink, destination) {
        return Err(
            "The selected arrow target is hidden; choose visible ink or free placement".into(),
        );
    }
    if a.distance(b) < 1. {
        return Err("Arrow targets are too close; use free placement".into());
    }
    let chord = unit(delta(b, a));
    let bond_direction = |pick: &Pick, toward: Point| {
        if let Pick::Bond { a, b, .. } = pick {
            let (a, b) = doc.atom(*a).zip(doc.atom(*b))?;
            let t = unit(delta(b.position, a.position));
            let n = Point::new(-t.y, t.x);
            Some(if dot(n, toward) >= 0. {
                n
            } else {
                Point::new(-n.x, -n.y)
            })
        } else {
            None
        }
    };
    let source_direction = match source {
        Pick::Bond { .. } => bond_direction(source, chord).ok_or("Missing source bond")?,
        Pick::LonePair { mark, .. } if mark.offset.distance(Point::default()) > 0.01 => {
            unit(mark.offset)
        }
        _ => unit(Point::new(chord.x - chord.y * 0.7, chord.y + chord.x * 0.7)),
    };
    let end_direction =
        bond_direction(destination, Point::new(-chord.x, -chord.y)).unwrap_or_else(|| {
            unit(Point::new(
                -chord.x - chord.y * 0.7,
                -chord.y + chord.x * 0.7,
            ))
        });
    let mut candidate = doc.clone();
    let mut start_anchor = Anchor {
        target: source.commit(&mut candidate)?,
        offset: Point::default(),
        direction: source_direction,
        gap: DEFAULT.world(1.2),
    };
    let mut end_anchor = Anchor {
        target: destination.commit(&mut candidate)?,
        offset: Point::default(),
        direction: end_direction,
        gap: DEFAULT.world(1.2),
    };
    let pen = DEFAULT.world(style.width_pt) * 0.5;
    let start = start_anchor
        .resolved(&candidate, pen)
        .ok_or("Missing arrow source")?;
    let end = end_anchor
        .resolved(&candidate, pen)
        .ok_or("Missing arrow destination")?;
    start_anchor.offset = delta(
        start,
        start_anchor
            .target
            .center(&candidate)
            .ok_or("Missing arrow source")?,
    );
    end_anchor.offset = delta(
        end,
        end_anchor
            .target
            .center(&candidate)
            .ok_or("Missing arrow destination")?,
    );
    if start.distance(end) < 1. {
        return Err("No room for the attached arrow; use free placement".into());
    }
    let bend = (start.distance(end) * 0.45).clamp(DEFAULT.world(3.), DEFAULT.world(12.));
    let id = candidate.next_id();
    let mut arrow = Arrow::new(id, start, end, preset, style);
    arrow.cubic = Some([
        start.offset(source_direction.x * bend, source_direction.y * bend),
        end.offset(end_direction.x * bend, end_direction.y * bend),
    ]);
    arrow.start_anchor = Some(start_anchor);
    arrow.end_anchor = Some(end_anchor);
    candidate.arrows.push(arrow);
    reconcile(&mut candidate);
    candidate.validate()?;
    *doc = candidate;
    Ok(id)
}
/// Absolute resolution is idempotent: an already transformed endpoint has no
/// displacement to apply twice. Missing targets detach at their cached position.
pub fn reconcile(doc: &mut Document) {
    // A preset change or a hand-authored native link may carry a legacy
    // quadratic. Degree elevation retains its exact curve while giving each
    // attached end an independent neighboring control.
    for arrow in &mut doc.arrows {
        if (arrow.start_anchor.is_some() || arrow.end_anchor.is_some())
            && arrow.cubic.is_none()
            && matches!(arrow.kind.as_str(), "curved" | "fishhook")
        {
            arrow.cubic = arrow.bezier_controls();
            arrow.control = None;
        }
    }
    let resolved: Vec<_> = doc
        .arrows
        .iter()
        .map(|a| {
            let resolve = |anchor: &Option<Anchor>| {
                let anchor = anchor.as_ref()?;
                Some((
                    anchor.resolved(doc, DEFAULT.world(a.appearance().width_pt) * 0.5)?,
                    anchor.target.center(doc)?,
                ))
            };
            (resolve(&a.start_anchor), resolve(&a.end_anchor))
        })
        .collect();
    for (a, (start, end)) in doc.arrows.iter_mut().zip(resolved) {
        for (index, p) in [(0, start), (1, end)] {
            let anchor = if index == 0 {
                &mut a.start_anchor
            } else {
                &mut a.end_anchor
            };
            if anchor.is_none() {
                continue;
            }
            if let Some((p, center)) = p {
                if let Some(anchor) = anchor {
                    anchor.offset = delta(p, center);
                }
                let old = if index == 0 { a.start } else { a.end };
                let d = delta(p, old);
                if let Some(c) = a.cubic.as_mut().and_then(|c| c.get_mut(index)) {
                    *c = c.offset(d.x, d.y);
                }
                if index == 0 {
                    a.start = p;
                } else {
                    a.end = p;
                }
            } else {
                *anchor = None;
            }
        }
    }
}
pub fn validate(doc: &Document) -> Result<(), String> {
    for a in &doc.atoms {
        let mut ids = HashSet::new();
        for id in a.marks.iter().filter_map(|m| m.id) {
            if id == 0 || id == u64::MAX || !ids.insert(id) {
                return Err("Positioned-mark IDs must be unique within their atom".into());
            }
        }
    }
    for a in &doc.arrows {
        for anchor in [&a.start_anchor, &a.end_anchor].into_iter().flatten() {
            anchor.validate(doc)?;
        }
    }
    Ok(())
}
pub fn remap(arrow: &mut Arrow, mapping: &HashMap<u64, u64>) {
    for a in [&mut arrow.start_anchor, &mut arrow.end_anchor] {
        if a.as_mut().is_some_and(|a| !a.target.remap(mapping)) {
            *a = None;
        }
    }
}
/// Called after selected points and targets have received an affine transform.
/// References to targets outside the selection detach. Kept links rebase their
/// offset to the transformed endpoint before absolute reconciliation.
pub fn transformed(doc: &mut Document, ids: &[u64], vector: impl Fn(Point) -> Point) {
    let ids: HashSet<_> = ids.iter().copied().collect();
    let plans: Vec<_> = doc
        .arrows
        .iter()
        .map(|a| {
            if !ids.contains(&a.id) {
                return None;
            }
            Some(
                [
                    (a.start, a.start_anchor.clone()),
                    (a.end, a.end_anchor.clone()),
                ]
                .map(|(endpoint, anchor)| {
                    let mut anchor = anchor?;
                    if !anchor.target.included(&ids) {
                        return None;
                    }
                    anchor.direction = unit(vector(anchor.direction));
                    let center = anchor.target.center(doc)?;
                    anchor.offset = delta(endpoint, center);
                    Some(anchor)
                }),
            )
        })
        .collect();
    for (arrow, plan) in doc.arrows.iter_mut().zip(plans) {
        if let Some([start, end]) = plan {
            arrow.start_anchor = start;
            arrow.end_anchor = end;
        }
    }
    reconcile(doc);
}
pub const EXPORT_NOTICE: &str = "Arrow curves are preserved; editing attachment links are omitted from this external format. Chemistry is unchanged. Use a native ReShiki file to retain links.";
pub fn export_notice(doc: &Document) -> Option<String> {
    doc.arrows
        .iter()
        .any(|a| a.start_anchor.is_some() || a.end_anchor.is_some())
        .then(|| EXPORT_NOTICE.into())
}
#[cfg(test)]
mod tests;
