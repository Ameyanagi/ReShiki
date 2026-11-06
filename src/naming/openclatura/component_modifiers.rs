//! Ester alcohol fragments and principal-group N substituents.
use super::{
    assembly_parts::{AssemblyParts, SubstituentItem},
    assembly_prefixes::substituent_sort_key,
    component_group_rules::ester_or_peroxy_single_oxygen,
    formatting::strip_outer_parentheses,
    graph_kernel::{Edge, normalize_edge},
    locants::parse_locant,
    molecule::{AtomSet, Molecule, bond_ids_within},
    namer::{name_subgraph_rendered, name_subgraph_with_upstream},
    perception::{LigandRole, PerceivedGroup},
    subgraph_tools::subgraph_component,
};
use crate::naming::{NameReason, NameStatus};
use std::collections::{BTreeMap, BTreeSet};

struct FrontModifier {
    name: String,
    attachment: usize,
    ester_center: Option<usize>,
}

#[derive(Default)]
pub struct ComponentFrontModifiers {
    modifiers: Vec<FrontModifier>,
    mixed_diester: bool,
    acid_atoms: AtomSet,
    pub consumed: AtomSet,
    pub component_splits: BTreeSet<Edge>,
}

impl ComponentFrontModifiers {
    pub fn acid_isotopes(&self, source: Option<&Molecule>) -> Vec<(usize, String, u16)> {
        source
            .filter(|_| self.mixed_diester)
            .map_or_else(Vec::new, |source| {
                source
                    .atoms
                    .values()
                    .filter_map(|atom| {
                        let mass = atom.isotope?;
                        let anchor = if atom.symbol == "H" {
                            source.get_neighbors(atom.idx).first().copied()?
                        } else {
                            atom.idx
                        };
                        self.acid_atoms.contains(&anchor).then_some((
                            anchor,
                            atom.symbol.clone(),
                            mass,
                        ))
                    })
                    .collect()
            })
    }

    pub fn numbering_modifiers(&self) -> BTreeMap<usize, String> {
        if self.mixed_diester {
            self.modifiers
                .iter()
                .map(|modifier| (modifier.attachment, modifier.name.clone()))
                .collect()
        } else {
            BTreeMap::new()
        }
    }

    pub fn add_to_parts(
        &self,
        mol: &Molecule,
        parts: &mut AssemblyParts,
        locants: &BTreeMap<usize, String>,
    ) {
        for modifier in &self.modifiers {
            parts.front_modifiers.push(modifier.name.clone());
            parts
                .front_modifier_locants
                .push(locants.get(&modifier.attachment).cloned());
        }
        if self.mixed_diester {
            let sites: Vec<_> = self
                .modifiers
                .iter()
                .filter_map(|modifier| modifier.ester_center)
                .collect();
            let acid = subgraph_component(mol, sites[0], &self.consumed);
            parts.ester_site_exchange_proven =
                super::locant_elision::ester_sites_are_equivalent(mol, &acid, &sites);
        }
    }
}

