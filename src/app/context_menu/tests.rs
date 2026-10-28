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

/// Labels and separators ("—") of a menu page, in order.
fn outline(app: &App, page: Page) -> Vec<&'static str> {
    app.context_entries(page)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Item { label, .. } => Some(label),
            Entry::Separator => Some("—"),
            Entry::Hint(_) => None,
        })
        .collect()
}

fn enabled(app: &App, page: Page, label: &str) -> Option<bool> {
    app.context_entries(page)
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Item {
                label: name,
                enabled,
                ..
            } if name == label => Some(enabled),
            _ => None,
        })
}

#[test]
fn marked_atom_menu_starts_positioning_without_changing_the_drawing() -> Result<(), String> {
    use reshiki::scientific::{AtomMark, MarkKind};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let atom = app.tab.doc.add_atom("O", World::default());
    let other = app.tab.doc.add_atom("C", World::new(42., 0.));
    app.tab
        .doc
        .atom_mut(atom)
        .ok_or("Oxygen")?
        .marks
        .push(AtomMark {
            id: None,
            kind: MarkKind::LonePair,
            offset: World::new(0., -20.),
            angle: 0.,
            size_pt: None,
        });
    let before = app.tab.doc.clone();
    for target in [other, atom] {
        app.edit(Edit::ContextMenu {
            position: Point::new(30., 40.),
            selected: vec![target],
            hit: vec![target],
        });
        assert_eq!(
            labels(&app, Page::Main).contains(&"Position atom marks"),
            target == atom
        );
    }
    app.inspector_open = false;
    app.inspector_tab = InspectorTab::Abbreviations;
    run_item(&mut app, Page::Main, "Position atom marks")?;
    assert!(app.context_menu.is_none());
    assert_eq!(app.tool, Tool::EditPoints);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert_eq!(app.tab.selected, vec![atom]);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[test]
fn atom_menu_groups_editing_naming_connecting_and_clipboard_commands() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let ring = app.tab.doc.all_ids();
    let atom = app.tab.doc.atoms.first().ok_or("ring atom")?.id;
    let before = app.tab.doc.clone();
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: vec![atom],
        hit: vec![atom],
    });
    assert_eq!(
        outline(&app, Page::Main),
        [
            "Edit atom label…",
            "Edit atom map…",
            "—",
            "Show chemical name",
            "Chemical names…",
            "Select molecule",
            "—",
            "Join selected atoms or bonds",
            "Merge selected atoms",
            "Show as",
            "Arrange & transform",
            "—",
            "Cut",
            "Copy",
            "Copy as",
            "Paste",
            "Duplicate",
            "—",
            "Properties…",
            "Delete"
        ]
    );
    // Join and Merge both stay, unavailable below their minimum selection.
    assert_eq!(
        enabled(&app, Page::Main, "Join selected atoms or bonds"),
        Some(false)
    );
    assert_eq!(
        enabled(&app, Page::Main, "Merge selected atoms"),
        Some(false)
    );
    // Move & attach and Coordinate in place have one menu home, Arrange &
    // transform, built from the same commands as the context row.
    let main = labels(&app, Page::Main);
    assert!(!main.contains(&"Move & attach…"));
    assert!(!main.contains(&"Coordinate in place…"));
    let entries = app.context_entries(Page::Align);
    let index = entries
        .iter()
        .position(|entry| {
            matches!(
                entry,
                Entry::Item {
                    label: "Coordinate in place…",
                    ..
                }
            )
        })
        .ok_or("Missing Coordinate in place…")?;
    let Some(Entry::Item { action, .. }) = entries.get(index) else {
        return Err("Coordinate in place… is not a command".into());
    };
    assert_eq!(item_id(Page::Align, index, action), "coordination.begin");
    assert!(matches!(
        entries.get(index - 1),
        Some(Entry::Item {
            label: "Move & attach…",
            enabled: true,
            ..
        })
    ));
    // Clear atom map appears once the atom has a map number.
    app.tab.doc.atom_mut(atom).ok_or("atom")?.map_num = 3;
    assert_eq!(
        outline(&app, Page::Main)[..4],
        ["Edit atom label…", "Edit atom map…", "Clear atom map", "—"]
    );
    app.tab.doc.atom_mut(atom).ok_or("atom")?.map_num = 0;
    assert_eq!(app.tab.doc, before);
    // Three or more atoms: both Join and Merge run.
    app.tab.selected = ring.into_iter().take(3).collect();
    assert_eq!(
        enabled(&app, Page::Main, "Join selected atoms or bonds"),
        Some(true)
    );
    assert_eq!(
        enabled(&app, Page::Main, "Merge selected atoms"),
        Some(true)
    );
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[test]
fn expand_generic_structures_follows_the_target_and_waits_for_a_running_expansion()
-> Result<(), String> {
    use super::super::generic::Action as Generic;
    let (mut app, _) = App::new();
    let mut doc = reshiki::rings::Preset::Regular.document(42., false);
    let ring_atom = doc.atoms.first().ok_or("ring atom")?.id;
    let carbon = doc.add_atom("C", World::new(200., 0.));
    let halogen = doc.add_atom("Cl", World::new(242., 0.));
    doc.add_bond(carbon, halogen, 1, "plain");
    app.tab.doc =
        reshiki::atom_text::apply(&doc, halogen, "[Cl,Br,I]", reshiki::atom_text::Mode::Auto)?;
    assert!(reshiki::generic::present(&app.tab.doc));
    const EXPAND: &str = "Expand generic structures";
    app.tab.selected = vec![ring_atom];
    assert_eq!(
        enabled(&app, Page::Main, EXPAND),
        None,
        "A target outside every generic structure has no Expand"
    );
    app.tab.selected = vec![carbon];
    assert_eq!(enabled(&app, Page::Main, EXPAND), Some(true));
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: vec![carbon],
        hit: vec![ring_atom],
    });
    assert_eq!(enabled(&app, Page::Main, EXPAND), None);
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: vec![ring_atom],
        hit: vec![carbon],
    });
    assert_eq!(enabled(&app, Page::Main, EXPAND), Some(true));
    app.context_menu = None;
    app.tab.selected = vec![carbon];
    let outline = outline(&app, Page::Main);
    assert!(
        outline
            .windows(4)
            .any(|group| group == ["—", EXPAND, "—", "Cut"]),
        "Expand is its own group before the clipboard: {outline:?}"
    );
    assert!(app.generic_expand_enabled());
    app.tab.busy = false;
    app.tab.selected = app.tab.doc.all_ids();
    let _ = app.update(Message::Generic(Generic::Expand));
    assert!(app.generic.is_pending(), "{}", app.status);
    app.tab.selected = vec![carbon];
    assert_eq!(
        enabled(&app, Page::Main, EXPAND),
        Some(false),
        "The entry and the button wait for the running expansion"
    );
    assert!(!app.generic_expand_enabled());
    Ok(())
}

