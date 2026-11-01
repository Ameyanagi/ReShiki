use super::*;
use crate::{atom_labels::StereoDisplay, document::Point, palette::Color};

#[test]
fn plain_bond_matches_the_fresh_bond_literals() {
    for (order, display) in [(1, "plain"), (DATIVE, "plain")] {
        // The three former literals have these same sixteen fields and values.
        let expected = Bond {
            distance_derived: false,
            highlight: None,
            ring_arc: false,
            projection: false,
            stereo_authoritative: false,
            z_order: 0,
            indicator: Default::default(),
            cip_label: None,
            undefined_stereo: false,
            a: 1,
            b: 2,
            order,
            display: display.into(),
            stereo: None,
            stereo_atoms: vec![],
            double_position: Default::default(),
            secondary_display: None,
            color: Default::default(),
        };
        let actual = Bond::plain(1, 2, order, display);
        assert_eq!(actual.highlight, expected.highlight);
        assert_eq!(actual.ring_arc, expected.ring_arc);
        assert_eq!(actual.projection, expected.projection);
        assert_eq!(actual.stereo_authoritative, expected.stereo_authoritative);
        assert_eq!(actual.z_order, expected.z_order);
        assert_eq!(actual.indicator, expected.indicator);
        assert_eq!(actual.cip_label, expected.cip_label);
        assert_eq!(actual.a, expected.a);
        assert_eq!(actual.b, expected.b);
        assert_eq!(actual.order, expected.order);
        assert_eq!(actual.display, expected.display);
        assert_eq!(actual.stereo, expected.stereo);
        assert_eq!(actual.stereo_atoms, expected.stereo_atoms);
        assert_eq!(actual.double_position, expected.double_position);
        assert_eq!(actual.secondary_display, expected.secondary_display);
        assert_eq!(actual.color, expected.color);
        assert_eq!(actual, expected);
    }
}

#[test]
fn add_bond_builds_on_plain_and_keeps_retained_style() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(28., 0.));
    doc.add_bond(a, b, 1, "plain");
    assert_eq!(doc.bonds, vec![Bond::plain(a, b, 1, "plain")]);

    let indicator = StereoDisplay {
        show: Some(true),
        offset: Some(Point::new(2., 3.)),
        style: Default::default(),
    };
    let bond = &mut doc.bonds[0];
    bond.highlight = Some(Color::Custom([12, 34, 56]));
    bond.z_order = 7;
    bond.indicator = indicator.clone();
    bond.double_position = DoublePosition::Left;
    bond.color = Color::Custom([65, 43, 21]);
    bond.ring_arc = true;
    bond.projection = true;
    bond.stereo_authoritative = true;
    bond.stereo = Some("E".into());
    bond.stereo_atoms = vec![a, b];
    bond.secondary_display = Some("dashed".into());
    bond.cip_label = Some("E".into());

    doc.add_bond(b, a, 2, "bold");
    assert_eq!(doc.bonds.len(), 1);
    let actual = &doc.bonds[0];
    assert_eq!(actual.highlight, Some(Color::Custom([12, 34, 56])));
    assert_eq!(actual.z_order, 7);
    assert_eq!(actual.indicator, indicator);
    assert_eq!(actual.double_position, DoublePosition::Left);
    assert_eq!(actual.color, Color::Custom([65, 43, 21]));

    let mut expected = Bond::plain(b, a, 2, "bold");
    assert_eq!(actual.ring_arc, expected.ring_arc);
    assert_eq!(actual.projection, expected.projection);
    assert_eq!(actual.stereo_authoritative, expected.stereo_authoritative);
    assert_eq!(actual.cip_label, expected.cip_label);
    assert_eq!(actual.a, expected.a);
    assert_eq!(actual.b, expected.b);
    assert_eq!(actual.order, expected.order);
    assert_eq!(actual.display, expected.display);
    assert_eq!(actual.stereo, expected.stereo);
    assert_eq!(actual.stereo_atoms, expected.stereo_atoms);
    assert_eq!(actual.secondary_display, expected.secondary_display);
    expected.highlight = actual.highlight;
    expected.z_order = actual.z_order;
    expected.indicator = actual.indicator.clone();
    expected.double_position = actual.double_position;
    expected.color = actual.color;
    assert_eq!(actual, &expected);
}

