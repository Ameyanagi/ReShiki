use super::*;
fn atom(id: u64) -> Atom {
    Atom {
        id,
        element: "Fe".into(),
        isotope: 0,
        map: id as u32,
        charge: 0,
        h: 0,
        radical: 0,
    }
}
#[test]
fn dative_direction_is_compared_after_endpoint_mapping() {
    let source = Graph {
        atoms: vec![atom(1), atom(2)],
        edges: BTreeMap::from([(
            (0, 1),
            BondClass {
                order: 5,
                donor: Some(0),
            },
        )]),
    };
    let target = Graph {
        atoms: vec![atom(3), atom(4)],
        edges: BTreeMap::from([(
            (0, 1),
            BondClass {
                order: 5,
                donor: Some(1),
            },
        )]),
    };
    let fixed = HashMap::from([(0, 0), (1, 1)]);
    let changed = score(&source, &target, &fixed);
    assert_eq!(changed.adjacency, 1);
    assert_eq!(changed.unchanged_bonds, 0);
    assert_eq!(changed.changed_atoms, 2);
    let swapped = HashMap::from([(0, 1), (1, 0)]);
    let same = score(&source, &target, &swapped);
    assert_eq!(same.unchanged_bonds, 1);
    assert_eq!(same.changed_atoms, 0);
}
#[test]
fn augment_budget_is_checked_before_scanning_domains() {
    let domains = vec![vec![0, 1], vec![0, 1]];
    let mut work = 0;
    assert!(
        augment(
            0,
            &domains,
            &HashSet::new(),
            &mut HashSet::new(),
            &mut HashMap::new(),
            &mut work,
            Budget {
                states: 100,
                work: 0
            }
        )
        .is_err()
    );
    assert_eq!(work, 1);
}
