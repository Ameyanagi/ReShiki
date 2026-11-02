use super::*;
use crate::canvas::Tool;
use reshiki::bonds::BondPreset;
use reshiki::document::Document;

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

/// The items index of the listed row with this name.
fn row(app: &App, name: &str) -> usize {
    let state = &app.commands;
    *state
        .results
        .iter()
        .find(|i| state.items[**i].name == name)
        .unwrap_or_else(|| panic!("{name} is listed"))
}

fn top(app: &App) -> &Command {
    app.commands.current().expect("a result")
}

#[test]
fn ionic_and_centred_charge_commands_follow_selection_eligibility() {
    let mut app = app();
    let o = app.tab.doc.add_atom("O", Point::default());
    let na = app.tab.doc.add_atom("Na", Point::new(42., 0.));
    app.tab.doc.add_bond(o, na, 1, "plain");
    search(&mut app, "Make ionic");
    assert!(app.command_unavailable(top(&app)).is_some());
    app.tab.selected = vec![o, na];
    assert!(app.command_unavailable(top(&app)).is_none());
    let _ = app.update(Message::Commands(Action::Submit));
    assert_eq!(app.tab.doc.atom(na).unwrap().charge, 1);
    assert!(app.tab.doc.bonds.is_empty());

    app.tab.doc = Document::default();
    app.tab.selected = reshiki::editing::ring(&mut app.tab.doc, Point::default(), 7, true, 5.);
    search(&mut app, "Centre charge in ring");
    assert!(app.command_unavailable(top(&app)).is_some());
    app.tab.doc.atoms[0].charge = 1;
    assert!(app.command_unavailable(top(&app)).is_none());
    let _ = app.update(Message::Commands(Action::Submit));
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .all(|a| a.display.center_charge_in_ring)
    );
}

#[test]
fn partial_charge_symbols_are_searchable_by_delta() {
    let mut app = app();
    search(&mut app, "delta");
    row(&app, "Partial positive");
    row(&app, "Partial negative");
}

#[test]
fn path_cleanup_catalog_has_selection_commands_and_explicit_upright_axis() {
    for (query, name, upright) in [
        ("Smooth", "Smooth", None),
        ("Symmetrise", "Symmetrise", Some(false)),
        ("Symmetrise upright", "Symmetrise upright", Some(true)),
    ] {
        let mut app = app();
        search(&mut app, query);
        let index = row(&app, name);
        assert_eq!(top(&app).name, name);
        let command = &app.commands.items[index];
        assert_eq!(command.need, catalog::Need::Selection);
        let catalog::Run::Send(messages) = &command.run else {
            panic!("cleanup dispatch");
        };
        let [Message::Graphics(super::super::graphics::Action::Cleanup(cleanup))] =
            messages.as_slice()
        else {
            panic!("cleanup message");
        };
        match (cleanup, upright) {
            (super::super::graphics::Cleanup::Smooth, None) => {}
            (super::super::graphics::Cleanup::Symmetrise { upright: actual }, Some(expected)) => {
                assert_eq!(*actual, expected);
            }
            _ => panic!("cleanup axis"),
        }
    }
}

#[test]
fn exact_template_keywords_rank_as_names_without_changing_other_first_results() {
    let mut app = app();
    // Before #630, Phe ranked Phenol first; the other first results are kept.
    for (query, expected) in [
        ("Phe", "L-Phenylalanine"),
        ("pHE", "L-Phenylalanine"),
        ("phenol", "Phenol"),
        ("Trp", "L-Tryptophan"),
        ("Gly", "Glycine"),
        ("benzene", "Benzene"),
        ("F", "CF3"),
        ("f", "Fluorine (F)"),
    ] {
        search(&mut app, query);
        assert_eq!(top(&app).name, expected, "{query}");
    }
}

/// The pre-#587 query, kept independent of the indexed membership path.
fn original_command_target(app: &App) -> Target {
    let doc = &app.tab.doc;
    let selected = &app.tab.selected;
    let target = Target {
        atoms: selected
            .iter()
            .copied()
            .filter(|id| doc.atom(*id).is_some())
            .collect(),
        bonds: doc
            .bonds
            .iter()
            .filter(|bond| selected.contains(&bond.a) && selected.contains(&bond.b))
            .map(|bond| (bond.a, bond.b))
            .collect(),
        pointed: false,
    };
    if !selected.is_empty() {
        return target;
    }
    let pointed = if app.keyboard_target() {
        match app.tab.keyboard_drawing.target() {
            Hotspot::Atom(id) => (Some(id), None),
            Hotspot::Bond(a, b) => (None, Some((a, b))),
            Hotspot::Blank(_) => (None, None),
        }
    } else {
        app.hovered_target()
    };
    match pointed {
        (Some(id), _) => Target {
            atoms: vec![id],
            pointed: true,
            ..Target::default()
        },
        (_, Some((a, b))) => Target {
            atoms: vec![a, b],
            bonds: vec![(a, b)],
            pointed: true,
        },
        _ => Target::default(),
    }
}

fn assert_original_target(app: &App) {
    let actual = app.command_target();
    let expected = original_command_target(app);
    assert_eq!(actual.atoms, expected.atoms, "atom order and duplicates");
    assert_eq!(actual.bonds, expected.bonds, "bond order and endpoints");
    assert_eq!(actual.pointed, expected.pointed, "selection precedence");
    assert_eq!(actual.atom(), expected.atom());
    assert_eq!(actual.bond(), expected.bond());
}

#[test]
fn indexed_command_targets_match_the_original_selection_and_pointing_query() {
    use reshiki::{
        document::Annotation,
        graphics::{Graphic, GraphicKind},
    };
    let mut app = app();
    let (a, b) = ethane(&mut app);
    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(100., 100.),
        text: "Caption".into(),
        format: Default::default(),
    });
    let graphic = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        graphic,
        GraphicKind::Rectangle,
        Point::new(150., 100.),
        Point::new(200., 150.),
        Default::default(),
        Default::default(),
        false,
    ));
    let ordinary = app.tab.doc.clone();
    let mut malformed = ordinary.clone();
    malformed.atoms.push(malformed.atoms[0].clone());
    let mut reversed = malformed.bonds[0].clone();
    std::mem::swap(&mut reversed.a, &mut reversed.b);
    malformed.bonds.push(reversed);
    malformed.bonds.push(malformed.bonds[0].clone());
    let mut missing = malformed.bonds[0].clone();
    missing.a = u64::MAX;
    malformed.bonds.push(missing);
    let collapsed = reshiki::atom_text::apply(&ordinary, b, "OMe", Mode::Group).unwrap();
    let (model, _, _) = crate::canvas::tilt::shown(reshiki::models3d::Display::ThreeD);

    for doc in [ordinary.clone(), malformed, collapsed, model] {
        app.tab.doc = doc;
        app.tab.hover = Some((Point::default(), app.tab.file_epoch));
        app.tab
            .keyboard_drawing
            .enter(&app.tab.doc, &[], Point::default(), app.tab.file_epoch);
        app.tab
            .keyboard_drawing
            .set_target(Hotspot::Atom(a), &app.tab.doc);
        for selected in [
            app.tab.doc.all_ids(),
            vec![a],
            vec![b, a],
            vec![b, a, b],
            vec![caption],
            vec![graphic],
            vec![caption, graphic],
            vec![u64::MAX],
            vec![caption, u64::MAX, b, a, b],
        ] {
            app.tab.selected = selected;
            assert_original_target(&app);
        }
    }

    // No selection: retain hover precedence and the tool/keyboard-mode gate,
    // including a keyboard-owned blank over an otherwise hoverable bond.
    app.tab.doc = ordinary;
    app.tab.selected.clear();
    for tool in [Tool::Select, Tool::Lasso, Tool::Bond(1)] {
        app.tool = tool;
        for keyboard in [false, true] {
            app.tab
                .keyboard_drawing
                .enter(&app.tab.doc, &[], Point::default(), app.tab.file_epoch);
            if !keyboard {
                app.tab.keyboard_drawing.leave();
            }
            for hotspot in [
                Hotspot::Atom(a),
                Hotspot::Bond(b, a),
                Hotspot::Blank(Point::new(400., 400.)),
            ] {
                app.tab.keyboard_drawing.set_target(hotspot, &app.tab.doc);
                for hover in [None, Some(Point::default()), Some(Point::new(21., 0.))] {
                    app.tab.hover = hover.map(|point| (point, app.tab.file_epoch));
                    assert_original_target(&app);
                }
                app.tab.hover = Some((Point::default(), app.tab.file_epoch.saturating_add(1)));
                assert_original_target(&app);
            }
        }
    }
}

