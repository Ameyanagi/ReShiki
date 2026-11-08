// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | crates/chemistry/src/stereo/perception/tests.rs | be63a799aa03a83dc6ec74bb294f4d42b89298d3

// Copied from crates/chemistry/src/stereo/perception/tests.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: pass explicit default preparation/import budgets
use super::*;
use crate::{
    graph::{Atom, Bond},
    ranking::StereoGroup,
};
type TestResult = anyhow::Result<()>;

fn state(graph: Graph) -> Result<State, String> {
    Ok(State {
        metadata: Metadata::unspecified(&graph),
        directions: vec![Direction::None; graph.bonds.len()],
        valences: graph.provisional_valences()?,
        conjugated: vec![false; graph.bonds.len()],
        hybridizations: vec![Hybridization::Sp3; graph.atoms.len()],
        rings: RingCache::default(),
        properties: Properties::unspecified(&graph),
        graph,
    })
}
fn options() -> Options {
    Options {
        clean: true,
        force: true,
        flag_possible: true,
    }
}

#[test]
fn allene_winding_needs_an_axis() -> TestResult {
    // Like ligands at a terminal leave no axis, as at a centre: two methyls,
    // atom maps, or ¹H beside H (#992). Stereo-only differences (m/p), ring
    // paths and a bridging hydride, which continues to B, keep the winding.
    for (smiles, kept) in [
        ("CC=C=C(C)C", false),
        ("CC=C=C([CH3:1])[CH3:2]", false),
        ("[1H]C=C=C(F)Cl", false),
        ("CC=C=C(C)CC", true),
        ("[2H]C=C=C(F)Cl", true),
        ("C([C@H](F)Cl)([C@@H](F)Cl)=C=C(I)Br", true),
        ("C1CCC2=C=C1CCC2", true),
        ("B[H-]C=C=C(F)Cl", true),
    ] {
        let mut input = crate::smiles::prepare(smiles, &crate::budget::Budget::default())?.state;
        let center = (0..input.graph.atoms.len())
            .find(|&i| crate::stereo::allene::terminals(&input.graph, i).is_some())
            .ok_or_else(|| anyhow::anyhow!("No allene in {smiles}"))?;
        input.metadata.atoms[center].chiral_tag = 5;
        input.metadata.atoms[center].chiral_permutation = Some(1);
        for clean in [false, true] {
            let options = Options { clean, ..options() };
            let meta = &perceive(&input, options)
                .map_err(anyhow::Error::msg)?
                .metadata
                .atoms[center];
            assert_eq!(
                (meta.chiral_tag, meta.chiral_permutation),
                if kept { (5, Some(1)) } else { (0, None) },
                "{smiles}, clean {clean}"
            );
        }
    }
    // A stereo group loses a cleared allene, as it loses a cleared centre.
    let read = crate::smiles::read("CC=[C@AL1]=C(C)C |&1:2|", &crate::budget::Budget::default())?
        .prepared
        .state;
    assert!(read.metadata.atoms.iter().all(|a| a.chiral_tag == 0));
    assert!(read.metadata.groups.is_empty());
    Ok(())
}

