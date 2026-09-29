//! Validate external reaction references without inventing native reaction roles.
use super::{Error, Result, tree::Tree};
use std::collections::HashMap;

pub(crate) const NOTICE: &str = "Drawing imported; external reaction roles and condition references are not retained. The molecules, captions, and arrows remain editable. Keep the original CDX/CDXML or native ReShiki document for reaction metadata.";

pub(crate) fn validate(text: &str) -> Result<bool> {
    // Serialized CDXML escapes literal '<' inside text and attribute values.
    // Avoid another tree allocation for the usual drawing without schemes.
    if !text.contains("<scheme") && !text.contains("<step") {
        return Ok(false);
    }
    let tree = Tree::parse_import(text)?;
    let nodes = tree.descendants(0)?;
    let mut ids = HashMap::new();
    let mut metadata = Vec::new();
    for &index in &nodes {
        let node = tree.node(index)?;
        if matches!(node.tag.as_str(), "scheme" | "step") {
            metadata.push(index);
        }
        // Fonts have their own identifier namespace.
        if node.tag != "font"
            && let Some(id) = node.attr("id")
        {
            let id = id.parse::<u32>().map_err(|_| invalid())?;
            if id != 0 && ids.insert(id, node.tag.as_str()).is_some() {
                return Err(invalid());
            }
        }
    }
    let mut references = 0usize;
    for &index in &metadata {
        let node = tree.node(index)?;
        if !node
            .attr("id")
            .is_some_and(|id| id.parse::<u32>().is_ok_and(|id| id != 0))
        {
            return Err(invalid());
        }
        let parent = node.parent.map(|p| tree.node(p)).transpose()?;
        if node.tag == "scheme" {
            if !parent.is_some_and(|p| matches!(p.tag.as_str(), "page" | "group"))
                || node.attributes.iter().any(|(key, _)| key != "id")
                || node
                    .children
                    .iter()
                    .any(|&i| tree.node(i).is_ok_and(|n| n.tag != "step"))
            {
                return Err(invalid());
            }
        } else if parent.is_none_or(|p| p.tag != "scheme") || !node.children.is_empty() {
            return Err(invalid());
        }
        for (name, value) in &node.attributes {
            if name == "id" {
                continue;
            }
            if !matches!(
                name.as_str(),
                "ReactionStepReactants"
                    | "ReactionStepProducts"
                    | "ReactionStepPlusses"
                    | "ReactionStepArrows"
                    | "ReactionStepObjectsAboveArrow"
                    | "ReactionStepObjectsBelowArrow"
                    | "ReactionStepAtomMap"
                    | "ReactionStepAtomMapManual"
                    | "ReactionStepAtomMapAuto"
            ) {
                return Err(invalid());
            }
            let mut count = 0;
            for reference in value.split_whitespace() {
                references += 1;
                if references > 100_000 {
                    return Err(Error::Limit);
                }
                let id = reference.parse::<u32>().map_err(|_| invalid())?;
                if ids
                    .get(&id)
                    .is_none_or(|tag| matches!(*tag, "scheme" | "step" | "CDXML" | "page"))
                {
                    return Err(invalid());
                }
                count += 1;
            }
            if name.starts_with("ReactionStepAtomMap") && count % 2 != 0 {
                return Err(invalid());
            }
        }
    }
    Ok(!metadata.is_empty())
}

fn invalid() -> Error {
    Error::Invalid("Unsupported or invalid reaction scheme references".into())
}