fn view_target_queries(app: &App) -> usize {
    TARGET_QUERIES.with(|count| count.set(0));
    drop(app.view());
    TARGET_QUERIES.with(std::cell::Cell::get)
}

#[test]
fn actual_palette_views_share_one_fresh_target_across_rows_and_detail() {
    let mut app = app();
    let (a, b) = ethane(&mut app);
    app.tab.selected = vec![b, a];
    let document = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let camera = (app.tab.camera.center, app.tab.camera.zoom);
    let revision = app.tab.revision;
    assert_eq!(view_target_queries(&app), 0, "closed palette");

    open(&mut app);
    assert!(app.commands.results.len() > 20, "many actual catalog rows");
    assert_eq!(view_target_queries(&app), 1, "all rows and current detail");
    for query in ["bond", "copy", "scale 150"] {
        let _ = app.update(Message::Commands(Action::Query(query.into())));
        assert_eq!(view_target_queries(&app), 1, "{query}");
        let _ = app.update(Message::Commands(Action::Move(1)));
        assert_eq!(view_target_queries(&app), 1, "moving in {query}");
    }
    let _ = app.update(Message::Commands(Action::Filter(Some(Category::Bond))));
    assert_eq!(view_target_queries(&app), 1, "filtered rows");
    let _ = app.update(Message::Commands(Action::Close));
    assert_eq!(view_target_queries(&app), 0, "closed again");
    assert_eq!(app.tab.doc, document);
    assert_eq!(app.tab.selected, selected);
    assert_eq!((app.tab.camera.center, app.tab.camera.zoom), camera);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
}

#[test]
fn palette_execution_rechecks_the_current_selection_after_the_view_was_built() {
    for replaced in [false, true] {
        let mut app = app();
        let (a, b) = ethane(&mut app);
        app.tab.selected = vec![a];
        search(&mut app, "increase charge");
        let index = row(&app, "Increase charge");
        assert_eq!(view_target_queries(&app), 1);
        let target = if replaced {
            app.tab.file_epoch = app.tab.file_epoch.saturating_add(1);
            app.tab.doc.add_atom("O", Point::new(200., 0.))
        } else {
            b
        };
        app.tab.selected = vec![target];
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Commands(Action::Run(index)));
        assert_eq!(app.tab.doc.atom(a).unwrap().charge, 0);
        assert_eq!(app.tab.doc.atom(target).unwrap().charge, 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before, "one current-target edit");
    }

    let mut app = app();
    let (a, _) = ethane(&mut app);
    app.tab.selected = vec![a];
    search(&mut app, "increase charge");
    let index = row(&app, "Increase charge");
    assert_eq!(view_target_queries(&app), 1);
    app.tab.selected = vec![u64::MAX];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Commands(Action::Run(index)));
    assert!(app.error, "a stale available row is refused");
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn blocked_3d_scale_offers_an_explicit_2d_recovery_and_two_undo_steps() {
    let mut app = app();
    let (doc, molecule, _) = crate::canvas::tilt::shown(reshiki::models3d::Display::ThreeD);
    app.tab.doc = doc;
    app.tab.selected = molecule.to_vec();
    let original = app.tab.doc.clone();
    search(&mut app, "scale 150");
    let ranked = app.commands.ranked();
    assert_eq!(ranked[0].name, "Scale selection to 150%");
    assert_eq!(ranked[1].name, "Show as 2D drawing");
    assert!(app.command_unavailable(ranked[0]).is_some());
    assert!(app.command_unavailable(ranked[1]).is_none());
    let _ = app.update(Message::Commands(Action::Move(1)));
    let _ = app.update(Message::Commands(Action::Submit));
    assert!(!app.commands.open);
    assert_eq!(
        app.tab.doc.models[0].display,
        reshiki::models3d::Display::TwoD
    );
    let two_d = app.tab.doc.clone();
    let (lo, hi) = reshiki::scene::selection_bounds(&two_d, &molecule).unwrap();
    run(&mut app, "scale 150");
    assert!(!app.error, "{}", app.status);
    let scaled = app.tab.doc.clone();
    let (scaled_lo, scaled_hi) = reshiki::scene::selection_bounds(&scaled, &molecule).unwrap();
    // Atom positions, rather than fixed-size label ink, follow the scale.
    let distance = |doc: &Document| {
        doc.atom(molecule[0])
            .unwrap()
            .position
            .distance(doc.atom(molecule[1]).unwrap().position)
    };
    assert!((distance(&scaled) / distance(&two_d) - 1.5).abs() < 1e-4);
    assert!(scaled_hi.x - scaled_lo.x > hi.x - lo.x);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, two_d);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());

    // An ordinary selection or an unrelated unavailable numeric command does
    // not acquire a model-switch suggestion.
    app.tab.doc.models.clear();
    search(&mut app, "scale 150");
    assert!(
        !app.commands
            .ranked()
            .iter()
            .any(|row| row.name == "Show as 2D drawing")
    );
    app.tab.selected.clear();
    search(&mut app, "scale 150");
    assert!(
        !app.commands
            .ranked()
            .iter()
            .any(|row| row.name == "Show as 2D drawing")
    );
}

