use super::*;
use crate::compare::tests::{chemical, drawn, edited, kinds, load, molecule, moved, nitrate};
use crate::compare::{Detail, Kind, MatchedBy, Part, PartKind, Report, Subject, compare};
use crate::document::{Document, Point};

/// `smiles` at `x` in a drawing numbered from `first` on, as a fresh import
/// numbers it.
fn alone(smiles: &str, first: u64) -> Document {
    at(&[(smiles, 0.)], first).0
}

/// Molecules `(smiles, x)` side by side, numbered from `first` on, and
/// each one's atoms.
fn at(molecules: &[(&str, f32)], first: u64) -> (Document, Vec<Vec<u64>>) {
    let mut doc = Document::default();
    if first > 1 {
        let spacer = doc.add_atom("C", Point::new(0., 900.));
        doc.atom_mut(spacer).unwrap().id = first - 1;
    }
    let mut atoms = Vec::new();
    for (smiles, x) in molecules {
        let ids = crate::editing::append(&mut doc, &molecule(smiles), Point::new(*x, 0.));
        assert!(!ids.is_empty());
        atoms.push(ids);
    }
    doc.delete(&[first - 1]);
    (load(&doc), atoms)
}

fn by(report: &Report, left: &[u64]) -> Option<MatchedBy> {
    let mut left = left.to_vec();
    left.sort_unstable();
    report
        .molecules
        .iter()
        .find(|pair| {
            let mut mine = pair.left.clone();
            mine.sort_unstable();
            mine == left
        })
        .map(|pair| pair.by)
}

fn removed_and_added(report: &Report) -> bool {
    kinds(report)
        == [
            (Kind::Added, Subject::Molecule),
            (Kind::Removed, Subject::Molecule),
        ]
}

#[test]
fn a_reimported_scheme_with_a_nitro_group_added_is_one_chemical_change() {
    let (left, again) = (drawn(false), drawn(true));
    let mut added = [0; 3];
    let right = edited(&again.doc, |doc| {
        added = nitrate(doc, &again.ester);
    });
    let report = compare(&left.doc, &right);
    assert_eq!(kinds(&report), [(Kind::Chemical, Subject::Molecule)]);
    assert_eq!(by(&report, &left.ester), Some(MatchedBy::Structure));
    assert_eq!(by(&report, &left.acid), Some(MatchedBy::Smiles));
    assert_eq!(by(&report, &left.ethanol), Some(MatchedBy::Smiles));
    // Painted as when the ids were kept: the nitro group and its carbon.
    let change = chemical(&report)[0];
    assert_eq!(change.right.atoms.len(), 4, "{:?}", change.right.atoms);
    assert!(added.iter().all(|id| change.right.atoms.contains(id)));
    assert_eq!(change.left.atoms.len(), 1);
    assert_eq!(change.right.bonds.len(), 3);
    let Detail::Edit {
        parts,
        by_structure,
        ..
    } = &change.detail
    else {
        panic!("{:?}", change.detail);
    };
    assert!(by_structure);
    assert_eq!(
        parts,
        &[Part {
            kind: PartKind::Added,
            what: "NO₂".into(),
            at: vec![],
        }]
    );
    assert_eq!(change.before.as_deref(), Some("CCOC(=O)c1ccccc1"));
}

#[test]
fn a_reimported_toluene_made_m_cresol_marks_the_carbon_where_it_is() {
    let left = alone("Cc1ccccc1", 1);
    let again = alone("Cc1ccccc1", 40);
    let bonded = |doc: &Document, id: u64| -> Vec<u64> {
        doc.bonds
            .iter()
            .filter_map(|b| match (b.a == id, b.b == id) {
                (true, _) => Some(b.b),
                (_, true) => Some(b.a),
                _ => None,
            })
            .collect()
    };
    // A carbon meta to the methyl: two bonds from the ring carbon bearing it.
    let ipso = again
        .atoms
        .iter()
        .find(|a| bonded(&again, a.id).len() == 3)
        .unwrap()
        .id;
    let ortho = bonded(&again, ipso);
    let meta = again
        .atoms
        .iter()
        .map(|a| a.id)
        .filter(|id| *id != ipso && !ortho.contains(id))
        .filter(|id| ortho.iter().any(|o| bonded(&again, *o).contains(id)))
        .max()
        .unwrap();
    let right = edited(&again, |doc| {
        let at = doc.atom(meta).unwrap().position;
        let oxygen = doc.add_atom("O", at.offset(0., -40.));
        doc.add_bond(meta, oxygen, 1, "plain");
    });
    let report = compare(&left, &right);
    let change = chemical(&report)[0];
    let place = right.atom(meta).unwrap().position;
    let carbon = left.atoms.iter().find(|a| a.position == place).unwrap().id;
    assert_eq!(change.left.atoms, [carbon]);
    assert!(change.right.atoms.contains(&meta));
}

