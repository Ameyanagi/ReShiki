use super::*;
use crate::canvas::Edit;
use reshiki::document::{Annotation, Document, Point};

#[path = "shape_tests.rs"]
mod shapes;

fn text(app: &mut App, x: f32, y: f32, value: &str) -> u64 {
    let id = app.tab.doc.next_id();
    let style = app.tab.doc.drawing_style.text_style();
    app.tab.doc.annotations.push(Annotation {
        angle: 0.,
        id,
        position: Point::new(x, y),
        text: value.into(),
        format: reshiki::typography::TextFormat {
            style,
            ..Default::default()
        },
    });
    id
}

/// Make catalytic cycle on the selection, then make it from the review.
fn make(app: &mut App) {
    let _ = app.update(Message::Cycle(Action::Make));
    let _ = app.update(Message::Cycle(Action::MakeApply));
}

fn cycle(app: &App) -> (u64, Cycle) {
    app.selected_cycle()
        .map(|(id, c)| (id, c.clone()))
        .or_else(|| {
            app.tab
                .doc
                .graphics
                .iter()
                .find_map(|g| Some((g.id, g.cycle.as_deref().cloned()?)))
        })
        .unwrap()
}

fn tidy_qa(name: &str, before: &Document, preview: &Document) {
    if let Some(dir) = std::env::var_os("RESHIKI_CYCLE_APP_QA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (state, doc) in [("before", before), ("preview", preview)] {
            std::fs::write(
                dir.join(format!("{name}-{state}.svg")),
                reshiki::scene::svg(doc),
            )
            .unwrap();
            std::fs::write(
                dir.join(format!("{name}-{state}.rsk")),
                doc.file_json().unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn make_name_and_reverse_are_each_one_undo_step() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "Pd(0)L2"),
        text(&mut app, 170., 90., "ArPdX"),
        text(&mut app, -230., 90., "ArPdAr"),
    ];
    let initial = app.tab.doc.clone();
    app.tab.selected = parts.to_vec();
    make(&mut app);
    assert!(!app.error, "{}", app.status);
    let made = app.tab.doc.clone();
    let (id, c) = cycle(&app);
    assert_eq!(c.intermediates.len(), 3);
    // The whole cycle is selected, so its card shows.
    assert!(app.tab.selected.contains(&id) && parts.iter().all(|p| app.tab.selected.contains(p)));
    assert!(app.cycle_card().is_some());

    let _ = app.update(Message::Cycle(Action::Edit(
        Field::Name,
        "Oxidative addition".into(),
    )));
    assert_eq!(app.tab.doc, made, "typing alone changes nothing");
    let _ = app.update(Message::Cycle(Action::Apply(Field::Name)));
    assert_eq!(cycle(&app).1.steps[0].name, "Oxidative addition");
    let named = app.tab.doc.clone();
    let _ = app.update(Message::Cycle(Action::Clockwise(false)));
    assert!(cycle(&app).1.counterclockwise);

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, named);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, made);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
}

