//! Retained fused-parent production gate and derivative admission policies.
use super::{
    assembly_parts::{RetainedParentMetadata, SubstituentItem},
    molecule::{AtomSet, Molecule},
    perception::PerceivedGroup,
    retained_derivative_hydrogen::prove_retained_oxo_carbon_hydrogen,
    retained_fused_templates::{
        MatchOptions, is_saturated_site, match_retained_graph_templates_with_options,
    },
    retained_graph_model::RetainedGraphTemplateMatch,
    retained_name_policy::retained_parent_name_policy,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in parser capability snapshot"
)]
static GATE: LazyLock<Value> = LazyLock::new(|| {
    let data: Value = serde_json::from_str(include_str!("data/parser_grammar_snapshot.json"))
        .expect("retained production gate");
    data["retained_fused_derivative_gate"].clone()
});
#[derive(Clone, Debug)]
pub struct ProductionRetainedFusedParent {
    pub name: String,
    pub locant_maps: Vec<BTreeMap<usize, String>>,
    pub metadata: RetainedParentMetadata,
}
#[allow(clippy::too_many_arguments)]
pub fn production_retained_fused_parent(
    mol: &Molecule,
    parent_path: &[usize],
    component_atoms: &AtomSet,
    perceived_groups: &[PerceivedGroup],
    principal_key: Option<&str>,
    substituent_mapping: &BTreeMap<usize, Vec<SubstituentItem>>,
    attachment_atom: Option<usize>,
) -> Result<Option<ProductionRetainedFusedParent>, String> {
    let parent_atoms: AtomSet = parent_path.iter().copied().collect();
    if component_atoms.iter().any(|a| mol.atoms[a].charge != 0)
        || parent_atoms.iter().any(|a| {
            mol.atoms[a].charge != 0
                || mol.get_neighbors(*a).iter().any(|n| {
                    parent_atoms.contains(n)
                        && mol
                            .get_bond(*a, *n)
                            .is_none_or(|b| !matches!(b.order, 1 | 2))
                })
        })
    {
        return Ok(None);
    }
    if !GATE["allowed_principal_keys"]
        .as_array()
        .is_some_and(|keys| keys.iter().any(|k| k.as_str() == principal_key))
    {
        return Ok(None);
    }
    if perceived_groups
        .iter()
        .filter(|g| parent_atoms.contains(&g.attachment_carbon))
        .any(|g| {
            !g.resolved_rule
                .as_ref()
                .is_some_and(|r| r.has_capability("promotes_member_groups"))
                && !listed("allowed_group_keys", &g.key)
        })
    {
        return Ok(None);
    }
    // SubstituentItem's ported shape contains rendered, already verified items;
    // spiro substituents are rejected before this parent-production boundary.
    let mut features: AtomSet = substituent_mapping.keys().copied().collect();
    features.extend(attachment_atom);
    features.extend(
        perceived_groups
            .iter()
            .filter(|g| parent_atoms.contains(&g.attachment_carbon))
            .map(|g| g.attachment_carbon),
    );
    let strict = match_retained_graph_templates_with_options(
        mol,
        &parent_atoms,
        MatchOptions {
            family: Some("fused"),
            ..Default::default()
        },
    );
    let strict_present = !strict.is_empty();
    let mut matches = eligible(mol, &parent_atoms, strict, principal_key)?;
    if strict_present && matches.is_empty() {
        return Ok(None);
    }
    if matches.is_empty() {
        let mut pooled = Vec::new();
        let mut seen = BTreeSet::new();
        for (nonaromatic, relocated) in [(true, false), (false, true), (true, true)] {
            let found = match_retained_graph_templates_with_options(
                mol,
                &parent_atoms,
                MatchOptions {
                    allow_nonaromatic: nonaromatic,
                    allow_relocated_indicated_h: relocated,
                    family: Some("fused"),
                    ..Default::default()
                },
            );
            for m in eligible(mol, &parent_atoms, found, principal_key)? {
                if seen.insert((m.template.name.clone(), m.atom_to_locant.clone())) {
                    pooled.push(m);
                }
            }
        }
        if let Some(best) = pooled
            .iter()
            .find(|m| !m.indicated_h.is_empty())
            .or_else(|| pooled.first())
        {
            let name = best.template.name.clone();
            let indicated = best.indicated_h.clone();
            matches = pooled
                .into_iter()
                .filter(|m| m.template.name == name && m.indicated_h == indicated)
                .collect();
        }
    }
    let Some(first) = matches.first() else {
        return Ok(None);
    };
    let parent_name = &first.template.name;
    let maps: Vec<_> = matches
        .iter()
        .filter(|m| {
            &m.template.name == parent_name
                && features.iter().all(|a| {
                    m.atom_to_locant
                        .get(a)
                        .is_some_and(|l| !l.chars().any(char::is_alphabetic))
                })
        })
        .map(|m| m.atom_to_locant.clone())
        .collect();
    if maps.is_empty() || !added_hydrogen_is_citable(mol, first) {
        return Ok(None);
    }
    let context =
        if attachment_atom.is_none() && substituent_mapping.is_empty() && principal_key.is_none() {
            "unsubstituted_parent"
        } else {
            "composite_parent"
        };
    let name = retained_parent_name_policy(parent_name)
        .map_or_else(|| parent_name.clone(), |p| p.output_name(context));
    let mut metadata = first.template.metadata();
    metadata.inherent_saturated_locants.clear();
    metadata.default_indicated_h = first.indicated_h.clone();
    metadata.relocated_indicated_h = first.indicated_h != first.template.default_indicated_h;
    Ok(Some(ProductionRetainedFusedParent {
        name,
        locant_maps: maps,
        metadata,
    }))
}
fn listed(key: &str, value: &str) -> bool {
    GATE[key]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|v| v.as_str() == Some(value)))
}
fn eligible(
    mol: &Molecule,
    parent_atoms: &AtomSet,
    matches: Vec<RetainedGraphTemplateMatch>,
    principal_key: Option<&str>,
) -> Result<Vec<RetainedGraphTemplateMatch>, String> {
    let mut matches = matches;
    if principal_key == Some("ketone") {
        for m in &mut matches {
            if let Some(proof) =
                prove_retained_oxo_carbon_hydrogen(mol, &m.template, &m.locant_to_atom, &[])?
            {
                m.indicated_h = proof.indicated_h;
            }
        }
    }
    matches.sort_by_key(|m| m.indicated_h.is_empty());
    Ok(matches
        .into_iter()
        .filter(|m| {
            listed("production_parent_names", &m.template.name)
                && m.template.derivative_production_enabled
                && (principal_key != Some("ketone")
                    || has_mancude_unsaturation(mol, parent_atoms, m)
                    || saturation_is_hydro_citable(mol, m))
                && added_hydrogen_is_citable(mol, m)
        })
        .collect())
}
pub fn added_hydrogen_is_citable(mol: &Molecule, m: &RetainedGraphTemplateMatch) -> bool {
    let by = m.template.atom_by_locant();
    let mut positions = 0usize;
    let mut saturated = 0usize;
    for (&a, l) in &m.atom_to_locant {
        if !by[l].aromatic {
            continue;
        }
        positions += 1;
        if mol
            .get_neighbors(a)
            .iter()
            .any(|n| m.matched_atoms.contains(n))
            && is_saturated_site(mol, a, &m.matched_atoms)
        {
            saturated += 1;
        }
    }
    if saturated > 0 && saturated >= positions {
        return false;
    }
    let excluded = m.indicated_h.iter().filter(|l| !by[*l].aromatic).count();
    let surplus =
        saturated as isize - (m.template.indicated_hydrogen_count() as isize - excluded as isize);
    surplus <= 0 || surplus % 2 == 0 || indicated_h_retained_name(&m.template.name)
}
fn indicated_h_retained_name(name: &str) -> bool {
    // The source set combines explicit monocycle names with the declared
    // indicated_hydrogen_name column; no inference from an alias is needed.
    static DATA: LazyLock<Value> = LazyLock::new(|| {
        serde_json::from_str(include_str!("data/retained_fused_graph_templates.json"))
            .unwrap_or(Value::Null)
    });
    DATA["parents"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|r| r["indicated_hydrogen_name"].as_str() == Some(name))
    })
}
fn has_mancude_unsaturation(
    mol: &Molecule,
    atoms: &AtomSet,
    m: &RetainedGraphTemplateMatch,
) -> bool {
    let Some(expected) = m.template.mancude_double_bonds else {
        return false;
    };
    let carbonyls = m
        .template
        .atoms
        .iter()
        .filter(|a| {
            a.symbol == "C"
                && !a.aromatic
                && mol
                    .get_neighbors(m.locant_to_atom[&a.locant])
                    .iter()
                    .any(|n| {
                        !atoms.contains(n)
                            && mol.atoms[n].symbol == "O"
                            && mol
                                .get_bond(m.locant_to_atom[&a.locant], *n)
                                .is_some_and(|b| b.order == 2)
                    })
        })
        .count();
    mol.bonds
        .values()
        .filter(|b| b.order == 2 && (atoms.contains(&b.u) || atoms.contains(&b.v)))
        .count()
        >= expected + carbonyls
}
fn saturation_is_hydro_citable(mol: &Molecule, m: &RetainedGraphTemplateMatch) -> bool {
    if m.template.mancude_double_bonds.is_none() {
        return false;
    }
    let by = m.template.atom_by_locant();
    let saturated = m
        .atom_to_locant
        .iter()
        .filter(|(a, l)| {
            by[*l].aromatic
                && mol
                    .get_neighbors(**a)
                    .iter()
                    .any(|n| m.matched_atoms.contains(n))
                && is_saturated_site(mol, **a, &m.matched_atoms)
        })
        .count();
    saturated > 0 && saturated < m.atom_to_locant.len()
}
