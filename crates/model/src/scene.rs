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
    /// Paper-colored ink spread `radius` around the shapes of `primitives`
    /// (atom labels drawn right after it), covering everything painted before.
    /// Without a background, backends knock that earlier ink out instead, so
    /// whatever lies behind the drawing shows through the halo.
    Halo {
        radius: f32,
        primitives: Vec<Primitive>,
    },
    /// A shaded 3D-model ball: a two-point radial gradient from a circle of
    /// `radius × models3d::FOCAL_RADIUS` at `focal` out to the disc. Colors
    /// are exact; no canvas theme changes them. With a `contact` it fills
    /// only the part of the disc the balls it overlaps leave visible.
    Sphere {
        center: Point,
        radius: f32,
        focal: Point,
        stops: Vec<(f32, [u8; 3])>,
        outline: Option<Outline>,
        contact: Option<Box<Contact>>,
    },
    /// A shaded 3D-model bond half: `corners` filled with a linear gradient
    /// from `from` to `to`, in exact colors. The outline strokes the two long
    /// sides, corners 0–1 and 3–2.
    ShadedQuad {
        corners: [Point; 4],
        from: Point,
        to: Point,
        stops: Vec<(f32, [u8; 3])>,
        outline: Option<Outline>,
    },
    /// A mitered 3D-model stick half: `points` filled with a linear gradient
    /// from `from` to `to`, in exact colors, without an outline.
    ShadedPolygon {
        points: Vec<Point>,
        from: Point,
        to: Point,
        stops: Vec<(f32, [u8; 3])>,
    },
    /// The outer edge of the 3D-model sticks drawn after it: every polygon
    /// stroked as one shape with round joins, so where strokes overlap they
    /// do not darken. The fills drawn over it leave only the outside half.
    Contour {
        polygons: Vec<Vec<Point>>,
        outline: Outline,
    },
    /// A 3D-model wireframe half-bond: a round-capped line in an exact color,
    /// or with `dash` above 0 a dashed one with dashes and gaps that long,
    /// as a crystal's hidden cell edges and bonds leaving the cell are.
    Wire {
        from: Point,
        to: Point,
        width: f32,
        color: [u8; 3],
        dash: f32,
    },
    /// A 3D model's soft background card: a rounded rectangle in exact colors.
    Card {
        lo: Point,
        hi: Point,
        radius: f32,
        fill: [u8; 3],
        outline: Outline,
    },
}
/// Where a 3D-model ball meets the balls it overlaps.
#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    /// The visible part of the ball, outer contours and holes filled
    /// even-odd, drawn in place of the disc. Points on the disc's rim are
    /// joined by its arc.
    pub region: Vec<Vec<Point>>,
    /// The visible runs of the rim, outlined in place of the whole circle.
    pub rim: Vec<Vec<Point>>,
    /// Where it meets balls drawn before it, outlined.
    pub seams: Vec<Vec<Point>>,
}
impl std::hash::Hash for Contact {
    /// Every coordinate's bits, cheaper than its `Debug` text for a cache key.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let point = |p: Point, state: &mut H| {
            p.x.to_bits().hash(state);
            p.y.to_bits().hash(state);
        };
        for line in [&self.region, &self.rim, &self.seams] {
            for run in line {
                run.iter().for_each(|p| point(*p, state));
                u32::MAX.hash(state);
            }
        }
    }
}
/// An exact, translucent stroke around a 3D-model shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    pub color: [u8; 3],
    pub alpha: f32,
    pub width: f32,
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
    let reach = LabelReach::new(doc);
    doc.atoms.iter().rev().find_map(|atom| {
        // Laying out a label is costly, so skip atoms it cannot reach from.
        let limit = reach.of(atom).map_or(f32::INFINITY, |reach| reach + radius);
        if (atom.position.x - point.x).abs() > limit
            || (atom.position.y - point.y).abs() > limit
            || crate::models3d::hides(doc, atom.id)
        {
            return None;
        }
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

/// An upper bound on how far an atom's label ink reaches from the atom.
struct LabelReach {
    /// The largest label font size times its fonts' widest glyph (at least
    /// two ems), or `None` when a label font cannot be bounded.
    em: Option<f32>,
    /// The longest text of each group label, by anchor.
    groups: std::collections::HashMap<u64, usize>,
}

impl LabelReach {
    fn new(doc: &Document) -> Self {
        let styles = || {
            doc.atoms
                .iter()
                .filter_map(|atom| atom.text_style.as_ref())
                .chain(
                    doc.abbreviations
                        .iter()
                        .filter_map(|g| g.label_style.as_ref()),
                )
        };
        let size_pt = styles()
            .map(|style| style.size_pt)
            .fold(doc.drawing_style.font_size_pt, f32::max);
        let families: std::collections::BTreeSet<_> = styles()
            .map(|style| style.family.as_str())
            .chain([doc.drawing_style.font_family.as_str()])
            .collect();
        let glyph = families.into_iter().try_fold(2f32, |em, family| {
            Some(em.max(crate::style::label_glyph_reach(family)?))
        });
        let mut groups = std::collections::HashMap::new();
        for group in &doc.abbreviations {
            let longest: &mut usize = groups.entry(group.anchor).or_default();
            *longest = (*longest)
                .max(group.label.len())
                .max(group.reverse_label.len());
        }
        Self {
            em: glyph.map(|glyph| glyph * STYLE.world(size_pt)),
            groups,
        }
    }

    /// Every glyph the label can draw (its symbol, variable or group text,
    /// hydrogens with their count, isotope, charge and radicals) in a row,
    /// plus three ems for a stacked appendage, raised scripts and the
    /// charge's clearance. Byte lengths count at least every character.
    fn of(&self, atom: &Atom) -> Option<f32> {
        let digits = |n: u32| n.checked_ilog10().map_or(1, |d| d as usize + 1);
        let glyphs = atom.element.len()
            + atom.display.variable.as_ref().map_or(0, String::len)
            + self.groups.get(&atom.id).copied().unwrap_or_default()
            + 1
            + digits(atom.label_h.max(atom.explicit_h))
            + digits(atom.isotope)
            + 1
            + digits(atom.charge.unsigned_abs())
            + usize::from(atom.radical_electrons);
        self.em.map(|em| em * (glyphs + 3) as f32)
    }
}

pub(crate) fn atom_label_ink_boxes(a: &Atom, doc: &Document) -> Vec<(Point, Point)> {
    label_ink_boxes(&atom_label(a, doc))
}

/// Each label glyph's ink box with the corner radius its outline leaves free.
pub(crate) fn atom_label_ink_shapes(a: &Atom, doc: &Document) -> Vec<((Point, Point), f32)> {
    atom_label(a, doc)
        .iter()
        .flat_map(|run| match run {
            Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } => crate::style::text_ink_shapes(text, *size, style)
                .into_iter()
                .map(|((lo, hi), radius)| {
                    (
                        (position.offset(lo.x, lo.y), position.offset(hi.x, hi.y)),
                        radius,
                    )
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect()
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
    // A free text label: its bonds meet the attachment glyph (O in MeO).
    let free_text = internal_group.is_none() && crate::atom_labels::free_text(a, doc);
    let attachment = a.display.label_attachment;
    let condensed = (internal_group.is_some() || a.element == "*" || is_link)
        .then(|| crate::atom_labels::condensed_label(label))
        .flatten()
        .filter(|_| !free_text || attachment.is_auto());
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
    let attached = (free_text && condensed.is_none())
        .then(|| crate::atom_labels::attachment_range(a, doc, label))
        .flatten();
    // A generic label meets its single horizontal bond with its near edge.
    let generic_side = (internal_group.is_none() && condensed.is_none())
        .then(|| crate::atom_labels::generic_label_side(a, doc))
        .flatten();
    let centered = match generic_side {
        Some(true) => element_width,
        Some(false) => 0.,
        None => element_width / 2.,
    };
    let anchor_x = attached.map_or(centered, |range| {
        let width = |end: usize| match scripts.as_ref().filter(|_| layout.is_some()) {
            Some(scripts) => free_label_layout(label, scripts, 0..end, &style).width,
            None => text_width(label.get(..end).unwrap_or_default(), size),
        };
        (width(range.start) + width(range.end)) / 2.
    });
    let origin = a.position.offset(
        -anchor_x,
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
    // A hyphen inside a name (2-Py, Indol-1-yl) is a separator, not a charge.
    let spans = content
        .char_indices()
        .zip(content.chars().skip(1))
        .filter(|&((_, c), next)| c == '-' && next.is_alphanumeric())
        .map(|((start, _), _)| crate::typography::TextSpan {
            start,
            end: start + 1,
            style: crate::typography::TextStyle {
                formula: false,
                ..style.clone()
            },
        })
        .collect();
    let format = crate::typography::TextFormat {
        style: style.clone(),
        spans,
        ..Default::default()
    };
    let layout = crate::typography::layout(content, &format);
    let range = crate::abbreviations::anchor_range(content, group.faces_left(doc));
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
        let mut position = crate::atom_labels::appendage_position(a, doc);
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
        let place = |position| {
            let mut parts = parts.clone();
            place_appendage(
                &cursor.runs,
                &mut parts,
                (origin, element_width),
                (width, isotope_width),
                size,
                position,
            );
            parts
        };
        let mut parts = place(position);
        // Keep the bond-based side unless its ink meets a nearby label or
        // charge; a replacement must also stay off unrelated bonds.
        if a.display.hydrogen_position == H::Auto
            && let Some(labels) = nearby_label_ink(a, doc, size)
            && !clear_of(&parts, &labels, &[], size)
        {
            let bonds = unrelated_bonds(a, doc);
            let mut reserved = labels;
            reserved.extend(nearby_mark_ink(a, doc, size).unwrap_or_default());
            if let Some((side, placed)) = crate::atom_labels::appendage_alternatives(a, doc)
                .into_iter()
                .filter(|side| *side != position)
                .map(|side| (side, place(side)))
                .find(|(_, placed)| clear_of(placed, &reserved, &bonds, size))
            {
                position = side;
                parts = placed;
            }
        }
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

thread_local! {
    /// Set while measuring neighboring labels for hydrogen placement, so their
    /// own automatic hydrogens keep the bond-based side instead of recursing.
    static MEASURING_NEARBY_LABELS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Reach of the nearby labels and marks that automatic hydrogens avoid.
fn nearby(a: &Atom, other: &Atom, size: f32) -> bool {
    a.position.distance(other.position) < size * 5.
}

/// Measure other labels or marks, or `None` while already measuring, so
/// neighbors (and marks placed around this label) do not recurse.
fn measuring<T>(measure: impl FnOnce() -> T) -> Option<T> {
    if MEASURING_NEARBY_LABELS.with(|flag| flag.replace(true)) {
        return None;
    }
    let measured = measure();
    MEASURING_NEARBY_LABELS.with(|flag| flag.set(false));
    Some(measured)
}

/// Ink of other atoms' labels and charges near `a` (for example an unbonded
/// metal's 3+).
fn nearby_label_ink(a: &Atom, doc: &Document, size: f32) -> Option<Vec<(Point, Point)>> {
    measuring(|| {
        doc.atoms
            .iter()
            .filter(|other| other.id != a.id && nearby(a, other, size))
            .filter(|other| doc.atom_visible(other.id))
            .flat_map(|other| label_ink_boxes(&atom_label_runs(other, doc)))
            .collect()
    })
}

/// Extents of the explicit marks on `a` and its nearby atoms.
fn nearby_mark_ink(a: &Atom, doc: &Document, size: f32) -> Option<Vec<(Point, Point)>> {
    measuring(|| {
        doc.atoms
            .iter()
            .filter(|other| !other.marks.is_empty() && nearby(a, other, size))
            .filter(|other| doc.atom_visible(other.id))
            .flat_map(|other| crate::scientific::styled_mark_parts(other, &doc.drawing_style))
            .filter_map(|part| {
                let pad = part.style.width() * 0.5;
                part.commands
                    .iter()
                    .flat_map(crate::graphics::PathCommand::iter_points)
                    .map(|p| (p, p))
                    .reduce(|(lo, hi), (p, _)| {
                        (
                            Point::new(lo.x.min(p.x), lo.y.min(p.y)),
                            Point::new(hi.x.max(p.x), hi.y.max(p.y)),
                        )
                    })
                    .map(|(lo, hi)| (lo.offset(-pad, -pad), hi.offset(pad, pad)))
            })
            .collect()
    })
}

/// The drawn strokes of visible bonds that do not end at `a`, each with how
/// far its ink reaches beyond the stroke's center line.
fn unrelated_bonds(a: &Atom, doc: &Document) -> Vec<(Point, Point, f32)> {
    use crate::bonds::DoublePosition as P;
    let style = &doc.drawing_style;
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    let positions: std::collections::HashMap<_, _> = doc
        .atoms
        .iter()
        .map(|atom| (atom.id, atom.position))
        .collect();
    let faces = std::cell::OnceCell::new();
    let faces = || faces.get_or_init(|| FaceIndex::new(doc));
    doc.bonds
        .iter()
        .filter(|b| b.a != a.id && b.b != a.id && doc.bond_visible(b.a, b.b))
        .filter_map(|b| Some((b, *positions.get(&b.a)?, *positions.get(&b.b)?)))
        .flat_map(|(b, p, q)| {
            // Each stroke's start and end offsets along the normal, as
            // `push_bond` and `push_bond_rails` draw them.
            let wavy = b.display == "wavy";
            let rails = |offsets: &[f32]| offsets.iter().map(|o| (*o, *o)).collect::<Vec<_>>();
            let strokes = match b.order {
                // A wavy double bond is two strokes crossing at the middle.
                2 if wavy => vec![(spacing / 2., -spacing / 2.), (-spacing / 2., spacing / 2.)],
                2 | 7 => match double_position(faces(), b) {
                    P::Left => rails(&[0., -spacing]),
                    P::Right => rails(&[0., spacing]),
                    _ => rails(&[-spacing / 2., spacing / 2.]),
                },
                3 => rails(&[-spacing, 0., spacing]),
                4 => rails(&[
                    0.,
                    spacing * automatic_double_side(faces(), b).unwrap_or(1.),
                ]),
                6 => rails(&[-spacing * 1.5, -spacing * 0.5, spacing * 0.5, spacing * 1.5]),
                _ => rails(&[0.]),
            };
            let bold = matches!(
                b.display.as_str(),
                "bold" | "wedge" | "hash" | "hashed" | "hollow_wedge"
            ) || b.secondary_display.as_deref() == Some("bold");
            let ink = if bold {
                style.world(style.bold_width_pt) / 2.
            } else if wavy && b.order != 2 {
                // The wave's amplitude.
                style.line_width() * 1.75
            } else {
                style.line_width() / 2.
            };
            let length = p.distance(q).max(0.001);
            let (nx, ny) = (-(q.y - p.y) / length, (q.x - p.x) / length);
            strokes.into_iter().map(move |(from, to)| {
                (
                    p.offset(nx * from, ny * from),
                    q.offset(nx * to, ny * to),
                    ink,
                )
            })
        })
        .collect()
}

/// Whether ink, padded by a small gap, misses the label boxes and bond strokes.
fn clear_of(
    parts: &[Primitive],
    labels: &[(Point, Point)],
    bonds: &[(Point, Point, f32)],
    size: f32,
) -> bool {
    let gap = size * 0.12;
    label_ink_boxes(parts).into_iter().all(|(lo, hi)| {
        let (lo, hi) = (lo.offset(-gap, -gap), hi.offset(gap, gap));
        labels
            .iter()
            .all(|(a, b)| hi.x <= a.x || lo.x >= b.x || hi.y <= a.y || lo.y >= b.y)
            && !bonds.iter().any(|&(p, q, pad)| {
                let (lo, hi) = (lo.offset(-pad, -pad), hi.offset(pad, pad));
                crate::atom_labels::segment_hits_rect(p, q, lo, hi)
            })
    })
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

pub(crate) fn label_ink_boxes(runs: &[Primitive]) -> Vec<(Point, Point)> {
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
        if crate::models3d::hides(doc, id) {
            continue;
        }
        let corner = label.origin.offset(label.width, label.height);
        grow(id, &mut [label.origin, corner].into_iter());
    }
    // Only selected objects are measured, so one small selection stays cheap.
    for g in doc.graphics.iter().filter(|g| owners.contains_key(&g.id)) {
        let (lo, hi) = g.bounds();
        grow(g.id, &mut [lo, hi].into_iter());
    }
    for model in doc
        .models
        .iter()
        .filter(|m| m.display != crate::models3d::Display::TwoD)
    {
        if let Some(id) = model
            .atoms
            .iter()
            .map(|a| a.id)
            .find(|id| owners.contains_key(id))
            && let Some(corners) = crate::models3d::frame_corners(doc, model)
        {
            grow(id, &mut corners.into_iter());
        }
    }
    // A crystal drawn in 2D and wholly in one selection holds its cell box
    // and axis labels too; split across selections, each part is measured
    // alone.
    for model in doc
        .models
        .iter()
        .filter(|m| m.display != crate::models3d::Display::ThreeD && m.cell.is_some())
    {
        let owner = model.atoms.first().and_then(|a| owners.get(&a.id));
        if let Some(id) = model.atoms.first().map(|a| a.id)
            && owner.is_some()
            && model.atoms.iter().all(|a| owners.get(&a.id) == owner)
        {
            let cells = crate::models3d::flat_cells(std::slice::from_ref(model), doc);
            let ink: Vec<_> = [cells.behind, cells.front, cells.above].concat();
            if !ink.is_empty() {
                let (lo, hi) = self::bounds(&ink);
                grow(id, &mut [lo, hi].into_iter());
            }
        }
    }
    for atom in doc
        .atoms
        .iter()
        .filter(|a| owners.contains_key(&a.id) && doc.atom_drawn(a.id))
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
            && doc.bond_drawn(bond.a, bond.b)
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

pub(crate) fn label_end(
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
pub(crate) fn automatic_double_side(faces: &FaceIndex<'_>, bond: &Bond) -> Option<f32> {
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
    let flat = crate::models3d::drawn(doc);
    let opacity = crate::rear_opacity::Paint::new(&flat);
    primitives_with_opacity(doc, &flat, &opacity)
}
/// Drawing primitives using shared rear-opacity paint. `flat` must be
/// [`crate::models3d::drawn`] of this same raw, unmaterialized document, and
/// `opacity` its `rear_opacity::Paint::new`: atoms shown as 3D models are not
/// painted in 2D.
pub fn primitives_with_opacity(
    raw: &Document,
    flat: &Document,
    opacity: &crate::rear_opacity::Paint,
) -> Vec<Primitive> {
    let inputs = BondInputs::prepare(flat, opacity, Resolve::Scene);
    let doc = inputs.doc();
    let style = &doc.drawing_style;
    let mut out = vec![];
    let mut graphics: Vec<_> = doc.graphics.iter().collect();
    graphics.sort_by_key(|g| g.layer);
    let mut halos = orbital_halos(doc, &graphics, &inputs.labels().runs, opacity);
    let cells = crate::models3d::flat_cells(&raw.models, doc);
    for id in cells.haloed {
        if !halos.contains(id) {
            halos.atoms.push((id, None));
        }
    }
    push_underlays(&mut out, doc, &graphics, opacity);
    out.extend(cells.behind);
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
    out.extend(cells.front);
    let runs = &pass.labels.runs;
    push_atom_labels(&mut out, doc, runs, opacity, &halos);
    push_label_halo(&mut out, doc, runs, opacity, &halos.above(None));
    out.extend(cells.above);
    out.extend(crate::models3d::primitives(raw));
    push_arrows_and_annotations(&mut out, doc);
    for graphic in graphics.iter().filter(|g| g.layer >= 0) {
        out.extend(graphic_primitive(graphic));
        if halos.front.contains(&graphic.id) {
            push_label_halo(&mut out, doc, runs, opacity, &halos.above(Some(graphic.id)));
        }
    }
    out
}

/// Orbitals keep complete lobes. The labels they reach are drawn above the
/// topmost one reaching each, readable inside a paper-colored halo.
fn push_label_halo(
    out: &mut Vec<Primitive>,
    doc: &Document,
    runs: &std::collections::HashMap<u64, Vec<Primitive>>,
    opacity: &crate::rear_opacity::Paint,
    haloed: &[u64],
) {
    let haloed: Vec<_> = haloed.iter().filter_map(|id| doc.atom(*id)).collect();
    let labels: Vec<_> = haloed
        .iter()
        .flat_map(|a| atom_label_parts(doc, runs, opacity, a))
        .collect();
    if labels.is_empty() {
        return;
    }
    // A glyph-shaped halo leaves wide counters (the middle of an O) open;
    // also cover the middle of every glyph's ink box.
    let middles = haloed
        .iter()
        .filter_map(|a| runs.get(&a.id))
        .flat_map(|runs| label_ink_boxes(runs))
        .map(|(lo, hi)| {
            let inset = (hi.x - lo.x).min(hi.y - lo.y) * 0.25;
            let (lo, hi) = (lo.offset(inset, inset), hi.offset(-inset, -inset));
            Primitive::Polygon(vec![lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)])
        });
    out.push(Primitive::Halo {
        radius: crate::scientific::label_halo_radius(),
        primitives: labels.iter().cloned().chain(middles).collect(),
    });
    out.extend(labels);
}

/// Visible labels the orbitals reach; empty without orbitals.
fn orbital_halos(
    doc: &Document,
    graphics: &[&crate::graphics::Graphic],
    labels: &std::collections::HashMap<u64, Vec<Primitive>>,
    opacity: &crate::rear_opacity::Paint,
) -> crate::scientific::OrbitalHalos {
    crate::scientific::orbital_halos(doc, graphics.iter().copied(), |atom| {
        if doc.atom_visible(atom.id) && opacity.atom(atom.id) > 0. {
            labels
                .get(&atom.id)
                .map(|runs| label_ink_boxes(runs))
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    })
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
    opacity: &crate::rear_opacity::Paint,
) {
    out.extend(
        graphics
            .iter()
            .filter(|g| g.layer < 0)
            .flat_map(graphic_primitive),
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
    let sides = crate::crossings::Sides::new(doc);
    for circle in circles.iter().filter(|c| !arcs.intersects(c)) {
        for part in circle.graphic().parts() {
            let stroke = Primitive::Path {
                commands: part.commands,
                style: part.style,
                filled: part.filled,
            };
            let (stroke, gaps) =
                crate::crossings::ring_stroke_with(doc, circle, stroke, ring_opacity(), &sides);
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
    haloed: &crate::scientific::OrbitalHalos,
) {
    for a in doc.atoms.iter().filter(|a| !haloed.contains(a.id)) {
        out.extend(atom_label_parts(doc, labels, opacity, a));
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
/// A visible atom's label runs and attached marks under its rear opacity.
fn atom_label_parts(
    doc: &Document,
    labels: &std::collections::HashMap<u64, Vec<Primitive>>,
    opacity: &crate::rear_opacity::Paint,
    a: &Atom,
) -> Vec<Primitive> {
    if !doc.atom_visible(a.id) {
        return Vec::new();
    }
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
    opacity.atom_parts(a.id, parts)
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
            // The haloed labels follow it; the margin covers the halo.
            Primitive::Halo { .. } => {}
            Primitive::Sphere {
                center,
                radius,
                outline,
                contact,
                ..
            } => {
                let r = radius + outline.map_or(0., |o| o.width / 2.);
                points.extend([center.offset(-r, -r), center.offset(r, r)]);
                // Under perspective a ball's outline reaches a little past
                // its disc.
                if let Some(contact) = contact {
                    let r = outline.map_or(0., |o| o.width / 2.);
                    for p in contact.region.iter().flatten() {
                        points.extend([p.offset(-r, -r), p.offset(r, r)]);
                    }
                }
            }
            Primitive::ShadedQuad { corners, .. } => points.extend(corners),
            Primitive::ShadedPolygon { points: p, .. } => points.extend(p),
            Primitive::Contour { polygons, outline } => {
                let r = outline.width / 2.;
                for p in polygons.iter().flatten() {
                    points.extend([p.offset(-r, -r), p.offset(r, r)]);
                }
            }
            Primitive::Wire {
                from, to, width, ..
            } => {
                let r = width / 2.;
                for p in [from, to] {
                    points.extend([p.offset(-r, -r), p.offset(r, r)]);
                }
            }
            Primitive::Card {
                lo, hi, outline, ..
            } => {
                let r = outline.width / 2.;
                points.extend([lo.offset(-r, -r), hi.offset(r, r)]);
            }
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
/// Part of a scene alone over `lo..hi`, for rasterizing it the way figure
/// export does: the canvas paints 3D models this way.
pub fn svg_part(drawing: &[Primitive], lo: Point, hi: Point) -> String {
    svg::render_part(drawing.to_vec(), lo, hi)
}
/// A figure carries the canvas background when exported or copied.
pub fn svg_with_background(doc: &Document) -> String {
    svg::render_svg(doc, true)
}

#[cfg(test)]
mod charge_placement_tests;
#[cfg(test)]
mod label_attachment_tests;
#[cfg(test)]
mod label_parity_tests;
#[cfg(test)]
mod primitives_parity_tests;
#[cfg(test)]
mod tests;
