//! Shared graph registry and exact, topology-indexed retained-parent matching.
use super::{
    assembly_parts::RetainedParentMetadata,
    locants::retained_locant_sort_key as parse_locant,
    molecule::{AtomSet, Molecule},
    nomenclature::section,
    retained_graph_model::{
        RetainedGraphAtomTemplate, RetainedGraphBondTemplate, RetainedGraphTemplate,
        RetainedGraphTemplateMatch,
    },
    rules::{elements, multipliers},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchOptions<'a> {
    pub include_disabled: bool,
    pub allow_nonaromatic: bool,
    pub allow_relocated_indicated_h: bool,
    pub pre_descriptor_only: bool,
    pub family: Option<&'a str>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TopologyKey {
    pub size: usize,
    pub symbols: BTreeMap<String, usize>,
    pub degrees: BTreeMap<usize, usize>,
    pub edge_degrees: BTreeMap<(usize, usize), usize>,
}
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained graph registry"
)]
static GRAPH_DATA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("data/retained_fused_graph_templates.json"))
        .expect("retained graph table")
});
#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained graph registry"
)]
static ALL_TEMPLATES: LazyLock<Vec<RetainedGraphTemplate>> = LazyLock::new(|| {
    let mut templates = Vec::new();
    for row in GRAPH_DATA["parents"]
        .as_array()
        .expect("retained graph rows")
    {
        templates.push(retained_graph_template_from_data(row));
    }
    for (data, family) in [
        (
            include_str!("data/retained_fused_hydrocarbon_templates.json"),
            "fused",
        ),
        (
            include_str!("data/retained_macrocycle_templates.json"),
            "macrocycle",
        ),
    ] {
        let data: Value = serde_json::from_str(data).expect("retained additional graph data");
        for row in data["parents"]
            .as_array()
            .expect("retained additional graph rows")
        {
            let mut row = row.clone();
            if let Some(t) = row["template"].as_object_mut() {
                t.entry("family").or_insert(json!(family));
                t.entry("charge_policy").or_insert(json!("exact"));
                if family == "macrocycle" {
                    t.entry("enforce_mancude_double_bonds")
                        .or_insert(json!(true));
                }
            }
            templates.push(retained_graph_template_from_data(&row));
        }
    }
    if let Some(rows) = section("retained_fused_polycycle_specs").as_array() {
        for row in rows.iter().filter(|r| r["template"].is_object()) {
            templates.push(retained_graph_template_from_data(row));
        }
    }
    for (family, minimum, maximum) in [("acenes", 4, 9), ("polyaphenes", 5, 10)] {
        for count in minimum..=maximum {
            let generated = generated_series_template(family, count);
            if !templates.iter().any(|t| t.name == generated.name) {
                templates.push(generated);
            }
        }
    }
    templates
});
static ENABLED_TEMPLATES: LazyLock<Vec<RetainedGraphTemplate>> = LazyLock::new(|| {
    ALL_TEMPLATES
        .iter()
        .filter(|t| t.enabled)
        .cloned()
        .collect()
});
static TOPOLOGY_INDEX: LazyLock<BTreeMap<TopologyKey, Vec<usize>>> = LazyLock::new(|| {
    let mut index = BTreeMap::<_, Vec<_>>::new();
    for (i, t) in ALL_TEMPLATES.iter().enumerate() {
        index
            .entry(retained_graph_template_topology_key(t))
            .or_default()
            .push(i);
    }
    index
});
pub fn retained_graph_templates(include_disabled: bool) -> &'static [RetainedGraphTemplate] {
    if include_disabled {
        &ALL_TEMPLATES
    } else {
        &ENABLED_TEMPLATES
    }
}
pub fn retained_parent_metadata(name: &str) -> Option<RetainedParentMetadata> {
    let t = ALL_TEMPLATES
        .iter()
        .find(|t| t.name == name)
        .or_else(|| ALL_TEMPLATES.iter().find(|t| t.output_name() == name))?;
    Some(t.metadata())
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn string(v: &Value, default: &str) -> String {
    v.as_str().unwrap_or(default).into()
}
fn boolean(v: &Value, default: bool) -> bool {
    v.as_bool().unwrap_or(default)
}
fn retained_graph_template_from_data(row: &Value) -> RetainedGraphTemplate {
    let original = &row["template"];
    let mut data = original["base_template"]
        .as_str()
        .and_then(|name| GRAPH_DATA["base_templates"][name].as_object())
        .cloned()
        .unwrap_or_default();
    if let Some(fields) = original.as_object() {
        data.extend(fields.clone());
    }
    let mut data = Value::Object(data);
    let locants = strings(data.get("locants").unwrap_or(&row["locants"]));
    if !data["atoms"].as_array().is_some_and(|v| !v.is_empty()) {
        let hetero: BTreeMap<_, _> = data["heteroatoms"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| {
                Some((
                    a["locant"].as_str()?.to_owned(),
                    a["symbol"].as_str()?.to_owned(),
                ))
            })
            .collect();
        let overrides: BTreeMap<_, _> = data["atom_overrides"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| Some((a["locant"].as_str()?.to_owned(), a.clone())))
            .collect();
        let fusions = strings(&data["fusion_atoms"]);
        let h = strings(&data["default_indicated_h"]);
        data["atoms"]=Value::Array(locants.iter().map(|l| {
            let mut a=json!({"locant":l,"symbol":hetero.get(l).map_or("C",String::as_str),"fusion":fusions.contains(l),"default_h":h.contains(l)});
            if let Some(over)=overrides.get(l).and_then(Value::as_object) && let Some(map)=a.as_object_mut(){map.extend(over.clone());}
            a
        }).collect());
    }
    let atoms: Vec<_> = data["atoms"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| RetainedGraphAtomTemplate {
            locant: string(&a["locant"], ""),
            symbol: string(&a["symbol"], "C"),
            charge: a["charge"].as_i64().unwrap_or(0) as i32,
            aromatic: boolean(&a["aromatic"], true),
            fusion: boolean(&a["fusion"], false),
            default_h: boolean(&a["default_h"], false),
            saturated: boolean(&a["saturated"], false),
            pi_capacity: a["pi_capacity"].as_u64().map(|n| n as usize),
            forced_single: boolean(&a["forced_single"], false),
            indicated_h_site: boolean(&a["indicated_h_site"], false),
            required_stereo: boolean(&a["required_stereo"], false),
        })
        .collect();
    let name = string(&row["name"], "");
    RetainedGraphTemplate {
        name,
        priority: row
            .get("priority")
            .unwrap_or(&data["priority"])
            .as_i64()
            .unwrap_or(1000),
        default_indicated_h: strings(
            row.get("default_indicated_h")
                .unwrap_or(&data["default_indicated_h"]),
        ),
        bonds: data["bonds"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|b| RetainedGraphBondTemplate {
                locants: (string(&b["locants"][0], ""), string(&b["locants"][1], "")),
                bond_class: string(&b["bond_class"], "aromatic"),
            })
            .collect(),
        fusion_atoms: strings(&data["fusion_atoms"]),
        locants,
        family: string(&data["family"], "fused"),
        aromatic_equivalence_policy: string(
            &data["aromatic_equivalence_policy"],
            "neutral_kekule_equivalent",
        ),
        charge_policy: string(
            &data["charge_policy"],
            if atoms.iter().all(|a| a.symbol == "C") {
                "exact"
            } else {
                "charge_layer"
            },
        ),
        enforce_mancude_double_bonds: boolean(&data["enforce_mancude_double_bonds"], false),
        enabled: boolean(
            data.get("enabled").unwrap_or(&row["template_enabled"]),
            false,
        ),
        derivative_production_enabled: boolean(&data["derivative_production_enabled"], false),
        implied_stereo: boolean(&data["implied_stereo"], false),
        mancude_double_bonds: data["mancude_double_bonds"].as_u64().map(|n| n as usize),
        indicated_hydrogen_count_override: data["indicated_hydrogen_count"]
            .as_u64()
            .map(|n| n as usize),
        pre_descriptor_selection: boolean(&data["pre_descriptor_selection"], false),
        atoms,
    }
}
pub(super) fn molecule_graph_topology_key(mol: &Molecule, atoms: &AtomSet) -> TopologyKey {
    let degrees: BTreeMap<_, _> = atoms
        .iter()
        .map(|&a| {
            (
                a,
                mol.get_neighbors(a)
                    .iter()
                    .filter(|n| atoms.contains(n))
                    .count(),
            )
        })
        .collect();
    let mut key = TopologyKey {
        size: atoms.len(),
        symbols: BTreeMap::new(),
        degrees: BTreeMap::new(),
        edge_degrees: BTreeMap::new(),
    };
    for a in atoms {
        *key.symbols.entry(mol.atoms[a].symbol.clone()).or_default() += 1;
        *key.degrees.entry(degrees[a]).or_default() += 1;
    }
    for b in mol
        .bonds
        .values()
        .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
    {
        let (a, b) = (degrees[&b.u], degrees[&b.v]);
        *key.edge_degrees.entry((a.min(b), a.max(b))).or_default() += 1;
    }
    key
}
pub(super) fn retained_graph_template_topology_key(t: &RetainedGraphTemplate) -> TopologyKey {
    let mut degrees: BTreeMap<_, usize> = t.locants.iter().map(|l| (l, 0)).collect();
    for b in &t.bonds {
        *degrees.entry(&b.locants.0).or_default() += 1;
        *degrees.entry(&b.locants.1).or_default() += 1;
    }
    let mut key = TopologyKey {
        size: t.atoms.len(),
        symbols: BTreeMap::new(),
        degrees: BTreeMap::new(),
        edge_degrees: BTreeMap::new(),
    };
    for a in &t.atoms {
        *key.symbols.entry(a.symbol.clone()).or_default() += 1;
        *key.degrees.entry(degrees[&a.locant]).or_default() += 1;
    }
    for b in &t.bonds {
        let (a, b) = (degrees[&b.locants.0], degrees[&b.locants.1]);
        *key.edge_degrees.entry((a.min(b), a.max(b))).or_default() += 1;
    }
    key
}
pub fn match_retained_graph_templates(
    mol: &Molecule,
    atoms: &AtomSet,
    allow_nonaromatic: bool,
) -> Vec<RetainedGraphTemplateMatch> {
    match_retained_graph_templates_with_options(
        mol,
        atoms,
        MatchOptions {
            allow_nonaromatic,
            ..Default::default()
        },
    )
}
pub fn match_retained_graph_templates_with_options(
    mol: &Molecule,
    atoms: &AtomSet,
    options: MatchOptions<'_>,
) -> Vec<RetainedGraphTemplateMatch> {
    // Upstream divergence: retained parent names and hydro prefixes only
    // describe single/double ring bonds. Neutral Kekule topology matching
    // otherwise accepts triples and drops their extra unsaturation, including
    // on the pre-descriptor route that precedes the production gate.
    if mol
        .bonds
        .values()
        .any(|b| atoms.contains(&b.u) && atoms.contains(&b.v) && !matches!(b.order, 1 | 2))
    {
        return Vec::new();
    }
    if options.family.is_none() {
        for family in ["fused", "macrocycle"] {
            let m = match_retained_graph_templates_with_options(
                mol,
                atoms,
                MatchOptions {
                    family: Some(family),
                    ..options
                },
            );
            if !m.is_empty() {
                return m;
            }
        }
        return Vec::new();
    }
    let key = molecule_graph_topology_key(mol, atoms);
    let mut candidates: Vec<_> = TOPOLOGY_INDEX
        .get(&key)
        .into_iter()
        .flatten()
        .map(|&i| ALL_TEMPLATES[i].clone())
        .collect();
    if options.family == Some("fused")
        && key.symbols.len() == 1
        && key.symbols.get("C") == Some(&key.size)
        && key.size >= 18
        && (key.size - 2).is_multiple_of(4)
    {
        let count = (key.size - 2) / 4;
        if count <= 1000
            && key.edge_degrees.values().sum::<usize>() + 1 == key.size + count
            && key.degrees.keys().all(|d| matches!(d, 2 | 3))
        {
            for (family, minimum, eager) in [("acenes", 4, 9), ("polyaphenes", 5, 10)] {
                if count >= minimum && count > eager {
                    let t = generated_series_template(family, count);
                    if retained_graph_template_topology_key(&t) == key
                        && !candidates.iter().any(|c| c.name == t.name)
                    {
                        candidates.push(t);
                    }
                }
            }
        }
    }
    let mut matches = Vec::new();
    for t in candidates.iter().filter(|t| {
        (options.include_disabled || t.enabled)
            && (!options.pre_descriptor_only || t.pre_descriptor_selection)
            && Some(t.family.as_str()) == options.family
    }) {
        if t.enforce_mancude_double_bonds
            && t.mancude_double_bonds.is_some_and(|n| {
                mol.bonds
                    .values()
                    .filter(|b| b.order == 2 && atoms.contains(&b.u) && atoms.contains(&b.v))
                    .count()
                    != n
            })
        {
            continue;
        }
        matches.extend(match_retained_graph_template_maps_with_options(
            mol,
            atoms,
            t,
            options.allow_nonaromatic,
            options.allow_relocated_indicated_h,
        ));
    }
    matches.sort_by_cached_key(|m| {
        (
            m.template.priority,
            m.indicated_h
                .iter()
                .map(|l| parse_locant(l))
                .collect::<Vec<_>>(),
            m.template
                .atoms
                .iter()
                .filter(|a| a.symbol != "C")
                .map(|a| parse_locant(&a.locant))
                .collect::<Vec<_>>(),
            m.template
                .fusion_atoms
                .iter()
                .map(|l| parse_locant(l))
                .collect::<Vec<_>>(),
            m.template.name.clone(),
            m.template
                .locants
                .iter()
                .map(|l| m.locant_to_atom[l])
                .collect::<Vec<_>>(),
        )
    });
    matches
}

pub fn is_saturated_site(mol: &Molecule, atom: usize, ring: &AtomSet) -> bool {
    mol.get_neighbors(atom)
        .iter()
        .filter(|n| ring.contains(n))
        .all(|&n| mol.get_bond(atom, n).is_none_or(|b| b.order == 1))
}
pub fn is_retained_oxo_site(mol: &Molecule, atom: usize) -> bool {
    mol.atoms[&atom].symbol == "C"
        && mol.get_neighbors(atom).iter().any(|n| {
            mol.atoms[n].symbol == "O" && mol.get_bond(atom, *n).is_some_and(|b| b.order == 2)
        })
}
pub fn relocatable_atom_by_locant(
    t: &RetainedGraphTemplate,
) -> BTreeMap<String, RetainedGraphAtomTemplate> {
    let movable: BTreeSet<_> = t
        .atoms
        .iter()
        .filter(|a| {
            !a.aromatic
                && !a.fusion
                && a.symbol != "C"
                && !elements::get(&a.symbol).is_some_and(|e| e.mancude_forced_single)
        })
        .map(|a| a.symbol.clone())
        .collect();
    t.atoms
        .iter()
        .map(|a| {
            let mut a = a.clone();
            if t.default_indicated_h.contains(&a.locant) && a.symbol == "C" {
                a.saturated = false;
                a.default_h = false;
            } else if movable.contains(&a.symbol) && !a.fusion {
                a.aromatic = false;
                a.default_h = false;
            }
            (a.locant.clone(), a)
        })
        .collect()
}
fn atom_matches(
    mol: &Molecule,
    idx: usize,
    a: &RetainedGraphAtomTemplate,
    ring: &AtomSet,
    nonaromatic: bool,
    charge: &str,
    fixed_hydrogenation: bool,
) -> bool {
    let atom = &mol.atoms[&idx];
    if atom.symbol != a.symbol
        || (charge == "exact" || (charge == "charge_layer" && a.charge != 0))
            && atom.charge != a.charge
    {
        return false;
    }
    if a.aromatic
        && !atom.is_aromatic
        && is_saturated_site(mol, idx, ring)
        && !nonaromatic
        && !is_retained_oxo_site(mol, idx)
    {
        return false;
    }
    let doubles = mol
        .get_neighbors(idx)
        .iter()
        .filter(|n| ring.contains(n) && mol.get_bond(idx, **n).is_some_and(|b| b.order == 2))
        .count();
    if doubles > 1
        || a.default_h && atom.explicit_h_count + atom.total_h_count == 0
        || a.saturated && !is_saturated_site(mol, idx, ring)
    {
        return false;
    }
    !(fixed_hydrogenation && !a.aromatic && !a.saturated && a.symbol != "C" && doubles > 0)
}
pub(super) fn match_retained_graph_template_maps_with_options(
    mol: &Molecule,
    atoms: &AtomSet,
    t: &RetainedGraphTemplate,
    nonaromatic: bool,
    relocate: bool,
) -> Vec<RetainedGraphTemplateMatch> {
    if atoms.len() != t.atoms.len()
        || t.atoms.iter().all(|a| !a.aromatic)
            && atoms.iter().any(|&a| !is_saturated_site(mol, a, atoms))
    {
        return Vec::new();
    }
    let by_locant = if relocate {
        relocatable_atom_by_locant(t)
    } else {
        t.atom_by_locant()
    };
    let index: BTreeMap<_, _> = t
        .locants
        .iter()
        .enumerate()
        .map(|(i, l)| (l.as_str(), i))
        .collect();
    let mut neighbors = vec![BTreeSet::new(); t.locants.len()];
    let mut bond_classes = BTreeMap::new();
    for b in &t.bonds {
        let (Some(&i), Some(&j)) = (
            index.get(b.locants.0.as_str()),
            index.get(b.locants.1.as_str()),
        ) else {
            return Vec::new();
        };
        neighbors[i].insert(j);
        neighbors[j].insert(i);
        bond_classes.insert((i.min(j), i.max(j)), b.bond_class.clone());
    }
    let fixed = t.atoms.iter().any(|a| a.saturated);
    let candidates: Vec<Vec<usize>> = t
        .locants
        .iter()
        .enumerate()
        .map(|(i, l)| {
            atoms
                .iter()
                .copied()
                .filter(|&a| {
                    mol.get_neighbors(a)
                        .iter()
                        .filter(|n| atoms.contains(n))
                        .count()
                        == neighbors[i].len()
                        && atom_matches(
                            mol,
                            a,
                            &by_locant[l],
                            atoms,
                            nonaromatic,
                            &t.charge_policy,
                            fixed,
                        )
                })
                .collect()
        })
        .collect();
    if candidates.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    let mut assignments = Vec::new();
    collect_assignments(
        mol,
        t,
        &neighbors,
        &bond_classes,
        &candidates,
        &mut vec![None; t.locants.len()],
        &mut AtomSet::new(),
        &mut assignments,
    );
    // Upstream divergence: neutral Kekule equivalence is not a proof for a
    // nonaromatic parent. A substituent or isotope can distinguish pi states
    // that its untyped graph automorphisms exchange. Without a declared bond
    // pattern for the emitted retained name, decline those ambiguous states.
    if t.aromatic_equivalence_policy == "neutral_kekule_equivalent"
        && t.atoms.iter().all(|a| a.symbol == "C" && a.aromatic)
        && atoms.iter().all(|a| !mol.atoms[a].is_aromatic)
        && atoms.iter().any(|a| {
            mol.atoms[a].isotope.is_some()
                || mol.get_neighbors(*a).iter().any(|n| !atoms.contains(n))
        })
        && t.mancude_double_bonds.is_some_and(|expected| {
            expected > 0
                && mol
                    .bonds
                    .values()
                    .filter(|b| b.order == 2 && atoms.contains(&b.u) && atoms.contains(&b.v))
                    .count()
                    == expected
        })
    {
        let pi_states: BTreeSet<Vec<usize>> = assignments
            .iter()
            .map(|assignment| {
                t.bonds
                    .iter()
                    .map(|b| {
                        mol.get_bond(
                            assignment[index[b.locants.0.as_str()]],
                            assignment[index[b.locants.1.as_str()]],
                        )
                        .map_or(0, |bond| bond.order)
                    })
                    .collect()
            })
            .collect();
        if pi_states.len() > 1 {
            return Vec::new();
        }
    }
    let mut order: Vec<_> = (0..t.locants.len()).collect();
    order.sort_by_key(|&i| {
        (
            std::cmp::Reverse(neighbors[i].len()),
            by_locant[&t.locants[i]].symbol == "C",
            t.locants[i].clone(),
        )
    });
    assignments.sort_by_key(|assignment| order.iter().map(|&i| assignment[i]).collect::<Vec<_>>());
    let derive = !t.default_indicated_h.is_empty() || t.indicated_hydrogen_count() == 0;
    assignments
        .into_iter()
        .filter_map(|assignment| {
            if t.implied_stereo
                && t.atoms.iter().filter(|a| a.required_stereo).any(|a| {
                    let x = &mol.atoms[&assignment[index[a.locant.as_str()]]];
                    x.stereo.is_none() && x.raw_stereo.is_none()
                })
            {
                return None;
            }
            let indicated = if relocate {
                relocated_indicated_h(mol, t, atoms, &assignment, false)?
            } else if !derive {
                relocated_indicated_h(mol, t, atoms, &assignment, true)
                    .unwrap_or_else(|| t.default_indicated_h.clone())
            } else {
                t.default_indicated_h.clone()
            };
            let locant_to_atom: BTreeMap<_, _> =
                t.locants.iter().cloned().zip(assignment).collect();
            Some(RetainedGraphTemplateMatch {
                template: t.clone(),
                atom_to_locant: locant_to_atom
                    .iter()
                    .map(|(l, &a)| (a, l.clone()))
                    .collect(),
                locant_to_atom,
                matched_atoms: atoms.clone(),
                indicated_h: indicated,
            })
        })
        .collect()
}
fn relocated_indicated_h(
    mol: &Molecule,
    t: &RetainedGraphTemplate,
    atoms: &AtomSet,
    assignment: &[usize],
    strict: bool,
) -> Option<Vec<String>> {
    let by = t.atom_by_locant();
    let saturated: Vec<_> = t
        .locants
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            !elements::get(&by[*l].symbol).is_some_and(|e| e.mancude_forced_single)
                && is_saturated_site(mol, assignment[*i], atoms)
        })
        .map(|(i, l)| (l.clone(), assignment[i]))
        .collect();
    let needed = t.indicated_hydrogen_count();
    if saturated.len() < needed {
        return None;
    }
    let mut movable: Vec<_> = saturated
        .iter()
        .filter(|(_, a)| !is_retained_oxo_site(mol, *a))
        .map(|(l, _)| l.clone())
        .collect();
    if movable.len() < needed {
        movable = saturated.into_iter().map(|(l, _)| l).collect();
    }
    if movable.len() < needed || strict && movable.len() != needed {
        return None;
    }
    movable.sort_by_key(|l| parse_locant(l));
    movable.truncate(needed);
    Some(movable)
}
#[allow(clippy::too_many_arguments)]
fn collect_assignments(
    mol: &Molecule,
    t: &RetainedGraphTemplate,
    neighbors: &[BTreeSet<usize>],
    classes: &BTreeMap<(usize, usize), String>,
    candidates: &[Vec<usize>],
    assignment: &mut [Option<usize>],
    used: &mut AtomSet,
    matches: &mut Vec<Vec<usize>>,
) {
    if matches.len() >= 256 {
        return;
    }
    if used.len() == assignment.len() {
        matches.push(assignment.iter().flatten().copied().collect());
        return;
    }
    let remaining: Vec<_> = (0..assignment.len())
        .filter(|&i| assignment[i].is_none())
        .collect();
    let frontier: Vec<_> = remaining
        .iter()
        .copied()
        .filter(|&i| neighbors[i].iter().any(|&n| assignment[n].is_some()))
        .collect();
    let mut best = None;
    for &i in if frontier.is_empty() {
        &remaining
    } else {
        &frontier
    } {
        let options: Vec<_> = candidates[i]
            .iter()
            .copied()
            .filter(|a| {
                !used.contains(a)
                    && assignment.iter().enumerate().all(|(j, assigned)| {
                        assigned.is_none_or(|b| {
                            let expected = neighbors[i].contains(&j);
                            let bond = mol.get_bond(*a, b);
                            expected == bond.is_some()
                                && (!expected
                                    || t.aromatic_equivalence_policy != "exact"
                                    || bond.is_some_and(|b| {
                                        b.order
                                            == if classes
                                                .get(&(i.min(j), i.max(j)))
                                                .is_some_and(|s| s == "double")
                                            {
                                                2
                                            } else {
                                                1
                                            }
                                    }))
                        })
                    })
            })
            .collect();
        if options.is_empty() {
            return;
        }
        let key = (
            options.len(),
            std::cmp::Reverse(
                neighbors[i]
                    .iter()
                    .filter(|&&n| assignment[n].is_some())
                    .count(),
            ),
            std::cmp::Reverse(neighbors[i].len()),
            t.locants[i].clone(),
        );
        if best.as_ref().is_none_or(|(old, _, _)| key < *old) {
            best = Some((key, i, options));
        }
    }
    if let Some((_, i, options)) = best {
        for a in options {
            assignment[i] = Some(a);
            used.insert(a);
            collect_assignments(
                mol, t, neighbors, classes, candidates, assignment, used, matches,
            );
            used.remove(&a);
            assignment[i] = None;
        }
    }
}

