use super::*;
use crate::{
    document::{AtomStereo, Document, Point},
    generic::{self, Limits},
};

fn prepared(doc: &Document) -> Molecule {
    crate::chemistry::document::prepare(doc).unwrap()
}

fn equivalent(a: &Document, b: &Document) -> bool {
    let mut unique = Deduplicator::default();
    let mut work = Work::new(Limits::default().max_work);
    assert!(unique.insert(&prepared(a), &mut work).unwrap());
    !unique.insert(&prepared(b), &mut work).unwrap()
}

fn ring(elements: &[&str], aromatic: bool) -> Document {
    let mut doc = Document::default();
    let ids: Vec<_> = elements
        .iter()
        .enumerate()
        .map(|(i, &element)| {
            let angle = i as f32 * std::f32::consts::TAU / elements.len() as f32;
            doc.add_atom(element, Point::new(angle.cos() * 28., angle.sin() * 28.))
        })
        .collect();
    for (&a, &b) in ids.iter().zip(ids.iter().cycle().skip(1)) {
        doc.add_bond(a, b, if aromatic { 4 } else { 1 }, "plain");
    }
    doc
}

fn variable(mut doc: Document, targets: &[u64], element: &str) -> Document {
    let attachment =
        crate::attachments::add(&mut doc, targets, crate::attachments::Kind::Variable).unwrap();
    let substituent = doc.add_atom(element, Point::new(70., 70.));
    doc.add_bond(attachment, substituent, 1, "plain");
    doc
}

#[test]
fn six_benzene_positions_keep_one_first_representative_on_every_path() {
    let source = variable(ring(&["C"; 6], true), &[1, 2, 3, 4, 5, 6], "Cl");
    let before = source.clone();
    let products = generic::expand(&source, Limits::default()).unwrap();
    assert_eq!(products.len(), 1);
    assert!(products[0].bonds.iter().any(|b| {
        (b.a == 1 && products[0].atom(b.b).unwrap().element == "Cl")
            || (b.b == 1 && products[0].atom(b.a).unwrap().element == "Cl")
    }));
    let drawings = generic::expand_drawings(&source, Limits::default()).unwrap();
    assert_eq!(drawings.len(), 1);
    let paired = generic::expand_drawing_inner(&source, Limits::default()).unwrap();
    assert_eq!(paired.tiles.len(), drawings.len());
    assert!(
        paired
            .tiles
            .iter()
            .all(|tile| tile.document.atoms.len() == tile.hydrogens.len())
    );
    let batch = generic::prepare_expansion(&source, Limits::default()).unwrap();
    assert_eq!(batch.count(), 1);
    let input = generic::prepare_expansion_input(&source, &[], Limits::default()).unwrap();
    assert_eq!(generic::expand_prepared(input).unwrap().count(), 1);
    assert_eq!(source, before);
}

#[test]
fn unequal_ring_substituents_preserve_ortho_meta_para_products() {
    let mut source = ring(&["C"; 6], true);
    let cl = source.add_atom("Cl", Point::new(56., 0.));
    source.add_bond(1, cl, 1, "plain");
    let source = variable(source, &[2, 3, 4, 5, 6], "Br");
    let products = generic::expand(&source, Limits::default()).unwrap();
    assert_eq!(products.len(), 3);
    // Stable first representatives are positions 2, 3 and 4.
    for (product, target) in products.iter().zip([2, 3, 4]) {
        assert!(product.bonds.iter().any(|bond| {
            (bond.a == target && product.atom(bond.b).unwrap().element == "Br")
                || (bond.b == target && product.atom(bond.a).unwrap().element == "Br")
        }));
    }
}

#[test]
fn asymmetrical_heteroring_keeps_each_distinct_position() {
    let mut source = ring(&["N", "C", "C", "C", "C", "C"], true);
    let fluorine = source.add_atom("F", Point::new(45., 45.));
    source.add_bond(2, fluorine, 1, "plain");
    let source = variable(source, &[3, 4, 5, 6], "Cl");
    assert_eq!(
        generic::expand(&source, Limits::default()).unwrap().len(),
        4
    );
}

#[test]
fn atom_ids_order_coordinates_style_and_kekule_form_do_not_change_identity() {
    let aromatic = ring(&["C"; 6], true);
    let mut alternate = aromatic.clone();
    for (i, bond) in alternate.bonds.iter_mut().enumerate() {
        bond.order = if i % 2 == 0 { 1 } else { 2 };
        bond.display = if bond.order == 2 { "dashed" } else { "plain" }.into();
        bond.ring_arc = true;
    }
    for atom in &mut alternate.atoms {
        atom.id += 900;
        atom.position.x = atom.position.x * 3. + 200.;
        atom.position.y = atom.position.y * 3. - 350.;
    }
    for bond in &mut alternate.bonds {
        bond.a += 900;
        bond.b += 900;
        std::mem::swap(&mut bond.a, &mut bond.b);
    }
    alternate.atoms.rotate_left(3);
    alternate.atoms.reverse();
    alternate.bonds.reverse();
    assert!(equivalent(&aromatic, &alternate));
}