#[test]
fn blocked_3d_flips_offer_2d_recovery_and_mixed_selections_stay_available() {
    use reshiki::models3d::Display;

    for (query, expected) in [
        ("flip horizontal", "Flip horizontally"),
        ("flip vertical", "Flip vertically"),
    ] {
        let mut app = app();
        let (doc, molecule, drawn) = crate::canvas::tilt::shown(Display::ThreeD);
        app.tab.doc = doc;
        app.tab.selected = molecule.to_vec();
        let before = app.tab.doc.clone();
        search(&mut app, query);
        let ranked = app.commands.ranked();
        assert_eq!(ranked[0].name, expected);
        assert_eq!(
            app.command_unavailable(ranked[0]),
            Some("Switch to 2D to flip the drawing")
        );
        assert_eq!(ranked[1].name, "Show as 2D drawing");
        assert!(app.command_unavailable(ranked[1]).is_none());
        let _ = app.update(Message::Commands(Action::Submit));
        assert!(app.commands.open && app.error);
        assert_eq!(app.status, "Switch to 2D to flip the drawing");
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, molecule);
        assert!(!app.tab.history.can_undo());

        let _ = app.update(Message::Commands(Action::Move(1)));
        let _ = app.update(Message::Commands(Action::Submit));
        assert!(!app.commands.open);
        assert_eq!(app.tab.doc.models[0].display, Display::TwoD);
        let two_d = app.tab.doc.clone();
        run(&mut app, query);
        assert!(!app.commands.open);
        assert_ne!(app.tab.doc, two_d);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, two_d);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());

        app.tab.selected = [molecule, drawn].concat();
        search(&mut app, query);
        assert_eq!(top(&app).name, expected);
        assert!(app.command_unavailable(top(&app)).is_none());
        assert!(
            !app.commands
                .ranked()
                .iter()
                .any(|row| row.name == "Show as 2D drawing")
        );
    }
}

fn ethane(app: &mut App) -> (u64, u64) {
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    (a, b)
}

#[test]
fn target_hint_names_the_pointed_atom_bond_selection_and_empty_canvas() {
    let mut app = app();
    let (a, b) = ethane(&mut app);
    app.tab.keyboard_drawing.leave();
    app.tab.hover = Some((Point::default(), app.tab.file_epoch));
    assert_eq!(
        app.command_target_hint(&app.command_target()),
        format!("Acts on C #{a} (pointed)")
    );
    app.tab.hover = Some((Point::new(21., 0.), app.tab.file_epoch));
    assert_eq!(
        app.command_target_hint(&app.command_target()),
        format!("Acts on bond {a}–{b} (pointed)")
    );
    app.tab.selected = vec![a, b];
    assert_eq!(
        app.command_target_hint(&app.command_target()),
        "Acts on 2 selected objects"
    );
    app.tab.selected = vec![a];
    assert_eq!(
        app.command_target_hint(&app.command_target()),
        "Acts on 1 selected object"
    );
    app.tab.selected.clear();
    app.tab.hover = None;
    assert_eq!(
        app.command_target_hint(&app.command_target()),
        "Choose a command to run"
    );
}

#[test]
fn palette_templates_finish_after_one_placement_even_with_keep_placing() {
    let mut app = app();
    app.templates.repeat = true;
    run(&mut app, "thiazole");
    assert_eq!(app.tab.doc.atoms.len(), 5, "{}", app.status);
    assert_eq!(app.tab.selected.len(), 5);
    assert_eq!(app.tool, Tool::Select);
    assert!(!app.templates.active);
    assert!(app.templates.repeat);
    assert!(!app.error, "{}", app.status);
    let placed = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(300., 300.))));
    assert_eq!(app.tab.doc, placed, "a deselecting click adds no copy");
}

#[test]
fn refused_palette_template_clears_placement_and_keeps_its_error() {
    let mut app = app();
    app.templates.repeat = false;
    app.templates.connection = reshiki::templates::Connection::Connect;
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let ring = app.tab.doc.atoms[0].id;
    let a = app.tab.doc.add_atom("C", Point::new(84., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(126., 0.));
    app.tab.doc.add_bond(ring, a, 1, "plain");
    app.tab.doc.add_bond(a, b, 1, "plain");
    let bond = &app.tab.doc.bonds[0];
    let a = app.tab.doc.atom(bond.a).unwrap().position;
    let b = app.tab.doc.atom(bond.b).unwrap().position;
    app.tab.keyboard_drawing.leave();
    app.tab.hover = Some((
        Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.),
        app.tab.file_epoch,
    ));
    let before = app.tab.doc.clone();
    run(&mut app, "pyridine");
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
    assert_eq!(
        app.status,
        "Connect with a bond: choose a template atom and point to a drawing atom."
    );
    assert_eq!(app.tool, Tool::Select);
    assert!(!app.templates.active);
    assert!(!app.templates.repeat);
    let _ = app.update(Message::Canvas(Edit::Click(Point::new(300., 300.))));
    assert_eq!(
        app.tab.doc, before,
        "a refused template leaves nothing armed"
    );
}

