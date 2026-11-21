//! Erase the geometry crossed by a pointer stroke, including between motion events.
use crate::{
    document::{Document, Point},
    graphics::{
        PathCommand, flattened,
        table::{CellRef, Frame, Membership, clip},
    },
    models3d::Display,
};
use std::collections::HashMap;

/// A stroke is restricted to each owner's affine cell before querying the
/// complete editable geometry. A long motion event can cross several cells.
struct CellStrokes {
    owners: Membership,
    frames: HashMap<CellRef, Option<Frame>>,
}

impl CellStrokes {
    fn new(doc: &Document) -> Option<Self> {
        if !doc.graphics.iter().any(|g| g.table.is_some()) {
            return Some(Self {
                owners: Membership::default(),
                frames: HashMap::new(),
            });
        }
        let owners = Membership::new(doc).ok()?;
        let frames = doc
            .graphics
            .iter()
            .filter_map(|g| g.table.as_ref().map(|t| (g, t)))
            .flat_map(|(g, table)| {
                table.cells.iter().map(move |cell| {
                    (
                        CellRef {
                            table: g.id,
                            cell: cell.id,
                        },
                        cell.frame(g),
                    )
                })
            })
            .collect();
        Some(Self { owners, frames })
    }

    fn frame(&self, id: u64) -> Option<Frame> {
        self.frames
            .get(&self.owners.cell_of(id)?)
            .copied()
            .flatten()
    }

    fn segment(&self, id: u64, from: Point, to: Point) -> Option<(Point, Point)> {
        if self.owners.cell_of(id).is_none() {
            return Some((from, to));
        }
        clip_segment(self.frame(id)?, from, to)
    }

    fn box_hit(&self, id: u64, lo: Point, hi: Point, from: Point, to: Point, radius: f32) -> bool {
        self.frame(id).map_or_else(
            || box_hit(lo, hi, from, to, radius),
            |frame| clip::box_hit(frame, lo, hi, from, to, radius),
        )
    }

    fn arrow_hit(
        &self,
        id: u64,
        part: &crate::arrows::ArrowPath,
        from: Point,
        to: Point,
        radius: f32,
    ) -> bool {
        self.frame(id).map_or_else(
            || path_hit(&part.commands, from, to, radius + part.style.width() * 0.5),
            |frame| clip::arrow_hit(frame, part, from, to, radius),
        )
    }

    fn part_hit(
        &self,
        id: u64,
        part: &crate::scientific::Part,
        from: Point,
        to: Point,
        radius: f32,
    ) -> bool {
        self.frame(id).map_or_else(
            || path_hit(&part.commands, from, to, radius + part.style.width() * 0.5),
            |frame| clip::part_hit(frame, part, from, to, radius),
        )
    }
}

/// Slab clipping in the cell's normalized coordinates also handles rotation,
/// skew and reflection. Preserve the original world segment parametrization.
fn clip_segment(frame: Frame, from: Point, to: Point) -> Option<(Point, Point)> {
    clip::segment(frame, from, to)
}

fn distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx * dx + dy * dy;
    let t = if length > 0.000001 {
        ((p.x - a.x) * dx + (p.y - a.y) * dy) / length
    } else {
        0.
    };
    p.distance(a.offset(dx * t.clamp(0., 1.), dy * t.clamp(0., 1.)))
}
fn crossed(a: Point, b: Point, c: Point, d: Point, radius: f32) -> bool {
    if distance(a, c, d)
        .min(distance(b, c, d))
        .min(distance(c, a, b))
        .min(distance(d, a, b))
        <= radius
    {
        return true;
    }
    let (ux, uy, vx, vy) = (b.x - a.x, b.y - a.y, d.x - c.x, d.y - c.y);
    let det = ux * vy - uy * vx;
    if det.abs() < 0.000001 {
        return false;
    }
    let t = ((c.x - a.x) * vy - (c.y - a.y) * vx) / det;
    let s = ((c.x - a.x) * uy - (c.y - a.y) * ux) / det;
    (0. ..=1.).contains(&t) && (0. ..=1.).contains(&s)
}
fn path_hit(commands: &[PathCommand], from: Point, to: Point, radius: f32) -> bool {
    flattened(commands).iter().any(|path| {
        path.windows(2)
            .any(|pair| matches!(pair, [a, b] if crossed(from, to, *a, *b, radius)))
    })
}
fn box_hit(lo: Point, hi: Point, from: Point, to: Point, radius: f32) -> bool {
    let inside = |p: Point| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    inside(from)
        || inside(to)
        || [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y), lo]
            .windows(2)
            .any(|pair| matches!(pair, [a, b] if crossed(from, to, *a, *b, radius)))
}