#[test]
fn isotopes_charges_radicals_and_meaningful_maps_remain_distinct() {
    let mut base = Document::default();
    base.add_atom("C", Point::default());
    for kind in 0..4 {
        let mut variant = base.clone();
        let atom = &mut variant.atoms[0];
        match kind {
            0 => atom.isotope = 13,
            1 => atom.charge = 1,
            2 => {
                atom.no_implicit = true;
                atom.explicit_h = 3;
                atom.radical_electrons = 1;
            }
            _ => atom.map_num = 17,
        }
        assert!(!equivalent(&base, &variant), "distinction {kind}");
    }
    let mut swapped = ring(&["C"; 6], true);
    swapped.atoms[0].map_num = 1;
    swapped.atoms[1].map_num = 2;
    let mut distant = swapped.clone();
    distant.atoms[1].map_num = 0;
    distant.atoms[2].map_num = 2;
    assert!(!equivalent(&swapped, &distant));
}

#[test]
fn equivalent_total_hydrogens_ignore_explicit_count_storage() {
    let mut implicit = Document::default();
    implicit.add_atom("C", Point::default());
    let mut explicit = implicit.clone();
    explicit.atoms[0].explicit_h = 4;
    explicit.atoms[0].no_implicit = true;
    assert!(equivalent(&implicit, &explicit));
}

fn tetrahedral(implicit_h: bool) -> Document {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    let mut neighbors = Vec::new();
    for (i, element) in ["F", "Cl", "Br", "I"]
        .iter()
        .take(if implicit_h { 3 } else { 4 })
        .enumerate()
    {
        let neighbor = doc.add_atom(element, Point::new(i as f32 * 28., 28.));
        doc.add_bond(center, neighbor, 1, "plain");
        neighbors.push(neighbor);
    }
    doc.atom_mut(center).unwrap().stereo = Some(AtomStereo {
        winding: "cw".into(),
        neighbors,
    });
    doc
}

#[test]
fn tetrahedral_parity_survives_neighbor_order_and_distinguishes_enantiomers() {
    for implicit_h in [false, true] {
        let source = tetrahedral(implicit_h);
        let mut reordered = source.clone();
        reordered.bonds.swap(0, 1);
        reordered.atoms.reverse();
        assert!(equivalent(&source, &reordered));
        let mut mirror = reordered.clone();
        mirror.atom_mut(1).unwrap().stereo.as_mut().unwrap().winding = "ccw".into();
        assert!(!equivalent(&source, &mirror));
        let mut unspecified = source.clone();
        unspecified.atom_mut(1).unwrap().stereo = None;
        assert!(!equivalent(&source, &unspecified));
    }
}

#[test]
fn stereo_mismatch_continues_search_to_a_later_component_mapping() {
    let mut source = tetrahedral(false);
    let second = source.add_atom("C", Point::new(140., 0.));
    let mut neighbors = Vec::new();
    for (i, element) in ["F", "Cl", "Br", "I"].iter().enumerate() {
        let neighbor = source.add_atom(element, Point::new(140. + i as f32 * 28., 28.));
        source.add_bond(second, neighbor, 1, "plain");
        neighbors.push(neighbor);
    }
    source.atom_mut(second).unwrap().stereo = Some(AtomStereo {
        winding: "ccw".into(),
        neighbors,
    });
    let mut exchanged = source.clone();
    exchanged
        .atom_mut(1)
        .unwrap()
        .stereo
        .as_mut()
        .unwrap()
        .winding = "ccw".into();
    exchanged
        .atom_mut(second)
        .unwrap()
        .stereo
        .as_mut()
        .unwrap()
        .winding = "cw".into();
    // Constitution first proposes 1 -> 1, which fails the full stereo check.
    // The successful mapping exchanges the two disconnected components.
    assert!(equivalent(&source, &exchanged));
}

fn alkene() -> Document {
    let mut doc = Document::default();
    for (element, x, y) in [
        ("C", 0., 0.),
        ("C", 28., 0.),
        ("F", -14., 24.),
        ("Cl", -14., -24.),
        ("Br", 42., 24.),
        ("I", 42., -24.),
    ] {
        doc.add_atom(element, Point::new(x, y));
    }
    doc.add_bond(1, 2, 2, "plain");
    for (a, b) in [(1, 3), (1, 4), (2, 5), (2, 6)] {
        doc.add_bond(a, b, 1, "plain");
    }
    doc.bonds[0].stereo = Some("cis".into());
    doc.bonds[0].stereo_atoms = vec![3, 5];
    doc.bonds[0].stereo_authoritative = true;
    doc
}

