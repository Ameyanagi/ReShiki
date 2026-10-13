use crate::document::{Atom, Bond, Document, Point};
use crate::style::DEFAULT as STYLE;
use bond_pass::{BondInputs, BondPass, Resolve};
use charge_placement::{ChargeLayout, place_automatic_charge};
use depth_field::{DepthField, bond_depth_field_with_faces};
pub(crate) use occlusion::{OcclusionSource, occlusion_ink};
use projected_bonds::FaceIndex;
pub(crate) mod attachment_ink;
mod bond_pass;
mod charge_placement;
pub(crate) mod depth_field;
mod occlusion;
pub(crate) mod projected_bonds;
mod svg;

#[derive(Debug, Clone)]
pub enum Primitive {
    /// Drawing-only alpha. Underlying ink stays exact for other backgrounds.
    Opacity {
        alpha: f32,
        primitive: Box<Primitive>,
    },
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

/// Whether this atom's label ink trims its bonds: shown under rear opacity,
/// with a visible element label or abbreviation. A charge beside an implicit
/// carbon does not trim bonds.
pub(crate) fn label_trims_bonds(
    atom: &Atom,
    doc: &Document,
    opacity: Option<&crate::rear_opacity::Paint>,
) -> bool {
    opacity.is_none_or(|paint| paint.atom(atom.id) > 0.)
        && (visible(atom, doc) || doc.abbreviation(atom.id).is_some())
}

pub fn atom_label_bounds(a: &Atom, doc: &Document) -> Option<(Point, Point)> {
    text_bounds(&atom_label(a, doc))
}

/// Ink extents for persistent label backgrounds, excluding font line padding.
pub fn atom_label_ink_bounds(a: &Atom, doc: &Document) -> Option<(Point, Point)> {
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
    atom_label_runs_with_charge_layout(a, doc, ChargeLayout::Automatic)
}

fn atom_label_runs_with_charge_layout(
    a: &Atom,
    doc: &Document,
    charge_layout: ChargeLayout,
) -> Vec<Primitive> {
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
        return abbreviation_label_runs(a, doc, group);
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
    let is_link = doc.generic.links.iter().any(|s| s.atom == a.id);
    let label = internal_group
        .map(|group| group.label.as_str())
        .or_else(|| {
            a.display
                .variable
                .as_deref()
                .filter(|_| a.element == "*" || is_link)
        })
        .unwrap_or(&a.element);
    let scripts = (internal_group.is_none()
        && a.element == "*"
        && a.display.variable.is_some()
        && !doc.generic.owns_label(a.id))
    .then(|| crate::typography::effective_label_runs(&a.display))
    .filter(|runs| {
        !a.display.label_scripts.is_empty()
            || runs
                .iter()
                .any(|(_, script)| *script != crate::typography::Script::Normal)
    });
    let condensed = (internal_group.is_some() || a.element == "*" || is_link)
        .then(|| crate::atom_labels::condensed_label(label))
        .flatten();
    let core_end = condensed.map_or(label.len(), |(core, _)| core.len());
    let layout = scripts
        .as_ref()
        .filter(|_| condensed.is_none() || !a.display.label_scripts.is_empty())
        .map(|scripts| free_label_layout(label, scripts, 0..core_end, &style));
    let normal: String = scripts
        .iter()
        .flatten()
        .filter(|(_, script)| *script == crate::typography::Script::Normal)
        .filter_map(|(range, _)| label.get(range.start..range.end.min(core_end)))
        .collect();
    let source = label;
    let label = condensed.map_or(label, |(core, _)| core);
    let element_width = layout
        .as_ref()
        .map_or_else(|| text_width(label, size), |layout| layout.width);
    let origin = a.position.offset(
        -element_width / 2.0,
        -crate::style::label_vertical_center(
            if layout.is_some() { &normal } else { label },
            size,
            &style,
        ),
    );
    let runs = if show_element {
        if let Some(layout) = layout {
            layout
                .fragments
                .into_iter()
                .map(|run| Primitive::Text {
                    position: origin.offset(run.position.x, run.position.y),
                    text: run.text,
                    size: run.style.size(),
                    color: run.style.color.rgb(),
                    style: run.style,
                })
                .collect()
        } else {
            vec![text(origin, label.to_string(), size)]
        }
    } else {
        vec![]
    };
    let right = origin.x + element_width;
    let mark_y = origin.y;
    let isotope_width = if a.isotope > 0 {
        text_width(&a.isotope.to_string(), small)
    } else {
        0.0
    };
    let frame = LabelFrame {
        style: &style,
        origin,
        element_width,
        isotope_width,
        size,
        small,
    };
    let mut cursor = LabelCursor {
        runs,
        right,
        mark_y,
    };
    if let Some((core, suffix)) = condensed {
        let layout = scripts
            .as_ref()
            .map(|scripts| free_label_layout(source, scripts, core.len()..source.len(), &style));
        push_condensed_suffix(a, doc, frame, &mut cursor, suffix, layout);
    }
    if internal_group.is_some() {
        return cursor.runs;
    }
    if !is_link {
        push_hydrogens(a, doc, frame, &mut cursor, show_element);
    }
    push_isotope(a, frame, &mut cursor);
    if matches!(charge_layout, ChargeLayout::Omitted) {
        return cursor.runs;
    }
    let charge_start = cursor.runs.len();
    push_charge(a, frame, &mut cursor);
    let automatic_charge = cursor.runs.len() > charge_start;
    push_radicals(a, frame, &mut cursor);
    if matches!(charge_layout, ChargeLayout::Automatic) && show_element && automatic_charge {
        let (core, charge) = cursor.runs.split_at_mut(charge_start);
        place_automatic_charge(a, doc, frame, core, charge);
    }
    cursor.runs
}

/// Source ranges remain unchanged for exchange; only drawn charge glyphs are
/// normalized here. Slicing full-label runs preserves the count exception.
fn free_label_layout(
    label: &str,
    scripts: &[(std::ops::Range<usize>, crate::typography::Script)],
    range: std::ops::Range<usize>,
    style: &crate::typography::TextStyle,
) -> crate::typography::Layout {
    use crate::typography::{Script, TextFormat, TextSpan, TextStyle};
    let style = TextStyle {
        formula: false,
        script: Script::Normal,
        ..style.clone()
    };
    // Formula inference identifies the optional charge separator, even when
    // manual formatting raises a larger span. Other raised spaces are literal.
    let charge_space = crate::typography::label_runs(label)
        .last()
        .filter(|(_, script)| *script == Script::Superscript)
        .and_then(|(source, _)| {
            label
                .get(source.clone())
                .is_some_and(|text| text.starts_with(' '))
                .then_some(source.start)
        });
    let mut drawn = String::new();
    let mut spans = vec![];
    for (source, script) in scripts {
        let start = source.start.max(range.start);
        let end = source.end.min(range.end);
        let Some(content) = (start < end).then(|| label.get(start..end)).flatten() else {
            continue;
        };
        let drawn_start = drawn.len();
        for (offset, c) in content.char_indices() {
            if *script == Script::Superscript {
                match c {
                    ' ' if charge_space == Some(start + offset) => continue,
                    '-' | '⁻' => drawn.push('−'),
                    '⁺' => drawn.push('+'),
                    _ => drawn.push(c),
                }
            } else {
                drawn.push(c);
            }
        }
        if *script != Script::Normal && drawn_start < drawn.len() {
            spans.push(TextSpan {
                start: drawn_start,
                end: drawn.len(),
                style: TextStyle {
                    script: *script,
                    ..style.clone()
                },
            });
        }
    }
    let normal_size = STYLE.world(style.size_pt);
    let mut layout = crate::typography::layout(
        &drawn,
        &TextFormat {
            style,
            spans,
            ..Default::default()
        },
    );
    // layout reserves a superscript top margin; anchor the normal glyphs at
    // the same vertical position as the previous unscripted atom label.
    let normal_y = layout
        .fragments
        .iter()
        .find(|run| run.style.script == Script::Normal)
        .map_or_else(
            || {
                if layout
                    .fragments
                    .iter()
                    .any(|run| run.style.script == Script::Superscript)
                {
                    normal_size * 0.3
                } else {
                    0.
                }
            },
            |run| run.position.y,
        );
    for run in &mut layout.fragments {
        run.position.y -= normal_y;
    }
    layout
}

/// A contracted abbreviation: the whole label is laid out as formula text
/// and anchored on its attachment glyph for the group's alignment.
fn abbreviation_label_runs(
    a: &Atom,
    doc: &Document,
    group: &crate::abbreviations::Abbreviation,
) -> Vec<Primitive> {
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
        crate::typography::layout(content.get(..range.start).unwrap_or_default(), &format).width;
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
    layout
        .fragments
        .into_iter()
        .map(|run| Primitive::Text {
            position: origin.offset(run.position.x, run.position.y),
            text: run.text,
            size: run.style.size(),
            color: run.style.color.rgb(),
            style: run.style,
        })
        .collect()
}

/// Read-only placement of an element label, shared by its appendage
/// stages. It borrows the label style, so building it clones nothing.
#[derive(Clone, Copy)]
struct LabelFrame<'s> {
    style: &'s crate::typography::TextStyle,
    origin: Point,
    element_width: f32,
    isotope_width: f32,
    size: f32,
    small: f32,
}
impl LabelFrame<'_> {
    fn text_width(&self, text: &str, size: f32) -> f32 {
        crate::style::styled_text_width(text, size, self.style)
    }
    fn text(&self, position: Point, content: String, size: f32) -> Primitive {
        Primitive::Text {
            position,
            text: content,
            size,
            color: self.style.color.rgb(),
            style: self.style.clone(),
        }
    }
}