fn pet_repeat() -> reshiki::document::Document {
    use reshiki::graphics::{BracketSides, Graphic, GraphicKind};
    let mut doc = reshiki::document::Document::default();
    let chain: Vec<_> = ["C", "O", "C", "C", "O", "C"]
        .into_iter()
        .enumerate()
        .map(|(i, element)| doc.add_atom(element, World::new(i as f32 * 42., 0.)))
        .collect();
    for pair in chain.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    let ring: Vec<_> = [
        (252., 0.),
        (273., -36.),
        (315., -36.),
        (336., 0.),
        (315., 36.),
        (273., 36.),
    ]
    .into_iter()
    .map(|(x, y)| doc.add_atom("C", World::new(x, y)))
    .collect();
    doc.add_bond(chain[5], ring[0], 1, "plain");
    for i in 0..6 {
        doc.add_bond(
            ring[i],
            ring[(i + 1) % 6],
            if i % 2 == 0 { 2 } else { 1 },
            "plain",
        );
    }
    let carbonyl = doc.add_atom("C", World::new(378., 0.));
    doc.add_bond(ring[3], carbonyl, 1, "plain");
    let end = doc.add_atom("C", World::new(420., 0.));
    doc.add_bond(carbonyl, end, 1, "plain");
    for (carbon, x) in [(chain[5], 210.), (carbonyl, 378.)] {
        let oxygen = doc.add_atom("O", World::new(x, -42.));
        doc.add_bond(carbon, oxygen, 2, "plain");
    }
    let bracket = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        bracket,
        GraphicKind::Brackets,
        World::new(20., -10.),
        World::new(398., 10.),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    reshiki::generic::define_bracket(
        &doc,
        bracket,
        "3",
        reshiki::generic::RepeatPattern::HeadToTail,
    )
    .unwrap()
}

fn add_caption(doc: &mut reshiki::document::Document, text: &str) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(reshiki::document::Annotation {
        id,
        position: World::new(150., 100.),
        text: text.into(),
        format: Default::default(),
    });
    id
}

fn expansion_command(app: &App) -> Result<Action, String> {
    app.context_entries(Page::Main)
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Item {
                label: "Expand generic structures",
                action,
                enabled: true,
            } => Some(action),
            _ => None,
        })
        .ok_or("Missing caption expansion command".into())
}