#[test]
fn tidy_cycle_is_one_undo_step_and_idempotent() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    make(&mut app);
    app.tab.doc.translate(&[parts[1]], -50., 20.);
    reshiki::leaders::add_callout(
        &mut app.tab.doc,
        reshiki::leaders::Target::Object { id: parts[1] },
        "Moved intermediate",
    )
    .unwrap();
    app.tab.selected = vec![cycle(&app).0];
    let before = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let undo_count = app.tab.history.undo_frames().len();

    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, before, "review leaves the drawing untouched");
    assert_eq!(app.tab.history.undo_frames().len(), undo_count);
    assert!(app.cycle_proposal_card().is_some());
    let preview = app.display_document().clone();
    assert_ne!(preview, before);

    let _ = app.update(Message::ContextKey("Enter".into()));
    let tidied = app.tab.doc.clone();
    assert_eq!(app.status, "Tidied the cycle");
    assert_eq!(tidied, preview);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.history.undo_frames().len(), undo_count + 1);

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, tidied);

    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.status, "The cycle is already tidy");
    assert_eq!(app.tab.doc, tidied);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn tidy_stereo_preview_matches_apply_and_discard_keeps_visible_labels() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap();
    let lactic: Vec<_> = app.tab.doc.atoms.iter().map(|a| a.id).collect();
    // (E)-but-2-ene, with the terminal methyl groups across the double bond.
    let a = app.tab.doc.add_atom("C", Point::new(210., 80.));
    let b = app.tab.doc.add_atom("C", Point::new(252., 80.));
    let left = app.tab.doc.add_atom("C", Point::new(189., 44.));
    let right = app.tab.doc.add_atom("C", Point::new(273., 116.));
    app.tab.doc.add_bond(a, b, 2, "plain");
    app.tab.doc.add_bond(a, left, 1, "plain");
    app.tab.doc.add_bond(b, right, 1, "plain");
    let third = text(&mut app, -240., 120., "Catalyst");
    let mut selected = lactic.clone();
    selected.extend([a, b, left, right, third]);
    let (id, _) = reshiki::cycles::make(&mut app.tab.doc, &selected).unwrap();
    app.tab.doc.translate(&lactic, -30., 45.);
    app.tab.doc.atom_labels.stereo = true;
    let labels =
        reshiki::atom_labels::refresh::Refresh::calculate(&app.tab.doc, &Default::default())
            .unwrap();
    labels.apply(&mut app.tab.doc);
    let indicators = reshiki::atom_labels::indicators(&app.tab.doc);
    assert!(indicators.iter().any(|label| label.text == "(R)"));
    assert!(indicators.iter().any(|label| label.text == "(E)"));
    app.tab.selected = vec![id];
    let before = app.tab.doc.clone();
    let roles = cycle(&app).1;
    let frames = app.tab.history.undo_frames().len();

    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(!app.error, "{}", app.status);
    let preview = app.display_document().clone();
    tidy_qa("stereo", &before, &preview);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), frames);
    assert!(preview.atom_labels.stereo, "display settings stay enabled");
    assert!(preview.atoms.iter().all(|a| a.cip_label.is_none()));
    assert!(preview.bonds.iter().all(|b| b.cip_label.is_none()));
    assert!(reshiki::atom_labels::indicators(&preview).is_empty());
    for atom in &before.atoms {
        let after = preview.atom(atom.id).unwrap();
        assert_eq!(after.stereo, atom.stereo);
        assert_eq!(after.label_h, atom.label_h);
    }
    let mut cleared = before.clone();
    reshiki::atom_labels::clear_computed(&mut cleared);
    assert_eq!(preview.bonds, cleared.bonds);
    let svg = reshiki::scene::svg(&preview);

    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, before);
    assert_eq!(
        reshiki::scene::svg(&app.tab.doc),
        reshiki::scene::svg(&before)
    );
    assert_eq!(app.tab.history.undo_frames().len(), frames);
    let _ = app.update(Message::Cycle(Action::Tidy));
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert_eq!(app.tab.doc, preview);
    assert_eq!(reshiki::scene::svg(&app.tab.doc), svg);
    assert_eq!(cycle(&app).1, roles);
    assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
}

#[test]
fn tidy_stereo_labels_on_unrelated_molecules_survive_a_text_only_cycle_edit() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap();
    app.tab.doc.atom_labels.stereo = true;
    let parts = [
        text(&mut app, 300., -200., "A"),
        text(&mut app, 470., 90., "B"),
        text(&mut app, 70., 90., "C"),
    ];
    let (id, _) = reshiki::cycles::make(&mut app.tab.doc, &parts).unwrap();
    app.tab.selected = vec![id];
    let before = app.tab.doc.clone();
    assert!(
        reshiki::atom_labels::indicators(&before)
            .iter()
            .any(|label| label.text == "(R)")
    );
    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(!app.error, "{}", app.status);
    let preview = app.display_document().clone();
    assert_ne!(preview.annotations, before.annotations);
    assert_eq!(preview.atoms, before.atoms);
    assert_eq!(preview.bonds, before.bonds);
    assert!(
        reshiki::atom_labels::indicators(&preview)
            .iter()
            .any(|label| label.text == "(R)")
    );
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert_eq!(app.tab.doc, preview);
}

