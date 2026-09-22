//! Restore drawing bond appearance using the original coordinate association.
//!
//! Source atom IDs cannot safely identify the native graph after partial imports
//! or reordering. Every direct fragment node must instead match exactly one atom
//! of the same element within the original strict 0.02 world-unit tolerance.
use super::{
    Fragment,
    numeric::{self, integer, quoted},
};
use crate::{bonds::DoublePosition, document::Bond};
use roxmltree::Node;
use std::collections::{HashMap, HashSet};

const MAX_ATOMS: usize = 100_000;
const MAX_BONDS: usize = 300_000;
const MATCH_BUDGET: usize = 10_000_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not associate a CDXML molecular fragment")]
    Fragment,
    #[error("Missing CDXML atom position")]
    Position,
    #[error("Could not safely associate CDXML bond appearance with its atoms")]
    Association,
    #[error("This CDXML bond order is not supported yet")]
    Order,
    #[error("Invalid CDXML bond color")]
    Color,
    #[error("Invalid CDXML double-bond position")]
    DoublePosition,
    #[error("Could not parse every CDXML molecular fragment; import was cancelled")]
    PartialImport,
    #[error("{0}")]
    Display(String),
    #[error("{0}")]
    Endpoint(String),
    #[error("invalid literal for int() with base 10: {0}")]
    Integer(String),
    #[error(
        "Exceeds the limit (4300 digits) for integer string conversion: value has {0} digits; use sys.set_int_max_str_digits() to increase the limit"
    )]
    IntegerDigits(usize),
    #[error("could not convert string to float: {0}")]
    Float(String),
    #[error("CDXML bond layer is outside the signed 16-bit document range")]
    Layer,
    #[error("Invalid CDXML bond appearance: {0}")]
    Invalid(String),
    #[error("CDXML bond appearance exceeds the size, nesting, or matching work limit")]
    Limit,
}
type Result<T> = std::result::Result<T, Error>;
impl From<numeric::Error> for Error {
    fn from(error: numeric::Error) -> Self {
        match error {
            numeric::Error::Integer(value) => Self::Integer(value),
            numeric::Error::IntegerDigits(count) => Self::IntegerDigits(count),
            numeric::Error::Float(value) => Self::Float(value),
        }
    }
}

fn named(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}

fn position(node: Node<'_, '_>) -> Result<[f64; 2]> {
    let mut xy = [0.0; 2];
    let mut count = 0;
    for text in node.attribute("p").unwrap_or("").split_whitespace() {
        let value = numeric::float(text)?;
        if let Some(slot) = xy.get_mut(count) {
            *slot = value;
        }
        count += 1;
    }
    if count != 2 {
        return Err(Error::Position);
    }
    Ok(xy)
}

fn display(text: &str) -> Result<String> {
    Ok(match text {
        "Solid" => "plain",
        "Dash" => "dashed",
        "Dot" => "dotted",
        "Bold" => "bold",
        "Hash" => "hashed",
        "WedgeBegin" | "WedgeEnd" => "wedge",
        "WedgedHashBegin" | "WedgedHashEnd" => "hash",
        "HollowWedgeBegin" | "HollowWedgeEnd" => "hollow_wedge",
        "Wavy" => "wavy",
        _ => return Err(Error::Display(quoted(text))),
    }
    .into())
}

struct Point {
    x: f64,
    y: f64,
    id: u64,
}
type Cells = HashMap<(u8, i64, i64), Vec<Point>>;

fn cells(fragment: &Fragment, offset: usize) -> Result<Cells> {
    if fragment.positions.len() != fragment.graph.atoms.len() {
        return Err(Error::Invalid(
            "Fragment position dimensions changed".into(),
        ));
    }
    let mut cells = Cells::new();
    for (index, (atom, p)) in fragment
        .graph
        .atoms
        .iter()
        .zip(&fragment.positions)
        .enumerate()
    {
        let x = p.x * 28.0;
        let y = -p.y * 28.0;
        // Native comparisons involving nonfinite positions cannot match.
        if x.is_finite() && y.is_finite() {
            cells
                .entry((atom.atomic_number, x.floor() as i64, y.floor() as i64))
                .or_default()
                .push(Point {
                    x,
                    y,
                    id: u64::try_from(offset + index + 1).map_err(|_| Error::Limit)?,
                });
        }
    }
    Ok(cells)
}

fn associate(
    cells: &Cells,
    element: Option<i128>,
    xy: [f64; 2],
    budget: &mut usize,
) -> Result<u64> {
    let element = element
        .and_then(|n| u8::try_from(n).ok())
        .ok_or(Error::Association)?;
    let [x, y] = xy;
    if !x.is_finite() || !y.is_finite() {
        return Err(Error::Association);
    }
    let mut matched = None;
    // Unit cells avoid additional division rounding. Strictly matching points
    // are in the same cell or an immediate neighbor. Saturating float-to-int
    // conversion is harmless at huge coordinates; the work budget bounds such
    // collisions without weakening the final exact floating-point comparison.
    let cx = x.floor() as i64;
    let cy = y.floor() as i64;
    for dx in -1..=1 {
        for dy in -1..=1 {
            let Some((cx, cy)) = cx.checked_add(dx).zip(cy.checked_add(dy)) else {
                continue;
            };
            for p in cells.get(&(element, cx, cy)).into_iter().flatten() {
                *budget = budget.checked_sub(1).ok_or(Error::Limit)?;
                if (p.x - x).abs() < 0.02 && (p.y - y).abs() < 0.02 {
                    if matched.is_some() {
                        return Err(Error::Association);
                    }
                    matched = Some(p.id);
                }
            }
        }
    }
    matched.ok_or(Error::Association)
}