/// The runs placed so far, and where the next charge or radical goes.
struct LabelCursor {
    runs: Vec<Primitive>,
    right: f32,
    mark_y: f32,
}

/// The suffix of a condensed label (Me in NMe), inline or stacked.
fn push_condensed_suffix(
    a: &Atom,
    doc: &Document,
    frame: LabelFrame<'_>,
    cursor: &mut LabelCursor,
    suffix: &str,
    layout: Option<crate::typography::Layout>,
) {
    let LabelFrame {
        style,
        origin,
        element_width,
        isotope_width,
        size,
        ..
    } = frame;
    let layout = layout.unwrap_or_else(|| {
        crate::typography::layout(
            suffix,
            &crate::typography::TextFormat {
                style: crate::typography::TextStyle {
                    formula: true,
                    ..style.clone()
                },
                ..Default::default()
            },
        )
    });
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
        &cursor.runs,
        &mut parts,
        (origin, element_width),
        (layout.width, isotope_width),
        size,
        side,
    );
    cursor.runs.extend(parts);
    if side == crate::atom_labels::HydrogenPosition::Right {
        cursor.right += layout.width;
    }
}

/// Attached hydrogens with their count, inline or stacked, in their own ink.
fn push_hydrogens(
    a: &Atom,
    doc: &Document,
    frame: LabelFrame<'_>,
    cursor: &mut LabelCursor,
    show_element: bool,
) {
    let LabelFrame {
        origin,
        element_width,
        isotope_width,
        size,
        small,
        ..
    } = frame;
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
        let h_width = frame.text_width("H", size);
        let width = h_width + frame.text_width(&count, small);
        use crate::atom_labels::HydrogenPosition as H;
        let position = crate::atom_labels::appendage_position(a, doc);
        let mut parts = vec![frame.text(Point::default(), "H".into(), size)];
        if !count.is_empty() {
            parts.push(frame.text(Point::new(h_width, size * 0.40), count, small));
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
            &cursor.runs,
            &mut parts,
            (origin, element_width),
            (width, isotope_width),
            size,
            position,
        );
        if matches!(position, H::Above | H::Below) {
            if let Some(Primitive::Text { position, .. }) = parts.first() {
                cursor.mark_y = position.y;
            }
            cursor.right = origin.x + width;
        }
        cursor.runs.extend(parts);
        if position == H::Right {
            cursor.right += width;
        }
    }
}

