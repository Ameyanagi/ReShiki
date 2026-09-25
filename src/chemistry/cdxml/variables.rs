//! Drawing-only R/X labels; query chemistry remains unsupported.
use super::{PreparedCdxml, Result, tree::Tree};
use std::collections::HashMap;

pub(crate) struct Variables(pub HashMap<u32, String>);

pub(crate) fn drawing_variables(xml: &str) -> Result<(String, Variables)> {
    let mut tree = Tree::parse_import(xml)?;
    let mut labels = HashMap::new();
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag != "n" || node.attr("NodeType") != Some("GenericNickname") {
            continue;
        }
        let label = node.attr("GenericNickname").unwrap_or("");
        if !matches!(label, "R" | "X")
            || ["Element", "Charge", "Isotope", "NumHydrogens"]
                .iter()
                .any(|key| node.attr(key).is_some_and(|v| v != "0"))
        {
            continue;
        }
        let Some(id) = node.attr("id").and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let label = label.to_owned();
        let node = tree.node_mut(index)?;
        node.set("NodeType", "Unspecified".into());
        node.set("Element", "0".into());
        node.set("NumHydrogens", "0".into());
        node.attributes.retain(|(key, _)| key != "GenericNickname");
        labels.insert(id, label);
    }
    Ok((
        if labels.is_empty() {
            xml.into()
        } else {
            tree.serialize()?
        },
        Variables(labels),
    ))
}

impl Variables {
    pub(crate) fn restore(
        &self,
        prepared: &PreparedCdxml,
        document: &mut crate::document::Document,
    ) -> Result<()> {
        let sources = prepared.fragments.iter().flat_map(|f| &f.atom_ids);
        for (&source, &id) in sources.zip(&prepared.molecule.ids) {
            if let Some(label) = self.0.get(&source) {
                let atom = document.atom_mut(id).ok_or(super::Error::Limit)?;
                atom.display.variable = Some(label.clone());
                atom.element = "*".into();
                atom.no_implicit = true;
                atom.label_h = 0;
            }
        }
        Ok(())
    }
}