#[test]
fn tidy_preview_blocks_edits_and_discard_preserves_the_drawing() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    make(&mut app);
    app.tab.doc.translate(&[parts[1]], -50., 20.);
    let before = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let undo_count = app.tab.history.undo_frames().len();

    let _ = app.update(Message::Cycle(Action::Tidy));
    let preview = app.display_document().clone();
    assert_ne!(preview, before);
    let _ = app.update(Message::Delete);
    assert_eq!(app.display_document(), &preview);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Cycle(Action::MakeReverse));
    assert_eq!(app.display_document(), &preview);
    let _ = app.update(Message::Escape);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.display_document(), &before);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.history.undo_frames().len(), undo_count);
    assert_eq!(app.status, "Cycle tidy discarded");

    app.tab.busy = true;
    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
}

#[test]
fn actual_suzuki_tidy_preview_applies_as_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/cycles/tidy-before.rsk"
    ))
    .unwrap();
    app.tab.selected = vec![191];
    let before = app.tab.doc.clone();
    let roles = cycle(&app).1;
    let frames = app.tab.history.undo_frames().len();

    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), frames);
    let preview = app.display_document().clone();
    assert_ne!(preview, before);
    tidy_qa("suzuki", &before, &preview);

    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    assert_eq!(cycle(&app).1, roles);
    assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
    let saved = Document::from_json(&app.tab.doc.file_json().unwrap()).unwrap();
    assert_eq!(saved.atoms, preview.atoms);
    assert_eq!(saved.bonds, preview.bonds);
    assert_eq!(saved.annotations, preview.annotations);
    assert_eq!(saved.graphics, preview.graphics);

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert_eq!(cycle(&app).1, roles);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
}

#[test]
fn noncircle_suzuki_tidy_preview_discard_apply_and_undo_keep_the_shape_and_roles() {
    for shape in [
        reshiki::cycles::Shape::Ellipse,
        reshiki::cycles::Shape::RoundedRectangle,
        reshiki::cycles::Shape::Rectangle,
    ] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::from_json(include_bytes!(
            "../../../tests/fixtures/cycles/tidy-before.rsk"
        ))
        .unwrap();
        let g = app
            .tab
            .doc
            .graphics
            .iter_mut()
            .find(|g| g.id == 191)
            .unwrap();
        let center = g.cycle_circle().0;
        g.cycle.as_mut().unwrap().shape = shape;
        g.axis_x.x *= 1.3;
        g.axis_x.y *= 1.3;
        g.axis_y.x *= 0.8;
        g.axis_y.y *= 0.8;
        g.origin = center.offset(
            -(g.axis_x.x + g.axis_y.x) * 0.5,
            -(g.axis_x.y + g.axis_y.y) * 0.5,
        );
        app.tab.selected = vec![191];
        let before = app.tab.doc.clone();
        let roles = cycle(&app).1;
        let frames = app.tab.history.undo_frames().len();
        let _ = app.update(Message::Cycle(Action::Tidy));
        assert!(!app.error, "{shape:?}: {}", app.status);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.history.undo_frames().len(), frames);
        let preview = app.display_document().clone();
        assert_ne!(preview, before);
        tidy_qa(&format!("noncircle-suzuki-{shape:?}"), &before, &preview);
        let drawing = reshiki::cycles::drawn(
            &preview,
            preview.graphics.iter().find(|g| g.id == 191).unwrap(),
        );
        assert!(drawing.arrows.iter().all(|arrow| arrow.arc.is_none()));
        let _ = app.update(Message::Escape);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.history.undo_frames().len(), frames);
        let _ = app.update(Message::Cycle(Action::Tidy));
        let _ = app.update(Message::ContextKey("Enter".into()));
        assert!(!app.error, "{shape:?}: {}", app.status);
        assert_eq!(app.tab.doc, preview);
        assert_eq!(cycle(&app).1, roles);
        assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
        let saved = app.tab.doc.file_json().unwrap();
        assert_eq!(
            Document::from_json(&saved).unwrap().file_json().unwrap(),
            saved
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, preview);
        let _ = app.update(Message::Cycle(Action::Tidy));
        assert!(app.tab.cycle_inputs.proposal.is_none());
        assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
    }
}

