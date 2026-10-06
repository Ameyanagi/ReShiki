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

pub fn atom_label_bounds(a: &Atom, doc: &Document) -> Option<(Point, Point)> {
    text_bounds(&atom_label(a, doc))
}

/// Ink extents for persistent label backgrounds, excluding font line padding.
pub(crate) fn atom_label_ink_bounds(a: &Atom, doc: &Document) -> Option<(Point, Point)> {
    label_ink_boxes(&atom_label(a, doc))
        .into_iter()
        .reduce(|(lo, hi), (a, b)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        })
}

/// The atom owning a visible label, including its hydrogens, isotope and charge.
/// Keep this separate from geometric nearest-atom searches used for bond growth.
pub fn atom_label_hit(doc: &Document, point: Point, radius: f32) -> Option<u64> {
    doc.atoms.iter().rev().find_map(|atom| {
        // Empty corners between a symbol and its stacked H/isotope/charge
        // must not hide a neighboring atom or bond from pointer targeting.
        label_ink_boxes(&atom_label(atom, doc))
            .into_iter()
            .any(|(lo, hi)| {
                point.x >= lo.x - radius
                    && point.x <= hi.x + radius
                    && point.y >= lo.y - radius
                    && point.y <= hi.y + radius
            })
            .then_some(atom.id)
    })
}

