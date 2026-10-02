//! Exercise the real ignored-key dispatcher, hover resolution and app history.
use super::*;
use crate::canvas::Edit;
use reshiki::{atom_labels::HydrogenPosition, scene::Primitive};

fn key(app: &mut App, text: &str) {
    let modifiers = if text.chars().any(char::is_uppercase) {
        Modifiers::SHIFT
    } else {
        Modifiers::empty()
    };
    let raw = Key::Character(text.to_ascii_lowercase().into());
    let modified = Key::Character(text.into());
    let message = key_message(&raw, &modified, modifiers).expect("Registered character shortcut");
    let _ = app.update(message);
}

fn fixture(element: &str) -> (App, u64, Vec<u64>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.bond_drawing.length = 42.;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let ring = app.tab.doc.atoms[0].clone();
    let target = app.tab.doc.add_atom(element, ring.position.offset(42., 0.));
    app.tab.doc.add_bond(ring.id, target, 1, "plain");
    let other = app.tab.doc.add_atom("C", Point::new(400., 200.));
    let end = app.tab.doc.add_atom("C", Point::new(442., 200.));
    app.tab.doc.add_bond(other, end, 1, "plain");
    // Bond edits invalidate computed H counts across the document. Populate
    // the visible label only after the entire fixture graph is constructed.
    let atom = app.tab.doc.atom_mut(target).unwrap();
    atom.label_h = match element {
        "C" => 3,
        "N" | "P" => 2,
        "O" | "S" => 1,
        _ => 0,
    };
    atom.display.hydrogens = Some(true);
    atom.display.hydrogen_position = HydrogenPosition::Right;
    let unrelated = vec![other, end];
    app.tab.selected = unrelated.clone();
    (app, target, unrelated)
}

/// An actual rendered glyph away from the atom center, rather than a hit-test
/// implementation's own expected rectangle.
#[track_caller]
fn glyph_center(doc: &Document, text: &str) -> Point {
    let runs: Vec<_> = reshiki::scene::primitives(doc)
        .into_iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text {
                position,
                text: run,
                size,
                style,
                ..
            } => Some((position, run, size, style)),
            _ => None,
        })
        .collect();
    runs.iter()
        .find_map(|(position, run, size, style)| {
            run.find(text).map(|start| {
                position.offset(
                    reshiki::style::styled_text_width(&run[..start], *size, style)
                        + reshiki::style::styled_text_width(text, *size, style) / 2.,
                    *size / 2.,
                )
            })
        })
        .unwrap_or_else(|| {
            panic!(
                "Expected rendered label glyph {text:?}; actual runs: {:?}",
                runs.iter().map(|(_, run, _, _)| run).collect::<Vec<_>>()
            )
        })
}

fn hover(app: &mut App, point: Point) {
    let _ = app.update(Message::Canvas(Edit::Hover(Some(point))));
}

fn assert_unrelated(before: &Document, after: &Document, ids: &[u64]) {
    for id in ids {
        assert_eq!(after.atom(*id), before.atom(*id), "Unrelated atom {id}");
    }
    let bonds = |doc: &Document| {
        doc.bonds
            .iter()
            .filter(|bond| ids.contains(&bond.a) || ids.contains(&bond.b))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(bonds(after), bonds(before), "Unrelated bonds");
}

fn assert_one_undo(app: &mut App, before: &Document) {
    let changed = app.tab.doc.clone();
    assert_ne!(&changed, before);
    let _ = app.update(Message::Undo);
    assert_eq!(&app.tab.doc, before);
    assert!(
        !app.tab.history.can_undo(),
        "One shortcut is one history entry"
    );
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, changed);
}

#[test]
fn aniline_and_toluene_dimethyl_shortcut_grows_on_the_hovered_center() {
    for element in ["C", "N"] {
        let (mut app, target, unrelated) = fixture(element);
        let before = app.tab.doc.clone();
        let point = app.tab.doc.atom(target).unwrap().position;
        hover(&mut app, point);
        key(&mut app, "9");
        assert!(!app.error, "{element}: {}", app.status);
        let added: Vec<_> = app
            .tab
            .doc
            .atoms
            .iter()
            .filter(|atom| before.atom(atom.id).is_none())
            .collect();
        assert_eq!(
            added.len(),
            2,
            "Two methyls, without an inserted carbon center on N"
        );
        assert!(added.iter().all(|atom| atom.element == "C"));
        for atom in added {
            let bonds: Vec<_> = app
                .tab
                .doc
                .bonds
                .iter()
                .filter(|bond| bond.a == atom.id || bond.b == atom.id)
                .collect();
            assert_eq!(bonds.len(), 1);
            assert!(bonds[0].a == target || bonds[0].b == target);
            assert_eq!(bonds[0].order, 1);
        }
        assert_eq!(app.tab.doc.atom(target).unwrap().element, element);
        assert_eq!(app.tab.doc.atom(target).unwrap().position, point);
        assert_eq!(
            app.tab.selected,
            vec![target],
            "Keep the growth hotspot on its center"
        );
        assert_unrelated(&before, &app.tab.doc, &unrelated);
        assert_one_undo(&mut app, &before);
    }
}

