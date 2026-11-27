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
