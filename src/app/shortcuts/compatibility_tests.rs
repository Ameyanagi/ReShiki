use super::*;
use crate::canvas::Edit;
use reshiki::bonds::BondPreset;

fn key(c: &str) -> Key {
    Key::Character(c.into())
}
fn primary() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    }
}

#[test]
fn primary_shift_d_starts_a_detached_3d_preview_instead_of_duplicating() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/geometry/adamantane.rsk"
    ))
    .unwrap();
    app.tab.selected = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    let original = app.tab.doc.clone();
    let revision = app.tab.revision;
    let modifiers = primary() | Modifiers::SHIFT;
    let message = key_message(&key("d"), &key("D"), modifiers).unwrap();
    assert!(matches!(
        message,
        Message::Optimization(super::super::optimization::Action::Begin)
    ));
    let _ = app.update(message.clone());
    assert!(app.tab.optimization.is_some());
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    let serial = app.tab.optimization_serial;
    let _ = app.update(message);
    assert_eq!(app.tab.optimization_serial, serial);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Optimization(
        super::super::optimization::Action::Cancel,
    ));
    assert!(app.tab.optimization.is_none());
    assert_eq!(app.tab.doc, original);
    assert!(matches!(
        key_message(&key("d"), &key("d"), primary()),
        Some(Message::Shortcut(Action::CopyText("cdxml")))
    ));
}

#[test]
fn displayed_shortcuts_are_the_keys_that_run_their_commands() {
    let mut messages = vec![
        Message::Undo,
        Message::Redo,
        Message::Copy(false),
        Message::Copy(true),
        Message::Paste,
        Message::CopyImage,
        Message::Optimization(super::super::optimization::Action::Begin),
        Message::SelectAll,
        Message::InvertSelection,
        Message::Group,
        Message::Ungroup,
        Message::Shortcut(Action::Join),
        Message::Fit,
        Message::BondDepth(true),
        Message::BondDepth(false),
        Message::Transform(Transform::FlipHorizontal),
        Message::Transform(Transform::FlipVertical),
        Message::Delete,
        Message::AtomText(crate::app::atom_text::Action::Begin(None)),
        Message::ToggleSelectedRing,
    ];
    messages.extend(
        [
            Arrange::AlignLeft,
            Arrange::AlignRight,
            Arrange::AlignTop,
            Arrange::AlignBottom,
            Arrange::AlignHorizontal,
            Arrange::AlignVertical,
            Arrange::DistributeHorizontal,
            Arrange::DistributeVertical,
        ]
        .map(Message::Arrange),
    );
    for message in messages {
        let (mods, name) = binding(&message).expect("a shortcut");
        let (pressed, expected) = match name {
            "Delete" => (Key::Named(Named::Delete), message.clone()),
            "Enter" => (Key::Named(Named::Enter), Message::ContextKey(name.into())),
            // Unmodified letters act on the selection under the pointer.
            _ if !mods.command() => (key(name), Message::ContextKey(name.into())),
            _ => (key(&name.to_lowercase()), message.clone()),
        };
        let actual = key_message(&pressed, &pressed, mods);
        assert_eq!(format!("{actual:?}"), format!("{:?}", Some(expected)));
    }
    // File commands are routed before text fields see them.
    for message in [
        Message::New,
        Message::Open,
        Message::Save,
        Message::SaveAs,
        Message::Printing(crate::app::printing::Action::Start(
            reshiki::printing::Scope::Document,
        )),
    ] {
        let (mods, name) = binding(&message).expect("a shortcut");
        let actual = crate::app::file_shortcuts::file_message(&key(&name.to_lowercase()), mods);
        assert_eq!(format!("{actual:?}"), format!("{:?}", Some(message)));
    }
    assert_eq!(label(&Message::Transform(Transform::Rotate(180.))), None);
    assert_eq!(label(&Message::Duplicate), None);
    let label = |message| label(&message).unwrap_or_default();
    let edit_label = || Message::AtomText(crate::app::atom_text::Action::Begin(None));
    let (command, alt) = (Modifiers::COMMAND, Modifiers::ALT);
    if cfg!(target_os = "macos") {
        assert_eq!(label(Message::Arrange(Arrange::AlignLeft)), "⌥⇧⌘L");
        assert_eq!(label(Message::Copy(true)), "⌘X");
        assert_eq!(label(Message::Delete), "⌫");
        assert_eq!(label(Message::ToggleSelectedRing), "⇧R");
        assert_eq!(label(edit_label()), "↩");
        assert_eq!(label(Message::SaveAs), "⇧⌘S");
        assert_eq!(keys(command | alt, "Left"), "⌥⌘←");
        assert_eq!(keys(command, "Enter"), "⌘↩");
        assert_eq!(keys(alt | Modifiers::SHIFT, ""), "⌥⇧");
    } else {
        assert_eq!(label(Message::ToggleSelectedRing), "Shift+R");
        assert_eq!(label(edit_label()), "Enter");
        assert_eq!(
            label(Message::Arrange(Arrange::AlignLeft)),
            "Ctrl+Alt+Shift+L"
        );
        assert_eq!(label(Message::Copy(true)), "Ctrl+X");
        assert_eq!(label(Message::Delete), "Delete");
        assert_eq!(label(Message::SaveAs), "Ctrl+Shift+S");
        assert_eq!(keys(command | alt, "Left"), "Ctrl+Alt+Left");
        assert_eq!(keys(command, "Enter"), "Ctrl+Enter");
        assert_eq!(keys(alt | Modifiers::SHIFT, ""), "Alt+Shift");
    }
}

