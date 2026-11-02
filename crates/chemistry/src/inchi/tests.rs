use super::{input, kernel, output};
use crate::smiles;

fn allene_output(smiles: &str, center: usize, neighbors: [i16; 4], parity: i8) -> output::Output {
    let state = smiles::prepare(smiles).unwrap().state;
    let mut atoms = state
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| output::Atom {
            position: [0.0; 3],
            element: crate::ELEMENTS[usize::from(atom.atomic_number)]
                .symbol
                .into(),
            isotopic_mass: 0,
            charge: atom.charge,
            hydrogens: [
                (u32::from(atom.explicit_hydrogens) + state.valences[index].implicit_hydrogens)
                    as i8,
                0,
                0,
                0,
            ],
            radical: 0,
            bonds: Vec::new(),
        })
        .collect::<Vec<_>>();
    for bond in &state.graph.bonds {
        atoms[bond.a].bonds.push(output::Bond {
            neighbor: bond.b as i16,
            kind: bond.order as i8,
            stereo: 0,
        });
    }
    output::Output {
        status: 0,
        message: String::new(),
        log: String::new(),
        warning_flags: [[0; 2]; 2],
        atoms,
        stereo: vec![output::Stereo {
            central_atom: Some(center as i16),
            neighbors,
            kind: 3,
            parity,
        }],
    }
}

#[test]
fn native_allene_parity_maps_to_extended_tetrahedral_configuration() {
    // InChI's documented EVEN projection for X-A=C=B-Y is AL1/P when
    // X and Y are the first carrier at their respective ends. The official
    // C kernel produces penta-2,3-diene /t5-/m1/s1 for this EVEN record.
    for (parity, permutation) in [(2, 1), (1, 2)] {
        let raw = allene_output("CC=C=CC", 2, [0, 1, 3, 4], parity);
        let assembly = output::assemble(&raw).unwrap();
        assert!(assembly.warnings.is_empty(), "{:?}", assembly.warnings);
        let state = assembly.state.unwrap();
        assert_eq!(state.metadata.atoms[2].chiral_tag, 5);
        assert_eq!(
            state.metadata.atoms[2].chiral_permutation,
            Some(permutation)
        );
        let prepared = input::prepare(&state, None).unwrap();
        assert_eq!(prepared.stereo.len(), 1);
        assert_eq!(prepared.stereo[0].central_atom, Some(2));
        assert_eq!(prepared.stereo[0].neighbors, [0, 1, 3, 4]);
        assert_eq!(prepared.stereo[0].kind, 3);
        assert_eq!(prepared.stereo[0].parity, parity);
    }
}

#[test]
fn allene_carrier_and_terminal_reordering_preserves_configuration() {
    for (neighbors, parity, permutation) in [
        ([0, 1, 4, 5], 2, 1),
        ([5, 4, 1, 0], 2, 1),
        ([2, 1, 4, 5], 2, 2),
        ([0, 1, 4, 6], 2, 2),
        ([2, 1, 4, 6], 2, 1),
        ([2, 1, 4, 5], 1, 1),
    ] {
        let raw = allene_output("FC(Cl)=C=C(Br)I", 3, neighbors, parity);
        let assembly = output::assemble(&raw).unwrap();
        assert!(assembly.warnings.is_empty());
        let state = assembly.state.unwrap();
        assert_eq!(state.metadata.atoms[3].chiral_tag, 5);
        assert_eq!(
            state.metadata.atoms[3].chiral_permutation,
            Some(permutation),
            "{neighbors:?}, parity {parity}"
        );
        let prepared = input::prepare(&state, None).unwrap();
        assert_eq!(prepared.stereo[0].neighbors, [0, 1, 4, 5]);
        assert_eq!(
            prepared.stereo[0].parity,
            if permutation == 1 { 2 } else { 1 }
        );
    }
}

#[test]
fn native_allene_configuration_survives_terminal_hydrogen_removal() {
    // GetStructFromINCHI expands both terminal H atoms for these identifiers.
    // Reproduce those explicit-H records without a foreign runtime dependency.
    for (parity, permutation) in [(2, 1), (1, 2)] {
        let mut raw = allene_output("CC=C=CC", 2, [5, 1, 3, 6], parity);
        for end in [1, 3] {
            raw.atoms[end].hydrogens[0] = 0;
            raw.atoms.push(output::Atom {
                position: [0.0; 3],
                element: "H".into(),
                isotopic_mass: 0,
                charge: 0,
                hydrogens: [0; 4],
                radical: 0,
                bonds: vec![output::Bond {
                    neighbor: end as i16,
                    kind: 1,
                    stereo: 0,
                }],
            });
        }
        let assembly = output::reconstruct(&raw, output::Options::default()).unwrap();
        assert!(assembly.warnings.is_empty());
        let state = assembly.state.unwrap();
        assert_eq!(state.graph.atoms.len(), 5);
        assert_eq!(state.metadata.atoms[2].chiral_tag, 5);
        assert_eq!(
            state.metadata.atoms[2].chiral_permutation,
            Some(permutation)
        );
        let prepared = input::prepare(&state, None).unwrap();
        assert_eq!(prepared.stereo[0].neighbors, [0, 1, 3, 4]);
        assert_eq!(prepared.stereo[0].parity, parity);
    }
}

#[test]
fn unknown_allene_parity_and_longer_cumulenes_remain_unspecified() {
    let raw = allene_output("CC=C=CC", 2, [0, 1, 3, 4], 3);
    let assembly = output::assemble(&raw).unwrap();
    assert_eq!(assembly.state.unwrap().metadata.atoms[2].chiral_tag, 0);

    let raw = allene_output("CC=C=C=C=CC", 3, [0, 1, 5, 6], 2);
    let assembly = output::assemble(&raw).unwrap();
    assert_eq!(assembly.warnings, [output::Warning::IgnoredAllene]);
    assert_eq!(assembly.state.unwrap().metadata.atoms[3].chiral_tag, 0);

    // Original raw-adapter captures include type 3 records on ordinary
    // alkenes with NO_ATOM. Keep those unsupported records ignored.
    let mut raw = allene_output("FC=CCl", 0, [0, 1, 2, 3], 2);
    raw.stereo[0].central_atom = None;
    let assembly = output::assemble(&raw).unwrap();
    assert_eq!(assembly.warnings, [output::Warning::IgnoredAllene]);
}

#[test]
fn scalar_kernel_requires_coordinates_instead_of_dropping_allene_configuration() {
    let mut state = smiles::prepare("CC=C=CC").unwrap().state;
    state.metadata.atoms[2].chiral_tag = 5;
    state.metadata.atoms[2].chiral_permutation = Some(1);
    let molecule = kernel::Molecule::prepare(&state, None).unwrap();
    let error = kernel::generate(&molecule).unwrap_err();
    assert!(error.contains("allene stereochemistry"), "{error}");
    assert!(error.contains("cosmolkit-inchi 0.5.0"), "{error}");
}
