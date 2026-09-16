use crate::document::{Atom, Document, Point};
use crate::style::{DEFAULT as STYLE, text_width};
use std::fmt::Write;

#[derive(Debug, Clone)]
pub enum Primitive {
    Line(Point, Point, f32),
    Polygon(Vec<Point>),
    Text {
        position: Point,
        text: String,
        size: f32,
        color: [u8; 3],
    },
}
fn visible(a: &Atom, doc: &Document) -> bool {
    a.element != "C"
        || a.charge != 0
        || a.isotope != 0
        || !doc.bonds.iter().any(|b| b.a == a.id || b.b == a.id)
}
fn text(position: Point, text: String, size: f32) -> Primitive {
    Primitive::Text {
        position,
        text,
        size,
        color: [0, 0, 0],
    }
}

fn atom_label(a: &Atom, doc: &Document) -> Vec<Primitive> {
    if !visible(a, doc) {
        return vec![];
    }
    let size = STYLE.font_size();
    let small = size * 0.7;
    let element_width = text_width(&a.element, size);
    let origin = a.position.offset(-element_width / 2.0, -size * 0.58);
    let mut runs = vec![text(origin, a.element.clone(), size)];
    let mut right = origin.x + element_width;
    let isotope_width = if a.isotope > 0 {
        text_width(&a.isotope.to_string(), small)
    } else {
        0.0
    };
    if a.element != "H" && a.label_h > 0 {
        let count = if a.label_h > 1 {
            a.label_h.to_string()
        } else {
            String::new()
        };
        let h_width = text_width("H", size);
        let width = h_width + text_width(&count, small);
        let neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == a.id {
                    doc.atom(b.b)
                } else if b.b == a.id {
                    doc.atom(b.a)
                } else {
                    None
                }
            })
            .collect();
        let left = !neighbors.is_empty()
            && neighbors
                .iter()
                .map(|n| n.position.x - a.position.x)
                .sum::<f32>()
                > 0.1;
        let x = if left {
            origin.x - isotope_width - width
        } else {
            right
        };
        runs.push(text(Point::new(x, origin.y), "H".into(), size));
        if !count.is_empty() {
            runs.push(text(
                Point::new(x + h_width, origin.y + size * 0.40),
                count,
                small,
            ));
        }
        if !left {
            right += width;
        }
    }
    if a.isotope > 0 {
        let isotope = a.isotope.to_string();
        runs.push(text(
            origin.offset(-text_width(&isotope, small), -size * 0.25),
            isotope,
            small,
        ));
    }
    if a.charge != 0 {
        let amount = if a.charge.abs() > 1 {
            a.charge.abs().to_string()
        } else {
            String::new()
        };
        runs.push(text(
            Point::new(right, origin.y - size * 0.25),
            format!("{amount}{}", if a.charge > 0 { "+" } else { "−" }),
            small,
        ));
    }
    runs
}

fn text_bounds(runs: &[Primitive]) -> Option<(Point, Point)> {
    runs.iter()
        .filter_map(|p| match p {
            Primitive::Text {
                position,
                text,
                size,
                ..
            } => Some((*position, position.offset(text_width(text, *size), *size))),
            _ => None,
        })
        .reduce(|(lo, hi), (a, b)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        })
}

/// Visible selected extents, including atom labels in the original graph.
pub fn selection_bounds(doc: &Document, ids: &[u64]) -> Option<(Point, Point)> {
    let mut points = Vec::new();
    for atom in doc.atoms.iter().filter(|a| ids.contains(&a.id)) {
        points.push(atom.position);
        if let Some((lo, hi)) = text_bounds(&atom_label(atom, doc)) {
            points.extend([lo, hi]);
        }
    }
    for a in doc.annotations.iter().filter(|a| ids.contains(&a.id)) {
        points.extend([a.position, a.position.offset(a.size().0, a.size().1)]);
    }
    for a in doc.arrows.iter().filter(|a| ids.contains(&a.id)) {
        points.extend([a.start, a.end]);
        if a.kind == "curved" {
            points.push(Point::new(
                (a.start.x + a.end.x) / 2.0 - (a.end.y - a.start.y) / 2.0,
                (a.start.y + a.end.y) / 2.0 + (a.end.x - a.start.x) / 2.0,
            ));
        }
    }
    points.into_iter().fold(None, |bounds, p| {
        Some(match bounds {
            None => (p, p),
            Some((lo, hi)) => (
                Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                Point::new(hi.x.max(p.x), hi.y.max(p.y)),
            ),
        })
    })
}

