//! ChemDraw generic properties are attached to their actual atom and bracket IDs.
use super::{Result, Writer, invalid, tree::Key};
use crate::{
    document::Atom,
    generic::{Alternative, RepeatCount, RepeatPattern},
    typography::TextFormat,
};
impl Writer<'_> {
    pub(super) fn generic_atom(&mut self, node: Key, atom: &Atom) -> Result<bool> {
        if let Some(site) = self
            .doc
            .generic
            .alternatives
            .iter()
            .find(|s| s.atom == atom.id)
        {
            if site.choices.iter().any(|c| {
                matches!(c, Alternative::Element { symbol, isotope }
                if *isotope != 0 && !(symbol == "H" && matches!(*isotope, 2 | 3)))
            }) {
                return Err(invalid(
                    "Isotopic alternatives other than D/T require native ReShiki export",
                ));
            }
            let elements = site
                .choices
                .iter()
                .all(|c| matches!(c, Alternative::Element { isotope: 0, .. }));
            if elements {
                let mut list = Vec::new();
                for choice in &site.choices {
                    if let Alternative::Element { symbol, .. } = choice {
                        let n = crate::chemistry::ELEMENTS
                            .iter()
                            .position(|e| e.symbol == symbol)
                            .ok_or_else(|| invalid("Invalid element alternative"))?;
                        list.push(n.to_string());
                    }
                }
                self.tree.set(node, "NodeType", "ElementList")?;
                self.tree.set(
                    node,
                    "ElementList",
                    format!(
                        "{}{}",
                        if site.excluded { "NOT " } else { "" },
                        list.join(" ")
                    ),
                )?;
            } else {
                if site.excluded {
                    return Err(invalid("An element NOT list cannot contain fragments"));
                }
                self.tree
                    .set(node, "NodeType", "AnonymousAlternativeGroup")?;
                if site.choices.len() < 2 {
                    return Err(invalid(
                        "Anonymous alternatives require at least two fragments",
                    ));
                }
                for choice in &site.choices {
                    self.alternative_fragment(node, atom, choice)?;
                }
            }
            self.tree.set(node, "Element", "0")?;
            self.tree.set(node, "NumHydrogens", "0")?;
            let label = {
                format!(
                    "[{}{}]",
                    if site.excluded { "NOT " } else { "" },
                    site.choices
                        .iter()
                        .map(|c| match c {
                            Alternative::Element { symbol, isotope } =>
                                if symbol == "H" && *isotope == 2 {
                                    "D"
                                } else if symbol == "H" && *isotope == 3 {
                                    "T"
                                } else {
                                    symbol.as_str()
                                },
                            Alternative::Group { label } | Alternative::Unresolved { label } =>
                                label.as_str(),
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            if !elements {
                self.tree.set(node, "GenericNickname", label.clone())?;
            }
            self.text(
                node,
                &label,
                &TextFormat {
                    style: atom
                        .text_style
                        .clone()
                        .unwrap_or_else(|| self.doc.drawing_style.text_style()),
                    ..Default::default()
                },
                [("p", self.position(atom.position))],
            )?;
            return Ok(true);
        }
        if let Some(link) = self.doc.generic.links.iter().find(|s| s.atom == atom.id) {
            self.tree.set(node, "NodeType", "LinkNode")?;
            self.tree.set(node, "LinkCountLow", link.min.to_string())?;
            self.tree.set(node, "LinkCountHigh", link.max.to_string())?;
            if atom.explicit_h != 0 {
                self.tree
                    .set(node, "NumHydrogens", atom.explicit_h.to_string())?;
            }
            let label = atom
                .display
                .variable
                .clone()
                .unwrap_or_else(|| format!("[{}]{}-{}", atom.element, link.min, link.max));
            self.text(
                node,
                &label,
                &TextFormat {
                    style: atom
                        .text_style
                        .clone()
                        .unwrap_or_else(|| self.doc.drawing_style.text_style()),
                    ..Default::default()
                },
                [("p", self.position(atom.position))],
            )?;
            return Ok(true);
        }
        Ok(false)
    }
    fn alternative_fragment(
        &mut self,
        parent: Key,
        anchor: &Atom,
        choice: &Alternative,
    ) -> Result<()> {
        let fragment = crate::generic::alternative_fragment(choice).map_err(invalid)?;
        let id = self.id()?;
        let node = self.tree.add(Some(parent), "fragment", [("id", id)])?;
        let external = self.id()?;
        self.tree.add(
            Some(node),
            "n",
            [
                ("id", external.clone()),
                ("NodeType", "ExternalConnectionPoint".into()),
                ("Element", "0".into()),
                ("p", self.position(anchor.position.offset(-42., 0.))),
            ],
        )?;
        let mut ids = std::collections::HashMap::new();
        for atom in &fragment.atoms {
            let id = self.id()?;
            let number = crate::chemistry::ELEMENTS
                .iter()
                .position(|e| e.symbol == atom.element)
                .ok_or_else(|| invalid("Invalid alternative atom"))?;
            let n = self.tree.add(
                Some(node),
                "n",
                [
                    ("id", id.clone()),
                    ("Element", number.to_string()),
                    (
                        "p",
                        self.position(anchor.position.offset(atom.position.x, atom.position.y)),
                    ),
                ],
            )?;
            if atom.isotope > 0 {
                self.tree.set(n, "Isotope", atom.isotope.to_string())?;
            }
            if atom.charge != 0 {
                self.tree.set(n, "Charge", atom.charge.to_string())?;
            }
            if atom.explicit_h > 0 || atom.no_implicit {
                self.tree
                    .set(n, "NumHydrogens", atom.explicit_h.to_string())?;
            }
            ids.insert(atom.id, id);
        }
        let root = fragment
            .atoms
            .first()
            .ok_or_else(|| invalid("Empty alternative fragment"))?;
        let root = ids
            .get(&root.id)
            .ok_or_else(|| invalid("Missing alternative root"))?
            .clone();
        let id = self.id()?;
        self.tree.add(
            Some(node),
            "b",
            [
                ("id", id),
                ("B", external),
                ("E", root),
                ("Order", "1".into()),
            ],
        )?;
        for b in &fragment.bonds {
            let id = self.id()?;
            let order = match b.order {
                1 => "1",
                2 => "2",
                3 => "3",
                4 => "1.5",
                _ => return Err(invalid("Unsupported alternative bond order")),
            };
            self.tree.add(
                Some(node),
                "b",
                [
                    ("id", id),
                    (
                        "B",
                        ids.get(&b.a)
                            .ok_or_else(|| invalid("Missing alternative bond endpoint"))?
                            .clone(),
                    ),
                    (
                        "E",
                        ids.get(&b.b)
                            .ok_or_else(|| invalid("Missing alternative bond endpoint"))?
                            .clone(),
                    ),
                    ("Order", order.into()),
                ],
            )?;
        }
        Ok(())
    }
    pub(super) fn generic_bracket(
        &mut self,
        g: &crate::graphics::Graphic,
        z: usize,
    ) -> Result<bool> {
        if !self
            .doc
            .generic
            .repeats
            .iter()
            .any(|u| u.bracket == Some(g.id) || u.bracket_objects.contains(&g.id))
        {
            return Ok(false);
        }
        if g.sides != crate::graphics::BracketSides::Both {
            return Err(invalid(
                "Semantic single-sided brackets require native ReShiki export",
            ));
        }
        let id = self.id()?;
        let (lo, hi) = g.bounds();
        let stroke = self.color(g.style.stroke)?;
        let node = self.tree.add(
            Some(self.page),
            "graphic",
            [
                ("id", id),
                ("Z", z.to_string()),
                ("GraphicType", "Bracket".into()),
                (
                    "BracketType",
                    match g.kind {
                        crate::graphics::GraphicKind::Brackets => "SquarePair",
                        crate::graphics::GraphicKind::Parentheses => "RoundPair",
                        crate::graphics::GraphicKind::Braces => "CurlyPair",
                        _ => return Err(invalid("Invalid polymer bracket kind")),
                    }
                    .into(),
                ),
                (
                    "BoundingBox",
                    format!("{} {}", self.position(lo), self.position(hi)),
                ),
                ("color", stroke),
                ("LineWidth", super::number(super::real(g.style.width_pt))),
            ],
        )?;
        self.objects.push((g.id, node));
        Ok(true)
    }
    pub(super) fn generic_units(&mut self) -> Result<()> {
        let doc = self.doc;
        for unit in &doc.generic.repeats {
            let ordered = if unit.crossings.len() == 2 {
                Some(1)
            } else {
                None
            };
            if unit.head_count != ordered {
                return Err(invalid(
                    "This explicit or unspecified head-bond assignment requires MOL or native ReShiki export",
                ));
            }
            if let RepeatCount::Finite { min, max } = unit.count
                && min != max
                && RepeatCount::parse(&unit.label).map_err(invalid)? != unit.count
            {
                return Err(invalid(
                    "A ranged repeat label must retain its typed count; use native ReShiki export",
                ));
            }
            let graphic = unit.bracket.ok_or_else(|| {
                invalid("Chemical bracket export needs an associated bracket graphic")
            })?;
            let mut members = Vec::new();
            for &id in &unit.members {
                members.push(self.atom_xml(id)?);
            }
            let id = self.id()?;
            let node = self.tree.add(
                Some(self.page),
                "bracketedgroup",
                [
                    ("id", id),
                    ("BracketUsage", unit.usage.clone()),
                    ("BracketedObjectIDs", members.join(" ")),
                    (
                        "PolymerRepeatPattern",
                        match unit.pattern {
                            RepeatPattern::HeadToTail => "HeadToTail",
                            RepeatPattern::HeadToHead => "HeadToHead",
                            RepeatPattern::EitherUnknown => "EitherUnknown",
                        }
                        .into(),
                    ),
                    (
                        "PolymerFlipType",
                        if unit.flip.is_empty() {
                            "Unspecified".into()
                        } else {
                            unit.flip.clone()
                        },
                    ),
                    ("SRULabel", unit.label.clone()),
                ],
            )?;
            if let RepeatCount::Finite { min, max } = unit.count
                && min == max
            {
                self.tree.set(node, "RepeatCount", min.to_string())?;
            }
            let graphic_ids = std::iter::once(graphic)
                .chain(unit.bracket_objects.iter().copied())
                .collect::<Vec<_>>();
            for (i, graphic_id) in graphic_ids.into_iter().enumerate() {
                let key = self
                    .objects
                    .iter()
                    .find(|(id, _)| *id == graphic_id)
                    .map(|(_, k)| *k)
                    .ok_or_else(|| invalid("Missing polymer bracket graphic"))?;
                let graphic = self.tree.value(key, "id")?;
                let id = self.id()?;
                let attachment = self.tree.add(
                    Some(node),
                    "bracketattachment",
                    [("id", id), ("GraphicID", graphic)],
                )?;
                for (crossing_index, crossing) in unit.crossings.iter().enumerate() {
                    if unit
                        .crossing_brackets
                        .get(crossing_index)
                        .map_or(i == 0, |id| *id == graphic_id)
                    {
                        let index = self
                            .doc
                            .bonds
                            .iter()
                            .position(|b| {
                                (b.a == crossing.inside && b.b == crossing.outside)
                                    || (b.b == crossing.inside && b.a == crossing.outside)
                            })
                            .ok_or_else(|| invalid("Missing polymer crossing bond"))?;
                        let bond = *self
                            .bond_nodes
                            .get(index)
                            .ok_or_else(|| invalid("Missing exported crossing bond"))?;
                        let bond = self.tree.value(bond, "id")?;
                        let atom = self.atom_xml(crossing.inside)?;
                        let id = self.id()?;
                        self.tree.add(
                            Some(attachment),
                            "crossingbond",
                            [("id", id), ("BondID", bond), ("InnerAtomID", atom)],
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}
