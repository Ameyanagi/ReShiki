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
    atom_label_runs(a, doc)
        .into_iter()
        .flat_map(resolve_label_fonts)
        .collect()
}

/// Rendering must use the same fallback face that supplied the ink bounds.
/// Keep the requested family in the document, and split only when glyphs need
/// different faces (for example a custom label mixing Latin and Japanese).
fn resolve_label_fonts(primitive: Primitive) -> Vec<Primitive> {
    let Primitive::Text {
        position,
        text,
        size,
        color,
        style,
    } = primitive
    else {
        return vec![primitive];
    };
    let mut runs = Vec::new();
    let mut x = position.x;
    for c in text.chars() {
        let (family, advance) = crate::style::glyph_metrics(c, &style);
        if let Some(Primitive::Text { text, style, .. }) = runs.last_mut()
            && style.family == family
        {
            text.push(c);
        } else {
            runs.push(Primitive::Text {
                position: Point::new(x, position.y),
                text: c.to_string(),
                size,
                color,
                style: crate::typography::TextStyle {
                    family: family.into(),
                    ..style.clone()
                },
            });
        }
        x += advance * size;
    }
    runs
}

fn atom_label_runs(a: &Atom, doc: &Document) -> Vec<Primitive> {
    if !doc.atom_visible(a.id) || crate::attachments::hidden(a, doc) {
        return vec![];
    }
    let internal_group = doc.abbreviation(a.id).filter(|group| {
        group.alignment.is_auto()
            && crate::atom_labels::condensed_label(&group.label).is_some()
            && doc
                .bonds
                .iter()
                .filter(|bond| {
                    (bond.a == a.id || bond.b == a.id) && doc.bond_visible(bond.a, bond.b)
                })
                .count()
                > 1
    });
    if let Some(group) = doc.abbreviation(a.id).filter(|_| internal_group.is_none()) {
        let style = crate::typography::TextStyle {
            formula: true,
            ..a.text_style
                .clone()
                .unwrap_or_else(|| doc.drawing_style.text_style())
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
        let range = crate::abbreviations::anchor_range(content, group.faces_left(doc));
        let format = crate::typography::TextFormat {
            style: style.clone(),
            ..Default::default()
        };
        let before =
            crate::typography::layout(content.get(..range.start).unwrap_or_default(), &format)
                .width;
        let through =
            crate::typography::layout(content.get(..range.end).unwrap_or_default(), &format).width;
        let anchor_x = (before + through) / 2.;
        let origin = a.position.offset(
            if matches!(
                group.alignment,
                crate::abbreviations::LabelAlignment::Center
                    | crate::abbreviations::LabelAlignment::Above
            ) {
                -layout.width / 2.
            } else {
                -anchor_x
            },
            if group.alignment == crate::abbreviations::LabelAlignment::Above {
                -layout.height - size * 0.35
            } else {
                -crate::style::label_vertical_center(
                    content.get(range.clone()).unwrap_or(content),
                    size,
                    &style,
                )
            },
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
    let show_element = internal_group.is_some() || visible(a, doc);
    if !show_element && a.charge == 0 {
        return vec![];
    }
    let style = a
        .text_style
        .clone()
        .unwrap_or_else(|| doc.drawing_style.text_style());
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
    let label = internal_group
        .map(|group| group.label.as_str())
        .or_else(|| a.display.variable.as_deref().filter(|_| a.element == "*"))
        .unwrap_or(&a.element);
    let condensed = (internal_group.is_some() || a.element == "*")
        .then(|| crate::atom_labels::condensed_label(label))
        .flatten();
    let label = condensed.map_or(label, |(core, _)| core);
    let element_width = text_width(label, size);
    let origin = a.position.offset(
        -element_width / 2.0,
        -crate::style::label_vertical_center(label, size, &style),
    );
    let mut runs = if show_element {
        vec![text(origin, label.to_string(), size)]
    } else {
        vec![]
    };
    let mut right = origin.x + element_width;
    let mut mark_y = origin.y;
    let isotope_width = if a.isotope > 0 {
        text_width(&a.isotope.to_string(), small)
    } else {
        0.0
    };
    if let Some((_, suffix)) = condensed {
        let layout = crate::typography::layout(
            suffix,
            &crate::typography::TextFormat {
                style: crate::typography::TextStyle {
                    formula: true,
                    ..style.clone()
                },
                ..Default::default()
            },
        );
        let mut parts: Vec<_> = layout
            .fragments
            .into_iter()
            .map(|run| Primitive::Text {
                position: run.position,
                text: run.text,
                size: run.style.size(),
                color: run.style.color,
                style: run.style,
            })
            .collect();
        let side = crate::atom_labels::appendage_position(a, doc);
        place_appendage(
            &runs,
            &mut parts,
            (origin, element_width),
            (layout.width, isotope_width),
            size,
            side,
        );
        runs.extend(parts);
        if side == crate::atom_labels::HydrogenPosition::Right {
            right += layout.width;
        }
    }
    if internal_group.is_some() {
        return runs;
    }
    let label_h = if a.no_implicit {
        a.explicit_h
    } else {
        a.label_h.max(a.explicit_h)
    };
    if show_element && a.element != "H" && label_h > 0 && crate::atom_labels::hydrogens(a, doc) {
        let count = if label_h > 1 {
            label_h.to_string()
        } else {
            String::new()
        };
        let h_width = text_width("H", size);
        let width = h_width + text_width(&count, small);
        use crate::atom_labels::HydrogenPosition as H;
        let position = crate::atom_labels::appendage_position(a, doc);
        let mut parts = vec![text(Point::default(), "H".into(), size)];
        if !count.is_empty() {
            parts.push(text(Point::new(h_width, size * 0.40), count, small));
        }
        place_appendage(
            &runs,
            &mut parts,
            (origin, element_width),
            (width, isotope_width),
            size,
            position,
        );
        if matches!(position, H::Above | H::Below) {
            if let Some(Primitive::Text { position, .. }) = parts.first() {
                mark_y = position.y;
            }
            right = origin.x + width;
        }
        runs.extend(parts);
        if position == H::Right {
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
        && !a.display.hide_charge
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
        runs.push(text(Point::new(right, mark_y - size * 0.25), label, small));
        right += width;
    }
    if a.radical_electrons > 0
        && !a.marks.iter().any(|m| {
            m.kind.radical() && (m.kind != crate::scientific::MarkKind::RadicalIon || a.charge != 0)
        })
    {
        runs.push(text(
            Point::new(right, mark_y - size * 0.25),
            "•".repeat(a.radical_electrons as usize),
            small,
        ));
    }
    runs
}

/// Stack using actual glyph ink, including subscripts, so H₂/Cl₂ clear the core.
/// A stacked appendage shares the core's left edge; its subscript is not centered
/// over the attachment point. Bond clipping consumes these same positioned runs.
fn place_appendage(
    core: &[Primitive],
    parts: &mut [Primitive],
    (origin, core_width): (Point, f32),
    (width, isotope_width): (f32, f32),
    size: f32,
    side: crate::atom_labels::HydrogenPosition,
) {
    use crate::atom_labels::HydrogenPosition as H;
    let ink_y = |runs: &[Primitive]| {
        label_ink_boxes(runs).into_iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(top, bottom), (lo, hi)| (top.min(lo.y), bottom.max(hi.y)),
        )
    };
    let x = match side {
        H::Left => origin.x - isotope_width - width,
        H::Above | H::Below => origin.x,
        _ => origin.x + core_width,
    };
    let y = match side {
        H::Above => ink_y(core).0 - ink_y(parts).1 - size * 0.12,
        H::Below => ink_y(core).1 - ink_y(parts).0 + size * 0.12,
        _ => origin.y,
    };
    let y = if y.is_finite() { y } else { origin.y };
    for part in parts {
        if let Primitive::Text { position, .. } = part {
            *position = position.offset(x, y);
        }
    }
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

fn label_ink_boxes(runs: &[Primitive]) -> Vec<(Point, Point)> {
    runs.iter()
        .flat_map(|run| {
            if let Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } = run
            {
                crate::style::text_ink_boxes(text, *size, style)
                    .into_iter()
                    .map(|(lo, hi)| (position.offset(lo.x, lo.y), position.offset(hi.x, hi.y)))
                    .collect()
            } else {
                Vec::new()
            }
        })
        .collect()
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
        for part in crate::scientific::styled_mark_parts(atom, &doc.drawing_style) {
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

fn label_end(
    origin: Point,
    ux: f32,
    uy: f32,
    bounds: &[(Point, Point)],
    max: f32,
    margin: f32,
) -> Point {
    bounds
        .iter()
        .map(|&(lo, hi)| label_box_end(origin, ux, uy, lo, hi, max, margin))
        .max_by(|a, b| {
            let along = |p: &Point| (p.x - origin.x) * ux + (p.y - origin.y) * uy;
            along(a).total_cmp(&along(b))
        })
        .unwrap_or(origin)
}

fn label_box_end(
    origin: Point,
    ux: f32,
    uy: f32,
    lo: Point,
    hi: Point,
    max: f32,
    margin: f32,
) -> Point {
    // Intersect the whole segment with the padded label box. A label can
    // extend past the bond midpoint, or sit above its attachment position.
    let mut enter: f32 = 0.;
    let mut exit = max.max(0.);
    for (position, direction, low, high) in [
        (origin.x, ux, lo.x - margin, hi.x + margin),
        (origin.y, uy, lo.y - margin, hi.y + margin),
    ] {
        if direction.abs() < 1e-6 {
            if position < low || position > high {
                return origin;
            }
        } else {
            let first = (low - position) / direction;
            let last = (high - position) / direction;
            enter = enter.max(first.min(last));
            exit = exit.min(first.max(last));
            if enter > exit {
                return origin;
            }
        }
    }
    origin.offset(ux * exit, uy * exit)
}

/// Resolve automatic positioning identically for drawing and repeated-click edits.
pub fn effective_double_position(doc: &Document, bond: &Bond) -> crate::bonds::DoublePosition {
    use crate::bonds::DoublePosition as P;
    match bond.double_position {
        P::Auto => match automatic_double_side(doc, bond) {
            Some(side) if side < 0. => P::Left,
            Some(_) => P::Right,
            // Keep the bold stroke on the atom-to-atom skeleton. Centering
            // unequal strokes offsets the backbone from its attached bonds.
            None if bond.display == "bold" => P::Right,
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
    let resolved = crate::canvas_theme::resolved_document(doc);
    let doc = resolved.as_ref();
    let style = &doc.drawing_style;
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
    out.extend(doc.ring_fills.iter().filter_map(|fill| fill.primitive(doc)));
    let arcs = crate::ring_arcs::render(doc);
    // A partial curve replaces the ring's circle, not its aromatic membership.
    // Retain every ring here so its other edges do not gain fallback dashes.
    let circles = crate::aromatic::circles(doc);
    let mut crossing_gaps = crate::crossings::gaps(doc);
    for circle in circles.iter().filter(|c| !arcs.intersects(c)) {
        for part in circle.graphic().parts() {
            let stroke = Primitive::Path {
                commands: part.commands,
                style: part.style,
                filled: part.filled,
            };
            let (stroke, gaps) = crate::crossings::ring_stroke(doc, circle, stroke);
            out.push(stroke);
            for (index, gap) in gaps {
                if let Some(gaps) = crossing_gaps.get_mut(index) {
                    gaps.push(gap);
                }
            }
        }
    }
    out.extend(arcs.primitives.iter().cloned());
    for (index, gap) in &arcs.crossings {
        if let Some(gaps) = crossing_gaps.get_mut(*index) {
            gaps.push(*gap);
        }
    }
    let labels: std::collections::HashMap<_, _> = doc
        .atoms
        .iter()
        .map(|a| (a.id, atom_label(a, doc)))
        .collect();
    let label_bounds: std::collections::HashMap<_, _> = labels
        .iter()
        .map(|(id, runs)| {
            // A charge beside an implicit carbon must not shorten its bonds.
            let bounds = doc
                .atom(*id)
                .filter(|a| visible(a, doc) || doc.abbreviation(*id).is_some())
                .map(|_| label_ink_boxes(runs))
                .unwrap_or_default();
            (*id, bounds)
        })
        .collect();
    // Fill joined bond outlines together. Separate antialiased polygons leave
    // translucent seams even when their mathematical corners agree exactly.
    let mut joined: std::collections::BTreeMap<[u8; 3], Vec<crate::graphics::PathCommand>> =
        Default::default();
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
            a.position,
            ux,
            uy,
            label_bounds
                .get(&a.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            length,
            style.world(style.margin_width_pt),
        );
        let end = label_end(
            z.position,
            -ux,
            -uy,
            label_bounds
                .get(&z.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            length,
            style.world(style.margin_width_pt),
        );
        if (end.x - start.x) * ux + (end.y - start.y) * uy <= 0.1 {
            continue;
        }
        let nx = -uy;
        let ny = ux;
        let bond_start = out.len();
        match b.display.as_str() {
            "plain" | "bold" | "wedge"
                if !matches!(b.order, 2 | 7) && crate::bond_joins::needed(doc, b) =>
            {
                out.push(Primitive::Polygon(crate::bond_joins::polygon(
                    doc, b, start, end,
                )));
            }
            "hollow_wedge" => {
                use crate::graphics::PathCommand;
                let points = crate::bond_joins::polygon(doc, b, start, end);
                if let Some(first) = points.first() {
                    let mut commands = vec![PathCommand::Move(*first)];
                    commands.extend(points.iter().skip(1).copied().map(PathCommand::Line));
                    commands.push(PathCommand::Close);
                    out.push(Primitive::Path {
                        commands,
                        style: crate::graphics::GraphicStyle {
                            width_pt: style.line_width_pt,
                            ..Default::default()
                        },
                        filled: false,
                    });
                }
            }
            "hash" | "hashed" => {
                let spacing = style.world(style.hash_spacing_pt);
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
                            nx * (style.line_width()
                                + t * (style.world(style.bold_width_pt) - style.line_width()))
                                / 2.0,
                            ny * (style.line_width()
                                + t * (style.world(style.bold_width_pt) - style.line_width()))
                                / 2.0,
                        ),
                        p.offset(
                            -nx * (style.line_width()
                                + t * (style.world(style.bold_width_pt) - style.line_width()))
                                / 2.0,
                            -ny * (style.line_width()
                                + t * (style.world(style.bold_width_pt) - style.line_width()))
                                / 2.0,
                        ),
                        style.line_width(),
                    ));
                }
            }
            "wavy" if b.order == 2 => {
                let half = style.bond_length_world * style.bond_spacing_ratio / 2.0;
                for sign in [-1.0, 1.0] {
                    out.push(Primitive::Line(
                        start.offset(nx * half * sign, ny * half * sign),
                        end.offset(-nx * half * sign, -ny * half * sign),
                        style.line_width(),
                    ));
                }
            }
            "wavy" => {
                out.push(Primitive::Path {
                    commands: crate::bonds::wavy_path(
                        start,
                        end,
                        style.bond_length_world / 4.,
                        style.line_width() * 1.25,
                    ),
                    style: crate::graphics::GraphicStyle {
                        width_pt: style.line_width_pt,
                        ..Default::default()
                    },
                    filled: false,
                });
            }
            _ => {
                let spacing = style.bond_length_world * style.bond_spacing_ratio;
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
                let order = if arcs.contains(b.a, b.b) { 1 } else { b.order };
                let offsets: &[f32] = match (order, side) {
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
                    let available = (last.x - first.x) * ux + (last.y - first.y) * uy;
                    if available <= 0.1 {
                        continue;
                    }
                    let first = label_end(
                        first,
                        ux,
                        uy,
                        label_bounds
                            .get(&a.id)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                        available,
                        style.world(style.margin_width_pt),
                    );
                    let last = label_end(
                        last,
                        -ux,
                        -uy,
                        label_bounds
                            .get(&z.id)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                        available,
                        style.world(style.margin_width_pt),
                    );
                    if (last.x - first.x) * ux + (last.y - first.y) * uy <= 0.1 {
                        continue;
                    }
                    if index == 0 && *offset == 0. && crate::bond_joins::needed(doc, b) {
                        out.push(Primitive::Polygon(crate::bond_joins::polygon(
                            doc, b, first, last,
                        )));
                    } else if display == "bold" {
                        // An explicitly centered bold rail still needs flat
                        // ends; round caps protrude beyond the junction.
                        let half = style.world(style.bold_width_pt) / 2.;
                        out.push(Primitive::Polygon(vec![
                            first.offset(nx * half, ny * half),
                            last.offset(nx * half, ny * half),
                            last.offset(-nx * half, -ny * half),
                            first.offset(-nx * half, -ny * half),
                        ]));
                    } else if matches!(display, "dashed" | "dotted") {
                        out.push(Primitive::Path {
                            commands: vec![
                                crate::graphics::PathCommand::Move(first),
                                crate::graphics::PathCommand::Line(last),
                            ],
                            style: crate::graphics::GraphicStyle {
                                width_pt: style.line_width_pt,
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
                                style.world(style.bold_width_pt)
                            } else {
                                style.line_width()
                            },
                        ));
                    }
                }
                if b.order == 4
                    && !arcs.contains(b.a, b.b)
                    && !circles.iter().any(|c| c.contains_bond(b.a, b.b))
                {
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
                            style.line_width(),
                        ));
                    }
                }
            }
        }
        if b.order == 5 && b.display == "plain" {
            head(&mut out, end, uy.atan2(ux), false, style.line_width());
        }
        if let Some(gaps) = crossing_gaps.get(bond_index).filter(|g| !g.is_empty()) {
            let bond_primitives = out.drain(bond_start..).collect();
            out.extend(crate::crossings::cut(bond_primitives, gaps));
        }
        if crate::bond_joins::needed(doc, b) && b.display != "hollow_wedge" {
            use crate::graphics::PathCommand;
            let commands = joined.entry(b.color).or_default();
            let mut secondary = Vec::new();
            for primitive in out.drain(bond_start..) {
                if let Primitive::Polygon(points) = primitive {
                    if let Some(first) = points.first() {
                        commands.push(PathCommand::Move(*first));
                        commands.extend(points.iter().skip(1).copied().map(PathCommand::Line));
                        commands.push(PathCommand::Close);
                    }
                } else {
                    secondary.push(primitive);
                }
            }
            out.extend(secondary);
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
    for junction in crate::bond_joins::junctions(doc) {
        use crate::graphics::PathCommand;
        let mut commands = Vec::new();
        for (bond_index, points) in junction.parts {
            let parts = crate::crossings::cut(
                vec![Primitive::Polygon(points)],
                crossing_gaps
                    .get(bond_index)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            );
            for part in parts {
                if let Primitive::Polygon(points) = part
                    && let Some(first) = points.first()
                {
                    commands.push(PathCommand::Move(*first));
                    commands.extend(points.iter().skip(1).copied().map(PathCommand::Line));
                    commands.push(PathCommand::Close);
                }
            }
        }
        if junction.underlay {
            out.push(Primitive::Path {
                commands,
                style: crate::graphics::GraphicStyle {
                    stroke: junction.color,
                    fill: Some(junction.color),
                    width_pt: 0.,
                    ..Default::default()
                },
                filled: true,
            });
        } else {
            joined.entry(junction.color).or_default().extend(commands);
        }
    }
    out.extend(joined.into_iter().map(|(color, commands)| Primitive::Path {
        commands,
        style: crate::graphics::GraphicStyle {
            stroke: color,
            fill: Some(color),
            width_pt: 0.,
            ..Default::default()
        },
        filled: true,
    }));
    for a in doc.atoms.iter().filter(|a| doc.atom_visible(a.id)) {
        out.extend(labels.get(&a.id).into_iter().flatten().cloned());
        out.extend(
            crate::scientific::styled_mark_parts(a, &doc.drawing_style)
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
fn head(out: &mut Vec<Primitive>, end: Point, angle: f32, half: bool, width: f32) {
    let a = end.offset(
        -angle.cos() * 10. - angle.sin() * 3.5,
        -angle.sin() * 10. + angle.cos() * 3.5,
    );
    if half {
        out.push(Primitive::Line(end, a, width));
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
/// The themed drawing with a transparent surround, for compositing and geometry checks.
pub fn svg(doc: &Document) -> String {
    render_svg(doc, false)
}
/// A figure carries the canvas background when exported or copied.
pub fn svg_with_background(doc: &Document) -> String {
    render_svg(doc, true)
}
fn render_svg(doc: &Document, background: bool) -> String {
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
    let theme = doc.canvas_theme;
    if background {
        let [r, g, b] = theme.background();
        s.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"rgb({r},{g},{b})\"/>\n",
            lo.x,
            lo.y,
            hi.x - lo.x,
            hi.y - lo.y
        ));
    }
    let [r, g, b] = theme.color([0; 3]);
    let ink = format!("rgb({r},{g},{b})");
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
                mut style,
                filled,
            } => {
                style.stroke = theme.color(style.stroke);
                style.fill = style.fill.map(|color| theme.color(color));
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
                s.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{ink}\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>",a.x,a.y,b.x,b.y));
            }
            Primitive::Polygon(points) => {
                s.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"{ink}\"/>",
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
                let color = theme.color(color);
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
    fn stacked_hydrogens_keep_charge_isotope_and_subscript_ink_separate() {
        for size in [8., 10., 18.] {
            for degrees in (0..360).step_by(15) {
                let mut doc = Document::default();
                let n = doc.add_atom("N", Point::default());
                for x in [-60., 60.] {
                    let c = doc.add_atom("C", Point::new(x, 35.));
                    doc.add_bond(n, c, 1, "plain");
                }
                let atom = doc.atom_mut(n).unwrap();
                atom.explicit_h = 2;
                atom.no_implicit = true;
                atom.charge = 1;
                atom.isotope = 15;
                atom.text_style = Some(crate::typography::TextStyle {
                    size_pt: size,
                    ..Default::default()
                });
                let ids = doc.all_ids();
                crate::editing::transform_about(
                    &mut doc,
                    &ids,
                    Point::default(),
                    1.,
                    degrees as f32,
                );
                let boxes = label_ink_boxes(&atom_label(doc.atom(n).unwrap(), &doc));
                for (i, (a, b)) in boxes.iter().enumerate() {
                    for (c, d) in &boxes[i + 1..] {
                        assert!(
                            b.x <= c.x || a.x >= d.x || b.y <= c.y || a.y >= d.y,
                            "Overlapping label glyphs at {degrees} degrees, {size} pt"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn internal_label_bonds_clear_every_glyph_through_rotation() -> Result<(), String> {
        for label in ["NH", "CH2", "CCl2", "CF2", "NMe", "SiH2", "C(OH)2"] {
            for degrees in (0..360).step_by(15) {
                let mut doc = Document::default();
                let center = doc.add_atom("C", Point::default());
                for x in [-60., 60.] {
                    let end = doc.add_atom("C", Point::new(x, 35.));
                    doc.add_bond(center, end, 1, "plain");
                }
                doc = crate::atom_text::apply(&doc, center, label, crate::atom_text::Mode::Auto)?;
                let ids = doc.all_ids();
                crate::editing::transform_about(
                    &mut doc,
                    &ids,
                    Point::default(),
                    1.,
                    degrees as f32,
                );
                let boxes = label_ink_boxes(&atom_label(doc.atom(center).ok_or("Atom")?, &doc));
                let mut edges = Vec::new();
                for primitive in primitives(&doc) {
                    match primitive {
                        Primitive::Line(a, b, width) => edges.push((a, b, width)),
                        Primitive::Polygon(points) => {
                            for i in 0..points.len() {
                                edges.push((points[i], points[(i + 1) % points.len()], 0.));
                            }
                        }
                        Primitive::Path {
                            commands, style, ..
                        } => {
                            use crate::graphics::PathCommand as P;
                            let mut first = Point::default();
                            let mut last = first;
                            for command in commands {
                                match command {
                                    P::Move(p) => {
                                        first = p;
                                        last = p;
                                    }
                                    P::Line(p) => {
                                        edges.push((last, p, style.width()));
                                        last = p;
                                    }
                                    P::Close => {
                                        edges.push((last, first, style.width()));
                                        last = first;
                                    }
                                    P::Cubic(..) => panic!("Unexpected curved bond"),
                                }
                            }
                        }
                        _ => {}
                    }
                }
                assert!(
                    !edges.is_empty(),
                    "The test must inspect actual rendered bonds"
                );
                for (from, to, width) in edges {
                    for step in 0..=100 {
                        let f = step as f32 / 100.;
                        let point =
                            Point::new(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
                        for (lo, hi) in &boxes {
                            assert!(
                                point.x + width / 2. <= lo.x
                                    || point.x - width / 2. >= hi.x
                                    || point.y + width / 2. <= lo.y
                                    || point.y - width / 2. >= hi.y,
                                "{label} at {degrees} crosses label ink"
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn rotated_group_bonds_do_not_cross_label_ink() -> Result<(), String> {
        use crate::abbreviations::LabelAlignment;
        for label in ["C2H5", "OCH3", "Boc"] {
            for alignment in LabelAlignment::ALL {
                for order in [1, 2, 3] {
                    for degrees in (0..360).step_by(15) {
                        let mut doc = Document::default();
                        let n = doc.add_atom("N", Point::new(-70., 0.));
                        let c = doc.add_atom("C", Point::default());
                        doc.add_bond(n, c, 1, "plain");
                        doc =
                            crate::atom_text::apply(&doc, c, label, crate::atom_text::Mode::Auto)?;
                        // Imported group drawings may carry multiple bonds.
                        doc.bonds
                            .iter_mut()
                            .find(|b| b.a == n && b.b == c)
                            .ok_or("Bond")?
                            .order = order;
                        doc.abbreviations.first_mut().ok_or("Group")?.alignment = alignment;
                        let ids = doc.all_ids();
                        crate::editing::transform_about(
                            &mut doc,
                            &ids,
                            Point::default(),
                            1.,
                            degrees as f32,
                        );
                        let boxes: Vec<_> = [n, c]
                            .into_iter()
                            .filter_map(|id| doc.atom(id))
                            .flat_map(|a| label_ink_boxes(&atom_label(a, &doc)))
                            .collect();
                        for primitive in primitives(&doc) {
                            if let Primitive::Line(from, to, width) = primitive {
                                // Sample the complete stroked rail, not only its midpoint.
                                for step in 0..=100 {
                                    let t = step as f32 / 100.;
                                    let p = from.offset((to.x - from.x) * t, (to.y - from.y) * t);
                                    for (lo, hi) in &boxes {
                                        assert!(
                                            p.x < lo.x - width / 2.
                                                || p.x > hi.x + width / 2.
                                                || p.y < lo.y - width / 2.
                                                || p.y > hi.y + width / 2.,
                                            "Bond crosses {label} at {degrees}°, {alignment:?}, order {order}: {p:?}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn expanded_azide_and_magnesium_bromide_keep_visible_bonds() -> Result<(), String> {
        for key in ["M", "Z"] {
            for angle in (0..360).step_by(15) {
                let mut doc = Document::default();
                let c = doc.add_atom("C", Point::new(-42., 0.));
                let end = doc.add_atom("C", Point::default());
                doc.add_bond(c, end, 1, "plain");
                doc = crate::hotkeys::atom_edit(&doc, end, key, 42.)
                    .ok_or("key")??
                    .0;
                let members = doc.abbreviation(end).ok_or("group")?.members.clone();
                doc.expand_abbreviations(&members);
                let ids = doc.all_ids();
                crate::editing::transform_about(&mut doc, &ids, Point::default(), 1., angle as f32);
                let drawing = primitives(&doc);
                let strokes: usize = drawing
                    .iter()
                    .map(|p| match p {
                        Primitive::Line(_, _, _) | Primitive::Polygon(_) => 1,
                        Primitive::Path { commands, .. } => commands
                            .iter()
                            .filter(|c| matches!(c, crate::graphics::PathCommand::Move(_)))
                            .count(),
                        _ => 0,
                    })
                    .sum();
                let expected = if key == "M" { 2 } else { 5 };
                assert_eq!(strokes, expected, "{key}, {angle} degrees");
            }
        }
        Ok(())
    }

    #[test]
    fn group_labels_remain_visible_and_anchor_to_elements_at_every_angle() -> Result<(), String> {
        use crate::abbreviations::LabelAlignment;
        for label in ["C2H5", "C₂H₅", "OCH3", "Boc"] {
            let mut source = Document::default();
            let n = source.add_atom("N", Point::new(-70., 0.));
            let a = source.add_atom("C", Point::default());
            source.add_bond(n, a, 1, "plain");
            source = crate::atom_text::apply(&source, a, label, crate::atom_text::Mode::Auto)?;
            for alignment in LabelAlignment::ALL {
                for angle in (0..360).step_by(5) {
                    let mut doc = source.clone();
                    doc.abbreviations.first_mut().ok_or("Group")?.alignment = alignment;
                    let ids = doc.all_ids();
                    crate::editing::transform_about(
                        &mut doc,
                        &ids,
                        Point::default(),
                        1.,
                        angle as f32,
                    );
                    let atom = doc.atom(a).ok_or("Anchor")?;
                    let runs = atom_label(atom, &doc);
                    let (lo, hi) = text_bounds(&runs).ok_or("Label disappeared")?;
                    assert!(lo.x.is_finite() && lo.y.is_finite() && hi.x > lo.x && hi.y > lo.y);
                    assert!((hi.x - lo.x) < 120., "Label bounds exploded at {angle}°");
                    let mut text = String::new();
                    for run in &runs {
                        if let Primitive::Text { text: value, .. } = run {
                            text.push_str(value);
                        }
                    }
                    assert_eq!(text, doc.abbreviation(a).ok_or("Group")?.text(&doc));
                    if label == "C2H5"
                        && !matches!(alignment, LabelAlignment::Above | LabelAlignment::Center)
                    {
                        // Digits are scripts; the attachment is centered on C,
                        // even for the reversed H5C2 spelling.
                        let (position, size, style) = runs
                            .iter()
                            .find_map(|p| match p {
                                Primitive::Text {
                                    text,
                                    position,
                                    size,
                                    style,
                                    ..
                                } if text == "C" => Some((position, size, style)),
                                _ => None,
                            })
                            .ok_or("Carbon glyph")?;
                        let center =
                            position.x + crate::style::styled_text_width("C", *size, style) / 2.;
                        assert!(
                            (center - atom.position.x).abs() < 0.01,
                            "{alignment:?}, {angle}°"
                        );
                    }
                }
            }
        }
        // Imported groups can contain a whitespace-only reverse spelling.
        let mut doc = Document::default();
        let n = doc.add_atom("N", Point::new(70., 0.));
        let a = doc.add_atom("C", Point::default());
        doc.add_bond(n, a, 1, "plain");
        doc = crate::atom_text::apply(&doc, a, "Boc", crate::atom_text::Mode::Auto)?;
        doc.abbreviations.first_mut().ok_or("Group")?.reverse_label = "  ".into();
        assert!(atom_label_bounds(doc.atom(a).ok_or("Anchor")?, &doc).is_some());
        assert_eq!(doc.abbreviation(a).ok_or("Group")?.text(&doc), "Boc");
        Ok(())
    }
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
        // The six outer edges now share filled joins; the three inset rails
        // remain independent strokes. Raster coverage is checked in bond_joins.
        assert_eq!(lines.len(), 3);
        assert!(
            drawing
                .iter()
                .any(|p| matches!(p, Primitive::Path { filled: true, .. }))
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