#[test]
fn a_delayed_caption_result_requires_fresh_tidy_review_before_apply() {
    use crate::app::naming::{Action as NamingAction, Outcome, test_support::record};
    use reshiki::linked_properties::Property;

    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/cycles/tidy-before.rsk"
    ))
    .unwrap();
    let ethanol = Document::from_json(include_bytes!(
        "../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"
    ))
    .unwrap();
    let atoms = reshiki::editing::append(&mut app.tab.doc, &ethanol, Point::new(1200., 0.));
    let _ = app.update(Message::ToggleProperty(atoms, Property::Name));
    let smiles = app.tab.naming.caption_pending.clone().unwrap();
    let caption = app.tab.doc.property_captions.last().unwrap().annotation;
    app.tab.selected = vec![191];
    let roles = cycle(&app).1;
    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(!app.error, "{}", app.status);
    let old_preview = app.display_document().clone();
    let frames = app.tab.history.undo_frames().len();
    let revision = app.tab.revision;

    // This real completion changes caption ink and label_generation, while
    // retaining the revision and Undo history, as ordinary async names do.
    let _ = app.update(Message::Naming(NamingAction::CaptionNamed(
        smiles.clone(),
        Box::new(Ok(Outcome::Structure(record(&smiles)))),
    )));
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.history.undo_frames().len(), frames);
    let named = app.tab.doc.clone();
    assert_eq!(
        named
            .annotations
            .iter()
            .find(|a| a.id == caption)
            .unwrap()
            .text,
        "ethan-1-ol"
    );

    // First Enter refreshes the candidate. The completed name is retained,
    // but the user must review that candidate before its layout is applied.
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, named);
    assert_eq!(app.tab.history.undo_frames().len(), frames);
    assert!(app.tab.cycle_inputs.proposal.is_some());
    let fresh_preview = app.display_document().clone();
    assert_ne!(fresh_preview, old_preview);
    assert_eq!(
        fresh_preview
            .annotations
            .iter()
            .find(|a| a.id == caption)
            .unwrap()
            .text,
        "ethan-1-ol"
    );

    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.doc, fresh_preview);
    assert_eq!(cycle(&app).1, roles);
    assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc, named,
        "Undo retains the completed background caption"
    );
}

#[test]
fn tidy_without_a_selected_cycle_changes_nothing() {
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Cycle(Action::Tidy));
    assert!(app.error);
    assert_eq!(app.status, "Select a catalytic cycle first");
    assert_eq!(app.tab.doc, before);
}

#[test]
fn dropping_text_beside_a_step_makes_it_a_reagent_in_the_same_undo_step() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    make(&mut app);
    let (id, _) = cycle(&app);
    let g = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
    let (center, radius) = g.cycle_circle();
    let free = text(&mut app, 2000., 2000., "Ar–X");
    let before = app.tab.doc.clone();
    // Just outside the circle, early in step 1 (from the top toward the right).
    let angle = -60_f32.to_radians();
    let r = radius + 2. * app.tab.doc.drawing_style.bond_length_world;
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &[free]).unwrap();
    let c = lo.lerp(hi, 0.5);
    let _ = app.update(Message::Canvas(Edit::Move(
        vec![free],
        center.x + r * angle.cos() - c.x,
        center.y + r * angle.sin() - c.y,
    )));
    assert_eq!(cycle(&app).1.steps[0].reagents, vec![vec![free]]);
    assert!(app.status.contains("Reagent of step 1"), "{}", app.status);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn a_part_selected_alone_shows_its_role_and_leaves_the_cycle() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    make(&mut app);
    app.tab.selected = vec![parts[1]];
    assert!(app.cycle_part_card().is_some());
    let _ = app.update(Message::Cycle(Action::Remove));
    assert!(!app.error, "{}", app.status);
    assert_eq!(cycle(&app).1.intermediates.len(), 2);
    assert!(app.tab.doc.annotations.iter().any(|a| a.id == parts[1]));
}

