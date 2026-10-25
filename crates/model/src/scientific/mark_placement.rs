//! Click placement and the orbit of a dragged mark. Stored marks never pass
//! through these, so editing a label cannot move an existing annotation.
use super::{AtomMark, MarkKind, Part, styled_mark_parts};
use crate::{
    document::{Atom, Document, Point},
    graphics::{PathCommand, flattened},
    polygon::closed_edges,
    scene::Primitive,
};

type Box2 = (Point, Point);
struct Obstacle {
    points: Vec<Point>,
    width: f32,
    filled: bool,
}

fn font_gap(doc: &Document, atom: &Atom) -> (f32, f32) {
    let font_pt = atom
        .text_style
        .as_ref()
        .map_or(doc.drawing_style.font_size_pt, |style| style.size_pt);
    // The pair sits 7% of the label's font size outside its ink.
    (font_pt, doc.drawing_style.world(font_pt * 0.07))
}

/// Lone pairs turn their dots across the atom-to-mark direction (degrees).
pub(super) fn tangent(mut mark: AtomMark, degrees: f32) -> AtomMark {
    if matches!(mark.kind, MarkKind::LonePair | MarkKind::LonePairBar) {
        mark.angle = (degrees + 90.).rem_euclid(360.);
    }
    mark
}

/// The mark centered in each direction (degrees) at the click gap outside
/// the owner's label ink, and whether it there clears the atom's bonds.
///
/// Each glyph counts as the rounded rectangle `text_ink_shapes` fits to it,
/// so the gap stays even around round letters like O yet clears the corners
/// of N or H. Gaps are measured between convex outlines: the label's (so OH
/// reads as one rounded shape and the orbit never dips between glyphs) and
/// the mark's (so a lone pair keeps the gap like the box a click measures).
pub(super) fn orbit(
    doc: &Document,
    atom: &Atom,
    mark: &AtomMark,
    directions: impl IntoIterator<Item = f32>,
) -> Vec<(AtomMark, bool)> {
    let (font_pt, gap) = font_gap(doc, atom);
    let style = &doc.drawing_style;
    // Separations are found along these normals, 1° apart.
    let normals: Vec<(f32, f32)> = (0..360)
        .map(|i| {
            let (s, c) = (i as f32).to_radians().sin_cos();
            (c, s)
        })
        .collect();
    let dot = |(c, s): (f32, f32), q: Point| q.x * c + q.y * s;
    // How far the label's outline reaches along each normal.
    let shapes = crate::scene::atom_label_ink_shapes(atom, doc);
    let label: Vec<f32> = normals
        .iter()
        .map(|&(c, s)| {
            if shapes.is_empty() {
                return dot((c, s), atom.position) + style.world(font_pt * 0.35);
            }
            shapes
                .iter()
                .map(|&((lo, hi), radius)| {
                    let (lo, hi) = (lo.offset(radius, radius), hi.offset(-radius, -radius));
                    (lo.x * c).max(hi.x * c) + (lo.y * s).max(hi.y * s) + radius
                })
                .fold(f32::NEG_INFINITY, f32::max)
        })
        .collect();
    // Drawn bond spans with their half widths, including parallel lines.
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter(|bond| bond.a == atom.id || bond.b == atom.id)
        .filter_map(|bond| {
            let (a, b) = crate::scene::bond_segment(doc, bond, None)?;
            let wide = bond.stereo.is_some() || bond.display != "plain";
            let width = style.world(if wide {
                style.bold_width_pt.max(style.line_width_pt)
            } else {
                style.line_width_pt
            });
            let parallel = matches!(bond.order, 2 | 3 | 4 | 6 | 7);
            Some((a, b, width * 0.5 + if parallel { spacing } else { 0. }))
        })
        .collect();
    let mut single = atom.clone();
    single.position = Point::new(0., 0.);
    directions
        .into_iter()
        .map(|degrees| {
            let (s, c) = degrees.to_radians().sin_cos();
            let mut placed = tangent(mark.clone(), degrees);
            placed.offset = Point::new(0., 0.);
            single.marks = vec![placed.clone()];
            let parts = styled_mark_parts(&single, style);
            let pad = parts
                .iter()
                .map(|part| part.style.width() * 0.5)
                .fold(0., f32::max);
            let ink: Vec<Point> = parts
                .iter()
                .flat_map(|part| flattened(&part.commands))
                .flatten()
                .collect();
            // How far the mark reaches back against each normal.
            let back: Vec<f32> = normals
                .iter()
                .map(|&n| {
                    ink.iter()
                        .map(|&q| -dot(n, q))
                        .fold(f32::NEG_INFINITY, f32::max)
                })
                .collect();
            // Centered at p, the mark clears the label along normal n by
            // n·p - back - label; along the ray that first reaches the gap at:
            let need = gap + pad;
            let radius = normals
                .iter()
                .zip(back.iter().zip(&label))
                .filter(|&(&n, _)| dot(n, Point::new(c, s)) > 1e-3)
                .map(|(&n, (back, label))| {
                    (need + back + label - dot(n, atom.position)) / dot(n, Point::new(c, s))
                })
                .fold(f32::INFINITY, f32::min)
                .max(0.);
            placed.offset = Point::new(radius * c, radius * s);
            let center = atom.position.offset(placed.offset.x, placed.offset.y);
            let clear = bonds.iter().all(|&(a, b, half)| {
                normals
                    .iter()
                    .zip(&back)
                    .map(|(&n, back)| dot(n, center) - back - dot(n, a).max(dot(n, b)))
                    .fold(f32::NEG_INFINITY, f32::max)
                    >= need + half
            });
            (placed, clear)
        })
        .collect()
}

