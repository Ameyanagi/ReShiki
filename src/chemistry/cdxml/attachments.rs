//! ChemDraw NodeType 10/11: retain target identities independently of the
//! temporary wildcard graph used by drawing reconstruction.
use super::{Error, Fragment, Result, tree::Tree};
use crate::attachments::{Attachment, Kind};
use std::collections::{HashMap, HashSet};

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

pub(super) fn normalize(tree: &mut Tree) -> Result<()> {
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag == "n" && node.attr("NodeType").and_then(Kind::from_cdxml).is_some() {
            if node.attr("Attachments").is_none() {
                return Err(invalid("Attachment node is missing its target atoms"));
            }
            // A missing Element defaults to carbon in the RDKit-compatible
            // parser. These nodes explicitly have no atomic identity.
            tree.node_mut(index)?.set("Element", "0".into());
            tree.node_mut(index)?.set("NumHydrogens", "0".into());
        } else if node.tag == "n"
            && node.attr("NodeType") == Some("Unspecified")
            && node.attr("Element").is_none_or(|value| value == "0")
        {
            // ChemDraw omits Element=0 when resaving an anonymous wildcard.
            // Only the exact '*' label identifies this case, never arbitrary text.
            let text = tree
                .descendants(index)?
                .iter()
                .filter_map(|&i| {
                    tree.node(i)
                        .ok()
                        .filter(|n| n.tag == "s")
                        .map(|n| n.text.as_str())
                })
                .collect::<String>();
            if text == "*" {
                tree.node_mut(index)?.set("Element", "0".into());
                tree.node_mut(index)?.set("NumHydrogens", "0".into());
            }
        }
    }
    Ok(())
}

pub(super) fn read(tree: &Tree, fragments: &[Fragment]) -> Result<Vec<Attachment>> {
    let mut nodes = HashMap::new();
    let mut targets = Vec::new();
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag != "n" {
            continue;
        }
        if let Some(kind) = node.attr("NodeType").and_then(Kind::from_cdxml) {
            targets.push((index, kind));
        }
        if let Some(id) = node.attr("id").and_then(|s| s.parse::<u32>().ok()) {
            nodes.entry(id).or_insert_with(Vec::new).push(index);
        }
    }
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids = HashMap::new();
    let mut next = 1u64;
    for fragment in fragments {
        for source in &fragment.atom_ids {
            if ids.insert(*source, next).is_some() {
                return Err(invalid("Ambiguous attachment atom ID"));
            }
            next = next.checked_add(1).ok_or(Error::Limit)?;
        }
    }
    let mut output = Vec::new();
    let mut total = 0usize;
    for (index, kind) in targets {
        let node = tree.node(index)?;
        let source = node
            .attr("id")
            .and_then(|v| v.parse::<u32>().ok())
            .ok_or_else(|| invalid("Invalid attachment ID"))?;
        let mut unique = HashSet::new();
        let mut members = Vec::new();
        for value in node
            .attr("Attachments")
            .ok_or_else(|| invalid("Missing attachment targets"))?
            .split_whitespace()
        {
            total = total
                .checked_add(1)
                .filter(|n| *n <= 1_000_000)
                .ok_or(Error::Limit)?;
            if members.len() >= 300 {
                return Err(Error::Limit);
            }
            let member = value
                .parse::<u32>()
                .map_err(|_| invalid("Invalid attachment target ID"))?;
            let candidates = nodes
                .get(&member)
                .ok_or_else(|| invalid("Missing attachment target atom"))?;
            let [target] = candidates.as_slice() else {
                return Err(invalid("Ambiguous attachment target atom"));
            };
            let target = tree.node(*target)?;
            if member == source
                || !unique.insert(member)
                || target.parent != node.parent
                || target.attr("NodeType").and_then(Kind::from_cdxml).is_some()
            {
                return Err(invalid(
                    "Attachment targets must be distinct ordinary atoms in the same fragment",
                ));
            }
            members.push(
                *ids.get(&member)
                    .ok_or_else(|| invalid("Unmapped attachment target atom"))?,
            );
        }
        if members.len() < 2 {
            return Err(invalid("An attachment needs at least two target atoms"));
        }
        output.push(Attachment {
            id: *ids
                .get(&source)
                .ok_or_else(|| invalid("Unmapped attachment point"))?,
            kind,
            members,
        });
    }
    Ok(output)
}
