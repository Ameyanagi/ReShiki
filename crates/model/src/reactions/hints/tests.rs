use super::*;
use crate::{
    arrows::Preset,
    document::{Arrow, Point},
};

fn molecule(doc: &mut Document, smiles: &str, center: f32) -> Vec<u64> {
    let parsed = chemistry::smiles::parse(smiles).unwrap();
    let ids: Vec<_> = parsed
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let id = doc.add_atom(
                chemistry::ELEMENTS[usize::from(input.atomic_number)].symbol,
                Point::new(center + i as f32, i as f32 * 10.),
            );
            let atom = doc.atom_mut(id).unwrap();
            atom.charge = i32::from(input.charge);
            atom.isotope = u32::from(input.isotope);
            atom.explicit_h = u32::from(input.explicit_hydrogens);
            atom.no_implicit = input.no_implicit;
            atom.aromatic = input.aromatic;
            atom.radical_electrons = input.radical_electrons;
            id
        })
        .collect();
    for bond in parsed.graph.bonds {
        doc.add_bond(ids[bond.a], ids[bond.b], bond.order, "plain");
    }
    ids
}

fn drawing(left: &str, right: &str, preset: Preset) -> Document {
    let mut doc = Document::default();
    molecule(&mut doc, left, -100.);
    molecule(&mut doc, right, 200.);
    let arrow = Arrow::new(
        doc.next_id(),
        Point::default(),
        Point::new(100., 0.),
        preset,
        Default::default(),
    );
    doc.arrows.push(arrow);
    doc.validate().unwrap();
    chemistry::document::prepare(&doc).unwrap();
    doc
}

fn kinds(doc: &Document) -> Vec<ArrowSemanticsHintKind> {
    arrow_semantics_hints(doc)
        .into_iter()
        .map(|hint| {
            assert_eq!(hint.arrow, doc.arrows[0].id);
            hint.kind
        })
        .collect()
}

#[test]
fn resonance_warns_for_hydrogen_movement_and_different_sigma_connectivity() {
    for (left, right) in [("CC=O", "C=CO"), ("CCO", "COC"), ("CCO", "CCN")] {
        let doc = drawing(left, right, Preset::Resonance);
        let before = doc.clone();
        assert_eq!(
            kinds(&doc),
            [ArrowSemanticsHintKind::NotContributingStructures],
            "{left} / {right}"
        );
        assert_eq!(doc, before);
        assert!(kinds(&drawing(left, right, Preset::Equilibrium)).is_empty());
    }
}

#[test]
fn electronic_rearrangement_suggests_resonance_only_for_equilibrium() {
    for (left, right) in [
        ("[CH2-]C=O", "C=C[O-]"),
        ("CC(=O)N", "CC([O-])=[NH2+]"),
        ("C[CH]C=C", "CC=C[CH2]"),
    ] {
        assert!(
            kinds(&drawing(left, right, Preset::Resonance)).is_empty(),
            "{left} / {right}"
        );
        assert_eq!(
            kinds(&drawing(left, right, Preset::Equilibrium)),
            [ArrowSemanticsHintKind::ContributingStructures],
            "{left} / {right}"
        );
    }
}

#[test]
fn atom_and_bond_permutations_do_not_change_the_result() {
    let mut doc = drawing("CC(=O)N", "[NH2+]=C([O-])C", Preset::Equilibrium);
    let expected = kinds(&doc);
    assert_eq!(expected, [ArrowSemanticsHintKind::ContributingStructures]);
    doc.atoms.reverse();
    doc.bonds.reverse();
    for bond in &mut doc.bonds {
        std::mem::swap(&mut bond.a, &mut bond.b);
    }
    assert_eq!(kinds(&doc), expected);
    for (left, right) in [
        ("CCO", "OCC"),
        ("CC(C)O", "OC(C)C"),
        ("CC(=O)[O-]", "CC([O-])=O"),
    ] {
        assert!(
            kinds(&drawing(left, right, Preset::Equilibrium)).is_empty(),
            "{left} / {right}"
        );
    }
}

#[test]
fn total_charge_radical_count_and_isotope_identity_are_conserved() {
    for (left, right) in [
        ("C[C+](C)C", "C[C](C)C"),
        ("[O][O]", "O=O"),
        ("[13CH3]CO", "CCO"),
        ("[2H]OC", "[2H]CO"),
    ] {
        assert_eq!(
            kinds(&drawing(left, right, Preset::Resonance)),
            [ArrowSemanticsHintKind::NotContributingStructures],
            "{left} / {right}"
        );
    }
}