pub fn collect_component_front_modifiers(
    mol: &Molecule,
    groups: &[PerceivedGroup],
    key: Option<&str>,
    exclude: &AtomSet,
) -> Result<ComponentFrontModifiers, String> {
    let mut collected = ComponentFrontModifiers::default();
    let Some(first) = groups.iter().find(|g| Some(g.key.as_str()) == key) else {
        return Ok(collected);
    };
    if !first
        .resolved_rule
        .as_ref()
        .is_some_and(|r| r.has_capability("front_modifier"))
    {
        return Ok(collected);
    }
    for group in groups.iter().filter(|g| Some(g.key.as_str()) == key) {
        let Some(linker) = ester_or_peroxy_single_oxygen(mol, group) else {
            continue;
        };
        let Some(&root) = mol
            .get_neighbors(linker)
            .iter()
            .find(|i| !group.atoms_involved.contains(i))
        else {
            continue;
        };
        let mut blocked = exclude.clone();
        blocked.insert(linker);
        let branch = name_subgraph_with_upstream(mol, root, &blocked, Some(linker))?;
        let mut modifier = strip_outer_parentheses(&branch);
        if let Some(d) = &group.descriptor {
            let linkers = d.ligands_with_role(LigandRole::CarbonLink);
            let doubles = d.ligands_with_role(LigandRole::DoubleBonded);
            if !linkers.is_empty()
                && !doubles.is_empty()
                && d.ligands.iter().any(|l| l.element != "O")
            {
                let element_locant = d
                    .ligands
                    .iter()
                    .filter(|l| l.role != LigandRole::DoubleBonded)
                    .map(|l| l.element.as_str())
                    .collect::<String>();
                if super::formatting::is_complex_prefix(&modifier) {
                    modifier = super::formatting::format_multiplier(&modifier, 1, true);
                }
                modifier = format!("{element_locant}-{modifier}");
            }
        }
        collected
            .consumed
            .extend(subgraph_component(mol, root, &blocked));
        let ester_center = group
            .descriptor
            .as_ref()
            .filter(|descriptor| {
                matches!(group.key.as_str(), "ester" | "ring_carboxylate")
                    && descriptor.family == "acyl"
                    && descriptor.derivative == "ester"
                    && descriptor.centers.len() == 1
                    && descriptor
                        .ligands
                        .iter()
                        .all(|ligand| ligand.element == "O")
            })
            .and_then(|descriptor| descriptor.centers.first().copied());
        collected.modifiers.push(FrontModifier {
            name: modifier,
            attachment: group.attachment_carbon,
            ester_center,
        });
        // P-65.6.3.2.1 / P-65.6.3.3.7.2.1: only rendered ester modifiers split.
        collected
            .component_splits
            .insert(normalize_edge(linker, root));
    }
    if collected.modifiers.len() == 2
        && groups
            .iter()
            .filter(|group| Some(group.key.as_str()) == key)
            .count()
            == 2
        && collected
            .modifiers
            .iter()
            .all(|modifier| modifier.ester_center.is_some())
        && collected.modifiers[0].name != collected.modifiers[1].name
    {
        let sites: AtomSet = collected
            .modifiers
            .iter()
            .filter_map(|modifier| modifier.ester_center)
            .collect();
        collected.acid_atoms =
            subgraph_component(mol, first.attachment_carbon, &collected.consumed);
        let acid_sites: AtomSet = super::perception::perceive_groups(mol)
            .iter()
            .filter_map(|group| group.descriptor.as_ref())
            .filter(|descriptor| {
                descriptor.family == "acyl"
                    && matches!(descriptor.derivative.as_str(), "acid" | "ester")
            })
            .flat_map(|descriptor| descriptor.centers.iter().copied())
            .filter(|center| collected.acid_atoms.contains(center))
            .collect();
        collected.mixed_diester = sites.len() == 2 && acid_sites == sites;
    }
    Ok(collected)
}

pub struct NumberingNSubstituents {
    items: Vec<SubstituentItem>,
    sites: BTreeMap<String, usize>,
    pub consumed: AtomSet,
}

impl NumberingNSubstituents {
    pub fn numbering_modifiers(&self) -> BTreeMap<usize, Vec<String>> {
        let mut names: BTreeMap<_, Vec<_>> = self
            .sites
            .values()
            .map(|&carbon| (carbon, Vec::new()))
            .collect();
        for item in &self.items {
            for locant in &item.locants {
                names
                    .entry(self.sites[locant])
                    .or_default()
                    .push(item.name.clone());
            }
        }
        names
    }

    pub fn add_to_parts(
        mut self,
        parts: &mut AssemblyParts,
        locants: &BTreeMap<usize, String>,
    ) -> AtomSet {
        let mut sites: Vec<_> = self.sites.values().copied().collect();
        sites.sort_by_key(|carbon| parse_locant(&locants[carbon]));
        let assigned: BTreeMap<_, _> = sites
            .iter()
            .enumerate()
            .map(|(index, &carbon)| (carbon, format!("N{}", "'".repeat(index))))
            .collect();
        for item in &mut self.items {
            for locant in &mut item.locants {
                *locant = assigned[&self.sites[locant]].clone();
            }
        }
        for item in self.items {
            super::subgraph_tools::add_substituent(parts, item);
        }
        self.consumed
    }
}

/// Precollect ordinary amine/amide branches without detaching their nitrogen
/// labels from the corresponding parent carbon. Other nitrogen suffix systems
/// retain their specialized local ordering below.
pub fn collect_numbering_n_substituents(
    mol: &Molecule,
    groups: &[PerceivedGroup],
    key: Option<&str>,
    path: &[usize],
    exclude: &AtomSet,
) -> Result<Option<NumberingNSubstituents>, String> {
    let Some(key @ ("amine" | "amide" | "ring_amide")) = key else {
        return Ok(None);
    };
    let selected: Vec<_> = groups
        .iter()
        .filter(|group| group.key == key && path.contains(&group.attachment_carbon))
        .collect();
    if selected.len() < 2
        || selected
            .iter()
            .map(|group| group.attachment_carbon)
            .collect::<AtomSet>()
            .len()
            != selected.len()
        || selected.iter().any(|group| {
            group
                .atoms_involved
                .iter()
                .filter(|atom| mol.atoms[atom].symbol == "N")
                .count()
                != 1
        })
    {
        return Ok(None);
    }
    let locants: BTreeMap<_, _> = path
        .iter()
        .enumerate()
        .map(|(index, &atom)| (atom, (index + 1).to_string()))
        .collect();
    let mut parts = AssemblyParts::new(path.len());
    let consumed =
        add_component_n_substituents(mol, &mut parts, groups, Some(key), path, &locants, exclude)?;
    let sites = path
        .iter()
        .filter(|carbon| {
            selected
                .iter()
                .any(|group| group.attachment_carbon == **carbon)
        })
        .enumerate()
        .map(|(index, &carbon)| (format!("N{}", "'".repeat(index)), carbon))
        .collect();
    Ok(Some(NumberingNSubstituents {
        items: parts.substituents,
        sites,
        consumed,
    }))
}

