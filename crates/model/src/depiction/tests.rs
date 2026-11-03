use super::*;
use crate::{chemistry::document as chemistry_document, document::Point};

fn molecule(elements: &[&str], bonds: &[(usize, usize, u8)]) -> Document {
    let mut document = Document::default();
    let ids: Vec<_> = elements
        .iter()
        .enumerate()
        .map(|(i, element)| {
            document.add_atom(
                element,
                Point::new((i % 3) as f32 * 42., (i / 3) as f32 * 42.),
            )
        })
        .collect();
    for &(a, b, order) in bonds {
        document.add_bond(ids[a], ids[b], order, "plain");
    }
    document
}

fn nitromethane() -> (Document, u64, u64) {
    (
        molecule(&["C", "N", "O", "O"], &[(0, 1, 1), (1, 2, 2), (1, 3, 2)]),
        2,
        3,
    )
}

fn trimethylamine_oxide(order: u8) -> (Document, u64, u64) {
    (
        molecule(
            &["N", "O", "C", "C", "C"],
            &[(0, 1, order), (0, 2, 1), (0, 3, 1), (0, 4, 1)],
        ),
        1,
        2,
    )
}

fn nitrone() -> (Document, u64, u64) {
    let mut document = molecule(
        &["C", "C", "N", "O", "C"],
        &[(0, 1, 1), (1, 2, 2), (2, 3, 2), (2, 4, 1)],
    );
    let double = document.bonds.get_mut(1).unwrap();
    double.stereo = Some("e".into());
    double.stereo_atoms = vec![1, 4];
    double.stereo_authoritative = true;
    (document, 3, 4)
}

fn methyl_azide() -> (Document, u64, u64) {
    (
        molecule(&["C", "N", "N", "N"], &[(0, 1, 1), (1, 2, 2), (2, 3, 3)]),
        3,
        4,
    )
}

fn hydrogens(document: &Document) -> Vec<(u64, u32)> {
    let prepared = chemistry_document::prepare(document).unwrap();
    prepared
        .ids
        .iter()
        .zip(&prepared.state.graph.atoms)
        .zip(&prepared.state.valences)
        .map(|((&id, atom), valence)| {
            (
                id,
                u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens,
            )
        })
        .collect()
}

fn check_conversion((mut document, nitrogen, other): (Document, u64, u64), order: u8) {
    let hydrogen_counts = hydrogens(&document);
    let hydrogen_inputs: Vec<_> = document
        .atoms
        .iter()
        .map(|atom| (atom.explicit_h, atom.no_implicit, atom.position))
        .collect();
    let source = document.clone();
    assert_eq!(eligible_atoms(&document, &[nitrogen]), vec![nitrogen]);
    assert_eq!(document, source, "Eligibility is read-only");
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(1));
    assert_eq!(document.atom(nitrogen).unwrap().charge, 1);
    assert_eq!(document.atom(other).unwrap().charge, -1);
    let bond = document.bond_between(nitrogen, other).unwrap().1;
    assert_eq!(bond.order, order);
    assert_eq!(bond.display, "plain");
    assert_eq!(hydrogens(&document), hydrogen_counts);
    assert_eq!(
        document
            .atoms
            .iter()
            .map(|atom| (atom.explicit_h, atom.no_implicit, atom.position))
            .collect::<Vec<_>>(),
        hydrogen_inputs
    );
    document.validate().unwrap();
    let converted = document.clone();
    assert!(eligible_atoms(&document, &[nitrogen]).is_empty());
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(0));
    assert_eq!(document, converted);
}

#[test]
fn nitromethane_uses_first_oxygen_and_keeps_other_carbonyl() {
    let (mut document, nitrogen, other) = nitromethane();
    document.atom_mut(nitrogen).unwrap().display.hide_charge = true;
    document.atom_mut(other).unwrap().display.hide_charge = true;
    document.atom_mut(other).unwrap().cip_label = Some("R".into());
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(1));
    assert!(!document.atom(nitrogen).unwrap().display.hide_charge);
    assert!(!document.atom(other).unwrap().display.hide_charge);
    assert_eq!(document.atom(other).unwrap().cip_label, None);
    assert_eq!(document.bond_between(nitrogen, 4).unwrap().1.order, 2);
    assert_eq!(document.atom(4).unwrap().charge, 0);
    assert_eq!(document.atom(1).unwrap().label_h, 3);
    check_conversion(nitromethane(), 1);
}