#[test]
fn expand_context_follows_grouped_graphics_and_generated_name_captions() -> Result<(), String> {
    use super::super::generic::Action as Generic;
    use reshiki::graphics::{BracketSides, Graphic, GraphicKind};
    let (mut app, _) = App::new();
    app.tab.doc = pet_repeat();
    let first = app.tab.doc.atoms.first().ok_or("PET atom")?.id;
    let atoms = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    let graphic = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        graphic,
        GraphicKind::Line,
        World::new(20., 100.),
        World::new(100., 100.),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    app.tab.doc.group_selection(&[first, graphic])?;
    let name = add_caption(&mut app.tab.doc, "PET");
    app.tab
        .doc
        .molecule_names
        .push(reshiki::molecule_names::MoleculeName {
            annotation: name,
            atoms,
            smiles: "OCCOC(=O)c1ccc(C(=O)C)cc1".into(),
            text: "PET".into(),
        });
    let unrelated = app.tab.doc.add_atom("C", World::new(700., 0.));
    let ordinary = add_caption(&mut app.tab.doc, "Unrelated caption");
    app.tab.doc.validate()?;
    let before = app.tab.doc.clone();
    for hit in [graphic, name] {
        assert!(!reshiki::generic::touches(&app.tab.doc, &[hit]));
        app.edit(Edit::ContextMenu {
            position: Point::new(30., 40.),
            selected: vec![unrelated],
            hit: vec![hit],
        });
        assert!(app.context_entries(Page::Main).iter().any(|entry| matches!(entry,
            Entry::Item {
                label: "Expand generic structures",
                enabled: true,
                action: Action::Run(message),
            } if matches!(message.as_ref(), Message::Generic(Generic::ExpandComponent(ids)) if *ids == [hit])
        )));
    }
    for hit in [unrelated, ordinary] {
        app.edit(Edit::ContextMenu {
            position: Point::new(30., 40.),
            selected: app.tab.doc.all_ids(),
            hit: vec![hit],
        });
        assert_eq!(enabled(&app, Page::Main, "Expand generic structures"), None);
    }
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[test]
fn expand_context_from_grouped_pet_caption_prepares_its_component() -> Result<(), String> {
    use super::super::generic::Action as Generic;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = pet_repeat();
    let caption = add_caption(&mut app.tab.doc, "PET");
    let grouped: Vec<_> = app
        .tab
        .doc
        .atoms
        .iter()
        .map(|atom| atom.id)
        .chain([caption])
        .collect();
    app.tab.doc.group_selection(&grouped)?;
    let expected = app.tab.doc.all_ids();
    let unrelated = app.tab.doc.add_atom("C", World::new(700., 0.));
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: vec![unrelated],
        hit: vec![caption],
    });
    let before = app.tab.doc.clone();
    let source = app.tab.id;
    assert!(!reshiki::generic::touches(&app.tab.doc, &[caption]));
    assert_eq!(
        reshiki::generic::component_scope(&app.tab.doc, &[caption])?,
        expected
    );
    let action = expansion_command(&app)?;
    assert!(matches!(&action, Action::Run(message)
        if matches!(message.as_ref(), Message::Generic(Generic::ExpandComponent(ids)) if *ids == [caption])
    ));
    let task = app.context_action(action);
    assert!(app.generic.is_pending(), "{}", app.status);
    assert!(task.units() > 0);
    assert_eq!(app.tab.selected, [unrelated]);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.id, source);
    assert!(!app.tab.history.can_undo());
    assert!(app.tabs.background.is_empty());
    Ok(())
}