pub(super) fn lone_pair(doc: &Document, atom: &Atom, kind: MarkKind) -> (Point, f32) {
    let (font_pt, gap) = font_gap(doc, atom);
    let owner_ink = crate::scene::atom_label_ink_boxes(atom, doc);
    // Anchor the pair to the glyph nearest the atom center. Hydrogens and
    // superscripts remain obstacles, rather than pulling the pair toward H
    // or an isotope when another side of the heteroatom is clear.
    let nearest = nearest_ink(&owner_ink, atom.position);
    let anchor_ink: Vec<_> = owner_ink
        .iter()
        .copied()
        .filter(|&bounds| box_distance(bounds, atom.position) <= nearest + 0.001)
        .collect();
    let label_ink: Vec<_> = doc
        .atoms
        .iter()
        .flat_map(|a| crate::scene::atom_label_ink_boxes(a, doc))
        .collect();
    // Use rendered bond/mark geometry, including wedge outlines and double
    // bonds. Free drawing objects are not chemical placement obstacles.
    let mut chemistry = doc.clone();
    chemistry.graphics.clear();
    chemistry.arrows.clear();
    chemistry.annotations.clear();
    let obstacles: Vec<_> = crate::scene::primitives(&chemistry)
        .into_iter()
        .flat_map(primitive_obstacles)
        .collect();
    let candidates: Vec<_> = [(0., -1., 0.), (-1., 0., 90.), (0., 1., 0.), (1., 0., 90.)]
        .into_iter()
        .map(|(dx, dy, angle)| (dx, dy, angle, new_mark_bounds(atom, doc, kind, angle)))
        .collect();
    let mut fallback = (Point::new(0., 0.), 0.);
    // Prefer top, left, bottom, right before increasing the distance. The pair
    // follows the label edge: horizontal above/below, vertical beside it.
    for step in 0..48 {
        for &(dx, dy, angle, shape) in &candidates {
            let side = if dx == 0. { dy } else { dx };
            let (shape_lo, shape_hi) = if dx == 0. {
                (shape.0.y, shape.1.y)
            } else {
                (shape.0.x, shape.1.x)
            };
            let mut radius: f32 = 0.;
            for &(lo, hi) in &anchor_ink {
                let crosses = if dx == 0. {
                    hi.x >= atom.position.x + shape.0.x - gap
                        && lo.x <= atom.position.x + shape.1.x + gap
                } else {
                    hi.y >= atom.position.y + shape.0.y - gap
                        && lo.y <= atom.position.y + shape.1.y + gap
                };
                if crosses {
                    let (label_lo, label_hi, center) = if dx == 0. {
                        (lo.y, hi.y, atom.position.y)
                    } else {
                        (lo.x, hi.x, atom.position.x)
                    };
                    radius = radius.max(if side < 0. {
                        center - label_lo + shape_hi + gap
                    } else {
                        label_hi - center - shape_lo + gap
                    });
                }
            }
            if owner_ink.is_empty() {
                radius = doc.drawing_style.world(font_pt * 0.35) + gap;
            }
            radius += step as f32 * (gap + (shape_hi - shape_lo).max(gap));
            let offset = Point::new(dx * radius, dy * radius);
            let center = atom.position.offset(offset.x, offset.y);
            let bounds = (
                center.offset(shape.0.x, shape.0.y),
                center.offset(shape.1.x, shape.1.y),
            );
            fallback = (offset, angle);
            if label_ink
                .iter()
                .all(|&label| !overlaps(bounds, expand(label, gap * 0.999)))
                && obstacles.iter().all(|obstacle| {
                    !intersects(obstacle, expand(bounds, gap + obstacle.width * 0.5))
                })
            {
                return fallback;
            }
        }
    }
    // Extremely crowded drawings retain a deterministic usable placement;
    // the user can still drag an exact offset or move the new mark afterward.
    fallback
}

