//! Native SMILES adapter for openclatura's plain graph model.
//!
//! Input identity checks remain those of the naming subsystem. In particular,
//! this adapter does not use upstream's unsanitized fallback to bypass them.
use super::graph_kernel::biconnected_edge_components;
use super::molecule::{Atom, AtomSet, Bond, Molecule};
use reshiki_chemistry::{
    ELEMENTS, kekulize, ranking, smiles,
    stereo::{cip::label, perception},
};
use std::collections::{BTreeMap, VecDeque};

/// Read one supported graph, retaining source atom order and aromatic identity.
pub fn read_smiles(text: &str) -> Result<Molecule, String> {
    // This is deliberately checked on the original text, before preparation
    // can clean winding, remove hydrogen atoms or normalize unsupported input.
    let canonical = super::super::canonical_smiles(text)?;
    if canonical.contains('.') {
        return Err("Openclatura requires a single connected molecule".into());
    }
    let prepared = smiles::prepare(text).map_err(|error| error.to_string())?;
    let mut state = prepared.state;
    // Upstream divergence: aromatic atom endpoints cannot identify localized
    // inter-ring bonds; retain bond aromaticity for the final fusion pi audit.
    let aromatic_bonds: Vec<_> = state.graph.bonds.iter().map(|b| b.aromatic).collect();
    let atom_metadata = state
        .graph
        .atoms
        .iter()
        .zip(&state.valences)
        .map(|(a, v)| {
            (
                a.aromatic,
                usize::from(a.explicit_hydrogens),
                usize::from(a.explicit_hydrogens) + v.implicit_hydrogens as usize,
            )
        })
        .collect::<Vec<_>>();

    // Preserve the aromatic/H snapshot before asking for the same ordinary
    // Kekule single/double graph consumed by upstream naming rules.
    // The upstream environment's Kekulize defaults to canonical atom ranking.
    let ranks = if state.graph.atoms.iter().any(|a| a.aromatic)
        || state.graph.bonds.iter().any(|b| b.aromatic)
    {
        Some(ranking::rank_cached(
            &state.graph,
            &state.rings.atoms,
            &state.metadata,
            ranking::Options {
                fragment: true,
                ..ranking::Options::default()
            },
            &state.valences,
        )?)
    } else {
        None
    };
    let kekule = kekulize::assign(
        &state.graph,
        &state.rings.atoms,
        &state.directions,
        kekulize::Options {
            ranks: ranks.as_deref(),
            ..kekulize::Options::default()
        },
    )?;
    state.graph = kekule.graph;
    state.directions = kekule.directions;
    state.valences = state.graph.valences()?;
    let legacy = perception::perceive(
        &state,
        perception::Options {
            clean: true,
            force: true,
            flag_possible: true,
        },
    )?;

    let has_stereo = legacy.metadata.atoms.iter().any(|a| a.chiral_tag != 0)
        || legacy.metadata.bonds.iter().any(|b| b.stereo != 0);
    let accurate = if has_stereo {
        Some(
            label::assign(&legacy, &label::Options::default())
                .map_err(|error| error.to_string())?
                .state,
        )
    } else {
        None
    };
    let mut mol = Molecule::new();
    mol.legacy_cip = atom_cip(&legacy);
    mol.accurate_cip = accurate.as_ref().map_or_else(BTreeMap::new, atom_cip);

    for (idx, atom) in legacy.graph.atoms.iter().enumerate() {
        let (is_aromatic, explicit_h_count, total_h_count) = atom_metadata
            .get(idx)
            .copied()
            .ok_or("Missing naming atom metadata")?;
        let mut stereo = mol.legacy_cip.get(&idx).cloned();
        if stereo.is_some() {
            if let Some(modern) = mol.accurate_cip.get(&idx) {
                stereo = Some(modern.clone());
            } else if atom.atomic_number == 16 && total_degree(&legacy, idx, total_h_count) == 3 {
                stereo = Some(
                    if stereo.as_deref() == Some("S") {
                        "R"
                    } else {
                        "S"
                    }
                    .into(),
                );
            }
        }
        let raw_stereo = if stereo.is_none() {
            match legacy.metadata.atoms.get(idx).map(|a| a.chiral_tag) {
                Some(1) => Some("CW".into()),
                Some(2) => Some("CCW".into()),
                _ => None,
            }
        } else {
            None
        };
        let symbol = ELEMENTS
            .get(usize::from(atom.atomic_number))
            .ok_or("Unknown naming element")?
            .symbol;
        mol.add_atom(Atom {
            idx,
            symbol: symbol.into(),
            charge: i32::from(atom.charge),
            isotope: (atom.isotope != 0).then_some(atom.isotope),
            stereo,
            raw_stereo,
            is_aromatic,
            explicit_h_count,
            total_h_count,
            radical_electrons: usize::from(atom.radical_electrons),
        })?;
    }
    for (idx, bond) in legacy.graph.bonds.iter().enumerate() {
        let legacy_stereo = match legacy.metadata.bonds.get(idx).map(|b| b.stereo) {
            Some(2) => Some("Z".into()),
            Some(3) => Some("E".into()),
            _ => None,
        };
        // Upstream divergence: modern bond CIP priorities preserve E/Z identity when legacy ranking disagrees.
        let modern_stereo = accurate
            .as_ref()
            .and_then(|state| state.properties.bond_codes.get(idx))
            .and_then(|code| code.as_ref());
        // Upstream divergence: pseudoasymmetric e/z bonds must not silently become ordinary E/Z.
        if modern_stereo.is_some_and(|code| !matches!(code.as_str(), "E" | "Z")) {
            return Err(
                "openclatura: pseudoasymmetric bond stereochemistry is not supported".into(),
            );
        }
        let stereo = modern_stereo.cloned().or(legacy_stereo);
        let in_small_ring = legacy.rings.atoms.iter().any(|ring| {
            (3..=7).contains(&ring.len())
                && ring
                    .iter()
                    .zip(ring.iter().cycle().skip(1))
                    .any(|(&u, &v)| (u == bond.a && v == bond.b) || (u == bond.b && v == bond.a))
        });
        mol.add_bond(Bond {
            idx: idx + 1,
            u: bond.a,
            v: bond.b,
            order: usize::from(bond.order),
            is_aromatic: aromatic_bonds[idx],
            stereo,
            in_small_ring,
        })?;
    }
    require_supported_ring_blocks(&mol)?;
    super::identity_validation::validate_input(&mol)?;
    super::identity_validation::validate_mancude_duplicate_ranking(&mol)?;
    Ok(mol)
}

