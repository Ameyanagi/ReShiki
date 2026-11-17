//! ChemDraw linked property text (`chemicalproperty`) read as property links.
use super::*;
use crate::linked_properties::{Property, PropertyCaption};

/// Live links whose property types ReShiki shows. An inactive link or an
/// unknown type leaves plain text. Import keeps one link per molecule.
pub(super) fn read(
    nodes: &[Node<'_, '_>],
    order: &[usize],
    tree: &Tree,
    prepared: &PreparedCdxml,
    association: &PreparedAtoms<'_>,
    objects: &Objects,
) -> Result<Vec<PropertyCaption>> {
    let by_id: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.attribute("id").map(|id| (id, i)))
        .collect();
    let mut links = Vec::new();
    for link in nodes
        .iter()
        .filter(|n| n.has_tag_name("chemicalproperty"))
        .filter(|n| n.attribute("ChemicalPropertyIsActive") == Some("yes"))
    {
        let Some(mut properties) = link
            .attribute("ChemicalPropertyType")
            .unwrap_or("")
            .split_whitespace()
            .map(|code| code.parse().ok().and_then(Property::from_chemdraw))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        properties.sort_by_key(|p| Property::ALL.iter().position(|q| q == p));
        properties.dedup();
        let Some(&caption) = link
            .attribute("ChemicalPropertyDisplayID")
            .and_then(|id| by_id.get(id))
            .filter(|&&i| nodes.get(i).is_some_and(|n| n.has_tag_name("t")))
            .and_then(|&i| objects.get(i))
            .and_then(|ids| ids.first())
        else {
            continue;
        };
        let mut atoms = BTreeSet::new();
        for id in link
            .attribute("BasisObjects")
            .unwrap_or("")
            .split_whitespace()
        {
            let Some((&ordinal, node)) = by_id.get(id).and_then(|i| nodes.get(*i).map(|n| (i, n)))
            else {
                continue;
            };
            if node.has_tag_name("fragment") {
                atoms.extend(objects.get(ordinal).into_iter().flatten().copied());
            } else if node.has_tag_name("n")
                && node.parent().is_some_and(|p| p.has_tag_name("fragment"))
                && let Some(p) = node.attribute("p")
            {
                atoms.insert(association.identify_with_tolerance(
                    tree,
                    *order.get(ordinal).ok_or(SceneError::Limit)?,
                    point(p, prepared.source_scale)?,
                    0.01,
                    "Could not associate linked property text with its atom",
                )?);
            }
        }
        if properties.is_empty() || atoms.is_empty() {
            continue;
        }
        links.push(PropertyCaption {
            annotation: caption,
            atoms: atoms.into_iter().collect(),
            properties,
            two_decimal_weight: true,
            ..Default::default()
        });
    }
    Ok(links)
}
