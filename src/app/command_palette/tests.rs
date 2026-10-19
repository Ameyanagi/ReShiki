use super::*;
use crate::canvas::Tool;
use reshiki::bonds::BondPreset;

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.bond_drawing.length = 42.;
    app
}

/// Press `:` through the real key router.
fn open(app: &mut App) {
    let message = crate::app::shortcuts::key_message(
        &Key::Character(";".into()),
        &Key::Character(":".into()),
        Modifiers::SHIFT,
    )
    .expect(": is routed");
    let _ = app.update(message);
}

fn search(app: &mut App, query: &str) {
    open(app);
    assert!(app.commands.open, ": opens the palette");
    let _ = app.update(Message::Commands(Action::Query(query.into())));
}

fn run(app: &mut App, query: &str) {
    search(app, query);
    let _ = app.update(Message::Commands(Action::Submit));
}

fn top(app: &App) -> &Command {
    app.commands.current().expect("a result")
}

fn ethane(app: &mut App) -> (u64, u64) {
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    (a, b)
}

fn order(app: &App, a: u64, b: u64) -> u8 {
    app.tab
        .doc
        .bonds
        .iter()
        .find(|e| (e.a, e.b) == (a, b))
        .unwrap()
        .order
}

#[test]
fn colon_opens_the_palette_and_benzene_inserts_one_undo_step() {
    let mut app = app();
    let before = app.tab.doc.clone();
    search(&mut app, "benzene");
    assert_eq!(top(&app).name, "Benzene");
    let _ = app.update(Message::Commands(Action::Submit));
    assert!(!app.commands.open, "running closes the palette");
    assert_eq!(app.tab.doc.atoms.len(), 6);
    assert_eq!(app.tab.doc.bonds.iter().filter(|b| b.order == 2).count(), 3);
    assert_eq!(app.tab.selected.len(), 6, "the new ring is selected");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before, "one undo removes the whole ring");
    assert!(!app.tab.history.can_undo());
}

#[test]
fn duplicate_titles_prefer_the_direct_insert() {
    let mut app = app();
    app.templates.library.templates = vec![reshiki::templates::Template {
        id: "test:benzene".into(),
        group: "Rings".into(),
        name: "Benzene".into(),
        smiles: "c1ccccc1".into(),
        keywords: vec![],
        note: String::new(),
        document: reshiki::rings::Preset::Benzene.document(42., false),
        anchor: Default::default(),
    }];
    search(&mut app, "benzene");
    assert!(
        app.commands
            .catalog()
            .iter()
            .filter(|command| command.name == "Benzene")
            .count()
            > 1,
        "the catalog lists Benzene as a direct insert, template and example"
    );
    let ranked = app.commands.ranked();
    let benzene: Vec<_> = ranked.iter().filter(|c| c.name == "Benzene").collect();
    assert!(
        matches!(benzene.as_slice(), [direct, saved]
            if matches!(direct.run, Run::Ring(_))
                && matches!(&saved.run, Run::Template(_, id) if id == "test:benzene")),
        "the built-in template and example repeat the direct insert; the saved template stays"
    );
    assert_eq!(top(&app).name, "Benzene");
    assert!(matches!(
        top(&app).run,
        Run::Ring(Ring::Preset(reshiki::rings::Preset::Benzene))
    ));
    assert!(
        ranked
            .iter()
            .any(|command| command.name == "Benzene (aromatic circle)"),
        "distinct visible titles stay listed"
    );
}