#[test]
fn trimethylamine_oxide_reduces_nitrogen_oxygen_double_bond() {
    check_conversion(trimethylamine_oxide(2), 1);
}

#[test]
fn nitrone_retains_its_carbon_nitrogen_double_bond_and_captured_stereo() {
    let (mut document, nitrogen, _) = nitrone();
    let double = document.bonds.get_mut(1).unwrap();
    double.stereo = Some("e".into());
    double.stereo_atoms = vec![1, 4];
    double.stereo_authoritative = true;
    let captured = double.clone();
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(1));
    let mut after = document.bonds[1].clone();
    after.cip_label = captured.cip_label.clone();
    assert_eq!(after, captured);
    check_conversion(nitrone(), 1);
}

#[test]
fn methyl_azide_reduces_triple_bond_to_double_without_adding_hydrogen() {
    check_conversion(methyl_azide(), 2);
}

#[test]
fn nitrogen_oxygen_dative_becomes_plain_single_charge_separation() {
    let (mut document, nitrogen, other) = trimethylamine_oxide(5);
    let bond = document.bonds.first_mut().unwrap();
    bond.projection = true;
    bond.display = "wedge_end".into();
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(1));
    let bond = document.bond_between(nitrogen, other).unwrap().1;
    assert_eq!(bond.order, 1);
    assert_eq!(bond.display, "plain");
    assert!(!bond.projection);
    check_conversion(trimethylamine_oxide(5), 1);
}

#[test]
fn nitrogen_hydrogen_inputs_are_retained_for_secondary_amine_oxides() {
    for no_implicit in [false, true] {
        let mut document = molecule(&["N", "O", "C", "C"], &[(0, 1, 2), (0, 2, 1), (0, 3, 1)]);
        let nitrogen = document.atom_mut(1).unwrap();
        nitrogen.explicit_h = 1;
        nitrogen.no_implicit = no_implicit;
        check_conversion((document, 1, 2), 1);
    }
    // An N->O donor with two carbon substituents keeps its implicit N-H.
    let document = molecule(&["N", "O", "C", "C"], &[(0, 1, 5), (0, 2, 1), (0, 3, 1)]);
    assert_eq!(hydrogens(&document)[0], (1, 1));
    check_conversion((document, 1, 2), 1);
}

#[test]
fn sulfur_phosphorus_metal_datives_and_unselected_centers_stay_unchanged() {
    // DMSO and dimethyl sulfone retain the ordinary S=O depiction.
    for document in [
        molecule(&["S", "O", "C", "C"], &[(0, 1, 2), (0, 2, 1), (0, 3, 1)]),
        molecule(
            &["S", "O", "O", "C", "C"],
            &[(0, 1, 2), (0, 2, 2), (0, 3, 1), (0, 4, 1)],
        ),
    ] {
        let source = document.clone();
        let mut document = document;
        let selection = document.all_ids();
        assert!(eligible_atoms(&document, &selection).is_empty());
        assert_eq!(draw_as_charge_separated(&mut document, &selection), Ok(0));
        assert_eq!(document, source);
    }
    for element in ["S", "P"] {
        for order in [2, 5] {
            let (mut document, _, _) = trimethylamine_oxide(order);
            document.atoms[0].element = element.into();
            let source = document.clone();
            assert!(eligible_atoms(&document, &document.all_ids()).is_empty());
            let selection = document.all_ids();
            assert_eq!(draw_as_charge_separated(&mut document, &selection), Ok(0));
            assert_eq!(document, source);
        }
    }
    let (mut document, nitrogen, _) = trimethylamine_oxide(5);
    document.atoms[1].element = "Cu".into();
    let source = document.clone();
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(0));
    assert_eq!(document, source);
    let (mut document, _, oxygen) = nitromethane();
    let source = document.clone();
    assert_eq!(draw_as_charge_separated(&mut document, &[oxygen]), Ok(0));
    assert_eq!(draw_as_charge_separated(&mut document, &[]), Ok(0));
    assert_eq!(document, source);
}