#[test]
fn every_allowed_dative_display_round_trips() {
    let projected_displays = [
        "plain",
        "dashed",
        "wedge",
        "hash",
        "bold",
        "hashed",
        "hollow_wedge",
        "wedge_end",
        "hash_end",
        "hollow_wedge_end",
    ];
    for (projection, displays) in [
        (true, projected_displays.as_slice()),
        (false, ["plain", "dashed"].as_slice()),
    ] {
        for display in displays {
            let mut bond = Bond::plain(1, 2, DATIVE, display);
            bond.projection = projection;
            assert_eq!(bond.validate_appearance(), Ok(()));
            let bytes = serde_json::to_vec(&bond).unwrap();
            let restored: Bond = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(restored, bond);
            assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
            assert_eq!(bond.narrowed_at_acceptor(), display.ends_with("_end"));
            if display.ends_with("_end") {
                assert_eq!(narrowed_display(bond.base_display()), *display);
            } else {
                assert_eq!(bond.base_display(), *display);
            }
            // CDXML derives this flag even when plain/dashed is saved as projected.
            let imported_projection = !matches!(*display, "plain" | "dashed");
            assert_eq!(dative_projects(bond.base_display()), imported_projection);
            let original = bond.clone();
            bond.reverse_projection();
            bond.reverse_projection();
            assert_eq!(bond, original);
        }
    }
}

#[test]
fn dative_predicates_require_the_directed_order() {
    for order in 0..=7 {
        for projection in [false, true] {
            let mut bond = Bond::plain(1, 2, order, "wedge_end");
            bond.projection = projection;
            assert_eq!(bond.is_dative(), order == DATIVE);
            assert_eq!(bond.is_projected_dative(), order == DATIVE && projection);
            assert_eq!(bond.narrowed_at_acceptor(), order == DATIVE);
        }
    }
    let repeated_suffix = Bond::plain(1, 2, DATIVE, "wedge_end_end");
    assert_eq!(repeated_suffix.base_display(), "wedge_end");
}

#[test]
fn covalent_orders_exclude_hydrogen_interactions_and_dative_bonds() {
    for order in 0..=7 {
        let bond = Bond::plain(1, 2, order, "plain");
        assert_eq!(bond.is_covalent(), [1, 2, 3, 4, 6, 7].contains(&order));
    }
}

#[test]
fn stereo_markers_are_the_six_chemical_displays() {
    for display in ["wedge", "hollow_wedge", "bold", "hash", "hashed", "wavy"] {
        assert!(is_stereo_marker(display), "{display}");
    }
    for display in ["plain", "dashed", "dotted", "wedge_end", "unknown", ""] {
        assert!(!is_stereo_marker(display), "{display}");
    }
}

#[test]
fn specifies_stereo_accepts_each_term_and_excludes_projection_markers() {
    let plain = Bond::plain(1, 2, 1, "plain");
    assert!(!plain.specifies_stereo());

    let mut captured = plain.clone();
    captured.stereo = Some("E".into());
    captured.projection = true;
    assert!(captured.specifies_stereo());

    let mut authoritative = plain.clone();
    authoritative.stereo_authoritative = true;
    authoritative.projection = true;
    assert!(authoritative.specifies_stereo());

    let mut neighbors = plain;
    neighbors.stereo_atoms = vec![3, 4];
    neighbors.projection = true;
    assert!(neighbors.specifies_stereo());

    for display in ["wedge", "hollow_wedge", "bold", "hash", "hashed", "wavy"] {
        let mut marker = Bond::plain(1, 2, 1, display);
        assert!(marker.specifies_stereo(), "{display}");
        marker.projection = true;
        assert!(!marker.specifies_stereo(), "{display}");
    }
}

#[test]
fn connects_accepts_either_endpoint_order() {
    let bond = Bond::plain(1, 2, 1, "plain");
    assert!(bond.connects(1, 2));
    assert!(bond.connects(2, 1));
    assert!(!bond.connects(1, 3));
    assert!(!bond.connects(1, 1));
}

#[test]
fn bond_between_returns_the_first_match_with_its_index() {
    let doc = Document {
        bonds: vec![
            Bond::plain(3, 4, 1, "plain"),
            Bond::plain(2, 1, DATIVE, "plain"),
            Bond::plain(1, 2, 2, "plain"),
        ],
        ..Document::default()
    };
    assert_eq!(doc.bond_between(1, 2), Some((1, &doc.bonds[1])));
    assert_eq!(doc.bond_between(2, 1), Some((1, &doc.bonds[1])));
    assert_eq!(doc.bond_between(1, 3), None);
}

