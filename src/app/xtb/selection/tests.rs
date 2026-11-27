use super::*;
use crate::app::{Message, xtb};
use crate::canvas::Edit;
use reshiki::document::{Annotation, Arrow, Point};

fn reaction(app: &mut App, reactant: u64, product: u64) {
    use reshiki::reactions::{self, Role};

    let before = app.tab.doc.clone();
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(70., 0.),
        Point::new(150., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut app.tab.doc, arrow, &[reactant], Role::Reactant).unwrap();
    reactions::assign(&mut app.tab.doc, arrow, &[product], Role::Product).unwrap();
    app.changed(before);
}

#[test]
fn finished_geometry_feedback_clears_when_selecting_the_product() {
    use xtb::tests::{MOVED, START, optimized, water};

    let (mut app, reactant) = water(xtb::CalcTask::Optimize);
    let product = app.tab.doc.add_atom("O", Point::new(220., 0.));
    reaction(&mut app, reactant, product);
    let serial = app.tab.xtb.run.as_ref().unwrap().serial;
    let _ = app.update(Message::Xtb(xtb::Action::Event(
        serial,
        optimized(&START, &MOVED),
    )));
    assert!(app.status.starts_with("Applied xTB geometry ·"));
    let applied = app.status.clone();

    let _ = app.update(Message::Canvas(Edit::Select(vec![reactant])));
    assert_eq!(app.status, applied, "the same molecule keeps its feedback");
    let _ = app.update(Message::Canvas(Edit::SelectAt(
        vec![product],
        Point::new(220., 0.),
    )));
    assert!(app.status.is_empty(), "{}", app.status);
    assert!(!app.error);
}

#[test]
fn applied_step_feedback_follows_the_molecule_and_preserves_newer_status() {
    use xtb::palette::Action as PaletteAction;
    use xtb::tests::{MOVED, START, duplicate_molecule, event, optimized, water};

    for unrelated in [false, true] {
        let (mut app, reactant) = water(xtb::CalcTask::Optimize);
        event(&mut app, optimized(&START, &MOVED));
        let copied = duplicate_molecule(&mut app);
        let product = copied[0];
        reaction(&mut app, reactant, product);
        let _ = app.update(Message::Canvas(Edit::Select(vec![reactant])));
        for action in [
            PaletteAction::View(xtb::results::View::Steps),
            PaletteAction::Step(0),
            PaletteAction::UseStep,
        ] {
            let _ = app.update(Message::Xtb(xtb::Action::Palette(action)));
        }
        assert!(app.status.starts_with("Applied xTB geometry of step 0"));
        if unrelated {
            app.status = "Could not save the drawing".into();
            app.error = true;
        }

        let _ = app.update(Message::Canvas(Edit::Select(vec![product])));
        assert_eq!(app.xtb_shown().unwrap().atoms[0], Some(product));
        if unrelated {
            assert_eq!(app.status, "Could not save the drawing");
            assert!(app.error);
        } else {
            assert!(app.status.is_empty(), "{}", app.status);
            assert!(!app.error);
        }
    }
}

#[test]
fn selection_hint_says_what_to_click() {
    if !available() {
        return;
    }
    let (mut app, _) = App::new();
    let check = |app: &App, subtitle: &str, hint: &str| {
        assert!(app.xtb_target().is_none());
        assert_eq!(
            app.xtb_selection_hint(),
            (subtitle.to_owned(), hint.to_owned())
        );
    };
    check(
        &app,
        "No molecule",
        "Draw or add a molecule to calculate it",
    );

    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(20., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let c = app.tab.doc.add_atom("O", Point::new(100., 0.));
    check(
        &app,
        "No molecule selected",
        "No molecule selected · click one to calculate it",
    );

    app.tab.selected = vec![a, b, c];
    check(
        &app,
        "2 molecules selected",
        "2 molecules selected · click one to calculate it",
    );

    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::default(),
        text: "Sample".into(),
        format: Default::default(),
        angle: 0.,
    });
    app.tab.selected = vec![caption];
    check(
        &app,
        "No molecule selected",
        "Select a molecule to calculate it",
    );
}

#[test]
fn failed_entry_hint_clears_when_one_molecule_is_selected() {
    if !available() {
        return;
    }
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
    let select = |app: &mut App, atoms| {
        let _ = app.update(Message::Canvas(Edit::Select(atoms)));
    };
    let enter = |app: &mut App| {
        let _ = app.update(Message::Xtb(xtb::Action::Entry(xtb::Entry::OptimizeGfn2)));
    };

    select(&mut app, vec![a, b]);
    enter(&mut app);
    assert_eq!(
        app.status,
        "2 molecules selected · click one to calculate it"
    );
    assert!(!app.error);
    assert!(!app.xtb_running());
    select(&mut app, vec![a]);
    assert!(app.status.is_empty());
    assert!(!app.error);

    select(&mut app, vec![a, b]);
    enter(&mut app);
    app.status = "Drawing saved".into();
    select(&mut app, vec![b]);
    assert_eq!(app.status, "Drawing saved");
}