/// Atoms `(element, x, y, map)` bonded in a chain, as one molecule.
fn chain(app: &mut App, atoms: &[(&str, f32, f32, u32)]) -> Vec<u64> {
    let doc = &mut app.tab.doc;
    let ids: Vec<u64> = atoms
        .iter()
        .map(|&(element, x, y, map)| {
            let id = doc.add_atom(element, Point::new(x, y));
            let atom = doc.atom_mut(id).unwrap();
            atom.map_num = map;
            atom.no_implicit = element != "C";
            id
        })
        .collect();
    for pair in ids.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    ids
}

#[test]
fn part_names_use_the_linked_name_and_shorten_long_labels() {
    let (mut app, _) = App::new();
    let methane = chain(&mut app, &[("C", 0., 0., 0)]);
    assert_eq!(app.part_name(&methane), "CH₄");
    let format = reshiki::typography::TextFormat {
        style: app.tab.doc.drawing_style.text_style(),
        ..Default::default()
    };
    reshiki::molecule_names::show(
        &mut app.tab.doc,
        &methane,
        "Methane".into(),
        "C".into(),
        format,
        false,
    )
    .unwrap();
    // Names are cached per drawing revision; an app edit moves it on.
    app.tab.revision += 1;
    assert_eq!(app.part_name(&methane), "Methane");

    let line = "α".repeat(33);
    let label = text(&mut app, 0., 100., &format!("{line}\nSecond line"));
    assert_eq!(app.part_name(&[label]), format!("{}…", "α".repeat(31)));
}

#[test]
fn cycle_step_rows_and_disclosures_open_on_click() {
    let (mut app, _) = App::new();
    let methane = chain(&mut app, &[("C", 0., -200., 0)]);
    let label = text(&mut app, 170., 90., "ArPdX\nSecond line");
    let ethane = chain(&mut app, &[("C", -230., 90., 0), ("C", -200., 90., 0)]);
    app.tab.selected = [methane, vec![label], ethane].concat();
    make(&mut app);
    let _ = app.update(Message::Cycle(Action::Edit(Field::Name, "Coupling".into())));
    let _ = app.update(Message::Cycle(Action::Apply(Field::Name)));

    let (id, cycle) = cycle(&app);
    let labels = app.step_labels(&cycle);
    assert_eq!(labels, ["1  Coupling", "2  ArPdX → C₂H₆", "3  C₂H₆ → CH₄"]);
    assert!(labels.iter().all(|label| !label.contains("atoms")));
    assert!(!app.tab.cycle_inputs.step_expanded);
    assert!(!app.tab.cycle_inputs.layout_open);
    assert!(!app.tab.cycle_inputs.purpose_open);

    let _ = app.update(Message::Cycle(Action::Step(1)));
    assert!(app.tab.cycle_inputs.step_expanded);
    assert_eq!(app.cycle_step(), Some((id, 1)));
    let _ = app.update(Message::Cycle(Action::Step(1)));
    assert!(!app.tab.cycle_inputs.step_expanded);
    assert_eq!(app.cycle_step(), Some((id, 1)));

    let _ = app.update(Message::Cycle(Action::ToggleLayout));
    assert!(app.tab.cycle_inputs.layout_open);
    let _ = app.update(Message::Cycle(Action::TogglePurpose));
    assert!(app.tab.cycle_inputs.purpose_open);
}