#[test]
fn keyboard_moves_scroll_only_when_the_highlight_leaves_the_viewport() {
    let mut app = app();
    search(&mut app, "ring");
    // Four whole rows, including their gaps; use the measured viewport height
    // rather than a count inferred from the dialog's maximum height.
    app.commands.viewport_height = 3. * (ROW_HEIGHT + ROW_SPACING) + ROW_HEIGHT;
    for _ in 0..3 {
        let _ = app.update(Message::Commands(Action::Move(1)));
        assert_eq!(app.commands.scroll_offset, 0.);
    }
    let _ = app.update(Message::Commands(Action::Move(1)));
    assert_eq!(app.commands.scroll_offset, ROW_HEIGHT + ROW_SPACING);
    let bottom = 4. * (ROW_HEIGHT + ROW_SPACING) + ROW_HEIGHT;
    assert_eq!(
        app.commands.scroll_offset + app.commands.viewport_height,
        bottom
    );
    let _ = app.update(Message::Commands(Action::Move(isize::MAX)));
    let last = app.commands.results.len() - 1;
    assert_eq!(
        app.commands.scroll_offset + app.commands.viewport_height,
        last as f32 * (ROW_HEIGHT + ROW_SPACING) + ROW_HEIGHT
    );
    let _ = app.update(Message::Commands(Action::Move(-PAGE)));
    assert_eq!(
        app.commands.scroll_offset,
        app.commands.selected as f32 * (ROW_HEIGHT + ROW_SPACING)
    );
    let _ = app.update(Message::Commands(Action::Move(isize::MIN)));
    assert_eq!(app.commands.scroll_offset, 0.);
    assert_eq!(row_scroll_offset(0, 0., 31.), 0.);
    assert_eq!(row_scroll_offset(1, 0., 31.), 27.);
    let _ = app.update(Message::Commands(Action::Move(PAGE)));
    let _ = app.update(Message::Commands(Action::Query("bond".into())));
    assert_eq!(
        app.commands.scroll_offset, 0.,
        "a new query returns to the top"
    );
    let _ = app.update(Message::Commands(Action::Move(PAGE)));
    let _ = app.update(Message::Commands(Action::Filter(Some(Category::Bond))));
    assert_eq!(
        app.commands.scroll_offset, 0.,
        "a filter returns to the top"
    );
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
        locants: Default::default(),
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

#[test]
fn groups_are_found_by_their_catalog_category() {
    let mut app = app();
    search(&mut app, "protecting group");
    let ranked: Vec<_> = app
        .commands
        .ranked()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    for label in ["Boc", "Cbz", "Fmoc"] {
        assert!(ranked.contains(&label), "{label} in {ranked:?}");
    }
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
fn hydroxy_finds_oxygen_only_for_a_single_terminal_atom() {
    let mut app = app();
    // Toluene (Cc1ccccc1): a methyl carbon attached to a benzene ring.
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let ring = app.tab.doc.atoms[0].id;
    let methyl = app.tab.doc.add_atom("C", Point::new(84., 0.));
    app.tab.doc.add_bond(ring, methyl, 1, "plain");

    app.tab.selected = vec![methyl];
    for query in ["hydroxy", "hydroxyl"] {
        search(&mut app, query);
        assert_eq!(top(&app).name, "Oxygen (O)", "{query}");
    }
    app.tab.selected = vec![ring];
    search(&mut app, "hydroxy");
    assert!(
        !app.commands
            .ranked()
            .iter()
            .any(|command| command.name == "Oxygen (O)")
    );
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

#[tokio::test]
async fn chemical_copy_rows_prepare_the_selection_or_whole_drawing() {
    use iced::futures::StreamExt;
    use reshiki::clipboard::CopyFormat;

    for (query, format, modifiers, key) in [
        ("Copy SMILES", CopyFormat::Smiles, Modifiers::ALT, "C"),
        ("Copy CDXML", CopyFormat::Cdxml, Modifiers::empty(), "D"),
        ("Copy MOL", CopyFormat::Mol, Modifiers::ALT, "O"),
    ] {
        for selected in [false, true] {
            let mut app = app();
            let (a, b) = ethane(&mut app);
            app.tab.doc.add_atom("O", Point::new(240., 0.));
            if selected {
                app.tab.selected = vec![a, b];
            }
            let before = app.tab.doc.clone();
            let selection = app.tab.selected.clone();
            search(&mut app, query);
            assert_eq!(top(&app).name, query);
            assert_eq!(
                top(&app).keys,
                crate::app::shortcuts::keys(Modifiers::COMMAND | modifiers, key)
            );
            assert!(app.command_unavailable(top(&app)).is_none());
            let task = app.update(Message::Commands(Action::Submit));
            assert!(!app.commands.open && !app.error, "{query}: {}", app.status);
            assert!(app.copy_as_busy && app.tab.clipboard_busy);
            assert_eq!(
                app.status,
                format!("Preparing {} for the clipboard…", format.label())
            );
            let mut stream = iced_runtime::task::into_stream(task).unwrap();
            let Some(iced_runtime::Action::Output(Message::Tab(tab, message))) =
                stream.next().await
            else {
                panic!("{query} keeps preparation with its drawing");
            };
            assert_eq!(tab, app.tab.id);
            let Message::CopyAsPrepared(copy_key, result) = *message else {
                panic!("{query} prepares chemical text");
            };
            let copy = result.unwrap();
            assert_eq!(copy_key.format, format);
            assert_eq!(copy_key.selected, selection);
            let text = copy.text().unwrap();
            let atoms = match format {
                CopyFormat::Smiles => reshiki::chemistry::smiles::parse(text)
                    .unwrap()
                    .graph
                    .atoms
                    .len(),
                CopyFormat::Mol => reshiki::chemistry::molfile::read(text)
                    .unwrap()
                    .molecule
                    .state
                    .graph
                    .atoms
                    .len(),
                CopyFormat::Cdxml => reshiki::chemistry::cdxml::read(text)
                    .unwrap()
                    .fragments
                    .iter()
                    .map(|fragment| fragment.graph.atoms.len())
                    .sum(),
                _ => unreachable!(),
            };
            assert_eq!(atoms, if selected { 2 } else { 3 }, "{query}");
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.selected, selection);
            assert!(!app.tab.history.can_undo());
        }
    }
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
            for (query, name, hotspot) in [
                ("increase charge", "Increase charge", Hotspot::Atom(a)),
                ("radical", "Radical", Hotspot::Atom(a)),
                (
                    "reverse bond direction",
                    "Reverse bond direction",
                    Hotspot::Bond(a, b),
                ),
            ] {
                app.tab.selected = selected.clone();
                app.tab.hover = Some((hotspot.point(&app.tab.doc).unwrap(), app.tab.file_epoch));
                app.tab.keyboard_drawing.set_target(hotspot, &app.tab.doc);
                search(&mut app, query);
                assert_eq!(app.keyboard_target(), keyboard);
                // Unavailable commands rank below available ones, so pick it by name.
                let index = row(&app, name);
                let command = app.commands.items[index].clone();
                let reason = app.command_unavailable(&command).expect(query);
                let _ = app.update(Message::Commands(Action::Run(index)));
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
        ("w out.cdxml", "Save as out.cdxml.rsk…"),
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
fn available_recents_lead_blocked_recents_on_an_empty_query() {
    let mut app = app();
    let (doc, molecule, _) = crate::canvas::tilt::shown(reshiki::models3d::Display::TwoD);
    app.tab.doc = doc;
    app.tab.selected = molecule.to_vec();
    run(&mut app, "lasso");
    run(&mut app, "zoom in");
    run(&mut app, "Molecular formula and properties");
    app.tab.busy = false;
    run(&mut app, "Show as 2D and 3D");
    app.tab.selected.clear();
    open(&mut app);

    let ranked = app.commands.ranked();
    assert_eq!(
        ranked
            .iter()
            .take(4)
            .map(|command| command.name.as_str())
            .collect::<Vec<_>>(),
        [
            "Molecular formula and properties",
            "Zoom in",
            "Lasso tool",
            "Show as 2D and 3D",
        ]
    );
    assert!(app.command_unavailable(ranked[0]).is_none());
    assert!(app.command_unavailable(ranked[3]).is_some());
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
    assert!(matches!(
        catalog::parameterised(&app, "chain 513").unwrap().need,
        Need::Blocked(_)
    ));
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

#[test]
fn tab_cycles_the_category_filter_and_a_new_query_resets_it() {
    assert!(matches!(
        modal_key(&Key::Named(Named::Tab), Modifiers::empty()),
        Some(Message::Commands(Action::Cycle(1)))
    ));
    assert!(matches!(
        modal_key(&Key::Named(Named::Tab), Modifiers::SHIFT),
        Some(Message::Commands(Action::Cycle(-1)))
    ));
    let mut app = app();
    search(&mut app, "bond");
    let categories: Vec<_> = app
        .commands
        .categories()
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert!(categories.len() > 1, "{categories:?}");
    let all = app.commands.results.len();
    let cycle = |app: &mut App, step| {
        let _ = app.update(Message::Commands(Action::Cycle(step)));
        app.commands.filter
    };
    for &category in &categories {
        assert_eq!(cycle(&mut app, 1), Some(category));
        assert!(app.commands.ranked().iter().all(|c| c.category == category));
        assert_eq!(app.commands.selected, 0);
    }
    assert_eq!(cycle(&mut app, 1), None, "Tab wraps back to All");
    assert_eq!(app.commands.results.len(), all);
    assert_eq!(
        cycle(&mut app, -1),
        categories.last().copied(),
        "Shift+Tab wraps"
    );
    // A chip filters directly; a query without that category shows all again.
    let _ = app.update(Message::Commands(Action::Filter(Some(Category::Bond))));
    assert_eq!(top(&app).category, Category::Bond);
    let _ = app.update(Message::Commands(Action::Query("double bond".into())));
    assert_eq!(
        app.commands.filter,
        Some(Category::Bond),
        "still has bond hits"
    );
    let _ = app.update(Message::Commands(Action::Query("zoom in".into())));
    assert_eq!(app.commands.filter, None);
    assert_eq!(top(&app).name, "Zoom in");
    // Opening again starts unfiltered.
    let _ = app.update(Message::Commands(Action::Filter(Some(Category::View))));
    assert_eq!(app.commands.filter, Some(Category::View));
    open(&mut app);
    assert_eq!(app.commands.filter, None);
}

#[test]
fn unavailable_commands_rank_after_available_ones() {
    let mut app = app();
    for query in ["copy", "charge", "bond", "select", "nmr"] {
        search(&mut app, query);
        let state = &app.commands;
        let blocked: Vec<_> = state
            .results
            .iter()
            .map(|i| app.command_unavailable(&state.items[*i]).is_some())
            .collect();
        assert!(blocked.contains(&true), "{query}: some cannot run");
        assert!(blocked.is_sorted(), "{query}: available first");
    }
    // An exact alias still ranks first, to say why it cannot run.
    search(&mut app, "noh");
    assert_eq!(top(&app).name, "Clear highlights");
    // So does a recent command on an empty query.
    let (a, b) = ethane(&mut app);
    app.tab.selected = vec![a, b];
    run(&mut app, "copy");
    assert!(!app.commands.open, "copy ran");
    app.tab.selected.clear();
    open(&mut app);
    assert_eq!(top(&app).name, "Copy");
    // Clicking a row that cannot run selects it, to show why.
    search(&mut app, "copy");
    let copy = row(&app, "Copy");
    let _ = app.update(Message::Commands(Action::Run(copy)));
    assert_eq!(top(&app).name, "Copy");
}

#[test]
fn name_matches_outrank_synonym_and_description_matches() {
    let mut app = app();
    open(&mut app);
    let command =
        |name: &str, synonyms| Command::new(name, Category::Edit, "", synonyms, Run::Send(vec![]));
    // A whole synonym scores above a name prefix, but the name ranks first.
    let items = vec![command("Alpha", "wedge"), command("Big wedges", "")];
    assert!(search::score(&items[0], "wedge") > search::score(&items[1], "wedge"));
    let state = &mut app.commands;
    state.catalog = items.len();
    state.items = items;
    let _ = app.update(Message::Commands(Action::Query("wedge".into())));
    let ranked: Vec<_> = app
        .commands
        .ranked()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(ranked[..2], ["Big wedges", "Alpha"]);
    // A shortcut key typed exactly ranks with the name matches.
    search(&mut app, "v");
    assert_eq!(top(&app).name, "Select tool");
}

#[test]
fn page_home_and_end_move_the_selection() {
    for (key, step) in [
        (Named::PageDown, PAGE),
        (Named::PageUp, -PAGE),
        (Named::Home, isize::MIN),
        (Named::End, isize::MAX),
    ] {
        assert!(matches!(
            modal_key(&Key::Named(key), Modifiers::empty()),
            Some(Message::Commands(Action::Move(s))) if s == step
        ));
    }
    let mut app = app();
    search(&mut app, "ring");
    let last = app.commands.results.len() - 1;
    assert!(last > 2 * PAGE as usize);
    let mut step = |step| {
        let _ = app.update(Message::Commands(Action::Move(step)));
        app.commands.selected
    };
    assert_eq!(step(PAGE), PAGE as usize);
    assert_eq!(step(PAGE), 2 * PAGE as usize);
    assert_eq!(step(-PAGE), PAGE as usize);
    assert_eq!(step(isize::MAX), last);
    assert_eq!(step(PAGE), last, "stops at the end");
    assert_eq!(step(isize::MIN), 0);
    assert_eq!(step(-PAGE), 0, "stops at the start");
}

#[test]
fn matched_letters_are_emphasised() {
    let marks = |name, query| -> String {
        matched(name, query)
            .into_iter()
            .map(|m| if m { '^' } else { ' ' })
            .collect()
    };
    assert_eq!(marks("Double bond", "bond"), "       ^^^^");
    assert_eq!(marks("Double bond", "do bo"), "^^     ^^  ");
    assert_eq!(marks("Thiophene", "phen"), "    ^^^^ ");
    assert_eq!(marks("Solid wedge bond", "wdg"), "      ^ ^^      ");
    assert_eq!(marks("Ångström Ölefin", "öle"), "         ^^^   ");
    assert_eq!(marks("Benzene", "x"), "       ");
    assert_eq!(marks("Invert selection", "inverse"), " ".repeat(16));
    let spans = name_spans("Double bond", "bond");
    let text: Vec<_> = spans.iter().map(|s| s.text.as_ref()).collect();
    assert_eq!(text, ["Double ", "bond"]);
    assert!(spans[0].font.is_none() && spans[1].font.is_some());
    // A match inside a word is split off with a word joiner.
    let spans = name_spans("Bring bond forward", "ring");
    let text: Vec<_> = spans.iter().map(|s| s.text.as_ref()).collect();
    assert_eq!(text, ["B\u{2060}", "ring", " bond forward"]);
}

/// The document after `query` runs on a selected benzene, and after `message`
/// does the same directly; each palette run is one Undo step.
fn ran_as(query: &str, message: Message) -> (Document, Document) {
    let selected = |app: &mut App| {
        app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
        app.tab.selected = app.tab.doc.all_ids();
        app.sync_numeric_transforms();
    };
    let mut app = app();
    selected(&mut app);
    let before = app.tab.doc.clone();
    run(&mut app, query);
    assert!(!app.commands.open, "{query} ran");
    let after = app.tab.doc.clone();
    assert_ne!(after, before, "{query} changed the drawing");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before, "{query} is one undo step");
    let mut direct = self::app();
    selected(&mut direct);
    let _ = direct.update(message);
    (after, direct.tab.doc)
}

#[test]
fn command_word_prefixes_take_a_value() {
    use reshiki::editing::Transform;
    for (query, message) in [
        ("rot 30", Message::Transform(Transform::Rotate(30.))),
        ("rotate ccw 30", Message::Transform(Transform::Rotate(-30.))),
        ("rotate cw 30", Message::Transform(Transform::Rotate(30.))),
        ("tilt up 20", Message::Transform(Transform::TiltX(-20.))),
        ("tilt down 20", Message::Transform(Transform::TiltX(20.))),
        ("tilt left 20", Message::Transform(Transform::TiltY(20.))),
        ("ti ri 20", Message::Transform(Transform::TiltY(-20.))),
        ("tilt x -15", Message::Transform(Transform::TiltX(-15.))),
        ("tilt y 15", Message::Transform(Transform::TiltY(15.))),
    ] {
        let (palette, direct) = ran_as(query, message);
        assert_eq!(palette, direct, "{query}");
    }
    // Scale goes through the Scale field: relative and uniform.
    use crate::app::numeric_transforms::{Action as Numeric, Field};
    let (palette, direct) = ran_as(
        "scale 150",
        Message::NumericTransform(Numeric::Input(Field::Scale, "150".into())),
    );
    let width = |doc: &Document| {
        let (lo, hi) = reshiki::scene::selection_bounds(doc, &doc.all_ids()).unwrap();
        hi.x - lo.x
    };
    assert!((width(&palette) / width(&direct) - 1.5).abs() < 1e-3);

    // With nothing selected, the row still ranks first, greyed, instead of a
    // chemical name.
    let mut app = app();
    for query in ["rot 30", "tilt up 20", "scale 150"] {
        search(&mut app, query);
        assert!(app.command_unavailable(top(&app)).is_some(), "{query}");
        assert!(
            !app.commands
                .ranked()
                .iter()
                .any(|c| matches!(c.run, Run::Name(_))),
            "{query}"
        );
    }
    // Scale greys out, with the field's reason, for a molecule shown only in 3D.
    let (doc, molecule, _) = crate::canvas::tilt::shown(reshiki::models3d::Display::ThreeD);
    app.tab.doc = doc;
    app.tab.selected = molecule.to_vec();
    search(&mut app, "scale 150");
    assert_eq!(
        app.command_unavailable(top(&app)),
        Some(crate::app::numeric_transforms::RESIZE_SHOWN_IN_3D)
    );
    let mut app = self::app();
    let named = |app: &App, query| catalog::parameterised(app, query).map(|c| c.name);
    assert_eq!(named(&app, "tilt up 20").unwrap(), "Tilt selection up 20°");
    assert_eq!(named(&app, "zo 150").unwrap(), "Zoom to 150%");
    assert_eq!(named(&app, "ri 7").unwrap(), "Insert 7-membered ring");
    assert_eq!(named(&app, "el pd").unwrap(), "Element Pd");
    assert_eq!(named(&app, "font 12").unwrap(), "Font size 12 pt");
    for query in ["bond length 14", "bond len 14"] {
        assert_eq!(named(&app, query).unwrap(), "New bond length 14 pt");
        assert_eq!(
            catalog::parameterised(&app, query).unwrap().detail,
            "For bonds you draw next; use scale to resize the drawing"
        );
    }
    assert_eq!(named(&app, "chain 5").unwrap(), "Chain tool with 5 atoms");
    // One letter is too short to pick a command word.
    assert!(named(&app, "r 30").is_none());
    run(&mut app, "zo 150");
    assert!((app.tab.camera.zoom - 1.5).abs() < 1e-4);
    // "ch" is charge, whose value fits, and its row ranks first.
    let (a, _) = ethane(&mut app);
    app.tab.selected = vec![a];
    search(&mut app, "ch -1");
    assert_eq!(top(&app).name, "Set charge to -1");
    let _ = app.update(Message::Commands(Action::Submit));
    assert_eq!(app.tab.doc.atom(a).unwrap().charge, -1);
}

#[test]
fn out_of_range_font_sizes_stay_first_and_leave_the_document_and_input_untouched() {
    let mut app = app();
    let id = app.tab.doc.next_id();
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id,
        position: Point::default(),
        text: "Caption".into(),
        format: Default::default(),
    });
    app.tab.selected = vec![id];
    let before = app.tab.doc.clone();
    let input = app.tab.font_size_input.clone();
    for size in [-1, 0, 3, 145] {
        search(&mut app, &format!("font size {size}"));
        assert_eq!(top(&app).name, format!("Font size {size} pt"));
        assert_eq!(
            app.command_unavailable(top(&app)),
            Some("Enter a font size from 4 to 144 pt")
        );
        assert!(
            !app.commands
                .ranked()
                .iter()
                .any(|row| matches!(row.run, Run::Name(_)))
        );
        let _ = app.update(Message::Commands(Action::Submit));
        assert!(app.commands.open && app.error);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.font_size_input, input);
        assert!(!app.tab.history.can_undo());
    }
    for size in [4, 12, 144] {
        search(&mut app, &format!("font size {size}"));
        assert!(app.command_unavailable(top(&app)).is_none());
    }
    run(&mut app, "font size 12");
    assert!(!app.commands.open);
    assert_eq!(app.tab.doc.annotations[0].format.style.size_pt, 12.);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn blocked_value_rows_keep_an_active_caption_draft_uncommitted() {
    use crate::app::inline_text;
    use iced::widget::text_editor::{Action as Input, Edit};

    let mut app = app();
    let _ = app.update(Message::InlineText(inline_text::Action::Begin(
        None,
        Point::default(),
    )));
    let _ = app.update(Message::CaptionAction(Input::Edit(Edit::Paste(
        std::sync::Arc::new("Caption".into()),
    ))));
    let before = app.tab.doc.clone();
    let input = app.tab.font_size_input.clone();
    for query in ["font size 3", "ring 2", "chain 600"] {
        search(&mut app, query);
        let index = app.commands.results[0];
        for action in [Action::Submit, Action::Run(index)] {
            let _ = app.update(Message::Commands(action));
            assert!(app.commands.open && app.error);
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.font_size_input, input);
            assert!(app.tab.inline_text.is_some());
            assert_eq!(app.tab.caption, "Caption");
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn out_of_range_ring_and_chain_sizes_stay_first_and_do_nothing() {
    let mut app = app();
    let before = app.tab.doc.clone();
    let tool = app.tool;
    let input = app.tab.chain_atoms_input.clone();
    let chain_reason = format!("Chains have 1 to {} atoms", reshiki::chains::MAX_ATOMS);
    for (query, name, reason) in [
        (
            "ring 2",
            "Insert 2-membered ring",
            "Rings have 3 to 8 atoms",
        ),
        (
            "ring 9",
            "Insert 9-membered ring",
            "Rings have 3 to 8 atoms",
        ),
        (
            "ring 3.5",
            "Insert 3.5-membered ring",
            "Rings have 3 to 8 atoms",
        ),
        ("chain 0", "Chain tool with 0 atoms", chain_reason.as_str()),
        (
            "chain 600",
            "Chain tool with 600 atoms",
            chain_reason.as_str(),
        ),
    ] {
        search(&mut app, query);
        assert_eq!(top(&app).name, name);
        assert_eq!(app.command_unavailable(top(&app)), Some(reason));
        assert!(
            !app.commands
                .ranked()
                .iter()
                .any(|row| matches!(row.run, Run::Name(_)))
        );
        let _ = app.update(Message::Commands(Action::Submit));
        assert!(app.commands.open && app.error);
        assert_eq!(app.status, reason);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tool, tool);
        assert_eq!(app.tab.chain_atoms_input, input);
        assert!(!app.tab.history.can_undo());
    }
    run(&mut app, "ring 7");
    assert!(!app.commands.open);
    assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 7);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    run(&mut app, "chain 5");
    assert!(!app.commands.open);
    assert_eq!(app.tool, Tool::Chain(reshiki::chains::ChainMode::Straight));
    assert_eq!(app.tab.chain_drawing.atoms, Some(5));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn full_turn_commands_preserve_the_document_and_redo() {
    let mut app = app();
    let (a, b) = ethane(&mut app);
    app.tab.selected = vec![a, b];
    let before = app.tab.doc.clone();
    run(&mut app, "rot 30");
    let rotated = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.history.can_redo());

    for query in ["rot 360", "rot -360", "rot 720", "rot -720", "rot 0"] {
        let revision = app.tab.revision;
        run(&mut app, query);
        assert_eq!(app.tab.doc, before, "{query}");
        assert_eq!(app.tab.revision, revision, "{query}");
        assert!(!app.tab.history.can_undo(), "{query}");
        assert!(app.tab.history.can_redo(), "{query}");
        assert_eq!(app.status, "No transform needed", "{query}");
    }
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, rotated);
}

#[test]
fn ring_presets_have_tools_and_loose_words_find_them() {
    let mut app = app();
    for query in [
        "ben to",
        "benzene tool",
        "bnz tool",
        "benzne tool",
        "to ben",
    ] {
        search(&mut app, query);
        assert_eq!(top(&app).name, "Benzene tool", "{query}");
    }
    // Letters may span words; the Bt group still ranks first for its name.
    search(&mut app, "bt");
    assert_eq!(top(&app).name, "Bt");
    row(&app, "Benzene tool");
    run(&mut app, "ben to");
    assert_eq!(app.tool, Tool::RingPreset(reshiki::rings::Preset::Benzene));
    for (query, preset) in [
        (
            "cyclopentadiene tool",
            reshiki::rings::Preset::Cyclopentadiene,
        ),
        ("chair up tool", reshiki::rings::Preset::ChairUp),
        ("chair down tool", reshiki::rings::Preset::ChairDown),
        ("haworth five tool", reshiki::rings::Preset::HaworthFive),
        ("haworth six tool", reshiki::rings::Preset::HaworthSix),
    ] {
        run(&mut app, query);
        assert_eq!(app.tool, Tool::RingPreset(preset), "{query}");
    }
    // Short words and abbreviations find their rows.
    search(&mut app, "rotate ccw");
    assert!(top(&app).name.ends_with("counterclockwise"));
    // Exact queries keep their first result.
    for (query, name) in [
        ("ring", "Ring tool"),
        ("v", "Select tool"),
        ("benzene", "Benzene"),
        ("lasso", "Lasso tool"),
    ] {
        search(&mut app, query);
        assert_eq!(top(&app).name, name, "{query}");
    }
}

#[test]
fn named_file_aliases_keep_the_complete_suggested_native_name() {
    let mut app = app();
    for alias in ["w", "sav", "saveas"] {
        for (typed, expected) in [
            ("Foo", "Foo.rsk"),
            ("save", "save.rsk"),
            ("file", "file.rsk"),
            ("My Drawing", "My Drawing.rsk"),
            ("Foo.RSK", "Foo.RSK"),
            ("Legacy.reshiki", "Legacy.reshiki"),
            ("Legacy.MORUNO", "Legacy.MORUNO"),
            ("Foo.cdxml", "Foo.cdxml.rsk"),
        ] {
            search(&mut app, &format!(":{alias} {typed}"));
            assert!(
                matches!(&top(&app).run, Run::SaveAs(name) if name == expected),
                "{alias} {typed}: {:?}",
                top(&app).run
            );
            assert!(top(&app).detail.contains(expected));
        }
    }
    for alias in ["e", "edit"] {
        for (typed, expected) in [
            ("My Drawing.RSK", "My Drawing.RSK"),
            ("drawing", "drawing.rsk"),
        ] {
            search(&mut app, &format!(":{alias} {typed}"));
            assert!(matches!(&top(&app).run, Run::OpenNamed(name) if name == expected));
        }
    }
    search(&mut app, "w foo");
    assert!(matches!(&top(&app).run, Run::SaveAs(name) if name == "foo.rsk"));
    let task = app.update(Message::Commands(Action::Submit));
    assert!(!app.commands.open);
    assert!(app.file_io.saving, "the named command requests a save");
    assert!(
        task.units() > 0,
        "the Save As dialog is requested, not polled"
    );
}

#[test]
fn unnamed_file_aliases_keep_the_existing_save_and_open_actions() {
    let mut app = app();
    app.tab.path = Some("My Drawing.rsk".into());
    for alias in ["w", "sav", "saveas", "e", "edit"] {
        search(&mut app, alias);
        let Run::Send(messages) = &top(&app).run else {
            panic!("An unnamed file alias uses the existing action: {alias}");
        };
        let matches = match alias {
            "w" => matches!(messages.as_slice(), [Message::Save]),
            "sav" | "saveas" => matches!(messages.as_slice(), [Message::SaveAs]),
            _ => matches!(messages.as_slice(), [Message::Open]),
        };
        assert!(matches, "{alias}");
    }
    assert_eq!(
        app.drawing_save_target(true),
        (None, "My Drawing.rsk".into())
    );
    assert_eq!(
        app.drawing_save_target(false),
        (Some("My Drawing.rsk".into()), "My Drawing.rsk".into())
    );
    run(&mut app, "w");
    assert!(app.file_io.saving, "bare :w still requests Save");
}

#[test]
fn named_edit_without_a_saved_drawing_reports_its_missing_neighbor() {
    let mut app = app();
    ethane(&mut app);
    let before = app.tab.doc.clone();
    run(&mut app, ":edit Neighbor");
    assert!(!app.commands.open);
    assert_eq!(
        app.status,
        "Neighbor.rsk not found next to the current drawing"
    );
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.path.is_none());
    assert!(app.tabs.background.is_empty());
    assert_eq!(app.open_drawing_named("Neighbor".into()).units(), 0);
}