#[test]
fn alkene_controls_and_endpoint_order_are_equivalent_but_geometric_isomers_are_not() {
    let source = alkene();
    let mut one = source.clone();
    one.bonds[0].stereo_atoms = vec![4, 5];
    one.bonds[0].stereo = Some("trans".into());
    assert!(equivalent(&source, &one));
    let mut both = source.clone();
    both.bonds[0].stereo_atoms = vec![4, 6];
    assert!(equivalent(&source, &both));
    let mut reverse = source.clone();
    let bond = &mut reverse.bonds[0];
    std::mem::swap(&mut bond.a, &mut bond.b);
    reverse.bonds[0].stereo_atoms.reverse();
    assert!(equivalent(&source, &reverse));
    let mut geometric = source.clone();
    geometric.bonds[0].stereo = Some("trans".into());
    assert!(!equivalent(&source, &geometric));
    let mut unspecified = source.clone();
    unspecified.bonds[0].stereo = None;
    unspecified.bonds[0].stereo_atoms.clear();
    assert!(!equivalent(&source, &unspecified));
    let mut unknown = source.clone();
    unknown.bonds[0].stereo = Some("any".into());
    unknown.bonds[0].stereo_atoms.clear();
    assert!(!equivalent(&source, &unknown));
}

#[test]
fn nonstandard_supplied_stereo_requires_exact_mapped_references() {
    // Preparation currently retains stereo tags on ordinary single bonds.
    // Their alternate references must not inherit alkene side-switch rules.
    let mut source = alkene();
    source.bonds[0].order = 1;
    source.bonds[0].stereo_authoritative = false;
    let mut alternate = source.clone();
    alternate.bonds[0].stereo_atoms[0] = 4;
    assert!(!equivalent(&source, &alternate));
    alternate.bonds[0].stereo = Some("trans".into());
    assert!(!equivalent(&source, &alternate));

    // An opposite endpoint used as a control is also outside that rule.
    let mut invalid_control = alkene();
    invalid_control.bonds[0].stereo_atoms = vec![2, 1];
    let mut changed = invalid_control.clone();
    changed.bonds[0].stereo_atoms = vec![3, 1];
    changed.bonds[0].stereo = Some("trans".into());
    assert!(!equivalent(&invalid_control, &changed));
}

#[test]
fn same_local_colors_do_not_merge_nonisomorphic_graphs() {
    let source = ring(&["C"; 6], false);
    let mut separate = Document::default();
    for _ in 0..6 {
        separate.add_atom("C", Point::default());
    }
    for (a, b) in [(1, 2), (2, 3), (3, 1), (4, 5), (5, 6), (6, 4)] {
        separate.add_bond(a, b, 1, "plain");
    }
    assert!(!equivalent(&source, &separate));
}

#[test]
fn dative_direction_is_chemical_identity() {
    let mut source = Document::default();
    let nitrogen = source.add_atom("N", Point::default());
    let metal = source.add_atom("Pt", Point::new(28., 0.));
    source.add_bond(nitrogen, metal, 5, "plain");
    let mut reverse = source.clone();
    let bond = &mut reverse.bonds[0];
    std::mem::swap(&mut bond.a, &mut bond.b);
    assert!(!equivalent(&source, &reverse));
}

#[test]
fn identity_allocation_and_search_spend_the_same_remaining_budget() {
    let source = prepared(&ring(&["C"; 6], true));
    let mut unique = Deduplicator::default();
    let mut insufficient = Work::new(1);
    assert!(
        unique
            .insert(&source, &mut insufficient)
            .unwrap_err()
            .contains("work limit")
    );
    assert!(unique.representatives.is_empty());
    let mut exact = Work::new(Limits::default().max_work);
    assert!(unique.insert(&source, &mut exact).unwrap());
    let before = exact.remaining;
    assert!(!unique.insert(&source, &mut exact).unwrap());
    assert!(exact.remaining < before);
    let mut snapshot_work = Work::new(Limits::default().max_work);
    Identity::new(&source, &mut snapshot_work).unwrap();
    let snapshot_cost = Limits::default().max_work - snapshot_work.remaining;
    let mut search_work = Work::new(snapshot_cost + 1);
    assert!(
        unique
            .insert(&source, &mut search_work)
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(unique.representatives.len(), 1);
    exact.remaining = 0;
    assert!(
        unique
            .insert(&source, &mut exact)
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(unique.representatives.len(), 1);
}
