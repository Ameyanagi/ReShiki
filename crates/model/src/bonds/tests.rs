use super::*;
use crate::{atom_labels::StereoDisplay, document::Point, palette::Color};

#[test]
fn plain_bond_matches_the_fresh_bond_literals() {
    for (order, display) in [(1, "plain"), (DATIVE, "plain")] {
        // The three former literals have these same sixteen fields and values.
        let expected = Bond {
            highlight: None,
            ring_arc: false,
            projection: false,
            stereo_authoritative: false,
            z_order: 0,
            indicator: Default::default(),
            cip_label: None,
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
fn reverse_drawn_keeps_each_callers_dative_semantics() {
    let mut plain_dative = Bond::plain(1, 2, DATIVE, "plain");
    plain_dative.stereo_atoms = vec![3, 4];
    plain_dative.double_position = DoublePosition::Left;
    let original = plain_dative.clone();
    plain_dative.reverse_drawn(false);
    assert_eq!(plain_dative, original);
    plain_dative.reverse_drawn(true);
    let mut reversed = original.clone();
    reversed.a = 2;
    reversed.b = 1;
    reversed.stereo_atoms = vec![4, 3];
    reversed.double_position = DoublePosition::Right;
    assert_eq!(plain_dative, reversed);

    for swap_plain_dative in [false, true] {
        let mut projected = original.clone();
        projected.projection = true;
        projected.display = "wedge".into();
        let mut expected = projected.clone();
        expected.display = "wedge_end".into();
        projected.reverse_drawn(swap_plain_dative);
        assert_eq!(projected, expected);

        for projection in [false, true] {
            let mut ordinary = original.clone();
            ordinary.order = 1;
            ordinary.projection = projection;
            let mut expected = reversed.clone();
            expected.order = 1;
            expected.projection = projection;
            ordinary.reverse_drawn(swap_plain_dative);
            assert_eq!(ordinary, expected);
        }
    }
}
