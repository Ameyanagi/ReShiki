//! Exercise the entry command and atom-map handoff without launching xTB.
use super::super::review;
use super::*;
use crate::app::xtb::ts::card::tests::hydrogen_transfer;
use reshiki::document::Point;

#[test]
fn the_selected_scheme_reviews_only_the_step_requesting_a_barrier() {
    let (mut doc, first) = hydrogen_transfer();
    // Return from the shared intermediate to the starting material.
    let mut arrow = doc.arrows[0].clone();
    arrow.id = doc.next_id();
    arrow.start = Point::new(280., 100.);
    arrow.end = Point::new(120., 100.);
    let second = arrow.id;
    doc.arrows.push(arrow);
    let mut reverse = doc.reactions[0].clone();
    reverse.arrow = second;
    std::mem::swap(&mut reverse.reactants, &mut reverse.products);
    doc.reactions.push(reverse);
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc.clone();
    app.tab.selected = vec![second, first];
    let entry = app
        .context_commands()
        .into_iter()
        .find(|c| c.menu == "Calculate scheme…")
        .unwrap();
    assert!(entry.enabled);
    let _ = app.update(entry.message);
    let _ = app.update(send(Action::Toggle(0, Quantity::Barrier)));
    let _ = app.update(send(Action::Run));
    assert!(app.ts_review_visible(), "{}", app.status);
    let reviews = app
        .tab
        .xtb
        .scheme
        .as_ref()
        .unwrap()
        .reviews
        .as_ref()
        .unwrap();
    assert_eq!(reviews.active, Some(1));
    assert!(reviews.pending.is_empty());
    let _ = app.update(Message::Xtb(xtb::Action::Ts(TsAction::ReviewSheet(
        review::Action::Accept,
    ))));
    let reviews = app
        .tab
        .xtb
        .scheme
        .as_ref()
        .unwrap()
        .reviews
        .as_ref()
        .unwrap();
    assert!(matches!(reviews.inputs[0], Ok(None)));
    assert!(reviews.inputs[1].as_ref().unwrap().is_some());
    assert!(!app.ts_review_open());
    assert_eq!(app.tab.doc, doc);
    let _ = app.update(send(Action::Cancel));
    assert!(!app.xtb_scheme_running());
    app.tab.selected = vec![first];
    assert!(!app.xtb_scheme_selected());
    let row = app.context_commands();
    assert!(!row.iter().any(|c| c.menu == "Calculate scheme…"));
    assert!(row.iter().any(|c| matches!(c.message, Message::Xtb(xtb::Action::Ts(TsAction::Calculate(id))) if id == first)));
}

#[test]
fn command_palette_opens_the_scheme_with_its_balance_notes() {
    use crate::app::command_palette::Action as Commands;
    let (doc, plan) = Plan::example();
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.selected = plan.arrows.clone();
    app.tab.busy = false;
    for action in [
        Commands::Open,
        Commands::Query("Calculate scheme".into()),
        Commands::Submit,
    ] {
        let _ = app.update(Message::Commands(action));
    }
    assert!(app.scheme_sheet_visible(), "{}", app.status);
    let state = app.tab.xtb.scheme.as_ref().unwrap();
    assert_eq!(state.plan.arrows, plan.arrows);
    assert!(state.plan.notes.iter().any(|note| note.contains("H₂O")));
    let _ = app.update(send(Action::Toggle(0, Quantity::Barrier)));
    let _ = app.update(send(Action::Close));
    let _ = app.update(send(Action::Open));
    assert!(app.tab.xtb.scheme.as_ref().unwrap().options[0].barrier);
}