#[test]
fn benzene_attaches_to_the_selected_atom_as_the_a_hotkey_does() {
    let mut app = app();
    let (a, _) = ethane(&mut app);
    app.tab.selected = vec![a];
    let (expected, _) = reshiki::hotkeys::ring_edit(&app.tab.doc, Some(a), None, "a", 42.)
        .unwrap()
        .unwrap();
    let before = app.tab.doc.clone();
    search(&mut app, "benzene");
    assert_eq!(top(&app).name, "Benzene");
    let _ = app.update(Message::Commands(Action::Submit));
    assert_eq!(
        app.tab.doc.atoms.len(),
        expected.atoms.len(),
        "{}",
        app.status
    );
    let place = |doc: &reshiki::document::Document| {
        doc.atoms
            .iter()
            .map(|atom| (atom.id, atom.position))
            .collect::<Vec<_>>()
    };
    assert_eq!(place(&app.tab.doc), place(&expected));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn double_bond_edits_the_selected_then_the_pointed_bond_or_chooses_the_tool() {
    let mut app = app();
    let (a, b) = ethane(&mut app);
    app.tab.selected = vec![a, b];
    search(&mut app, "double bond");
    assert_eq!(top(&app).name, "Double bond");
    let _ = app.update(Message::Commands(Action::Submit));
    assert_eq!(order(&app, a, b), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(order(&app, a, b), 1, "one undo restores the single bond");
    assert!(!app.tab.history.can_undo());

    // Nothing selected: the bond under the pointer.
    app.tab.selected.clear();
    app.tab.hover = Some((Point::new(21., 0.), app.tab.file_epoch));
    run(&mut app, "triple bond");
    assert_eq!(order(&app, a, b), 3);
    assert_eq!(app.tab.selected, vec![a, b]);

    // No bond at all: choose that bond tool, changing nothing.
    let mut app = self::app();
    run(&mut app, "wedge bond");
    assert_eq!(app.tool, Tool::Wedge);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn element_and_group_label_the_target_atom_or_insert_at_the_center() {
    let mut app = app();
    let (a, _) = ethane(&mut app);
    app.tab.selected = vec![a];
    run(&mut app, "bromine");
    assert_eq!(app.tab.doc.atom(a).unwrap().element, "Br");

    let mut app = self::app();
    run(&mut app, "ome");
    assert_eq!(top_name_after(&app), None);
    assert_eq!(app.tab.doc.abbreviations.len(), 1, "OMe is a group");
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.atoms.is_empty());

    // An element with nothing targeted chooses the atom tool.
    let mut app = self::app();
    run(&mut app, "palladium");
    assert_eq!((app.tool, app.element.as_str()), (Tool::Atom, "Pd"));
}

#[test]
fn gold_finds_the_au_element_command() {
    let mut app = app();
    search(&mut app, "gold");
    assert_eq!(top(&app).name, "Gold (Au)");
    assert!(matches!(top(&app).run, Run::Element("Au")));
}

fn top_name_after(app: &App) -> Option<&str> {
    app.commands.open.then(|| top(app).name.as_str())
}

#[test]
fn atom_edits_use_the_contextual_keys() {
    let mut app = app();
    let (a, _) = ethane(&mut app);
    app.tab.selected = vec![a];
    run(&mut app, "increase charge");
    assert_eq!(app.tab.doc.atom(a).unwrap().charge, 1);
    run(&mut app, "charge -1");
    assert_eq!(app.tab.doc.atom(a).unwrap().charge, -1);
}

#[test]
fn tool_and_menu_entries_send_their_messages() {
    let mut app = app();
    run(&mut app, "lasso");
    assert_eq!(app.tool, Tool::Lasso);
    run(&mut app, "straight chain");
    assert!(matches!(app.tool, Tool::Chain(_)));
    let zoom = app.tab.camera.zoom;
    run(&mut app, "zoom in");
    assert!(app.tab.camera.zoom > zoom);
    run(&mut app, "zoom 50");
    assert!((app.tab.camera.zoom - 0.5).abs() < 1e-4);
    // Menu rows show their shortcut.
    search(&mut app, "select all");
    assert_eq!(
        top(&app).keys,
        crate::app::shortcuts::label(&Message::SelectAll).unwrap()
    );
}

#[test]
fn nmr_runs_the_predict_button_message() {
    let mut app = app();
    ethane(&mut app);
    search(&mut app, "nmr");
    assert!(
        matches!(&top(&app).run, Run::Send(messages)
            if matches!(messages.as_slice(), [Message::Nmr(crate::app::nmr::Action::Open)])),
        "the Properties panel's Predict NMR… button sends Nmr(Open)"
    );
    let _ = app.update(Message::Commands(Action::Submit));
    assert!(app.tab.nmr.open);
}

#[test]
fn unavailable_commands_stay_listed_and_do_nothing() {
    let mut app = app();
    search(&mut app, "copy nmr report");
    assert_eq!(top(&app).name, "Copy NMR report");
    assert!(app.command_unavailable(top(&app)).is_some());
    let _ = app.update(Message::Commands(Action::Submit));
    assert!(app.error);
}

#[test]
fn caption_and_graphic_selections_never_fall_back_to_pointer_or_hotspot() {
    use reshiki::{
        document::Annotation,
        graphics::{Graphic, GraphicKind},
    };
    for keyboard in [false, true] {
        let mut app = app();
        let (a, b) = ethane(&mut app);
        let caption = app.tab.doc.next_id();
        app.tab.doc.annotations.push(Annotation {
            id: caption,
            position: Point::new(150., 100.),
            text: "Caption".into(),
            format: Default::default(),
        });
        let graphic = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::dragged(
            graphic,
            GraphicKind::Rectangle,
            Point::new(200., 100.),
            Point::new(250., 150.),
            Default::default(),
            Default::default(),
            false,
        ));
        if !keyboard {
            app.tab.keyboard_drawing.leave();
        }
        let before = app.tab.doc.clone();
        for selected in [vec![caption], vec![graphic], vec![caption, graphic]] {
            for (query, hotspot) in [
                ("increase charge", Hotspot::Atom(a)),
                ("radical", Hotspot::Atom(a)),
                ("reverse bond direction", Hotspot::Bond(a, b)),
            ] {
                app.tab.selected = selected.clone();
                app.tab.hover = Some((hotspot.point(&app.tab.doc).unwrap(), app.tab.file_epoch));
                app.tab.keyboard_drawing.set_target(hotspot, &app.tab.doc);
                search(&mut app, query);
                assert_eq!(app.keyboard_target(), keyboard);
                let reason = app.command_unavailable(top(&app)).expect(query);
                let _ = app.update(Message::Commands(Action::Submit));
                assert!(app.error);
                assert_eq!(app.status, reason);
                assert_eq!(app.tab.doc, before);
                assert_eq!(app.tab.selected, selected);
                assert!(!app.tab.history.can_undo());
            }
        }
    }
}

#[test]
fn vim_aliases_rank_first() {
    let mut app = app();
    for (alias, name) in [
        ("w", "Save"),
        (":w", "Save"),
        ("w out.cdxml", "Save as…"),
        ("q", "Close tab"),
        ("q!", "Close tab without saving"),
        ("wq", "Save and close tab"),
        ("x", "Save and close tab"),
        ("qa", "Close window"),
        ("e", "Open…"),
        ("tabnew", "New drawing"),
        ("tabn", "Next tab"),
        ("u", "Undo"),
        ("redo", "Redo"),
        ("noh", "Clear highlights"),
    ] {
        search(&mut app, alias);
        assert_eq!(top(&app).name, name, "{alias}");
    }
    search(&mut app, "w");
    assert!(matches!(&top(&app).run, Run::Send(m) if matches!(m.as_slice(), [Message::Save])));
    assert!(top(&app).hint().starts_with(":w"));
}

#[test]
fn quit_on_a_dirty_tab_asks_to_save_and_bang_discards() {
    let mut app = app();
    ethane(&mut app);
    assert!(app.dirty());
    run(&mut app, "q");
    assert!(
        matches!(app.pending, Some(Pending::CloseTab(_))),
        "asks first"
    );
    let _ = app.update(Message::Cancel);
    assert!(app.dirty());

    run(&mut app, "q!");
    assert!(app.pending.is_none());
    assert!(!app.dirty(), "the edited drawing was closed without saving");

    let mut app = self::app();
    ethane(&mut app);
    run(&mut app, "wq");
    assert!(
        matches!(app.pending, Some(Pending::CloseTab(_))),
        "the tab closes once the save succeeds"
    );
    // While that save is in progress, :q! keeps Close's own save barrier.
    let _ = app.update(Message::Cancel);
    run(&mut app, "q!");
    assert!(app.dirty(), "a save in progress is never discarded");
}

#[test]
fn recent_commands_lead_an_empty_query() {
    let mut app = app();
    run(&mut app, "lasso");
    run(&mut app, "zoom in");
    open(&mut app);
    let ranked = app.commands.ranked();
    assert_eq!(ranked[0].name, "Zoom in");
    assert_eq!(ranked[1].name, "Lasso tool");
}

#[test]
fn context_favors_commands_for_the_target() {
    let mut app = app();
    let (a, b) = ethane(&mut app);
    app.tab.selected = vec![a, b];
    search(&mut app, "wedge");
    assert_eq!(top(&app).category, Category::Bond);
}

#[test]
fn unknown_words_offer_a_name_and_valid_smiles() {
    let mut app = app();
    search(&mut app, "ibuprofen");
    let names: Vec<_> = app
        .commands
        .ranked()
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert!(
        names
            .iter()
            .any(|n| n == "Insert structure named “ibuprofen”"),
        "{names:?}"
    );
    search(&mut app, "c1ccccc1O");
    assert!(
        app.commands
            .ranked()
            .iter()
            .any(|c| matches!(&c.run, Run::Smiles(s) if s == "c1ccccc1O"))
    );
}

#[test]
fn colon_is_typed_into_an_open_label_editor() {
    let mut app = app();
    let (a, _) = ethane(&mut app);
    let _ = app.update(Message::AtomText(crate::app::atom_text::Action::Begin(
        Some(a),
    )));
    open(&mut app);
    assert!(!app.commands.open, "the label draft keeps `:`");
}

#[test]
fn escape_closes() {
    let mut app = app();
    open(&mut app);
    assert!(matches!(
        modal_key(&Key::Named(Named::Escape), Modifiers::empty()),
        Some(Message::Commands(Action::Close))
    ));
    let _ = app.update(Message::Escape);
    assert!(!app.commands.open);
}

/// Every entry runs through a real path: its messages exist, its hotkeys are
/// assigned, and running it on a small drawing does not panic.
#[test]
fn every_catalog_entry_runs() {
    let mut fixture = app();
    open(&mut fixture);
    let catalog = fixture.commands.catalog().to_vec();
    let mut counts = std::collections::BTreeMap::new();
    for command in &catalog {
        *counts.entry(command.category.label()).or_insert(0) += 1;
        match &command.run {
            Run::Send(messages) => assert!(!messages.is_empty(), "{}", command.name),
            Run::AtomKey(key) if *key != "Enter" => {
                let doc = reshiki::rings::Preset::Benzene.document(42., false);
                let id = doc.atoms[0].id;
                assert!(
                    reshiki::hotkeys::atom_edit(&doc, id, key, 42.).is_some(),
                    "{}: {key} is not an atom hotkey",
                    command.name
                );
            }
            Run::Ring(ring) => {
                let (atom, bond) = ring.keys();
                let doc = reshiki::rings::Preset::Benzene.document(42., false);
                let (a, b) = (doc.bonds[0].a, doc.bonds[0].b);
                if let Some(key) = atom {
                    assert!(reshiki::hotkeys::ring_edit(&doc, Some(a), None, key, 42.).is_some());
                }
                if let Some(key) = bond {
                    assert!(
                        reshiki::hotkeys::ring_edit(&doc, None, Some((a, b)), key, 42.).is_some()
                    );
                }
            }
            _ => {}
        }
    }
    eprintln!(
        "Command palette catalog: {} entries, {counts:?}",
        catalog.len()
    );
    assert!(catalog.len() > 300, "{}", catalog.len());
    for category in Category::ALL {
        assert!(counts.contains_key(category.label()), "{category:?}");
    }
    for preset in BondPreset::ALL {
        assert!(
            catalog
                .iter()
                .any(|c| matches!(c.run, Run::Bond(p) if p == preset))
        );
    }
    for command in catalog {
        let mut app = app();
        let doc = reshiki::rings::Preset::Benzene.document(42., false);
        let (a, b) = (doc.bonds[0].a, doc.bonds[0].b);
        app.tab.doc = doc;
        app.tab.selected = match command.category {
            Category::Bond => vec![a, b],
            Category::Atom | Category::Insert => vec![a],
            _ => app.tab.doc.atoms.iter().map(|atom| atom.id).collect(),
        };
        let name = command.name.clone();
        let _ = app.run_command(command);
        app.tab
            .doc
            .validate()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn colon_typed_with_shift_and_a_digit_still_opens_the_palette() {
    let message = crate::app::shortcuts::key_message(
        &Key::Character("6".into()),
        &Key::Character(":".into()),
        Modifiers::SHIFT,
    );
    assert!(matches!(message, Some(Message::Commands(Action::Open))));
}

#[test]
fn review_fixes_for_targets_parameters_and_stale_rows() {
    // Invert selection works with nothing selected, as its shortcut does.
    let mut app = app();
    ethane(&mut app);
    search(&mut app, "invert selection");
    assert!(app.command_unavailable(top(&app)).is_none());

    // Chain lengths follow the chain field's own limits.
    assert!(catalog::parameterised(&app, "chain 513").is_none());
    let chain = catalog::parameterised(&app, "chain 5.0").unwrap();
    assert!(matches!(&chain.run, Run::Send(m)
        if matches!(m.as_slice(), [Message::ChainAtoms(n), _] if n == "5")));

    // A template row that no longer matches the library does nothing.
    let before = app.tab.doc.clone();
    let stale = Command::new(
        "Gone",
        Category::Insert,
        "",
        "",
        Run::Template(usize::MAX, "missing-template".into()),
    );
    let _ = app.run_command(stale);
    assert!(app.error);
    assert_eq!(app.tab.doc, before);

    // Analysis on a pointed atom selects its whole molecule.
    let mut app = self::app();
    let (a, b) = ethane(&mut app);
    let far = app.tab.doc.add_atom("O", Point::new(400., 0.));
    app.tab.hover = Some((Point::new(0., 0.), app.tab.file_epoch));
    run(&mut app, "name molecule");
    let mut selected = app.tab.selected.clone();
    selected.sort_unstable();
    assert_eq!(selected, vec![a, b]);
    assert!(!app.tab.selected.contains(&far));
}

#[test]
fn a_cleanup_preview_arriving_under_the_palette_still_lets_it_close() {
    let mut app = app();
    open(&mut app);
    assert!(app.commands.open);
    app.tab.cleanup = Some(crate::app::CleanupPreview {
        job: crate::app::cleanup::CleanupJob {
            options: Default::default(),
            selection: vec![],
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        },
        warnings: vec![],
        document: app.tab.doc.clone(),
        analysis: None,
        revision: app.tab.revision,
        epoch: app.tab.file_epoch,
        original: false,
    });
    // Escape, the close button and clicks outside all send Close.
    let _ = app.update(Message::Commands(Action::Close));
    assert!(!app.commands.open);
    assert!(
        app.tab.cleanup.is_some(),
        "closing leaves the preview to apply or cancel"
    );
}

/// Windows draws with Tiny Skia, whose shadow pass ignores the damage clip
/// (see `appearance::surface_shadow`); the palette must leave pixels outside
/// a hover or caret redraw untouched.
#[cfg(windows)]
#[test]
fn palette_partial_redraws_preserve_unchanged_pixels() {
    use iced::advanced::{graphics::Viewport, layout, mouse, widget::Tree};
    use iced::{Color, Length, widget::Space};
    use resvg::tiny_skia::{Mask, Pixmap};

    for mode in [
        crate::appearance::Mode::Light,
        crate::appearance::Mode::Dark,
    ] {
        let mut app = app();
        app.appearance.mode = mode;
        search(&mut app, "benzene");
        let size = iced::Size::new(1040., 680.);
        let bounds = iced::Rectangle::with_size(size);
        let mut renderer = iced::Renderer::Secondary(iced_tiny_skia::Renderer::new(
            iced::Font::default(),
            iced::Pixels(16.),
        ));
        let mut view =
            app.with_commands(Space::new().width(Length::Fill).height(Length::Fill).into());
        let mut tree = Tree::new(view.as_widget());
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        view.as_widget_mut().update(
            &mut tree,
            &iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut Vec::new()),
            &bounds,
        );
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &app.theme(),
            &iced::advanced::renderer::Style::default(),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &bounds,
        );
        // A hover redraw on a result row near the dialog's edge.
        let damage = iced::Rectangle {
            x: 300.,
            y: 200.,
            width: 40.,
            height: 40.,
        };
        let iced::Renderer::Secondary(mut renderer) = renderer else {
            panic!("partial-redraw regression requires the software renderer");
        };
        for scale in [1., 1.25, 2.] {
            let width = (size.width * scale) as u32;
            let height = (size.height * scale) as u32;
            let viewport = Viewport::with_physical_size(iced::Size::new(width, height), scale);
            let mut pixels = Pixmap::new(width, height).unwrap();
            let mut mask = Mask::new(width, height).unwrap();
            renderer.draw(
                &mut pixels.as_mut(),
                &mut mask,
                &viewport,
                &[bounds],
                Color::WHITE,
            );
            let original = pixels.clone();
            for _ in 0..16 {
                renderer.draw(
                    &mut pixels.as_mut(),
                    &mut mask,
                    &viewport,
                    &[damage],
                    Color::WHITE,
                );
            }
            let physical_damage = damage * scale;
            let changed = pixels
                .pixels()
                .iter()
                .zip(original.pixels())
                .enumerate()
                .filter(|(index, (actual, expected))| {
                    let point = iced::Point::new(
                        (*index % width as usize) as f32,
                        (*index / width as usize) as f32,
                    );
                    !physical_damage.contains(point) && actual != expected
                })
                .count();
            assert_eq!(
                changed, 0,
                "{mode:?}, scale {scale}: partial redraws changed pixels outside damage"
            );
        }
    }
}
