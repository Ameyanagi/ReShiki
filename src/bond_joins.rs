//! Shared, bounded corners for normal, bold and tapered bond outlines.
use crate::document::{Bond, Document, Point};
mod branches;
#[cfg(test)]
mod double_tests;
fn eligible(doc: &Document, b: &Bond) -> bool {
    (b.order == 1
        || b.order == 4 && b.projection
        || matches!(b.order, 2 | 7)
            && match crate::scene::effective_double_position(doc, b) {
                crate::bonds::DoublePosition::Auto => false,
                crate::bonds::DoublePosition::Left | crate::bonds::DoublePosition::Right => true,
                crate::bonds::DoublePosition::Center => false,
            })
        && matches!(
            b.display.as_str(),
            "plain" | "bold" | "wedge" | "hollow_wedge"
        )
}
fn other(b: &Bond, id: u64) -> u64 {
    if b.a == id { b.b } else { b.a }
}
fn neighbors<'a>(doc: &'a Document, bond: &'a Bond, id: u64) -> impl Iterator<Item = &'a Bond> {
    doc.bonds.iter().filter(move |b| {
        (b.a == id || b.b == id)
            && !std::ptr::eq(*b, bond)
            && eligible(doc, b)
            && doc.bond_visible(b.a, b.b)
    })
}
pub fn needed(doc: &Document, b: &Bond) -> bool {
    eligible(doc, b)
        && (b.display != "plain"
            || [b.a, b.b]
                .iter()
                .any(|id| neighbors(doc, b, *id).next().is_some()))
}
fn half_width(doc: &Document, b: &Bond) -> f32 {
    doc.drawing_style.world(
        if b.display == "bold" || matches!(b.display.as_str(), "wedge" | "hollow_wedge") {
            doc.drawing_style.bold_width_pt
        } else {
            doc.drawing_style.line_width_pt
        },
    ) / 2.
}
fn end_width(doc: &Document, b: &Bond, id: u64) -> f32 {
    if matches!(b.display.as_str(), "wedge" | "hollow_wedge") && id == b.a {
        doc.drawing_style.line_width() / 2.
    } else {
        half_width(doc, b)
    }
}
fn cross(a: Point, b: Point) -> f32 {
    a.x * b.y - a.y * b.x
}
fn subtract(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn cap(doc: &Document, b: &Bond, id: u64, point: Point, opposite: Point) -> [Point; 2] {
    let length = point.distance(opposite).max(0.001);
    let u = Point::new(
        (opposite.x - point.x) / length,
        (opposite.y - point.y) / length,
    );
    let width = end_width(doc, b, id);
    let far_width = end_width(doc, b, other(b, id));
    let backbone = branches::backbone(doc, id);
    let corner = |side: f32| {
        let start = point.offset(-u.y * width * side, u.x * width * side);
        if doc.atom(id).is_none_or(|a| {
            a.position.distance(point) > 0.001 || crate::atom_labels::visible(a, doc)
        }) {
            return start;
        }
        if backbone.as_ref().is_some_and(|ring| !ring.contains(b)) {
            return start;
        }
        // Each edge meets its angular neighbour, including three-way junctions.
        let adjacent = neighbors(doc, b, id)
            .filter(|adj| backbone.as_ref().is_none_or(|ring| ring.contains(adj)))
            .filter_map(|adj| {
                let end = doc.atom(other(adj, id))?.position;
                let v = subtract(end, point);
                let angle = (side * cross(u, v).atan2(u.x * v.x + u.y * v.y))
                    .rem_euclid(std::f32::consts::TAU);
                (v.distance(Point::default()) > 0.001 && angle > 0.001).then_some((adj, end, angle))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        let Some((adj, end, _)) = adjacent else {
            return start;
        };
        let adjacent_length = point.distance(end).max(0.001);
        let v = Point::new(
            (end.x - point.x) / adjacent_length,
            (end.y - point.y) / adjacent_length,
        );
        let w = end_width(doc, adj, id);
        let far_w = end_width(doc, adj, other(adj, id));
        let a = point.offset(v.y * w * side, -v.x * w * side);
        let direction = subtract(
            opposite.offset(-u.y * far_width * side, u.x * far_width * side),
            start,
        );
        let adjacent_direction = subtract(end.offset(v.y * far_w * side, -v.x * far_w * side), a);
        let denominator = cross(direction, adjacent_direction);
        if denominator.abs() < 0.001 {
            return start;
        }
        let t = cross(subtract(a, start), adjacent_direction) / denominator;
        let intersection = start.offset(direction.x * t, direction.y * t);
        let offset = subtract(intersection, point);
        let limit = (4. * width.max(w)).min(0.45 * length.min(adjacent_length));
        let scale = (limit / offset.distance(Point::default()).max(0.001)).min(1.);
        point.offset(offset.x * scale, offset.y * scale)
    };
    [corner(1.), corner(-1.)]
}
pub fn polygon(doc: &Document, b: &Bond, start: Point, end: Point) -> Vec<Point> {
    let [al, ar] = cap(doc, b, b.a, start, end);
    let [bl, br] = cap(doc, b, b.b, end, start);
    vec![al, br, bl, ar]
}
/// Keep a ring's outer miter intact and stop substituents at that outline.
pub fn outlines(doc: &Document, b: &Bond, start: Point, end: Point) -> Vec<Vec<Point>> {
    let mut parts = vec![polygon(doc, b, start, end)];
    for (id, point) in [(b.a, start), (b.b, end)] {
        if doc
            .atom(id)
            .is_none_or(|a| a.position.distance(point) > 0.001)
        {
            continue;
        }
        if let Some(ring) = branches::backbone(doc, id)
            && !ring.contains(b)
            && let Some(boundary) = ring.boundary(doc, id)
        {
            parts = parts
                .into_iter()
                .flat_map(|part| branches::outside(part, &boundary))
                .collect();
        }
    }
    parts
}
/// Three or more incident strips bound a small central polygon. Fill that
/// polygon in the same path as the strips, so antialiasing cannot open a seam.
pub fn junctions(doc: &Document) -> Vec<([u8; 3], Vec<Point>)> {
    let mut result = Vec::new();
    for atom in &doc.atoms {
        if !doc.atom_visible(atom.id) || crate::atom_labels::visible(atom, doc) {
            continue;
        }
        let incident: Vec<_> = doc
            .bonds
            .iter()
            .filter(|b| {
                (b.a == atom.id || b.b == atom.id) && eligible(doc, b) && doc.bond_visible(b.a, b.b)
            })
            .collect();
        let Some(first) = incident.first() else {
            continue;
        };
        if incident.len() < 3 {
            continue;
        }
        if branches::backbone(doc, atom.id).is_some() {
            continue;
        }
        // Color is presentation, not connectivity. Join the strips using all
        // incident widths, then divide a mixed-color junction at its center.
        if incident.iter().any(|b| b.color != first.color) {
            let caps: Vec<_> = incident
                .iter()
                .filter_map(|b| {
                    let other = doc.atom(other(b, atom.id))?;
                    Some((b.color, cap(doc, b, atom.id, atom.position, other.position)))
                })
                .collect();
            if caps.is_empty() {
                continue;
            }
            let mut center = Point::default();
            for (_, corners) in &caps {
                for p in corners {
                    center.x += p.x / (2 * caps.len()) as f32;
                    center.y += p.y / (2 * caps.len()) as f32;
                }
            }
            for (color, [a, b]) in caps {
                let mut triangle = vec![a, b, center];
                if cross(subtract(b, a), subtract(center, a)) > 0. {
                    triangle.reverse();
                }
                result.push((color, triangle));
            }
            continue;
        }
        let mut points: Vec<_> = incident
            .iter()
            .filter_map(|b| {
                doc.atom(other(b, atom.id))
                    .map(|a| cap(doc, b, atom.id, atom.position, a.position))
            })
            .flatten()
            .collect();
        points.push(atom.position);
        points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        points.dedup_by(|a, b| a.distance(*b) < 0.0001);
        let mut hull: Vec<Point> = Vec::new();
        for point in &points {
            while hull.len() >= 2 {
                let mut previous = hull.iter().rev();
                let Some(last) = previous.next() else {
                    break;
                };
                let Some(before) = previous.next() else {
                    break;
                };
                if cross(subtract(*last, *before), subtract(*point, *last)) > 0. {
                    break;
                }
                hull.pop();
            }
            hull.push(*point);
        }
        let lower = hull.len();
        for point in points.iter().rev().skip(1) {
            while hull.len() > lower {
                let mut previous = hull.iter().rev();
                let Some(last) = previous.next() else {
                    break;
                };
                let Some(before) = previous.next() else {
                    break;
                };
                if cross(subtract(*last, *before), subtract(*point, *last)) > 0. {
                    break;
                }
                hull.pop();
            }
            hull.push(*point);
        }
        hull.pop();
        // Match the clockwise winding of bond strips, preventing fill cancellation.
        hull.reverse();
        if hull.len() >= 3 {
            result.push((first.color, hull));
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_corners_remain_opaque_across_sizes_widths_orientations_and_tilts() {
        for members in 3..=8 {
            for tilt in [0., 45., 75.] {
                for line_width in [0.6, 1.5] {
                    for mode in ["plain", "bold", "wedge", "mixed"] {
                        let mut doc = Document::default();
                        doc.drawing_style.line_width_pt = line_width;
                        doc.drawing_style.bold_width_pt = line_width * 3.;
                        let ids =
                            crate::editing::ring(&mut doc, Point::default(), members, false, 5.);
                        for (i, bond) in doc.bonds.iter_mut().enumerate() {
                            bond.display = if mode == "mixed" {
                                ["plain", "bold", "wedge"][i % 3]
                            } else {
                                mode
                            }
                            .into();
                            bond.color = [180, 68, 32];
                            if i % 2 == 0 {
                                bond.reverse();
                            }
                        }
                        crate::projection::tilt(&mut doc, &ids, tilt, true);
                        crate::editing::transform(
                            &mut doc,
                            &ids,
                            crate::editing::Transform::Rotate(17.),
                        );
                        let svg = crate::scene::svg(&doc);
                        let xml = roxmltree::Document::parse(&svg).unwrap();
                        let bounds: Vec<f32> = xml
                            .root_element()
                            .attribute("viewBox")
                            .unwrap()
                            .split_whitespace()
                            .map(|s| s.parse().unwrap())
                            .collect();
                        let tree = resvg::usvg::Tree::from_str(
                            &svg,
                            &resvg::usvg::Options {
                                dpi: 72.,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        let scale = 8.;
                        let mut pixmap = resvg::tiny_skia::Pixmap::new(
                            (tree.size().width() * scale).ceil() as u32,
                            (tree.size().height() * scale).ceil() as u32,
                        )
                        .unwrap();
                        resvg::render(
                            &tree,
                            resvg::tiny_skia::Transform::from_scale(scale, scale),
                            &mut pixmap.as_mut(),
                        );
                        for atom in &doc.atoms {
                            let x = ((atom.position.x - bounds[0]) * tree.size().width() * scale
                                / bounds[2])
                                .floor() as u32;
                            let y = ((atom.position.y - bounds[1]) * tree.size().height() * scale
                                / bounds[3])
                                .floor() as u32;
                            assert_eq!(
                                pixmap.pixel(x, y).unwrap().alpha(),
                                255,
                                "gap at atom {}: {members}-ring, tilt {tilt}, width {line_width}, {mode}",
                                atom.id
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn reported_three_way_junctions_have_no_white_pixels_at_the_shared_atoms() {
        let mut doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/bond-join-regression.rsk"))
                .unwrap();
        for color in [[0, 0, 0], [180, 68, 32]] {
            for b in &mut doc.bonds {
                b.color = color;
            }
            let svg = crate::scene::svg(&doc);
            let xml = roxmltree::Document::parse(&svg).unwrap();
            let bounds: Vec<f32> = xml
                .root_element()
                .attribute("viewBox")
                .unwrap()
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            let tree = resvg::usvg::Tree::from_str(
                &svg,
                &resvg::usvg::Options {
                    dpi: 72.,
                    ..Default::default()
                },
            )
            .unwrap();
            for scale in [3., 8., 16.] {
                let mut pixmap = resvg::tiny_skia::Pixmap::new(
                    (tree.size().width() * scale).ceil() as u32,
                    (tree.size().height() * scale).ceil() as u32,
                )
                .unwrap();
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::from_scale(scale, scale),
                    &mut pixmap.as_mut(),
                );
                let sx = tree.size().width() * scale / bounds[2];
                let sy = tree.size().height() * scale / bounds[3];
                for id in [1, 2, 3] {
                    let p = doc.atom(id).unwrap().position;
                    let x = ((p.x - bounds[0]) * sx).floor() as u32;
                    let y = ((p.y - bounds[1]) * sy).floor() as u32;
                    assert_eq!(
                        pixmap.pixel(x, y).unwrap().alpha(),
                        255,
                        "white gap at atom {id}, scale {scale}"
                    );
                }
            }
        }
    }

    #[test]
    fn tapered_bonds_share_wide_end_corners_and_have_normal_width_tips() {
        for style in ["wedge", "hollow_wedge"] {
            let mut d = Document::default();
            let a = d.add_atom("C", Point::new(-30., -40.));
            let joint = d.add_atom("C", Point::default());
            let b = d.add_atom("C", Point::new(30., -40.));
            d.add_bond(a, joint, 1, style);
            d.add_bond(b, joint, 1, style);
            let first = cap(
                &d,
                &d.bonds[0],
                joint,
                Point::default(),
                d.atom(a).unwrap().position,
            );
            let second = cap(
                &d,
                &d.bonds[1],
                joint,
                Point::default(),
                d.atom(b).unwrap().position,
            );
            assert!(first[0].distance(second[1]) < 0.001);
            assert!(first[1].distance(second[0]) < 0.001);
            let tip = cap(
                &d,
                &d.bonds[0],
                a,
                d.atom(a).unwrap().position,
                Point::default(),
            );
            assert!((tip[0].distance(tip[1]) - d.drawing_style.line_width()).abs() < 0.001);
            assert!(
                polygon(
                    &d,
                    &d.bonds[0],
                    d.atom(a).unwrap().position,
                    Point::default()
                )
                .iter()
                .all(|p| p.x.is_finite() && p.y.is_finite())
            );
        }
    }
    #[test]
    fn reported_wedge_junctions_have_shared_atoms_and_complete_outlines() {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/bond-join-regression.rsk"))
                .unwrap();
        doc.validate().unwrap();
        assert_eq!(doc.atoms.len(), 8);
        assert_eq!(doc.bonds.len(), 10);
        assert_eq!(doc.bonds.iter().filter(|b| b.a == 7 || b.b == 7).count(), 2);
        assert_eq!(doc.bonds.iter().filter(|b| b.a == 8 || b.b == 8).count(), 2);
        let scene = crate::scene::primitives(&doc);
        assert!(
            scene
                .iter()
                .any(|p| matches!(p, crate::scene::Primitive::Path { filled: true, .. }))
        );
        assert!(
            scene
                .iter()
                .any(|p| matches!(p, crate::scene::Primitive::Path { filled: false, .. }))
        );
        assert!(crate::assistant::canvas_tools::image(&doc).is_ok());
    }

    #[test]
    fn rasterized_joins_have_ink_on_both_sides_of_the_shared_vertex() {
        for angle in [0., 30., 60., 80.] {
            let mut doc = Document::default();
            let ids = crate::editing::ring(&mut doc, Point::new(100., 100.), 5, false, 5.);
            for b in &mut doc.bonds {
                b.display = "bold".into();
            }
            crate::projection::tilt(&mut doc, &ids, angle, true);
            let svg = crate::scene::svg(&doc);
            let xml = roxmltree::Document::parse(&svg).unwrap();
            let bounds: Vec<f32> = xml
                .root_element()
                .attribute("viewBox")
                .unwrap()
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            let options = resvg::usvg::Options {
                dpi: 72.,
                ..Default::default()
            };
            let tree = resvg::usvg::Tree::from_str(&svg, &options).unwrap();
            let scale = 8.;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(
                (tree.size().width() * scale).ceil() as u32,
                (tree.size().height() * scale).ceil() as u32,
            )
            .unwrap();
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            let sx = tree.size().width() * scale / bounds[2];
            let sy = tree.size().height() * scale / bounds[3];
            for b in &doc.bonds {
                let a = doc.atom(b.a).unwrap().position;
                let z = doc.atom(b.b).unwrap().position;
                for t in [0.02, 0.98] {
                    let p = Point::new(a.x + (z.x - a.x) * t, a.y + (z.y - a.y) * t);
                    let x = ((p.x - bounds[0]) * sx).round() as u32;
                    let y = ((p.y - bounds[1]) * sy).round() as u32;
                    if pixmap.pixel(x, y).unwrap().alpha() <= 200 {
                        let path = std::env::temp_dir().join("reshiki-bond-join-failure.png");
                        pixmap.save_png(&path).unwrap();
                        panic!(
                            "white seam at tilt {angle}: {p:?}, pixel {x},{y}, alpha {} bounds {bounds:?}, image {}",
                            pixmap.pixel(x, y).unwrap().alpha(),
                            path.display()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn thick_thin_and_thick_thick_share_corners_at_many_tilts() {
        for tilt in [0., 30., 60., 80.] {
            for displays in [["bold", "bold"], ["bold", "plain"]] {
                let mut d = Document::default();
                let ids = crate::editing::ring(&mut d, Point::default(), 5, false, 5.);
                d.bonds[0].display = displays[0].into();
                d.bonds[1].display = displays[1].into();
                crate::projection::tilt(&mut d, &ids, tilt, true);
                let a = &d.bonds[0];
                let b = &d.bonds[1];
                let joint = [a.a, a.b]
                    .into_iter()
                    .find(|id| *id == b.a || *id == b.b)
                    .unwrap();
                let p = d.atom(joint).unwrap().position;
                let cap_a = cap(
                    &d,
                    a,
                    joint,
                    p,
                    d.atom(if a.a == joint { a.b } else { a.a })
                        .unwrap()
                        .position,
                );
                let cap_b = cap(
                    &d,
                    b,
                    joint,
                    p,
                    d.atom(if b.a == joint { b.b } else { b.a })
                        .unwrap()
                        .position,
                );
                assert!(
                    cap_a[0].distance(cap_b[1]) < 0.001,
                    "tilt {tilt} {displays:?}"
                );
                assert!(cap_a[1].distance(cap_b[0]) < 0.001);
                for p2 in cap_a {
                    assert!(
                        p2.distance(p) <= 4. * half_width(&d, a).max(half_width(&d, b)) + 0.001
                    );
                }
                d.validate().unwrap();
            }
        }
    }
    #[test]
    fn labels_and_unconnected_crossings_are_not_joined() {
        let mut d = Document::default();
        let a = d.add_atom("N", Point::default());
        let b = d.add_atom("C", Point::new(40., 0.));
        let c = d.add_atom("C", Point::new(0., 40.));
        d.add_bond(a, b, 1, "bold");
        d.add_bond(a, c, 1, "plain");
        let clipped = Point::new(12., 0.);
        let ends = cap(&d, &d.bonds[0], a, clipped, d.atom(b).unwrap().position);
        assert_eq!(ends[0].x, 12.);
        assert_eq!(ends[1].x, 12.);
        let x = d.add_atom("C", Point::new(20., -20.));
        let y = d.add_atom("C", Point::new(20., 20.));
        d.add_bond(x, y, 1, "plain");
        assert!(!needed(&d, &d.bonds[2]));
    }
}
