//! Exercise the shared local naming queue without launching naming or xTB.
use super::*;
use crate::app::naming::{Action as NamingAction, Outcome, test_support::record};
use crate::app::xtb::ts::profile as energy_profile;
use reshiki::document::{Document, Point};
use reshiki::graphics::{
    EnergyProfile, EnergyProfileStateKind as Kind, EnergyProfileStep, Graphic, GraphicStyle,
};

fn open(doc: Document, arrows: Vec<u64>) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.selected = arrows;
    app.tab.busy = false;
    let _ = app.update(send(Action::Open));
    assert!(app.scheme_sheet_visible(), "{}", app.status);
    app
}

fn name_next(app: &mut App, name: &str) {
    let smiles = app.tab.naming.caption_pending.clone().unwrap();
    let mut named = record(&smiles);
    named.systematic_name = Some(name.into());
    let _ = app.update(Message::Naming(NamingAction::CaptionNamed(
        smiles,
        Box::new(Ok(Outcome::Structure(named))),
    )));
}

#[test]
fn scheme_setup_names_arrive_from_the_shared_queue_and_failed_names_keep_formulas() {
    let (doc, plan) = Plan::example();
    let mut app = open(doc.clone(), plan.arrows);
    let frames = app.tab.history.frames();
    let revision = app.tab.revision;
    assert!(
        app.naming_can_edit(),
        "the scheme sheet permits background names"
    );
    assert_eq!(app.tab.xtb.scheme.as_ref().unwrap().name_requests.len(), 3);
    assert_eq!(
        app.start_caption_names().units(),
        0,
        "one request runs at a time"
    );
    name_next(&mut app, "ethanol");
    let refused = app.tab.naming.caption_pending.clone().unwrap();
    let _ = app.update(Message::Naming(NamingAction::CaptionNamed(
        refused.clone(),
        Box::new(Err(reshiki::naming::Error::Rejected(
            "not supported".into(),
        ))),
    )));
    name_next(&mut app, "ethane");
    let plan = &app.tab.xtb.scheme.as_ref().unwrap().plan;
    assert_eq!(plan.step_label(0), "ethanol → C2H4 + water");
    assert_eq!(plan.step_label(1), "C2H4 + hydrogen → ethane");
    assert_eq!(
        plan.levels()
            .unwrap()
            .iter()
            .map(|level| level.label())
            .collect::<Vec<_>>(),
        [
            "ethanol + hydrogen",
            "C2H4 + water + hydrogen",
            "ethane + water"
        ]
    );
    assert_eq!(
        plan.notes,
        [
            "Balanced by adding H₂O to step 1.",
            "Balanced by adding H₂ to step 2."
        ]
    );
    assert!(app.tab.naming.caption_failed.contains_key(&refused));
    assert!(app.tab.naming.caption_pending.is_none());
    assert_eq!(
        app.start_caption_names().units(),
        0,
        "failed identities are not retried"
    );
    assert!(
        !app.error,
        "an unavailable display name keeps the formula visible"
    );
    assert_eq!(app.tab.doc, doc);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.history.frames(), frames);
    assert!(app.xtb_scheme_current());
}

#[test]
fn a_numbered_representative_does_not_hide_an_unnumbered_copy_from_the_name_queue() {
    let (mut doc, plan) = Plan::example();
    let reactant = doc.reactions[0].reactants[0].atoms.clone();
    reshiki::compound_numbers::number(&mut doc, &reactant, Default::default()).unwrap();
    let copy = [
        doc.add_atom("C", Point::new(700., 0.)),
        doc.add_atom("C", Point::new(736., 0.)),
        doc.add_atom("O", Point::new(772., 0.)),
    ];
    doc.add_bond(copy[0], copy[1], 1, "plain");
    doc.add_bond(copy[1], copy[2], 1, "plain");
    doc.reactions[1]
        .products
        .push(reshiki::reactions::Participant {
            atoms: copy.to_vec(),
            coefficient: 1,
        });
    let mut app = open(doc, plan.arrows);
    let requested = app.tab.naming.caption_pending.clone().unwrap();
    assert_eq!(
        requested,
        reshiki::naming::selected_identity(&app.tab.doc, &copy)
            .unwrap()
            .smiles
    );
    name_next(&mut app, "ethanol");
    let plan = &app.tab.xtb.scheme.as_ref().unwrap().plan;
    assert_eq!(plan.endpoint_label(0, false), "1");
    assert_eq!(plan.endpoint_label(1, true), "C2H6 + ethanol");
    assert_eq!(plan.species[0].label, "1");
    assert_eq!(plan.species[0].name, "ethanol");
}

