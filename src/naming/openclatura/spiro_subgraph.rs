//! Shared component planning at a graph-proved spiro boundary.
use super::{
    assembly_parts::AssemblyParts,
    assembly_spiro::{SpiroAssembly, spiro_assembly_from_parts},
    component_namer::{build_spiro_component_parts, render_component_parent},
    graph_kernel::biconnected_edge_components,
    molecule::{AtomSet, Molecule},
};
use std::collections::{BTreeSet, VecDeque};

pub fn require_single_parent_attachment(
    mol: &Molecule,
    component: &AtomSet,
    parent: &AtomSet,
) -> Result<(), String> {
    let attachments: AtomSet = component
        .iter()
        .flat_map(|&atom| mol.get_neighbors(atom))
        .filter(|atom| parent.contains(atom))
        .copied()
        .collect();
    if attachments.len() > 1 {
        // Upstream divergence: a partial polycyclic parent can expose the same
        // bridge at two sites. Naming it as two detachable groups duplicates
        // atoms, so it must have its own complete skeleton proof instead.
        return Err(
            "openclatura: substituent component has multiple parent attachment atoms".into(),
        );
    }
    Ok(())
}

pub fn find_spiro_side_pair(
    mol: &Molecule,
    junction: usize,
    neighbors: &[usize],
    main: &AtomSet,
    exclude: &AtomSet,
) -> Option<(usize, usize)> {
    for (i, &first) in neighbors.iter().enumerate() {
        for &second in &neighbors[i + 1..] {
            let mut visited = AtomSet::from([junction]);
            let mut queue = VecDeque::from([first]);
            while let Some(current) = queue.pop_front() {
                if current == second {
                    return Some((first, second));
                }
                if !visited.insert(current) {
                    continue;
                }
                for &next in mol.get_neighbors(current) {
                    if !visited.contains(&next) && !main.contains(&next) && !exclude.contains(&next)
                    {
                        queue.push_back(next);
                    }
                }
            }
        }
    }
    None
}
pub fn spiro_side_component(
    mol: &Molecule,
    junction: usize,
    start: usize,
    main: &AtomSet,
    exclude: &AtomSet,
) -> AtomSet {
    let mut component = AtomSet::new();
    let mut visited = AtomSet::from([junction]);
    let mut queue = VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        if !component.insert(current) {
            continue;
        }
        visited.insert(current);
        for &next in mol.get_neighbors(current) {
            if !visited.contains(&next) && !main.contains(&next) && !exclude.contains(&next) {
                queue.push_back(next);
            }
        }
    }
    component.insert(junction);
    component
}
pub fn polycyclic_side_core(
    mol: &Molecule,
    side_atoms: &AtomSet,
    junction: usize,
) -> Option<AtomSet> {
    let cores: Vec<_> = biconnected_edge_components(mol, side_atoms)
        .into_iter()
        .filter_map(|edges| {
            let atoms: AtomSet = edges.iter().flat_map(|&(a, b)| [a, b]).collect();
            (atoms.contains(&junction) && edges.len() > atoms.len()).then_some(atoms)
        })
        .collect();
    (cores.len() == 1).then(|| cores[0].clone())
}
fn has_retained_fused_parent(mol: &Molecule, core: &AtomSet) -> bool {
    super::retained_fused_templates::match_retained_graph_templates(mol, core, true)
        .iter()
        .any(|m| !m.template.name.contains('['))
}
fn project_spiro_side_molecule(mol: &Molecule, side_atoms: &AtomSet, junction: usize) -> Molecule {
    let mut side = mol.subgraph(side_atoms);
    let atom = &mol.atoms[&junction];
    let neighbors: BTreeSet<_> = mol.get_neighbors(junction).iter().copied().collect();
    if atom.is_carbon()
        && atom.charge == 0
        && !atom.is_aromatic
        && atom.total_h_count == 0
        && atom.explicit_h_count == 0
        && neighbors.intersection(side_atoms).count() == 2
        && neighbors.difference(side_atoms).count() == 2
        && neighbors
            .iter()
            .all(|&n| mol.get_bond(junction, n).is_some_and(|b| b.order == 1))
        && let Some(atom) = side.atoms.get_mut(&junction)
    {
        atom.total_h_count = 2;
    }
    side
}
pub fn plan_graph_spiro_side(
    mol: &Molecule,
    side_atoms: &AtomSet,
    junction: usize,
    required_core: Option<&AtomSet>,
    prefix_only: bool,
) -> Result<Option<SpiroAssembly>, String> {
    if !side_atoms.contains(&junction) {
        return Ok(None);
    }
    let side_mol = project_spiro_side_molecule(mol, side_atoms, junction);
    let Some(mut plan) =
        build_spiro_component_parts(&side_mol, side_atoms, junction, required_core, prefix_only)?
    else {
        return Ok(None);
    };
    let parts = &mut plan.parts;
    let locants = &parts.parent_atom_ids_by_locant;
    if locants.values().copied().collect::<AtomSet>() != parts.parent_atom_ids
        || locants.len() != parts.parent_atom_ids.len()
    {
        return Ok(None);
    }
    let Some(junction_locant) = locants
        .iter()
        .find_map(|(l, &a)| (a == junction).then_some(l.clone()))
    else {
        return Ok(None);
    };
    for item in &mut parts.substituents {
        let Some(side) = item.spiro.as_deref() else {
            continue;
        };
        if side.side_parts.is_some() || item.locants.len() != 1 {
            continue;
        }
        let Some(&nested_junction) = locants.get(&item.locants[0]) else {
            return Ok(None);
        };
        let mut nested_atoms = item.atom_ids.clone();
        nested_atoms.insert(nested_junction);
        let Some(mut nested) =
            plan_graph_spiro_side(&side_mol, &nested_atoms, nested_junction, None, false)?
        else {
            return Ok(None);
        };
        nested.parent_locant = item.locants[0].clone();
        item.spiro = Some(Box::new(nested));
    }
    let mut render = |parts: &mut AssemblyParts| {
        render_component_parent(&side_mol, parts, &plan.numbered_path, &plan.locant_map)
    };
    let result = spiro_assembly_from_parts(parts, &junction_locant, Some(&mut render))?;
    if result.is_none() && parts.principal_group.is_some() && !prefix_only {
        return plan_graph_spiro_side(mol, side_atoms, junction, required_core, true);
    }
    Ok(result)
}
pub fn plan_substituted_fusion_spiro_side(
    mol: &Molecule,
    side_atoms: &AtomSet,
    junction: usize,
) -> Result<Option<SpiroAssembly>, String> {
    let Some(core) = polycyclic_side_core(mol, side_atoms, junction) else {
        return Ok(None);
    };
    if has_retained_fused_parent(mol, &core) {
        return Ok(None);
    }
    plan_graph_spiro_side(mol, side_atoms, junction, Some(&core), false)
}
pub fn spiro_subgraph_assembly(
    mol: &Molecule,
    junction: usize,
    side_atoms: &AtomSet,
) -> Result<SpiroAssembly, String> {
    // Upstream divergence: retained-N-ring and aromatic side-ring shortcut
    // success paths outside the stage-four coverage use this graph projection.
    // The accepted covered paths all select the component pipeline below.
    if let Some(side) = plan_substituted_fusion_spiro_side(mol, side_atoms, junction)? {
        return Ok(side);
    }
    plan_graph_spiro_side(mol,side_atoms,junction,None,false)?.ok_or_else(||"openclatura: spiro side component has no complete graph-numbered parent containing its junction".into())
}

