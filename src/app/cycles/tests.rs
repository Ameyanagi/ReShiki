use super::*;
use crate::canvas::Edit;
use reshiki::document::{Annotation, Point};

fn text(app: &mut App, x: f32, y: f32, value: &str) -> u64 {
    let id = app.tab.doc.next_id();
    let style = app.tab.doc.drawing_style.text_style();
    app.tab.doc.annotations.push(Annotation {
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
    let _ = app.update(Message::Cycle(Action::Make));
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
fn dropping_text_beside_a_step_makes_it_a_reagent_in_the_same_undo_step() {
    let (mut app, _) = App::new();
    let parts = [
        text(&mut app, 0., -200., "A"),
        text(&mut app, 170., 90., "B"),
        text(&mut app, -230., 90., "C"),
    ];
    app.tab.selected = parts.to_vec();
    let _ = app.update(Message::Cycle(Action::Make));
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
    let _ = app.update(Message::Cycle(Action::Make));
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

fn join(app: &mut App, id: u64, role: Role, part: &[u64]) {
    let ink = reshiki::scene::selection_bounds(&app.tab.doc, part).unwrap();
    reshiki::cycles::assign(
        &mut app.tab.doc,
        &reshiki::cycles::Target {
            cycle: id,
            role,
            part: part.to_vec(),
            ink,
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
    assert_eq!(app.status, "Atom maps applied and colored by origin");
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
    let _ = app.update(Message::Cycle(Action::Make));
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
