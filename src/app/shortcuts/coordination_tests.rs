use super::*;

fn drawing() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/coordination/co-en3-before.rsk"
    ))
    .unwrap();
    app.tab.saved = app.tab.doc.clone();
    app
}

fn join(app: &mut App, selected: [u64; 2]) {
    app.tab.selected = selected.into();
    let key = Key::Character("j".into());
    let message = key_message(&key, &key, Modifiers::COMMAND).expect("primary J shortcut");
    assert!(matches!(message, Message::Shortcut(Action::Join)));
    let _ = app.update(message);
}

#[test]
fn join_uses_command_j_on_macos_and_control_j_elsewhere() {
    let key = Key::Character("j".into());
    let (command, other) = if cfg!(target_os = "macos") {
        (Modifiers::LOGO, Modifiers::CTRL)
    } else {
        (Modifiers::CTRL, Modifiers::LOGO)
    };
    assert!(matches!(
        key_message(&key, &key, command),
        Some(Message::Shortcut(Action::Join))
    ));
    assert!(key_message(&key, &key, other).is_none());
}

#[test]
fn join_coordinates_all_six_en_donors_in_either_selection_order_without_moving_atoms() {
    let mut app = drawing();
    // Seed CH2 labels as on a refreshed native load.
    for atom in &mut app.tab.doc.atoms {
        if atom.element == "C" {
            atom.label_h = 2;
        }
    }
    let original = app.tab.doc.clone();
    let revision = app.tab.revision;
    let mut snapshots = vec![original.clone()];
    // Each second donor closes a chelate already connected to this same metal.
    for (index, donor) in [2, 5, 6, 9, 10, 13].into_iter().enumerate() {
        join(
            &mut app,
            if index % 2 == 0 {
                [donor, 1]
            } else {
                [1, donor]
            },
        );
        assert!(!app.error, "donor {donor}: {}", app.status);
        assert_eq!(app.tab.selected, [donor, 1]);
        assert_eq!(
            app.tab.doc.atoms, original.atoms,
            "all atom metadata and XYZ"
        );
        assert_eq!(&app.tab.doc.bonds[..9], original.bonds.as_slice());
        assert_eq!(app.tab.doc.bonds.len(), 10 + index);
        let contact = app.tab.doc.bonds.last().unwrap();
        assert_eq!((contact.a, contact.b, contact.order), (donor, 1, 5));
        assert_eq!(contact.display, "plain");
        assert!(!contact.projection);
        assert!(contact.stereo.is_none());
        app.tab.doc.validate().unwrap();
        assert_eq!(app.tab.revision, revision + index as u64 + 1);
        snapshots.push(app.tab.doc.clone());
    }
    assert_eq!(app.tab.doc.atoms.len(), 13);
    assert_eq!(app.tab.doc.atom(1).unwrap().charge, 3);
    for donor in [2, 5, 6, 9, 10, 13] {
        assert_eq!(app.tab.doc.atom(donor).unwrap().label_h, 2);
    }

    let revision = app.tab.revision;
    join(&mut app, [1, 13]);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, snapshots[6]);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.history.undo_frames().len(), 6);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, snapshots[5], "duplicate adds no Undo frame");
    join(&mut app, [1, 2]);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, snapshots[5]);
    assert!(app.tab.history.can_redo(), "duplicate preserves Redo");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, snapshots[6]);

    for snapshot in snapshots[..6].iter().rev() {
        let _ = app.update(Message::Undo);
        assert_eq!(&app.tab.doc, snapshot, "each contact is one Undo step");
    }
    assert!(!app.tab.history.can_undo());
    for snapshot in &snapshots[1..] {
        let _ = app.update(Message::Redo);
        assert_eq!(&app.tab.doc, snapshot);
    }
    assert!(!app.tab.history.can_redo());
}

#[test]
fn join_rejects_unsupported_metal_partners_without_sharing_or_recording_history() {
    for (element, charge, radical_electrons, explicit_h, diagnostic) in [
        ("C", 0, 0, 0, "N, O, S or P donor"),
        ("N", 1, 0, 0, "N, O, S or P donor"),
        ("N", 0, 1, 0, "radicals are unsupported"),
        ("N", 0, 0, 4, "no supported lone pair"),
        // Two metals share an atom instead; these charges differ.
        ("Co", 0, 0, 0, "same element and charge"),
    ] {
        for selected in [[5, 1], [1, 5]] {
            let mut app = drawing();
            let donor = app.tab.doc.atom_mut(5).unwrap();
            donor.element = element.into();
            donor.charge = charge;
            donor.radical_electrons = radical_electrons;
            donor.explicit_h = explicit_h;
            let original = app.tab.doc.clone();
            let revision = app.tab.revision;
            join(&mut app, selected);
            assert!(app.error, "{element}: {}", app.status);
            assert!(app.status.contains(diagnostic), "{}", app.status);
            assert_eq!(app.tab.doc, original);
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());
            assert!(!app.tab.history.can_redo());
        }
    }
}

/// The fixture as a native load leaves it, with refreshed CH2 labels.
fn refreshed() -> App {
    let mut app = drawing();
    for atom in &mut app.tab.doc.atoms {
        if atom.element == "C" {
            atom.label_h = 2;
        }
    }
    app
}

fn join_selection(app: &mut App, selected: &[u64]) {
    app.tab.selected = selected.to_vec();
    let _ = app.update(Message::Shortcut(Action::Join));
}

