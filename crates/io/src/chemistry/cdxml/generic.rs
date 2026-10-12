//! Typed generic data is detached before the concrete molecular parser runs.
//! References are restored through source object identities, never coordinates.
use super::{Error, Fragment, ObjectMapEntry, Result, tree::Tree};
use crate::generic::{
    Alternative, AlternativeSite, Crossing, Generic, LinkNode, RepeatCount, RepeatPattern,
    RepeatUnit,
    limits::{MAX_DEFINITIONS, MAX_REPEAT_COUNT, MAX_REPEAT_MEMBERS},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, Default, Serialize)]
pub struct SourceGeneric {
    atoms: Generic,
    labels: Vec<(u64, String)>,
    repeats: Vec<SourceRepeat>,
}
#[derive(Clone, Debug, Serialize)]
struct SourceRepeat {
    members: Vec<u64>,
    graphics: Vec<u64>,
    crossings: Vec<Crossing>,
    crossing_brackets: Vec<u64>,
    usage: String,
    pattern: RepeatPattern,
    count: RepeatCount,
    label: String,
    flip: String,
}
impl SourceGeneric {
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty() && self.repeats.is_empty()
    }
    pub(super) fn restore(
        &self,
        xml: &str,
        fragments: &[Fragment],
        objects: &[ObjectMapEntry],
    ) -> Result<(Generic, Vec<(u64, String)>)> {
        if self.is_empty() {
            return Ok((Generic::default(), Vec::new()));
        }
        let tree = Tree::parse_import(xml)?;
        let order = tree.descendants(0)?;
        let mut atom_ids = HashMap::new();
        let mut next = 1u64;
        for fragment in fragments {
            for &source in &fragment.atom_ids {
                if atom_ids.insert(u64::from(source), next).is_some() {
                    return Err(invalid("Ambiguous generic atom ID"));
                }
                next = next.checked_add(1).ok_or(Error::Limit)?;
            }
        }
        let mut by_ordinal = HashMap::new();
        for object in objects {
            by_ordinal.insert(object.source, object.atoms.as_slice());
        }
        let mut source_objects = HashMap::new();
        for (ordinal, &i) in order.iter().enumerate() {
            let n = tree.node(i)?;
            if let Some(id) = n.attr("id").and_then(|s| s.parse::<u64>().ok())
                && let Some(ids) = by_ordinal.get(&ordinal)
                && source_objects.insert(id, *ids).is_some()
            {
                return Err(invalid("Ambiguous generic object ID"));
            }
        }
        let drawing_atoms: HashSet<_> = atom_ids.values().copied().collect();
        let mut total_members = 0usize;
        let mut generic = self.atoms.clone();
        generic
            .remap(&atom_ids)
            .ok_or_else(|| invalid("Missing generic atom reference"))?;
        for source in &self.repeats {
            let mut members = Vec::new();
            let mut seen = HashSet::new();
            for id in &source.members {
                if let Some(&id) = atom_ids.get(id) {
                    if seen.insert(id) {
                        members.push(id);
                    }
                } else if let Some(ids) = source_objects.get(id) {
                    for &id in ids.iter() {
                        if drawing_atoms.contains(&id) && seen.insert(id) {
                            members.push(id);
                        }
                    }
                } else {
                    return Err(invalid("Missing bracketed object reference"));
                }
            }
            total_members = total_members.saturating_add(members.len());
            if total_members > MAX_REPEAT_MEMBERS {
                return Err(Error::Limit);
            }
            let mut graphics = Vec::new();
            for id in &source.graphics {
                let ids = source_objects
                    .get(id)
                    .ok_or_else(|| invalid("Missing bracket graphic reference"))?;
                let [id] = *ids else {
                    return Err(invalid("Ambiguous bracket graphic reference"));
                };
                if !graphics.contains(id) {
                    graphics.push(*id);
                }
            }
            let mut crossings = Vec::new();
            for c in &source.crossings {
                crossings.push(Crossing {
                    inside: *atom_ids
                        .get(&c.inside)
                        .ok_or_else(|| invalid("Missing inner atom reference"))?,
                    outside: *atom_ids
                        .get(&c.outside)
                        .ok_or_else(|| invalid("Missing outer atom reference"))?,
                });
            }
            generic.repeats.push(RepeatUnit {
                bracket: graphics.first().copied(),
                bracket_objects: graphics.iter().skip(1).copied().collect(),
                members,
                crossings,
                crossing_brackets: source
                    .crossing_brackets
                    .iter()
                    .map(|id| {
                        let ids = source_objects
                            .get(id)
                            .ok_or_else(|| invalid("Missing crossing bracket reference"))?;
                        let [id] = *ids else {
                            return Err(invalid("Ambiguous crossing bracket reference"));
                        };
                        Ok(*id)
                    })
                    .collect::<Result<Vec<_>>>()?,
                head_count: RepeatUnit::default_head_count(source.crossings.len()),
                usage: source.usage.clone(),
                pattern: source.pattern,
                count: source.count.clone(),
                label: source.label.clone(),
                flip: source.flip.clone(),
            });
        }
        let labels = self
            .labels
            .iter()
            .map(|(id, label)| {
                Ok((
                    *atom_ids
                        .get(id)
                        .ok_or_else(|| invalid("Missing generic label atom"))?,
                    label.clone(),
                ))
            })
            .collect::<Result<_>>()?;
        Ok((generic, labels))
    }
}
fn invalid(s: &str) -> Error {
    Error::Invalid(s.into())
}
fn id(s: Option<&str>) -> Result<u64> {
    s.ok_or_else(|| invalid("Missing generic object ID"))?
        .parse()
        .map_err(|_| invalid("Invalid generic object ID"))
}
fn text(tree: &Tree, index: usize) -> Result<String> {
    let mut out = String::new();
    for t in tree.children(index, "t")? {
        for i in tree.descendants(t)? {
            let n = tree.node(i)?;
            if n.tag == "s" {
                out.push_str(&n.text);
            }
        }
    }
    Ok(out)
}
fn ids(value: Option<&str>) -> Result<Vec<u64>> {
    let values = value.ok_or_else(|| invalid("Missing bracketed objects"))?;
    if values.len() > 1_000_000 {
        return Err(Error::Limit);
    }
    values.split_whitespace().map(|s| id(Some(s))).collect()
}
pub(super) fn extract(xml: &str) -> Result<(String, SourceGeneric)> {
    let mut tree = Tree::parse_import(xml)?;
    let order = tree.descendants(0)?;
    let mut source = SourceGeneric::default();
    let mut has_generic = false;
    for &i in &order {
        let n = tree.node(i)?;
        has_generic |= n.tag == "bracketedgroup"
            || n.tag == "graphic" && n.attr("BracketUsage").is_some()
            || n.tag == "n"
                && (matches!(
                    n.attr("NodeType"),
                    Some(
                        "ElementList"
                            | "AnonymousAlternativeGroup"
                            | "ElementListNickname"
                            | "LinkNode"
                    )
                ) || n.attr("LinkCountLow").is_some()
                    || n.attr("LinkCountHigh").is_some());
    }
    if !has_generic {
        return Ok((xml.into(), source));
    }
    let mut bonds = HashMap::new();
    for &i in &order {
        let n = tree.node(i)?;
        if n.tag == "b" {
            // Ordinary CDXML may omit bond IDs. Only explicitly referenced
            // crossing bonds need an unambiguous ID in the generic index.
            let Some(value) = n.attr("id") else {
                continue;
            };
            let bond = id(Some(value))?;
            if bonds
                .insert(bond, (id(n.attr("B"))?, id(n.attr("E"))?))
                .is_some()
            {
                return Err(invalid("Ambiguous crossing bond ID"));
            }
        }
    }
    for &i in &order {
        let node = tree.node(i)?.clone();
        if node.tag == "n"
            && matches!(
                node.attr("NodeType"),
                Some("ElementList" | "AnonymousAlternativeGroup" | "ElementListNickname")
            )
        {
            let atom = id(node.attr("id"))?;
            if node.attr("GenericList").is_some() {
                return Err(Error::Unsupported(
                    "generic nickname element-list predicates",
                ));
            }
            let label = if node.attr("NodeType") == Some("AnonymousAlternativeGroup") {
                node.attr("GenericNickname")
                    .map(str::to_owned)
                    .unwrap_or(text(&tree, i)?)
            } else {
                text(&tree, i)?
            };
            let (choices, excluded) = if node.attr("NodeType") == Some("ElementList") {
                let value = node
                    .attr("ElementList")
                    .ok_or_else(|| invalid("Missing element list"))?;
                let (value, excluded) = if let Some(rest) = value.strip_prefix("NOT ") {
                    (rest, true)
                } else {
                    (value, false)
                };
                let mut choices = Vec::new();
                for number in value.split_whitespace() {
                    let n = number
                        .parse::<usize>()
                        .map_err(|_| invalid("Invalid element-list atomic number"))?;
                    let element = crate::chemistry::ELEMENTS
                        .get(n)
                        .filter(|_| n > 0 && n <= 118)
                        .ok_or_else(|| invalid("Invalid element-list atomic number"))?;
                    choices.push(Alternative::Element {
                        symbol: element.symbol.into(),
                        isotope: 0,
                    });
                }
                (choices, excluded)
            } else {
                crate::generic::alternative(&label)
                    .map_err(Error::Invalid)?
                    .ok_or_else(|| {
                        invalid("Anonymous alternatives require a comma-separated label")
                    })?
            };
            if choices.is_empty() {
                return Err(invalid("An element list cannot be empty"));
            }
            if node.attr("NodeType") == Some("AnonymousAlternativeGroup") {
                let fragments = tree.children(i, "fragment")?;
                if fragments.len() < 2 || fragments.len() != choices.len() {
                    return Err(invalid(
                        "Anonymous alternatives require one explicit fragment for each of at least two choices",
                    ));
                }
                for (&fragment, choice) in fragments.iter().zip(&choices) {
                    verify_alternative_fragment(&tree, fragment, choice)?;
                }
                for fragment in fragments {
                    tree.detach(fragment)?;
                }
            }
            let label = if label.trim().is_empty() {
                AlternativeSite::label_without_isotope_aliases(excluded, &choices)
            } else {
                label
            };
            source.atoms.alternatives.push(AlternativeSite {
                atom,
                choices,
                excluded,
            });
            source.labels.push((atom, label));
            let node = tree.node_mut(i)?;
            node.set("NodeType", "Unspecified".into());
            node.set("Element", "0".into());
            node.set("NumHydrogens", "0".into());
            node.attributes
                .retain(|(k, _)| !matches!(k.as_str(), "ElementList" | "GenericNickname"));
        } else if node.tag == "n"
            && (node.attr("NodeType") == Some("LinkNode")
                || node.attr("LinkCountLow").is_some()
                || node.attr("LinkCountHigh").is_some())
        {
            if node.attr("GenericNickname").is_some() {
                return Err(Error::Unsupported("generic-nickname link nodes"));
            }
            let atom = id(node.attr("id"))?;
            let min = node
                .attr("LinkCountLow")
                .ok_or_else(|| invalid("Missing lower link count"))?
                .parse()
                .map_err(|_| invalid("Invalid lower link count"))?;
            let max = node
                .attr("LinkCountHigh")
                .ok_or_else(|| invalid("Missing upper link count"))?
                .parse()
                .map_err(|_| invalid("Invalid upper link count"))?;
            source.atoms.links.push(LinkNode { atom, min, max });
            let entered = text(&tree, i)?;
            let symbol = node
                .attr("GenericNickname")
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    let number = node
                        .attr("Element")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(6);
                    crate::chemistry::ELEMENTS
                        .get(number)
                        .map(|e| e.symbol)
                        .unwrap_or("*")
                        .into()
                });
            source.labels.push((
                atom,
                if entered.trim().is_empty() {
                    LinkNode::format_label(&symbol, min, max)
                } else {
                    entered
                },
            ));
            let node = tree.node_mut(i)?;
            node.set("NodeType", "Element".into());
            if node.attr("GenericNickname").is_some() {
                node.set("Element", "0".into());
                node.set("NumHydrogens", "0".into());
            }
            node.attributes.retain(|(k, _)| {
                !matches!(
                    k.as_str(),
                    "LinkCountLow" | "LinkCountHigh" | "GenericNickname"
                )
            });
        } else if node.tag == "bracketedgroup" {
            if node.attr("ComponentOrder").is_some() {
                return Err(Error::Unsupported("polymer component ordering"));
            }
            let members = ids(node.attr("BracketedObjectIDs"))?;
            let usage = node
                .attr("BracketUsage")
                .unwrap_or("Unspecified")
                .to_owned();
            let label = node
                .attr("SRULabel")
                .map(str::to_owned)
                .or_else(|| node.attr("RepeatCount").map(str::to_owned))
                .unwrap_or_else(|| "n".into());
            let count = if let Some(value) = node.attr("RepeatCount") {
                let value = value
                    .parse::<f64>()
                    .map_err(|_| invalid("Invalid polymer repeat count"))?;
                if !value.is_finite()
                    || value.fract() != 0.
                    || value < 1.
                    || value > f64::from(MAX_REPEAT_COUNT)
                {
                    return Err(invalid("Invalid finite polymer repeat count"));
                }
                RepeatCount::Finite {
                    min: value as u32,
                    max: value as u32,
                }
            } else {
                RepeatCount::parse(&label).map_err(Error::Invalid)?
            };
            let pattern = RepeatPattern::from_cdxml(
                node.attr("PolymerRepeatPattern").unwrap_or("HeadToTail"),
            )
            .ok_or_else(|| invalid("Unsupported polymer repeat-pattern value"))?;
            let mut graphics = Vec::new();
            let mut crossings = Vec::new();
            let mut crossing_brackets = Vec::new();
            for a in tree.children(i, "bracketattachment")? {
                let n = tree.node(a)?;
                let graphic = id(n.attr("GraphicID"))?;
                graphics.push(graphic);
                for c in tree.children(a, "crossingbond")? {
                    let n = tree.node(c)?;
                    let inner = id(n.attr("InnerAtomID"))?;
                    let (a, b) = *bonds
                        .get(&id(n.attr("BondID"))?)
                        .ok_or_else(|| invalid("Missing polymer crossing bond"))?;
                    let outer = if a == inner {
                        b
                    } else if b == inner {
                        a
                    } else {
                        return Err(invalid("Polymer inner atom is not on its crossing bond"));
                    };
                    crossings.push(Crossing {
                        inside: inner,
                        outside: outer,
                    });
                    crossing_brackets.push(graphic);
                }
            }
            source.repeats.push(SourceRepeat {
                members,
                graphics,
                crossings,
                crossing_brackets,
                usage,
                pattern,
                count,
                label,
                flip: node.attr("PolymerFlipType").unwrap_or("Unspecified").into(),
            });
            let node = tree.node_mut(i)?;
            node.tag = "group".into();
            node.attributes.retain(|(k, _)| k == "id");
        } else if matches!(node.tag.as_str(), "bracketattachment" | "crossingbond") {
            let node = tree.node_mut(i)?;
            node.tag = "group".into();
            node.attributes.retain(|(k, _)| k == "id");
        } else if node.tag == "graphic"
            && node
                .attr("BracketUsage")
                .is_some_and(|s| s != "Unspecified")
        {
            return Err(Error::Unsupported(
                "legacy chemical bracket has no explicit membership; save as bracketedgroup CDXML or native ReShiki",
            ));
        }
    }
    if source.atoms.alternatives.len() + source.atoms.links.len() + source.repeats.len()
        > MAX_DEFINITIONS
    {
        return Err(Error::Limit);
    }
    Ok((
        if source.is_empty() {
            xml.into()
        } else {
            tree.serialize()?
        },
        source,
    ))
}

