//! Hit testing: which objects a pointer position or region selects, and where a drawn bond attaches.

use super::{Document, World};
use iced::Vector;
use reshiki::chains::BondDrawing;

pub(super) fn region_selection(
    doc: &Document,
    selected: &[u64],
    polygon: &[World],
    mods: iced::keyboard::Modifiers,
) -> Vec<u64> {
    let hits = doc.expand_groups(&reshiki::selection_region::objects(doc, polygon));
    reshiki::selection_region::combine(selected, &hits, mods.shift(), mods.alt())
}

/// Bonds are manipulated through their two endpoint atoms; they have no separate ID.
pub(super) fn hit_selection(doc: &Document, p: World, r: f32) -> Vec<u64> {
    if let Some(i) = reshiki::atom_labels::indicators(doc).into_iter().find(|i| {
        (p.x - i.center.x).abs() < i.width / 2. + r && (p.y - i.center.y).abs() < i.height / 2. + r
    }) {
        return match i.owner {
            reshiki::atom_labels::Owner::Number(id)
            | reshiki::atom_labels::Owner::Mapping(id)
            | reshiki::atom_labels::Owner::AtomStereo(id) => {
                vec![id]
            }
            reshiki::atom_labels::Owner::BondStereo(a, b) => vec![a, b],
        };
    }
    if let Some(id) = hit_object(doc, p, r) {
        return vec![id];
    }
    reshiki::editing::nearest_bond(doc, p, r * 0.7)
        .and_then(|i| doc.bonds.get(i))
        .map(|b| vec![b.a, b.b])
        .unwrap_or_default()
}
/// Use the same attachment resolution for the drag preview and committed bond.
/// Excluding the source lets short drags grow a bond rather than snap to themselves.
#[cfg(test)]
pub(super) fn bond_target(
    doc: &Document,
    start: World,
    end: World,
    source: Option<u64>,
    radius: f32,
) -> (World, Option<u64>) {
    bond_target_with(doc, start, end, source, radius, BondDrawing::default())
}
pub(super) fn bond_target_with(
    doc: &Document,
    start: World,
    end: World,
    source: Option<u64>,
    radius: f32,
    mut drawing: BondDrawing,
) -> (World, Option<u64>) {
    let origin = source.and_then(|id| doc.atom(id));
    if origin.is_some_and(|a| !a.centroid.is_empty()) {
        // A centre-to-metal contact often extends beyond the ring radius.
        // A normal fixed length can land exactly on a member atom instead.
        drawing.fixed_length = false;
    }
    let nearest = |p: World| {
        doc.atoms
            .iter()
            .filter(|a| {
                doc.atom_visible(a.id)
                    && Some(a.id) != source
                    && a.position.distance(p) < radius
                    && !origin.is_some_and(|o| {
                        o.attachment.is_some()
                            && (o.centroid.contains(&a.id) || !a.centroid.is_empty())
                    })
                    && !(a.attachment.is_some()
                        && source.is_some_and(|id| a.centroid.contains(&id)))
            })
            .min_by(|a, b| a.position.distance(p).total_cmp(&b.position.distance(p)))
    };
    let snapped = source
        .and_then(|id| plane_endpoint(doc, id, end, drawing))
        .map(|p| p.position)
        .unwrap_or_else(|| drawing.endpoint(start, end));
    if let Some(atom) = nearest(end).or_else(|| nearest(snapped)) {
        (atom.position, Some(atom.id))
    } else {
        (snapped, None)
    }
}
pub(super) fn plane_endpoint(
    doc: &Document,
    id: u64,
    cursor: World,
    drawing: BondDrawing,
) -> Option<reshiki::projection::growth::Endpoint> {
    reshiki::projection::growth::Plane::at(doc, id)?.endpoint(cursor, drawing)
}
pub fn hit_object(doc: &Document, p: World, r: f32) -> Option<u64> {
    let graphic = |front| {
        doc.graphics
            .iter()
            .enumerate()
            .filter(|(_, g)| (g.layer >= 0) == front && g.hit(p, r))
            .max_by_key(|(i, g)| (g.layer, *i))
            .map(|(_, g)| g.id)
    };
    graphic(true)
        .or_else(|| {
            doc.atoms
                .iter()
                .rev()
                .find(|a| {
                    doc.atom_visible(a.id)
                        && reshiki::scientific::styled_mark_parts(a, &doc.drawing_style)
                            .iter()
                            .any(|part| {
                                reshiki::graphics::flattened(&part.commands)
                                    .iter()
                                    .any(|points| {
                                        points.windows(2).any(
                                    |q| matches!(q, [a, b] if distance_to_segment(p, *a, *b) < r),
                                )
                                    })
                            })
                })
                .map(|a| a.id)
        })
        // A visible atom label owns its H/isotope/charge appendages when
        // selecting, just as it does for hover shortcuts. Keep the exact label
        // bounds so adjacent bonds remain targetable; growth still uses nearest.
        .or_else(|| reshiki::scene::atom_label_hit(doc, p, 0.))
        .or_else(|| doc.nearest(p, r))
        .or_else(|| {
            doc.annotations
                .iter()
                .rev()
                .find(|a| {
                    p.x >= a.position.x
                        && p.x < a.position.x + a.size().0
                        && p.y >= a.position.y
                        && p.y < a.position.y + a.size().1
                })
                .map(|a| a.id)
        })
        .or_else(|| doc.arrows.iter().rev().find(|a| a.hit(p, r)).map(|a| a.id))
        .or_else(|| {
            if reshiki::editing::nearest_bond(doc, p, r * 0.7).is_none() {
                graphic(false)
            } else {
                None
            }
        })
}
pub fn distance_to_segment(p: World, a: World, b: World) -> f32 {
    let v = Vector::new(b.x - a.x, b.y - a.y);
    let len = v.x * v.x + v.y * v.y;
    if len < 0.001 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * v.x + (p.y - a.y) * v.y) / len).clamp(0.0, 1.0);
    p.distance(a.offset(v.x * t, v.y * t))
}