#[test]
fn a_reimport_with_colliding_ids_is_matched_again_by_content() {
    let (left, again) = (drawn(false), drawn(true));
    // Most ids now name atoms of another molecule.
    let shared = left
        .acid
        .iter()
        .filter(|id| again.ester.contains(id))
        .count();
    assert!(shared > 5, "{shared}");
    let report = compare(&left.doc, &again.doc);
    assert_eq!(report.changes, vec![]);
    assert!(
        report
            .molecules
            .iter()
            .all(|pair| pair.by == MatchedBy::Smiles)
    );
}

#[test]
fn a_molecule_cut_to_under_half_keeping_its_ids_is_still_one_change() {
    let left = alone("CCCCCCCCCC", 1);
    // Six of ten carbons deleted: three of nine bonds kept.
    let right = edited(&left, |doc| doc.delete(&[5, 6, 7, 8, 9, 10]));
    let report = compare(&left, &right);
    assert_eq!(kinds(&report), [(Kind::Chemical, Subject::Molecule)]);
    assert_eq!(report.molecules[0].by, MatchedBy::Id);
}

#[test]
fn a_molecule_cut_down_keeps_its_ids_even_beside_a_new_copy() {
    let left = alone("CCCCCCCCCC", 1);
    let right = edited(&left, |doc| {
        doc.delete(&[5, 6, 7, 8, 9, 10]);
        // A new decane, numbered from 101.
        let spacer = doc.add_atom("C", Point::new(0., 900.));
        doc.atom_mut(spacer).unwrap().id = 100;
        crate::editing::append(doc, &molecule("CCCCCCCCCC"), Point::new(1000., 0.));
        doc.delete(&[100]);
    });
    let report = compare(&left, &right);
    assert_eq!(report.molecules.len(), 1);
    assert_eq!(report.molecules[0].by, MatchedBy::Id);
    assert_eq!(report.molecules[0].right, [1, 2, 3, 4]);
}

#[test]
fn groups_added_all_round_a_ring_drawn_again_are_not_a_redraw() {
    let left = alone("c1ccccc1", 1);
    let right = edited(&alone("c1ccccc1", 40), |doc| {
        let ring: Vec<(u64, Point)> = doc.atoms.iter().map(|a| (a.id, a.position)).collect();
        let n = ring.len() as f32;
        let (cx, cy) = ring
            .iter()
            .fold((0., 0.), |(x, y), (_, p)| (x + p.x / n, y + p.y / n));
        for (id, p) in ring {
            let f = doc.add_atom("F", Point::new(2. * p.x - cx, 2. * p.y - cy));
            doc.add_bond(id, f, 1, "plain");
        }
    });
    let report = compare(&left, &right);
    assert_eq!(report.molecules[0].by, MatchedBy::Structure);
    assert_eq!(kinds(&report), [(Kind::Chemical, Subject::Molecule)]);
}

#[test]
fn a_constitution_pairing_must_keep_bonds_to_drawn_hydrogens() {
    let methanol = |first: u64, hydrogen_on_oxygen: bool| {
        let mut doc = Document::default();
        let c = doc.add_atom("C", Point::new(0., 0.));
        doc.atom_mut(c).unwrap().id = first;
        let o = doc.add_atom("O", Point::new(40., 0.));
        let at = if hydrogen_on_oxygen {
            (40., 40.)
        } else {
            (0., 40.)
        };
        let h = doc.add_atom("H", Point::new(at.0, at.1));
        doc.add_bond(first, o, 1, "plain");
        doc.add_bond(if hydrogen_on_oxygen { o } else { first }, h, 1, "plain");
        if hydrogen_on_oxygen {
            doc.atom_mut(first).unwrap().isotope = 13;
        }
        load(&doc)
    };
    let report = compare(&methanol(1, false), &methanol(40, true));
    assert_eq!(report.molecules[0].by, MatchedBy::Constitution);
    // No bond to a hydrogen moved from carbon to oxygen.
    let change = chemical(&report)[0];
    assert!(change.left.bonds.is_empty(), "{:?}", change.left.bonds);
}