fn verify_alternative_fragment(tree: &Tree, fragment: usize, choice: &Alternative) -> Result<()> {
    let nodes = tree.descendants(fragment)?;
    if nodes.len() > 512 {
        return Err(Error::Limit);
    }
    let external = nodes
        .iter()
        .filter(|&&id| {
            tree.node(id).is_ok_and(|n| {
                n.tag == "n" && n.attr("NodeType") == Some("ExternalConnectionPoint")
            })
        })
        .count();
    if external != 1 {
        return Err(Error::Unsupported(
            "anonymous fragments require exactly one explicit external connection point",
        ));
    }
    let mut detached = Tree::parse_import(&tree.serialize_subtree(fragment)?)?;
    for id in detached.descendants(0)? {
        if detached.node(id)?.tag == "n"
            && detached.node(id)?.attr("NodeType") == Some("ExternalConnectionPoint")
        {
            // A missing port appearance still denotes a semantic external
            // dummy in a fragment definition. The ordinary inherited parser
            // defaults a standalone unspecified element to carbon.
            detached.node_mut(id)?.set("Element", "0".into());
            detached.node_mut(id)?.set("NumHydrogens", "0".into());
        }
    }
    let bond_length = super::numeric::float(tree.node(0)?.attr("BondLength").unwrap_or("14.4"))
        .map_err(|e| invalid(&format!("Invalid anonymous fragment bond length: {e}")))?;
    let xml = format!(
        "<CDXML BondLength=\"{bond_length}\"><page>{}</page></CDXML>",
        detached.serialize()?
    );
    let actual = super::prepare_cdxml(&xml)
        .map_err(|e| invalid(&format!("Invalid anonymous alternative fragment: {e}")))?;
    let actual_smiles =
        crate::chemistry::smiles::write::write(&actual.molecule.state, Default::default())
            .map_err(|_| invalid("Invalid anonymous fragment chemistry"))?
            .text;
    let mut expected = crate::generic::alternative_fragment(choice).map_err(Error::Invalid)?;
    let anchor = expected
        .atoms
        .first()
        .ok_or_else(|| invalid("Missing catalog fragment root"))?
        .id;
    let dummy = expected.add_atom("*", crate::document::Point::new(-42., 0.));
    expected.add_bond(anchor, dummy, 1, "plain");
    let expected = crate::chemistry::document::prepare(&expected)
        .map_err(|_| invalid("Invalid catalog fragment"))?;
    let expected_smiles =
        crate::chemistry::smiles::write::write(&expected.state, Default::default())
            .map_err(|_| invalid("Invalid catalog fragment chemistry"))?
            .text;
    if actual_smiles != expected_smiles {
        return Err(Error::Unsupported(
            "anonymous fragment differs from its defined catalog label; retain it in the original ChemDraw file",
        ));
    }
    Ok(())
}
