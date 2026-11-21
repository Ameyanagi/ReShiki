use super::*;
use reshiki::{cycles, document::Document};

fn fitted_cycle() -> (App, u64) {
    let mut doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/cycles/bowed-insertion.rsk"
    ))
    .unwrap();
    let id = 191;
    let g = doc.graphics.iter_mut().find(|g| g.id == id).unwrap();
    let cycle = g.cycle.as_mut().unwrap();
    cycle.shape = Shape::Ellipse;
    cycle.steps[0].bow_pt = 180.;
    let drawn = cycles::drawn(&doc, &doc.graphics[0]);
    let to = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .find(|(_, step)| **step == Some(0))
        .map(|(arrow, _)| arrow.point(0.37))
        .unwrap();
    let part: Vec<_> = (40..=85).collect();
    let (lo, hi) = reshiki::scene::selection_bounds(&doc, &part).unwrap();
    let from = lo.lerp(hi, 0.5);
    doc.translate(&part, to.x - from.x, to.y - from.y);
    let baseline = doc.clone();
    let proposal = cycles::propose_drop(&doc, &part).expect("a fitted source run");
    let mut doc = proposal.doc;
    reconcile_preview(&mut doc, &baseline, false).unwrap();
    assert!(
        !doc.graphics[0]
            .cycle
            .as_ref()
            .unwrap()
            .split_runs
            .is_empty()
    );
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc;
    app.tab.selected = vec![id];
    app.sync_cycle();
    (app, id)
}

fn field(app: &mut App, field: Field, value: &str) {
    let _ = app.update(Message::Cycle(Action::Edit(field, value.into())));
    let _ = app.update(Message::Cycle(Action::Apply(field)));
}

#[test]
fn fitted_numeric_and_drag_tilt_keep_the_raw_family_through_a_half_turn() {
    use super::super::numeric_transforms::{Action as NumericAction, Field as NumericField};
    use crate::canvas::{Edit, Tool, tilt};
    for around_x in [true, false] {
        for numeric in [true, false] {
            let (mut app, id) = fitted_cycle();
            app.tab.selected = app.tab.doc.all_ids();
            if !numeric {
                let _ = app.update(Message::Tool(Tool::Tilt));
            }
            app.sync_numeric_transforms();
            let before = app.tab.doc.clone();
            let ids = app.tab.selected.clone();
            let mut preview = before.clone();
            let (x, y) = if around_x { (180., 0.) } else { (0., 180.) };
            tilt::apply(&mut preview, &ids, x, y);
            assert_ne!(preview, before, "the requested half turn must run");
            preview.validate().unwrap();
            let captured = before.graphics[0]
                .cycle
                .as_ref()
                .unwrap()
                .split_runs
                .clone();
            let raw = preview.graphics[0]
                .cycle
                .as_ref()
                .unwrap()
                .split_runs
                .clone();
            assert_eq!(raw.len(), captured.len());
            for (old, new) in captured.iter().zip(&raw) {
                assert_eq!(new.source_span, -old.source_span);
                assert_eq!(new.cuts, old.cuts);
            }
            reconcile_preview(&mut preview, &before, app.appearance.two_decimal_weight).unwrap();
            assert_eq!(
                preview.graphics[0].cycle.as_ref().unwrap().split_runs,
                raw,
                "the raw candidate is already fitted; reconciliation must not retire it"
            );
            let frames = app.tab.history.undo_frames().len();
            if numeric {
                let field = if around_x {
                    NumericField::TiltX
                } else {
                    NumericField::TiltY
                };
                let _ = app.update(Message::NumericTransform(NumericAction::Input(
                    field,
                    "180".into(),
                )));
                let _ = app.update(Message::NumericTransform(NumericAction::Apply(field)));
            } else {
                let _ = app.update(Message::Canvas(Edit::Tilt { ids, x, y }));
            }
            assert!(!app.error, "{numeric}, {around_x}: {}", app.status);
            assert_eq!(
                app.tab.doc, preview,
                "numeric and drag commit the fitted candidate"
            );
            assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
            let g = app.tab.doc.graphics.iter().find(|g| g.id == id).unwrap();
            let drawn = cycles::drawn(&app.tab.doc, g);
            let c = g.cycle.as_ref().unwrap();
            for run in &c.split_runs {
                for child in 0..=run.cuts.len() {
                    let step = (run.first_step + child) % c.steps.len();
                    assert!(
                        drawn.arcs.contains(&Some(step)),
                        "every fitted child stays visible"
                    );
                }
            }
            assert_eq!(
                Document::from_native_file(&preview.file_json().unwrap()).unwrap(),
                preview
            );
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, preview);
        }
    }
}