fn join(app: &mut App, id: u64, role: Role, part: &[u64]) {
    let ink = reshiki::scene::selection_bounds(&app.tab.doc, part).unwrap();
    reshiki::cycles::assign(
        &mut app.tab.doc,
        &reshiki::cycles::Target {
            cycle: id,
            role,
            part: part.to_vec(),
            ink,
            arc: vec![],
            tag: String::new(),
        },
    );
}

/// Pd + CH3Br → CH3PdBr; + CH3Li → CH3PdCH3 + LiBr; → Pd + ethane, as a
/// selected cycle. With `mapped`, each atom carries the map of the atom it
/// becomes. Returns the cycle and CH3PdBr.
fn coupling(app: &mut App, mapped: bool) -> (u64, Vec<u64>) {
    let m = |n: u32| if mapped { n } else { 0 };
    let pd = chain(app, &[("Pd", 0., -200., m(1))]);
    let oa = chain(
        app,
        &[
            ("C", 150., 90., m(2)),
            ("Pd", 180., 90., m(1)),
            ("Br", 210., 90., m(3)),
        ],
    );
    let tm = chain(
        app,
        &[
            ("C", -250., 90., m(2)),
            ("Pd", -220., 90., m(1)),
            ("C", -190., 90., m(4)),
        ],
    );
    let ids: Vec<u64> = [&pd, &oa, &tm].into_iter().flatten().copied().collect();
    let (id, _) = reshiki::cycles::make(&mut app.tab.doc, &ids).unwrap();
    let mebr = chain(app, &[("C", 260., -160., m(2)), ("Br", 290., -160., m(3))]);
    join(app, id, Role::Reagent(0), &mebr);
    let meli = chain(app, &[("C", 60., 330., m(4)), ("Li", 90., 330., m(5))]);
    join(app, id, Role::Reagent(1), &meli);
    let libr = chain(app, &[("Li", -120., 330., m(5)), ("Br", -90., 330., m(3))]);
    join(app, id, Role::Product(1), &libr);
    let ethane = chain(app, &[("C", -330., -160., m(2)), ("C", -300., -160., m(4))]);
    join(app, id, Role::Product(2), &ethane);
    app.tab.doc.validate().unwrap();
    app.tab.busy = false;
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = vec![id];
    (id, oa)
}

#[test]
fn step_tools_act_on_the_chosen_step_and_color_is_one_undo_step() {
    let (mut app, _) = App::new();
    let (_, oa) = coupling(&mut app, true);
    app.tab.cycle_inputs.step = 1;
    let start = app.tab.doc.clone();
    let _ = app.update(Message::Cycle(Action::Check));
    assert_eq!(app.status, "Step 2 is balanced");
    assert_eq!(app.tab.doc, start);
    // Without LiBr, step 2 loses lithium and bromine; step 3 stays balanced.
    let mut lost = app.tab.doc.clone();
    let libr: Vec<u64> = lost
        .atoms
        .iter()
        .filter(|a| a.element == "Li" && a.map_num == 5)
        .map(|a| a.id)
        .collect();
    let id = app.selected_cycle().unwrap().0;
    let libr = reshiki::reactions::molecules(&lost, &libr)
        .into_iter()
        .find(|m| {
            m.len() == 2 && reshiki::cycles::role_of(&lost, m[0]) == Some((id, Role::Product(1)))
        })
        .unwrap();
    reshiki::cycles::remove(&mut lost, &libr);
    app.tab.doc = lost;
    let _ = app.update(Message::Cycle(Action::Check));
    assert_eq!(
        app.status,
        "Step 2 not balanced · extra on the left: Br, Li"
    );
    app.tab.cycle_inputs.step = 2;
    let _ = app.update(Message::Cycle(Action::Check));
    assert_eq!(app.status, "Step 3 is balanced");
    app.tab.doc = start.clone();
    app.tab.cycle_inputs.step = 1;

    let _ = app.update(Message::Cycle(Action::Color));
    assert!(!app.error, "{}", app.status);
    let colored = app.tab.doc.clone();
    assert_ne!(colored, start);
    // Bromine keeps the color it came in with, through the cycle.
    assert!(colored.atom(oa[2]).unwrap().text_style.is_some());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, start);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, colored);

    // Copying names the step it copied.
    let _ = app.update(Message::Cycle(Action::CopySmiles));
    assert!(app.copy_as_busy, "{}", app.status);
    let key = crate::app::clipboard::CopyAsKey {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        format: reshiki::clipboard::CopyFormat::ReactionSmiles,
        selected: app.tab.selected.clone(),
        what: Some("Step 2".into()),
    };
    let _ = app.update(Message::CopyAsWritten(key, Ok(vec![])));
    assert_eq!(app.status, "Step 2 copied as Reaction SMILES");
}

