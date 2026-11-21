//! Table cells are independently painted native drawing scopes. Only this
//! derived scene is clipped and wrapped; the document keeps every child whole.
use super::{Document, Point, Primitive};
use crate::graphics::table::{CellRef, Frame, Membership};
use std::borrow::Cow;
use std::collections::HashMap;

/// Derived cell geometry shared by painting and pointer/region picking.
/// No stored position, text width, or ownership is changed by this view.
pub struct Layout {
    frames: HashMap<u64, Option<Frame>>,
}

impl Layout {
    pub fn new(doc: &Document) -> Self {
        let mut frames = HashMap::new();
        for g in &doc.graphics {
            if let Some(table) = &g.table {
                for cell in &table.cells {
                    let frame = cell.frame(g);
                    for &id in &cell.members {
                        frames.insert(id, frame);
                    }
                }
            }
        }
        Self { frames }
    }

    pub fn contains(&self, id: u64, point: Point) -> bool {
        self.frames
            .get(&id)
            .is_none_or(|frame| frame.is_some_and(|frame| frame.contains(point)))
    }

    pub fn annotation<'a>(
        &self,
        annotation: &'a crate::document::Annotation,
    ) -> Cow<'a, crate::document::Annotation> {
        let Some(Some(frame)) = self.frames.get(&annotation.id) else {
            return Cow::Borrowed(annotation);
        };
        let mut effective = annotation.clone();
        wrap(*frame, &mut effective);
        Cow::Owned(effective)
    }

    /// Clip in the cell's affine coordinates, rather than its world AABB.
    pub fn segment(&self, id: u64, a: Point, b: Point) -> Option<(Point, Point)> {
        let Some(frame) = self.frames.get(&id) else {
            return Some((a, b));
        };
        let frame = (*frame)?;
        let (from, to) = (frame.local(a)?, frame.local(b)?);
        let (mut lo, mut hi) = (0f32, 1f32);
        for (start, end) in [(from.x, to.x), (from.y, to.y)] {
            let delta = end - start;
            if delta == 0. {
                if !(0. ..=1.).contains(&start) {
                    return None;
                }
            } else {
                let (u, v) = (-start / delta, (1. - start) / delta);
                lo = lo.max(u.min(v));
                hi = hi.min(u.max(v));
            }
        }
        (lo <= hi).then(|| (a.lerp(b, lo), a.lerp(b, hi)))
    }

    pub fn polygon(&self, id: u64, points: &[Point]) -> Vec<Point> {
        let Some(frame) = self.frames.get(&id) else {
            return points.to_vec();
        };
        let Some(frame) = frame else { return vec![] };
        let Some(mut points) = points
            .iter()
            .map(|p| frame.local(*p))
            .collect::<Option<Vec<_>>>()
        else {
            return vec![];
        };
        for (axis, edge, above) in [(0, 0., true), (0, 1., false), (1, 0., true), (1, 1., false)] {
            let coordinate = |p: Point| if axis == 0 { p.x } else { p.y };
            let inside = |p: Point| {
                if above {
                    coordinate(p) >= edge
                } else {
                    coordinate(p) <= edge
                }
            };
            let mut clipped = vec![];
            for (&a, &b) in points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
            {
                if inside(a) != inside(b) {
                    clipped
                        .push(a.lerp(b, (edge - coordinate(a)) / (coordinate(b) - coordinate(a))));
                }
                if inside(b) {
                    clipped.push(b);
                }
            }
            points = clipped;
        }
        points.into_iter().map(|p| frame.point(p)).collect()
    }
}

fn wrap(frame: Frame, annotation: &mut crate::document::Annotation) {
    if let Some(width) = available(frame, annotation.position, annotation.angle) {
        let width_pt = width * super::STYLE.points_per_world();
        annotation.format.width_pt = Some(
            annotation
                .format
                .width_pt
                .map_or(width_pt, |old| old.min(width_pt)),
        );
    }
}

fn empty(doc: &Document) -> Document {
    Document {
        canvas_theme: doc.canvas_theme,
        color_theme: doc.color_theme,
        custom_theme: doc.custom_theme.clone(),
        drawing_style: doc.drawing_style.clone(),
        atom_labels: doc.atom_labels.clone(),
        highlight_style: doc.highlight_style,
        version: doc.version,
        piece_definitions: doc.piece_definitions.clone(),
        ..Document::default()
    }
}