pub fn stroke(doc: &mut Document, from: Point, to: Point, radius: f32) {
    let Some(clips) = CellStrokes::new(doc) else {
        return;
    };
    let text_style = doc.drawing_style.text_style();
    let mut ids: Vec<_> = doc
        .atoms
        .iter()
        .filter(|a| {
            doc.atom_drawn(a.id)
                && clips
                    .frame(a.id)
                    .is_none_or(|frame| frame.contains(a.position))
                && clips
                    .segment(a.id, from, to)
                    .is_some_and(|(from, to)| distance(a.position, from, to) <= radius)
        })
        .map(|a| a.id)
        .collect();
    let drawn = doc.abbreviation_render_copy();
    // Atom labels and attached marks belong to the atom, even away from its center.
    for atom in drawn.atoms.iter().filter(|a| drawn.atom_drawn(a.id)) {
        let Some((from, to)) = clips.segment(atom.id, from, to) else {
            continue;
        };
        let label = if clips.frame(atom.id).is_some() {
            crate::scene::atom_label_ink_boxes(atom, &drawn)
                .into_iter()
                .any(|(lo, hi)| clips.box_hit(atom.id, lo, hi, from, to, radius))
        } else {
            crate::scene::atom_label_bounds(atom, &drawn)
                .is_some_and(|(lo, hi)| box_hit(lo, hi, from, to, radius))
        };
        if label
            || crate::scientific::document_mark_parts(atom, &drawn)
                .iter()
                .any(|part| {
                    if clips.frame(atom.id).is_some() {
                        clips.part_hit(atom.id, part, from, to, radius)
                    } else {
                        path_hit(&part.commands, from, to, radius)
                    }
                })
        {
            ids.push(atom.id);
        }
    }
    // A shown model erases its whole molecule; its hidden 2D drawing stays out of reach.
    for model in doc.models.iter().filter(|m| m.display != Display::TwoD) {
        let linked = model.linked_ids(doc);
        let Some(&owner) = linked.first() else {
            continue;
        };
        let Some((from, to)) = clips.segment(owner, from, to) else {
            continue;
        };
        let Some((balls, sticks)) = crate::models3d::balls_and_sticks(doc, model) else {
            continue;
        };
        if balls.iter().any(|(c, r)| {
            clips.frame(owner).map_or_else(
                || distance(*c, from, to) <= r + radius,
                |frame| clip::line_hit(frame, *c, *c, *r, from, to, radius),
            )
        }) || sticks.iter().any(|(a, b, half)| {
            clips.frame(owner).map_or_else(
                || crossed(from, to, *a, *b, radius + half),
                |frame| clip::line_hit(frame, *a, *b, *half, from, to, radius),
            )
        }) {
            ids.extend(linked);
        }
    }
    ids.extend(
        doc.annotations
            .iter()
            .filter(|a| {
                let Some((from, to)) = clips.segment(a.id, from, to) else {
                    return false;
                };
                // In the text's own upright frame.
                let (w, h) = clips.frame(a.id).map_or_else(
                    || a.size(),
                    |frame| {
                        let layout = crate::scene::table_annotation_layout(a, frame);
                        (layout.width, layout.height)
                    },
                );
                let centre = a.position.offset(w / 2., h / 2.);
                if let Some(frame) = clips.frame(a.id) {
                    let corners = [
                        a.position,
                        a.position.offset(w, 0.),
                        a.position.offset(w, h),
                        a.position.offset(0., h),
                    ]
                    .map(|p| p.turned(centre, a.angle));
                    return clip::polygon_hit(frame, &corners, from, to, radius);
                }
                let (from, to) = (from.turned(centre, -a.angle), to.turned(centre, -a.angle));
                box_hit(a.position, a.position.offset(w, h), from, to, radius)
            })
            .map(|a| a.id),
    );
    ids.extend(
        doc.arrows
            .iter()
            .filter(|a| {
                let Some((from, to)) = clips.segment(a.id, from, to) else {
                    return false;
                };
                a.paths()
                    .iter()
                    .any(|part| clips.arrow_hit(a.id, part, from, to, radius))
            })
            .map(|a| a.id),
    );
    ids.extend(
        doc.graphics
            .iter()
            .filter(|g| {
                let Some((from, to)) = clips.segment(g.id, from, to) else {
                    return false;
                };
                if let Some(frame) = clips.frame(g.id) {
                    return g
                        .superscript_label(&text_style)
                        .into_iter()
                        .chain(g.panel_titles(&text_style))
                        .any(|label| {
                            label
                                .ink_boxes()
                                .into_iter()
                                .any(|(lo, hi)| clip::box_hit(frame, lo, hi, from, to, radius))
                        })
                        || g.parts()
                            .iter()
                            .chain(g.panel_parts(&text_style).iter())
                            .any(|part| {
                                if g.kind == crate::graphics::GraphicKind::Picture {
                                    clip::path_hit(
                                        frame,
                                        &part.commands,
                                        0.,
                                        true,
                                        false,
                                        from,
                                        to,
                                        radius,
                                    )
                                } else {
                                    clip::part_hit(frame, part, from, to, radius)
                                }
                            });
                }
                g.hit_with_text_style(from, radius, &text_style)
                    || g.hit_with_text_style(to, radius, &text_style)
                    || g.superscript_label(&text_style).is_some_and(|label| {
                        label
                            .ink_boxes()
                            .into_iter()
                            .any(|(lo, hi)| box_hit(lo, hi, from, to, radius))
                    })
                    || g.parts().iter().any(|part| {
                        path_hit(&part.commands, from, to, radius + part.style.width() / 2.)
                    })
            })
            .map(|g| g.id),
    );
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .zip(&drawn.bonds)
        .filter(|(_, b)| drawn.bond_drawn(b.a, b.b))
        .filter(|(_, b)| {
            let Some((from, to)) = clips.segment(b.a, from, to) else {
                return false;
            };
            drawn.atom(b.a).zip(drawn.atom(b.b)).is_some_and(|(a, z)| {
                clips.frame(b.a).map_or_else(
                    || crossed(from, to, a.position, z.position, radius),
                    |frame| {
                        clip::line_hit(
                            frame,
                            a.position,
                            z.position,
                            doc.drawing_style.world(doc.drawing_style.line_width_pt) * 0.5,
                            from,
                            to,
                            radius,
                        )
                    },
                )
            })
        })
        .map(|(b, _)| (b.a, b.b))
        .collect();
    drop(drawn);
    if !ids.is_empty() {
        doc.delete(&crate::shape_text::with_texts(doc, &ids));
    }
    if !bonds.is_empty() {
        let affected: Vec<_> = bonds.iter().flat_map(|(a, b)| [*a, *b]).collect();
        doc.invalidate_chemistry(&affected);
        doc.bonds.retain(|b| !bonds.contains(&(b.a, b.b)));
        crate::ring_fills::prune(doc);
        crate::generic::prune(doc);
    }
}

#[cfg(test)]
mod table_tests;
#[cfg(test)]
mod tests;