#[test]
fn a_step_s_atom_map_review_shows_in_the_card_and_maps_the_drawing() {
    use super::super::reaction_mapping::Action as Mapping;
    let (mut app, _) = App::new();
    let (id, oa) = coupling(&mut app, false);
    // Color by origin without maps asks for a review first.
    let _ = app.update(Message::Cycle(Action::Color));
    assert!(app.tab.mapping.color_step);
    assert_eq!(app.tab.history.undo_frames().len(), 0);
    assert_eq!(app.tab.mapping.step, Some((id, 0)));
    let (copy, arrows) = reshiki::cycles::steps::reactions(&app.tab.doc, id).unwrap();
    let proposal = std::sync::Arc::new(
        reshiki::reaction_mapping::propose(&copy, arrows[0], Default::default()).unwrap(),
    );
    let (epoch, serial) = app.tab.mapping.pending.unwrap();
    let _ = app.update(Message::Mapping(Mapping::Ready(
        epoch,
        serial,
        Ok(proposal),
    )));
    assert_eq!(app.status, "Atom-map proposal ready to review");
    assert!(app.step_review_card().is_some());
    // Reviewing a pair selects its atoms; the review stays in view.
    let _ = app.update(Message::Mapping(Mapping::ReviewPair(0)));
    assert!(app.cycle_card().is_none() && app.step_review_card().is_some());
    let _ = app.update(Message::Mapping(Mapping::ApplyProposal));
    assert_eq!(app.status, super::super::reactions::COLORED);
    // The colored figure hides its map numbers in the same Undo step.
    assert!(!app.tab.doc.atom_labels.maps);
    assert!(
        oa.iter()
            .all(|a| app.tab.doc.atom(*a).unwrap().map_num != 0)
    );
    assert!(app.tab.doc.reactions.is_empty() && app.tab.doc.arrows.is_empty());
    assert_eq!(app.tab.history.undo_frames().len(), 1);
}

#[test]
fn right_clicking_a_step_chooses_it_for_the_menu() {
    let (mut app, _) = App::new();
    let (id, _) = coupling(&mut app, false);
    let g = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
    let (center, radius) = g.cycle_circle();
    // Toward the lower left of the circle: from CH3PdBr to CH3PdCH3, step 2.
    let p = Point::new(center.x, center.y + radius);
    assert_eq!(reshiki::cycles::step_at(&app.tab.doc, g, p), Some(1));
    app.edit(Edit::ContextMenu {
        position: iced::Point::ORIGIN,
        at: p,
        selected: vec![id],
        hit: vec![id],
    });
    assert_eq!(app.cycle_step(), Some((id, 1)));
    // With a second cycle selected too, the one right-clicked is used.
    let parts = [
        text(&mut app, 3000., -200., "A"),
        text(&mut app, 3170., 90., "B"),
        text(&mut app, 2770., 90., "C"),
    ];
    let (other, _) = reshiki::cycles::make(&mut app.tab.doc, &parts).unwrap();
    let g = app.tab.doc.graphics.iter().find(|g| g.id == other).unwrap();
    let (center, radius) = g.cycle_circle();
    let p = Point::new(center.x, center.y + radius);
    app.edit(Edit::ContextMenu {
        position: iced::Point::ORIGIN,
        at: p,
        selected: vec![id, other],
        hit: vec![other],
    });
    assert_eq!(app.cycle_step(), Some((other, 1)));
    // An empty circle right-clicked has no steps to act on.
    let empty = reshiki::cycles::insert(&mut app.tab.doc, Point::new(6000., 0.), 100.);
    app.edit(Edit::ContextMenu {
        position: iced::Point::ORIGIN,
        at: Point::new(6000., 100.),
        selected: vec![id, empty],
        hit: vec![empty],
    });
    assert_eq!(app.cycle_step(), None);
}

