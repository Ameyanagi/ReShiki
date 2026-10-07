use super::*;
use crate::graph::Bond;

fn donor() -> Graph {
    Graph {
        atoms: vec![
            Atom {
                atomic_number: 8,
                explicit_hydrogens: 2,
                ..Atom::default()
            },
            Atom {
                atomic_number: 26,
                ..Atom::default()
            },
            Atom {
                atomic_number: 26,
                ..Atom::default()
            },
        ],
        bonds: (1..3)
            .map(|a| Bond {
                a,
                b: 0,
                order: 1,
                aromatic: false,
            })
            .collect(),
    }
}

#[test]
fn one_conversion_per_donor_preserves_atoms_and_corrects_direction() -> Result<(), String> {
    let graph = donor();
    let before = serde_json::to_value(&graph).map_err(|error| error.to_string())?;
    let result = organometallics(&graph, &Metadata::unspecified(&graph), None)?;
    assert_eq!(result.bonds.iter().filter(|b| b.order == 5).count(), 1);
    assert_eq!(result.bonds.iter().filter(|b| b.order == 1).count(), 1);
    assert!(
        result
            .bonds
            .iter()
            .filter(|b| b.order == 5)
            .all(|b| b.a == 0)
    );
    assert_eq!(result.provisional_valences()?[0].explicit_valence, 3);
    assert_eq!(
        serde_json::to_value(&result.atoms).map_err(|e| e.to_string())?,
        before["atoms"]
    );
    assert_eq!(
        serde_json::to_value(&graph).map_err(|e| e.to_string())?,
        before
    );
    Ok(())
}

#[test]
fn malformed_inputs_fail_before_any_result_is_exposed() -> Result<(), String> {
    for kind in 0..4 {
        let mut graph = donor();
        let mut metadata = Metadata::unspecified(&graph);
        let mut rings = None;
        match kind {
            0 => graph.bonds[0].a = 999,
            1 => graph.atoms[0].charge = i8::MIN,
            2 => metadata.atoms.clear(),
            _ => rings = Some(vec![vec![0, 1, 999]]),
        }
        let before = serde_json::to_value(&graph).map_err(|error| error.to_string())?;
        assert!(organometallics(&graph, &metadata, rings.as_deref()).is_err());
        assert_eq!(
            serde_json::to_value(&graph).map_err(|e| e.to_string())?,
            before
        );
    }
    Ok(())
}