#[test]
fn a_reimported_stereo_flip_matches_by_constitution() {
    let report = compare(&alone("C[C@H](O)CC", 1), &alone("C[C@@H](O)CC", 6));
    assert_eq!(kinds(&report), [(Kind::Chemical, Subject::Molecule)]);
    assert_eq!(report.molecules[0].by, MatchedBy::Constitution);
    // The centre, by its own id on each side.
    let change = chemical(&report)[0];
    assert_eq!(
        (&change.left.atoms[..], &change.right.atoms[..]),
        (&[2][..], &[7][..])
    );
    assert!(matches!(
        change.detail,
        Detail::Edit {
            by_structure: false,
            ..
        }
    ));
}

#[test]
fn a_reimported_moved_and_edited_scheme_is_one_move_and_one_change() {
    let (left, again) = (drawn(false), drawn(true));
    let right = edited(&again.doc, |doc| {
        moved(doc, &doc.all_ids(), -38., 14., 0.);
        nitrate(doc, &again.ester);
    });
    let report = compare(&left.doc, &right);
    assert_eq!(
        kinds(&report),
        [
            (Kind::Chemical, Subject::Molecule),
            (Kind::SchemeMoved, Subject::Scheme)
        ]
    );
    assert_eq!(by(&report, &left.ester), Some(MatchedBy::Structure));
    assert_eq!(chemical(&report)[0].right.atoms.len(), 4);
}

#[test]
fn unrelated_molecules_are_never_matched_by_structure() {
    for (a, b) in [
        // Rings correspond: biphenyl has two ring systems, hexylbenzene one.
        ("c1ccc(cc1)-c1ccccc1", "CCCCCCc1ccccc1"),
        // A ring bond only matches a ring bond.
        ("CCCC", "C1CCC1"),
        // Fewer than three bonds in common.
        ("O", "CO"),
        ("CCO", "CCCO"),
        // The tail alone: a ring system needs a ring bond kept with it.
        ("CCCCCCC1CC1", "CCCCCCC1NN1"),
    ] {
        for (x, y) in [(a, b), (b, a)] {
            let report = compare(&alone(x, 1), &alone(y, 40));
            assert!(
                removed_and_added(&report),
                "{x} → {y}: {:?}",
                kinds(&report)
            );
        }
    }
}

#[test]
fn a_molecule_that_cannot_be_verified_is_never_matched_by_structure() {
    let left = alone("CCCCc1ccccc1", 1);
    let right = edited(&alone("CCCCc1ccccc1", 40), |doc| {
        let carbon = doc.atoms.first().unwrap().id;
        let at = doc.atom(carbon).unwrap().position;
        let r = doc.add_atom("R", at.offset(30., 0.));
        doc.add_bond(carbon, r, 1, "plain");
    });
    assert!(removed_and_added(&compare(&left, &right)));
}

#[test]
fn of_two_candidates_only_a_clearly_nearer_one_matches() {
    let left = alone("CCc1ccccc1", 1);
    let pair = |near: f32, far: f32| {
        let (doc, atoms) = at(&[("CCCc1ccccc1", near), ("CCCc1ccccc1", far)], 40);
        (compare(&left, &doc), atoms)
    };
    // At the same place, or not twice as near: neither.
    for (near, far) in [(0., 0.), (-300., 300.), (200., 300.)] {
        let (report, _) = pair(near, far);
        assert!(
            report.molecules.is_empty(),
            "{near}, {far}: {:?}",
            kinds(&report)
        );
    }
    let (report, atoms) = pair(100., 400.);
    assert_eq!(report.molecules.len(), 1);
    assert_eq!(report.molecules[0].by, MatchedBy::Structure);
    let mut right = report.molecules[0].right.clone();
    right.sort_unstable();
    assert_eq!(right, atoms[0]);
}