#[allow(
    clippy::expect_used,
    reason = "Immutable checked-in retained series data"
)]
fn generated_series_template(family: &str, count: usize) -> RetainedGraphTemplate {
    static SERIES: LazyLock<Value> = LazyLock::new(|| {
        serde_json::from_str(include_str!("data/retained_fused_series.json"))
            .expect("retained fused series")
    });
    let (locants, edges) = if family == "acenes" {
        higher_acene_locant_graph(count)
    } else {
        polyaphene_locant_graph(count)
    };
    let overrides = &SERIES[family]["member_overrides"][count.to_string()];
    let name = format!(
        "{}{}",
        multipliers::basic(count),
        if family == "acenes" { "cene" } else { "phene" }
    );
    let fusion_atoms: Vec<_> = locants
        .iter()
        .filter(|l| !l.chars().all(|c| c.is_ascii_digit()))
        .cloned()
        .collect();
    RetainedGraphTemplate {
        name,
        priority: overrides["priority"].as_i64().unwrap_or(1000),
        atoms: locants
            .iter()
            .map(|l| RetainedGraphAtomTemplate {
                locant: l.clone(),
                symbol: "C".into(),
                aromatic: true,
                fusion: fusion_atoms.contains(l),
                ..Default::default()
            })
            .collect(),
        bonds: edges
            .iter()
            .map(|e| RetainedGraphBondTemplate {
                locants: e.clone(),
                bond_class: "aromatic".into(),
            })
            .collect(),
        fusion_atoms,
        locants,
        family: "fused".into(),
        aromatic_equivalence_policy: "neutral_kekule_equivalent".into(),
        charge_policy: "exact".into(),
        enabled: boolean(&overrides["enabled"], true),
        derivative_production_enabled: boolean(&overrides["derivative_production_enabled"], true),
        mancude_double_bonds: Some(2 * count + 1),
        pre_descriptor_selection: true,
        ..Default::default()
    }
}
fn higher_acene_locant_graph(count: usize) -> (Vec<String>, Vec<(String, String)>) {
    let left: Vec<_> = (4..count + 3).map(|n| format!("{n}a")).collect();
    let right_start = count + 6;
    let right: Vec<_> = (right_start..2 * count + 5)
        .map(|n| format!("{n}a"))
        .collect();
    let mut locants: Vec<_> = (1..=4).map(|n| n.to_string()).collect();
    locants.push(left[0].clone());
    for n in 5..count + 3 {
        locants.push(n.to_string());
        locants.push(format!("{n}a"));
    }
    locants.extend((count + 3..=right_start).map(|n| n.to_string()));
    locants.push(right[0].clone());
    for n in right_start + 1..2 * count + 5 {
        locants.push(n.to_string());
        locants.push(format!("{n}a"));
    }
    let mut edges: Vec<_> = (0..locants.len())
        .map(|i| (locants[i].clone(), locants[(i + 1) % locants.len()].clone()))
        .collect();
    edges.extend(left.into_iter().rev().zip(right));
    (locants, edges)
}
fn polyaphene_locant_graph(count: usize) -> (Vec<String>, Vec<(String, String)>) {
    let maximum = 2 * count + 4;
    let omitted = (count + 7) / 2;
    let bay = 3 * count / 2 + 6;
    let first: Vec<_> = (4..count + 4).filter(|&n| n != omitted).collect();
    let second: Vec<_> = (count + 7..=maximum).collect();
    let mut locants = Vec::new();
    for n in 1..=maximum {
        locants.push(n.to_string());
        if first.contains(&n) || second.contains(&n) {
            locants.push(format!("{n}a"));
        }
        if n == bay {
            locants.push(format!("{n}b"));
        }
    }
    let mut edges: Vec<_> = (0..locants.len())
        .map(|i| (locants[i].clone(), locants[(i + 1) % locants.len()].clone()))
        .collect();
    let targets: Vec<_> = locants
        .iter()
        .filter(|l| **l == format!("{bay}b") || second.iter().any(|n| **l == format!("{n}a")))
        .cloned()
        .collect();
    edges.extend(
        first
            .into_iter()
            .rev()
            .map(|n| format!("{n}a"))
            .zip(targets),
    );
    (locants, edges)
}
#[cfg(test)]
mod tests {
    use super::super::graph_io::read_smiles;
    use super::*;
    fn matches(smiles: &str, options: MatchOptions<'_>) -> Vec<RetainedGraphTemplateMatch> {
        let mol = read_smiles(smiles).unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        match_retained_graph_templates_with_options(&mol, &atoms, options)
    }
    #[test]
    fn registry_expands_all_declared_locanted_graphs() {
        let templates = retained_graph_templates(true);
        assert!(templates.len() > 250);
        for t in templates {
            assert_eq!(t.locants.len(), t.atoms.len(), "{}", t.name);
            let locants: BTreeSet<_> = t.locants.iter().collect();
            assert_eq!(locants.len(), t.locants.len(), "{}", t.name);
            assert_eq!(
                locants,
                t.atoms.iter().map(|a| &a.locant).collect(),
                "{}",
                t.name
            );
            assert!(
                t.bonds
                    .iter()
                    .all(|b| locants.contains(&b.locants.0) && locants.contains(&b.locants.1)),
                "{}",
                t.name
            );
            assert!(!t.bonds.is_empty(), "{}", t.name);
        }
        assert!(
            templates
                .iter()
                .any(|t| t.family == "macrocycle" && t.name == "porphyrin")
        );
        assert!(templates.iter().any(|t| t.name == "tetracene"));
        assert!(templates.iter().any(|t| t.name == "decaphene"));
    }
    #[test]
    fn retained_graphs_accept_kekule_forms_and_all_numberings() {
        for (smiles, name, count) in [
            ("c1ccc2ccccc2c1", "naphthalene", 4),
            ("C1=CC=C2C=CC=CC2=C1", "naphthalene", 4),
            ("c1ccc2ncccc2c1", "quinoline", 1),
            ("c1ccc2[nH]ccc2c1", "1H-indole", 1),
            ("C1C2CC3CC1CC(C2)C3", "adamantane", 24),
            ("C12C3C4C1C5C2C3C45", "cubane", 48),
        ] {
            let rows = matches(smiles, MatchOptions::default());
            assert_eq!(
                rows.first().map(|m| m.template.name.as_str()),
                Some(name),
                "{smiles}"
            );
            assert_eq!(
                rows.iter().filter(|m| m.template.name == name).count(),
                count,
                "{smiles}"
            );
        }
    }
    #[test]
    fn exact_charges_and_graph_connectivity_are_not_relaxed() {
        assert!(
            matches(
                "[C+]12C3C4C1C5C2C3C45",
                MatchOptions {
                    allow_nonaromatic: true,
                    ..Default::default()
                }
            )
            .is_empty()
        );
        assert!(matches("C1CC2CCC1CC2", MatchOptions::default()).is_empty());
        let rows = matches("c1ccc2CCCc2c1", MatchOptions::default());
        assert_eq!(
            rows.first().map(|m| m.template.name.as_str()),
            Some("indane")
        );
    }
    #[test]
    fn retained_fused_names_do_not_erase_ring_triples_or_cumulated_doubles() {
        for (smiles, legacy_name) in [
            ("C1=CC=C2C(=C1)C3=CC#CC=C3C2", None),
            ("C1=CC=C2C=CC#CC2=C1", None),
            ("C1=CC=C2NC#CC2=C1", None),
            (
                "C1=CC=C2C(=C1)C3=C=CC=CC3C2",
                Some("tricyclo[7.4.0.0^{2,7}]trideca-1(13),2,3,5,9,11-hexaene"),
            ),
            (
                "C1=CC=C2C=C=CCC2=C1",
                Some("bicyclo[4.4.0]deca-1(10),2,3,6,8-pentaene"),
            ),
        ] {
            for (allow_nonaromatic, allow_relocated_indicated_h) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                assert!(
                    matches(
                        smiles,
                        MatchOptions {
                            family: Some("fused"),
                            allow_nonaromatic,
                            allow_relocated_indicated_h,
                            ..Default::default()
                        }
                    )
                    .is_empty(),
                    "{smiles}"
                );
            }
            let name = super::super::name_smiles(smiles);
            if let Some(expected) = legacy_name {
                assert_eq!(name.unwrap(), expected);
            } else {
                assert!(name.is_err(), "{smiles}");
            }
        }
        assert!(super::super::name_smiles("C1=CC=C2C(=C1)C3=CC#CC=C3C2(CCO)CCO").is_err());