#[test]
fn only_key_symbols_change_font() {
    let runs = |keys| {
        spans(keys)
            .into_iter()
            .map(|run| (run.text.into_owned(), run.font.is_some()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        runs("⌥⇧⌘L"),
        [("⌥⇧⌘".to_owned(), true), ("L".to_owned(), false)]
    );
    assert_eq!(
        runs("⌘ Z / ⇧ ⌘ Z"),
        [
            ("⌘".to_owned(), true),
            (" Z / ".to_owned(), false),
            ("⇧".to_owned(), true),
            (" ".to_owned(), false),
            ("⌘".to_owned(), true),
            (" Z".to_owned(), false),
        ]
    );
    assert_eq!(runs("Ctrl+Shift+D"), [("Ctrl+Shift+D".to_owned(), false)]);
    // Tooltips such as the toolbar's Redo use the same runs.
    assert_eq!(
        runs("Redo · ⇧⌘Z"),
        [
            ("Redo · ".to_owned(), false),
            ("⇧⌘".to_owned(), true),
            ("Z".to_owned(), false),
        ]
    );
    assert!("Redo · ⇧⌘Z".contains(symbol) && !"Redo · Ctrl+Shift+Z".contains(symbol));
    assert!(runs("").is_empty());
}

#[test]
fn dispatcher_preserves_shift_and_does_not_fall_through_modifier_chords() {
    assert!(
        matches!(key_message(&key("c"), &key("C"), Modifiers::SHIFT), Some(Message::ContextKey(k)) if k == "C")
    );
    assert!(
        matches!(key_message(&key("n"), &key("N"), Modifiers::empty()), Some(Message::ContextKey(k)) if k == "N")
    );
    assert!(matches!(
        key_message(&key("d"), &key("d"), primary()),
        Some(Message::Shortcut(Action::CopyText("cdxml")))
    ));
    assert!(matches!(
        key_message(&key("e"), &key("e"), primary()),
        Some(Message::Shortcut(Action::FixedAngles))
    ));
    assert!(matches!(
        key_message(&key("k"), &key("K"), primary() | Modifiers::SHIFT),
        Some(Message::Cleanup(crate::app::cleanup::Action::Begin))
    ));
    assert!(matches!(
        key_message(&key("j"), &key("j"), primary()),
        Some(Message::Shortcut(Action::Join))
    ));
    assert!(key_message(&key("x"), &key("X"), primary() | Modifiers::SHIFT).is_none());
    assert!(key_message(&key("z"), &key("z"), primary() | Modifiers::ALT).is_none());
    assert!(matches!(
        key_message(
            &Key::Named(Named::ArrowLeft),
            &Key::Named(Named::ArrowLeft),
            Modifiers::ALT | Modifiers::SHIFT
        ),
        Some(Message::Transform(Transform::TiltY(_)))
    ));
}

#[test]
fn numeric_hotkeys_distinguish_hovered_bond_atom_and_blank_canvas() -> Result<(), String> {
    let (mut app, _) = App::new();
    // This compatibility contract uses classic hover/selection routing:
    // Hover(None) leaves no target, unlike the independent hybrid hotspot.
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    assert!(!app.tab.keyboard_drawing.enabled());
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let initial = app.tab.doc.clone();
    app.tab.selected = vec![a]; // The actual hovered bond must win over this selection.
    app.edit(Edit::Hover(Some(Point::new(21., 0.))));
    let _ = app.context_key("2");
    assert_eq!(app.tab.doc.atoms.len(), 2);
    assert_eq!(app.tab.doc.bonds.first().ok_or("Missing bond")?.order, 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
    app.edit(Edit::Hover(Some(Point::new(42., 0.))));
    let _ = app.context_key("2");
    assert_eq!(app.tab.doc.atoms.len(), 4);
    assert!(app.tab.doc.atoms.iter().any(|a| a.element == "O"));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
    app.tab.selected.clear();
    app.edit(Edit::Hover(None));
    let _ = app.context_key("2");
    assert_eq!(app.tool, Tool::Bond(2));
    assert_eq!(app.tab.doc, initial);
    Ok(())
}

#[test]
fn every_bond_hotkey_is_undoable_and_preserves_endpoints() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let original = app.tab.doc.clone();
    for key in ["2", "3", "b", "B", "w", "h", "W", "H", "y", "d", "D"] {
        app.tab.selected = vec![a, b];
        let _ = app.context_key(key);
        app.tab.doc.validate()?;
        let bond = app.tab.doc.bonds.first().ok_or("Missing bond")?;
        assert_eq!((bond.a, bond.b), (a, b));
        assert_eq!(BondPreset::of(bond), hotkeys::bond_preset(key));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original, "{key}");
    }
    Ok(())
}

#[test]
fn triple_shortcut_geometry_and_order_undo_and_redo_as_one_edit() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(36.373, -21.));
    let c = app.tab.doc.add_atom("C", Point::new(72.746, 0.));
    let d = app.tab.doc.add_atom("C", Point::new(109.119, -21.));
    for (a, b) in [(a, b), (b, c), (c, d)] {
        app.tab.doc.add_bond(a, b, 1, "plain");
    }
    app.tab.selected = vec![b, c];
    app.tab.hover = None;
    let original = app.tab.doc.clone();
    let _ = app.context_key("3");
    assert!(!app.error, "{}", app.status);
    let changed = app.tab.doc.clone();
    assert_ne!(
        changed.atom(a).ok_or("Missing atom")?.position,
        original.atom(a).ok_or("Missing atom")?.position
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, changed);
    Ok(())
}