#[test]
fn dimethyl_rejects_insufficient_heteroatom_valence_without_adding_a_carbon() {
    // Aniline is the native ChemDraw parity oracle above. These are ReShiki
    // chemistry/transaction contracts, not claims about ChemDraw's warnings.
    for element in ["O", "S", "P", "N"] {
        let (mut app, target, unrelated) = fixture(element);
        if element == "N" {
            let other = app.tab.doc.add_atom(
                "C",
                app.tab
                    .doc
                    .atom(target)
                    .unwrap()
                    .position
                    .offset(21., -36.373),
            );
            app.tab.doc.add_bond(target, other, 1, "plain");
        }
        let before = app.tab.doc.clone();
        let point = before.atom(target).unwrap().position;
        hover(&mut app, point);
        key(&mut app, "9");
        assert!(
            app.error,
            "{element}: insufficient/unsupported valence must not choose another group"
        );
        assert!(app.status.contains("two methyl groups"));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, unrelated);
        assert!(!app.tab.history.can_undo());
    }
    for element in ["N", "O", "S"] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc = Document::default();
        let target = app.tab.doc.add_atom(element, Point::default());
        let before = app.tab.doc.clone();
        hover(&mut app, Point::default());
        key(&mut app, "9");
        assert!(!app.error, "{element}: {}", app.status);
        assert_eq!(app.tab.doc.atoms.len(), 3);
        assert_eq!(app.tab.doc.bonds.len(), 2);
        assert!(
            app.tab
                .doc
                .bonds
                .iter()
                .all(|bond| (bond.a == target || bond.b == target) && bond.order == 1)
        );
        assert_eq!(app.tab.doc.atom(target).unwrap().element, element);
        assert_one_undo(&mut app, &before);
    }
}

