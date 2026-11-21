//! Hit testing: which objects a pointer position or region selects, and where a drawn bond attaches.

use super::{Document, Tool, World};
use iced::Vector;
use reshiki::chains::BondDrawing;
mod table;
use table::CellClips;

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
    hit_selection_with_object(doc, p, r, |clips| {
        hit_object_in_cells(doc, p, r, foreground_orbital(doc), clips)
    })
}

pub(super) fn hit_selection_with_orbital(
    doc: &Document,
    p: World,
    r: f32,
    foreground_orbital: bool,
) -> Vec<u64> {
    hit_selection_with_object(doc, p, r, |clips| {
        hit_object_in_cells(doc, p, r, foreground_orbital, clips)
    })
}

fn hit_selection_with_object(
    doc: &Document,
    p: World,
    r: f32,
    object: impl FnOnce(&CellClips<'_>) -> Option<u64>,
) -> Vec<u64> {
    let clips = CellClips::new(doc);
    if let Some(i) = reshiki::atom_labels::indicators(doc).into_iter().find(|i| {
        i.owner.drawn(doc)
            && (p.x - i.center.x).abs() < i.width / 2. + r
            && (p.y - i.center.y).abs() < i.height / 2. + r
            && indicator_allowed(i.owner, |id| {
                clips.is_empty()
                    || reshiki::scene::label_ink_boxes(&i.primitives())
                        .into_iter()
                        .any(|(lo, hi)| clips.box_hit(id, lo, hi, p, r))
            })
    }) {
        return match i.owner {
            reshiki::atom_labels::Owner::Number(id)
            | reshiki::atom_labels::Owner::Mapping(id)
            | reshiki::atom_labels::Owner::AtomStereo(id)
            | reshiki::atom_labels::Owner::StereoGroup(id) => {
                vec![id]
            }
            reshiki::atom_labels::Owner::BondStereo(a, b) => vec![a, b],
        };
    }
    let object = object(&clips);
    // A 3D model selects its whole molecule. It is painted above molecules and
    // background graphics, below arrows, text and foreground graphics.
    if let Some(atoms) = model_frame_hit(doc, p, &clips)
        && !object.is_some_and(|id| above_models(doc, id))
    {
        return atoms;
    }
    if let Some(id) = object {
        return vec![id];
    }
    nearest_bond_in_cells(doc, p, r * 0.7, &clips)
        .and_then(|i| doc.bonds.get(i))
        .map(|b| vec![b.a, b.b])
        .unwrap_or_default()
}
pub(super) fn above_models(doc: &Document, id: u64) -> bool {
    doc.arrows.iter().any(|a| a.id == id)
        || doc.annotations.iter().any(|a| a.id == id)
        || doc.graphics.iter().any(|g| g.id == id && g.layer >= 0)
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
    drawing: BondDrawing,
) -> (World, Option<u64>) {
    let (snapped, pointer, near_snap) = bond_targets(doc, start, end, source, radius, drawing);
    match pointer.or(near_snap) {
        Some((position, id)) => (position, Some(id)),
        None => (snapped, None),
    }
}
/// The free endpoint, the atom under the pointer and the atom at the snapped
/// endpoint, each as (position, id).
type BondTargets = (World, Option<(World, u64)>, Option<(World, u64)>);
fn bond_targets(
    doc: &Document,
    start: World,
    end: World,
    source: Option<u64>,
    radius: f32,
    mut drawing: BondDrawing,
) -> BondTargets {
    let clips = CellClips::new(doc);
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
                doc.atom_drawn(a.id)
                    && Some(a.id) != source
                    && a.position.distance(p) < radius
                    && clips.allows(a.id, p)
                    && clips.point_visible(a.id, a.position)
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
    let target = |p: World| nearest(p).map(|atom| (atom.position, atom.id));
    (snapped, target(end), target(snapped))
}
/// A caption must own the hit and the pointer must be on its ink, without padding.
pub fn bond_text_at(doc: &Document, p: World, radius: f32) -> Option<u64> {
    let clips = CellClips::new(doc);
    let id = hit_object_in_cells(doc, p, radius, foreground_orbital(doc), &clips)?;
    let annotation = doc.annotations.iter().find(|a| a.id == id)?;
    clips.annotation_ink_contains(annotation, p).then_some(id)
}

/// An atom under the pointer wins, then a caption under the pointer, then an
/// atom at the snapped endpoint: a fixed length never redirects a drop that
/// lands on caption ink.
pub(super) fn bond_target_with_text(
    doc: &Document,
    start: World,
    end: World,
    source: (Option<u64>, Option<u64>),
    radius: f32,
    drawing: BondDrawing,
    tool: Tool,
) -> Result<(World, Option<u64>, Option<u64>), String> {
    let (snapped, pointer, near_snap) = bond_targets(doc, start, end, source.0, radius, drawing);
    if pointer.is_none()
        && tool.bonds_from_text()
        && let Some(id) = bond_text_at(doc, end, radius).filter(|id| Some(*id) != source.1)
    {
        if let Some(reason) = reshiki::atom_text::text_atom_refusal(doc, id) {
            return Err(reason);
        }
        if let Some(position) = reshiki::atom_text::text_atom_position(doc, id) {
            return Ok((position, None, Some(id)));
        }
    }
    Ok(match pointer.or(near_snap) {
        Some((position, id)) => (position, Some(id), None),
        None => (snapped, None, None),
    })
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
    hit_object_with_orbital(doc, p, r, foreground_orbital(doc))
}

pub(super) fn foreground_orbital(doc: &Document) -> bool {
    doc.graphics
        .iter()
        .any(|g| g.layer >= 0 && matches!(g.kind, reshiki::graphics::GraphicKind::Orbital(_)))
}

fn hit_object_with_orbital(
    doc: &Document,
    p: World,
    r: f32,
    foreground_orbital: bool,
) -> Option<u64> {
    hit_object_in_cells(doc, p, r, foreground_orbital, &CellClips::new(doc))
}

fn hit_object_in_cells(
    doc: &Document,
    p: World,
    r: f32,
    foreground_orbital: bool,
    clips: &CellClips<'_>,
) -> Option<u64> {
    let mark = || {
        doc.atoms
            .iter()
            .rev()
            .find(|a| {
                !a.marks.is_empty()
                    && doc.atom_drawn(a.id)
                    && reshiki::scientific::document_mark_parts(a, doc)
                        .iter()
                        .any(|part| {
                            reshiki::graphics::flattened(&part.commands)
                                .iter()
                                .any(|points| {
                                    points.windows(2).any(
                                        |q| matches!(q, [a, b] if distance_to_segment(p, *a, *b) < r),
                                    )
                                })
                                && clips.part_hit(a.id, part, p, r)
                        })
                    && clips.allows(a.id, p)
            })
            .map(|a| a.id)
    };
    // Only precompute label ownership when a foreground orbital may need to
    // pass through it. Ordinary foreground graphics retain their fast path.
    let label = || reshiki::scene::atom_label_hit_where(doc, p, 0., |id| clips.allows(id, p));
    let orbital_label = foreground_orbital.then(label);
    // A label an orbital reaches, with its attached marks, is painted right
    // above the topmost front orbital reaching it; graphics stacked at or
    // below that point lie under the visible label or mark, which must reach
    // the atom that owns it. A label and another atom's mark may overlap, so
    // the higher of the two counts.
    let raised: Vec<_> = foreground_orbital
        .then(|| [orbital_label.flatten(), mark()])
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    let label_rank = (!raised.is_empty())
        .then(|| reshiki::scientific::orbital_label_halos(doc).atoms)
        .into_iter()
        .flatten()
        .filter(|(id, _)| raised.contains(id))
        .filter_map(|(_, above)| above)
        .filter_map(|orbital| {
            doc.graphics
                .iter()
                .enumerate()
                .find(|(_, g)| g.id == orbital && clips.allows(g.id, p))
                .map(|(i, g)| (g.layer, i))
        })
        .max();
    let graphic = |front| {
        doc.graphics
            .iter()
            .enumerate()
            .filter(|(i, g)| {
                (g.layer >= 0) == front
                    && !(orbital_label.flatten().is_some()
                        && matches!(g.kind, reshiki::graphics::GraphicKind::Orbital(_)))
                    && !label_rank.is_some_and(|rank| (g.layer, *i) <= rank)
                    && (if g.cycle.is_some() {
                        reshiki::cycles::hit(doc, g, p, r) && clips.graphic_hit(doc, g, p, r)
                    } else {
                        (if g.table.is_some() { table::border_hit(g, p, r) } else { g.hit_with_text_style(p, r, &doc.drawing_style.text_style()) })
                            && clips.graphic_hit(doc, g, p, r)
                            // A piece wins only where it shows above its assembly.
                            && (g.piece.is_none()
                                || reshiki::equipment::placed::on_top(doc, g.id, p, r))
                            // Text a shape holds is drawn right after it.
                            || g.text.is_some_and(|text| {
                                doc.annotations.iter().any(|a| {
                                    a.id == text
                                        && clips.held_annotation_contains(a, p)
                                })
                            })
                    })
                    && clips.allows(g.id, p)
            })
            .max_by_key(|(i, g)| (g.layer, *i))
            .map(|(_, g)| g.id)
    };
    graphic(true)
        .or_else(mark)
        // A visible atom label owns its H/isotope/charge appendages when
        // selecting, just as it does for hover shortcuts. Keep the exact label
        // bounds so adjacent bonds remain targetable; growth still uses nearest.
        .or_else(|| orbital_label.unwrap_or_else(label))
        .or_else(|| nearest_atom_in_cells(doc, p, r, clips))
        .or_else(|| {
            doc.annotations
                .iter()
                .rev()
                .find(|a| clips.annotation_contains(a, p))
                .map(|a| a.id)
        })
        .or_else(|| {
            doc.arrows
                .iter()
                .rev()
                .find(|a| {
                    a.hit(p, r)
                        && clips.allows(a.id, p)
                        && a.paths()
                            .iter()
                            .any(|part| clips.arrow_hit(a.id, part, p, r))
                })
                .map(|a| a.id)
        })
        .or_else(|| {
            if nearest_bond_in_cells(doc, p, r * 0.7, clips).is_none() {
                graphic(false)
            } else {
                None
            }
        })
}

pub(crate) fn nearest_atom(doc: &Document, p: World, r: f32) -> Option<u64> {
    nearest_atom_in_cells(doc, p, r, &CellClips::new(doc))
}

pub(crate) fn atom_label_at(doc: &Document, p: World, r: f32) -> Option<u64> {
    let clips = CellClips::new(doc);
    reshiki::scene::atom_label_hit_where(doc, p, r, |id| clips.allows(id, p))
}

pub(crate) fn nearest_bond(doc: &Document, p: World, r: f32) -> Option<usize> {
    nearest_bond_in_cells(doc, p, r, &CellClips::new(doc))
}

pub(super) fn cell_filter(doc: &Document, p: World) -> impl Fn(u64) -> bool + '_ {
    let clips = CellClips::new(doc);
    move |id| clips.allows(id, p)
}

