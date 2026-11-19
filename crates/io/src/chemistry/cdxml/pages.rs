//! Place independent top-level drawing spaces on one editable canvas.
use super::{Error, Result, numeric, presentation, tree::Tree};
use crate::{
    atom_labels::HydrogenPosition,
    document::{Document, Point},
    typography::TextFormat,
};
use std::collections::HashMap;

pub(super) const NOTICE: &str = "Multiple ChemDraw pages were placed in rows on one drawing, in page order. Page boundaries, paper settings and empty pages are not kept.";

/// Width to height of the arranged pages, about that of a drawing window, so
/// Fit shows a page library as a sheet of pages rather than a thin column.
const SHEET_ASPECT: f64 = 4. / 3.;

// These fields are absolute locations, not dimensions or relative label offsets.
const GEOMETRY: &[(&str, usize)] = &[
    ("p", 2),
    ("BoundingBox", 2),
    ("Head3D", 3),
    ("Tail3D", 3),
    ("Center3D", 3),
    ("MajorAxisEnd3D", 3),
    ("MinorAxisEnd3D", 3),
    ("CurvePoints", 2),
    ("CurvePoints3D", 3),
];

fn native_integer(value: f64) -> Result<f64> {
    let integer = (value * 65536.).trunc();
    if !integer.is_finite() || integer < f64::from(i32::MIN) || integer > f64::from(i32::MAX) {
        return Err(Error::Invalid(
            "Coordinate exceeds the signed 16.16 range".into(),
        ));
    }
    Ok(integer)
}

fn text_bounds(text: &str, format: &TextFormat) -> Option<(Point, Point)> {
    let runs: Vec<_> = crate::typography::layout(text, format)
        .fragments
        .into_iter()
        .map(|fragment| crate::scene::Primitive::Text {
            position: fragment.position,
            text: fragment.text,
            size: fragment.style.size(),
            color: fragment.style.color.rgb(),
            style: fragment.style,
        })
        .collect();
    (!runs.is_empty()).then(|| crate::scene::bounds(&runs))
}

