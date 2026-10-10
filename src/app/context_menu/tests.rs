use super::*;
use crate::canvas::Edit;
use reshiki::document::{Arrow, Point as World};

fn run_item(app: &mut App, page: Page, label: &str) -> Result<(), String> {
    let action = app
        .context_entries(page)
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Item {
                label: name,
                action,
                enabled: true,
            } if name == label => Some(action),
            _ => None,
        })
        .ok_or_else(|| format!("Missing enabled menu item: {label}"))?;
    let _ = app.context_action(action);
    Ok(())
}

fn labels(app: &App, page: Page) -> Vec<&'static str> {
    app.context_entries(page)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Item { label, .. } => Some(label),
            _ => None,
        })
        .collect()
}

#[test]
fn chemical_name_menu_uses_clicked_component_and_switches_between_show_and_hide()
-> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::document::Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))?;
    let first: Vec<_> = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    let source = app.tab.doc.clone();
    reshiki::editing::append(&mut app.tab.doc, &source, World::new(250., 0.));
    let all = app.tab.doc.all_ids();
    app.tab.doc.group_selection(&all)?;
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: all,
        hit: vec![first[0]],
    });
    assert!(app.context_entries(Page::Main).iter().any(|entry| matches!(entry,
        Entry::Item { label: "Show chemical name", enabled: true, action: Action::Run(message) }
            if matches!(message.as_ref(), Message::Naming(super::super::naming::Action::ShowMoleculeName(atoms)) if *atoms == first)
    )));
    reshiki::molecule_names::show(
        &mut app.tab.doc,
        &first,
        "ethan-1-ol".into(),
        "CCO".into(),
        Default::default(),
    )?;
    assert!(labels(&app, Page::Main).contains(&"Hide chemical name"));
    assert!(!labels(&app, Page::Main).contains(&"Show chemical name"));
    let shown = app.tab.doc.clone();
    run_item(&mut app, Page::Main, "Hide chemical name")?;
    assert!(app.tab.doc.annotations.is_empty());
    assert!(app.tab.doc.molecule_names.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, shown);
    Ok(())
}

#[test]
fn copy_as_menu_names_scope_and_explains_disabled_reactions() -> Result<(), String> {
    use reshiki::clipboard::CopyFormat;
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let original = app.tab.doc.clone();
    assert!(labels(&app, Page::Main).contains(&"Copy as"));
    assert!(
        app.context_entries(Page::CopyAs)
            .iter()
            .any(|entry| { matches!(entry, Entry::Hint("Copy as · whole drawing")) })
    );
    app.tab.selected = app.tab.doc.all_ids();
    assert!(
        app.context_entries(Page::CopyAs)
            .iter()
            .any(|entry| { matches!(entry, Entry::Hint("Copy as · selected objects")) })
    );
    assert!(app.context_entries(Page::CopyAs).iter().any(|entry| {
        matches!(entry, Entry::Hint(reason) if reason.contains("one complete defined reaction"))
    }));
    for format in [CopyFormat::Mol, CopyFormat::Smiles, CopyFormat::Rxn] {
        assert!(app.context_entries(Page::CopyAs).iter().any(|entry| {
            matches!(entry, Entry::Item { action: Action::Run(message), enabled, .. }
                    if matches!(message.as_ref(), Message::CopyAs(value) if *value == format)
                    && *enabled == (format != CopyFormat::Rxn))
        }));
    }
    app.context_menu = Some(State::new(Point::new(20., 20.), Page::Main));
    let _ = app.context_action(Action::Page(Page::CopyAs));
    assert_eq!(
        app.context_menu
            .as_ref()
            .unwrap()
            .children
            .last()
            .unwrap()
            .page,
        Page::CopyAs
    );
    let _ = app.context_action(Action::Page(Page::Main));
    assert_eq!(
        app.context_menu.as_ref().map(|state| state.page),
        Some(Page::Main)
    );
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[test]
fn copy_as_menu_and_command_share_inference_and_explicit_selection_guards() {
    use reshiki::clipboard::CopyFormat;
    let (mut app, _) = App::new();
    let reactant = app.tab.doc.add_atom("O", World::default());
    let product = app.tab.doc.add_atom("O", World::new(240., 0.));
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        World::new(90., 0.),
        World::new(150., 0.),
        Default::default(),
        Default::default(),
    ));
    app.tab.selected = app.tab.doc.all_ids();
    let enabled = |app: &App, format| {
        app.context_entries(Page::CopyAs).iter().any(|entry| {
            matches!(entry, Entry::Item { action: Action::Run(message), enabled: true, .. }
                if matches!(message.as_ref(), Message::CopyAs(value) if *value == format))
        })
    };
    assert!(enabled(&app, CopyFormat::ChemDoodleReaction));
    assert!(enabled(&app, CopyFormat::Rxn));
    assert!(!enabled(&app, CopyFormat::Smiles));
    let original = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    assert!(app.copy_as(CopyFormat::ChemDoodleReaction).units() > 0);
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.selected, selection);
    assert!(!app.tab.history.can_undo());
    app.copy_as_busy = false;
    app.tab.clipboard_busy = false;
    app.tab.doc.reactions = vec![
        reshiki::reactions::copy_reaction(&original, &original)
            .unwrap()
            .unwrap(),
    ];
    app.tab.selected = vec![reactant, arrow];
    let explicit = app.tab.doc.clone();
    assert!(!enabled(&app, CopyFormat::Smiles));
    assert!(!enabled(&app, CopyFormat::ChemDoodleReaction));
    assert!(enabled(&app, CopyFormat::Cdxml));
    for format in [CopyFormat::Smiles, CopyFormat::ChemDoodleReaction] {
        assert_eq!(app.copy_as(format).units(), 0);
        assert!(app.error);
        assert!(app.status.contains("complete defined reaction"));
    }
    assert_eq!(app.tab.doc, explicit);
    assert!(!app.tab.history.can_undo());
    app.tab.selected = vec![reactant, product];
    assert!(!enabled(&app, CopyFormat::Smiles));
    assert_eq!(app.copy_as(CopyFormat::Smiles).units(), 0);
    assert!(app.status.contains("participant"));
    app.tab.selected = vec![product];
    assert!(enabled(&app, CopyFormat::Smiles));
}