#[test]
fn join_keeps_a_different_existing_metal_bond_and_coordinates_the_rest_of_a_ligand() {
    let mut app = drawing();
    app.tab.doc.add_bond(5, 1, 1, "plain");
    let original = app.tab.doc.clone();
    join(&mut app, [1, 5]);
    assert!(app.error);
    assert!(app.status.contains("already have a different bond"));
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());

    // The whole ligand: N #2 gains its contact, N #5 keeps its covalent bond.
    join_selection(&mut app, &[1, 2, 3, 4, 5]);
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.status,
        "Coordinated N #2 → Co #1 · skipped N #5 (these atoms already have a different bond)"
    );
    for (atom, before) in app.tab.doc.atoms.iter().zip(&original.atoms) {
        assert_eq!((atom.id, atom.position), (before.id, before.position));
    }
    assert_eq!(app.tab.doc.bonds.len(), original.bonds.len() + 1);
    let contact = app.tab.doc.bonds.last().unwrap();
    assert_eq!((contact.a, contact.b, contact.order), (2, 1, 5));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn join_coordinates_a_whole_ligand_in_place() {
    let mut app = refreshed();
    let original = app.tab.doc.clone();
    join_selection(&mut app, &[1, 2, 3, 4, 5]);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Coordinated N #2, N #5 → Co #1");
    assert_eq!(app.tab.selected, [2, 5, 1]);
    assert_eq!(app.tab.doc.atoms, original.atoms, "nothing moves");
    assert_eq!(app.tab.doc.bonds.len(), 11);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

/// The #246 acceptance case: all 13 atoms of Co(en)3, one ⌘J.
#[test]
fn join_coordinates_all_thirteen_atoms_in_one_undo_step() {
    let mut app = refreshed();
    let original = app.tab.doc.clone();
    let all: Vec<u64> = (1..=13).collect();
    join_selection(&mut app, &all);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Coordinated 6 donors → Co #1");
    let six = app.tab.doc.clone();
    assert_eq!(six.atoms, original.atoms, "ids, XYZ, NH2 and Co 3+");
    assert_eq!(&six.bonds[..9], original.bonds.as_slice(), "three N–C–C–N");
    let contacts: Vec<_> = six.bonds[9..].iter().map(|b| (b.a, b.b, b.order)).collect();
    assert_eq!(contacts, [2, 5, 6, 9, 10, 13].map(|donor| (donor, 1, 5)));
    six.validate().unwrap();

    // Nothing left to add: no edit and no Undo frame.
    join_selection(&mut app, &all);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "All selected contacts already exist");
    assert_eq!(app.tab.history.undo_frames().len(), 1);

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, six);
}

#[test]
fn join_skips_a_far_donor_and_refuses_a_metal_without_donors() {
    let mut app = drawing();
    let far = app.tab.doc.add_atom("N", Point::new(400., 0.));
    let original = app.tab.doc.clone();
    join_selection(&mut app, &[1, 3, 4]);
    assert!(app.error);
    assert_eq!(
        app.status,
        "Select a donor (N, O, S or P) and a metal to coordinate; Join does not merge metals"
    );
    join_selection(&mut app, &[1, 3, far]);
    assert!(app.error);
    assert_eq!(
        app.status,
        format!("No contact added: N #{far} (too far from Co #1)")
    );
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());

    join_selection(&mut app, &[1, 2, 3, 4, 5, far]);
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.status,
        format!("Coordinated N #2, N #5 → Co #1 · skipped N #{far} (too far from Co #1)")
    );
}

/// An ignored atom, here an unrelated generic site, does not block the contacts.
#[test]
fn join_coordinates_past_an_ignored_generic_atom() {
    let mut app = refreshed();
    let site = app.tab.doc.add_atom("C", Point::new(300., 0.));
    app.tab.doc =
        reshiki::atom_text::apply(&app.tab.doc, site, "[C,N]", reshiki::atom_text::Mode::Auto)
            .unwrap();
    join_selection(&mut app, &[1, 2, 3, 4, 5, site]);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Coordinated N #2, N #5 → Co #1");
}

#[test]
fn join_shares_two_matching_metals_and_names_mismatched_elements() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("Co", Point::default());
    let b = app.tab.doc.add_atom("Co", Point::new(80., 0.));
    join(&mut app, [a, b]);
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "Joined selected atoms or bonds");
    assert_eq!(app.tab.doc.atoms.len(), 1);

    // Sharing must not drop either metal's isotope.
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("Co", Point::default());
    let b = app.tab.doc.add_atom("Co", Point::new(80., 0.));
    app.tab.doc.atom_mut(a).unwrap().isotope = 60;
    let original = app.tab.doc.clone();
    join(&mut app, [a, b]);
    assert!(app.error);
    assert!(
        app.status
            .starts_with("Join cannot share atoms with an isotope")
    );
    assert_eq!(app.tab.doc, original);

    for (first, second) in [("Co", "Ni"), ("N", "O")] {
        app.tab.doc = Document::default();
        let a = app.tab.doc.add_atom(first, Point::default());
        let b = app.tab.doc.add_atom(second, Point::new(80., 0.));
        let original = app.tab.doc.clone();
        join(&mut app, [a, b]);
        assert!(app.error);
        assert_eq!(
            app.status,
            format!(
                "Join shares only atoms of the same element and charge; {first} and {second} differ"
            )
        );
        assert_eq!(app.tab.doc, original);
    }
}

#[test]
fn join_still_fuses_two_ordinary_bonds_and_undo_restores_both_fragments() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("C", Point::new(180., 0.));
    let d = app.tab.doc.add_atom("C", Point::new(222., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(c, d, 1, "plain");
    let original = app.tab.doc.clone();
    app.tab.selected = vec![a, b, c, d];
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.tab.doc.atoms.iter().map(|a| a.id).collect::<Vec<_>>(),
        [c, d]
    );
    assert_eq!(app.tab.doc.bonds.len(), 1);
    assert_eq!(app.tab.doc.bonds[0].order, 1);
    app.tab.doc.validate().unwrap();
    let fused = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, fused);
}