/// The mass number, raised before the element symbol.
fn push_isotope(a: &Atom, frame: LabelFrame<'_>, cursor: &mut LabelCursor) {
    let LabelFrame {
        origin,
        size,
        small,
        ..
    } = frame;
    if a.isotope > 0 {
        let isotope = a.isotope.to_string();
        cursor.runs.push(frame.text(
            origin.offset(-frame.text_width(&isotope, small), -size * 0.25),
            isotope,
            small,
        ));
    }
}

/// The printed charge, unless hidden or drawn by an attached charge mark.
fn push_charge(a: &Atom, frame: LabelFrame<'_>, cursor: &mut LabelCursor) {
    let LabelFrame { size, small, .. } = frame;
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
        let width = frame.text_width(&label, small);
        cursor.runs.push(frame.text(
            Point::new(cursor.right, cursor.mark_y - size * 0.25),
            label,
            small,
        ));
        cursor.right += width;
    }
}

/// Radical dots, unless drawn by an attached radical mark.
fn push_radicals(a: &Atom, frame: LabelFrame<'_>, cursor: &mut LabelCursor) {
    let LabelFrame { size, small, .. } = frame;
    if a.radical_electrons > 0
        && !a.marks.iter().any(|m| {
            m.kind.radical() && (m.kind != crate::scientific::MarkKind::RadicalIon || a.charge != 0)
        })
    {
        cursor.runs.push(frame.text(
            Point::new(cursor.right, cursor.mark_y - size * 0.25),
            "•".repeat(a.radical_electrons as usize),
            small,
        ));
    }
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
            Owner::Number(id) | Owner::Mapping(id) | Owner::AtomStereo(id) => id,
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

/// Visible backbone span, with the same label ink and margin as bond rendering.
/// Used by drawing-target picking so hidden label-covered extensions cannot
/// create an initial mechanism-arrow attachment.
pub(crate) fn bond_segment(
    doc: &Document,
    bond: &Bond,
    opacity: Option<&crate::rear_opacity::Paint>,
) -> Option<(Point, Point)> {
    if !doc.bond_visible(bond.a, bond.b) {
        return None;
    }
    let (a, b) = doc.atom(bond.a).zip(doc.atom(bond.b))?;
    let boxes = |a: &Atom| {
        if label_trims_bonds(a, doc, opacity) {
            atom_label_ink_boxes(a, doc)
        } else {
            vec![]
        }
    };
    let margin = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
    let span = trimmed_span(a.position, b.position, &boxes(a), &boxes(b), margin)?;
    // Unlike bond_frame, picking rejects a NaN visible span.
    (span.along > 0.1).then_some((span.start, span.end))
}

/// A label-trimmed segment: its ends, unit direction and visible length.
struct TrimmedSpan {
    start: Point,
    end: Point,
    ux: f32,
    uy: f32,
    along: f32,
}

/// Shared trimming arithmetic. Each caller keeps its own visible-span test:
/// bond_frame's `<= 0.1` and bond_segment's `> 0.1` differ for NaN.
fn trimmed_span(
    a: Point,
    z: Point,
    a_boxes: &[(Point, Point)],
    z_boxes: &[(Point, Point)],
    margin: f32,
) -> Option<TrimmedSpan> {
    let length = a.distance(z);
    if length < 0.1 {
        return None;
    }
    let ux = (z.x - a.x) / length;
    let uy = (z.y - a.y) / length;
    let start = label_end(a, ux, uy, a_boxes, length, margin);
    let end = label_end(z, -ux, -uy, z_boxes, length, margin);
    let along = (end.x - start.x) * ux + (end.y - start.y) * uy;
    Some(TrimmedSpan {
        start,
        end,
        ux,
        uy,
        along,
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
    double_position(&FaceIndex::new(doc), bond)
}
pub(crate) fn double_position(faces: &FaceIndex<'_>, bond: &Bond) -> crate::bonds::DoublePosition {
    use crate::bonds::DoublePosition as P;
    match bond.double_position {
        P::Auto => match automatic_double_side(faces, bond) {
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
fn automatic_double_side(faces: &FaceIndex<'_>, bond: &Bond) -> Option<f32> {
    if matches!(bond.order, 2 | 7)
        && let Some(side) = faces.automatic_side(bond)
    {
        return Some(side);
    }
    let doc = faces.doc();
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
    let opacity = crate::rear_opacity::Paint::new(doc);
    primitives_with_opacity(doc, &opacity)
}
/// Drawing primitives using shared rear-opacity paint. `opacity` must be
/// `rear_opacity::Paint::new` of this same raw, unmaterialized document.
pub fn primitives_with_opacity(
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<Primitive> {
    let inputs = BondInputs::prepare(doc, opacity, Resolve::Scene);
    let doc = inputs.doc();
    let style = &doc.drawing_style;
    let mut out = vec![];
    let orbital_label_boxes = orbital_label_boxes(doc, &inputs.labels().runs, opacity);
    let mut graphics: Vec<_> = doc.graphics.iter().collect();
    graphics.sort_by_key(|g| g.layer);
    push_underlays(&mut out, doc, &graphics, opacity, &orbital_label_boxes);
    let rings = push_ring_strokes(&mut out, doc, opacity);
    let pass = BondPass::new(&inputs);
    let doc = pass.doc;
    // Fill joined bond outlines together. Separate antialiased polygons leave
    // translucent seams even when their mathematical corners agree exactly.
    let mut joined: std::collections::BTreeMap<
        (crate::palette::Color, u32),
        Vec<crate::graphics::PathCommand>,
    > = Default::default();
    let scene = pass.scene(style, &rings, opacity);
    let bond_paint_start = out.len();
    for (bond_index, b) in doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| doc.bond_visible(b.a, b.b))
    {
        push_bond(scene, &mut out, &mut joined, bond_index, b, None);
    }
    push_junctions(
        &mut out,
        &mut joined,
        &pass.joins,
        &rings.crossing_gaps,
        doc,
        opacity,
    );
    push_joined_outlines(&mut out, joined);
    if opacity.has_visibility() {
        paint_rear_before_front(&mut out, bond_paint_start);
    }
    push_atom_labels(&mut out, doc, &pass.labels.runs, opacity);
    push_arrows_and_annotations(&mut out, doc);
    out.extend(
        graphics
            .iter()
            .filter(|g| g.layer >= 0)
            .flat_map(|graphic| graphic_primitive(graphic, &orbital_label_boxes)),
    );
    out
}

/// Visible label ink that orbital lobes keep clear of; empty without orbitals.
fn orbital_label_boxes(
    doc: &Document,
    labels: &std::collections::HashMap<u64, Vec<Primitive>>,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<(Point, Point)> {
    if doc
        .graphics
        .iter()
        .any(|graphic| matches!(graphic.kind, crate::graphics::GraphicKind::Orbital(_)))
    {
        doc.atoms
            .iter()
            .filter(|atom| doc.atom_visible(atom.id) && opacity.atom(atom.id) > 0.)
            .flat_map(|atom| {
                labels
                    .get(&atom.id)
                    .map(|runs| label_ink_boxes(runs))
                    .unwrap_or_default()
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Projection tip orientation is independent of dative donor→acceptor
/// direction. Normalize only a detached render copy, after collecting the
/// chemical labels from the original graph (reversing a dative graph would
/// otherwise change donor hydrogen counts).
fn projection_render_copy(doc: &Document) -> std::borrow::Cow<'_, Document> {
    if doc.bonds.iter().any(|b| b.narrowed_at_acceptor()) {
        let mut painted = doc.clone();
        for bond in &mut painted.bonds {
            if bond.narrowed_at_acceptor() {
                // Unlike Bond::base_display, render normalization strips repeated suffixes.
                bond.display = bond.display.trim_end_matches("_end").into();
                bond.reverse();
            }
        }
        std::borrow::Cow::Owned(painted)
    } else {
        std::borrow::Cow::Borrowed(doc)
    }
}

fn paint_rear_before_front(out: &mut Vec<Primitive>, start: usize) {
    // Every opaque front shape covers its faded base, including differently
    // colored secondary rails; preserve order within each paint pass.
    let (rear, front): (Vec<_>, Vec<_>) = out
        .drain(start..)
        .partition(|part| matches!(part, Primitive::Opacity { .. }));
    out.extend(rear);
    out.extend(front);
}

fn graphic_primitive(
    g: &&crate::graphics::Graphic,
    label_boxes: &[(Point, Point)],
) -> Vec<Primitive> {
    if g.kind == crate::graphics::GraphicKind::Picture {
        vec![Primitive::Picture((*g).clone())]
    } else {
        let parts = if matches!(g.kind, crate::graphics::GraphicKind::Orbital(_)) {
            crate::scientific::readable_orbital_parts(g, label_boxes)
        } else {
            g.parts()
        };
        parts
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
    opacity: &crate::rear_opacity::Paint,
    label_boxes: &[(Point, Point)],
) {
    out.extend(
        graphics
            .iter()
            .filter(|g| g.layer < 0)
            .flat_map(|graphic| graphic_primitive(graphic, label_boxes)),
    );
    for fill in &doc.ring_fills {
        out.extend(opacity.owned_parts(
            doc,
            &fill.atoms,
            fill.primitive(doc).into_iter().collect(),
        ));
    }
    out.extend(crate::highlights::primitives_with_opacity(doc, opacity));
}
/// Aromatic circles and ring arcs, with the crossing gaps they add.
struct RingStrokes {
    arcs: crate::ring_arcs::Arcs,
    circles: Vec<crate::aromatic::Circle>,
    crossing_gaps: Vec<Vec<crate::crossings::Gap>>,
}
impl RingStrokes {
    /// Occlusion uses stored ring geometry without ring ink or crossing gaps,
    /// unlike the scene/picking stage in push_ring_strokes.
    fn without_gaps(doc: &Document) -> Self {
        Self {
            arcs: crate::ring_arcs::render(doc),
            circles: crate::aromatic::circles(doc),
            crossing_gaps: vec![vec![]; doc.bonds.len()],
        }
    }
}
fn push_ring_strokes(
    out: &mut Vec<Primitive>,
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> RingStrokes {
    // Ring knockouts decide crossings with a Paint of the canonical document
    // they cut: its RGB colors can split one ring arc into several pieces, so
    // near the visibility budget only that Paint falls back to opaque. The
    // raw-document Paint still fades ring ink and bond gaps. Built once, for
    // the first ring stroke.
    let ring_paint = std::cell::OnceCell::new();
    let ring_opacity = || ring_paint.get_or_init(|| crate::rear_opacity::Paint::new(doc));
    let arcs = crate::ring_arcs::render_with_opacity(doc, ring_opacity);
    // A partial curve replaces the ring's circle, not its aromatic membership.
    // Retain every ring here so its other edges do not gain fallback dashes.
    let circles = crate::aromatic::circles(doc);
    let mut crossing_gaps = crate::crossings::gaps_with_opacity(doc, opacity);
    for circle in circles.iter().filter(|c| !arcs.intersects(c)) {
        for part in circle.graphic().parts() {
            let stroke = Primitive::Path {
                commands: part.commands,
                style: part.style,
                filled: part.filled,
            };
            let (stroke, gaps) = crate::crossings::ring_stroke(doc, circle, stroke, ring_opacity());
            out.extend(opacity.owned_parts(doc, &circle.atoms, vec![stroke]));
            for (index, gap) in gaps {
                if let Some(gaps) = crossing_gaps.get_mut(index) {
                    gaps.push(gap);
                }
            }
        }
    }
    for (primitive, ids) in arcs.primitives.iter().zip(&arcs.owners) {
        out.extend(opacity.owned_parts(doc, ids, vec![primitive.clone()]));
    }
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
fn collect_atom_labels(doc: &Document, opacity: &crate::rear_opacity::Paint) -> AtomLabels {
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
                .filter(|a| label_trims_bonds(a, doc, Some(opacity)))
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
        (crate::palette::Color, u32),
        Vec<crate::graphics::PathCommand>,
    >,
    joins: &crate::bond_joins::Joins<'_>,
    crossing_gaps: &[Vec<crate::crossings::Gap>],
    doc: &Document,
    opacity: &crate::rear_opacity::Paint,
) {
    for junction in joins.junctions() {
        use crate::graphics::PathCommand;
        let mut batches: std::collections::BTreeMap<u32, Vec<PathCommand>> = Default::default();
        for (bond_index, points) in junction.parts {
            let parts = crate::crossings::cut(
                vec![Primitive::Polygon(points)],
                crossing_gaps
                    .get(bond_index)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            );
            let parts = if let Some(bond) = doc.bonds.get(bond_index) {
                opacity.bond_parts(doc, bond, parts)
            } else {
                parts
            };
            for part in parts {
                let (alpha, part) = take_opacity(part);
                if let Primitive::Polygon(points) = part
                    && let Some(first) = points.first()
                {
                    let commands = batches.entry(alpha.to_bits()).or_default();
                    commands.push(PathCommand::Move(*first));
                    commands.extend(points.iter().skip(1).copied().map(PathCommand::Line));
                    commands.push(PathCommand::Close);
                }
            }
        }
        for (alpha, commands) in batches {
            let alpha = f32::from_bits(alpha);
            if junction.underlay && alpha < 1. {
                continue;
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
                joined
                    .entry((junction.color, alpha.to_bits()))
                    .or_default()
                    .extend(commands);
            }
        }
    }
}
fn push_joined_outlines(
    out: &mut Vec<Primitive>,
    joined: std::collections::BTreeMap<
        (crate::palette::Color, u32),
        Vec<crate::graphics::PathCommand>,
    >,
) {
    out.extend(joined.into_iter().flat_map(|((color, alpha), commands)| {
        crate::rear_opacity::with_opacity(
            vec![Primitive::Path {
                commands,
                style: crate::graphics::GraphicStyle {
                    stroke: color,
                    fill: Some(color),
                    width_pt: 0.,
                    ..Default::default()
                },
                filled: true,
            }],
            f32::from_bits(alpha),
        )
    }));
}
fn push_atom_labels(
    out: &mut Vec<Primitive>,
    doc: &Document,
    labels: &std::collections::HashMap<u64, Vec<Primitive>>,
    opacity: &crate::rear_opacity::Paint,
) {
    for a in doc.atoms.iter().filter(|a| doc.atom_visible(a.id)) {
        let mut parts: Vec<_> = labels.get(&a.id).into_iter().flatten().cloned().collect();
        parts.extend(
            crate::scientific::styled_mark_parts(a, &doc.drawing_style)
                .into_iter()
                .map(|p| Primitive::Path {
                    commands: p.commands,
                    style: p.style,
                    filled: p.filled,
                }),
        );
        out.extend(opacity.atom_parts(a.id, parts));
    }
    for indicator in crate::atom_labels::indicators(doc) {
        use crate::atom_labels::Owner;
        let alpha = match indicator.owner {
            Owner::Number(id) | Owner::Mapping(id) | Owner::AtomStereo(id) => opacity.atom(id),
            Owner::BondStereo(a, b) => doc
                .bonds
                .iter()
                .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
                .map_or(1., |bond| opacity.bond(bond, 0.5)),
        };
        out.extend(crate::rear_opacity::with_opacity(
            vec![indicator.primitive()],
            alpha,
        ));
    }
}
fn push_arrows_and_annotations(out: &mut Vec<Primitive>, doc: &Document) {
    for unit in &doc.generic.repeats {
        if let Some(graphic) = unit
            .bracket
            .and_then(|id| doc.graphics.iter().find(|g| g.id == id))
        {
            let (_, hi) = graphic.bounds();
            let style = doc.drawing_style.text_style();
            out.push(Primitive::Text {
                position: hi.offset(4., 4.),
                text: unit.label.clone(),
                size: style.size(),
                color: style.color.rgb(),
                style,
            });
        }
    }
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
    faces: &'a FaceIndex<'d>,
    crossing_gaps: &'a [Vec<crate::crossings::Gap>],
    opacity: &'a crate::rear_opacity::Paint,
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
    let span = trimmed_span(
        a.position,
        z.position,
        label_bounds
            .get(&a.id)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        label_bounds
            .get(&z.id)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        style.world(style.margin_width_pt),
    )?;
    // Unlike bond_segment, rendering retains a NaN visible span.
    if span.along <= 0.1 {
        return None;
    }
    let TrimmedSpan {
        start, end, ux, uy, ..
    } = span;
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

/// A rail carries its own depth (projected face or plain atom-to-atom
/// DepthField::Bond) only when the pass needs depth; every other piece paints
/// with bond_depth_field.
struct BondPiece {
    primitive: Primitive,
    depth: Option<projected_bonds::RailDepth>,
}
impl BondPiece {
    /// A non-rail piece, painted with the bond's own depth.
    fn plain(primitive: Primitive) -> Self {
        Self {
            primitive,
            depth: None,
        }
    }
}

fn push_bond(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    joined: &mut std::collections::BTreeMap<
        (crate::palette::Color, u32),
        Vec<crate::graphics::PathCommand>,
    >,
    bond_index: usize,
    b: &Bond,
    ink_sources: Option<&mut Vec<(DepthField, Vec<Primitive>)>>,
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
    let depth = ink_sources.is_some() || scene.opacity.has_visibility();
    let mut pieces = Vec::new();
    match b.display.as_str() {
        "plain" | "bold" | "wedge" if !matches!(b.order, 2 | 7) && joins.needed(b) => {
            pieces.push(BondPiece::plain(Primitive::Polygon(
                joins.polygon(b, start, end),
            )));
        }
        "hollow_wedge" => push_hollow_wedge(scene, &mut pieces, b, start, end),
        "hash" | "hashed" => push_hashed_wedge(scene, &mut pieces, b, frame),
        "wavy" if b.order == 2 => {
            let half = style.bond_length_world * style.bond_spacing_ratio / 2.0;
            for sign in [-1.0, 1.0] {
                pieces.push(BondPiece::plain(Primitive::Line(
                    start.offset(nx * half * sign, ny * half * sign),
                    end.offset(-nx * half * sign, -ny * half * sign),
                    style.line_width(),
                )));
            }
        }
        "wavy" => {
            pieces.push(BondPiece::plain(Primitive::Path {
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
            }));
        }
        _ => push_bond_rails(scene, &mut pieces, b, frame, depth),
    }
    if b.order == 5 && b.display == "plain" {
        head(&mut pieces, end, uy.atan2(ux), false, style.line_width());
    }
    finish_bond_paint(scene, out, b, bond_index, bond_start, pieces, ink_sources);
    collect_joined_outlines(scene, out, joined, bond_start, b);
    recolor_bond(out, bond_start, b);
}

/// Collect occlusion ink, cut crossing gaps and apply rear opacity to the
/// bond's pieces, appending them to `out` in order.
fn finish_bond_paint(
    scene: BondScene<'_, '_>,
    out: &mut Vec<Primitive>,
    b: &Bond,
    bond_index: usize,
    bond_start: usize,
    pieces: Vec<BondPiece>,
    ink_sources: Option<&mut Vec<(DepthField, Vec<Primitive>)>>,
) {
    if let Some(sources) = ink_sources {
        for piece in &pieces {
            let field = piece
                .depth
                .as_ref()
                .map(|r| r.field)
                .unwrap_or_else(|| bond_depth_field_with_faces(scene.faces, b));
            sources.push((field, vec![piece.primitive.clone()]));
        }
    }
    if !scene.opacity.has_visibility() {
        out.extend(pieces.into_iter().map(|piece| piece.primitive));
        cut_bond_crossings(scene, out, bond_index, bond_start);
    } else {
        for BondPiece { primitive, depth } in pieces {
            let cut = crate::crossings::cut(
                vec![primitive],
                scene
                    .crossing_gaps
                    .get(bond_index)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            );
            if let Some(rail) = depth {
                out.extend(scene.opacity.rail_parts(scene.doc, b, &rail, cut));
            } else {
                out.extend(scene.opacity.bond_parts(scene.doc, b, cut));
            }
        }
    }
}
fn push_hollow_wedge(
    scene: BondScene<'_, '_>,
    pieces: &mut Vec<BondPiece>,
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
        pieces.push(BondPiece::plain(Primitive::Path {
            commands,
            style: crate::graphics::GraphicStyle {
                width_pt: style.line_width_pt,
                ..Default::default()
            },
            filled: false,
        }));
    }
}
fn push_hashed_wedge(
    scene: BondScene<'_, '_>,
    pieces: &mut Vec<BondPiece>,
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
        pieces.push(BondPiece::plain(Primitive::Line(
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
        )));
    }
}
fn push_bond_rails(
    scene: BondScene<'_, '_>,
    pieces: &mut Vec<BondPiece>,
    b: &Bond,
    frame: BondFrame<'_>,
    depth: bool,
) {
    let BondScene {
        style,
        arcs,
        circles,
        label_bounds,
        joins,
        faces,
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
        automatic_double_side(faces, b).unwrap_or(1.)
    } else {
        1.
    };
    use crate::bonds::DoublePosition;
    let position = if matches!(b.order, 2 | 7) {
        double_position(faces, b)
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
        let projected =
            if matches!(order, 2 | 7) && b.double_position == DoublePosition::Auto && *offset != 0.
            {
                faces.rail_depth(
                    b,
                    start,
                    end,
                    *offset,
                    trim,
                    style.world(if display == "bold" {
                        style.bold_width_pt
                    } else {
                        style.line_width_pt
                    }) / 2.,
                )
            } else {
                None
            };
        let (first, last) = match projected.as_ref() {
            Some(Some(rail)) => rail.points,
            Some(None) => continue,
            None => (
                start.offset(nx * offset + ux * trim, ny * offset + uy * trim),
                end.offset(nx * offset - ux * trim, ny * offset - uy * trim),
            ),
        };
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
        let depth = depth.then(|| match projected {
            Some(Some(mut rail)) => {
                rail.points = (first, last);
                rail
            }
            _ => projected_bonds::RailDepth {
                points: (first, last),
                field: DepthField::Bond {
                    a: a.position,
                    b: z.position,
                    wa: a.depth,
                    wb: z.depth,
                },
                support: vec![],
            },
        });
        let primitive = if index == 0 && *offset == 0. && joins.needed(b) {
            Primitive::Polygon(joins.polygon(b, first, last))
        } else if display == "bold" {
            // An explicitly centered bold rail still needs flat
            // ends; round caps protrude beyond the junction.
            let half = style.world(style.bold_width_pt) / 2.;
            Primitive::Polygon(vec![
                first.offset(nx * half, ny * half),
                last.offset(nx * half, ny * half),
                last.offset(-nx * half, -ny * half),
                first.offset(-nx * half, -ny * half),
            ])
        } else if matches!(display, "dashed" | "dotted") {
            Primitive::Path {
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
            }
        } else {
            Primitive::Line(
                first,
                last,
                if display == "bold" {
                    style.world(style.bold_width_pt)
                } else {
                    style.line_width()
                },
            )
        };
        pieces.push(BondPiece { primitive, depth });
    }
    if b.order == 4
        && !arcs.contains(b.a, b.b)
        && !circles.iter().any(|c| c.contains_bond(b.a, b.b))
    {
        for i in 0..5 {
            let t = i as f32 / 5.0;
            let v = (i as f32 + 0.5) / 5.0;
            pieces.push(BondPiece::plain(Primitive::Line(
                Point::new(
                    start.x + (end.x - start.x) * t + nx * spacing * inward,
                    start.y + (end.y - start.y) * t + ny * spacing * inward,
                ),
                Point::new(
                    start.x + (end.x - start.x) * v + nx * spacing * inward,
                    start.y + (end.y - start.y) * v + ny * spacing * inward,
                ),
                style.line_width(),
            )));
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
        (crate::palette::Color, u32),
        Vec<crate::graphics::PathCommand>,
    >,
    bond_start: usize,
    b: &Bond,
) {
    let BondScene { joins, .. } = scene;
    if joins.needed(b) && b.display != "hollow_wedge" {
        use crate::graphics::PathCommand;
        let mut secondary = Vec::new();
        for primitive in out.drain(bond_start..) {
            let (alpha, primitive) = take_opacity(primitive);
            if let Primitive::Polygon(points) = primitive {
                if let Some(first) = points.first() {
                    let commands = joined.entry((b.color, alpha.to_bits())).or_default();
                    commands.push(PathCommand::Move(*first));
                    commands.extend(points.iter().skip(1).copied().map(PathCommand::Line));
                    commands.push(PathCommand::Close);
                }
            } else {
                secondary.extend(crate::rear_opacity::with_opacity(vec![primitive], alpha));
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
                Primitive::Opacity { primitive, .. } => {
                    recolor_bond(std::slice::from_mut(primitive.as_mut()), 0, b)
                }
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
fn take_opacity(primitive: Primitive) -> (f32, Primitive) {
    match primitive {
        Primitive::Opacity { alpha, primitive } => (alpha, *primitive),
        primitive => (1., primitive),
    }
}
/// Dative arrowhead extents in world units: length along the bond and half
/// width across it. Generic ink bounds reserve the same wings.
pub(crate) const DATIVE_HEAD_LENGTH: f32 = 10.;
pub(crate) const DATIVE_HEAD_HALF_WIDTH: f32 = 3.5;

fn head(pieces: &mut Vec<BondPiece>, end: Point, angle: f32, half: bool, width: f32) {
    let a = end.offset(
        -angle.cos() * DATIVE_HEAD_LENGTH - angle.sin() * DATIVE_HEAD_HALF_WIDTH,
        -angle.sin() * DATIVE_HEAD_LENGTH + angle.cos() * DATIVE_HEAD_HALF_WIDTH,
    );
    if half {
        pieces.push(BondPiece::plain(Primitive::Line(end, a, width)));
    } else {
        let b = end.offset(
            -angle.cos() * DATIVE_HEAD_LENGTH + angle.sin() * DATIVE_HEAD_HALF_WIDTH,
            -angle.sin() * DATIVE_HEAD_LENGTH - angle.cos() * DATIVE_HEAD_HALF_WIDTH,
        );
        pieces.push(BondPiece::plain(Primitive::Polygon(vec![end, a, b])));
    }
}
pub fn bounds(drawing: &[Primitive]) -> (Point, Point) {
    let mut points = vec![];
    for p in drawing {
        match p {
            Primitive::Opacity { alpha, primitive } => {
                if *alpha > 0. {
                    let (lo, hi) = bounds(std::slice::from_ref(primitive.as_ref()));
                    let pad = STYLE.world(4.);
                    points.extend([lo.offset(pad, pad), hi.offset(-pad, -pad)]);
                }
            }
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
    svg::render_svg(doc, false)
}
/// A figure carries the canvas background when exported or copied.
pub fn svg_with_background(doc: &Document) -> String {
    svg::render_svg(doc, true)
}

#[cfg(test)]
mod charge_placement_tests;
#[cfg(test)]
mod label_parity_tests;
#[cfg(test)]
mod primitives_parity_tests;
#[cfg(test)]
mod tests;
