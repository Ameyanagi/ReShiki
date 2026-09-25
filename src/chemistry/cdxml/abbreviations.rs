//! Explicit chemical abbreviation expansion, ported from
//! engine/abbreviations_exchange.py::flatten. All edits are to a detached tree.
use super::{Error, Result, tree::Tree};
use serde::Serialize;
use std::{borrow::Cow, collections::HashMap};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Abbreviation {
    pub label: String,
    pub reverse_label: String,
    /// Source CDXML identifier, before any application document ID mapping.
    /// None preserves the original reader's supported absent-ID case.
    pub anchor: Option<String>,
    pub members: Vec<Option<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Flattened {
    pub xml: String,
    pub abbreviations: Vec<Abbreviation>,
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn wrapper(tree: &Tree, index: usize) -> Result<bool> {
    let node = tree.node(index)?;
    Ok(node.tag == "n" && matches!(node.attr("NodeType"), Some("Fragment" | "Nickname")))
}
pub(super) fn position(tree: &Tree, index: usize, error: &str) -> Result<[f64; 2]> {
    let mut values = tree.node(index)?.attr("p").unwrap_or("").split_whitespace();
    let mut next = || {
        values
            .next()
            .and_then(python_float)
            .ok_or_else(|| invalid(error))
    };
    let x = next()?;
    let y = next()?;
    if values.next().is_some() {
        return Err(invalid(error));
    }
    Ok([x, y])
}
fn python_float(text: &str) -> Option<f64> {
    python_number(text)?.parse().ok()
}
pub(super) fn python_number(text: &str) -> Option<Cow<'_, str>> {
    // Decimal zero codepoints in Unicode 15.0, as used by the pinned Python
    // 3.12 unicodedata database. Python float() accepts every Nd decimal digit.
    const ZEROES: &[u32] = &[
        0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
        0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
        0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0,
        0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0,
        0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x11f50, 0x16a60,
        0x16ac0, 0x16b50, 0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e4f0,
        0x1e950, 0x1fbf0,
    ];
    let normalized = if text.is_ascii() {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(
            text.chars()
                .map(|c| {
                    if c.is_ascii() {
                        return Some(c);
                    }
                    let code = u32::from(c);
                    ZEROES
                        .iter()
                        .find_map(|&zero| code.checked_sub(zero).filter(|&digit| digit < 10))
                        .and_then(|digit| char::from_u32(u32::from('0') + digit))
                })
                .collect::<Option<String>>()?,
        )
    };
    let text = normalized.as_ref();
    // Python allows a single underscore between decimal digits.
    if !text.contains('_') {
        return Some(normalized);
    }
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_'
            && !(i
                .checked_sub(1)
                .and_then(|j| bytes.get(j))
                .is_some_and(u8::is_ascii_digit)
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    Some(Cow::Owned(text.replace('_', "")))
}
fn touches(tree: &Tree, bond: usize, id: Option<&str>) -> Result<bool> {
    let node = tree.node(bond)?;
    Ok(node.attr("B") == id || node.attr("E") == id)
}
fn attached(tree: &Tree, parent: usize, id: Option<&str>) -> Result<Vec<usize>> {
    tree.children(parent, "b")?
        .into_iter()
        .filter_map(|index| match touches(tree, index, id) {
            Ok(true) => Some(Ok(index)),
            Ok(false) => None,
            Err(e) => Some(Err(e)),
        })
        .collect()
}

/// Expand explicit Fragment/Nickname definitions without changing the input.
///
/// Source IDs, chemistry, XML text spans, and attachment geometry are retained.
/// Absent IDs remain absent; duplicate IDs or discarded nested chemistry fail
/// instead of dropping atoms.
/// The result is still drawing XML: bond-display normalization and molecular
/// parsing are separate stages. External DTDs are ignored; internal DTD
/// declarations and namespaces are rejected. Input, output, tree depth, and
/// transformation work are bounded.
pub fn flatten_abbreviations(text: &str) -> Result<Flattened> {
    flatten_tree(Tree::parse(text)?)
}
pub(super) fn flatten_tree(mut tree: Tree) -> Result<Flattened> {
    let source = tree.descendants(0)?;
    let before = chemical_count(&tree, &source)?;
    let wrappers = source
        .into_iter()
        .filter_map(|index| match wrapper(&tree, index) {
            Ok(true) => Some(Ok(index)),
            Ok(false) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<Vec<_>>>()?;
    let mut abbreviations = Vec::new();
    let mut removed = (0usize, 0usize);
    for outer in wrappers {
        let expanded = flatten_one(&mut tree, outer)?;
        abbreviations.push(expanded.abbreviation);
        removed.0 += expanded.atoms;
        removed.1 += expanded.bonds;
    }
    let after = chemical_count(&tree, &tree.descendants(0)?)?;
    if (after.0 + removed.0, after.1 + removed.1) != before {
        return Err(Error::Unsupported("discarded abbreviation chemistry"));
    }
    Ok(Flattened {
        xml: tree.serialize()?,
        abbreviations,
    })
}

fn chemical_count(tree: &Tree, nodes: &[usize]) -> Result<(usize, usize)> {
    let mut counts = (0usize, 0usize);
    for &node in nodes {
        match tree.node(node)?.tag.as_str() {
            "n" => counts.0 += 1,
            "b" => counts.1 += 1,
            _ => (),
        }
    }
    Ok(counts)
}
struct Expanded {
    abbreviation: Abbreviation,
    atoms: usize,
    bonds: usize,
}
fn flatten_one(tree: &mut Tree, outer: usize) -> Result<Expanded> {
    // Python snapshots wrappers before editing, then rebuilds parents from the
    // surviving root for each one. A wrapper discarded by an earlier expansion
    // must therefore fail, even though its arena allocation still exists.
    let mut ancestor = outer;
    while ancestor != 0 {
        ancestor = tree
            .node(ancestor)?
            .parent
            .ok_or_else(|| invalid("Abbreviation outside a molecular fragment"))?;
    }
    let fragments = tree.children(outer, "fragment")?;
    let text = tree.children(outer, "t")?.first().copied();
    let (&inner, text) = fragments
        .first()
        .zip(text)
        .filter(|_| fragments.len() == 1)
        .ok_or_else(|| invalid("An abbreviation needs its explicit chemical definition"))?;
    let descendants = tree.descendants(inner)?;
    for &index in &descendants {
        if wrapper(tree, index)? {
            return Err(invalid(
                "Nested abbreviations must be expanded before import",
            ));
        }
    }
    for index in descendants {
        if !matches!(
            tree.node(index)?.tag.as_str(),
            "fragment" | "n" | "b" | "t" | "s"
        ) {
            return Err(invalid(
                "Unsupported objects within an abbreviation definition",
            ));
        }
    }
    let mut nodes = Vec::new();
    let mut by_id = HashMap::new();
    let mut connections = Vec::new();
    for node in tree.children(inner, "n")? {
        let element = tree.node(node)?;
        let id = element.attr("id").map(str::to_owned);
        if by_id.insert(id.clone(), node).is_some() {
            return Err(Error::Unsupported("duplicate abbreviation atom IDs"));
        }
        if element.attr("NodeType") == Some("ExternalConnectionPoint") {
            connections.push(node);
        }
        nodes.push((id, node));
    }
    if connections.len() > 1 && tree.node(outer)?.attr("BondOrdering").is_none() {
        return Err(invalid(
            "Multiple abbreviation attachments are not supported yet",
        ));
    }
    let parent = tree
        .node(outer)?
        .parent
        .ok_or_else(|| invalid("Abbreviation outside a molecular fragment"))?;
    if tree.node(parent)?.tag != "fragment" {
        return Err(invalid("Abbreviation outside a molecular fragment"));
    }
    let outer_id = tree.node(outer)?.attr("id").map(str::to_owned);
    let mut external = attached(tree, parent, outer_id.as_deref())?;
    let multiple = connections.len() > 1;
    if multiple && connections.len() != external.len() {
        return Err(invalid(
            "Multiple abbreviation attachments are not supported yet",
        ));
    }
    if external.len() > 1 && !multiple {
        return Err(invalid("Multiple abbreviation bonds are not supported yet"));
    }
    let target = position(tree, outer, "Invalid abbreviation position")?;
    if !target.iter().all(|v| v.is_finite()) {
        return Err(invalid("Invalid abbreviation position"));
    }
    let mut removed_bonds = Vec::new();
    let multi_anchor = if multiple {
        let order: Vec<_> = tree
            .node(outer)?
            .attr("BondOrdering")
            .unwrap_or("")
            .split_whitespace()
            .collect();
        let order_connections: Vec<_> = tree
            .node(inner)?
            .attr("ConnectionOrder")
            .map(|value| value.split_whitespace().map(str::to_owned).collect())
            .unwrap_or(
                connections
                    .iter()
                    .map(|&key| {
                        tree.node(key)
                            .map(|node| node.attr("id").unwrap_or("").to_owned())
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
        if order.len() != external.len() || order_connections.len() != connections.len() {
            return Err(invalid("Invalid internal abbreviation attachment ordering"));
        }
        let ordered = |keys: &[usize], ids: &[&str]| -> Result<Vec<usize>> {
            let mut result = Vec::new();
            for id in ids {
                let key = keys
                    .iter()
                    .copied()
                    .find(|&key| tree.node(key).is_ok_and(|n| n.attr("id") == Some(*id)))
                    .ok_or_else(|| invalid("Missing internal abbreviation attachment"))?;
                if result.contains(&key) {
                    return Err(invalid("Duplicate internal abbreviation attachment"));
                }
                result.push(key);
            }
            Ok(result)
        };
        external = ordered(&external, &order)?;
        connections = ordered(
            &connections,
            &order_connections
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        )?;
        let mut anchor = None;
        for (&connection, &outside) in connections.iter().zip(&external) {
            let id = tree.node(connection)?.attr("id");
            let bonds = attached(tree, inner, id)?;
            let [bond] = bonds.as_slice() else {
                return Err(invalid("Invalid abbreviation connection point"));
            };
            let b = tree.node(*bond)?;
            let target = b
                .attr(if b.attr("B") == id { "E" } else { "B" })
                .map(str::to_owned);
            let target = by_id
                .get(&target)
                .copied()
                .ok_or_else(|| invalid("Missing internal abbreviation anchor"))?;
            if connections.contains(&target) || anchor.is_some_and(|a| a != target) {
                return Err(invalid(
                    "Internal abbreviation attachments must share one atom",
                ));
            }
            if b.attr("Order").unwrap_or("1") != tree.node(outside)?.attr("Order").unwrap_or("1") {
                return Err(invalid(
                    "Abbreviation attachment bond order conflicts with its definition",
                ));
            }
            anchor = Some(target);
            removed_bonds.push(*bond);
        }
        anchor
    } else {
        None
    };
    let connection = connections.first().copied();
    let (anchor, connection_bond) = if multiple {
        (multi_anchor, removed_bonds.first().copied())
    } else if let Some(connection) = connection {
        let id = tree.node(connection)?.attr("id");
        let matches = attached(tree, inner, id)?;
        let bond = matches
            .first()
            .copied()
            .filter(|_| matches.len() == 1)
            .ok_or_else(|| invalid("Invalid abbreviation connection point"))?;
        let bond_node = tree.node(bond)?;
        let anchor = bond_node
            .attr(if bond_node.attr("B") == id { "E" } else { "B" })
            .map(str::to_owned);
        let anchor = by_id.get(&anchor).copied();
        if let Some(&external) = external.first()
            && bond_node.attr("Order").unwrap_or("1")
                != tree.node(external)?.attr("Order").unwrap_or("1")
        {
            return Err(invalid(
                "Abbreviation attachment bond order conflicts with its definition",
            ));
        }
        (anchor, Some(bond))
    } else {
        if !external.is_empty() {
            return Err(invalid("Missing abbreviation attachment point"));
        }
        (nodes.first().map(|(_, node)| *node), None)
    };
    let anchor = anchor.ok_or_else(|| invalid("Empty abbreviation definition"))?;
    if connections.contains(&anchor) {
        return Err(invalid("Invalid abbreviation connection point"));
    }
    if !multiple {
        removed_bonds.extend(connection_bond);
    }
    let anchor_id = tree.node(anchor)?.attr("id").map(str::to_owned);
    let origin = position(tree, anchor, "Missing abbreviation atom position")?;
    let (mut rotation, mut scale) = (0.0_f64, 1.0_f64);
    if let Some((connection, &external)) = connection.zip(external.first()) {
        let source = position(tree, connection, "Invalid abbreviation attachment geometry")?;
        let bond = tree.node(external)?;
        let outside_id = bond.attr(if bond.attr("B") == outer_id.as_deref() {
            "E"
        } else {
            "B"
        });
        let mut outside = None;
        for node in tree.children(parent, "n")? {
            if tree.node(node)?.attr("id") == outside_id {
                outside = Some(node);
                break;
            }
        }
        let outside = outside.ok_or_else(|| invalid("Missing abbreviation neighbor"))?;
        let dest = position(tree, outside, "Invalid abbreviation attachment geometry")?;
        let source_length = (source[0] - origin[0]).hypot(source[1] - origin[1]);
        let dest_length = (dest[0] - target[0]).hypot(dest[1] - target[1]);
        if source_length > 1e-6 && dest_length > 1e-6 {
            rotation = (dest[1] - target[1]).atan2(dest[0] - target[0])
                - (source[1] - origin[1]).atan2(source[0] - origin[0]);
            scale = dest_length / source_length;
        }
    }
    let (c, s) = (rotation.cos(), rotation.sin());
    let mut members = Vec::new();
    // Capture bonds before moving nodes out of the discarded definition.
    let bonds = tree.children(inner, "b")?;
    for (id, node) in nodes {
        if connections.contains(&node) {
            continue;
        }
        let values = position(tree, node, "Invalid abbreviation atom position")?;
        if !values.iter().all(|v| v.is_finite()) {
            return Err(invalid("Invalid abbreviation atom position"));
        }
        let (x, y) = (
            (values[0] - origin[0]) * scale,
            (values[1] - origin[1]) * scale,
        );
        let p = format!(
            "{} {}",
            significant(target[0] + x * c - y * s),
            significant(target[1] + x * s + y * c)
        );
        tree.node_mut(node)?.set("p", p.clone());
        for text in tree.children(node, "t")? {
            tree.node_mut(text)?.set("p", p.clone());
        }
        tree.append(parent, node)?;
        members.push(id);
    }
    for bond in bonds {
        if !removed_bonds.contains(&bond) {
            tree.append(parent, bond)?;
        }
    }
    for bond in external {
        let side = if tree.node(bond)?.attr("B") == outer_id.as_deref() {
            "B"
        } else {
            "E"
        };
        tree.node_mut(bond)?.set(
            side,
            anchor_id
                .clone()
                .ok_or_else(|| invalid("Missing abbreviation attachment atom ID"))?,
        );
    }
    for text in tree.children(anchor, "t")? {
        tree.detach(text)?;
    }
    let label_text = tree.clone_subtree(text)?;
    let p = tree.node(anchor)?.attr("p").unwrap_or("").to_owned();
    tree.node_mut(label_text)?.set("p", p);
    if multiple {
        // A multi-bond label's saved line arrangement is derived from geometry.
        // Recompute it as the bonds move; retain its text and complete definition.
        tree.node_mut(label_text)?
            .set("LabelJustification", "Auto".into());
    }
    tree.append(anchor, label_text)?;
    let mut label = String::new();
    for span in tree.children(text, "s")? {
        label.push_str(&tree.node(span)?.text);
    }
    let mut reverse_label = String::new();
    // Other presets have no alternate spelling and leave both strings alone.
    for (name, reverse) in [
        ("OMe", "MeO"),
        ("OEt", "EtO"),
        ("OAc", "AcO"),
        ("OBz", "BzO"),
        ("OTs", "TsO"),
        ("OMs", "MsO"),
        ("CF3", "F3C"),
        ("CN", "NC"),
        ("NO2", "O2N"),
        ("CO2H", "HO2C"),
        ("CO2Me", "MeO2C"),
        ("CO2Et", "EtO2C"),
    ] {
        if label == name || label == reverse {
            label = name.into();
            reverse_label = reverse.into();
            break;
        }
    }
    tree.detach(outer)?;
    Ok(Expanded {
        abbreviation: Abbreviation {
            label,
            reverse_label,
            anchor: anchor_id,
            members,
        },
        atoms: 1 + connections.len(),
        bonds: removed_bonds.len(),
    })
}

/// Python's .8g spelling, including exponent padding and signed zero.
fn significant(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into();
    }
    let scientific = format!("{value:.7e}");
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return scientific;
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return scientific;
    };
    if !(-4..8).contains(&exponent) {
        format!(
            "{}e{exponent:+03}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        )
    } else {
        let fixed = format!("{:.*}", usize::try_from(7 - exponent).unwrap_or(0), value);
        if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            fixed
        }
    }
}