#[test]
fn attached_explicit_hydrogen_and_hydrogen_count_labels_match_implicit_hydrogen() {
    for right in ["[H]OC", "[OH]C"] {
        assert!(kinds(&drawing("CO", right, Preset::Resonance)).is_empty());
        assert!(kinds(&drawing("CO", right, Preset::Equilibrium)).is_empty());
    }
}

#[test]
fn incomplete_abbreviated_or_multi_molecule_sides_are_silent() {
    let base = drawing("CC=O", "C=CO", Preset::Resonance);
    let mut extra = base.clone();
    molecule(&mut extra, "O", -150.);
    assert!(kinds(&extra).is_empty());
    let mut abbreviated = base.clone();
    let ids: Vec<_> = abbreviated
        .atoms
        .iter()
        .take(3)
        .map(|atom| atom.id)
        .collect();
    abbreviated.contract(&ids, "A", "").unwrap();
    assert!(kinds(&abbreviated).is_empty());
    let partial = drawing("*C=O", "C=CO", Preset::Resonance);
    assert!(kinds(&partial).is_empty());
    for order in [0, 5, 7] {
        let mut partial = base.clone();
        partial.bonds[0].order = order;
        assert!(kinds(&partial).is_empty());
    }
    let mut invalid = base;
    invalid.atoms[0].explicit_h = 10;
    assert!(kinds(&invalid).is_empty());
}

#[test]
fn end_side_geometry_follows_reversed_rotated_and_diagonal_arrows() {
    for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.), (0.6, 0.8)] {
        let mut doc = drawing("CC=O", "C=CO", Preset::Resonance);
        let transform =
            |p: Point| Point::new(1000. + p.x * dx - p.y * dy, -200. + p.x * dy + p.y * dx);
        for atom in &mut doc.atoms {
            atom.position = transform(atom.position);
        }
        doc.arrows[0].start = transform(doc.arrows[0].start);
        doc.arrows[0].end = transform(doc.arrows[0].end);
        assert_eq!(
            kinds(&doc),
            [ArrowSemanticsHintKind::NotContributingStructures]
        );
    }
    let base = drawing("CC=O", "C=CO", Preset::Resonance);
    let mut midpoint = base.clone();
    midpoint.atoms[0].position = Point::new(50., 0.);
    assert!(kinds(&midpoint).is_empty());
    let mut zero = base.clone();
    zero.arrows[0].end = zero.arrows[0].start;
    assert!(kinds(&zero).is_empty());
    let mut curved = base.clone();
    curved.arrows[0].control = Some(Point::new(50., 20.));
    assert_eq!(
        kinds(&curved),
        [ArrowSemanticsHintKind::NotContributingStructures]
    );
    let mut forward = base;
    forward.arrows[0].kind = "forward".into();
    assert!(kinds(&forward).is_empty());
}

#[test]
fn matching_checks_connectivity_beyond_element_hydrogen_and_degree_multisets() {
    // Both are cubic carbon graphs with identical atom labels and H counts.
    let left = "C12C3C1C4C2C34";
    let right = "C12C3C4C1C3C24";
    let doc = drawing(left, right, Preset::Resonance);
    assert_eq!(
        kinds(&doc),
        [ArrowSemanticsHintKind::NotContributingStructures]
    );
}

#[test]
fn each_arrow_checks_only_its_own_row_and_neighbors() {
    // A ↔ B ↔ C: the next arrow ends each side, so each arrow sees one molecule.
    let mut doc = drawing("[CH2-]C=O", "C=C[O-]", Preset::Resonance);
    molecule(&mut doc, "CC=O", 500.);
    let mut arrow = doc.arrows[0].clone();
    arrow.id = doc.next_id();
    (arrow.start, arrow.end) = (Point::new(300., 0.), Point::new(400., 0.));
    doc.arrows.push(arrow);
    let hints = arrow_semantics_hints(&doc);
    assert_eq!(
        hints,
        [ArrowSemanticsHint {
            arrow: doc.arrows[1].id,
            kind: ArrowSemanticsHintKind::NotContributingStructures,
        }]
    );
    // A second scheme in another row neither joins nor blocks the first.
    let mut rows = drawing("CC=O", "C=CO", Preset::Resonance);
    molecule(&mut rows, "CCO", -100.);
    molecule(&mut rows, "CCN", 200.);
    for atom in rows.atoms.iter_mut().skip(6) {
        atom.position.y += 300.;
    }
    assert_eq!(
        kinds(&rows),
        [ArrowSemanticsHintKind::NotContributingStructures]
    );
}
