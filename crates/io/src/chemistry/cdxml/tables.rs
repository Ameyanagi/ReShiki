//! Native table pages own ordinary editable objects. The chemical readers use
//! one world-coordinate snapshot; the retained page nodes supply ownership.
use super::{Error, ObjectMapEntry, Result, graphics, numeric, presentation, tree::Tree};
use crate::{
    document::Point,
    graphics::{
        Graphic, GraphicKind, GraphicStyle, LinePattern,
        table::{Border, Cell, Table},
    },
};
use roxmltree::Node;
use std::collections::{HashMap, HashSet};

/// Absolute paper coordinates. Dimensions, label offsets and relative text
/// positions deliberately do not appear here.
pub(crate) const GEOMETRY: &[(&str, usize)] = &[
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

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

fn float(text: &str) -> Result<f64> {
    numeric::float(text).map_err(|e| invalid(e.to_string()))
}
fn integer(text: &str) -> Result<Option<i128>> {
    numeric::integer(text).map_err(|e| invalid(e.to_string()))
}

fn rectangle(value: &str) -> Result<[f64; 4]> {
    let v = value
        .split_whitespace()
        .map(float)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let [left, top, right, bottom] = v.as_slice() else {
        return Err(invalid("Invalid table cell bounds"));
    };
    if v.iter().any(|v| !v.is_finite()) || right <= left || bottom <= top {
        return Err(invalid("A table cell needs positive finite bounds"));
    }
    Ok([*left, *top, *right, *bottom])
}

pub(super) fn cell_count(tree: &Tree) -> Result<usize> {
    let mut count = 0;
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag == "table" {
            count += tree.children(index, "page")?.len();
        }
    }
    Ok(count)
}