#[test]
fn selection_changes_only_its_centers_and_returns_unique_count() {
    let (mut document, first, _) = nitromethane();
    let (second, nitrogen, _) = trimethylamine_oxide(2);
    let inserted = crate::editing::append(&mut document, &second, Point::new(200., 0.));
    let next = inserted[nitrogen as usize - 1];
    assert_eq!(
        eligible_atoms(&document, &[first, next, first]),
        [first, next]
    );
    assert_eq!(draw_as_charge_separated(&mut document, &[next]), Ok(1));
    assert_eq!(document.atom(first).unwrap().charge, 0);
    assert_eq!(document.atom(next).unwrap().charge, 1);
    assert_eq!(
        draw_as_charge_separated(&mut document, &[first, next]),
        Ok(1)
    );
}

#[test]
fn malformed_drawings_fail_atomically() {
    let (mut document, nitrogen, _) = nitromethane();
    document.bonds[0].b = 999;
    let source = document.clone();
    assert!(eligible_atoms(&document, &[nitrogen]).is_empty());
    assert!(draw_as_charge_separated(&mut document, &[nitrogen]).is_err());
    assert_eq!(document, source);
}

#[cfg(not(target_arch = "wasm32"))]
async fn identical_inchikey((mut document, nitrogen, _): (Document, u64, u64)) {
    use chemistry::inchi::{generator, helper, kernel, key};
    use std::time::Duration;

    if std::env::var_os("RESHIKI_INCHI_HELPER").is_none() {
        assert!(
            std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
            "RESHIKI_REQUIRE_INCHI_HELPER requires RESHIKI_INCHI_HELPER for depiction identity coverage"
        );
        eprintln!("Set RESHIKI_INCHI_HELPER to run charge-separated depiction InChIKey coverage");
        return;
    }
    let helper = helper::discover().unwrap();
    let before = chemistry_document::prepare(&document).unwrap();
    let input = kernel::Molecule::prepare(&before.state, Some(&before.positions)).unwrap();
    let before = generator::generate(&helper, &input, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(before.status.is_success(), "{before:?}");
    assert!(!before.inchi.is_empty());
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(1));
    let after = chemistry_document::prepare(&document).unwrap();
    let input = kernel::Molecule::prepare(&after.state, Some(&after.positions)).unwrap();
    let after = generator::generate(&helper, &input, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(after.status.is_success(), "{after:?}");
    assert!(!after.inchi.is_empty());
    assert_eq!(
        key::from_inchi(&before.inchi).unwrap(),
        key::from_inchi(&after.inchi).unwrap()
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn nitromethane_inchikey_is_identical_after_charge_separation() {
    identical_inchikey(nitromethane()).await;
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn trimethylamine_oxide_inchikey_is_identical_after_charge_separation() {
    identical_inchikey(trimethylamine_oxide(2)).await;
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn nitrone_inchikey_is_identical_after_charge_separation() {
    identical_inchikey(nitrone()).await;
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn methyl_azide_inchikey_is_identical_after_charge_separation() {
    identical_inchikey(methyl_azide()).await;
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn nitrogen_oxygen_dative_inchikey_is_identical_after_charge_separation() {
    identical_inchikey(trimethylamine_oxide(5)).await;
}

#[test]
fn nitrogen_next_to_an_abbreviated_substituent_stays_eligible() {
    let (mut document, nitrogen, other) = nitromethane();
    document
        .abbreviations
        .push(crate::abbreviations::Abbreviation {
            label: "Me".into(),
            reverse_label: "Me".into(),
            anchor: 1,
            exit: None,
            members: vec![1],
            label_style: None,
            label_color_override: false,
            highlight: None,
            alignment: Default::default(),
        });
    assert_eq!(eligible_atoms(&document, &[nitrogen]), vec![nitrogen]);
    assert_eq!(draw_as_charge_separated(&mut document, &[nitrogen]), Ok(1));
    assert_eq!(document.atom(other).unwrap().charge, -1);
    // The abbreviation's own atoms stay as drawn.
    let (mut document, nitrogen, _) = nitromethane();
    document
        .abbreviations
        .push(crate::abbreviations::Abbreviation {
            label: "NO2".into(),
            reverse_label: "O2N".into(),
            anchor: nitrogen,
            exit: None,
            members: vec![nitrogen, 3, 4],
            label_style: None,
            label_color_override: false,
            highlight: None,
            alignment: Default::default(),
        });
    assert!(eligible_atoms(&document, &[nitrogen]).is_empty());
}