#[test]
fn saturated_ring_hotkeys_share_the_aniline_nitrogen_without_an_extra_carbon() {
    // ChemDraw 26's aniline+6 file is the independent N-phenylpiperidine
    // graph oracle. The same valid sharing applies to its smaller ring family.
    for (shortcut, size) in [("6", 6), ("7", 5), ("u", 4), ("v", 3)] {
        let (mut app, target, unrelated) = fixture("N");
        let before = app.tab.doc.clone();
        let point = before.atom(target).unwrap().position;
        hover(&mut app, point);
        key(&mut app, shortcut);
        assert!(!app.error, "{shortcut}: {}", app.status);
        let mut ring: Vec<_> = app
            .tab
            .doc
            .atoms
            .iter()
            .filter(|atom| before.atom(atom.id).is_none())
            .map(|atom| atom.id)
            .collect();
        assert_eq!(
            ring.len(),
            size - 1,
            "The existing nitrogen is one of the ring vertices"
        );
        assert!(
            ring.iter()
                .all(|id| app.tab.doc.atom(*id).unwrap().element == "C")
        );
        ring.push(target);
        let ring_bonds: Vec<_> = app
            .tab
            .doc
            .bonds
            .iter()
            .filter(|bond| ring.contains(&bond.a) && ring.contains(&bond.b))
            .collect();
        assert_eq!(ring_bonds.len(), size);
        assert!(ring_bonds.iter().all(|bond| bond.order == 1));
        for id in &ring {
            assert_eq!(
                ring_bonds
                    .iter()
                    .filter(|bond| bond.a == *id || bond.b == *id)
                    .count(),
                2
            );
        }
        assert_eq!(
            app.tab
                .doc
                .bonds
                .iter()
                .filter(|bond| bond.a == target || bond.b == target)
                .count(),
            3
        );
        assert_eq!(app.tab.doc.atom(target).unwrap().element, "N");
        assert_eq!(app.tab.doc.atom(target).unwrap().charge, 0);
        assert_eq!(app.tab.doc.atom(target).unwrap().position, point);
        assert_eq!(app.tab.doc.bonds.len(), before.bonds.len() + size);
        assert_unrelated(&before, &app.tab.doc, &unrelated);
        assert_one_undo(&mut app, &before);
    }
    // Explicitly entered hydrogens remain a protected chemical constraint.
    for shortcut in ["9", "6", "7", "u", "v"] {
        let (mut app, target, _) = fixture("N");
        app.tab.doc =
            reshiki::atom_text::apply(&app.tab.doc, target, "NH2", reshiki::atom_text::Mode::Auto)
                .unwrap();
        let before = app.tab.doc.clone();
        hover(&mut app, before.atom(target).unwrap().position);
        key(&mut app, shortcut);
        assert!(
            app.error,
            "{shortcut}: fixed hydrogens require an explicit edit"
        );
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn aniline_group_hotkeys_keep_valid_acetyl_tert_butyl_and_phenyl_attachments() {
    // K matches the native reference. Native 2/3 produce a neutral nitrogen
    // valence warning; ReShiki deliberately keeps these valid group attachments.
    for (shortcut, added_c, added_o) in [("2", 2, 1), ("K", 4, 0), ("3", 6, 0), ("a", 6, 0)] {
        let (mut app, target, unrelated) = fixture("N");
        let before = app.tab.doc.clone();
        hover(&mut app, before.atom(target).unwrap().position);
        key(&mut app, shortcut);
        assert!(!app.error, "{shortcut}: {}", app.status);
        let added: Vec<_> = app
            .tab
            .doc
            .atoms
            .iter()
            .filter(|atom| before.atom(atom.id).is_none())
            .collect();
        assert_eq!(
            added.iter().filter(|atom| atom.element == "C").count(),
            added_c
        );
        assert_eq!(
            added.iter().filter(|atom| atom.element == "O").count(),
            added_o
        );
        assert_eq!(added.len(), added_c + added_o);
        let neighbors: Vec<_> = app
            .tab
            .doc
            .bonds
            .iter()
            .filter_map(|bond| {
                if bond.a == target {
                    Some((bond.b, bond.order))
                } else if bond.b == target {
                    Some((bond.a, bond.order))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.iter().all(|(_, order)| *order == 1));
        assert_eq!(
            neighbors
                .iter()
                .filter(|(id, _)| before.atom(*id).is_none())
                .count(),
            1
        );
        let attached = neighbors
            .iter()
            .find(|(id, _)| before.atom(*id).is_none())
            .unwrap()
            .0;
        assert_eq!(app.tab.doc.atom(attached).unwrap().element, "C");
        if shortcut == "2" {
            assert!(app.tab.doc.bonds.iter().any(|bond| bond.order == 2
                && ((bond.a == attached && app.tab.doc.atom(bond.b).unwrap().element == "O")
                    || (bond.b == attached && app.tab.doc.atom(bond.a).unwrap().element == "O"))));
        }
        assert_unrelated(&before, &app.tab.doc, &unrelated);
        assert_one_undo(&mut app, &before);
    }
}

#[test]
fn atom_growth_keys_preserve_the_target_and_add_the_documented_carbon_graph() {
    for (shortcut, order, display, added_c, added_o) in [
        ("0", 1, "plain", 1, 0),
        ("1", 1, "plain", 1, 0),
        ("4", 1, "wedge", 1, 0),
        ("5", 1, "hash", 1, 0),
        ("8", 2, "plain", 1, 0),
        ("z", 3, "plain", 1, 0),
        ("2", 2, "plain", 1, 1),
        ("9", 1, "plain", 2, 0),
        ("K", 1, "plain", 3, 0),
    ] {
        let (mut app, target, unrelated) = fixture("C");
        let before = app.tab.doc.clone();
        let point = before.atom(target).unwrap().position;
        hover(&mut app, point);
        key(&mut app, shortcut);
        assert!(!app.error, "{shortcut}: {}", app.status);
        let added: Vec<_> = app
            .tab
            .doc
            .atoms
            .iter()
            .filter(|atom| before.atom(atom.id).is_none())
            .collect();
        assert_eq!(
            added.iter().filter(|atom| atom.element == "C").count(),
            added_c,
            "{shortcut}: added carbons"
        );
        assert_eq!(
            added.iter().filter(|atom| atom.element == "O").count(),
            added_o,
            "{shortcut}: added oxygens"
        );
        assert_eq!(added.len(), added_c + added_o);
        assert!(
            added.iter().all(|atom| app
                .tab
                .doc
                .bonds
                .iter()
                .any(|bond| (bond.a == target && bond.b == atom.id)
                    || (bond.b == target && bond.a == atom.id))),
            "{shortcut}: new atoms attach directly to the carbon hotspot"
        );
        assert!(
            app.tab
                .doc
                .bonds
                .iter()
                .filter(|bond| (bond.a == target && before.atom(bond.b).is_none())
                    || (bond.b == target && before.atom(bond.a).is_none()))
                .any(|bond| bond.order == order && bond.display == display),
            "{shortcut}: bond order/style"
        );
        assert_eq!(app.tab.doc.atom(target).unwrap().position, point);
        assert_eq!(app.tab.doc.atom(target).unwrap().element, "C");
        assert_unrelated(&before, &app.tab.doc, &unrelated);
        assert_one_undo(&mut app, &before);
    }
}

#[test]
fn terminal_carbonyl_keeps_carbon_on_the_chain_and_oxygen_on_the_other_side() {
    let close = |actual: Point, expected: Point| {
        assert!(
            actual.distance(expected) < 0.0002,
            "{actual:?} != {expected:?}"
        );
    };
    for length in [24_f32, 42., 60.] {
        for degrees in [0_f32, 17.3, 90., 211.] {
            for mirror in [-1_f32, 1.] {
                let (sin, cos) = degrees.to_radians().sin_cos();
                let world = |x: f32, y: f32| {
                    Point::new(
                        137. + x * cos - mirror * y * sin,
                        -91. + x * sin + mirror * y * cos,
                    )
                };
                let dx = 3_f32.sqrt() * length / 2.;
                let (mut app, _) = App::new();
                app.tab.busy = false;
                app.tab.doc = Document::default();
                app.tab.bond_drawing.length = length;
                let first = app.tab.doc.add_atom("C", world(-2. * dx, 0.));
                let previous = app.tab.doc.add_atom("C", world(-dx, length / 2.));
                let target = app.tab.doc.add_atom("C", world(0., 0.));
                app.tab.doc.add_bond(first, previous, 1, "plain");
                app.tab.doc.add_bond(previous, target, 1, "plain");
                let before = app.tab.doc.clone();
                hover(&mut app, world(0., 0.));
                key(&mut app, "2");
                assert!(!app.error, "{length}, {degrees}, {mirror}: {}", app.status);
                let methyl = app
                    .tab
                    .doc
                    .atoms
                    .iter()
                    .find(|atom| before.atom(atom.id).is_none() && atom.element == "C")
                    .unwrap();
                let oxygen = app
                    .tab
                    .doc
                    .atoms
                    .iter()
                    .find(|atom| atom.element == "O")
                    .unwrap();
                close(methyl.position, world(dx, length / 2.));
                close(oxygen.position, world(0., -length));
                assert_eq!(app.tab.selected, vec![methyl.id]);
                let methyl_id = methyl.id;
                let carbonyl = app.tab.doc.clone();
                assert_one_undo(&mut app, &before);
                key(&mut app, "1");
                assert!(!app.error, "Continue carbon chain: {}", app.status);
                let added = app
                    .tab
                    .doc
                    .atoms
                    .iter()
                    .find(|atom| carbonyl.atom(atom.id).is_none())
                    .unwrap();
                assert!(
                    app.tab
                        .doc
                        .bonds
                        .iter()
                        .any(|bond| (bond.a == methyl_id && bond.b == added.id)
                            || (bond.b == methyl_id && bond.a == added.id))
                );
                let continued = app.tab.doc.clone();
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, carbonyl);
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, before);
                assert!(!app.tab.history.can_undo());
                let _ = app.update(Message::Redo);
                let _ = app.update(Message::Redo);
                assert_eq!(app.tab.doc, continued);
            }
        }
    }
    // With just a horizontal incoming bond, its normal +60° growth goes to
    // carbon and the remaining -60° direction goes to oxygen.
    let (mut app, a, target, unrelated) = bond_fixture();
    let before = app.tab.doc.clone();
    hover(&mut app, Point::new(42., 0.));
    key(&mut app, "2");
    assert!(!app.error, "{}", app.status);
    let methyl = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|atom| before.atom(atom.id).is_none() && atom.element == "C")
        .unwrap();
    let oxygen = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|atom| atom.element == "O")
        .unwrap();
    close(methyl.position, Point::new(63., 21. * 3_f32.sqrt()));
    close(oxygen.position, Point::new(63., -21. * 3_f32.sqrt()));
    assert_eq!(app.tab.doc.atom(a), before.atom(a));
    assert_eq!(
        app.tab.doc.atom(target).unwrap().position,
        Point::new(42., 0.)
    );
    assert_unrelated(&before, &app.tab.doc, &unrelated);
    assert_one_undo(&mut app, &before);
}