#[test]
fn fitted_numeric_and_drag_edge_on_tilt_decline_without_retiring_sources_or_history() {
    use super::super::numeric_transforms::{Action as NumericAction, Field as NumericField};
    use crate::canvas::{Edit, Tool, tilt};
    for numeric in [true, false] {
        let (mut app, id) = fitted_cycle();
        if !numeric {
            let _ = app.update(Message::Tool(Tool::Tilt));
        }
        let center = reshiki::editing::center(&app.tab.doc, &[id]);
        let cos = 90_f32.to_radians().cos();
        let g = app
            .tab
            .doc
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .unwrap();
        g.depth = [
            (g.origin.y - center.y) * cos,
            g.axis_x.y * cos,
            g.axis_y.y * cos,
        ];
        app.tab.doc.validate().unwrap();
        app.sync_numeric_transforms();
        let before = app.tab.doc.clone();
        let mut preview = before.clone();
        tilt::apply(&mut preview, &[id], 90., 0.);
        assert_eq!(
            preview, before,
            "the complete edge-on candidate is declined"
        );
        if numeric {
            let _ = app.update(Message::NumericTransform(NumericAction::Input(
                NumericField::TiltX,
                "90".into(),
            )));
            let _ = app.update(Message::NumericTransform(NumericAction::Apply(
                NumericField::TiltX,
            )));
        } else {
            let _ = app.update(Message::Canvas(Edit::Tilt {
                ids: vec![id],
                x: 90.,
                y: 0.,
            }));
        }
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        app.tab.doc.validate().unwrap();
    }
}

#[test]
fn a_width_edit_with_a_fitted_cut_reviews_and_applies_the_exact_snapshot_once() {
    let (mut app, _) = fitted_cycle();
    let before = app.tab.doc.clone();
    let frames = app.tab.history.undo_frames().len();
    field(&mut app, Field::Width, "800");
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.undo_frames().len(), frames);
    let proposal = app
        .tab
        .cycle_inputs
        .proposal
        .as_ref()
        .expect("review moved cuts");
    assert!(matches!(proposal.kind, ProposalKind::Refit { .. }));
    let preview = app.display_document().clone();
    assert_ne!(preview.atoms, before.atoms);
    assert!(app.cycle_proposal_card().is_some());
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
}

