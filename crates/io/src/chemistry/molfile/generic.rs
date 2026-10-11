//! V3000 generic records retain finite choices and explicit repeat boundaries.
use super::{Error, invalid};
use crate::{
    document::Document,
    generic::{Alternative, RepeatCount, RepeatPattern},
};
use std::collections::HashMap;
pub(super) fn restore_output(mut mol: String, doc: &Document) -> Result<String, Error> {
    if doc.generic.is_empty() {
        return Ok(mol);
    }
    let ids: HashMap<_, _> = doc
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i + 1))
        .collect();
    let id = |source: u64| {
        ids.get(&source)
            .copied()
            .ok_or_else(|| invalid("Missing generic atom ID"))
    };
    let mut lines = mol.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut in_atoms = false;
    for line in &mut lines {
        if line == "M  V30 BEGIN ATOM" {
            in_atoms = true;
            continue;
        }
        if line == "M  V30 END ATOM" {
            in_atoms = false;
            continue;
        }
        if !in_atoms {
            continue;
        }
        let Some(data) = line.strip_prefix("M  V30 ") else {
            continue;
        };
        let fields = data.split_whitespace().collect::<Vec<_>>();
        let Some(n) = fields.first().and_then(|s| s.parse::<usize>().ok()) else {
            continue;
        };
        if let Some(site) = doc
            .generic
            .alternatives
            .iter()
            .find(|s| ids.get(&s.atom) == Some(&n))
        {
            let mut symbols = Vec::new();
            for choice in &site.choices {
                match choice {
                    Alternative::Element { symbol, isotope } => {
                        symbols.push(match (symbol.as_str(), *isotope) {
                            ("H", 2) => "D",
                            ("H", 3) => "T",
                            (_, 0) => symbol.as_str(),
                            _ => {
                                return Err(invalid(
                                    "MOL element-list isotope requires CDXML or native ReShiki",
                                ));
                            }
                        })
                    }
                    _ => {
                        return Err(invalid(
                            "Anonymous fragment alternatives require CDXML, CDX or native ReShiki",
                        ));
                    }
                }
            }
            let symbol = format!(
                "{}[{}]",
                if site.excluded { "NOT" } else { "" },
                symbols.join(",")
            );
            let rest = fields.iter().skip(2).copied().collect::<Vec<_>>().join(" ");
            *line = format!("M  V30 {n} {symbol} {rest}");
        }
    }
    let mut records = Vec::new();
    for link in &doc.generic.links {
        let contacts = doc
            .bonds
            .iter()
            .filter(|b| b.a == link.atom || b.b == link.atom)
            .collect::<Vec<_>>();
        let [a, b] = contacts.as_slice() else {
            return Err(invalid("A link node requires exactly two bonds"));
        };
        let other =
            |bond: &crate::document::Bond| id(if bond.a == link.atom { bond.b } else { bond.a });
        let repeated = id(link.atom)?;
        records.push(format!(
            "M  V30 LINKNODE {} {} 2 {repeated} {} {repeated} {}",
            link.min,
            link.max,
            other(a)?,
            other(b)?
        ));
    }
    if !doc.generic.repeats.is_empty() {
        for line in &mut lines {
            if let Some(rest) = line.strip_prefix("M  V30 COUNTS ") {
                let fields = rest.split_whitespace().collect::<Vec<_>>();
                if let [a, b, _, objects, chirality] = fields.as_slice() {
                    *line = format!(
                        "M  V30 COUNTS {a} {b} {} {objects} {chirality}",
                        doc.generic.repeats.len()
                    );
                } else {
                    return Err(invalid("Invalid generated V3000 counts"));
                }
            }
        }
        records.push("M  V30 BEGIN SGROUP".into());
        for (i, unit) in doc.generic.repeats.iter().enumerate() {
            if !unit.crossing_brackets.is_empty()
                && unit
                    .crossing_brackets
                    .iter()
                    .any(|id| unit.bracket != Some(*id))
            {
                return Err(invalid(
                    "Crossing-bond bracket associations require CDXML, CDX or native ReShiki",
                ));
            }
            if !matches!(unit.flip.as_str(), "" | "Unspecified" | "NoFlip") {
                return Err(invalid(
                    "Polymer flip semantics require CDXML or native ReShiki",
                ));
            }
            let kind = match unit.usage.as_str() {
                "SRU" => "SRU",
                "Monomer" => "MON",
                "Mer" => "MER",
                "Copolymer" | "CopolymerAlternating" | "CopolymerRandom" | "CopolymerBlock" => {
                    "COP"
                }
                "Crosslink" => "CRO",
                "Graft" => "GRA",
                "Modification" => "MOD",
                "MultipleGroup" => "MUL",
                "MixtureUnordered" => "MIX",
                "MixtureOrdered" => "FOR",
                "Component" => "COM",
                "Generic" => "GEN",
                _ => {
                    return Err(invalid(
                        "This polymer usage requires CDXML or native ReShiki",
                    ));
                }
            };
            let members = unit
                .members
                .iter()
                .map(|&source| id(source).map(|n| n.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let mut bonds = Vec::new();
            for c in &unit.crossings {
                let n = doc
                    .bonds
                    .iter()
                    .position(|b| {
                        (b.a == c.inside && b.b == c.outside)
                            || (b.b == c.inside && b.a == c.outside)
                    })
                    .ok_or_else(|| invalid("Missing polymer crossing bond"))?;
                bonds.push((n + 1).to_string());
            }
            let label = unit.label.replace('"', "\"\"");
            let multiplier = if kind == "MUL" {
                match unit.count {
                    RepeatCount::Finite { min, max } if min == max => Some(min),
                    _ => {
                        return Err(invalid(
                            "MOL multiple groups require one finite multiplicity",
                        ));
                    }
                }
            } else {
                if RepeatCount::parse(&unit.label).map_err(invalid)? != unit.count {
                    return Err(invalid(
                        "MOL repeat labels must retain their typed repeat count; use CDXML or native ReShiki",
                    ));
                }
                None
            };
            let connect = match unit.pattern {
                RepeatPattern::HeadToTail => "HT",
                RepeatPattern::HeadToHead => "HH",
                RepeatPattern::EitherUnknown => "EU",
            };
            let mut record = format!(
                "M  V30 {} {kind} 0 ATOMS=({} {}) XBONDS=({} {}) CONNECT={connect} LABEL=\"{label}\"",
                i + 1,
                members.len(),
                members.join(" "),
                bonds.len(),
                bonds.join(" ")
            );
            if let Some(n) = multiplier {
                record.push_str(&format!(" MULT={n}"));
            }
            if let Some(n) = unit.head_count {
                record.push_str(&format!(
                    " XBHEAD=({n} {})",
                    bonds.iter().take(n).cloned().collect::<Vec<_>>().join(" ")
                ));
            }
            if let Some(subtype) = match unit.usage.as_str() {
                "CopolymerAlternating" => Some("ALT"),
                "CopolymerRandom" => Some("RAN"),
                "CopolymerBlock" => Some("BLO"),
                _ => None,
            } {
                record.push_str(&format!(" SUBTYPE={subtype}"));
            }
            records.push(record);
        }
        records.push("M  V30 END SGROUP".into());
    }
    let index = lines
        .iter()
        .position(|s| s == "M  V30 END CTAB")
        .ok_or_else(|| invalid("Missing generated V3000 CTAB"))?;
    lines.splice(index..index, records);
    mol = lines.join("\n");
    mol.push('\n');
    Ok(mol)
}