#[tokio::test]
async fn named_edit_opens_the_native_file_next_to_the_current_drawing() {
    let directory = tempfile::tempdir().unwrap();
    let current = directory.path().join("Current.rsk");
    for (alias, typed, filename) in [
        ("e", "Neighbor", "Neighbor.rsk"),
        ("edit", "My Drawing.RSK", "My Drawing.RSK"),
        ("e", "Legacy.moruno", "Legacy.moruno"),
    ] {
        let mut app = app();
        ethane(&mut app);
        app.tab.path = Some(current.clone());
        let before = app.tab.doc.clone();
        let neighbor = directory.path().join(filename);
        let mut document = Document::default();
        document.add_atom("O", Point::default());
        std::fs::write(&neighbor, document.file_json().unwrap()).unwrap();
        search(&mut app, &format!(":{alias} {typed}"));
        let task = app.update(Message::Commands(Action::Submit));
        assert!(!app.commands.open);
        let _ = app.update(file_completion(task).await);
        assert_eq!(app.tab.path, Some(neighbor));
        assert_eq!(app.tab.doc.atoms.len(), 1);
        assert_eq!(app.tab.doc.atoms[0].element, "O");
        assert!(!app.error);
        assert_eq!(app.tabs.background.len(), 1);
        assert_eq!(app.tabs.background[0].doc, before);
    }
}