/// Input checks that do not depend on the selected nomenclature system.
fn require_supported_ring_blocks(mol: &Molecule) -> Result<(), String> {
    let atoms = mol.atoms.keys().copied().collect();
    let mut ring_blocks = BTreeMap::<usize, usize>::new();
    for block in biconnected_edge_components(mol, &atoms) {
        let block_atoms: AtomSet = block.iter().flat_map(|&(u, v)| [u, v]).collect();
        if block.len() >= block_atoms.len() {
            for atom in block_atoms {
                *ring_blocks.entry(atom).or_default() += 1;
            }
        }
    }
    if ring_blocks.values().any(|&count| count > 2) {
        return Err("openclatura: three rings at one spiro atom are not ported".into());
    }
    Ok(())
}

fn atom_cip(state: &perception::State) -> BTreeMap<usize, String> {
    state
        .properties
        .atoms
        .iter()
        .enumerate()
        .filter_map(|(idx, a)| a.cip_code.as_ref().map(|label| (idx, label.clone())))
        .collect()
}

fn total_degree(state: &perception::State, atom: usize, hydrogens: usize) -> usize {
    hydrogens
        + state
            .graph
            .bonds
            .iter()
            .filter(|b| b.a == atom || b.b == atom)
            .count()
}

pub fn get_connected_components(mol: &Molecule) -> Vec<AtomSet> {
    let mut visited = AtomSet::new();
    let mut components = Vec::new();
    for &idx in mol.atoms.keys() {
        if visited.contains(&idx) {
            continue;
        }
        let mut comp = AtomSet::new();
        let mut queue = VecDeque::from([idx]);
        while let Some(current) = queue.pop_front() {
            if visited.insert(current) {
                comp.insert(current);
                queue.extend(mol.get_neighbors(current).iter().copied());
            }
        }
        components.push(comp);
    }
    components
}

