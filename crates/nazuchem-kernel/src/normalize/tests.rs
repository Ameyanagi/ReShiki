// Copied from crates/chemistry/src/normalize/tests.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3; changed: pass an explicit default sanitization budget
use super::*;
use crate::{
    graph::{Atom, Bond},
    kekulize::Direction,
    ranking::Metadata,
    sanitize,
};

fn oxide(center: u8, order: u8) -> Graph {
    Graph {
        atoms: [center, 8, 6, 6, 6]
            .map(|atomic_number| Atom {
                atomic_number,
                ..Default::default()
            })
            .into(),
        bonds: [(0, 1, order), (0, 2, 1), (0, 3, 1), (0, 4, 1)]
            .map(|(a, b, order)| Bond {
                a,
                b,
                order,
                aromatic: false,
            })
            .into(),
    }
}

#[test]
fn nitrogen_oxide_dative_normalizes_before_hydrogen_and_radical_assignment() {
    for no_implicit in [false, true] {
        let mut graph = oxide(7, 5);
        graph.atoms[1].no_implicit = no_implicit;
        let before = graph.clone();
        let result = sanitize::sanitize(
            &graph,
            &Metadata::unspecified(&graph),
            &vec![Direction::None; graph.bonds.len()],
            &crate::budget::Budget::default(),
        )
        .unwrap();
        assert_eq!(graph, before);
        assert_eq!(result.graph.atoms[0].charge, 1);
        assert_eq!(result.graph.atoms[1].charge, -1);
        assert_eq!(result.graph.bonds[0].order, 1);
        for atom in 0..2 {
            assert_eq!(result.valences[atom].implicit_hydrogens, 0);
            assert_eq!(result.graph.atoms[atom].radical_electrons, 0);
        }
        assert_eq!(
            functional_groups(&result.graph).unwrap(),
            result.graph,
            "Cleanup must be idempotent"
        );
    }
}

#[test]
fn other_elements_reverse_arrows_and_nonterminal_acceptors_are_unchanged() {
    for center in [15, 16, 26, 29] {
        let graph = oxide(center, 5);
        assert_eq!(functional_groups(&graph).unwrap(), graph);
    }
    let mut reverse = oxide(7, 5);
    reverse.bonds[0].a = 1;
    reverse.bonds[0].b = 0;
    assert_eq!(functional_groups(&reverse).unwrap(), reverse);

    let mut nonterminal = oxide(7, 5);
    nonterminal.bonds.push(Bond {
        a: 1,
        b: 2,
        order: 1,
        aromatic: false,
    });
    assert_eq!(functional_groups(&nonterminal).unwrap(), nonterminal);
}

#[test]
fn nitrogen_metal_datives_and_intentional_radicals_are_unchanged() {
    let mut graph = oxide(7, 5);
    graph.atoms[1].atomic_number = 29;
    assert_eq!(functional_groups(&graph).unwrap(), graph);
    graph.atoms[1].atomic_number = 8;
    graph.atoms[1].radical_electrons = 1;
    assert_eq!(functional_groups(&graph).unwrap(), graph);
}