async fn file_completion(task: Task<Message>) -> Message {
    use iced::futures::StreamExt;

    let mut stream = iced_runtime::task::into_stream(task).expect("Read the named drawing");
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(Message::Tab(id, message)) = action
            && matches!(message.as_ref(), Message::FilePrepared(_))
        {
            return Message::Tab(id, message);
        }
    }
    panic!("A named edit completes through the existing file-open path");
}

#[test]
fn missing_named_edit_reports_the_requested_name_without_opening_a_dialog() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = app();
    ethane(&mut app);
    let current = directory.path().join("Current.rsk");
    app.tab.path = Some(current.clone());
    let before = app.tab.doc.clone();
    search(&mut app, ":e Missing");
    assert_eq!(app.update(Message::Commands(Action::Submit)).units(), 0);
    assert_eq!(
        app.status,
        "Missing.rsk not found next to the current drawing"
    );
    assert!(app.error);
    assert_eq!(app.tab.path, Some(current));
    assert_eq!(app.tab.doc, before);
    assert!(app.tabs.background.is_empty());
}

#[test]
fn molecular_properties_palette_reports_the_current_scope_and_reopens_its_section() {
    use crate::app::{InspectorTab, inspector};

    for (scope, expected) in [
        ("all", "C2H8O · 48.085 g/mol"),
        ("molecule", "C2H6 · 30.070 g/mol"),
        ("fragment", "CH4 · 16.043 g/mol"),
        ("drawing", "C2H8O · 48.085 g/mol"),
    ] {
        let mut app = app();
        let (a, b) = ethane(&mut app);
        let oxygen = app.tab.doc.add_atom("O", Point::new(200., 0.));
        app.tab.selected = match scope {
            "all" => vec![a, b, oxygen],
            "molecule" => vec![a, b],
            "fragment" => vec![a],
            _ => vec![],
        };
        // With no selection, hovering a molecule still analyzes the whole drawing.
        app.tab.hover = Some((Point::default(), app.tab.file_epoch));
        app.inspector_tab = InspectorTab::Export;
        app.inspector_open = false;
        let _ = app.inspector_action(inspector::Action::Section(
            inspector::Section::Molecule,
            false,
        ));
        let document = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        search(&mut app, "Molecular formula and properties");
        let index = row(&app, "Molecular formula and properties");
        let task = app.update(Message::Commands(Action::Run(index)));

        assert_eq!(
            app.status,
            format!("Molecular properties · {expected}"),
            "{scope}"
        );
        assert!(
            !app.error && !app.tab.busy,
            "{scope}: properties are immediate"
        );
        assert!(app.inspector_open && app.inspector_tab == InspectorTab::Properties);
        assert_eq!(
            app.tab.inspector_ui.expanded(inspector::Section::Molecule),
            Some(true)
        );
        assert!(task.units() > 0, "{scope}: reveal the molecular section");
        assert!(!app.commands.open);
        assert_eq!(
            app.tab.doc, document,
            "{scope}: analysis preserves the drawing"
        );
        assert_eq!(
            app.tab.selected, selected,
            "{scope}: analysis preserves its scope"
        );
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn header_analyze_retains_its_structure_check_status() {
    use crate::app::{InspectorTab, Job};
    use reshiki::engine::Response;

    let mut app = app();
    let (a, _) = ethane(&mut app);
    app.tab.selected = vec![a];
    app.inspector_tab = InspectorTab::Reactions;
    let _ = app.update(Message::Analyze);
    assert!(app.tab.busy);
    assert_eq!(app.status, "Working…");
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: Job::Analyze,
        result: Box::new(Ok(Response {
            document: None,
            analysis: Some(crate::app::tests::completed_analysis()),
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        })),
    });
    assert_eq!(app.status, "No chemistry errors found");
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert!(!app.error && !app.tab.busy);
}

