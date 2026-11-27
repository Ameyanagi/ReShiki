use super::*;
use crate::app::{Message, xtb};
use crate::canvas::Edit;
use reshiki::document::{Annotation, Arrow, Point};

fn reaction(app: &mut App, reactant: u64, product: u64) -> u64 {
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
    arrow
}

#[test]
fn molecule_completion_feedback_follows_the_effective_selection() {
    use xtb::tests::{MOVED, START, optimized, water};

    for task in [xtb::CalcTask::Optimize, xtb::CalcTask::Energy] {
        for selection in ["product", "none", "both", "arrow"] {
            for unrelated in [false, true] {
                let (mut app, reactant) = water(task);
                let product = app.tab.doc.add_atom("O", Point::new(220., 0.));
                let arrow = reaction(&mut app, reactant, product);
                let serial = app.tab.xtb.run.as_ref().unwrap().serial;
                let _ = app.update(Message::Xtb(xtb::Action::Event(
                    serial,
                    optimized(&START, &MOVED),
                )));
                let prefix = if task == xtb::CalcTask::Optimize {
                    "Applied xTB geometry ·"
                } else {
                    "xTB · E −5.070176 Eh"
                };
                assert!(app.status.starts_with(prefix), "{}", app.status);
                let completed = app.status.clone();
                let _ = app.update(Message::Canvas(Edit::Select(vec![reactant])));
                assert_eq!(
                    app.status, completed,
                    "the same molecule keeps its feedback"
                );
                if unrelated {
                    app.status = "Could not save the drawing".into();
                    app.error = true;
                }

                let selected = match selection {
                    "product" => vec![product],
                    "none" => vec![],
                    "both" => vec![reactant, product],
                    "arrow" => vec![arrow],
                    _ => unreachable!(),
                };
                let _ = app.update(Message::Canvas(Edit::SelectAt(
                    selected,
                    Point::new(220., 0.),
                )));
                if unrelated {
                    assert_eq!(app.status, "Could not save the drawing");
                    assert!(app.error);
                } else {
                    assert!(
                        app.status.is_empty(),
                        "{task:?} · {selection}: {}",
                        app.status
                    );
                    assert!(!app.error);
                }
            }
        }
    }
}

#[test]
fn clearing_selection_keeps_completion_feedback_for_the_only_molecule() {
    use xtb::tests::{MOVED, START, optimized, water};

    for task in [xtb::CalcTask::Optimize, xtb::CalcTask::Energy] {
        let (mut app, molecule) = water(task);
        let serial = app.tab.xtb.run.as_ref().unwrap().serial;
        let _ = app.update(Message::Xtb(xtb::Action::Event(
            serial,
            optimized(&START, &MOVED),
        )));
        let completed = app.status.clone();
        assert!(!completed.is_empty());
        let _ = app.update(Message::Canvas(Edit::Select(vec![])));
        assert_eq!(app.xtb_target(), Some(vec![molecule]));
        assert_eq!(app.status, completed);
    }
}

#[test]
fn molecule_review_feedback_follows_the_effective_selection() {
    use xtb::tests::{halocarbon, optimized};

    for selection in ["product", "none", "both", "arrow"] {
        for unrelated in [false, true] {
            let (mut app, reactant, start) = halocarbon();
            let ligand = app
                .tab
                .doc
                .atoms
                .iter()
                .find(|atom| atom.id != reactant)
                .unwrap()
                .id;
            let product = app.tab.doc.add_atom("O", Point::new(220., 0.));
            let arrow = reaction(&mut app, reactant, product);
            let mirrored: Vec<_> = start.iter().map(|&[x, y, z]| [-x, y, z]).collect();
            let serial = app.tab.xtb.run.as_ref().unwrap().serial;
            let _ = app.update(Message::Xtb(xtb::Action::Event(
                serial,
                optimized(&start, &mirrored),
            )));
            assert!(matches!(app.tab.xtb.outcome, Some(xtb::Outcome::Review(_))));
            assert!(app.status.ends_with("· nothing applied"), "{}", app.status);
            let review = app.status.clone();
            let _ = app.update(Message::Canvas(Edit::Select(vec![ligand])));
            assert_eq!(
                app.status, review,
                "another atom in the molecule keeps its feedback"
            );
            if unrelated {
                app.status = "Could not save the drawing".into();
                app.error = true;
            }

            let selected = match selection {
                "product" => vec![product],
                "none" => vec![],
                "both" => vec![reactant, product],
                "arrow" => vec![arrow],
                _ => unreachable!(),
            };
            let _ = app.update(Message::Canvas(Edit::SelectAt(
                selected,
                Point::new(220., 0.),
            )));
            if unrelated {
                assert_eq!(app.status, "Could not save the drawing");
                assert!(app.error);
            } else {
                assert!(app.status.is_empty(), "{selection}: {}", app.status);
                assert!(!app.error);
            }
        }
    }
}

#[test]
fn clearing_selection_keeps_review_feedback_for_the_only_molecule() {
    use xtb::tests::{halocarbon, optimized};

    for initially_empty in [false, true] {
        let (mut app, molecule, start) = halocarbon();
        if initially_empty {
            let _ = app.update(Message::Canvas(Edit::Select(vec![])));
        }
        let mirrored: Vec<_> = start.iter().map(|&[x, y, z]| [-x, y, z]).collect();
        let serial = app.tab.xtb.run.as_ref().unwrap().serial;
        let _ = app.update(Message::Xtb(xtb::Action::Event(
            serial,
            optimized(&start, &mirrored),
        )));
        assert!(matches!(app.tab.xtb.outcome, Some(xtb::Outcome::Review(_))));
        assert!(app.status.ends_with("· nothing applied"), "{}", app.status);
        let review = app.status.clone();
        let _ = app.update(Message::Canvas(Edit::Select(vec![])));
        assert_eq!(app.xtb_target().unwrap().first(), Some(&molecule));
        assert_eq!(app.status, review);
    }
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
