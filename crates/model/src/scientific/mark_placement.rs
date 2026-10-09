//! Initial click placement only. Stored and dragged marks never pass through
//! this search, so editing a label cannot move an existing annotation.
use super::{AtomMark, MarkKind, Part, styled_mark_parts};
use crate::{
    document::{Atom, Document, Point},
    graphics::{PathCommand, flattened},
    scene::Primitive,
};

type Box2 = (Point, Point);
struct Obstacle {
    points: Vec<Point>,
    width: f32,
    filled: bool,
}

pub(super) fn lone_pair(doc: &Document, atom: &Atom, kind: MarkKind) -> (Point, f32) {
    let font_pt = atom
        .text_style
        .as_ref()
        .map_or(doc.drawing_style.font_size_pt, |style| style.size_pt);
    // A modest positive gap, scaled with the actual label's font size. This is
    // a ReShiki default, not a numerical claim about another application's UI.
    let gap = doc.drawing_style.world(font_pt * 0.07);
    let owner_ink = crate::scene::atom_label_ink_boxes(atom, doc);
    // Anchor the pair to the glyph nearest the atom center. Hydrogens and
    // superscripts remain obstacles, rather than pulling the pair toward H
    // or an isotope when another side of the heteroatom is clear.
    let nearest = owner_ink
        .iter()
        .map(|&bounds| box_distance(bounds, atom.position))
        .fold(f32::INFINITY, f32::min);
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
        .flat_map(|primitive| match primitive {
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
        })
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

fn new_mark_bounds(atom: &Atom, doc: &Document, kind: MarkKind, angle: f32) -> Box2 {
    let mut candidate = atom.clone();
    candidate.position = Point::new(0., 0.);
    candidate.marks = vec![AtomMark {
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
    for (a, b) in obstacle
        .points
        .iter()
        .zip(obstacle.points.iter().cycle().skip(1))
        .take(obstacle.points.len())
    {
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
mod tests {
    use super::*;

    #[test]
    fn a_wedge_closing_edge_blocks_a_mark_even_when_its_center_is_outside_the_fill() {
        let wedge = Obstacle {
            points: vec![
                Point::new(-10., -10.),
                Point::new(10., -10.),
                Point::new(10., 10.),
            ],
            width: 0.,
            filled: true,
        };
        assert!(intersects(
            &wedge,
            (Point::new(-1., -0.5), Point::new(1., 1.5))
        ));
        assert!(!intersects(
            &wedge,
            (Point::new(-5., 2.), Point::new(-3., 4.))
        ));
        assert!(intersects(
            &wedge,
            (Point::new(3., -1.), Point::new(4., 0.))
        ));
    }
}