#[test]
fn automatic_mapping_palette_opens_the_review_and_reveals_a_ready_proposal() {
    use crate::app::{InspectorTab, inspector, reaction_mapping};
    use reshiki::{
        document::Arrow,
        reaction_mapping::{Budget, propose},
        reactions::{self, Role},
    };
    use std::sync::Arc;

    let mut app = app();
    let reactant = app.tab.doc.add_atom("C", Point::default());
    let product = app.tab.doc.add_atom("C", Point::new(250., 0.));
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(80., 0.),
        Point::new(180., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut app.tab.doc, arrow, &[reactant], Role::Reactant).unwrap();
    reactions::assign(&mut app.tab.doc, arrow, &[product], Role::Product).unwrap();
    let _ = app.inspector_action(inspector::Action::Section(
        inspector::Section::AtomMaps,
        false,
    ));
    app.inspector_open = false;
    app.inspector_tab = InspectorTab::Properties;
    let document = app.tab.doc.clone();
    search(&mut app, "Map atoms automatically");
    let index = row(&app, "Map atoms automatically");
    let _ = app.update(Message::Commands(Action::Run(index)));
    assert!(app.inspector_open && app.inspector_tab == InspectorTab::Reactions);
    assert_eq!(
        app.tab.inspector_ui.expanded(inspector::Section::AtomMaps),
        Some(true)
    );
    assert!(app.tab.mapping.is_pending());
    assert!(app.tab.mapping.proposal().is_none());
    assert_eq!(app.status, "Finding atom correspondence…");

    // Complete the pending first job through the same Ready message its worker sends.
    let proposal = Arc::new(propose(&app.tab.doc, arrow, Budget::default()).unwrap());
    let task = app.update(Message::Mapping(reaction_mapping::Action::Ready(
        app.tab.file_epoch,
        1,
        Ok(proposal),
    )));
    assert!(!app.tab.mapping.is_pending());
    assert_eq!(
        app.tab.mapping.proposal().unwrap().pairs,
        [(reactant, product)]
    );
    assert_eq!(app.status, "Atom-map proposal ready to review");
    assert!(
        task.units() > 0,
        "Scroll to the proposal's Apply and Discard controls"
    );
    assert!(!app.error);
    assert!(app.inspector_open && app.inspector_tab == InspectorTab::Reactions);
    assert_eq!(
        app.tab.inspector_ui.expanded(inspector::Section::AtomMaps),
        Some(true)
    );
    assert_eq!(app.tab.doc, document);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn molecular_properties_palette_uses_attachment_composition() {
    let mut app = app();
    let fe = app.tab.doc.add_atom("Fe", Point::new(100., 100.));
    app.tab.doc.atom_mut(fe).unwrap().charge = 2;
    for position in [Point::new(20., 30.), Point::new(180., 170.)] {
        let endpoint = app.tab.doc.add_atom("C", position);
        app.tab.doc.add_bond(fe, endpoint, 1, "plain");
        app.tab.doc =
            reshiki::atom_text::apply(&app.tab.doc, endpoint, "Cp", reshiki::atom_text::Mode::Auto)
                .unwrap();
    }
    run(&mut app, "Molecular formula and properties");
    assert!(
        app.status.starts_with("Molecular properties · C10H10Fe · "),
        "{}",
        app.status
    );
    assert!(!app.error);
}
