//! Chemistry-only copy from reference/engine/bonds_exchange.py::chemistry_xml.
use super::{Error, Result, tree::Tree};
use std::collections::HashSet;

/// Prepare an XML copy for molecular parsing, preserving the drawing source.
///
/// Embedded pictures are removed, bond displays are validated, and hydrogen/
/// aromatic orders and equivalent wedge depictions are normalized exactly as
/// in the original importer. The caller must retain the drawing XML to restore
/// its bond orders and appearance before chemical preparation. This function
/// neither decodes pictures nor validates their presentation data.
///
/// The detached tree's XML size, depth, work, namespace, and DTD limits apply.
pub fn chemistry_xml(text: &str) -> Result<String> {
    normalize_tree(Tree::parse(text)?)
}
pub(super) fn normalize_tree(mut tree: Tree) -> Result<String> {
    remove_pictures(&mut tree)?;
    // ChemDraw's AS="N" (no stereocentre) at a metal marks frame projection wedges.
    let mut frame_metals = HashSet::new();
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag == "n"
            && node.attr("AS") == Some("N")
            && node
                .attr("Element")
                .and_then(|n| crate::chemistry::ELEMENTS.get(n.trim().parse::<usize>().ok()?))
                .is_some_and(|e| crate::joining::coordination_metal_element(e.symbol))
            && let Some(id) = node.attr("id")
        {
            frame_metals.insert((node.parent, id.trim().to_owned()));
        }
    }
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag != "b" {
            continue;
        }
        let display = node.attr("Display").unwrap_or("Solid").to_owned();
        let original_order = node.attr("Order").unwrap_or("1").to_owned();
        let narrow = node.attr(if display.ends_with("End") { "E" } else { "B" });
        let frame_wedge = original_order == "1"
            && matches!(
                display.as_str(),
                "WedgeBegin" | "WedgeEnd" | "WedgedHashBegin" | "WedgedHashEnd"
            )
            && narrow.is_some_and(|id| frame_metals.contains(&(node.parent, id.trim().to_owned())));
        let secondary = node.attr("Display2").unwrap_or("Solid");
        if !matches!(
            display.as_str(),
            "Solid"
                | "Dash"
                | "Dot"
                | "Bold"
                | "Hash"
                | "WedgeBegin"
                | "WedgeEnd"
                | "WedgedHashBegin"
                | "WedgedHashEnd"
                | "HollowWedgeBegin"
                | "HollowWedgeEnd"
                | "Wavy"
        ) || !matches!(secondary, "Solid" | "Dash" | "Bold" | "DottedHydrogen")
        {
            return Err(Error::Invalid(
                "This CDXML bond display is not supported yet".into(),
            ));
        }
        if secondary == "DottedHydrogen" {
            tree.node_mut(index)?
                .attributes
                .retain(|(key, _)| key != "Display2");
        }
        if matches!(tree.node(index)?.attr("Order"), Some("hydrogen" | "1.5")) {
            tree.node_mut(index)?.set("Order", "1".into());
        }
        // Remove frame depth only from the chemical copy, before tetrahedral
        // perception. The original drawing retains its wedge/hash appearance.
        let normalized = if frame_wedge {
            Some("Solid")
        } else {
            match display.as_str() {
                "Bold" | "WedgeBegin" | "WedgeEnd" if original_order == "1.5" => Some("Solid"),
                "HollowWedgeBegin" => Some("WedgeBegin"),
                "HollowWedgeEnd" => Some("WedgeEnd"),
                "Hash" => Some("WedgedHashBegin"),
                // An aromatic bold edge has no tetrahedral meaning. Converting it
                // after the temporary order normalization could invent a hydrogen.
                "Bold" if original_order == "1" => Some("WedgeBegin"),
                _ => None,
            }
        };
        if let Some(normalized) = normalized {
            tree.node_mut(index)?.set("Display", normalized.into());
        }
    }
    tree.serialize()
}

fn remove_pictures(tree: &mut Tree) -> Result<()> {
    let mut pending = vec![0];
    while let Some(parent) = pending.pop() {
        // One pass over siblings avoids repeated Vec removal for large batches
        // of pictures. Removed descendants never reach the bond validation.
        let children = std::mem::take(&mut tree.node_mut(parent)?.children);
        for child in children {
            if tree.node(child)?.tag == "embeddedobject" {
                tree.node_mut(child)?.parent = None;
            } else {
                tree.node_mut(parent)?.children.push(child);
                pending.push(child);
            }
        }
    }
    Ok(())
}