fn inherited<'a, 'input: 'a>(node: Node<'a, 'input>, name: &str) -> Option<&'a str> {
    node.ancestors().find_map(|node| node.attribute(name))
}

/// Preserve native appearance and dense IDs in the supplied fragment order.
/// Every error leaves the XML and molecular fragments unchanged. The signed
/// 16-bit layer limit is the editable document's bound; the Python helper alone
/// accepts wider integers before the original response reaches that boundary.
pub fn read(text: &str, parts: &[Fragment], scale: f64, colors: &[[u8; 3]]) -> Result<Vec<Bond>> {
    if text.len() > 16 * 1024 * 1024 || parts.len() > MAX_ATOMS {
        return Err(Error::Limit);
    }
    if super::xml_guard::has_entity_declaration(text) {
        return Err(Error::Invalid("DTD entities are unsupported".into()));
    }
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 400_000,
        },
    )
    .map_err(|error| Error::Invalid(error.to_string()))?;
    let mut fragments = HashMap::new();
    let mut count = 0;
    let mut properties = 0;
    for node in document.descendants().filter(Node::is_element) {
        count += 1;
        properties += node.attributes().len();
        if count > 100_000 || properties > 1_000_000 || node.ancestors().take(67).count() > 66 {
            return Err(Error::Limit);
        }
        if named(node, "fragment") {
            // Exactly like the original dict: spelling matters, and duplicate
            // source IDs retain only their last element.
            fragments.insert(node.attribute("id"), node);
        }
    }
    let mut result = Vec::new();
    let mut offset = 0usize;
    let mut parsed = HashSet::new();
    let mut budget = MATCH_BUDGET;
    for part in parts {
        let next = offset
            .checked_add(part.graph.atoms.len())
            .filter(|n| *n <= MAX_ATOMS)
            .ok_or(Error::Limit)?;
        let id = part.id.to_string();
        let fragment = fragments.get(&Some(id.as_str())).ok_or(Error::Fragment)?;
        parsed.insert(fragment.id());
        let cells = cells(part, offset)?;
        let mut nodes = HashMap::new();
        for node in fragment.children().filter(|n| named(*n, "n")) {
            let [x, y] = position(node)?;
            // With no native atoms, the original loop never parses Element.
            if part.graph.atoms.is_empty() {
                return Err(Error::Association);
            }
            let element = integer(node.attribute("Element").unwrap_or("6"))?;
            let id = associate(&cells, element, [x * scale, y * scale], &mut budget)?;
            nodes.insert(node.attribute("id"), id);
        }
        for node in fragment.children().filter(|n| named(*n, "b")) {
            if result.len() >= MAX_BONDS {
                return Err(Error::Limit);
            }
            let mut order = match node.attribute("Order").unwrap_or("1") {
                "1" => 1,
                "2" => 2,
                "3" => 3,
                "1.5" => 4,
                "hydrogen" => 0,
                "dative" => 5,
                "4" => 6,
                _ => return Err(Error::Order),
            };
            let mut primary = node.attribute("Display").unwrap_or("Solid");
            let secondary = node.attribute("Display2");
            if order == 4
                && (node.attribute("Display") == Some("Dash") || secondary == Some("Dash"))
            {
                order = 7;
            }
            if secondary == Some("DottedHydrogen") {
                order = 0;
            } else if order == 1 && primary == "Dash" {
                order = 5;
            }
            if order == 0 {
                primary = "Dot";
            }
            let color = integer(inherited(node, "color").unwrap_or("3"))?
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| colors.get(n).copied())
                .ok_or(Error::Color)?;
            let mut double_position = match node
                .attribute("DoublePosition")
                .unwrap_or("auto")
                .to_lowercase()
                .as_str()
            {
                "auto" => DoublePosition::Auto,
                "left" => DoublePosition::Left,
                "right" => DoublePosition::Right,
                "center" => DoublePosition::Center,
                _ => return Err(Error::DoublePosition),
            };
            let endpoint = |name| {
                let id = node.attribute(name);
                nodes
                    .get(&id)
                    .copied()
                    .ok_or_else(|| Error::Endpoint(id.map(quoted).unwrap_or_else(|| "None".into())))
            };
            let (mut a, mut b) = (endpoint("B")?, endpoint("E")?);
            if primary.ends_with("End") {
                std::mem::swap(&mut a, &mut b);
                double_position = double_position.reversed();
            }
            let secondary_display = if order == 0 {
                None
            } else if order == 2 && primary == "Bold" && secondary.is_none_or(str::is_empty) {
                Some("plain".into())
            } else {
                secondary
                    .filter(|s| !s.is_empty() && *s != primary)
                    .map(display)
                    .transpose()?
            };
            let display = display(primary)?;
            let z_order = integer(inherited(node, "Z").unwrap_or("0"))?
                .and_then(|n| i16::try_from(n).ok())
                .ok_or(Error::Layer)?;
            result.push(Bond {
                a,
                b,
                order,
                display,
                z_order,
                secondary_display,
                double_position,
                color,
                indicator: Default::default(),
                cip_label: None,
                stereo: None,
                stereo_atoms: Vec::new(),
            });
        }
        offset = next;
    }
    if fragments.values().any(|fragment| {
        !parsed.contains(&fragment.id()) && fragment.children().any(|n| named(n, "n"))
    }) {
        return Err(Error::PartialImport);
    }
    Ok(result)
}