#[test]
fn row_menus_toggle_and_explain_unavailable_arrange_commands() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    let before = app.tab.doc.clone();
    let open = Message::ContextMenu(Action::Open(Page::AlignObjects, 300.));
    let _ = app.update(open.clone());
    assert_eq!(
        app.context_menu.as_ref().map(|m| m.page),
        Some(Page::AlignObjects)
    );
    let _ = app.update(open);
    assert!(
        app.context_menu.is_none(),
        "The same button closes its menu"
    );
    // One molecule is one object: alignment is unavailable and says why.
    assert_eq!(
        labels(&app, Page::Arrange),
        [
            "Align needs 2 objects",
            "Distribute needs 3 objects",
            "Bring to front",
            "Send to back",
            "Flip horizontal",
            "Flip vertical",
            "Rotate 180°"
        ]
    );
    assert!(run_item(&mut app, Page::Arrange, "Align needs 2 objects").is_err());
    assert_eq!(
        labels(&app, Page::More(2)),
        ["3D optimize…", "Move & attach…"],
        "Default keyboard drawing folds the selection commands first"
    );
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    assert_eq!(
        labels(&app, Page::More(2)),
        ["3D optimize…", "Keyboard drawing (F8)"],
        "⋯ lists the folded commands in row order"
    );
    let _ = app.update(Message::ContextMenu(Action::Open(Page::Arrange, 300.)));
    run_item(&mut app, Page::Arrange, "Flip horizontal")?;
    assert!(app.context_menu.is_none());
    assert_ne!(app.tab.doc, before);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    Ok(())
}

#[test]
fn context_commands_follow_the_target_without_modifying_the_drawing() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let ring = app.tab.doc.all_ids();
    let atom = *ring.first().ok_or("ring")?;
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        World::new(100., 100.),
        World::new(160., 100.),
        Default::default(),
        Default::default(),
    ));
    let before = app.tab.doc.clone();
    assert_eq!(
        labels(&app, Page::Main),
        [
            "Undo",
            "Redo",
            "Paste",
            "Copy as",
            "Select all",
            "Fit drawing"
        ]
    );
    app.tab.selected = vec![atom];
    let single = labels(&app, Page::Main);
    assert!(single.contains(&"Edit atom label…"));
    assert!(!single.contains(&"3D tilt"));
    assert!(!single.contains(&"Bond appearance"));
    assert!(single.contains(&"Select molecule"));
    app.tab.selected = ring.clone();
    let molecule = labels(&app, Page::Main);
    assert!(!molecule.contains(&"Select molecule"));
    for expected in [
        "3D tilt",
        "Arrange & transform",
        "Bond appearance",
        "Attachment points",
    ] {
        assert!(molecule.contains(&expected), "Missing {expected}");
    }
    assert!(!molecule.contains(&"Bond in front"));
    assert!(labels(&app, Page::Bonds).contains(&"Bond in front"));
    assert!(!labels(&app, Page::Align).contains(&"Align middles"));
    app.tab.selected.push(arrow);
    assert!(!labels(&app, Page::Main).contains(&"Attachment points"));
    assert!(labels(&app, Page::Align).contains(&"Align middles"));
    app.tab.selected = vec![arrow];
    let arrow_items = labels(&app, Page::Main);
    assert!(arrow_items.contains(&"Reverse arrow"));
    assert!(!arrow_items.contains(&"3D tilt"));
    assert!(!arrow_items.contains(&"Bond appearance"));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[test]
