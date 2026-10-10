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
        ("Co", 0, 0, 0, "N, O, S or P donor"),
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

#[test]
fn join_keeps_a_different_existing_metal_bond_and_rejects_a_whole_ligand_selection() {
    let mut app = drawing();
    app.tab.doc.add_bond(5, 1, 1, "plain");
    let original = app.tab.doc.clone();
    join(&mut app, [1, 5]);
    assert!(app.error);
    assert!(app.status.contains("already have a different bond"));
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());

    let mut app = drawing();
    let original = app.tab.doc.clone();
    app.tab.selected = vec![1, 2, 3, 4, 5];
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
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