#[tokio::test]
async fn expand_context_from_grouped_alternative_caption_opens_concrete_products()
-> Result<(), String> {
    use super::super::generic::Action as Generic;
    use iced::futures::StreamExt;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbon = app.tab.doc.add_atom("C", World::default());
    let halogen = app.tab.doc.add_atom("Cl", World::new(42., 0.));
    app.tab.doc.add_bond(carbon, halogen, 1, "plain");
    app.tab.doc = reshiki::atom_text::apply(
        &app.tab.doc,
        halogen,
        "[Cl,Br]",
        reshiki::atom_text::Mode::Auto,
    )?;
    let caption = add_caption(&mut app.tab.doc, "Choices");
    app.tab.doc.group_selection(&[carbon, halogen, caption])?;
    let unrelated = app.tab.doc.add_atom("C", World::new(700., 0.));
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: vec![unrelated],
        hit: vec![caption],
    });
    let before = app.tab.doc.clone();
    let source = app.tab.id;
    let action = expansion_command(&app)?;
    assert!(matches!(&action, Action::Run(message)
        if matches!(message.as_ref(), Message::Generic(Generic::ExpandComponent(ids)) if *ids == [caption])
    ));
    let task = app.context_action(action);
    assert!(app.generic.is_pending(), "{}", app.status);
    assert_eq!(app.tab.selected, [unrelated]);
    assert_eq!(app.tab.doc, before);
    let mut stream = iced_runtime::task::into_stream(task).ok_or("Missing expansion task")?;
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
        }
    }
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.status,
        "Expanded 2 concrete structures in a new drawing"
    );
    assert_ne!(app.tab.id, source);
    assert_eq!(app.tab.doc.atoms.len(), 4);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    assert_eq!(app.tab.doc.annotations.len(), 2);
    assert!(
        app.tab
            .doc
            .annotations
            .iter()
            .all(|caption| caption.text == "Choices")
    );
    assert_eq!(app.tab.doc.groups.len(), 2);
    app.tab.doc.validate()?;
    let background = app.tabs.background.first().ok_or("Expansion source tab")?;
    assert_eq!(background.doc, before);
    assert!(!background.history.can_undo());
    Ok(())
}

#[tokio::test]
async fn expand_pet_from_ring_bond_context_opens_complete_product() {
    use iced::futures::StreamExt;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = pet_repeat();
    let bond = app.tab.doc.bonds[6].clone();
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: vec![bond.a, bond.b],
        hit: vec![bond.a, bond.b],
    });
    let before = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let source = app.tab.id;
    let action = app
        .context_entries(Page::Main)
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Item {
                label: "Expand generic structures",
                action,
                enabled: true,
            } => Some(action),
            _ => None,
        })
        .unwrap();
    let task = app.context_action(action);
    assert!(app.generic.is_pending(), "{}", app.status);
    assert_eq!(app.tab.selected, selected);
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
        }
    }
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.status,
        "Expanded 1 concrete structures in a new drawing"
    );
    assert_ne!(app.tab.id, source);
    assert_eq!(app.tab.doc.atoms.len(), 44);
    assert_eq!(app.tab.doc.bonds.len(), 46);
    let molecule = reshiki::chemistry::document::prepare(&app.tab.doc).unwrap();
    let facts = molecule
        .state
        .graph
        .atoms
        .iter()
        .zip(&molecule.state.valences)
        .map(|(atom, valence)| reshiki::chemistry::AtomFacts {
            atomic_number: atom.atomic_number,
            isotope: atom.isotope,
            charge: atom.charge,
            hydrogens: (u32::from(atom.explicit_hydrogens) + valence.implicit_hydrogens) as u8,
            radical_electrons: atom.radical_electrons,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reshiki::chemistry::properties(&facts).unwrap().formula,
        "C32H30O12"
    );
    assert_eq!(app.tabs.background.first().unwrap().doc, before);
    assert!(!app.tabs.background.first().unwrap().history.can_undo());
}

#[tokio::test]
async fn expand_context_uses_clicked_molecule_when_selection_contains_another_molecule() {
    use iced::futures::StreamExt;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = pet_repeat();
    let carbon = app.tab.doc.add_atom("C", World::new(700., 0.));
    let oxygen = app.tab.doc.add_atom("O", World::new(742., 0.));
    app.tab.doc.add_bond(carbon, oxygen, 1, "plain");
    let bond = app.tab.doc.bonds[6].clone();
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: app.tab.doc.all_ids(),
        hit: vec![bond.a, bond.b],
    });
    let before = app.tab.doc.clone();
    let action = app
        .context_entries(Page::Main)
        .into_iter()
        .find_map(|entry| match entry {
            Entry::Item {
                label: "Expand generic structures",
                action,
                enabled: true,
            } => Some(action),
            _ => None,
        })
        .unwrap();
    let mut stream = iced_runtime::task::into_stream(app.context_action(action)).unwrap();
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
        }
    }
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.status,
        "Expanded 1 concrete structures in a new drawing"
    );
    assert_eq!(app.tab.doc.atoms.len(), 44);
    assert_eq!(app.tab.doc.bonds.len(), 46);
    assert_eq!(app.tabs.background.first().unwrap().doc, before);
}