#[test]
fn a_pinned_caption_completing_in_the_sheet_updates_labels_without_staling_the_scheme() {
    let (mut doc, plan) = Plan::example();
    let reactant = doc.reactions[0].reactants[0].atoms.clone();
    reshiki::linked_properties::show(
        &mut doc,
        &reactant,
        &[reshiki::linked_properties::Property::Name],
        Default::default(),
        false,
    )
    .unwrap();
    let mut app = open(doc, plan.arrows);
    let frames = app.tab.history.frames();
    let revision = app.tab.revision;
    name_next(&mut app, "ethanol");
    assert_eq!(
        app.tab
            .xtb
            .scheme
            .as_ref()
            .unwrap()
            .plan
            .endpoint_label(0, false),
        "ethanol"
    );
    assert_eq!(app.tab.history.frames(), frames);
    assert_eq!(app.tab.revision, revision);
    app.tab.doc.atoms[0].position.x += 10.;
    app.tab.revision += 1;
    assert!(
        app.xtb_scheme_current(),
        "a later position edit also keeps the plan current"
    );
}

#[test]
fn late_names_wait_for_the_drawing_and_update_only_linked_generated_labels() {
    let (doc, plan) = Plan::example();
    let mut app = open(doc, plan.arrows.clone());
    let labels = plan.levels().unwrap();
    let states = labels
        .iter()
        .zip([Kind::Reactant, Kind::Intermediate, Kind::Product])
        .map(|(level, kind)| energy_profile::state(kind, level.label(), 0.))
        .collect();
    let steps = plan
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| EnergyProfileStep {
            unresolved: false,
            from: index,
            to: index + 1,
            transition: None,
            source: (index > 0).then_some(step.arrow),
            source_key: (index > 0).then(|| {
                energy_profile::source_key(
                    &app.tab.doc,
                    &plan::preview(&app.tab.doc, step.arrow).unwrap(),
                )
            }),
            irc: vec![],
        })
        .collect();
    let figure = EnergyProfile::new(
        plan.arrows[0],
        energy_profile::source_key(
            &app.tab.doc,
            &plan::preview(&app.tab.doc, plan.arrows[0]).unwrap(),
        ),
        "GFN2-xTB".into(),
        states,
        steps,
    );
    for index in 0..3 {
        let mut figure = figure.clone();
        if index == 1 {
            figure.unlink();
        } else if index == 2 {
            figure.states[0].label = "Starting mixture".into();
        }
        let id = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::energy_profile(
            id,
            Point::new(20., 100. + index as f32 * 150.),
            Point::new(500., 120.),
            figure,
            GraphicStyle::default(),
        ));
    }
    let before = app.tab.doc.clone();
    let mut expected = before.clone();
    expected.graphics[0]
        .energy_profile
        .as_deref_mut()
        .unwrap()
        .states[0]
        .label = "ethanol + hydrogen".into();
    let frames = app.tab.history.frames();
    let revision = app.tab.revision;
    app.tab.busy = true;
    name_next(&mut app, "ethanol");
    assert_eq!(app.tab.doc, before);
    app.tab.busy = false;
    let _ = app.start_caption_names();
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.history.frames(), frames);
    assert_eq!(app.tab.revision, revision);
    assert!(app.xtb_scheme_current());
    // Hide every scheme surface: an unfinished queue does not keep working
    // just because its arrows remain selected.
    let _ = app.update(send(Action::Close));
    app.inspector_open = false;
    assert!(app.xtb_scheme_name_request().is_none());
}