pub(super) fn ring_at(doc: &Document, p: World) -> Option<Vec<u64>> {
    let clips = CellClips::new(doc);
    reshiki::editing::ring_at_where(doc, p, |id| clips.allows(id, p))
}

fn nearest_atom_in_cells(doc: &Document, p: World, r: f32, clips: &CellClips<'_>) -> Option<u64> {
    if clips.is_empty() {
        return doc.nearest(p, r);
    }
    doc.atoms
        .iter()
        .filter(|a| {
            a.position.distance(p) < r
                && doc.atom_drawn(a.id)
                && clips.allows(a.id, p)
                && clips.point_visible(a.id, a.position)
        })
        .min_by(|a, b| a.position.distance(p).total_cmp(&b.position.distance(p)))
        .map(|a| a.id)
        .or_else(|| {
            reshiki::abbreviations::label_hit_in(doc, p, r, |id, lo, hi| {
                clips.box_hit(id, lo, hi, p, r)
            })
        })
}

fn nearest_bond_in_cells(doc: &Document, p: World, r: f32, clips: &CellClips<'_>) -> Option<usize> {
    if clips.is_empty() {
        reshiki::editing::nearest_bond(doc, p, r)
    } else {
        reshiki::editing::nearest_bond_where(doc, p, r, |a, b, start, end| {
            clips.allows(a, p)
                && clips.allows(b, p)
                && clips.line_hit(
                    a,
                    start,
                    end,
                    doc.drawing_style.world(doc.drawing_style.line_width_pt) * 0.5,
                    p,
                    r,
                )
        })
    }
}

pub(super) fn indicator_allowed(
    owner: reshiki::atom_labels::Owner,
    allowed: impl Fn(u64) -> bool,
) -> bool {
    use reshiki::atom_labels::Owner;
    match owner {
        Owner::Number(id) | Owner::Mapping(id) | Owner::AtomStereo(id) | Owner::StereoGroup(id) => {
            allowed(id)
        }
        Owner::BondStereo(a, b) => allowed(a) && allowed(b),
    }
}

fn model_frame_hit(doc: &Document, p: World, clips: &CellClips<'_>) -> Option<Vec<u64>> {
    if clips.is_empty() {
        return reshiki::models3d::frame_hit(doc, p);
    }
    doc.models.iter().rev().find_map(|model| {
        if model.display == reshiki::models3d::Display::TwoD
            || !reshiki::models3d::frame(doc, model).is_some_and(|(lo, hi)| {
                (lo.x..=hi.x).contains(&p.x) && (lo.y..=hi.y).contains(&p.y)
            })
        {
            return None;
        }
        let ids = model.linked_ids(doc);
        ids.iter().all(|id| clips.allows(*id, p)).then_some(ids)
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