fn nitrogen_rank(
    mol: &Molecule,
    n: usize,
    c: usize,
    exclude: &AtomSet,
) -> Result<(usize, (String, String), isize), String> {
    let mut blocked = exclude.clone();
    blocked.insert(n);
    let status = mol.name_status();
    let names = mol
        .get_neighbors(n)
        .iter()
        .copied()
        .filter(|r| *r != c && mol.atoms[r].symbol != "H")
        .map(|r| {
            name_subgraph_with_upstream(mol, r, &blocked, Some(n)).map(|s| substituent_sort_key(&s))
        })
        .collect::<Result<Vec<_>, _>>();
    // These provisional names only rank the nitrogens; their text is not emitted.
    mol.set_name_status(status);
    let names = names?;
    Ok(if let Some(first) = names.iter().min() {
        (0, first.clone(), -(names.len() as isize))
    } else {
        (1, Default::default(), 0)
    })
}

pub fn n_substituent_locant(
    key: &str,
    group_count: usize,
    n_count: usize,
    n_index: usize,
    global: usize,
    group_index: usize,
) -> String {
    if key == "hydrazine" {
        return if n_index == 0 { "N" } else { "N'" }.into();
    }
    let primes = if matches!(key, "amidine" | "ring_amidine") && group_count > 1 {
        n_index * group_count + group_index
    } else if super::nomenclature::RULES
        .functional_groups
        .get(key)
        .is_some_and(|r| r.families.contains("hydrazone"))
    {
        group_index
    } else if group_count == 1 && n_count == 1 {
        0
    } else {
        global
    };
    format!("N{}", "'".repeat(primes))
}