#[test]
fn skipping_a_barrier_preserves_endpoint_choices_and_continues_the_map_queue() {
    let (doc, plan) = Plan::example();
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc.clone();
    app.tab.selected = plan.arrows;
    let _ = app.update(send(Action::Open));
    let _ = app.update(send(Action::Toggle(0, Quantity::Energy)));
    for quantity in [Quantity::FreeEnergy, Quantity::FreeBarrier] {
        let _ = app.update(send(Action::Toggle(0, quantity)));
    }
    let _ = app.update(send(Action::Run));
    let _ = app.update(Message::Xtb(xtb::Action::Ts(TsAction::ReviewSheet(
        review::Action::SkipBarrier,
    ))));
    let state = app.tab.xtb.scheme.as_ref().unwrap();
    assert_eq!(
        state.options[0],
        StepOptions {
            reaction_energy: true,
            barrier: false,
            free_energy: true,
            free_barrier: false,
        }
    );
    assert!(state.options[1].barrier);
    let reviews = state.reviews.as_ref().unwrap();
    assert!(matches!(reviews.inputs[0], Ok(None)));
    assert_eq!(reviews.active, None);
    assert_eq!(reviews.pending.iter().copied().collect::<Vec<_>>(), [1]);
    let next = reviews.id;
    let _ = app.update(send(Action::Next(next)));
    assert_eq!(
        app.tab
            .xtb
            .scheme
            .as_ref()
            .unwrap()
            .reviews
            .as_ref()
            .unwrap()
            .active,
        Some(1)
    );
    assert!(app.ts_review_open());
    assert_eq!(app.tab.doc, doc);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn a_late_event_after_cancel_cannot_replace_status_after_a_chemistry_edit() {
    let (doc, plan) = Plan::example();
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.selected = plan.arrows;
    app.tab.busy = false;
    let _ = app.update(send(Action::Open));
    let work = &mut app.tab.xtb.scheme.as_mut().unwrap().work;
    let late = work.completion_event();
    work.cancel();
    app.tab.doc.atoms[0].charge += 1;
    app.tab.revision += 1;
    app.status = "The user's next action".into();
    let _ = app.update(send(Action::Event(Box::new(late))));
    assert_eq!(app.status, "The user's next action");
}

#[test]
fn retry_after_a_stereo_failure_recaptures_the_replacement_conformer() {
    use reshiki::models3d::{ForceField, Hydrogen, Model3d, Placed};
    use reshiki::science;

    let (mut doc, plan) = Plan::example();
    let index = plan.steps[0].products[0].species;
    let [left, right] = plan.species[index].atoms[..] else {
        panic!("expected the shared ethene intermediate");
    };
    let model = Model3d::new(
        &doc,
        vec![
            Placed {
                id: left,
                xyz: [0., 0., 0.],
            },
            Placed {
                id: right,
                xyz: [1.3, 0., 0.],
            },
        ],
        [(left, -0.5), (right, 1.8)]
            .into_iter()
            .flat_map(|(parent, x)| {
                [-0.9, 0.9].map(|y| Hydrogen {
                    parent,
                    xyz: [x, y, 0.],
                })
            })
            .collect(),
        ForceField::Mmff94s,
    );
    doc.models.push(model);
    reshiki::models3d::require_version(&mut doc);
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc;
    app.tab.selected = plan.arrows;
    let _ = app.update(send(Action::Open));
    let state = app.tab.xtb.scheme.as_mut().unwrap();
    let launch = |state: &State| {
        science::launch(&state.plan.species[index].calculation, left, 0, 1).unwrap()
    };
    let old = launch(state);
    state.sheet = false;
    state.work.steps =
        vec![
            run::StepStatus::Failed("Optimization changed the drawing's stereochemistry".into());
            2
        ];

    let before = app.tab.doc.clone();
    app.tab.doc.models[0].hydrogens[0].xyz[1] = -1.1;
    app.changed(before);
    let current = science::launch(&app.tab.doc, left, 0, 1).unwrap();
    assert_ne!(old.frame.hash(), current.frame.hash());
    assert!(app.xtb_scheme_current());

    let _ = app.update(send(Action::Retry(0)));
    let state = app.tab.xtb.scheme.as_ref().unwrap();
    assert_eq!(state.reviews.as_ref().unwrap().retry, Some(0));
    assert_eq!(launch(state).frame.hash(), current.frame.hash());
}