#[test]
fn a_shape_edit_with_a_fitted_cut_can_be_discarded_without_history() {
    let (mut app, id) = fitted_cycle();
    let before = app.tab.doc.clone();
    // Circle retires the shaped source and transports its cut, requiring
    // review; a compatible shaped outline can preserve the cut in place.
    let _ = app.update(Message::Cycle(Action::Shape(Shape::Circle)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, before);
    let preview = app.display_document();
    let g = preview.graphics.iter().find(|g| g.id == id).unwrap();
    assert_eq!(g.cycle.as_ref().unwrap().shape, Shape::Circle);
    assert_ne!(preview.atoms, before.atoms);
    assert!(app.tab.cycle_inputs.proposal.is_some());
    let _ = app.update(Message::Escape);
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn refit_apply_refuses_a_changed_baseline_or_label_generation() {
    for labels_only in [false, true] {
        let (mut app, _) = fitted_cycle();
        field(&mut app, Field::Width, "800");
        assert!(app.tab.cycle_inputs.proposal.is_some(), "{}", app.status);
        if labels_only {
            app.tab.label_generation += 1;
        } else {
            app.tab.doc.atoms[0].position = app.tab.doc.atoms[0].position.offset(7., 11.);
        }
        let current = app.tab.doc.clone();
        let frames = app.tab.history.undo_frames().len();
        let _ = app.update(Message::ContextKey("Enter".into()));
        assert!(app.error);
        assert!(app.status.contains("Drawing changed"), "{}", app.status);
        assert!(app.tab.cycle_inputs.proposal.is_none());
        assert_eq!(app.tab.doc, current);
        assert_eq!(app.tab.history.undo_frames().len(), frames);
    }
}

#[test]
fn fitted_cycle_labels_keep_their_existing_immediate_edit_behavior() {
    let (mut app, _) = fitted_cycle();
    let before = app.tab.doc.clone();
    field(&mut app, Field::Label, "Catalytic cycle");
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.doc.atoms, before.atoms);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn an_independent_child_bow_edit_uses_the_model_refit_candidate() {
    let (mut app, id) = fitted_cycle();
    let before = app.tab.doc.clone();
    let step = before.graphics[0].cycle.as_ref().unwrap().split_runs[0].first_step;
    app.tab.cycle_inputs.step = step;
    app.sync_cycle();
    let mut expected = before.clone();
    expected.graphics[0].cycle.as_mut().unwrap().steps[step].bow_pt = 72.;
    let review = cycles::fit_edited(&before, &mut expected, id, Some(step)).unwrap();
    reconcile_preview(&mut expected, &before, app.appearance.two_decimal_weight).unwrap();
    field(&mut app, Field::Bow, "72");
    assert!(!app.error, "{}", app.status);
    if review {
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.display_document(), &expected);
        let _ = app.update(Message::ContextKey("Enter".into()));
    } else {
        assert!(app.tab.cycle_inputs.proposal.is_none());
    }
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, expected);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn refit_apply_preserves_a_stationary_name_that_the_preview_repositions() {
    let (mut app, _) = fitted_cycle();
    let a = app
        .tab
        .doc
        .add_atom("C", reshiki::document::Point::new(3000., -1500.));
    let b = app
        .tab
        .doc
        .add_atom("C", reshiki::document::Point::new(3042., -1500.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let name = reshiki::molecule_names::show(
        &mut app.tab.doc,
        &[a, b],
        "ethane".into(),
        "CC".into(),
        Default::default(),
        false,
    )
    .unwrap();
    let caption = app
        .tab
        .doc
        .annotations
        .iter_mut()
        .find(|item| item.id == name)
        .unwrap();
    caption.position = caption.position.offset(13., 17.);
    let before = app.tab.doc.clone();
    field(&mut app, Field::Width, "800");
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.cycle_inputs.proposal.is_some());
    let preview = app.display_document().clone();
    assert_eq!(preview.atom(a), before.atom(a));
    assert_eq!(preview.atom(b), before.atom(b));
    assert_eq!(preview.molecule_names, before.molecule_names);
    assert_ne!(
        preview
            .annotations
            .iter()
            .find(|item| item.id == name)
            .unwrap()
            .position,
        before
            .annotations
            .iter()
            .find(|item| item.id == name)
            .unwrap()
            .position
    );
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.doc.molecule_names, before.molecule_names);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn refit_apply_refuses_a_precision_change_during_review() {
    let (mut app, _) = fitted_cycle();
    field(&mut app, Field::Width, "800");
    assert!(app.tab.cycle_inputs.proposal.is_some(), "{}", app.status);
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let two_decimal_weight = !app.appearance.two_decimal_weight;
    let _ = app.update(Message::View(
        super::super::view_settings::Action::TwoDecimalWeight(two_decimal_weight),
    ));
    assert_eq!(app.appearance.two_decimal_weight, two_decimal_weight);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    let _ = app.update(Message::ContextKey("Enter".into()));
    assert!(app.error);
    assert!(app.status.contains("Drawing changed"), "{}", app.status);
    assert!(app.tab.cycle_inputs.proposal.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}