#[test]
fn a_rival_too_large_to_search_leaves_an_equal_candidate_unmatched() {
    // 32 of 64 bonds kept, or perhaps 64 of 128: both a half.
    let left = alone(&"C".repeat(65), 1);
    let (right, _) = at(&[(&"C".repeat(33), 0.), (&"C".repeat(129), 0.)], 100);
    let report = compare(&left, &right);
    assert!(report.molecules.is_empty(), "{:?}", kinds(&report));
}

#[test]
fn a_structure_tie_is_broken_after_the_move_constitution_matches_share() {
    let (left, _) = at(&[("CO", -2000.), ("CCCC", 0.), ("CN", 2000.)], 1);
    // Everything 300 to the right, the methyls labelled, and a second butane
    // where the first was.
    let (right, atoms) = at(
        &[
            ("[13CH3]O", -1700.),
            ("CCCCC", 0.),
            ("CCCCC", 300.),
            ("[13CH3]N", 2300.),
        ],
        40,
    );
    let report = compare(&left, &right);
    let pair = (report.molecules.iter())
        .find(|pair| pair.by == MatchedBy::Structure)
        .unwrap();
    let mut matched = pair.right.clone();
    matched.sort_unstable();
    assert_eq!(matched, atoms[2]);
}

#[test]
fn benzene_and_pyridine_are_the_boundary_of_similar() {
    // Four of six aromatic bonds kept: half of all bonds of either.
    let report = compare(&alone("c1ccccc1", 1), &alone("c1ccncc1", 40));
    assert_eq!(report.molecules.len(), 1, "{:?}", kinds(&report));
    assert_eq!(report.molecules[0].by, MatchedBy::Structure);
    assert_eq!(chemical(&report).len(), 1);
}

#[test]
fn a_large_molecule_matches_the_same_way_twice() {
    let smiles = "c1ccc2ccccc2c1CC".repeat(8) + "C";
    let left = alone(&smiles, 1);
    let right = alone(&format!("{smiles}Cl"), 200);
    let first = compare(&left, &right);
    assert_eq!(first, compare(&left, &right));
    assert_eq!(first.molecules.len(), 1);
    assert_eq!(first.molecules[0].by, MatchedBy::Structure);
}

#[test]
fn similarity_is_counted_in_twentieths_from_whole_bonds() {
    assert!(similar(4, 6, 6));
    assert!(!similar(3, 6, 6));
    assert!(!similar(2, 2, 3), "fewer than three bonds");
    assert_eq!(step(4, 6, 6), 10);
    assert_eq!(step(11, 11, 14), 16);
    assert_eq!(step(0, 0, 0), 0);
}

fn candidate(found: Option<u32>, most: u32, complete: bool, distance: f64) -> Candidate {
    Candidate {
        left: 0,
        right: 0,
        found,
        most,
        complete,
        distance,
        pairs: Vec::new(),
    }
}

#[test]
fn an_unknown_rival_that_could_be_as_good_leaves_the_best_unmatched() {
    let best = candidate(Some(16), 16, true, 500.);
    let best_of = |rival: Candidate| clear_best(&[(0, &best), (1, &rival)]);
    // Skipped or cut short, it could reach the best: no match.
    assert_eq!(best_of(candidate(None, 16, false, 10.)), None);
    assert_eq!(best_of(candidate(Some(12), 18, false, 10.)), None);
    // It cannot: the best stands, wherever the rival is.
    assert_eq!(best_of(candidate(None, 15, false, 10.)), Some(0));
    // Finished: a worse or unacceptable one does not count.
    assert_eq!(best_of(candidate(Some(15), 15, true, 10.)), Some(0));
    assert_eq!(best_of(candidate(None, 20, true, 10.)), Some(0));
    // As good: the nearer, if strictly nearer and at most half as far.
    assert_eq!(best_of(candidate(Some(16), 16, true, 1000.)), Some(0));
    assert_eq!(best_of(candidate(Some(16), 16, true, 999.)), None);
    assert_eq!(best_of(candidate(Some(16), 16, true, 500.)), None);
    // A best cut short is a lower bound, judged the same way.
    let short = candidate(Some(16), 18, false, 500.);
    let rival = candidate(Some(15), 15, true, 10.);
    assert_eq!(clear_best(&[(0, &short), (1, &rival)]), Some(0));
    let zero = candidate(Some(16), 16, true, 0.);
    assert_eq!(
        clear_best(&[(0, &zero), (1, &zero)]),
        None,
        "0 ≤ 0 / 2 is a tie"
    );
}