#[test]
fn internal_carbonyl_adds_only_oxygen_and_keeps_the_existing_carbon_hotspot() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let target = app.tab.doc.add_atom("C", Point::default());
    for x in [-21. * 3_f32.sqrt(), 21. * 3_f32.sqrt()] {
        let other = app.tab.doc.add_atom("C", Point::new(x, 21.));
        app.tab.doc.add_bond(target, other, 1, "plain");
    }
    let before = app.tab.doc.clone();
    hover(&mut app, Point::default());
    key(&mut app, "2");
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 1);
    let oxygen = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|atom| atom.element == "O")
        .unwrap();
    assert!(oxygen.position.distance(Point::new(0., -42.)) < 0.0002);
    assert_eq!(app.tab.selected, vec![target]);
    assert_one_undo(&mut app, &before);
}

#[test]
fn far_hydrogen_label_targets_its_atom_for_growth_replacement_charge_and_properties() {
    for zoom in [0.5, 1., 3.] {
        for shortcut in ["1", "9", "o", "O", "+", "g", "?"] {
            let (mut app, target, unrelated) = fixture("N");
            app.tab.camera.zoom = zoom;
            // A large but valid label makes this independent of the center's
            // zoom-scaled hit radius at every tested zoom.
            app.tab.doc.atom_mut(target).unwrap().text_style =
                Some(reshiki::typography::TextStyle {
                    size_pt: 24.,
                    ..Default::default()
                });
            let point = glyph_center(&app.tab.doc, "2");
            assert!(point.distance(app.tab.doc.atom(target).unwrap().position) > 10. / zoom);
            assert_eq!(
                app.tab.doc.nearest(point, 10. / zoom),
                None,
                "Not a center or abbreviation hit"
            );
            let before = app.tab.doc.clone();
            // An unrelated single-atom selection would have been the fallback
            // target before ordinary label bounds participated in hit testing.
            app.tab.selected = vec![unrelated[0]];
            hover(&mut app, point);
            key(&mut app, shortcut);
            assert!(!app.error, "{zoom}, {shortcut}: {}", app.status);
            assert_unrelated(&before, &app.tab.doc, &unrelated);
            match shortcut {
                "1" => assert!(
                    app.tab
                        .doc
                        .bonds
                        .iter()
                        .any(|bond| (bond.a == target && before.atom(bond.b).is_none())
                            || (bond.b == target && before.atom(bond.a).is_none()))
                ),
                "9" => assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 2),
                "o" => assert_eq!(app.tab.doc.atom(target).unwrap().element, "O"),
                "O" => assert_eq!(app.tab.doc.abbreviation(target).unwrap().label, "OMe"),
                "+" => assert_eq!(app.tab.doc.atom(target).unwrap().charge, 1),
                "g" | "?" => {
                    assert_eq!(app.tab.selected, vec![target]);
                    assert_eq!(app.tab.doc, before);
                    assert!(!app.tab.history.can_undo());
                }
                _ => unreachable!(),
            }
            if !matches!(shortcut, "g" | "?") {
                assert_one_undo(&mut app, &before);
            }
        }
    }
}