#[test]
fn reverse_drawn_swaps_plain_datives_and_flips_projected_tips() {
    let mut plain_dative = Bond::plain(1, 2, DATIVE, "plain");
    plain_dative.stereo_atoms = vec![3, 4];
    plain_dative.double_position = DoublePosition::Left;
    let original = plain_dative.clone();
    plain_dative.reverse_drawn();
    let mut reversed = original.clone();
    reversed.a = 2;
    reversed.b = 1;
    reversed.stereo_atoms = vec![4, 3];
    reversed.double_position = DoublePosition::Right;
    assert_eq!(plain_dative, reversed);
    let mut reversed_directly = original.clone();
    reversed_directly.reverse();
    assert_eq!(plain_dative, reversed_directly);

    let mut projected = original.clone();
    projected.projection = true;
    projected.display = "wedge".into();
    let mut expected = projected.clone();
    expected.display = "wedge_end".into();
    projected.reverse_drawn();
    assert_eq!(projected, expected);

    for projection in [false, true] {
        let mut ordinary = original.clone();
        ordinary.order = 1;
        ordinary.projection = projection;
        let mut expected = reversed.clone();
        expected.order = 1;
        expected.projection = projection;
        ordinary.reverse_drawn();
        assert_eq!(ordinary, expected);
    }
}

fn first_wedge_chain() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let left = doc.add_atom("C", Point::default());
    let center = doc.add_atom("C", Point::new(84., 0.));
    let tail = doc.add_atom("C", Point::new(-84., 0.));
    let above = doc.add_atom("F", Point::new(84., -84.));
    let below = doc.add_atom("Cl", Point::new(84., 84.));
    // The chain was drawn toward the center, so its stored direction starts
    // at the degree-two atom. Only the degree-three center is tetrahedral.
    doc.add_bond(left, center, 1, "plain");
    doc.add_bond(left, tail, 1, "plain");
    doc.add_bond(center, above, 1, "plain");
    doc.add_bond(center, below, 1, "plain");
    (doc, left, center)
}

#[test]
fn first_wedge_narrows_at_the_only_tetrahedral_endpoint_in_either_gesture_direction() {
    for preset in [
        BondPreset::Wedge,
        BondPreset::HashedWedge,
        BondPreset::HollowWedge,
    ] {
        for reversed_gesture in [false, true] {
            let (mut doc, left, center) = first_wedge_chain();
            let (a, b) = if reversed_gesture {
                (center, left)
            } else {
                (left, center)
            };
            preset.place(&mut doc, a, b);
            assert_eq!((doc.bonds[0].a, doc.bonds[0].b), (center, left));
            assert_eq!(doc.bonds[0].display, preset.parts().1);
            assert_eq!(doc.bonds[0].order, 1);
        }
    }
}

#[test]
fn first_wedge_uses_local_valence_and_explicit_hydrogens_including_simple_p_s() {
    for (element, charge, no_implicit, explicit_h, expected) in [
        ("C", 0, false, 0, true),
        ("C", 0, true, 1, true),
        ("C", 0, true, 0, false),
        ("N", 0, false, 0, false),
        ("N", 1, false, 0, true),
        ("P", 0, true, 0, true),
        ("S", 1, true, 0, true),
    ] {
        let (mut doc, left, center) = first_wedge_chain();
        let atom = doc.atom_mut(center).unwrap();
        atom.element = element.into();
        atom.charge = charge;
        atom.no_implicit = no_implicit;
        atom.explicit_h = explicit_h;
        atom.label_h = 9; // Computed display labels must not drive the choice.
        BondPreset::Wedge.place(&mut doc, left, center);
        assert_eq!(
            doc.bonds[0].a == center,
            expected,
            "{element}, charge {charge}"
        );
    }
    for (element, neighbors) in [("C", 4), ("P", 5), ("S", 6)] {
        let (mut doc, left, center) = first_wedge_chain();
        let atom = doc.atom_mut(center).unwrap();
        atom.element = element.into();
        atom.no_implicit = true;
        for n in 3..neighbors {
            let added = doc.add_atom("F", Point::new(168., n as f32 * 42.));
            doc.add_bond(center, added, 1, "plain");
        }
        BondPreset::Wedge.place(&mut doc, left, center);
        assert_eq!(doc.bonds[0].a, center, "{element}, degree {neighbors}");
    }
}

#[test]
fn first_wedge_preserves_stored_direction_when_both_or_neither_endpoint_qualifies() {
    for both_qualify in [false, true] {
        let (mut doc, left, center) = first_wedge_chain();
        if both_qualify {
            let extra = doc.add_atom("F", Point::new(0., 84.));
            doc.add_bond(left, extra, 1, "plain");
        } else {
            doc.bonds.pop(); // Both endpoints now have degree two.
        }
        BondPreset::Wedge.place(&mut doc, center, left);
        assert_eq!((doc.bonds[0].a, doc.bonds[0].b), (left, center));
    }
    let (mut doc, left, center) = first_wedge_chain();
    doc.bonds.truncate(1); // Both terminal endpoints also remain ambiguous.
    BondPreset::Wedge.place(&mut doc, center, left);
    assert_eq!((doc.bonds[0].a, doc.bonds[0].b), (left, center));
}