/// Keep an unobstructed click default; otherwise try the nearest 15° side.
/// Called only for a newly attached charge or radical, after its label refresh.
pub(super) fn charge_click(doc: &Document, atom: &Atom, mark: &AtomMark) -> Point {
    let (_, gap) = font_gap(doc, atom);
    let labels: Vec<_> = doc
        .atoms
        .iter()
        .flat_map(|a| crate::scene::atom_label_ink_boxes(a, doc))
        .collect();
    let mut chemistry = doc.clone();
    chemistry.graphics.clear();
    chemistry.arrows.clear();
    chemistry.annotations.clear();
    if let Some(owner) = chemistry.atom_mut(atom.id) {
        owner.marks.retain(|m| m.id != mark.id);
    }
    let obstacles: Vec<_> = crate::scene::primitives(&chemistry)
        .into_iter()
        .flat_map(primitive_obstacles)
        .collect();
    let preferred = mark.offset.y.atan2(mark.offset.x).to_degrees();
    let radius = mark.offset.distance(Point::default());
    for turn in std::iter::once(0.).chain((1..=12).flat_map(|i| {
        let turn = i as f32 * super::MARK_ORBIT_STEP;
        [-turn, turn]
    })) {
        let (s, c) = (preferred + turn).to_radians().sin_cos();
        let offset = Point::new(radius * c, radius * s);
        let mut candidate = atom.clone();
        candidate.marks = vec![AtomMark {
            offset,
            ..mark.clone()
        }];
        let bounds = bounds(&styled_mark_parts(&candidate, &doc.drawing_style));
        if labels
            .iter()
            .all(|&label| !overlaps(bounds, expand(label, gap)))
            && obstacles
                .iter()
                .all(|obstacle| !intersects(obstacle, expand(bounds, gap + obstacle.width * 0.5)))
        {
            return offset;
        }
    }
    mark.offset
}

fn primitive_obstacles(primitive: Primitive) -> Vec<Obstacle> {
    match primitive {
        Primitive::Opacity { alpha, primitive } if alpha > 0. => primitive_obstacles(*primitive),
        Primitive::Line(a, b, width) => vec![Obstacle {
            points: vec![a, b],
            width,
            filled: false,
        }],
        Primitive::Polygon(points) => vec![Obstacle {
            points,
            width: 0.,
            filled: true,
        }],
        Primitive::Path {
            commands,
            style,
            filled,
        } => flattened(&commands)
            .into_iter()
            .map(|points| Obstacle {
                points,
                width: style.width(),
                filled,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn new_mark_bounds(atom: &Atom, doc: &Document, kind: MarkKind, angle: f32) -> Box2 {
    let mut candidate = atom.clone();
    candidate.position = Point::new(0., 0.);
    candidate.marks = vec![AtomMark {
        id: None,
        kind,
        offset: Point::new(0., 0.),
        angle,
        size_pt: None,
    }];
    bounds(&styled_mark_parts(&candidate, &doc.drawing_style))
}

pub(super) fn bounds(parts: &[Part]) -> Box2 {
    let mut lo = Point::new(f32::INFINITY, f32::INFINITY);
    let mut hi = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for part in parts {
        let pad = part.style.width() * 0.5;
        for point in part.commands.iter().flat_map(PathCommand::iter_points) {
            lo.x = lo.x.min(point.x - pad);
            lo.y = lo.y.min(point.y - pad);
            hi.x = hi.x.max(point.x + pad);
            hi.y = hi.y.max(point.y + pad);
        }
    }
    (lo, hi)
}
fn nearest_ink(ink: &[Box2], point: Point) -> f32 {
    ink.iter()
        .map(|&bounds| box_distance(bounds, point))
        .fold(f32::INFINITY, f32::min)
}
fn box_distance((lo, hi): Box2, point: Point) -> f32 {
    point.distance(Point::new(
        point.x.clamp(lo.x, hi.x),
        point.y.clamp(lo.y, hi.y),
    ))
}
fn expand((lo, hi): Box2, gap: f32) -> Box2 {
    (lo.offset(-gap, -gap), hi.offset(gap, gap))
}
fn overlaps((a, b): Box2, (c, d): Box2) -> bool {
    a.x < d.x && b.x > c.x && a.y < d.y && b.y > c.y
}
fn contains((lo, hi): Box2, point: Point) -> bool {
    (lo.x..=hi.x).contains(&point.x) && (lo.y..=hi.y).contains(&point.y)
}
fn intersects(obstacle: &Obstacle, bounds: Box2) -> bool {
    if obstacle.points.iter().any(|&p| contains(bounds, p)) {
        return true;
    }
    let (lo, hi) = bounds;
    let edges = [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y), lo];
    let edge_count = obstacle
        .points
        .len()
        .saturating_sub(usize::from(!obstacle.filled));
    if obstacle
        .points
        .iter()
        .zip(obstacle.points.iter().cycle().skip(1))
        .take(edge_count)
        .any(|(a, b)| {
            edges
                .windows(2)
                .any(|edge| matches!(edge, [c, d] if segments_cross(*a, *b, *c, *d)))
        })
    {
        return true;
    }
    if !obstacle.filled {
        return false;
    }
    let center = Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
    let mut inside = false;
    for (a, b) in closed_edges(&obstacle.points) {
        if (a.y > center.y) != (b.y > center.y)
            && center.x < (b.x - a.x) * (center.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}
fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let cross =
        |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);
    if ab_c.abs() < 1e-6 && ab_d.abs() < 1e-6 {
        return a.x.min(b.x) <= c.x.max(d.x)
            && a.x.max(b.x) >= c.x.min(d.x)
            && a.y.min(b.y) <= c.y.max(d.y)
            && a.y.max(b.y) >= c.y.min(d.y);
    }
    ab_c * ab_d <= 0. && cd_a * cd_b <= 0.
}

#[cfg(test)]
mod tests;
