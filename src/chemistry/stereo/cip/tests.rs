use super::*;
use crate::chemistry::{
    electronic::Hybridization,
    graph::{Atom, Bond},
    kekulize::Direction,
    ranking::Metadata,
    stereo::perception::Properties,
};

fn chain(n: usize) -> anyhow::Result<State> {
    let graph = Graph {
        atoms: vec![
            Atom {
                atomic_number: 6,
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
    };
    Ok(State {
        metadata: Metadata::unspecified(&graph),
        directions: vec![Direction::None; graph.bonds.len()],
        valences: graph.provisional_valences().map_err(anyhow::Error::msg)?,
        conjugated: vec![false; graph.bonds.len()],
        hybridizations: vec![Hybridization::Sp3; graph.atoms.len()],
        rings: RingCache::default(),
        properties: Properties::unspecified(&graph),
        graph,
    })
}

#[test]
fn rejects_malformed_graph_caches_and_indices() -> anyhow::Result<()> {
    let valid = chain(4)?;
    let mut bad = valid.clone();
    bad.graph.bonds[0].b = usize::MAX;
    assert!(matches!(Molecule::new(&bad), Err(Error::Invalid(_))));
    let mut bad = valid.clone();
    bad.valences[0].implicit_hydrogens = u32::MAX;
    assert!(Molecule::new(&bad).is_err());
    let mut bad = valid.clone();
    bad.metadata.atoms.pop();
    assert!(Molecule::new(&bad).is_err());
    for ring in [vec![0, 1], vec![0, 1, 0, 2], vec![0, 1, 9], vec![0, 1, 3]] {
        let mut bad = valid.clone();
        bad.rings = RingCache {
            kind: RingKind::Fast,
            atoms: vec![ring],
        };
        assert!(Molecule::new(&bad).is_err());
    }
    let mut bad = valid.clone();
    bad.rings.atoms.push(vec![0, 1, 2]);
    assert!(Molecule::new(&bad).is_err());
    let mut bad = valid.clone();
    bad.rings = RingCache {
        kind: RingKind::Fast,
        atoms: vec![vec![0; 2_000_001]],
    };
    assert!(matches!(Molecule::new(&bad), Err(Error::Limit)));
    let mut mol = Molecule::new(&valid)?;
    assert!(mol.bond_order(usize::MAX).is_err());
    assert!(mol.is_in_ring(usize::MAX).is_err());
    assert!(mol.fraction(usize::MAX).is_err());
    assert!(mol.bond_types.is_none());
    assert!(mol.ring_bonds.is_none());
    assert!(mol.fractions.is_none());
    Ok(())
}

#[test]
fn deep_chain_is_iterative_and_preserves_the_input() -> anyhow::Result<()> {
    let mut input = chain(100_000)?;
    input.graph.atoms[99_999].isotope = 13;
    input.metadata.atoms[99_999].map_number = 73;
    let mut mol = Molecule::new(&input)?;
    assert_eq!(mol.fraction(99_999)?, Fraction(6, 1));
    assert_eq!(mol.bond_order(99_998)?, 1);
    assert!(!mol.is_in_ring(99_998)?);
    assert_eq!(input.graph.atoms[99_999].isotope, 13);
    assert_eq!(input.metadata.atoms[99_999].map_number, 73);
    assert_eq!(input.rings.kind, RingKind::None);
    Ok(())
}

#[test]
fn partial_bond_returns_a_typed_error_without_changing_the_input() -> anyhow::Result<()> {
    let mut input = chain(2)?;
    input.graph.bonds[0].order = 7;
    let before = serde_json::to_value(&input)?;
    let mut mol = Molecule::new(&input)?;
    assert!(matches!(
        mol.bond_order(0),
        Err(Error::BondOrder { index: 0, order: 7 })
    ));
    assert!(matches!(mol.fraction(0), Err(Error::BondOrder { .. })));
    assert_eq!(serde_json::to_value(&input)?, before);
    Ok(())
}