fn label_end(atom: &Atom, ux: f32, uy: f32, bounds: Option<(Point, Point)>, max: f32) -> Point {
    let Some((lo, hi)) = bounds else {
        return atom.position;
    };
    let margin = STYLE.world(STYLE.margin_width_pt);
    let dx = if ux > 0.001 {
        (hi.x + margin - atom.position.x) / ux
    } else if ux < -0.001 {
        (lo.x - margin - atom.position.x) / ux
    } else {
        f32::INFINITY
    };
    let dy = if uy > 0.001 {
        (hi.y + margin - atom.position.y) / uy
    } else if uy < -0.001 {
        (lo.y - margin - atom.position.y) / uy
    } else {
        f32::INFINITY
    };
    let distance = dx.min(dy).clamp(0.0, max);
    atom.position.offset(ux * distance, uy * distance)
}

/// Find the smallest ring containing this edge, using the alternate path.
fn ring_center(doc: &Document, from: u64, to: u64) -> Option<Point> {
    use std::collections::{HashMap, VecDeque};
    let mut previous = HashMap::from([(from, from)]);
    let mut queue = VecDeque::from([from]);
    while let Some(id) = queue.pop_front() {
        for b in &doc.bonds {
            if (b.a == from && b.b == to) || (b.a == to && b.b == from) {
                continue;
            }
            let next = if b.a == id {
                b.b
            } else if b.b == id {
                b.a
            } else {
                continue;
            };
            if previous.contains_key(&next) {
                continue;
            }
            previous.insert(next, id);
            if next == to {
                let mut path = vec![to];
                let mut current = to;
                while current != from {
                    current = previous[&current];
                    path.push(current);
                }
                let points: Vec<_> = path
                    .iter()
                    .filter_map(|id| doc.atom(*id).map(|a| a.position))
                    .collect();
                return Some(Point::new(
                    points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32,
                    points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32,
                ));
            }
            queue.push_back(next);
        }
    }
    None
}