pub(super) fn arrange(xml: &str) -> Result<String> {
    let mut tree = Tree::parse_import(xml)?;
    let pages = tree.children(0, "page")?;
    if pages.len() < 2 {
        return Ok(xml.into());
    }
    let source = presentation::parse(xml).map_err(|error| Error::Invalid(error.to_string()))?;
    let reader = presentation::TextReader::new(source.root_element())
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let synthetic_label = presentation::parse("<t><s>H</s></t>")
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let elements: HashMap<_, _> = tree
        .descendants(0)?
        .into_iter()
        .zip(source.descendants().filter(roxmltree::Node::is_element))
        .collect();
    let source_scale = 1. / (14.4 / 42.);
    let drawing_style = presentation::drawing_style(source.root_element())
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let conformer_scale = drawing_style.bond_length_pt
        / presentation::NativeDrawingStyle::defaults()
            .map_err(|error| Error::Invalid(error.to_string()))?
            .bond_length_pt;
    // Native XYZ bypasses the 2D reader's normalization and carries signed
    // fixed-point integers into the molecular conformer. One XYZ integer step
    // therefore moves this many paper points, rather than 1/65536 of a point.
    let xyz_paper_step = conformer_scale * 28. / source_scale;
    let mut placed = Vec::new();
    for page in pages {
        let nodes = tree.descendants(page)?;
        let defaults: presentation::Attributes =
            tree.node(page)?.attributes.iter().cloned().collect();
        let mut geometry = Vec::new();
        let mut bounds: Option<[f64; 4]> = None;
        let mut native_atoms = Vec::new();
        for &index in &nodes {
            if tree.node(index)?.tag == "fragment" {
                let atoms = tree.children(index, "n")?;
                let mut is_3d = false;
                for &atom in &atoms {
                    is_3d |= tree.node(atom)?.attr("xyz").is_some();
                }
                if is_3d {
                    for atom in atoms {
                        let node = tree.node(atom)?;
                        let explicit = node.attr("xyz");
                        let text = explicit.or_else(|| node.attr("p")).unwrap_or("0 0");
                        let mut values = text
                            .split_whitespace()
                            .map(numeric::float)
                            .collect::<std::result::Result<Vec<_>, _>>()
                            .map_err(|error| Error::Invalid(error.to_string()))?;
                        if values.len() != if explicit.is_some() { 3 } else { 2 } {
                            return Err(Error::Invalid("Invalid native XYZ coordinates".into()));
                        }
                        if explicit.is_none() {
                            // The native parser treats every atom in this
                            // fragment as 3D, including these p-only atoms.
                            // Preserve their original raw integer coordinates
                            // in XYZ before translating their paper projection.
                            values.push(0.);
                        }
                        let [x, y, z] = values.as_slice() else {
                            return Err(Error::Limit);
                        };
                        native_integer(*z)?;
                        native_atoms.push((atom, [native_integer(*x)?, native_integer(*y)?, *z]));
                    }
                }
            }
        }
        for index in nodes {
            let node = tree.node(index)?;
            for &(field, dimensions) in GEOMETRY {
                let Some(text) = node.attr(field) else {
                    continue;
                };
                let values = text
                    .split_whitespace()
                    .map(numeric::float)
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                let valid_count = match field {
                    "BoundingBox" => values.len() == 4,
                    "CurvePoints" | "CurvePoints3D" => {
                        !values.is_empty() && values.len().is_multiple_of(dimensions)
                    }
                    _ => values.len() == dimensions,
                };
                if !valid_count || values.iter().any(|value| !value.is_finite()) {
                    return Err(Error::Invalid(format!("Invalid page {field} coordinates")));
                }
                // An arrow's arc centre and axis ends are construction points
                // that ChemDraw can leave far from the arrow; its box covers
                // what it draws.
                let drawn = node.tag != "arrow"
                    || node.attr("BoundingBox").is_none()
                    || !matches!(field, "Center3D" | "MajorAxisEnd3D" | "MinorAxisEnd3D");
                for point in values.chunks_exact(dimensions).filter(|_| drawn) {
                    let x = *point.first().ok_or(Error::Limit)?;
                    let y = *point.get(1).ok_or(Error::Limit)?;
                    let b = bounds.get_or_insert([x, y, x, y]);
                    b[0] = b[0].min(x);
                    b[1] = b[1].min(y);
                    b[2] = b[2].max(x);
                    b[3] = b[3].max(y);
                    if field == "p" && node.tag == "n" {
                        let element = *elements.get(&index).ok_or(Error::Limit)?;
                        let label = element.children().find(|child| child.has_tag_name("t"));
                        let attributes = node.attributes.iter().cloned().collect();
                        let (text, format) = reader
                            .read(
                                label.unwrap_or(synthetic_label.root_element()),
                                Some(&attributes),
                                true,
                            )
                            .and_then(|text| text.into_document())
                            .map_err(|error| Error::Invalid(error.to_string()))?;
                        let mut style = format.style.clone();
                        style.script = crate::typography::Script::Normal;
                        style.formula = false;
                        let symbol = node
                            .attr("Element")
                            .and_then(|value| value.parse::<usize>().ok())
                            .and_then(|number| crate::chemistry::ELEMENTS.get(number))
                            .map_or("C", |element| element.symbol);
                        // Rendering this isolated preview requires no chemical
                        // preparation or recursive import. Until the real graph
                        // supplies implicit H and neighbor-dependent placement,
                        // union the renderer's supported appendage orientations.
                        let mut preview = Document::default();
                        preview.add_atom(symbol, Point::default());
                        let atom = preview.atoms.first_mut().ok_or(Error::Limit)?;
                        atom.text_style = Some(style);
                        atom.label_h = node
                            .attr("NumHydrogens")
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(4);
                        atom.isotope = node
                            .attr("Isotope")
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(0);
                        atom.charge = node
                            .attr("Charge")
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(0);
                        for side in [
                            HydrogenPosition::Left,
                            HydrogenPosition::Right,
                            HydrogenPosition::Above,
                            HydrogenPosition::Below,
                        ] {
                            preview
                                .atoms
                                .first_mut()
                                .ok_or(Error::Limit)?
                                .display
                                .hydrogen_position = side;
                            let (lo, hi) =
                                crate::scene::bounds(&crate::scene::primitives(&preview));
                            b[0] = b[0].min(x + f64::from(lo.x) / source_scale);
                            b[1] = b[1].min(y + f64::from(lo.y) / source_scale);
                            b[2] = b[2].max(x + f64::from(hi.x) / source_scale);
                            b[3] = b[3].max(y + f64::from(hi.y) / source_scale);
                        }
                        // Generic and contracted labels can be wider than an
                        // element/H label. Their literal text has the same
                        // resolved typography but can anchor on either side.
                        if label.is_some()
                            && let Some((lo, hi)) = text_bounds(&text, &format)
                        {
                            let width = f64::from(lo.x.abs().max(hi.x.abs())) / source_scale;
                            let height = f64::from(lo.y.abs().max(hi.y.abs())) / source_scale;
                            b[0] = b[0].min(x - width);
                            b[1] = b[1].min(y - height);
                            b[2] = b[2].max(x + width);
                            b[3] = b[3].max(y + height);
                        }
                    }
                    // A caption's box can be narrower than its text, as
                    // when an upright copy replaces a rotated caption.
                    if field == "p"
                        && node.tag == "t"
                        && !node
                            .parent
                            .map(|parent| tree.node(parent))
                            .transpose()?
                            .is_some_and(|parent| matches!(parent.tag.as_str(), "n" | "objecttag"))
                    {
                        let element = *elements.get(&index).ok_or(Error::Limit)?;
                        let (text, format) = reader
                            .read(element, Some(&defaults), false)
                            .and_then(|text| text.into_document())
                            .map_err(|error| Error::Invalid(error.to_string()))?;
                        if let Some((lo, hi)) = text_bounds(&text, &format) {
                            b[0] = b[0].min(x + f64::from(lo.x) / source_scale);
                            b[1] = b[1].min(y + f64::from(lo.y) / source_scale);
                            b[2] = b[2].max(x + f64::from(hi.x) / source_scale);
                            b[3] = b[3].max(y + f64::from(hi.y) / source_scale);
                        }
                    }
                }
                geometry.push((index, field, dimensions, values));
            }
        }
        if let Some(bounds) = bounds {
            placed.push((bounds, native_atoms, geometry));
        }
    }
    // Rows of pages in page order, left to right, then down, about as wide
    // as a window. Each column is as wide as its widest page and each row
    // starts below everything above it.
    let gap = f64::from(crate::pages::GAP_PT);
    let largest = |extent: fn(&[f64; 4]) -> f64| {
        placed
            .iter()
            .map(|(bounds, ..)| extent(bounds))
            .fold(0., f64::max)
            + gap
    };
    let width = largest(|&[left, _, right, _]| right - left);
    let height = largest(|&[_, top, _, bottom]| bottom - top);
    let columns = ((placed.len() as f64 * height / width * SHEET_ASPECT).sqrt() as usize)
        .clamp(1, placed.len().max(1));
    let mut starts = vec![0_f64; columns];
    for (slot, ([left, _, right, _], ..)) in placed.iter().enumerate() {
        if let Some(start) = starts.get_mut(slot % columns + 1) {
            *start = start.max(right - left + gap);
        }
    }
    let mut column_left = 0.;
    for start in &mut starts {
        column_left += *start;
        *start = column_left;
    }
    let (mut next_left, mut row_top, mut lowest) = (0., 0., 0_f64);
    for (slot, ([left, top, right, bottom], native_atoms, geometry)) in
        placed.into_iter().enumerate()
    {
        if slot > 0 && slot % columns == 0 {
            (next_left, row_top) = (0., lowest + gap);
        }
        // A page snapped to whole XYZ steps can reach past its column.
        let start = starts
            .get(slot % columns)
            .map_or(next_left, |s| s.max(next_left));
        let (mut dx, mut dy) = (start - left, row_top - top);
        let (kx, ky) = if native_atoms.is_empty() {
            (0., 0.)
        } else {
            let steps = ((dx / xyz_paper_step).ceil(), (dy / xyz_paper_step).ceil());
            dx = steps.0 * xyz_paper_step;
            dy = steps.1 * xyz_paper_step;
            steps
        };
        next_left = right + dx + gap;
        lowest = lowest.max(bottom + dy);
        for (atom, [ax, ay, az]) in native_atoms {
            let (ax, ay) = ((ax + kx) / 65536., (ay + ky) / 65536.);
            native_integer(ax)?;
            native_integer(ay)?;
            tree.node_mut(atom)?.set("xyz", format!("{ax} {ay} {az}"));
        }
        for (index, field, dimensions, mut values) in geometry {
            for point in values.chunks_exact_mut(dimensions) {
                let x = point.first_mut().ok_or(Error::Limit)?;
                *x += dx;
                let y = point.get_mut(1).ok_or(Error::Limit)?;
                *y += dy;
                // CDX coordinates are signed 16.16, including in binary CDS.
                if point.iter().any(|value| {
                    !value.is_finite()
                        || *value < f64::from(i32::MIN) / 65536.
                        || *value > f64::from(i32::MAX) / 65536.
                }) {
                    return Err(Error::Invalid(
                        "Arranged ChemDraw pages exceed the supported coordinate range".into(),
                    ));
                }
            }
            tree.node_mut(index)?.set(
                field,
                values
                    .iter()
                    .map(f64::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
    }
    tree.serialize()
}