pub fn add_component_n_substituents(
    mol: &Molecule,
    parts: &mut AssemblyParts,
    groups: &[PerceivedGroup],
    key: Option<&str>,
    path: &[usize],
    locants: &BTreeMap<usize, String>,
    exclude: &AtomSet,
) -> Result<AtomSet, String> {
    let mut consumed = AtomSet::new();
    let Some(key) = key else {
        return Ok(consumed);
    };
    let mut selected: Vec<_> = groups
        .iter()
        .filter(|g| g.key == key && path.contains(&g.attachment_carbon))
        .collect();
    selected.sort_by_key(|g| parse_locant(&locants[&g.attachment_carbon]));
    let mut global = 0;
    for (gi, group) in selected.iter().enumerate() {
        let carbon = group.attachment_carbon;
        let Some(rule) = &group.resolved_rule else {
            continue;
        };
        let mut nitrogens: Vec<_> = group
            .atoms_involved
            .iter()
            .copied()
            .filter(|n| mol.atoms[n].symbol == "N")
            .collect();
        nitrogens.sort_by_key(|&n| mol.get_bond(n, carbon).is_none());
        if rule.has_capability("urea") {
            let ranks = nitrogens
                .iter()
                .map(|&n| nitrogen_rank(mol, n, carbon, exclude).map(|r| (n, r)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            nitrogens.sort_by_key(|n| ranks[n].clone());
        }
        let core = group
            .atoms_involved
            .iter()
            .copied()
            .find(|c| {
                mol.atoms[c].is_carbon() && nitrogens.iter().any(|&n| mol.get_bond(*c, n).is_some())
            })
            .unwrap_or(carbon);
        let mut guanidine = BTreeMap::new();
        if matches!(key, "amidine" | "ring_amidine") {
            nitrogens.sort_by_key(|&n| mol.bond_order(n, core));
        } else if rule.has_capability("hydrazide") {
            let centers = group
                .descriptor
                .as_ref()
                .map(|d| d.centers.as_slice())
                .unwrap_or(std::slice::from_ref(&core));
            nitrogens.sort_by_key(|&n| !centers.iter().any(|&c| mol.get_bond(n, c).is_some()));
        } else if key == "guanidine" {
            let imino = nitrogens
                .iter()
                .copied()
                .find(|&n| mol.get_bond(n, core).is_some_and(|b| b.order == 2))
                .ok_or("openclatura: guanidine imino nitrogen missing")?;
            let mut amines: Vec<_> = nitrogens.iter().copied().filter(|&n| n != imino).collect();
            let ranks = amines
                .iter()
                .map(|&n| nitrogen_rank(mol, n, core, exclude).map(|r| (n, r)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            amines.sort_by_key(|n| ranks[n].clone());
            if amines.len() != 2 {
                return Err("openclatura: guanidine topology not ported".into());
            }
            guanidine.extend([
                (amines[0], "1".into()),
                (imino, "2".into()),
                (amines[1], "3".into()),
            ]);
        }
        let hydrazone = rule.families.contains("hydrazone");
        for (ni, &n) in nitrogens.iter().enumerate() {
            let roots: Vec<_> = mol
                .get_neighbors(n)
                .iter()
                .copied()
                .filter(|r| {
                    *r != carbon
                        && !group.atoms_involved.contains(r)
                        && mol.atoms[r].symbol != "H"
                        && !(hydrazone
                            && mol.atoms[r].is_carbon()
                            && mol.get_bond(n, *r).is_some_and(|b| b.order == 2))
                })
                .collect();
            let loc = guanidine.get(&n).cloned().unwrap_or_else(|| {
                n_substituent_locant(key, selected.len(), nitrogens.len(), ni, global, gi)
            });
            for root in roots {
                let mut blocked = exclude.clone();
                blocked.insert(n);
                let status = mol.name_status();
                let mut rendered = name_subgraph_rendered(mol, root, &blocked, Some(n), false)?;
                if mol.get_bond(n, root).is_some_and(|b| b.order == 2)
                    && mol.atoms[&root].symbol == "N"
                    && mol
                        .get_neighbors(root)
                        .iter()
                        .all(|&r| r == n || mol.atoms[&r].symbol == "H")
                {
                    mol.set_name_status(status);
                    rendered.text = "imino".into();
                }
                let atoms = subgraph_component(mol, root, &blocked);
                let mut bond_atoms = atoms.clone();
                bond_atoms.insert(n);
                let mut item = SubstituentItem::new(rendered.text, vec![loc.clone()]);
                item.outer_parentheses_optional = rendered.outer_parentheses_optional;
                item.bond_ids = bond_ids_within(mol, &bond_atoms);
                item.atom_ids = atoms.clone();
                item.charge_atom_ids = atoms
                    .iter()
                    .copied()
                    .filter(|i| mol.atoms[i].charge != 0)
                    .collect();
                consumed.extend(atoms);
                if hydrazone
                    && parts
                        .parent_atom_symbols_by_locant
                        .values()
                        .any(|s| s == "N")
                {
                    item.locants.clear();
                    parts.principal_suffix_modifiers.push(item);
                } else {
                    // Di- and polyamides cite the parent locant on each
                    // nitrogen, N1, N3 (P-66.1.1.3.1.1), never N or N'; an
                    // ethanediamide is marked as the unretained oxamide.
                    if selected.len() > 1
                        && path.len() > 2
                        && rule.has_capability("amide_like")
                        && !rule.has_capability("urea")
                        && !rule.has_capability("hydrazide")
                    {
                        mol.mark_name(NameStatus::NotPreferred(NameReason::DiamideLocants));
                    }
                    super::subgraph_tools::add_substituent(parts, item);
                }
            }
            global += 1;
        }
    }
    Ok(consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::openclatura::graph_io::read_smiles;

    #[test]
    fn mixed_diester_collection_requires_two_fully_esterified_acid_sites() {
        for (smiles, omitted_center, expected) in [
            ("CCOC(=O)CC(=O)OC", None, true),
            ("CCOC(=O)CC(=O)O", None, false),
            ("CCOC(=O)C(C(=O)OC)C(=O)OC", None, false),
            ("CCOC(=O)C(CCOC(=O)C)C(=O)OC", Some(9), false),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let mut groups = super::super::perception::perceive_groups(&mol);
            groups.retain(|group| Some(group.attachment_carbon) != omitted_center);
            let collected =
                collect_component_front_modifiers(&mol, &groups, Some("ester"), &AtomSet::new())
                    .unwrap();
            assert_eq!(collected.mixed_diester, expected, "{smiles}");
        }
    }

    #[test]
    fn nitrogen_ranking_discards_provisional_branch_status() {
        let mol = read_smiles("NC(=O)NS(=O)(=O)CC(=O)O").unwrap();
        let exclude = AtomSet::from([0, 1, 2]);
        let mut blocked = exclude.clone();
        blocked.insert(3);
        name_subgraph_with_upstream(&mol, 4, &blocked, Some(3)).unwrap();
        assert_eq!(
            mol.name_status(),
            NameStatus::NotPreferred(NameReason::SulfonylPrefix)
        );
        let previous = NameStatus::Unverified(NameReason::UnauditedParent);
        mol.set_name_status(previous);
        nitrogen_rank(&mol, 3, 1, &exclude).unwrap();
        assert_eq!(mol.name_status(), previous);
    }
}