fn context_tilt_routes_all_axes_and_tool_without_losing_selection() -> Result<(), String> {
    for (label, around_x, degrees) in [
        ("X −15°", true, -15.),
        ("X +15°", true, 15.),
        ("Y −15°", false, -15.),
        ("Y +15°", false, 15.),
    ] {
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        let ids = app.tab.doc.all_ids();
        let source = app.tab.doc.clone();
        reshiki::editing::append(&mut app.tab.doc, &source, World::new(240., 0.));
        let before = app.tab.doc.clone();
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            hit: vec![],
            selected: ids.clone(),
        });
        run_item(&mut app, Page::Main, "3D tilt")?;
        assert!(matches!(
            app.context_menu
                .as_ref()
                .and_then(|s| s.children.last())
                .map(|child| child.page),
            Some(Page::Tilt)
        ));
        let _ = app.context_action(Action::Page(Page::Main));
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        run_item(&mut app, Page::Main, "3D tilt")?;
        run_item(&mut app, Page::Tilt, label)?;
        let mut expected = before.clone();
        reshiki::projection::tilt(&mut expected, &ids, degrees, around_x);
        assert_eq!(app.tab.doc, expected);
        assert_eq!(app.tab.selected, ids);
        assert!(app.context_menu.is_none());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, expected);
        app.edit(Edit::ContextMenu {
            position: Point::new(20., 20.),
            hit: vec![],
            selected: ids.clone(),
        });
        run_item(&mut app, Page::Tilt, "Drag to tilt")?;
        assert_eq!(app.tool, Tool::Tilt);
        assert_eq!(app.tab.selected, ids);
        assert_eq!(app.tab.doc, expected);
        assert!(app.context_menu.is_none());
    }
    Ok(())
}

#[test]
fn tilt_drag_is_one_undo_step_and_matches_projection_preview() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., true);
    let ids = app.tab.doc.all_ids();
    let source = app.tab.doc.clone();
    reshiki::editing::append(&mut app.tab.doc, &source, World::new(240., 0.));
    let before = app.tab.doc.clone();
    app.tab.selected = ids.clone();
    let _ = app.update(Message::Tool(Tool::Tilt));
    assert_eq!(app.tab.selected, ids);
    assert!(!app.tab.history.can_undo());
    let mut preview = before.clone();
    crate::canvas::tilt::apply(&mut preview, &ids, 30., -15.);
    app.edit(Edit::Tilt {
        ids: ids.clone(),
        x: 30.,
        y: -15.,
    });
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.doc.bonds, before.bonds);
    assert_eq!(app.tool, Tool::Tilt);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
}

#[test]
fn context_alignment_preserves_molecular_geometry_and_undo_restores_every_group()
-> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let source = app.tab.doc.clone();
    reshiki::editing::append(&mut app.tab.doc, &source, World::new(240., 80.));
    let arrow = Arrow::new(
        app.tab.doc.next_id(),
        World::new(90., -60.),
        World::new(160., -60.),
        Default::default(),
        Default::default(),
    );
    app.tab.doc.arrows.push(arrow);
    let before = app.tab.doc.clone();
    let ids = app.tab.doc.all_ids();
    app.edit(Edit::ContextMenu {
        position: Point::new(20., 20.),
        hit: vec![],
        selected: ids.clone(),
    });
    assert_eq!(app.alignment_count(), 3);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::InspectorScroll(0.));
    assert!(
        app.context_menu.is_some(),
        "Background inspector updates must leave the menu open"
    );
    let _ = app.context_action(Action::Run(Box::new(Message::Arrange(
        Arrange::AlignVertical,
    ))));
    assert!(app.context_menu.is_none());
    let centers: Vec<_> = reshiki::editing::groups(&app.tab.doc, &ids)
        .iter()
        .map(|ids| {
            let (lo, hi) =
                reshiki::scene::selection_bounds(&app.tab.doc, ids).ok_or("selection bounds")?;
            Ok::<_, String>((lo.y + hi.y) / 2.)
        })
        .collect::<Result<_, _>>()?;
    let center = centers.first().ok_or("alignment center")?;
    assert!(centers.iter().all(|y| (y - center).abs() < 0.001));
    for bond in &app.tab.doc.bonds {
        let length = |doc: &reshiki::document::Document| -> Result<f32, String> {
            Ok(doc
                .atom(bond.a)
                .ok_or("bond start")?
                .position
                .distance(doc.atom(bond.b).ok_or("bond end")?.position))
        };
        assert!((length(&app.tab.doc)? - length(&before)?).abs() < 0.001);
    }
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    app.edit(Edit::ContextMenu {
        position: Point::new(20., 20.),
        hit: vec![],
        selected: ids,
    });
    let _ = app.update(Message::Escape);
    assert!(app.context_menu.is_none());
    assert_eq!(app.tab.doc, before);
    Ok(())
}

#[tokio::test]
async fn inserted_examples_keep_existing_objects_and_can_be_removed_in_one_undo()
-> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let before = app.tab.doc.clone();
    let result = app
        .engine
        .request(reshiki::engine::Request::import_smiles("CCO"))
        .await?;
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: super::super::Job::Insert,
        result: Box::new(Ok(result)),
    });
    assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 3);
    assert_eq!(app.tab.selected.len(), 3);
    for atom in &before.atoms {
        assert_eq!(app.tab.doc.atom(atom.id), Some(atom));
    }
    let (_, old_max) = before.bounds();
    assert!(app.tab.selected.iter().all(|id| {
        app.tab
            .doc
            .atom(*id)
            .is_some_and(|a| a.position.x > old_max.x)
    }));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    Ok(())
}