#[test]
fn unspecified_stereo_uses_perceived_atom_and_double_bond_candidates() -> TestResult {
    for (smiles, unspecified) in [
        ("CC(O)C(=O)O", true),
        ("C[C@H](O)C(=O)O", false),
        ("C[C@H](O)C(O)C", true),
        ("CC=CC", true),
        ("C/C=C/C", false),
        ("C[C@H](O)CC=CC", true),
        ("C[C@H](O)C/C=C/C", false),
        // Legacy bond candidates are conservative for cumulenes; do not skip
        // them and silently admit unspecified axial stereo in naming.
        ("C[C@H](O)CC=C=C(C)C", true),
        ("C[C@H](O)CC=C=CC", true),
        ("C=C", false),
        ("CC=C(C)C", false),
        ("O=C(O)C", false),
        ("C1=CCCCC1", false),
        // Ring cis/trans needs both members of the pair specified.
        ("CC1CCC(CC1)[C@@H](C)O", true),
        ("C[C@H]1CC[C@@H](CC1)[C@@H](C)O", false),
        ("C[C@H]1CCC(CC1)[C@@H](C)O", true),
        ("C1CCC(CC1)[C@@H](C)O", false),
        ("Cc1ccc(cc1)[C@@H](C)O", false),
        ("CCO.[Na+]", false),
    ] {
        let imported = crate::smiles::read(smiles, &crate::budget::Budget::default())?;
        assert_eq!(
            has_unspecified_stereo(&imported.prepared.state).map_err(anyhow::Error::msg)?,
            unspecified,
            "{smiles}"
        );
    }
    Ok(())
}
fn star() -> Result<State, String> {
    state(Graph {
        atoms: [6, 9, 17, 35, 53]
            .into_iter()
            .map(|atomic_number| Atom {
                atomic_number,
                no_implicit: true,
                ..Atom::default()
            })
            .collect(),
        bonds: (1..5)
            .map(|b| Bond {
                a: 0,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    })
}

#[test]
fn winding_and_atom_order_preserve_opposite_labels() -> TestResult {
    let mut input = star().map_err(anyhow::Error::msg)?;
    input.metadata.atoms[0].chiral_tag = 1;
    let before = serde_json::to_value(&input)?;
    let cw = perceive(&input, options()).map_err(anyhow::Error::msg)?;
    assert_eq!(cw.properties.atoms[0].cip_code.as_deref(), Some("R"));
    assert_eq!(cw.properties.atoms[0].possible, Some(true));
    assert_eq!(serde_json::to_value(&input)?, before);
    input.metadata.atoms[0].chiral_tag = 2;
    assert_eq!(
        perceive(&input, options())
            .map_err(anyhow::Error::msg)?
            .properties
            .atoms[0]
            .cip_code
            .as_deref(),
        Some("S")
    );
    input.graph.bonds.swap(0, 1);
    assert_eq!(
        perceive(&input, options())
            .map_err(anyhow::Error::msg)?
            .properties
            .atoms[0]
            .cip_code
            .as_deref(),
        Some("R")
    );
    Ok(())
}

#[test]
fn cleanup_removes_invalid_center_and_repairs_hydrogen_cache() -> TestResult {
    let mut input = star().map_err(anyhow::Error::msg)?;
    input.graph.atoms.pop();
    input.graph.bonds.pop();
    input.graph.atoms[1].atomic_number = 17;
    input.graph.atoms[0].explicit_hydrogens = 1;
    input = state(input.graph).map_err(anyhow::Error::msg)?;
    input.metadata.atoms[0].chiral_tag = 2;
    input.directions[0] = Direction::Wedge;
    let output = perceive(&input, options()).map_err(anyhow::Error::msg)?;
    assert_eq!(output.metadata.atoms[0].chiral_tag, 0);
    assert_eq!(output.graph.atoms[0].explicit_hydrogens, 0);
    assert!(!output.graph.atoms[0].no_implicit);
    assert_eq!(output.valences[0].implicit_hydrogens, 1);
    assert_eq!(output.directions[0], Direction::None);
    assert!(output.properties.atoms[0].cip_code.is_none());
    Ok(())
}

#[test]
fn empty_and_cached_done_do_not_require_stereo_ranks() -> TestResult {
    let empty = state(Graph {
        atoms: Vec::new(),
        bonds: Vec::new(),
    })
    .map_err(anyhow::Error::msg)?;
    let output = perceive(&empty, options()).map_err(anyhow::Error::msg)?;
    assert_eq!(output.properties.done, Some(true));
    assert_eq!(output.rings.kind, RingKind::Symmetric);
    let mut input = star().map_err(anyhow::Error::msg)?;
    input.properties.done = Some(false);
    input.properties.needs_detection = Some(true);
    input.properties.atoms[0].cip_code = Some("preserved".into());
    let output = perceive(
        &input,
        Options {
            force: false,
            ..options()
        },
    )
    .map_err(anyhow::Error::msg)?;
    assert_eq!(serde_json::to_value(output)?, serde_json::to_value(input)?);
    Ok(())
}

#[test]
fn invalid_metadata_and_work_exhaustion_leave_inputs_unchanged() -> TestResult {
    let input = star().map_err(anyhow::Error::msg)?;
    let before = serde_json::to_value(&input)?;
    assert!(with_work(&input, options(), None, true, &mut Work(0)).is_err());
    assert!(with_work(&input, options(), None, true, &mut Work(10)).is_err());
    assert!(perceive_file_queries(&input, options(), &[]).is_err());
    let mut bad = input.clone();
    bad.properties.atoms.pop();
    assert!(perceive(&bad, options()).is_err());
    let mut bad = input.clone();
    bad.properties.atoms[0].ring_members = Some(vec![1]);
    assert!(perceive(&bad, options()).is_err());
    bad.metadata.atoms[0].ring_stereo = true;
    bad.properties.atoms[0].ring_members = Some(vec![i32::MIN]);
    assert!(perceive(&bad, options()).is_err());
    let mut bad = input.clone();
    bad.rings = RingCache {
        kind: RingKind::Fast,
        atoms: vec![vec![0, 1, 2]],
    };
    assert!(perceive(&bad, options()).is_err());
    let mut bad = input.clone();
    bad.metadata.atoms[0].chiral_tag = 1;
    bad.metadata.atoms[0].map_number = i32::MIN;
    assert!(perceive(&bad, options()).is_err());
    assert_eq!(serde_json::to_value(&input)?, before);
    Ok(())
}

#[test]
fn deep_chain_is_iterative_and_group_expansion_is_bounded() -> TestResult {
    let n = 20_000;
    let input = state(Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
                no_implicit: true,
                ..Atom::default()
            };
            n
        ],
        bonds: (1..n)
            .map(|b| Bond {
                a: b - 1,
                b,
                order: 1,
                aromatic: false,
            })
            .collect(),
    })
    .map_err(anyhow::Error::msg)?;
    let output = perceive(
        &input,
        Options {
            clean: false,
            flag_possible: false,
            force: true,
        },
    )
    .map_err(anyhow::Error::msg)?;
    assert!(output.rings.atoms.is_empty());
    assert_eq!(output.properties.done, Some(true));
    let mut graph = input.graph;
    graph.atoms.truncate(2001);
    graph.bonds = (1..2001)
        .map(|b| Bond {
            a: 0,
            b,
            order: 1,
            aromatic: false,
        })
        .collect();
    let mut input = state(graph).map_err(anyhow::Error::msg)?;
    for bond in &mut input.metadata.bonds {
        bond.stereo = 6;
    }
    input.metadata.groups = vec![
        StereoGroup {
            kind: 1,
            atoms: vec![0],
            ..StereoGroup::default()
        };
        1100
    ];
    let before = serde_json::to_value(&input)?;
    let error = perceive(&input, options());
    assert!(error.is_err_and(|e| e.contains("group output storage")));
    assert_eq!(serde_json::to_value(&input)?, before);
    Ok(())
}

#[test]
fn cleanup_keeps_allene_winding_only_on_an_allene_centre() -> TestResult {
    // A raw tag 5 on an atom that is not an allene centre is cleared, as RDKit
    // does; a supported allene keeps its winding.
    let mut lone = crate::smiles::prepare("C", &crate::budget::Budget::default())?.state;
    lone.metadata.atoms[0].chiral_tag = 5;
    lone.metadata.atoms[0].chiral_permutation = Some(1);
    assert_eq!(
        perceive(&lone, options())
            .map_err(anyhow::Error::msg)?
            .metadata
            .atoms[0]
            .chiral_tag,
        0
    );
    let allene = crate::smiles::prepare("OC=[C@AL1]=CF", &crate::budget::Budget::default())?.state;
    let perceived = perceive(&allene, options()).map_err(anyhow::Error::msg)?;
    assert_eq!(perceived.metadata.atoms[2].chiral_tag, 5);
    assert_eq!(
        perceived.metadata.atoms[2].chiral_permutation,
        allene.metadata.atoms[2].chiral_permutation
    );
    Ok(())
}