/// Partition in one pass over the source, retaining source order and exact
/// computed labels. Editor selection/deletion would repair chemistry and links.
fn partition(doc: &Document, owners: &Membership) -> HashMap<Option<CellRef>, Document> {
    let mut parts = HashMap::new();
    parts.insert(None, empty(doc));
    macro_rules! put {
        ($id:expr, $field:ident, $value:expr) => {
            parts
                .entry(owners.cell_of($id))
                .or_insert_with(|| empty(doc))
                .$field
                .push($value.clone());
        };
    }
    for a in &doc.atoms {
        put!(a.id, atoms, a);
    }
    for b in &doc.bonds {
        put!(b.a, bonds, b);
    }
    for a in &doc.annotations {
        put!(a.id, annotations, a);
    }
    for a in &doc.arrows {
        put!(a.id, arrows, a);
    }
    for g in &doc.graphics {
        put!(g.id, graphics, g);
    }
    for g in &doc.abbreviations {
        put!(g.anchor, abbreviations, g);
    }
    for f in &doc.ring_fills {
        if let Some(&id) = f.atoms.first() {
            put!(id, ring_fills, f);
        }
    }
    for s in &doc.depth_appearance {
        if let Some(&id) = s.atoms.first() {
            put!(id, depth_appearance, s);
        }
    }
    for n in &doc.molecule_names {
        put!(n.annotation, molecule_names, n);
    }
    for n in &doc.property_captions {
        put!(n.annotation, property_captions, n);
    }
    for n in &doc.arrow_labels {
        put!(n.annotation, arrow_labels, n);
    }
    for g in &doc.groups {
        if let Some(&id) = g.members.first() {
            put!(id, groups, g);
        }
    }
    for r in &doc.reactions {
        put!(r.arrow, reactions, r);
    }
    for m in &doc.models {
        if let Some(&id) = m.linked_ids(doc).first() {
            put!(id, models, m);
        }
    }
    for s in &doc.seats {
        put!(s.a.piece, seats, s);
    }
    for s in &doc.generic.alternatives {
        parts
            .entry(owners.cell_of(s.atom))
            .or_insert_with(|| empty(doc))
            .generic
            .alternatives
            .push(s.clone());
    }
    for s in &doc.generic.links {
        parts
            .entry(owners.cell_of(s.atom))
            .or_insert_with(|| empty(doc))
            .generic
            .links
            .push(s.clone());
    }
    for s in &doc.generic.repeats {
        if let Some(id) = s.bracket.or_else(|| s.members.first().copied()) {
            parts
                .entry(owners.cell_of(id))
                .or_insert_with(|| empty(doc))
                .generic
                .repeats
                .push(s.clone());
        }
    }
    for &id in &doc.on_other_canvas {
        parts
            .entry(owners.cell_of(id))
            .or_insert_with(|| empty(doc))
            .on_other_canvas
            .insert(id);
    }
    parts
}

/// Distance from an annotation's top-left along its baseline to the affine
/// cell boundary. Rotation/reflection therefore preserves the wrapping width.
fn available(frame: Frame, position: Point, angle: f32) -> Option<f32> {
    let p = frame.local(position)?;
    if !(0. ..1.).contains(&p.x) || !(0. ..1.).contains(&p.y) {
        return None;
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    let determinant = frame.axis_x.x * frame.axis_y.y - frame.axis_x.y * frame.axis_y.x;
    let dx = (cos * frame.axis_y.y - sin * frame.axis_y.x) / determinant;
    let dy = (sin * frame.axis_x.x - cos * frame.axis_x.y) / determinant;
    let edge = |v: f32, dv: f32| {
        if dv > 1e-8 {
            (1. - v) / dv
        } else if dv < -1e-8 {
            -v / dv
        } else {
            f32::INFINITY
        }
    };
    let width = edge(p.x, dx).min(edge(p.y, dy));
    (width.is_finite() && width > 1e-4).then_some(width)
}

pub(super) fn primitives(doc: &Document) -> Vec<Primitive> {
    // Malformed ownership must never paint the same child outside its clip.
    // Stored drawings are validated; an invalid transient view paints no ink.
    let Ok(owners) = Membership::new(doc) else {
        return vec![];
    };
    let mut parts = partition(doc, &owners);
    let mut tables = HashMap::new();
    let canonical = crate::canvas_theme::canonical_document(doc);
    for g in &canonical.graphics {
        let Some(table) = &g.table else {
            continue;
        };
        let mut drawing = Vec::new();
        for cell in &table.cells {
            let Some(frame) = cell.frame(g) else {
                continue;
            };
            let key = Some(CellRef {
                table: g.id,
                cell: cell.id,
            });
            let content = if let Some(mut part) = parts.remove(&key) {
                for a in &mut part.annotations {
                    wrap(frame, a);
                }
                super::primitives_in_cell(&part)
            } else {
                vec![]
            };
            drawing.push(Primitive::Clip {
                corners: frame.corners(),
                primitives: content,
            });
            // Paint each source cell in order, with native vertical/horizontal
            // border overpaint. Adjacent cell edges intentionally overlap.
            drawing.extend(cell.border_parts(g).into_iter().map(|p| Primitive::Path {
                commands: p.commands,
                style: p.style,
                filled: p.filled,
            }));
        }
        tables.insert(g.id, drawing);
    }
    let ordinary = parts.remove(&None).unwrap_or_else(|| empty(doc));
    let flat = crate::models3d::drawn(&ordinary);
    let opacity = crate::rear_opacity::Paint::new(&flat);
    super::primitives_with_tables(&ordinary, &flat, &opacity, &tables, false)
}

#[cfg(test)]
mod tests;