/// Move only a cell's contents. BoundsInParent and the cell's own BoundingBox
/// are already in its parent page, while all child coordinates are local.
pub(super) fn normalize(tree: &mut Tree) -> Result<()> {
    let order = tree.descendants(0)?;
    for &index in &order {
        let node = tree.node(index)?;
        if node.tag == "border" {
            let parent = node.parent.and_then(|p| tree.node(p).ok());
            let table = parent
                .and_then(|p| p.parent)
                .and_then(|p| tree.node(p).ok());
            if !parent.is_some_and(|p| p.tag == "page") || !table.is_some_and(|t| t.tag == "table")
            {
                return Err(invalid("A table border must belong to a cell page"));
            }
        }
        if node.tag != "table" {
            continue;
        }
        if float(node.attr("RotationAngle").unwrap_or("0"))? != 0. {
            return Err(invalid("Native rotated tables are not supported"));
        }
        let mut parent = node.parent;
        while let Some(p) = parent {
            let ancestor = tree.node(p)?;
            if ancestor.tag == "table" {
                return Err(invalid("Nested tables are not supported"));
            }
            parent = ancestor.parent;
        }
        let cells = tree.children(index, "page")?;
        if cells.is_empty()
            || cells.len() > crate::graphics::table::MAX_CELLS
            || cells.len() != tree.node(index)?.children.len()
        {
            return Err(invalid("A native table needs 1–4096 cell pages"));
        }
        for cell in cells {
            let page = tree.node(cell)?;
            let bounds = rectangle(
                page.attr("BoundsInParent")
                    .or_else(|| page.attr("BoundingBox"))
                    .ok_or_else(|| invalid("Missing table cell bounds"))?,
            )?;
            let [x, y, _, _] = bounds;
            // Resolve the fallback now, so page arrangement translates the
            // same authoritative rectangle as the table reconstruction.
            tree.node_mut(cell)?.set(
                "BoundsInParent",
                bounds
                    .iter()
                    .map(f64::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            for child in tree.descendants(cell)?.into_iter().skip(1) {
                let node = tree.node(child)?;
                if matches!(node.tag.as_str(), "page" | "table") {
                    return Err(invalid("Nested table pages are not supported"));
                }
                if node.attr("xyz").is_some() {
                    return Err(invalid(
                        "Native table cells with XYZ coordinates are not supported",
                    ));
                }
                // The molecular reader defaults an omitted p to (0,0).
                // In a native cell that means the cell's origin, not the
                // document origin. Standalone text has the same fallback.
                let default_position = node.attr("p").is_none()
                    && (node.tag == "n"
                        || node.tag == "t"
                            && node.attr("BoundingBox").is_none()
                            && node
                                .parent
                                .and_then(|p| tree.node(p).ok())
                                .is_none_or(|p| !matches!(p.tag.as_str(), "n" | "objecttag")));
                if default_position {
                    tree.node_mut(child)?.set("p", "0 0".into());
                }
                let node = tree.node(child)?;
                let fields = GEOMETRY
                    .iter()
                    .filter_map(|&(key, dimensions)| {
                        node.attr(key).map(|v| (key, dimensions, v.to_owned()))
                    })
                    .collect::<Vec<_>>();
                for (key, dimensions, text) in fields {
                    tree.node_mut(child)?
                        .set(key, translate(&text, key, dimensions, x, y)?);
                }
                if let Some(record) = tree.node(child)?.attr(crate::exchange::PANEL) {
                    let record = translate_panel(record, x, y)?;
                    tree.node_mut(child)?.set(crate::exchange::PANEL, record);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn translate(
    text: &str,
    key: &str,
    dimensions: usize,
    dx: f64,
    dy: f64,
) -> Result<String> {
    let mut values = text
        .split_whitespace()
        .map(float)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let valid = match key {
        "BoundingBox" => values.len() == 4,
        "CurvePoints" | "CurvePoints3D" => {
            !values.is_empty() && values.len().is_multiple_of(dimensions)
        }
        _ => values.len() == dimensions,
    };
    if !valid || values.iter().any(|v| !v.is_finite()) {
        return Err(invalid(format!("Invalid table {key} coordinates")));
    }
    for point in values.chunks_exact_mut(dimensions) {
        *point.first_mut().ok_or(Error::Limit)? += dx;
        *point.get_mut(1).ok_or(Error::Limit)? += dy;
        if point.iter().any(|v| {
            !v.is_finite() || *v < f64::from(i32::MIN) / 65536. || *v > f64::from(i32::MAX) / 65536.
        }) {
            return Err(invalid("Table contents exceed the native coordinate range"));
        }
    }
    Ok(values
        .iter()
        .map(f64::to_string)
        .collect::<Vec<_>>()
        .join(" "))
}

pub(crate) fn translate_panel(record: &str, dx: f64, dy: f64) -> Result<String> {
    // Invalid extension data has always fallen back to its visible curves.
    let Ok(mut graphic) = serde_json::from_str::<Graphic>(record) else {
        return Ok(record.into());
    };
    graphic.origin = graphic.origin.offset(dx as f32, dy as f32);
    serde_json::to_string(&graphic).map_err(|e| invalid(e.to_string()))
}

pub(super) struct ReadTables {
    pub graphics: Vec<Graphic>,
    pub bindings: Vec<(usize, u64)>,
}

/// Bind cells through the already checked source object map, never by where
/// an atom or label happens to lie. Overflow therefore remains in its cell.
pub(super) fn read(
    root: Node<'_, '_>,
    scale: f64,
    mappings: &[ObjectMapEntry],
    first_id: u64,
) -> Result<ReadTables> {
    let nodes: Vec<_> = root.descendants().filter(Node::is_element).collect();
    let ordinals: HashMap<_, _> = nodes.iter().enumerate().map(|(i, n)| (n.id(), i)).collect();
    let mapped: HashMap<_, _> = mappings
        .iter()
        .map(|m| (m.source, m.atoms.as_slice()))
        .collect();
    let colors = presentation::palette(root).map_err(|e| invalid(e.to_string()))?;
    // Cell Z values belong to that cell's own plane; they must not move the
    // whole table behind an unrelated outside object.
    let top_layer = nodes
        .iter()
        .filter(|n| {
            graphics::table_cell(**n).is_none()
                && n.ancestors().all(|p| p.attribute("Visible") != Some("no"))
                && (matches!(n.tag_name().name(), "fragment" | "n" | "b" | "t" | "arrow")
                    || n.has_tag_name("curve")
                        && ["ArrowheadHead", "ArrowheadTail"].iter().any(|key| {
                            !matches!(n.attribute(*key).unwrap_or("None"), "None" | "Unspecified")
                        }))
                && !graphics::bare_stroke(**n)
        })
        .filter_map(|n| n.attribute("Z"))
        .map(graphics::NativeLayer::parse)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| invalid(e.to_string()))?
        .into_iter()
        .max()
        .unwrap_or(graphics::NativeLayer::parse("32767").map_err(|e| invalid(e.to_string()))?);
    let mut result = ReadTables {
        graphics: vec![],
        bindings: vec![],
    };
    for node in nodes.iter().filter(|n| n.has_tag_name("table")) {
        let source = *ordinals.get(&node.id()).ok_or(Error::Limit)?;
        if mapped.get(&source).is_some_and(|ids| ids.is_empty()) {
            continue;
        }
        let pages: Vec<_> = node.children().filter(|p| p.has_tag_name("page")).collect();
        let boxes = pages
            .iter()
            .map(|p| {
                rectangle(
                    p.attribute("BoundsInParent")
                        .ok_or_else(|| invalid("Missing table cell bounds"))?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut bounds = *boxes
            .first()
            .ok_or_else(|| invalid("A table needs cells"))?;
        for b in &boxes {
            bounds = [
                bounds[0].min(b[0]),
                bounds[1].min(b[1]),
                bounds[2].max(b[2]),
                bounds[3].max(b[3]),
            ];
        }
        let [left, top, right, bottom] = bounds;
        let mut cells = Vec::new();
        for (index, (page, [x, y, r, b])) in pages.iter().zip(boxes).enumerate() {
            let mut members = Vec::new();
            let mut seen = HashSet::new();
            for child in page.descendants().filter(Node::is_element) {
                if let Some(ids) = ordinals
                    .get(&child.id())
                    .and_then(|ordinal| mapped.get(ordinal))
                {
                    for &id in *ids {
                        if seen.insert(id) {
                            members.push(id);
                        }
                    }
                }
            }
            let mut borders = Vec::new();
            for border in page.children().filter(|p| p.has_tag_name("border")) {
                let side = match border.attribute("Side") {
                    Some("left") => crate::graphics::table::Side::Left,
                    Some("top") => crate::graphics::table::Side::Top,
                    Some("bottom") => crate::graphics::table::Side::Bottom,
                    Some("right") => crate::graphics::table::Side::Right,
                    _ => return Err(invalid("Invalid table border side")),
                };
                let width = float(
                    border
                        .attribute("LineWidth")
                        .or_else(|| root.attribute("LineWidth"))
                        .unwrap_or("0.6"),
                )?;
                let color = border
                    .attribute("color")
                    .or_else(|| page.attribute("color"))
                    .or_else(|| node.attribute("color"))
                    .or_else(|| root.attribute("color"))
                    .unwrap_or("3");
                let color = integer(color)?
                    .and_then(|v| usize::try_from(v).ok())
                    .and_then(|i| colors.colors.get(i))
                    .copied()
                    .ok_or_else(|| invalid("Invalid table border color"))?
                    .into_color()
                    .map_err(|e| invalid(e.to_string()))?;
                let pattern = match border.attribute("LineType").unwrap_or("Solid") {
                    "Solid" => LinePattern::Solid,
                    "Dashed" => LinePattern::Dashed,
                    _ => return Err(invalid("Unsupported table border pattern")),
                };
                borders.push(Border {
                    side,
                    width_pt: width as f32,
                    color,
                    pattern,
                });
            }
            cells.push(Cell {
                id: u32::try_from(index + 1).map_err(|_| Error::Limit)?,
                lo: Point::new(
                    ((x - left) / (right - left)) as f32,
                    ((y - top) / (bottom - top)) as f32,
                ),
                hi: Point::new(
                    ((r - left) / (right - left)) as f32,
                    ((b - top) / (bottom - top)) as f32,
                ),
                members,
                borders,
            });
        }
        let table = Table { cells };
        table.validate().map_err(invalid)?;
        let id = first_id
            .checked_add(result.graphics.len() as u64)
            .ok_or(Error::Limit)?;
        let mut graphic = Graphic::dragged(
            id,
            GraphicKind::Table,
            Point::new((left * scale) as f32, (top * scale) as f32),
            Point::new((right * scale) as f32, (bottom * scale) as f32),
            GraphicStyle::default(),
            Default::default(),
            false,
        );
        // Keep sub-point frames exactly; Graphic::dragged has a 1-unit UI floor.
        graphic.axis_x = Point::new(((right - left) * scale) as f32, 0.);
        graphic.axis_y = Point::new(0., ((bottom - top) * scale) as f32);
        graphic.table = Some(Box::new(table));
        graphic.layer = graphics::NativeLayer::parse(node.attribute("Z").unwrap_or("0"))
            .and_then(|z| z.relative(&top_layer).into_document())
            .map_err(|e| invalid(e.to_string()))?;
        graphic.validate().map_err(invalid)?;
        result.graphics.push(graphic);
        result.bindings.push((source, id));
    }
    Ok(result)
}