pub fn primitives(doc: &Document) -> Vec<Primitive> {
    let mut out = vec![];
    let labels: std::collections::HashMap<_, _> = doc
        .atoms
        .iter()
        .map(|a| (a.id, atom_label(a, doc)))
        .collect();
    let label_bounds: std::collections::HashMap<_, _> = labels
        .iter()
        .map(|(id, runs)| (*id, text_bounds(runs)))
        .collect();
    for b in &doc.bonds {
        let (Some(a), Some(z)) = (doc.atom(b.a), doc.atom(b.b)) else {
            continue;
        };
        let length = a.position.distance(z.position);
        if length < 0.1 {
            continue;
        }
        let ux = (z.position.x - a.position.x) / length;
        let uy = (z.position.y - a.position.y) / length;
        let start = label_end(a, ux, uy, label_bounds[&a.id], length * 0.45);
        let end = label_end(z, -ux, -uy, label_bounds[&z.id], length * 0.45);
        let nx = -uy;
        let ny = ux;
        match b.display.as_str() {
            "wedge" => out.push(Primitive::Polygon(vec![
                start,
                end.offset(
                    nx * STYLE.world(STYLE.bold_width_pt) / 2.0,
                    ny * STYLE.world(STYLE.bold_width_pt) / 2.0,
                ),
                end.offset(
                    -nx * STYLE.world(STYLE.bold_width_pt) / 2.0,
                    -ny * STYLE.world(STYLE.bold_width_pt) / 2.0,
                ),
            ])),
            "hash" => {
                let spacing = STYLE.world(STYLE.hash_spacing_pt);
                let count = (start.distance(end) / spacing).floor().max(1.0) as u32;
                for i in 1..=count {
                    let t = (i as f32 * spacing / start.distance(end)).min(1.0);
                    let p = Point::new(
                        start.x + (end.x - start.x) * t,
                        start.y + (end.y - start.y) * t,
                    );
                    out.push(Primitive::Line(
                        p.offset(
                            nx * t * STYLE.world(STYLE.bold_width_pt) / 2.0,
                            ny * t * STYLE.world(STYLE.bold_width_pt) / 2.0,
                        ),
                        p.offset(
                            -nx * t * STYLE.world(STYLE.bold_width_pt) / 2.0,
                            -ny * t * STYLE.world(STYLE.bold_width_pt) / 2.0,
                        ),
                        STYLE.line_width(),
                    ));
                }
            }
            "wavy" => {
                let mut prev = start;
                for i in 1..=32 {
                    let t = i as f32 / 32.0;
                    let offset = (t * std::f32::consts::TAU * 4.0).sin() * 2.0;
                    let p = Point::new(
                        start.x + (end.x - start.x) * t + nx * offset,
                        start.y + (end.y - start.y) * t + ny * offset,
                    );
                    out.push(Primitive::Line(prev, p, STYLE.line_width()));
                    prev = p;
                }
            }
            _ => {
                let spacing = STYLE.bond_length_world * STYLE.bond_spacing_ratio;
                let ring = if b.order == 2 || b.order == 4 {
                    ring_center(doc, b.a, b.b)
                } else {
                    None
                };
                let inward = ring
                    .map(|center| {
                        let middle = Point::new(
                            (a.position.x + z.position.x) / 2.0,
                            (a.position.y + z.position.y) / 2.0,
                        );
                        if (center.x - middle.x) * nx + (center.y - middle.y) * ny >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        }
                    })
                    .unwrap_or(1.0);
                let offsets: &[f32] = match b.order {
                    2 if ring.is_some() => &[0.0, spacing * inward],
                    2 => &[-spacing / 2.0, spacing / 2.0],
                    3 => &[-spacing, 0.0, spacing],
                    _ => &[0.0],
                };
                for offset in offsets {
                    let trim = if ring.is_some() && *offset != 0.0 {
                        spacing * 0.75
                    } else {
                        0.0
                    };
                    out.push(Primitive::Line(
                        start.offset(nx * offset + ux * trim, ny * offset + uy * trim),
                        end.offset(nx * offset - ux * trim, ny * offset - uy * trim),
                        STYLE.line_width(),
                    ));
                }
                if b.order == 4 {
                    for i in 0..5 {
                        let t = i as f32 / 5.0;
                        let v = (i as f32 + 0.5) / 5.0;
                        out.push(Primitive::Line(
                            Point::new(
                                start.x + (end.x - start.x) * t + nx * spacing * inward,
                                start.y + (end.y - start.y) * t + ny * spacing * inward,
                            ),
                            Point::new(
                                start.x + (end.x - start.x) * v + nx * spacing * inward,
                                start.y + (end.y - start.y) * v + ny * spacing * inward,
                            ),
                            STYLE.line_width(),
                        ));
                    }
                }
            }
        }
    }
    for a in &doc.atoms {
        out.extend(labels[&a.id].iter().cloned());
    }
    for a in &doc.arrows {
        let angle = (a.end.y - a.start.y).atan2(a.end.x - a.start.x);
        let nx = -angle.sin();
        let ny = angle.cos();
        match a.kind.as_str() {
            "equilibrium" => {
                let a1 = a.start.offset(nx * 3.0, ny * 3.0);
                let b1 = a.end.offset(nx * 3.0, ny * 3.0);
                let a2 = a.end.offset(-nx * 3.0, -ny * 3.0);
                let b2 = a.start.offset(-nx * 3.0, -ny * 3.0);
                out.push(Primitive::Line(a1, b1, STYLE.line_width()));
                head(&mut out, b1, angle, true);
                out.push(Primitive::Line(a2, b2, STYLE.line_width()));
                head(&mut out, b2, angle + std::f32::consts::PI, true);
            }
            "resonance" => {
                out.push(Primitive::Line(a.start, a.end, STYLE.line_width()));
                head(&mut out, a.end, angle, false);
                head(&mut out, a.start, angle + std::f32::consts::PI, false);
            }
            "retro" => {
                for offset in [-2.0, 2.0] {
                    out.push(Primitive::Line(
                        a.start.offset(nx * offset, ny * offset),
                        a.end.offset(
                            -angle.cos() * 5.0 + nx * offset,
                            -angle.sin() * 5.0 + ny * offset,
                        ),
                        STYLE.line_width(),
                    ));
                }
                for sign in [-1.0, 1.0] {
                    out.push(Primitive::Line(
                        a.end,
                        a.end.offset(
                            -angle.cos() * 10.0 + nx * sign * 7.0,
                            -angle.sin() * 10.0 + ny * sign * 7.0,
                        ),
                        STYLE.line_width(),
                    ));
                }
            }
            "curved" => {
                let control = Point::new((a.start.x + a.end.x) / 2.0, (a.start.y + a.end.y) / 2.0)
                    .offset(
                        nx * a.start.distance(a.end) * 0.5,
                        ny * a.start.distance(a.end) * 0.5,
                    );
                let mut prev = a.start;
                for i in 1..=32 {
                    let t = i as f32 / 32.0;
                    let u = 1.0 - t;
                    let p = Point::new(
                        u * u * a.start.x + 2.0 * u * t * control.x + t * t * a.end.x,
                        u * u * a.start.y + 2.0 * u * t * control.y + t * t * a.end.y,
                    );
                    out.push(Primitive::Line(prev, p, STYLE.line_width()));
                    prev = p;
                }
                head(
                    &mut out,
                    a.end,
                    (a.end.y - control.y).atan2(a.end.x - control.x),
                    false,
                );
            }
            _ => {
                out.push(Primitive::Line(a.start, a.end, STYLE.line_width()));
                head(&mut out, a.end, angle, false);
            }
        }
    }
    for a in &doc.annotations {
        for (i, line) in a.text.lines().enumerate() {
            out.push(Primitive::Text {
                position: a.position.offset(0.0, i as f32 * STYLE.line_height()),
                text: line.into(),
                size: STYLE.font_size(),
                color: [0, 0, 0],
            });
        }
    }
    out
}
fn head(out: &mut Vec<Primitive>, end: Point, angle: f32, half: bool) {
    let a = end.offset(
        -angle.cos() * 10.0 - angle.sin() * 3.5,
        -angle.sin() * 10.0 + angle.cos() * 3.5,
    );
    if half {
        out.push(Primitive::Line(end, a, STYLE.line_width()));
    } else {
        let b = end.offset(
            -angle.cos() * 10.0 + angle.sin() * 3.5,
            -angle.sin() * 10.0 - angle.cos() * 3.5,
        );
        out.push(Primitive::Polygon(vec![end, a, b]));
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn bounds(drawing: &[Primitive]) -> (Point, Point) {
    let mut points = vec![];
    for p in drawing {
        match p {
            Primitive::Line(a, b, _) => points.extend([*a, *b]),
            Primitive::Polygon(p) => points.extend(p),
            Primitive::Text {
                position,
                text,
                size,
                ..
            } => {
                points.extend([*position, position.offset(text_width(text, *size), *size)]);
            }
        }
    }
    let Some(first) = points.first().copied() else {
        return (Point::default(), Point::new(100.0, 100.0));
    };
    let (lo, hi) = points.into_iter().fold((first, first), |(lo, hi), p| {
        (
            Point::new(lo.x.min(p.x), lo.y.min(p.y)),
            Point::new(hi.x.max(p.x), hi.y.max(p.y)),
        )
    });
    let pad = STYLE.world(4.0);
    (lo.offset(-pad, -pad), hi.offset(pad, pad))
}
pub fn svg(doc: &Document) -> String {
    let drawing = primitives(doc);
    let (lo, hi) = bounds(&drawing);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}pt\" height=\"{}pt\">\n",
        lo.x,
        lo.y,
        hi.x - lo.x,
        hi.y - lo.y,
        (hi.x - lo.x) * STYLE.points_per_world(),
        (hi.y - lo.y) * STYLE.points_per_world()
    );
    for p in drawing {
        match p {
            Primitive::Line(a, b, w) => {
                writeln!(s,"<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#000000\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>",a.x,a.y,b.x,b.y).unwrap();
            }
            Primitive::Polygon(points) => {
                writeln!(
                    s,
                    "<polygon points=\"{}\" fill=\"#000000\"/>",
                    points
                        .iter()
                        .map(|p| format!("{},{}", p.x, p.y))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .unwrap();
            }
            Primitive::Text {
                position,
                text,
                size,
                color,
            } => {
                writeln!(s,"<text x=\"{}\" y=\"{}\" font-family=\"{}, Helvetica, sans-serif\" font-size=\"{size}\" fill=\"rgb({},{},{})\" dominant-baseline=\"text-before-edge\">{}</text>",position.x,position.y,escape(&STYLE.font_family),color[0],color[1],color[2],escape(&text)).unwrap();
            }
        }
    }
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn svg_preserves_annotations_and_escapes_xml() {
        let mut d = Document::default();
        d.annotations.push(crate::document::Annotation {
            id: 1,
            position: Point::default(),
            text: "A < B & C".into(),
        });
        let xml = svg(&d);
        assert!(xml.contains("A &lt; B &amp; C"));
        assert!(!xml.contains("NaN"));
    }

    #[test]
    fn publication_labels_clear_bonds_and_keep_scripts_separate() {
        let mut d = Document::default();
        let n = d.add_atom("N", Point::default());
        let c = d.add_atom("C", Point::new(42.0, 0.0));
        d.add_bond(n, c, 1, "plain");
        d.atom_mut(n).unwrap().label_h = 2;
        let drawing = primitives(&d);
        let bounds = text_bounds(&drawing).unwrap();
        // A terminal amine bonded to the right must put H2 to its left.
        assert!(
            drawing
                .iter()
                .any(|p| matches!(p, Primitive::Text { text, position, .. }
            if text == "H" && position.x < -10.0))
        );
        assert!(
            drawing
                .iter()
                .any(|p| matches!(p, Primitive::Text { text, size, .. }
            if text == "2" && *size < STYLE.font_size()))
        );
        assert!(
            drawing
                .iter()
                .filter_map(|p| match p {
                    Primitive::Line(a, _, _) => Some(a.x > bounds.1.x),
                    _ => None,
                })
                .all(|clear| clear)
        );
        assert!(
            drawing
                .iter()
                .all(|p| !matches!(p, Primitive::Text { color, .. } if *color != [0, 0, 0]))
        );
        d.atom_mut(n).unwrap().isotope = 15;
        let drawing = primitives(&d);
        let right = text_bounds(&drawing).unwrap().1.x;
        assert!(
            drawing
                .iter()
                .any(|p| matches!(p, Primitive::Line(a, _, _) if a.x > right))
        );
    }

    #[test]
    fn ring_double_bonds_keep_a_continuous_outer_edge() {
        let mut d = Document::default();
        crate::editing::ring(&mut d, Point::default(), 6, false, 5.0);
        for (i, b) in d.bonds.iter_mut().enumerate() {
            if i % 2 == 0 {
                b.order = 2;
            }
        }
        let drawing = primitives(&d);
        let lines: Vec<_> = drawing
            .iter()
            .filter_map(|p| match p {
                Primitive::Line(a, b, _) => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 9);
        assert_eq!(
            lines
                .iter()
                .filter(|(a, b)| (a.distance(*b) - 42.0).abs() < 0.01)
                .count(),
            6
        );
        assert_eq!(
            lines
                .iter()
                .filter(|(a, b)| a.distance(Point::default()) < 41.0
                    && b.distance(Point::default()) < 41.0)
                .count(),
            3
        );
    }
}