#[test]
fn group_shortcuts_are_atomic_and_modal_editors_block_them() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("N", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let original = app.tab.doc.clone();
    app.tab.selected = vec![b];
    let _ = app.update(Message::ContextKey("y".into()));
    assert_eq!(
        app.tab.doc.abbreviation(b).ok_or("Missing Boc")?.label,
        "Boc"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::AtomText(super::super::atom_text::Action::Begin(
        Some(b),
    )));
    let _ = app.update(Message::ContextKey("2".into()));
    let _ = app.update(Message::Shortcut(Action::Join));
    assert_eq!(app.tab.doc, original);
    assert!(app.tab.atom_text.is_some());
    Ok(())
}

#[test]
fn new_group_and_pi_ligand_keys_undo_and_respect_text_editors() -> Result<(), String> {
    for key in ["M", "Z", "j", "J"] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let target = app.tab.doc.add_atom(
            if matches!(key, "j" | "J") { "Fe" } else { "C" },
            Point::default(),
        );
        let original = app.tab.doc.clone();
        app.tab.selected = vec![target];
        let _ = app.update(Message::ContextKey(key.into()));
        assert!(!app.error, "{key}: {}", app.status);
        assert_ne!(app.tab.doc, original);
        app.tab.doc.validate()?;
        let changed = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, changed);
        let _ = app.update(Message::AtomText(super::super::atom_text::Action::Begin(
            Some(target),
        )));
        let _ = app.update(Message::ContextKey(key.into()));
        assert_eq!(app.tab.doc, changed);
    }
    Ok(())
}