        let mol = read_smiles("C#Cc1ccc2ccccc2c1").unwrap();
        let ring = mol
            .atoms
            .iter()
            .filter(|(_, atom)| atom.is_aromatic)
            .map(|(&idx, _)| idx)
            .collect();
        assert_eq!(
            match_retained_graph_templates(&mol, &ring, false)
                .first()
                .map(|m| m.template.name.as_str()),
            Some("naphthalene")
        );
        assert_eq!(
            super::super::name_smiles("C#Cc1ccc2ccccc2c1").as_deref(),
            Ok("2-ethynylnaphthalene")
        );
    }
    #[test]
    fn decorated_nonaromatic_parent_requires_unambiguous_pi_state() {
        let mol = read_smiles("ClC1=CC=CC2=CC=CC=CC2=C1").unwrap();
        let ring: AtomSet = mol
            .atoms
            .iter()
            .filter(|(_, a)| a.symbol == "C")
            .map(|(&i, _)| i)
            .collect();
        assert!(match_retained_graph_templates(&mol, &ring, false).is_empty());
        assert_eq!(
            matches("C1=CC=CC2=CC=CC=CC2=C1", MatchOptions::default())
                .first()
                .map(|m| m.template.name.as_str()),
            Some("heptalene")
        );

        let aromatic = read_smiles("ClC1=CC=CC2=CC=CC=C12").unwrap();
        let ring: AtomSet = aromatic
            .atoms
            .iter()
            .filter(|(_, a)| a.symbol == "C")
            .map(|(&i, _)| i)
            .collect();
        assert_eq!(
            match_retained_graph_templates(&aromatic, &ring, false)
                .first()
                .map(|m| m.template.name.as_str()),
            Some("naphthalene")
        );
    }
    #[test]
    fn preferred_output_names_have_metadata_and_aliases_do_not() {
        assert!(retained_parent_metadata("2,3-dihydro-1H-indole").is_some());
        assert!(retained_parent_metadata("indoline").is_some());
        assert!(retained_parent_metadata("isoindole").is_none());
    }
}