#[test]
fn first_wedge_prefers_nonterminal_end_but_never_an_endpoint_with_multiple_bonds() {
    for neighbor_order in [1, 2, 3, 4, 6, 7] {
        let (mut doc, left, center) = first_wedge_chain();
        doc.bonds.remove(1); // Left is terminal; center remains degree three.
        doc.bonds[1].order = neighbor_order;
        // Fixed hydrogens keep this center from passing the tetrahedral rule,
        // so the single-bond case exercises only the nonterminal fallback.
        doc.atom_mut(center).unwrap().no_implicit = true;
        BondPreset::Wedge.place(&mut doc, left, center);
        let expected = if neighbor_order == 1 { center } else { left };
        assert_eq!(doc.bonds[0].a, expected, "neighbor order {neighbor_order}");
    }
    let (mut doc, left, center) = first_wedge_chain();
    doc.bonds[2].order = 2;
    BondPreset::Wedge.place(&mut doc, left, center);
    assert_eq!(
        doc.bonds[0].a, left,
        "Multiple bonds also exclude tetrahedral candidates"
    );
}

#[test]
fn first_wedge_terminal_fallback_ignores_drawn_hydrogens() {
    for preset in [
        BondPreset::Wedge,
        BondPreset::HashedWedge,
        BondPreset::HollowWedge,
    ] {
        for drawn_hydrogens in [0, 1, 3] {
            for reversed_gesture in [false, true] {
                let mut doc = Document::default();
                let terminal = doc.add_atom("C", Point::default());
                let middle = doc.add_atom("C", Point::new(84., 0.));
                let tail = doc.add_atom("C", Point::new(168., 0.));
                doc.add_bond(terminal, middle, 1, "plain");
                doc.add_bond(middle, tail, 1, "plain");
                for n in 0..drawn_hydrogens {
                    let h = doc.add_atom("H", Point::new(-84., n as f32 * 42.));
                    doc.add_bond(terminal, h, 1, "plain");
                }
                let (a, b) = if reversed_gesture {
                    (middle, terminal)
                } else {
                    (terminal, middle)
                };
                preset.place(&mut doc, a, b);
                assert_eq!(
                    (doc.bonds[0].a, doc.bonds[0].b),
                    (middle, terminal),
                    "{preset}, {drawn_hydrogens} drawn H, reversed gesture {reversed_gesture}"
                );
                assert_eq!(doc.bonds[0].display, preset.parts().1);
            }
        }
    }
}

#[test]
fn first_wedge_endpoint_choice_excludes_existing_styles_projection_and_new_bonds() {
    for (order, display, projection) in [
        (1, "wedge", false),
        (1, "hash", false),
        (1, "hollow_wedge", false),
        (1, "bold", false),
        (1, "hashed", false),
        (1, "wavy", false),
        (1, "plain", true),
        (2, "plain", false),
        (4, "plain", false),
        (4, "plain", true),
        (DATIVE, "plain", false),
        (DATIVE, "plain", true),
    ] {
        let (mut doc, left, center) = first_wedge_chain();
        doc.bonds[0].order = order;
        doc.bonds[0].display = display.into();
        doc.bonds[0].projection = projection;
        BondPreset::Wedge.place(&mut doc, left, center);
        assert_eq!(
            (doc.bonds[0].a, doc.bonds[0].b),
            (left, center),
            "{order}, {display}, projection {projection}"
        );
    }
    for preset in [BondPreset::Bold, BondPreset::Hashed, BondPreset::Wavy] {
        let (mut doc, left, center) = first_wedge_chain();
        preset.place(&mut doc, left, center);
        assert_eq!(doc.bonds[0].a, left);
    }
    let (mut doc, left, center) = first_wedge_chain();
    doc.bonds.remove(0);
    BondPreset::Wedge.place(&mut doc, left, center);
    let added = doc.bonds.last().unwrap();
    assert_eq!((added.a, added.b), (left, center));
}

#[test]
fn first_wedge_ignores_drawn_hydrogens_when_counting_substituents() {
    let (mut doc, left, center) = first_wedge_chain();
    let h = doc.add_atom("H", Point::new(0., 84.));
    doc.add_bond(left, h, 1, "plain");
    BondPreset::Wedge.place(&mut doc, left, center);
    assert_eq!((doc.bonds[0].a, doc.bonds[0].b), (center, left));
}