pub(crate) fn atom_label_ink_boxes(a: &Atom, doc: &Document) -> Vec<(Point, Point)> {
    label_ink_boxes(&atom_label(a, doc))
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
            ..group.text_style(doc)
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
                color: run.style.color.rgb(),
                style: run.style,
            })
            .collect();
    }
    let show_element = internal_group.is_some() || visible(a, doc);
    if !show_element && a.charge == 0 {
        return vec![];
    }
    let style = internal_group.map_or_else(
        || {
            a.text_style
                .clone()
                .unwrap_or_else(|| doc.drawing_style.text_style())
        },
        |group| group.text_style(doc),
    );
    let text_width = |text: &str, size| crate::style::styled_text_width(text, size, &style);
    let text = |position, content, size| Primitive::Text {
        position,
        text: content,
        size,
        color: style.color.rgb(),
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
                color: run.style.color.rgb(),
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
        if let Some(hydrogen_color) = a.display.hydrogen_color {
            for part in &mut parts {
                if let Primitive::Text { color, style, .. } = part {
                    *color = hydrogen_color.rgb();
                    style.color = hydrogen_color;
                }
            }
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
    selections_bounds(doc, &[ids.to_vec()]).pop().flatten()
}

/// `selection_bounds` of several disjoint selections, such as the Align menu's
/// units, in one pass over the drawing.
pub fn selections_bounds(doc: &Document, selections: &[Vec<u64>]) -> Vec<Option<(Point, Point)>> {
    use crate::atom_labels::Owner;
    let owners: std::collections::HashMap<u64, usize> = selections
        .iter()
        .enumerate()
        .flat_map(|(i, ids)| ids.iter().map(move |id| (*id, i)))
        .collect();
    let mut bounds: Vec<Option<(Point, Point)>> = vec![None; selections.len()];
    let mut grow = |id: u64, points: &mut dyn Iterator<Item = Point>| {
        let Some(extent) = owners.get(&id).and_then(|i| bounds.get_mut(*i)) else {
            return;
        };
        for p in points {
            *extent = Some(match *extent {
                None => (p, p),
                Some((lo, hi)) => (
                    Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                ),
            });
        }
    };
    for label in crate::atom_labels::indicators(doc) {
        let id = match label.owner {
            Owner::Number(id) | Owner::AtomStereo(id) => id,
            Owner::BondStereo(a, b) if owners.get(&a) == owners.get(&b) => a,
            Owner::BondStereo(..) => continue,
        };
        let corner = label.origin.offset(label.width, label.height);
        grow(id, &mut [label.origin, corner].into_iter());
    }
    // Only selected objects are measured, so one small selection stays cheap.
    for g in doc.graphics.iter().filter(|g| owners.contains_key(&g.id)) {
        let (lo, hi) = g.bounds();
        grow(g.id, &mut [lo, hi].into_iter());
    }
    for atom in doc
        .atoms
        .iter()
        .filter(|a| owners.contains_key(&a.id) && doc.atom_visible(a.id))
    {
        grow(atom.id, &mut std::iter::once(atom.position));
        for part in crate::scientific::styled_mark_parts(atom, &doc.drawing_style) {
            grow(
                atom.id,
                &mut part
                    .commands
                    .iter()
                    .flat_map(crate::graphics::PathCommand::iter_points),
            );
        }
        if let Some((lo, hi)) = text_bounds(&atom_label(atom, doc)) {
            grow(atom.id, &mut [lo, hi].into_iter());
        }
        if let Some((lo, hi)) = crate::highlights::atom_bounds(doc, atom) {
            grow(atom.id, &mut [lo, hi].into_iter());
        }
    }
    let mut highlight_joins = None;
    for bond in &doc.bonds {
        if bond.highlight.is_some()
            && doc.bond_visible(bond.a, bond.b)
            && owners
                .get(&bond.a)
                .is_some_and(|owner| Some(owner) == owners.get(&bond.b))
            && let Some((lo, hi)) = crate::highlights::bond_bounds(
                doc,
                bond,
                highlight_joins.get_or_insert_with(|| crate::bond_joins::Joins::new(doc)),
            )
        {
            grow(bond.a, &mut [lo, hi].into_iter());
        }
    }
    for a in doc
        .annotations
        .iter()
        .filter(|a| owners.contains_key(&a.id))
    {
        let (width, height) = a.size();
        grow(
            a.id,
            &mut [a.position, a.position.offset(width, height)].into_iter(),
        );
    }
    for a in doc.arrows.iter().filter(|a| owners.contains_key(&a.id)) {
        let (lo, hi) = a.bounds();
        grow(a.id, &mut [lo, hi].into_iter());
    }
    bounds
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

/// Drawing primitives with colors in canonical light-canvas bytes; renderers
/// apply the canvas conversion once.
pub fn primitives(doc: &Document) -> Vec<Primitive> {
    let paint = crate::depth_appearance::Paint::new(doc);
    let painted = paint.materialize(doc);
    let resolved = crate::canvas_theme::canonical_document(&painted);
    let doc = resolved.as_ref();
    let style = &doc.drawing_style;
    let mut out = vec![];
    let mut graphics: Vec<_> = doc.graphics.iter().collect();
    graphics.sort_by_key(|g| g.layer);
    push_underlays(&mut out, doc, &graphics);
    let RingStrokes {
        arcs,
        circles,
        crossing_gaps,
    } = push_ring_strokes(&mut out, doc);
    let AtomLabels {
        runs: labels,
        bounds: label_bounds,
    } = collect_atom_labels(doc);
    // Fill joined bond outlines together. Separate antialiased polygons leave
    // translucent seams even when their mathematical corners agree exactly.
    let joins = crate::bond_joins::Joins::new(doc);
    let mut joined: std::collections::BTreeMap<
        crate::palette::Color,
        Vec<crate::graphics::PathCommand>,
    > = Default::default();
    let scene = BondScene {
        doc,
        style,
        arcs: &arcs,
        circles: &circles,
        label_bounds: &label_bounds,
        joins: &joins,
        crossing_gaps: &crossing_gaps,
    };
    for (bond_index, b) in doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| doc.bond_visible(b.a, b.b))
    {
        push_bond(scene, &mut out, &mut joined, bond_index, b);
    }
    push_junctions(&mut out, &mut joined, &joins, &crossing_gaps);
    push_joined_outlines(&mut out, joined);
    push_atom_labels(&mut out, doc, &labels);
    push_arrows_and_annotations(&mut out, doc);
    out.extend(
        graphics
            .iter()
            .filter(|g| g.layer >= 0)
            .flat_map(graphic_primitive),
    );
    out
}
fn graphic_primitive(g: &&crate::graphics::Graphic) -> Vec<Primitive> {
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
}
fn push_underlays(
    out: &mut Vec<Primitive>,
    doc: &Document,
    graphics: &[&crate::graphics::Graphic],
) {
    out.extend(
        graphics
            .iter()
            .filter(|g| g.layer < 0)
            .flat_map(graphic_primitive),
    );
    out.extend(doc.ring_fills.iter().filter_map(|fill| fill.primitive(doc)));
    out.extend(crate::highlights::primitives(doc));
}
/// Aromatic circles and ring arcs, with the crossing gaps they add.
struct RingStrokes {
    arcs: crate::ring_arcs::Arcs,
    circles: Vec<crate::aromatic::Circle>,
    crossing_gaps: Vec<Vec<crate::crossings::Gap>>,
}
fn push_ring_strokes(out: &mut Vec<Primitive>, doc: &Document) -> RingStrokes {
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
    RingStrokes {
        arcs,
        circles,
        crossing_gaps,
    }
}
/// Label runs per atom and the ink boxes that trim its bonds.
struct AtomLabels {
    runs: std::collections::HashMap<u64, Vec<Primitive>>,
    bounds: std::collections::HashMap<u64, Vec<(Point, Point)>>,
}
fn collect_atom_labels(doc: &Document) -> AtomLabels {
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
    AtomLabels {
        runs: labels,
        bounds: label_bounds,
    }
}
fn push_junctions(
    out: &mut Vec<Primitive>,
    joined: &mut std::collections::BTreeMap<
        crate::palette::Color,
        Vec<crate::graphics::PathCommand>,
    >,
    joins: &crate::bond_joins::Joins<'_>,
    crossing_gaps: &[Vec<crate::crossings::Gap>],
) {
    for junction in joins.junctions() {
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
}
fn push_joined_outlines(
    out: &mut Vec<Primitive>,
    joined: std::collections::BTreeMap<crate::palette::Color, Vec<crate::graphics::PathCommand>>,
) {
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
}
fn push_atom_labels(
    out: &mut Vec<Primitive>,
    doc: &Document,
    labels: &std::collections::HashMap<u64, Vec<Primitive>>,
) {
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
}
fn push_arrows_and_annotations(out: &mut Vec<Primitive>, doc: &Document) {
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
                color: fragment.style.color.rgb(),
                style: fragment.style,
            });
        }
    }
}
/// Read-only state shared by every bond of the bond pass.
#[derive(Clone, Copy)]
struct BondScene<'a, 'd> {
    doc: &'a Document,
    style: &'a crate::style::DrawingStyle,
    arcs: &'a crate::ring_arcs::Arcs,
    circles: &'a [crate::aromatic::Circle],
    label_bounds: &'a std::collections::HashMap<u64, Vec<(Point, Point)>>,
    joins: &'a crate::bond_joins::Joins<'d>,
    crossing_gaps: &'a [Vec<crate::crossings::Gap>],
}
/// A drawable bond's end atoms, label-trimmed ends, direction and normal.
#[derive(Clone, Copy)]
struct BondFrame<'a> {
    a: &'a Atom,
    z: &'a Atom,
    start: Point,
    end: Point,
    ux: f32,
    uy: f32,
    nx: f32,
    ny: f32,
}
fn bond_frame<'a>(scene: BondScene<'a, '_>, b: &Bond) -> Option<BondFrame<'a>> {
    let BondScene {
        doc,
        style,
        label_bounds,
        ..
    } = scene;
    let (Some(a), Some(z)) = (doc.atom(b.a), doc.atom(b.b)) else {
        return None;
    };
    let length = a.position.distance(z.position);
    if length < 0.1 {
        return None;
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
        return None;
    }
    let nx = -uy;
    let ny = ux;
    Some(BondFrame {
        a,
        z,
        start,
        end,
        ux,
        uy,
        nx,
        ny,
    })
}
fn push_bond(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    joined: &mut std::collections::BTreeMap<
        crate::palette::Color,
        Vec<crate::graphics::PathCommand>,
    >,
    bond_index: usize,
    b: &Bond,
) {
    let BondScene { style, joins, .. } = scene;
    let Some(frame) = bond_frame(scene, b) else {
        return;
    };
    let BondFrame {
        start,
        end,
        ux,
        uy,
        nx,
        ny,
        ..
    } = frame;
    let bond_start = out.len();
    match b.display.as_str() {
        "plain" | "bold" | "wedge" if !matches!(b.order, 2 | 7) && joins.needed(b) => {
            out.push(Primitive::Polygon(joins.polygon(b, start, end)));
        }
        "hollow_wedge" => push_hollow_wedge(scene, out, b, start, end),
        "hash" | "hashed" => push_hashed_wedge(scene, out, b, frame),
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
        _ => push_bond_rails(scene, out, b, frame),
    }
    if b.order == 5 && b.display == "plain" {
        head(out, end, uy.atan2(ux), false, style.line_width());
    }
    cut_bond_crossings(scene, out, bond_index, bond_start);
    collect_joined_outlines(scene, out, joined, bond_start, b);
    recolor_bond(out, bond_start, b);
}
fn push_hollow_wedge(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    b: &Bond,
    start: Point,
    end: Point,
) {
    let BondScene { style, joins, .. } = scene;
    use crate::graphics::PathCommand;
    let points = joins.polygon(b, start, end);
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
fn push_hashed_wedge(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    b: &Bond,
    frame: BondFrame<'_>,
) {
    let BondScene { style, .. } = scene;
    let BondFrame {
        start, end, nx, ny, ..
    } = frame;
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
fn push_bond_rails(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    b: &Bond,
    frame: BondFrame<'_>,
) {
    let BondScene {
        doc,
        style,
        arcs,
        circles,
        label_bounds,
        joins,
        ..
    } = scene;
    let BondFrame {
        a,
        z,
        start,
        end,
        ux,
        uy,
        nx,
        ny,
    } = frame;
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
        if index == 0 && *offset == 0. && joins.needed(b) {
            out.push(Primitive::Polygon(joins.polygon(b, first, last)));
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
fn cut_bond_crossings(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    bond_index: usize,
    bond_start: usize,
) {
    let BondScene { crossing_gaps, .. } = scene;
    if let Some(gaps) = crossing_gaps.get(bond_index).filter(|g| !g.is_empty()) {
        let bond_primitives = out.drain(bond_start..).collect();
        out.extend(crate::crossings::cut(bond_primitives, gaps));
    }
}
fn collect_joined_outlines(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    joined: &mut std::collections::BTreeMap<
        crate::palette::Color,
        Vec<crate::graphics::PathCommand>,
    >,
    bond_start: usize,
    b: &Bond,
) {
    let BondScene { joins, .. } = scene;
    if joins.needed(b) && b.display != "hollow_wedge" {
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
}
fn recolor_bond(out: &mut [Primitive], bond_start: usize, b: &Bond) {
    if b.color.rgb() != [0, 0, 0] {
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
pub fn bounds(drawing: &[Primitive]) -> (Point, Point) {
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
                    .flat_map(crate::graphics::PathCommand::iter_points)
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
                style,
                filled,
            } => {
                let stroke = theme.color(style.stroke.rgb());
                let fill = style.fill.map(|color| theme.color(color.rgb()));
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
                    fill.map(|c| format!("rgb({},{},{})", c[0], c[1], c[2]))
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
                s.push_str(&format!("<path d=\"{path}\" fill=\"{fill}\" stroke=\"rgb({},{},{})\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{dash}/>",stroke[0],stroke[1],stroke[2],style.width()));
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
mod primitives_parity_tests;
#[cfg(test)]
mod tests;