#[test]
fn label_hotspots_follow_hydrogen_position_isotopes_charge_and_abbreviations() {
    for variant in [
        "left",
        "right",
        "above",
        "below",
        "isotope",
        "charge",
        "carbon",
        "variable",
        "abbreviation",
    ] {
        let (mut app, target, unrelated) = fixture("N");
        let glyph = match variant {
            "carbon" => {
                app.tab.doc = reshiki::atom_text::apply(
                    &app.tab.doc,
                    target,
                    "C",
                    reshiki::atom_text::Mode::Auto,
                )
                .unwrap();
                app.tab.doc.atom_mut(target).unwrap().label_h = 3;
                "3"
            }
            "variable" => {
                app.tab.doc = reshiki::atom_text::apply(
                    &app.tab.doc,
                    target,
                    "Rlong",
                    reshiki::atom_text::Mode::Text,
                )
                .unwrap();
                "g"
            }
            "abbreviation" => {
                app.tab.doc = reshiki::atom_text::apply(
                    &app.tab.doc,
                    target,
                    "OMe",
                    reshiki::atom_text::Mode::Group,
                )
                .unwrap();
                "e"
            }
            "isotope" => {
                app.tab.doc.atom_mut(target).unwrap().isotope = 15;
                "1"
            }
            "charge" => {
                app.tab.doc.atom_mut(target).unwrap().charge = 1;
                "+"
            }
            _ => {
                app.tab
                    .doc
                    .atom_mut(target)
                    .unwrap()
                    .display
                    .hydrogen_position = match variant {
                    "left" => HydrogenPosition::Left,
                    "above" => HydrogenPosition::Above,
                    "below" => HydrogenPosition::Below,
                    _ => HydrogenPosition::Right,
                };
                "2"
            }
        };
        app.tab.doc.atom_mut(target).unwrap().text_style = Some(reshiki::typography::TextStyle {
            size_pt: 24.,
            ..Default::default()
        });
        let before = app.tab.doc.clone();
        let point = glyph_center(&before, glyph);
        assert!(
            point.distance(before.atom(target).unwrap().position) > 10.,
            "{variant}: glyph must extend beyond center target"
        );
        app.tab.selected = vec![unrelated[0]];
        hover(&mut app, point);
        key(&mut app, "g");
        assert_eq!(
            app.tab.selected,
            vec![target],
            "{variant}: visible label owns the hotspot"
        );
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn every_atom_label_key_replaces_the_hovered_atom_and_is_one_undo_step() {
    // Published atom-label contracts, checked through dispatch and the app;
    // this deliberately does not derive expectations from atom_label().
    let elements = [
        ("b", "Br"),
        ("B", "B"),
        ("c", "C"),
        ("C", "Cl"),
        ("l", "Cl"),
        ("f", "F"),
        ("h", "H"),
        ("i", "I"),
        ("L", "Li"),
        ("n", "N"),
        ("w", "N"),
        ("o", "O"),
        ("q", "O"),
        ("p", "P"),
        ("s", "S"),
        ("S", "Si"),
    ];
    let groups = [
        ("A", "Ac"),
        ("e", "Et"),
        ("E", "CO2Me"),
        ("F", "CF3"),
        ("H", "Cbz"),
        ("m", "Me"),
        ("M", "MgBr"),
        ("Z", "N3"),
        ("N", "NO2"),
        ("O", "OMe"),
        ("P", "Ph"),
        ("Q", "Fmoc"),
        ("y", "Boc"),
    ];
    for source in ["C", "N"] {
        for (shortcut, expected, group) in elements
            .into_iter()
            .map(|(key, value)| (key, value, false))
            .chain(groups.into_iter().map(|(key, value)| (key, value, true)))
        {
            let (mut app, target, unrelated) = fixture(source);
            let point = if source == "N" {
                glyph_center(&app.tab.doc, "2")
            } else {
                app.tab.doc.atom(target).unwrap().position
            };
            let before = app.tab.doc.clone();
            let original_atom = before.atom(target).unwrap();
            hover(&mut app, point);
            key(&mut app, shortcut);
            assert!(!app.error, "{source}, {shortcut}: {}", app.status);
            app.tab.doc.validate().unwrap();
            assert_eq!(
                app.tab.doc.atom(target).unwrap().position,
                original_atom.position
            );
            assert_eq!(app.tab.doc.atom(target).unwrap().depth, original_atom.depth);
            if group {
                let abbreviation = app
                    .tab
                    .doc
                    .abbreviation(target)
                    .expect("Real chemical group");
                assert_eq!(abbreviation.label, expected);
                assert!(abbreviation.members.contains(&target));
            } else {
                assert_eq!(app.tab.doc.atom(target).unwrap().element, expected);
                assert_eq!(app.tab.doc.atoms.len(), before.atoms.len());
                assert_eq!(
                    app.tab.doc.bonds, before.bonds,
                    "Element labels retain existing bonds"
                );
            }
            assert_unrelated(&before, &app.tab.doc, &unrelated);
            if app.tab.doc == before {
                assert!(
                    !app.tab.history.can_undo(),
                    "Unchanged labels do not consume history"
                );
            } else {
                assert_one_undo(&mut app, &before);
            }
        }
        for shortcut in ["r", "x", "d", "+", "-"] {
            let (mut app, target, unrelated) = fixture(source);
            let point = app.tab.doc.atom(target).unwrap().position;
            let before = app.tab.doc.clone();
            hover(&mut app, point);
            key(&mut app, shortcut);
            assert!(!app.error, "{source}, {shortcut}: {}", app.status);
            let atom = app.tab.doc.atom(target).unwrap();
            match shortcut {
                "r" | "x" => {
                    assert_eq!(atom.element, "*");
                    assert_eq!(
                        atom.display.variable.as_deref(),
                        Some(if shortcut == "r" { "R" } else { "X" })
                    );
                }
                "d" => {
                    assert_eq!(atom.element, "H");
                    assert_eq!(atom.isotope, 2);
                }
                "+" => assert_eq!(atom.charge, 1),
                "-" => assert_eq!(atom.charge, -1),
                _ => unreachable!(),
            }
            assert_eq!(app.tab.doc.atoms.len(), before.atoms.len());
            assert_eq!(app.tab.doc.bonds, before.bonds);
            assert_unrelated(&before, &app.tab.doc, &unrelated);
            assert_one_undo(&mut app, &before);
        }
    }
}

#[test]
fn all_structural_atom_keys_are_atomic_at_skeletal_element_fixed_and_group_labels() {
    // These are transaction and scope checks, not a claim that every heteroatom
    // outcome matches ChemDraw. Individual chemistry oracles cover confirmed
    // behaviors; unsupported valence/group operations must fail atomically.
    for label in [
        "skeletal",
        "carbon label",
        "nitrogen",
        "fixed NH2",
        "abbreviation",
        "methyl abbreviation",
        "variable",
    ] {
        for shortcut in [
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "z", "K", "k", "a", "v", "u", "j",
            "J",
        ] {
            let (mut app, target, unrelated) =
                fixture(if matches!(label, "nitrogen" | "fixed NH2") {
                    "N"
                } else {
                    "C"
                });
            match label {
                "carbon label" => {
                    app.tab.doc.atom_mut(target).unwrap().display.carbons =
                        Some(reshiki::atom_labels::Carbons::All)
                }
                "fixed NH2" => {
                    app.tab.doc = reshiki::atom_text::apply(
                        &app.tab.doc,
                        target,
                        "NH2",
                        reshiki::atom_text::Mode::Auto,
                    )
                    .unwrap()
                }
                "abbreviation" => {
                    app.tab.doc = reshiki::atom_text::apply(
                        &app.tab.doc,
                        target,
                        "OMe",
                        reshiki::atom_text::Mode::Group,
                    )
                    .unwrap()
                }
                "methyl abbreviation" => {
                    app.tab.doc = reshiki::atom_text::apply(
                        &app.tab.doc,
                        target,
                        "Me",
                        reshiki::atom_text::Mode::Group,
                    )
                    .unwrap()
                }
                "variable" => {
                    app.tab.doc = reshiki::atom_text::apply(
                        &app.tab.doc,
                        target,
                        "R",
                        reshiki::atom_text::Mode::Text,
                    )
                    .unwrap()
                }
                _ => {}
            }
            let before = app.tab.doc.clone();
            let point = before.atom(target).unwrap().position;
            hover(&mut app, point);
            key(&mut app, shortcut);
            app.tab.doc.validate().unwrap();
            assert_unrelated(&before, &app.tab.doc, &unrelated);
            if app.error {
                assert_eq!(
                    app.tab.doc, before,
                    "{label}, {shortcut}: failure must be atomic"
                );
                assert!(!app.tab.history.can_undo());
            } else {
                assert_ne!(
                    app.tab.doc, before,
                    "{label}, {shortcut}: assigned growth must act or explain rejection"
                );
                if label == "methyl abbreviation" {
                    assert!(
                        app.tab.doc.abbreviation(target).is_none(),
                        "A changed methyl group must expand rather than retain its old label"
                    );
                }
                assert_one_undo(&mut app, &before);
            }
        }
    }
}

#[test]
fn select_and_properties_hotkeys_refresh_the_selected_atom_and_bond_fields() {
    for shortcut in ["g", "?", "/"] {
        let (mut app, a, b, unrelated) = bond_fixture();
        app.tab.doc.bonds[0].color = reshiki::palette::Color::Custom([160, 20, 40]);
        app.tab.doc.bonds[1].color = reshiki::palette::Color::Custom([10, 40, 180]);
        app.edit(Edit::Select(unrelated.clone()));
        assert_eq!(app.tab.bond_color_input, "#0A28B4");
        let before = app.tab.doc.clone();
        hover(&mut app, Point::new(21., 0.));
        key(&mut app, shortcut);
        assert_eq!(app.tab.selected, vec![a, b]);
        assert_eq!(
            app.tab.bond_color_input, "#A01428",
            "{shortcut}: target color replaces the previous selection's field"
        );
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());

        let (mut app, target, unrelated) = fixture("N");
        for (id, number, size) in [(target, "7", 16.), (unrelated[0], "99", 28.)] {
            let atom = app.tab.doc.atom_mut(id).unwrap();
            atom.display.number = Some(reshiki::atom_labels::Number {
                text: number.into(),
                offset: None,
                style: reshiki::atom_labels::number_style(),
            });
            atom.text_style = Some(reshiki::typography::TextStyle {
                size_pt: size,
                ..Default::default()
            });
        }
        app.edit(Edit::Select(vec![unrelated[0]]));
        assert_eq!(app.tab.labels.number, "99");
        let before = app.tab.doc.clone();
        hover(&mut app, before.atom(target).unwrap().position);
        key(&mut app, shortcut);
        assert_eq!(app.tab.selected, vec![target]);
        assert_eq!(app.tab.labels.number, "7", "{shortcut}: atom number field");
        assert_eq!(
            app.tab.font_size_input, "16",
            "{shortcut}: atom typography field"
        );
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

fn bond_fixture() -> (App, u64, u64, Vec<u64>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.bond_drawing.length = 42.;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let c = app.tab.doc.add_atom("C", Point::new(400., 200.));
    let d = app.tab.doc.add_atom("C", Point::new(442., 200.));
    app.tab.doc.add_bond(c, d, 1, "plain");
    app.tab.selected = vec![c, d];
    (app, a, b, vec![c, d])
}

#[test]
fn every_bond_style_and_ring_key_uses_the_hovered_bond_instead_of_selected_atoms() {
    for (shortcut, order, display) in [
        ("1", 1, "plain"),
        ("2", 2, "plain"),
        ("3", 3, "plain"),
        ("b", 1, "bold"),
        ("B", 2, "bold"),
        ("w", 1, "wedge"),
        ("h", 1, "hash"),
        ("W", 1, "hash"),
        ("d", 5, "dashed"),
        ("D", 7, "plain"),
        ("H", 1, "hashed"),
        ("y", 1, "wavy"),
    ] {
        let (mut app, a, b, unrelated) = bond_fixture();
        if shortcut == "1" {
            app.tab.doc.bonds[0].order = 2;
        }
        let before = app.tab.doc.clone();
        hover(&mut app, Point::new(21., 0.));
        key(&mut app, shortcut);
        assert!(!app.error, "{shortcut}: {}", app.status);
        assert_eq!(app.tab.doc.atoms, before.atoms);
        assert_eq!(app.tab.doc.bonds.len(), before.bonds.len());
        let bond = app
            .tab
            .doc
            .bonds
            .iter()
            .find(|bond| bond.a == a && bond.b == b)
            .unwrap();
        assert_eq!(
            (bond.order, bond.display.as_str()),
            (order, display),
            "{shortcut}"
        );
        if shortcut == "D" {
            assert_eq!(bond.secondary_display.as_deref(), Some("dashed"));
        }
        if shortcut == "B" {
            assert_eq!(bond.secondary_display.as_deref(), Some("plain"));
        }
        assert_unrelated(&before, &app.tab.doc, &unrelated);
        assert_one_undo(&mut app, &before);
    }
    for (shortcut, count) in [
        ("v", 3),
        ("4", 4),
        ("5", 5),
        ("6", 6),
        ("7", 7),
        ("8", 8),
        ("a", 6),
        ("z", 5),
        ("9", 6),
        ("0", 6),
    ] {
        let (mut app, a, b, unrelated) = bond_fixture();
        let before = app.tab.doc.clone();
        hover(&mut app, Point::new(21., 0.));
        key(&mut app, shortcut);
        assert!(!app.error, "{shortcut}: {}", app.status);
        assert_eq!(
            app.tab.doc.atoms.len(),
            before.atoms.len() + count - 2,
            "{shortcut}: fuse the existing edge"
        );
        assert_eq!(
            app.tab.doc.bonds.len(),
            before.bonds.len() + count - 1,
            "{shortcut}: close one ring"
        );
        assert_eq!(app.tab.doc.atom(a), before.atom(a));
        assert_eq!(app.tab.doc.atom(b), before.atom(b));
        assert_unrelated(&before, &app.tab.doc, &unrelated);
        assert_one_undo(&mut app, &before);
    }
    for (shortcut, position) in [
        ("l", DoublePosition::Left),
        ("c", DoublePosition::Center),
        ("r", DoublePosition::Right),
    ] {
        let (mut app, _, _, unrelated) = bond_fixture();
        app.tab.doc.bonds[0].order = 2;
        let before = app.tab.doc.clone();
        hover(&mut app, Point::new(21., 0.));
        key(&mut app, shortcut);
        assert_eq!(app.tab.doc.bonds[0].double_position, position);
        assert_unrelated(&before, &app.tab.doc, &unrelated);
        assert_one_undo(&mut app, &before);
    }
}

#[test]
fn empty_canvas_tool_aliases_do_not_relabel_or_change_drawing_history() {
    for (shortcut, tool) in [
        ("v", Tool::Select),
        ("l", Tool::Lasso),
        ("1", Tool::Bond(1)),
        ("x", Tool::Bond(1)),
        ("b", Tool::Bond(1)),
        ("2", Tool::Bond(2)),
        ("3", Tool::Bond(3)),
        ("4", Tool::StyledBond(reshiki::bonds::BondPreset::Quadruple)),
        ("X", Tool::Chain(reshiki::chains::ChainMode::Straight)),
        ("r", Tool::Ring),
        ("j", Tool::RingPreset(reshiki::rings::Preset::Benzene)),
        (
            "J",
            Tool::RingPreset(reshiki::rings::Preset::Cyclopentadiene),
        ),
        ("a", Tool::Arrow),
        ("e", Tool::Arrow),
        ("t", Tool::Text),
        ("T", Tool::Graphic(reshiki::graphics::GraphicKind::Brackets)),
        (
            "E",
            Tool::Graphic(reshiki::graphics::GraphicKind::Symbol(
                reshiki::scientific::SymbolKind::CirclePlus,
            )),
        ),
        (
            "G",
            Tool::Graphic(reshiki::graphics::GraphicKind::Orbital(
                reshiki::scientific::OrbitalKind::P,
            )),
        ),
    ] {
        let (mut app, _, _) = fixture("N");
        app.tab.selected.clear();
        hover(&mut app, Point::new(-400., -400.));
        let before = app.tab.doc.clone();
        key(&mut app, shortcut);
        assert_eq!(app.tool, tool, "{shortcut}");
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
    for (shortcut, element) in [
        ("c", "C"),
        ("C", "Cl"),
        ("f", "F"),
        ("h", "H"),
        ("i", "I"),
        ("L", "Li"),
        ("n", "N"),
        ("w", "N"),
        ("o", "O"),
        ("q", "O"),
        ("p", "P"),
        ("s", "S"),
        ("S", "Si"),
        ("B", "B"),
    ] {
        let (mut app, _, _) = fixture("N");
        app.tab.selected.clear();
        hover(&mut app, Point::new(-400., -400.));
        let before = app.tab.doc.clone();
        key(&mut app, shortcut);
        assert_eq!(app.tool, Tool::Atom);
        assert_eq!(app.element, element, "{shortcut}");
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn atom_label_editor_blocks_every_contextual_character_hotkey() {
    let (mut app, target, _) = fixture("N");
    let before = app.tab.doc.clone();
    let point = before.atom(target).unwrap().position;
    hover(&mut app, point);
    let _ = app.update(Message::AtomText(crate::app::atom_text::Action::Begin(
        Some(target),
    )));
    for shortcut in [
        "b", "B", "c", "C", "l", "f", "h", "i", "L", "n", "w", "o", "q", "p", "s", "S", "A", "e",
        "E", "F", "H", "m", "M", "Z", "N", "O", "P", "Q", "y", "r", "x", "d", "+", "-", "0", "1",
        "2", "3", "4", "5", "6", "7", "8", "9", "z", "K", "k", "a", "v", "u", "j", "J", "g", "?",
        "/", "=",
    ] {
        key(&mut app, shortcut);
        assert!(app.tab.atom_text.is_some());
        assert_eq!(
            app.tab.doc, before,
            "{shortcut}: a modal draft must capture drawing edits"
        );
        assert!(!app.tab.history.can_undo());
    }
}