/// The local isotope-numbering graph used by `small_ring_stereo.py`.
/// These artificial isotopes exist only in this detached priority calculation.
pub fn assign_local_cip_with_locant_labels(
    mol: &Molecule,
    locants: &BTreeMap<usize, usize>,
    raw_atoms: &[usize],
) -> Result<BTreeMap<usize, String>, String> {
    use reshiki_chemistry::{
        electronic::Hybridization, graph, kekulize::Direction, ranking, rings,
    };
    let mut graph = graph::Graph {
        atoms: Vec::new(),
        bonds: Vec::new(),
    };
    let mut atom_to_native = BTreeMap::new();
    let mut tags = Vec::new();
    for (&idx, atom) in &mol.atoms {
        let atomic_number = atom
            .element()
            .ok_or_else(|| format!("Unknown element: {}", atom.symbol))?
            .atomic_number;
        let isotope = locants
            .get(&idx)
            .map(|n| {
                n.checked_add(100)
                    .and_then(|n| u16::try_from(n).ok())
                    .ok_or("Local stereo locant exceeds isotope range")
            })
            .transpose()?
            .unwrap_or(0);
        atom_to_native.insert(idx, graph.atoms.len());
        graph.atoms.push(graph::Atom {
            atomic_number: u8::try_from(atomic_number).map_err(|_| "Invalid element number")?,
            isotope,
            charge: i8::try_from(atom.charge).map_err(|_| "Invalid local stereo charge")?,
            ..graph::Atom::default()
        });
        tags.push(match atom.raw_stereo.as_deref() {
            Some("CW") => 1,
            Some("CCW") => 2,
            _ => 0,
        });
    }
    for bond in mol.bonds.values() {
        if !(1..=3).contains(&bond.order) {
            return Ok(BTreeMap::new());
        }
        graph.bonds.push(graph::Bond {
            a: *atom_to_native
                .get(&bond.u)
                .ok_or("Missing local stereo bond atom")?,
            b: *atom_to_native
                .get(&bond.v)
                .ok_or("Missing local stereo bond atom")?,
            order: bond.order as u8,
            aromatic: false,
        });
    }
    let mut metadata = ranking::Metadata::unspecified(&graph);
    for (atom, chiral_tag) in metadata.atoms.iter_mut().zip(tags) {
        atom.chiral_tag = chiral_tag;
    }
    // RWMol::AddAtom/AddBond defaults, as used upstream: no sanitization,
    // aromaticity, hybridization, explicit H counts or preexisting CIP labels.
    let state = perception::State {
        valences: graph.provisional_valences()?,
        rings: perception::RingCache {
            kind: perception::RingKind::Fast,
            atoms: rings::fast(&graph)?.atoms,
        },
        properties: perception::Properties::unspecified(&graph),
        hybridizations: vec![Hybridization::Unspecified; graph.atoms.len()],
        conjugated: vec![false; graph.bonds.len()],
        directions: vec![Direction::None; graph.bonds.len()],
        graph,
        metadata,
    };
    let assigned = perception::perceive(
        &state,
        perception::Options {
            force: true,
            clean: true,
            flag_possible: false,
        },
    )?;
    let mut result = BTreeMap::new();
    for idx in raw_atoms {
        if let Some(label) = atom_to_native
            .get(idx)
            .and_then(|&native| assigned.properties.atoms.get(native))
            .and_then(|a| a.cip_code.as_ref())
        {
            result.insert(*idx, label.clone());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn graph_snapshot(mol: &Molecule, text: &str) -> Value {
        let atoms = mol
            .atoms
            .values()
            .map(|a| {
                json!({
                    "idx": a.idx, "symbol": a.symbol, "charge": a.charge, "isotope": a.isotope,
                    "stereo": a.stereo, "raw_stereo": a.raw_stereo, "is_aromatic": a.is_aromatic,
                    "explicit_h_count": a.explicit_h_count, "total_h_count": a.total_h_count,
                    "radical_electrons": a.radical_electrons,
                })
            })
            .collect::<Vec<_>>();
        let bonds = mol
            .bonds
            .values()
            .map(|b| {
                json!({
                    "idx": b.idx, "u": b.u, "v": b.v, "order": b.order,
                    "stereo": b.stereo, "in_small_ring": b.in_small_ring,
                })
            })
            .collect::<Vec<_>>();
        let adjacency = mol
            .atoms
            .keys()
            .map(|&idx| (idx.to_string(), json!(mol.get_neighbors(idx))))
            .collect::<serde_json::Map<_, _>>();
        json!({"smiles": text, "atoms": atoms, "bonds": bonds, "accurate_cip": mol.accurate_cip, "legacy_cip": mol.legacy_cip, "adjacency": adjacency})
    }

    #[test]
    fn graph_fields_match_upstream_rdkit_oracle() {
        // Generated by upstream openclatura 0.4.3 read_smiles with RDKit 2026.09.1 (Python -I -B).
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/openclatura/graph_oracle.json"
        ))
        .unwrap();
        for expected in cases {
            let text = expected["smiles"].as_str().unwrap();
            let mol = read_smiles(text).unwrap_or_else(|error| panic!("{text}: {error}"));
            let actual = graph_snapshot(&mol, text);
            assert_eq!(actual, expected, "{text}");
        }
    }

    #[test]
    fn identity_and_stage_guards_precede_graph_conversion() {
        for text in [
            "",
            "C.C",
            "[CH3]",
            "[C@H](F)Cl",
            "FC=[C@AL1]=CF",
            "C |a:0|",
            "C>C>C",
            "c1ccnc1",
        ] {
            assert!(
                read_smiles(text).is_err(),
                "Unexpected input acceptance: {text}"
            );
        }
        assert!(read_smiles("[13CH3][C@@H](F)Cl").is_ok());
    }

    #[test]
    fn separate_monocycles_are_supported_across_bridges_and_chains() {
        for text in [
            "C1CC1C2CC2",
            "C1CC1CC2CC2",
            "c1ccccc1-c2ccccc2",
            "C1CC1CC2CC2CC3CC3",
        ] {
            read_smiles(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        }
    }

    #[test]
    fn pure_spiro_blocks_are_admitted() {
        for text in [
            "C1CCC2(CC1)CCCC2",
            "C1CC12CC21CC1",
            "C1CC2(C1)CCC1(CC2)CC1",
            "C1CC1CC2CC23CC3",
        ] {
            read_smiles(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        }
    }

    #[test]
    fn polycyclic_blocks_are_admitted_only_when_fusion_abstains() {
        for text in [
            "C1CC2CCC1C2",            // norbornane: SSSR rings share a longer path.
            "C1CC2CCC1CC2",           // bicyclo[2.2.2]octane.
            "C1CC2CCC2C1",            // bicyclo[3.2.0]heptane: one SSSR ring is small.
            "C1C2CC12",               // bicyclo[1.1.0]butane.
            "C1C2CC3CC1CC(C2)C3",     // adamantane.
            "C12C3C4C1C5C2C3C45",     // cubane.
            "C1CC2CCC1C2C3CC3",       // A separate monocycle connected by a bridge.
            "C1CC2CCC1C2C3CC4CCC3C4", // Two separate nonsimple cyclic blocks.
        ] {
            read_smiles(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        }
    }

    #[test]
    fn fusion_blocks_are_admitted_for_audited_parent_selection() {
        for text in [
            "C1CCC2CCCCC2C1",  // decalin.
            "C1CC2CCCC2C1",    // bicyclo[3.3.0]octane.
            "C1C2CC3CC1CC2C3", // noradamantane.
            "c1ccc2ccccc2c1",  // Aromatic fused system.
            "c1ccc2c(c1)CC2",  // Only one SSSR ring has five or more atoms.
        ] {
            let mol = read_smiles(text).unwrap();
            assert!(
                !super::super::chains::get_cyclic_atoms(&mol, &AtomSet::new()).is_empty(),
                "{text}"
            );
        }
    }

    #[test]
    fn spiro_junctions_on_polycyclic_blocks_reach_parent_selection() {
        for text in [
            "C1CC2(C1)CCC1CCCCC12", // A fused block and a simple cycle.
            "C12(CC1)CC3CCC2C3",    // A bridged block and a simple cycle.
        ] {
            read_smiles(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        }
    }

    #[test]
    fn three_rings_on_one_spiro_atom_remain_outside_scope() {
        assert_eq!(
            read_smiles("S123(CC1)(CC2)CC3").unwrap_err(),
            "openclatura: three rings at one spiro atom are not ported"
        );
    }
}