#[test]
fn join_merges_sites_instead_of_adding_an_extra_bond_and_undo_restores_all() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let c = app.tab.doc.add_atom("C", Point::new(150., 0.));
    let d = app.tab.doc.add_atom("O", Point::new(192., 0.));
    app.tab.doc.add_bond(c, d, 1, "plain");
    app.tab.selected = vec![b, c];
    let original = app.tab.doc.clone();
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    app.tab.doc.validate()?;
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    // A failed attempt cannot lose either fragment.
    app.tab.selected = vec![a, d];
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    Ok(())
}

#[test]
fn join_three_atoms_retains_the_first_at_label_bounds_center_in_one_history_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/join-three-atoms-before.rsk"
    ))
    .unwrap();
    app.tab.selected = vec![1, 3, 5];
    let before = app.tab.doc.clone();
    let expected = app.merge_selected_atoms().unwrap().0;
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(!app.error, "{}", app.status);
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (4, 3));
    assert_eq!(app.tab.selected, [1]);
    let survivor = app.tab.doc.atom(1).unwrap();
    assert_eq!(survivor.position, expected.atom(1).unwrap().position);
    assert_ne!(survivor.position, Point::default());
    assert_eq!(survivor.element, "N");
    assert_eq!(survivor.text_style, before.atom(1).unwrap().text_style);
    assert_eq!(app.tab.history.frames(), 1);
    let joined = app.tab.doc.clone();
    let reopened = Document::from_json(&serde_json::to_vec(&joined).unwrap()).unwrap();
    assert_eq!(reopened.atoms, joined.atoms);
    assert_eq!(reopened.bonds, joined.bonds);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, joined);
}

#[test]
fn join_keeps_two_bond_fusion_and_explicit_merge_handles_the_four_atom_ambiguity() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("C", Point::new(150., 0.));
    let d = app.tab.doc.add_atom("C", Point::new(192., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(c, d, 1, "plain");
    app.tab.selected = vec![d, c, b, a];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(!app.error, "{}", app.status);
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (2, 1));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    app.tab.selected = vec![d, c, b, a];
    let expected = app.merge_selected_atoms().unwrap().0;
    let _ = app.update(Message::Shortcut(Action::MergeAtoms));
    assert!(!app.error, "{}", app.status);
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (1, 0));
    assert_eq!(app.tab.selected, [d]);
    assert_eq!(
        app.tab.doc.atom(d).unwrap().position,
        expected.atom(d).unwrap().position
    );
}

#[test]
fn failed_atom_merge_keeps_selection_document_and_history() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(12., 0.));
    let c = app.tab.doc.add_atom("C", Point::new(6., 12.));
    app.tab.doc.atom_mut(b).unwrap().map_num = 7;
    app.tab.selected = vec![c, b, a];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(app.error);
    assert!(app.status.contains("atom maps"));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, [c, b, a]);
    assert_eq!(app.tab.history.frames(), 0);
}

#[test]
fn join_four_atoms_with_adjacent_bonds_merges_instead_of_attempting_fusion() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(20., 0.));
    let c = app.tab.doc.add_atom("C", Point::new(20., 20.));
    let d = app.tab.doc.add_atom("N", Point::new(80., 80.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    app.tab.selected = vec![d, a, b, c];
    let before = app.tab.doc.clone();
    let expected = app
        .merge_selected_atoms()
        .unwrap()
        .0
        .atom(d)
        .unwrap()
        .position;
    let _ = app.update(Message::Shortcut(Action::Join));
    assert!(!app.error, "{}", app.status);
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (1, 0));
    assert_eq!(app.tab.selected, [d]);
    assert_eq!(app.tab.doc.atom(d).unwrap().element, "N");
    assert_eq!(app.tab.doc.atom(d).unwrap().position, expected);
    assert_eq!(app.tab.history.frames(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}