#[test]
fn properties_expand_pet_ring_bond_selection_explains_whole_molecule_requirement() {
    let (mut app, _) = App::new();
    app.tab.doc = pet_repeat();
    let bond = &app.tab.doc.bonds[6];
    app.tab.selected = vec![bond.a, bond.b];
    let before = app.tab.doc.clone();
    let task = app.generic_action(super::super::generic::Action::Expand);
    assert_eq!(task.units(), 0);
    assert!(app.error);
    assert_eq!(
        app.status,
        "Select whole molecules, or clear the selection to expand the whole drawing"
    );
    assert!(!app.generic.is_pending());
    assert_eq!(app.tab.doc, before);
    assert!(app.tabs.background.is_empty());
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
fn chemical_names_entry_selects_the_clicked_molecule_and_opens_its_inspector() -> Result<(), String>
{
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::document::Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))?;
    let mut first: Vec<_> = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    let source = app.tab.doc.clone();
    reshiki::editing::append(&mut app.tab.doc, &source, World::new(250., 0.));
    let before = app.tab.doc.clone();
    app.edit(Edit::ContextMenu {
        position: Point::new(30., 40.),
        selected: vec![first[1]],
        hit: vec![first[1]],
    });
    let entries = app.context_entries(Page::Main);
    let index = entries
        .iter()
        .position(|entry| {
            matches!(
                entry,
                Entry::Item {
                    label: "Chemical names…",
                    ..
                }
            )
        })
        .ok_or("Missing Chemical names…")?;
    let Some(Entry::Item {
        action, enabled, ..
    }) = entries.get(index)
    else {
        return Err("Chemical names… is not a command".into());
    };
    assert!(enabled);
    assert_eq!(item_id(Page::Main, index, action), "menu-chemical-names");
    // It follows Show chemical name, which keeps its positional id.
    let Some(Entry::Item {
        label: "Show chemical name",
        action: show,
        ..
    }) = entries.get(index - 1)
    else {
        return Err("Chemical names… does not follow Show chemical name".into());
    };
    assert_eq!(
        item_id(Page::Main, index - 1, show),
        format!("menu-Main-{}", index - 1)
    );
    run_item(&mut app, Page::Main, "Chemical names…")?;
    assert!(app.context_menu.is_none());
    assert_eq!(app.inspector_tab, InspectorTab::Names);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_width(), 300.);
    let mut selected = app.tab.selected.clone();
    selected.sort_unstable();
    first.sort_unstable();
    assert_eq!(selected, first);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
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
        ["3D optimize…", "Join selected atoms or bonds"],
        "Default keyboard drawing folds the selection commands first"
    );
    assert_eq!(
        labels(&app, Page::More(4)),
        [
            "3D optimize…",
            "Join selected atoms or bonds",
            "Merge selected atoms",
            "Move & attach…"
        ],
        "⋯ keeps Join, Merge and Move & attach in row order"
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

#[test]
fn show_as_offers_the_views_the_molecule_is_not_in() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    assert!(outline(&app, Page::Main).contains(&"Show as"));
    assert_eq!(enabled(&app, Page::Model, "2D drawing"), Some(false));
    assert_eq!(enabled(&app, Page::Model, "3D model"), Some(true));
    assert_eq!(enabled(&app, Page::Model, "2D and 3D"), Some(true));
}

#[test]
fn show_as_can_cancel_a_generation_in_flight() {
    use reshiki::models3d::Display;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    let _ = app.update(Message::Models3d(super::super::models3d::Action::Show(
        Display::ThreeD,
    )));
    assert!(app.tab.model_job.is_some(), "generation started");
    assert_eq!(enabled(&app, Page::Model, "2D drawing"), Some(true));
    assert_eq!(enabled(&app, Page::Model, "3D model"), Some(false));
}

#[test]
fn show_as_is_offered_for_a_molecule_shown_in_3d() {
    use reshiki::models3d::{self, Display, ForceField, Model3d, Placed};
    let (mut app, _) = App::new();
    let doc = &mut app.tab.doc;
    let c = doc.add_atom("C", World::new(0., 0.));
    let o = doc.add_atom("O", World::new(36., 0.));
    doc.add_bond(c, o, 1, "plain");
    let placed = |id, xyz| Placed { id, xyz };
    let model = Model3d::new(
        doc,
        vec![placed(c, [-0.7, 0., 0.]), placed(o, [0.7, 0., 0.])],
        vec![],
        ForceField::Mmff94s,
    );
    doc.models.push(model);
    reshiki::models3d::require_version(doc);
    assert!(models3d::set_display(doc, 0, Display::ThreeD));
    app.tab.selected = vec![c, o];
    assert!(outline(&app, Page::Main).contains(&"Show as"));
    assert_eq!(enabled(&app, Page::Model, "3D model"), Some(false));
    assert_eq!(enabled(&app, Page::Model, "2D drawing"), Some(true));
    assert_eq!(
        enabled(&app, Page::Model, "2D from the 3D view"),
        Some(true)
    );
    assert_eq!(enabled(&app, Page::Model, "2D and 3D"), Some(true));
}