#[cfg(test)]
mod tests {
    use super::super::molecule::{Atom, Bond};
    use super::*;
    #[test]
    fn neutral_junction_cap_restores_isolated_hydride_state() {
        let mut mol = Molecule::new();
        for atom in 0..5 {
            mol.add_atom(Atom::new(atom, "C")).unwrap();
        }
        for (id, (a, b)) in [(0, 1), (1, 2), (2, 0), (0, 3), (3, 4), (4, 0)]
            .into_iter()
            .enumerate()
        {
            mol.add_bond(Bond::new(id, a, b, 1)).unwrap();
        }
        let side = project_spiro_side_molecule(&mol, &AtomSet::from([0, 1, 2]), 0);
        assert_eq!(side.atoms[&0].total_h_count, 2);
        assert_eq!(mol.atoms[&0].total_h_count, 0);
        assert_eq!(
            find_spiro_side_pair(&mol, 0, &[3, 4], &AtomSet::from([0, 1, 2]), &AtomSet::new()),
            Some((3, 4))
        );
    }
    #[test]
    fn graph_sides_keep_bridged_parent_names_and_side_group_locants() {
        for (smiles, expected) in [
            (
                "CN1CC11C2C3CC2C13",
                "1-methylspiro[aziridine-2,3'-tricyclo[2.2.0.0^{2,5}]hexane]",
            ),
            (
                "CC1CC11CC2CCC12",
                "2'-methylspiro[bicyclo[2.2.0]hexane-2,1'-cyclopropane]",
            ),
            (
                "OC1CC11C2CC1(O)C2",
                "spiro[bicyclo[1.1.1]pentane-2,1'-cyclopropane]-1,2'-diol",
            ),
        ] {
            assert_eq!(
                super::super::name_smiles(smiles).as_deref(),
                Ok(expected),
                "{smiles}"
            );
        }
    }
    #[test]
    fn fallback_parent_cannot_name_the_same_bridge_as_two_methyl_groups() {
        let smiles = "CC1C2CCC3C4(CCCC5(C4C(OC5)O)C)COC(=O)C3(C2)C1=O";
        assert!(
            super::super::name_smiles(smiles)
                .is_err_and(|error| error.contains("multiple parent attachment atoms"))
        );
    }
    #[test]
    fn fused_parent_can_keep_a_complete_spiro_descriptor_as_its_side() {
        assert_eq!(
            super::super::name_smiles("C1COC2=CC3=C(C=C21)C=C(C34CCC5(CC4)OCCO5)Br").as_deref(),
            Ok(
                "6-bromo-2,3-dihydro-1',4'-dioxaspiro[cyclopenta[f]1-benzofuran-7,8'-(spiro[4.5]decane)]"
            )
        );
    }
    #[test]
    fn spiro_parent_derivatives_use_the_systematic_fusion_proof() {
        assert_eq!(
            super::super::name_smiles(
                "CC1=CC(=C(C=C1)OCC(=O)N2CCC3(CC2)CC(=O)C4=C(O3)C=C(C(=C4)C)C)C(C)(C)C"
            )
            .as_deref(),
            Ok(
                "1'-(2-((2-(tert-butyl)-4-methylphenyl)oxy)acetyl)-6,7-dimethylspiro[benzo[b]pyran-2,4'-piperidine]-4-one"
            )
        );
    }
}
