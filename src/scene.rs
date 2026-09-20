use crate::document::{Atom, Bond, Document, Point};
use crate::style::DEFAULT as STYLE;

#[derive(Debug, Clone)]
pub enum Primitive {
    Picture(crate::graphics::Graphic),
    Path {
        commands: Vec<crate::graphics::PathCommand>,
        style: crate::graphics::GraphicStyle,
        filled: bool,
    },
    Line(Point, Point, f32),
    Polygon(Vec<Point>),
    Text {
        position: Point,
        text: String,
        size: f32,
        color: [u8; 3],
        style: crate::typography::TextStyle,
    },
}
fn visible(a: &Atom, doc: &Document) -> bool {
    crate::atom_labels::visible(a, doc)
}

pub(crate) fn atom_label_bounds(a: &Atom, doc: &Document) -> Option<(Point, Point)> {
    text_bounds(&atom_label(a, doc))
}

fn atom_label(a: &Atom, doc: &Document) -> Vec<Primitive> {
    if !doc.atom_visible(a.id) {
        return vec![];
    }
    if let Some(group) = doc.abbreviation(a.id) {
        let style = crate::typography::TextStyle {
            formula: true,
            ..a.text_style.clone().unwrap_or_default()
        };
        let content = group.text(doc);
        let size = STYLE.world(style.size_pt);
        let layout = crate::typography::layout(
            content,
            &crate::typography::TextFormat {
                style: style.clone(),
                ..Default::default()
            },
        );
        let anchor_character = if group.faces_left(doc) {
            content.chars().last()
        } else {
            content.chars().next()
        };
        let half = anchor_character
            .map(|c| crate::style::styled_text_width(&c.to_string(), size, &style) * 0.5)
            .unwrap_or(0.0);
        let origin = a.position.offset(
            if group.faces_left(doc) {
                half - layout.width
            } else {
                -half
            },
            -size * 0.58,
        );
        return layout
            .fragments
            .into_iter()
            .map(|run| Primitive::Text {
                position: origin.offset(run.position.x, run.position.y),
                text: run.text,
                size: run.style.size(),
                color: run.style.color,
                style: run.style,
            })
            .collect();
    }
    if !visible(a, doc) {
        return vec![];
    }
    let style = a.text_style.clone().unwrap_or_default();
    let text_width = |text: &str, size| crate::style::styled_text_width(text, size, &style);
    let text = |position, content, size| Primitive::Text {
        position,
        text: content,
        size,
        color: style.color,
        style: style.clone(),
    };
    let size = STYLE.world(style.size_pt);
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
    if a.element != "H" && a.label_h > 0 && crate::atom_labels::hydrogens(a, doc) {
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
        let auto_left = !neighbors.is_empty()
            && neighbors
                .iter()
                .map(|n| n.position.x - a.position.x)
                .sum::<f32>()
                > 0.1;
        use crate::atom_labels::HydrogenPosition as H;
        let left = a.display.hydrogen_position == H::Left
            || (a.display.hydrogen_position == H::Auto && auto_left);
        let vertical = matches!(a.display.hydrogen_position, H::Above | H::Below);
        let x = if vertical {
            a.position.x - width / 2.
        } else if left {
            origin.x - isotope_width - width
        } else {
            right
        };
        let y = origin.y
            + match a.display.hydrogen_position {
                H::Above => -size * 1.1,
                H::Below => size * 1.1,
                _ => 0.,
            };
        runs.push(text(Point::new(x, y), "H".into(), size));
        if !count.is_empty() {
            runs.push(text(Point::new(x + h_width, y + size * 0.40), count, small));
        }
        if !left && !vertical {
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
    if a.charge != 0
        && !a.marks.iter().any(|m| {
            m.kind.charge()
                && (m.kind != crate::scientific::MarkKind::RadicalIon || a.radical_electrons > 0)
        })
    {
        let amount = if a.charge.unsigned_abs() > 1 {
            a.charge.unsigned_abs().to_string()
        } else {
            String::new()
        };
        let label = format!("{amount}{}", if a.charge > 0 { "+" } else { "−" });
        let width = text_width(&label, small);
        runs.push(text(
            Point::new(right, origin.y - size * 0.25),
            label,
            small,
        ));
        right += width;
    }
    if a.radical_electrons > 0
        && !a.marks.iter().any(|m| {
            m.kind.radical() && (m.kind != crate::scientific::MarkKind::RadicalIon || a.charge != 0)
        })
    {
        runs.push(text(
            Point::new(right, origin.y - size * 0.25),
            "•".repeat(a.radical_electrons as usize),
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
                style,
                ..
            } => Some((
                *position,
                position.offset(crate::style::styled_text_width(text, *size, style), *size),
            )),
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
    for label in crate::atom_labels::indicators(doc)
        .into_iter()
        .filter(|l| l.owner.selected(ids))
    {
        points.extend([label.origin, label.origin.offset(label.width, label.height)]);
    }
    for g in doc.graphics.iter().filter(|g| ids.contains(&g.id)) {
        let (lo, hi) = g.bounds();
        points.extend([lo, hi]);
    }
    for atom in doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id) && doc.atom_visible(a.id))
    {
        points.push(atom.position);
        for part in crate::scientific::mark_parts(atom) {
            points.extend(
                part.commands
                    .iter()
                    .flat_map(crate::graphics::PathCommand::points),
            );
        }
        if let Some((lo, hi)) = text_bounds(&atom_label(atom, doc)) {
            points.extend([lo, hi]);
        }
    }
    for a in doc.annotations.iter().filter(|a| ids.contains(&a.id)) {
        points.extend([a.position, a.position.offset(a.size().0, a.size().1)]);
    }
    for a in doc.arrows.iter().filter(|a| ids.contains(&a.id)) {
        let (lo, hi) = a.bounds();
        points.extend([lo, hi]);
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
    let distance = dx.min(dy).clamp(0.0, max.max(0.0));
    atom.position.offset(ux * distance, uy * distance)
}

/// Resolve automatic positioning identically for drawing and repeated-click edits.
pub fn effective_double_position(doc: &Document, bond: &Bond) -> crate::bonds::DoublePosition {
    use crate::bonds::DoublePosition as P;
    match bond.double_position {
        P::Auto => match automatic_double_side(doc, bond) {
            Some(side) if side < 0. => P::Left,
            Some(_) => P::Right,
            None => P::Center,
        },
        position => position,
    }
}
fn automatic_double_side(doc: &Document, bond: &Bond) -> Option<f32> {
    let center = ring_center(doc, bond.a, bond.b)?;
    let a = doc.atom(bond.a)?.position;
    let b = doc.atom(bond.b)?.position;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let cross = (center.x - (a.x + b.x) * 0.5) * (-dy) + (center.y - (a.y + b.y) * 0.5) * dx;
    Some(if cross >= 0. { 1. } else { -1. })
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
                    current = *previous.get(&current)?;
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
    let mut graphics: Vec<_> = doc.graphics.iter().collect();
    graphics.sort_by_key(|g| g.layer);
    let graphic_primitive = |g: &&crate::graphics::Graphic| {
        if g.kind == crate::graphics::GraphicKind::Picture {
            vec![Primitive::Picture((*g).clone())]
        } else {
            g.parts()
                .into_iter()
                .map(|p| Primitive::Path {
                    commands: p.commands,
                    style: p.style,
                    filled: p.filled,
                })
                .collect()
        }
    };
    out.extend(
        graphics
            .iter()
            .filter(|g| g.layer < 0)
            .flat_map(graphic_primitive),
    );
    let circles = crate::aromatic::circles(doc);
    out.extend(circles.iter().flat_map(|c| {
        c.graphic().parts().into_iter().map(|p| Primitive::Path {
            commands: p.commands,
            style: p.style,
            filled: p.filled,
        })
    }));
    let labels: std::collections::HashMap<_, _> = doc
        .atoms
        .iter()
        .map(|a| (a.id, atom_label(a, doc)))
        .collect();
    let label_bounds: std::collections::HashMap<_, _> = labels
        .iter()
        .map(|(id, runs)| (*id, text_bounds(runs)))
        .collect();
    let crossing_gaps = crate::crossings::gaps(doc);
    for (bond_index, b) in doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| doc.bond_visible(b.a, b.b))
    {
        let (Some(a), Some(z)) = (doc.atom(b.a), doc.atom(b.b)) else {
            continue;
        };
        let length = a.position.distance(z.position);
        if length < 0.1 {
            continue;
        }
        let ux = (z.position.x - a.position.x) / length;
        let uy = (z.position.y - a.position.y) / length;
        let start = label_end(
            a,
            ux,
            uy,
            label_bounds.get(&a.id).copied().flatten(),
            length * 0.45,
        );
        let end = label_end(
            z,
            -ux,
            -uy,
            label_bounds.get(&z.id).copied().flatten(),
            length * 0.45,
        );
        let nx = -uy;
        let ny = ux;
        let bond_start = out.len();
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
            "hollow_wedge" => {
                let width = STYLE.world(STYLE.bold_width_pt) / 2.0;
                let points = [
                    start,
                    end.offset(nx * width, ny * width),
                    end.offset(-nx * width, -ny * width),
                    start,
                ];
                for pair in points.windows(2) {
                    if let [a, b] = pair {
                        out.push(Primitive::Line(*a, *b, STYLE.line_width()));
                    }
                }
            }
            "hash" | "hashed" => {
                let spacing = STYLE.world(STYLE.hash_spacing_pt);
                let count = (start.distance(end) / spacing).floor().max(1.0) as u32;
                for i in 1..=count {
                    let t = (i as f32 * spacing / start.distance(end)).min(1.0);
                    let p = Point::new(
                        start.x + (end.x - start.x) * t,
                        start.y + (end.y - start.y) * t,
                    );
                    let t = if b.display == "hashed" { 1.0 } else { t };
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
            "wavy" if b.order == 2 => {
                let half = STYLE.bond_length_world * STYLE.bond_spacing_ratio / 2.0;
                for sign in [-1.0, 1.0] {
                    out.push(Primitive::Line(
                        start.offset(nx * half * sign, ny * half * sign),
                        end.offset(-nx * half * sign, -ny * half * sign),
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
                let inward = if b.order == 4 {
                    automatic_double_side(doc, b).unwrap_or(1.)
                } else {
                    1.
                };
                use crate::bonds::DoublePosition;
                let position = if matches!(b.order, 2 | 7) {
                    effective_double_position(doc, b)
                } else {
                    DoublePosition::Center
                };
                let side = match position {
                    DoublePosition::Left => Some(-1.),
                    DoublePosition::Right => Some(1.),
                    DoublePosition::Auto | DoublePosition::Center => None,
                };
                let offsets: &[f32] = match (b.order, side) {
                    (2 | 7, Some(side)) => &[0.0, spacing * side],
                    (2 | 7, None) => &[-spacing / 2.0, spacing / 2.0],
                    (6, _) => &[-spacing * 1.5, -spacing * 0.5, spacing * 0.5, spacing * 1.5],
                    (3, _) => &[-spacing, 0.0, spacing],
                    _ => &[0.0],
                };
                for (index, offset) in offsets.iter().enumerate() {
                    let trim = if side.is_some() && [2, 7].contains(&b.order) && *offset != 0.0 {
                        spacing * 0.75
                    } else {
                        0.0
                    };
                    let display = if index == 1 {
                        b.secondary_display.as_deref().unwrap_or(&b.display)
                    } else {
                        &b.display
                    };
                    let first = start.offset(nx * offset + ux * trim, ny * offset + uy * trim);
                    let last = end.offset(nx * offset - ux * trim, ny * offset - uy * trim);
                    if matches!(display, "dashed" | "dotted") {
                        out.push(Primitive::Path {
                            commands: vec![
                                crate::graphics::PathCommand::Move(first),
                                crate::graphics::PathCommand::Line(last),
                            ],
                            style: crate::graphics::GraphicStyle {
                                pattern: if display == "dotted" {
                                    crate::graphics::LinePattern::Dotted
                                } else {
                                    crate::graphics::LinePattern::Dashed
                                },
                                ..Default::default()
                            },
                            filled: false,
                        });
                    } else {
                        out.push(Primitive::Line(
                            first,
                            last,
                            if display == "bold" {
                                STYLE.world(STYLE.bold_width_pt)
                            } else {
                                STYLE.line_width()
                            },
                        ));
                    }
                }
                if b.order == 4 && !circles.iter().any(|c| c.contains_bond(b.a, b.b)) {
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
        if b.order == 5 && b.display == "plain" {
            head(&mut out, end, uy.atan2(ux), false);
        }
        if let Some(gaps) = crossing_gaps.get(bond_index).filter(|g| !g.is_empty()) {
            let bond_primitives = out.drain(bond_start..).collect();
            out.extend(crate::crossings::cut(bond_primitives, gaps));
        }
        if b.color != [0, 0, 0] {
            for primitive in out.iter_mut().skip(bond_start) {
                use crate::graphics::{GraphicStyle, PathCommand};
                match primitive {
                    Primitive::Path { style, .. } => {
                        style.stroke = b.color;
                        if style.fill.is_some() {
                            style.fill = Some(b.color);
                        }
                    }
                    Primitive::Line(a, z, width) => {
                        *primitive = Primitive::Path {
                            commands: vec![PathCommand::Move(*a), PathCommand::Line(*z)],
                            style: GraphicStyle {
                                stroke: b.color,
                                width_pt: *width * STYLE.points_per_world(),
                                ..Default::default()
                            },
                            filled: false,
                        };
                    }
                    Primitive::Polygon(points) => {
                        let Some(first) = points.first() else {
                            continue;
                        };
                        let mut commands = vec![PathCommand::Move(*first)];
                        commands.extend(points.iter().skip(1).map(|p| PathCommand::Line(*p)));
                        commands.push(PathCommand::Close);
                        *primitive = Primitive::Path {
                            commands,
                            style: GraphicStyle {
                                stroke: b.color,
                                fill: Some(b.color),
                                width_pt: 0.0,
                                ..Default::default()
                            },
                            filled: true,
                        };
                    }
                    _ => {}
                }
            }
        }
    }
    for a in doc.atoms.iter().filter(|a| doc.atom_visible(a.id)) {
        out.extend(labels.get(&a.id).into_iter().flatten().cloned());
        out.extend(
            crate::scientific::mark_parts(a)
                .into_iter()
                .map(|p| Primitive::Path {
                    commands: p.commands,
                    style: p.style,
                    filled: p.filled,
                }),
        );
    }
    out.extend(
        crate::atom_labels::indicators(doc)
            .iter()
            .map(|l| l.primitive()),
    );
    for a in &doc.arrows {
        out.extend(a.paths().into_iter().map(|p| Primitive::Path {
            commands: p.commands,
            style: p.style,
            filled: p.filled,
        }));
    }
    for a in &doc.annotations {
        for fragment in crate::typography::layout(&a.text, &a.format).fragments {
            out.push(Primitive::Text {
                position: a.position.offset(fragment.position.x, fragment.position.y),
                text: fragment.text,
                size: fragment.style.size(),
                color: fragment.style.color,
                style: fragment.style,
            });
        }
    }
    out.extend(
        graphics
            .iter()
            .filter(|g| g.layer >= 0)
            .flat_map(graphic_primitive),
    );
    out
}
fn head(out: &mut Vec<Primitive>, end: Point, angle: f32, half: bool) {
    let a = end.offset(
        -angle.cos() * 10. - angle.sin() * 3.5,
        -angle.sin() * 10. + angle.cos() * 3.5,
    );
    if half {
        out.push(Primitive::Line(end, a, STYLE.line_width()));
    } else {
        let b = end.offset(
            -angle.cos() * 10. + angle.sin() * 3.5,
            -angle.sin() * 10. - angle.cos() * 3.5,
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
pub(crate) fn bounds(drawing: &[Primitive]) -> (Point, Point) {
    let mut points = vec![];
    for p in drawing {
        match p {
            Primitive::Picture(g) => {
                let (lo, hi) = g.bounds();
                points.extend([lo, hi]);
            }
            Primitive::Path {
                commands, style, ..
            } => {
                let pad = style.width() * 0.5;
                for p in commands
                    .iter()
                    .flat_map(crate::graphics::PathCommand::points)
                {
                    points.extend([p.offset(-pad, -pad), p.offset(pad, pad)]);
                }
            }
            Primitive::Line(a, b, _) => points.extend([*a, *b]),
            Primitive::Polygon(p) => points.extend(p),
            Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } => {
                points.extend([
                    *position,
                    position.offset(
                        crate::style::styled_text_width(text, *size, style),
                        *size * 1.1,
                    ),
                ]);
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
            Primitive::Picture(g) => {
                use base64::Engine as _;
                if let Some(picture) = &g.picture {
                    let data = base64::engine::general_purpose::STANDARD.encode(picture.png());
                    s.push_str(&format!("<image width=\"1\" height=\"1\" preserveAspectRatio=\"none\" transform=\"matrix({} {} {} {} {} {})\" href=\"data:image/png;base64,{}\"/>",g.axis_x.x,g.axis_x.y,g.axis_y.x,g.axis_y.y,g.origin.x,g.origin.y,data));
                }
            }
            Primitive::Path {
                commands,
                style,
                filled,
            } => {
                use crate::graphics::PathCommand;
                let mut path = String::new();
                for c in commands {
                    match c {
                        PathCommand::Move(p) => path.push_str(&format!("M{} {} ", p.x, p.y)),
                        PathCommand::Line(p) => path.push_str(&format!("L{} {} ", p.x, p.y)),
                        PathCommand::Cubic(a, b, c) => path.push_str(&format!(
                            "C{} {} {} {} {} {} ",
                            a.x, a.y, b.x, b.y, c.x, c.y
                        )),
                        PathCommand::Close => path.push_str("Z "),
                    }
                }
                let fill = if filled {
                    style
                        .fill
                        .map(|c| format!("rgb({},{},{})", c[0], c[1], c[2]))
                        .unwrap_or_else(|| "none".into())
                } else {
                    "none".into()
                };
                let dashes = style
                    .dashes()
                    .iter()
                    .map(f32::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                let dash = if dashes.is_empty() {
                    String::new()
                } else {
                    format!(" stroke-dasharray=\"{dashes}\"")
                };
                s.push_str(&format!("<path d=\"{path}\" fill=\"{fill}\" stroke=\"rgb({},{},{})\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{dash}/>",style.stroke[0],style.stroke[1],style.stroke[2],style.width()));
            }
            Primitive::Line(a, b, w) => {
                s.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#000000\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>",a.x,a.y,b.x,b.y));
            }
            Primitive::Polygon(points) => {
                s.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"#000000\"/>",
                    points
                        .iter()
                        .map(|p| format!("{},{}", p.x, p.y))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                s.push_str(&format!("<text xml:space=\"preserve\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{size}\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{}\" fill=\"rgb({},{},{})\" dominant-baseline=\"text-before-edge\">{}</text>",position.x,position.y,escape(&style.family),if style.bold {"bold"} else {"normal"},if style.italic {"italic"} else {"normal"},if style.underline {"underline"} else {"none"},color[0],color[1],color[2],escape(&text)));
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
            format: Default::default(),
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