#[test]
fn a_text_that_leaves_its_reaction_for_a_cycle_says_so() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    make(&mut app);
    let (id, _) = cycle(&app);
    // A condition of an ordinary reaction elsewhere.
    let free = text(&mut app, 2000., 2000., "Ar–X");
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(reshiki::document::Arrow::new(
        arrow,
        Point::new(1800., 2100.),
        Point::new(2200., 2100.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = reshiki::reactions::Reaction::new(arrow);
    reaction.annotations.push(free);
    app.tab.doc.reactions.push(reaction);
    let g = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
    let (center, radius) = g.cycle_circle();
    let angle = -60_f32.to_radians();
    let r = radius + 2. * app.tab.doc.drawing_style.bond_length_world;
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &[free]).unwrap();
    let c = lo.lerp(hi, 0.5);
    let _ = app.update(Message::Canvas(Edit::Move(
        vec![free],
        center.x + r * angle.cos() - c.x,
        center.y + r * angle.sin() - c.y,
    )));
    assert!(
        app.status
            .contains("Now Reagent of step 1 · it left its reaction"),
        "{}",
        app.status
    );
    assert!(app.tab.doc.reactions[0].annotations.is_empty());
}

#[test]
fn balance_status_names_the_extra_atoms_by_side() {
    let balance = reshiki::cycles::steps::Balance {
        before: vec![("B".into(), 1), ("H".into(), 2)],
        after: vec![("Br".into(), 1)],
        charge: [0, 1],
    };
    assert_eq!(
        super::balance_status(1, &balance),
        "Step 2 not balanced · extra on the left: B, 2 H · extra on the right: Br · charge +0 → +1"
    );
}

#[test]
fn make_shows_the_cycle_for_review_before_it_changes_the_drawing() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    let initial = app.tab.doc.clone();
    let _ = app.update(Message::Cycle(Action::Make));
    assert!(!app.error, "{}", app.status);
    // Shown, not made: the drawing and its history are as they were.
    assert_eq!(app.tab.doc, initial);
    assert!(!app.tab.history.can_undo());
    assert!(app.cycle_proposal_card().is_some());
    let shown = app.display_document();
    assert!(shown.graphics.iter().any(|g| g.cycle.is_some()));
    // Edits wait while it is under review.
    let _ = app.update(Message::Delete);
    assert_eq!(app.tab.doc, initial);
    assert!(app.status.contains("Enter applies"), "{}", app.status);
    // Esc discards.
    let _ = app.update(Message::Escape);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.doc, initial);
    assert_eq!(app.tab.selected, parts);

    // Reversed, then Enter makes it as one Undo step.
    let _ = app.update(Message::Cycle(Action::Make));
    let _ = app.update(Message::Cycle(Action::MakeReverse));
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    let (_, c) = cycle(&app);
    assert!(c.counterclockwise);
    assert_eq!(c.intermediates[0], vec![parts[0]]);
    assert_eq!(c.intermediates[1], vec![parts[2]]);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
}

#[test]
fn make_waits_while_a_task_runs() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
    ];
    app.tab.selected = parts.to_vec();
    app.tab.busy = true;
    let _ = app.update(Message::Cycle(Action::Make));
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert!(app.error);
}
