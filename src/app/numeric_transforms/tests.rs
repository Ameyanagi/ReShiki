use super::*;
use reshiki::{
    document::{Annotation, Arrow},
    engine::{ChemistryEngine, LocalEngine, Request},
    graphics::{Graphic, GraphicKind},
};

fn transformed(
    source: &Document,
    selected: &[u64],
    field: Field,
    input: &str,
    lock: bool,
) -> Result<Document, String> {
    let mut candidate = source.clone();
    transform_candidate(&mut candidate, selected, field, input, lock)?;
    Ok(candidate)
}

pub(super) fn fixture() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.tab.doc.atoms[0].element = "N".into();
    app.tab.doc.annotations.push(Annotation {
        id: app.tab.doc.next_id(),
        position: Point::new(95., 60.),
        text: "Fixed text".into(),
        format: Default::default(),
    });
    app.tab.doc.arrows.push(Arrow::new(
        app.tab.doc.next_id(),
        Point::new(90., 0.),
        Point::new(180., 0.),
        Default::default(),
        Default::default(),
    ));
    app.tab.doc.graphics.push(Graphic::dragged(
        app.tab.doc.next_id(),
        GraphicKind::Rectangle,
        Point::new(210., -30.),
        Point::new(260., 45.),
        Default::default(),
        Default::default(),
        false,
    ));
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.doc.group_selection(&app.tab.selected).unwrap();
    app.tab.doc.add_atom("O", Point::new(400., 150.));
    app.sync_numeric_transforms();
    app
}

fn apply(app: &mut App, field: Field, input: impl ToString) {
    let _ = app.update(Message::NumericTransform(Action::Input(
        field,
        input.to_string(),
    )));
    let _ = app.update(Message::NumericTransform(Action::Apply(field)));
}

#[test]
fn handle_shortcut_reveals_properties_without_applying_or_discarding_drafts() {
    use super::super::inspector::{Action as InspectorAction, Section};
    use crate::canvas::{Edit, TransformField};
    for target in [
        TransformField::Rotation,
        TransformField::Scale,
        TransformField::Width,
        TransformField::Height,
    ] {
        let mut app = fixture();
        apply(&mut app, Field::Rotation, "15");
        let _ = app.update(Message::Undo);
        input(&mut app, Field::Rotation, "-");
        input(&mut app, Field::Scale, "125");
        app.inspector_open = false;
        app.inspector_tab = super::super::InspectorTab::Export;
        app.tab
            .inspector_ui
            .update(InspectorAction::Section(Section::Transform, false));
        let before = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let revision = app.tab.revision;
        let _ = app.update(Message::Canvas(Edit::BeginTransform(target)));
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, super::super::InspectorTab::Properties);
        assert_eq!(
            app.tab.inspector_ui.expanded(Section::Transform),
            Some(true)
        );
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());
        assert_eq!(app.tab.numeric_transforms.rotation, "-");
        assert_eq!(app.tab.numeric_transforms.scale, "125");
    }
    let mut app = fixture();
    let id = app.tab.doc.annotations[0].id;
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::Begin(Some(id), Point::default()),
    ));
    let _ = app.update(Message::CaptionAction(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
            " draft".to_owned().into(),
        )),
    ));
    let before = app.tab.doc.clone();
    let caption = app.tab.caption.clone();
    let _ = app.update(Message::Canvas(Edit::BeginTransform(
        TransformField::Rotation,
    )));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.caption, caption);
    assert!(app.tab.inline_text.is_some());
    assert!(!app.tab.history.can_undo());
}

#[test]
fn numeric_edits_are_atomic_and_preserve_mixed_group_styles_and_unselected_objects() {
    for (field, input) in [
        (Field::Rotation, "72"),
        (Field::TiltX, "25.5"),
        (Field::TiltY, "-33.25"),
        (Field::Width, "160"),
        (Field::Height, "90"),
        (Field::Scale, "125"),
    ] {
        let mut app = fixture();
        let original = app.tab.doc.clone();
        let ids = app.tab.selected.clone();
        apply(&mut app, field, input);
        assert!(!app.error, "{field:?}: {}", app.status);
        let changed = app.tab.doc.clone();
        assert_ne!(changed, original);
        assert_eq!(changed.atoms.last(), original.atoms.last());
        assert_eq!(changed.bonds, original.bonds);
        assert_eq!(changed.groups, original.groups);
        assert_eq!(
            changed.annotations[0].format,
            original.annotations[0].format
        );
        assert_eq!(changed.arrows[0].style, original.arrows[0].style);
        assert_eq!(changed.graphics[0].style, original.graphics[0].style);
        assert_eq!(changed.drawing_style, original.drawing_style);
        assert_eq!(app.tab.selected, ids);
        let reopened: Document =
            serde_json::from_slice(&serde_json::to_vec(&changed).unwrap()).unwrap();
        assert_eq!(reopened, changed);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        assert!(
            !app.tab.history.can_undo(),
            "Each Apply is exactly one edit"
        );
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, changed);
    }
}

#[test]
fn apply_button_applies_every_edited_field_as_one_undo_step() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    let _ = app.update(Message::NumericTransform(Action::ApplyAll));
    assert!(!app.error);
    assert_eq!(app.tab.doc, original, "Nothing edited is a no-op");
    assert!(!app.tab.history.can_undo());
    let mut sequential = fixture();
    apply(&mut sequential, Field::Rotation, "72");
    apply(&mut sequential, Field::Scale, "125");
    for (field, input) in [(Field::Scale, "125"), (Field::Rotation, "72")] {
        let _ = app.update(Message::NumericTransform(Action::Input(
            field,
            input.into(),
        )));
    }
    let _ = app.update(Message::NumericTransform(Action::ApplyAll));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.tab.doc, sequential.tab.doc,
        "Rotation applies before scale"
    );
    let changed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo(), "One Apply is one Undo step");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, changed);
}

#[test]
fn apply_button_rejects_all_edits_when_one_field_is_invalid() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    let _ = app.update(Message::NumericTransform(Action::More(true)));
    for (field, input) in [(Field::Rotation, "72"), (Field::TiltX, "90")] {
        let _ = app.update(Message::NumericTransform(Action::Input(
            field,
            input.into(),
        )));
    }
    let _ = app.update(Message::NumericTransform(Action::ApplyAll));
    assert!(app.error);
    assert!(app.status.starts_with("Tilt X: "), "{}", app.status);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    assert_eq!(
        app.tab.numeric_transforms.rotation, "72",
        "Typed values stay"
    );
    app.tab.selected = vec![app.tab.doc.graphics[0].id];
    let _ = app.update(Message::Tick);
    assert_eq!(app.tab.numeric_transforms.rotation, "0");
    assert!(
        app.tab.numeric_transforms.more,
        "More stays open across selections"
    );
}

fn input(app: &mut App, field: Field, value: &str) {
    let _ = app.update(Message::NumericTransform(Action::Input(
        field,
        value.into(),
    )));
}

#[test]
fn enter_applies_its_field_and_keeps_values_typed_in_others() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    for (field, value) in [
        (Field::Rotation, "72"),
        (Field::Scale, "125"),
        (Field::Width, "160"),
    ] {
        input(&mut app, field, value);
    }
    let _ = app.update(Message::NumericTransform(Action::Apply(Field::Rotation)));
    assert!(!app.error, "{}", app.status);
    let state = &app.tab.numeric_transforms;
    assert_eq!(state.rotation, "0");
    assert_eq!((state.scale.as_str(), state.width.as_str()), ("125", "160"));
    assert_ne!(state.dimensions.0, "160");
    // Enter on an unchanged field keeps them too.
    let _ = app.update(Message::NumericTransform(Action::Apply(Field::Rotation)));
    assert_eq!(app.tab.numeric_transforms.scale, "125");
    let _ = app.update(Message::NumericTransform(Action::Apply(Field::Scale)));
    let _ = app.update(Message::NumericTransform(Action::Apply(Field::Width)));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.tab.numeric_transforms.width,
        app.tab.numeric_transforms.dimensions.0
    );
    assert_eq!(app.tab.numeric_transforms.width, "160.00");
    for _ in 0..3 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo(), "Each Enter is one Undo step");
}

#[test]
fn locked_apply_uses_the_size_edited_last_and_clears_the_other() {
    for last in [Field::Width, Field::Height] {
        let mut app = fixture();
        let (width, height) = (
            extent(&app.tab.doc, &app.tab.selected, Field::Width).unwrap(),
            extent(&app.tab.doc, &app.tab.selected, Field::Height).unwrap(),
        );
        let first = if last == Field::Width {
            Field::Height
        } else {
            Field::Width
        };
        let target = |field| if field == Field::Width { width } else { height } * 1.5;
        input(&mut app, first, &format!("{:.2}", target(first) * 2.));
        input(&mut app, last, &format!("{:.2}", target(last)));
        let _ = app.update(Message::NumericTransform(Action::ApplyAll));
        assert!(!app.error, "{}", app.status);
        let reached = extent(&app.tab.doc, &app.tab.selected, last).unwrap();
        assert!((reached - target(last)).abs() < 0.01, "{last:?}: {reached}");
        let state = &app.tab.numeric_transforms;
        assert_eq!(state.width, state.dimensions.0);
        assert_eq!(state.height, state.dimensions.1);
        let _ = app.update(Message::Undo);
        assert!(!app.tab.history.can_undo(), "One Apply is one Undo step");
    }
    // Unlocked, both sizes apply.
    let mut app = fixture();
    let _ = app.update(Message::NumericTransform(Action::Proportional(false)));
    input(&mut app, Field::Width, "150");
    input(&mut app, Field::Height, "90");
    assert_eq!(
        app.tab.numeric_transforms.to_apply(),
        [Field::Width, Field::Height]
    );
}

#[test]
fn apply_includes_tilt_typed_before_more_was_closed() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    let mut sequential = fixture();
    apply(&mut sequential, Field::Rotation, "72");
    apply(&mut sequential, Field::TiltX, "20");
    let _ = app.update(Message::NumericTransform(Action::More(true)));
    input(&mut app, Field::Rotation, "72");
    input(&mut app, Field::TiltX, "20");
    let _ = app.update(Message::NumericTransform(Action::More(false)));
    assert_eq!(
        app.tab.numeric_transforms.to_apply(),
        [Field::Rotation, Field::TiltX]
    );
    let _ = app.update(Message::NumericTransform(Action::ApplyAll));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, sequential.tab.doc);
    assert_eq!(app.tab.numeric_transforms.tilt_x, "0");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn point_dimensions_account_for_fixed_labels_and_lock_only_coordinate_proportions() {
    for lock in [true, false] {
        for field in [Field::Width, Field::Height] {
            let mut app = fixture();
            let original = app.tab.doc.clone();
            let current = extent(&app.tab.doc, &app.tab.selected, field).unwrap();
            let target = current * 1.65;
            let _ = app.update(Message::NumericTransform(Action::Proportional(lock)));
            apply(&mut app, field, target);
            assert!(!app.error, "{}", app.status);
            assert!(
                (extent(&app.tab.doc, &app.tab.selected, field).unwrap() - target).abs() < 0.005
            );
            let old = &original.graphics[0];
            let new = &app.tab.doc.graphics[0];
            let x_scale = new.axis_x.x / old.axis_x.x;
            let y_scale = new.axis_y.y / old.axis_y.y;
            if lock {
                assert!((x_scale - y_scale).abs() < 0.0001);
            } else if field == Field::Width {
                assert_eq!(old.axis_y, new.axis_y);
            } else {
                assert_eq!(old.axis_x, new.axis_x);
            }
        }
    }
}

#[test]
fn invalid_and_noop_values_do_not_mutate_or_consume_history() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    for field in Field::ALL {
        for input in ["", "-", "hello", "NaN", "inf", "-inf", "1e40"] {
            apply(&mut app, field, input);
            assert!(app.error, "{field:?} accepted {input}");
            assert_eq!(app.tab.doc, original);
            assert!(!app.tab.history.can_undo());
        }
    }
    for (field, input) in [
        (Field::Rotation, "36001"),
        (Field::TiltX, "86"),
        (Field::TiltY, "-86"),
        (Field::Width, "0"),
        (Field::Height, "-1"),
        (Field::Scale, "0"),
        (Field::Scale, "-100"),
        (Field::Scale, "1000001"),
    ] {
        apply(&mut app, field, input);
        assert!(app.error);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }
    for (field, input) in [
        (Field::Rotation, "360"),
        (Field::Rotation, "-720"),
        (Field::TiltX, "0"),
        (Field::TiltY, "0"),
        (Field::Scale, "100"),
    ] {
        apply(&mut app, field, input);
        assert!(!app.error);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }
    app.tab.numeric_transforms.key = None;
    app.sync_numeric_transforms();
    for field in [Field::Width, Field::Height] {
        let _ = app.update(Message::NumericTransform(Action::Apply(field)));
        assert_eq!(app.tab.doc, original, "Displayed rounded sizes are no-ops");
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn numeric_apply_preserves_caption_drafts_until_a_valid_change() {
    use iced::widget::text_editor::{Action as TextAction, Edit as TextEdit};
    for (field, input, invalid) in [
        (Field::Width, "NaN", true),
        (Field::Width, "0", true),
        (Field::Width, "1000", true),
        (Field::Scale, "100", false),
        (Field::Rotation, "360", false),
    ] {
        let mut app = fixture();
        let original = app.tab.doc.clone();
        apply(&mut app, Field::Scale, "125");
        let redo = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        let id = app.tab.doc.annotations[0].id;
        let _ = app.update(Message::InlineText(
            super::super::inline_text::Action::Begin(Some(id), Point::default()),
        ));
        let _ = app.update(Message::CaptionAction(TextAction::Edit(TextEdit::Paste(
            " draft".to_owned().into(),
        ))));
        let draft = app.tab.caption.clone();
        let format = app.tab.caption_format.clone();
        let revision = app.tab.revision;
        let draft_history = app.text_history_available(false);
        apply(&mut app, field, input);
        assert_eq!(app.error, invalid, "{field:?}: {}", app.status);
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.caption, draft);
        assert_eq!(app.tab.caption_editor.text(), draft);
        assert_eq!(app.tab.caption_format, format);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.text_history_available(false), draft_history);
        assert!(app.tab.inline_text.is_some());
        assert_eq!(app.tab.selected, vec![id]);
        assert!(!app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());
        let _ = app.update(Message::InlineText(
            super::super::inline_text::Action::Finish(false),
        ));
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, redo);
    }
    let mut app = fixture();
    let original = app.tab.doc.clone();
    let id = app.tab.doc.annotations[0].id;
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::Begin(Some(id), Point::default()),
    ));
    let _ = app.update(Message::CaptionAction(TextAction::Edit(TextEdit::Paste(
        " draft".to_owned().into(),
    ))));
    let mut committed = original.clone();
    committed.annotations[0].text = app.tab.caption.clone();
    committed.annotations[0].format = app.tab.caption_format.clone();
    apply(&mut app, Field::Scale, "125");
    assert!(!app.error, "{}", app.status);
    assert!(app.tab.inline_text.is_none());
    assert_eq!(
        app.tab.doc.annotations[0].text,
        committed.annotations[0].text
    );
    assert_ne!(
        app.tab.doc.annotations[0].position,
        committed.annotations[0].position
    );
    let transformed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc, committed,
        "Transform is one Undo step after committing the caption"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, transformed);
}

#[test]
fn degenerate_sizes_and_fixed_text_report_unreachable_targets_without_changes() {
    for vertical in [true, false] {
        let (mut app, _) = App::new();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom(
            "C",
            if vertical {
                Point::new(0., 42.)
            } else {
                Point::new(42., 0.)
            },
        );
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.selected = vec![a, b];
        app.sync_numeric_transforms();
        let original = app.tab.doc.clone();
        apply(
            &mut app,
            if vertical {
                Field::Width
            } else {
                Field::Height
            },
            "10",
        );
        assert!(app.error);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }
    let mut app = fixture();
    app.tab.selected = vec![app.tab.doc.annotations[0].id];
    app.sync_numeric_transforms();
    let original = app.tab.doc.clone();
    for field in [Field::Width, Field::Height] {
        apply(&mut app, field, "100");
        assert!(app.error);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
    }
    app.tab.selected.clear();
    app.sync_numeric_transforms();
    apply(&mut app, Field::Rotation, "72");
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn pending_numbers_reset_with_selection_and_undo_but_typing_does_not_edit() {
    let mut app = fixture();
    let original = app.tab.doc.clone();
    let _ = app.update(Message::NumericTransform(Action::Input(
        Field::Width,
        "160".into(),
    )));
    assert_eq!(app.tab.numeric_transforms.width, "160");
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    app.tab.selected = vec![app.tab.doc.graphics[0].id];
    let _ = app.update(Message::Tick);
    assert_ne!(app.tab.numeric_transforms.width, "160");
    let old_width = app.tab.numeric_transforms.width.clone();
    apply(&mut app, Field::Scale, "200");
    assert_eq!(app.tab.numeric_transforms.scale, "100");
    assert_ne!(app.tab.numeric_transforms.width, old_width);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.numeric_transforms.width, old_width);
}

#[test]
fn computed_label_refresh_updates_dimensions_without_discarding_pending_input() {
    let (mut app, _) = App::new();
    let oxygen = app.tab.doc.add_atom("O", Point::default());
    app.tab.selected = vec![oxygen];
    app.sync_numeric_transforms();
    let old_width = app.tab.numeric_transforms.width.clone();
    let _ = app.update(Message::NumericTransform(Action::Input(
        Field::Height,
        "65".into(),
    )));
    let _ = app.update(Message::NumericTransform(Action::Input(
        Field::Rotation,
        "72".into(),
    )));
    let mut checked = app.tab.doc.clone();
    checked.atom_mut(oxygen).unwrap().label_h = 2;
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: super::super::Job::Analyze,
        result: Box::new(Ok(reshiki::engine::Response {
            document: Some(checked),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        })),
    });
    assert_ne!(app.tab.numeric_transforms.width, old_width);
    assert_eq!(app.tab.numeric_transforms.height, "65");
    assert_eq!(app.tab.numeric_transforms.rotation, "72");
    assert!(!app.tab.history.can_undo());
}

#[test]
fn collapsed_abbreviations_keep_hidden_atoms_and_projection_depth_in_scale() {
    let (mut app, _) = App::new();
    let n = app.tab.doc.add_atom("N", Point::new(-42., 0.));
    let c = app.tab.doc.add_atom("C", Point::default());
    app.tab.doc.add_bond(n, c, 1, "plain");
    app.tab.doc =
        reshiki::atom_text::apply(&app.tab.doc, c, "Boc", reshiki::atom_text::Mode::Auto).unwrap();
    let ids = app.tab.doc.all_ids();
    reshiki::projection::tilt(&mut app.tab.doc, &ids, 25., true);
    app.tab.selected = vec![n, c];
    app.sync_numeric_transforms();
    let original = app.tab.doc.clone();
    apply(&mut app, Field::Scale, "150");
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert_eq!(app.tab.doc.abbreviations, original.abbreviations);
    for (now, old) in app.tab.doc.atoms.iter().zip(&original.atoms) {
        assert!((now.depth - old.depth * 1.5).abs() < 0.0001);
    }
    let changed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    apply(&mut app, Field::Scale, "NaN");
    apply(&mut app, Field::Rotation, "360");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, changed, "Invalid and no-op edits retain redo");
}

#[test]
fn crossed_fixed_labels_can_reach_smaller_bounds_without_collapsing() {
    use reshiki::atom_labels::HydrogenPosition;
    let mut doc = Document::default();
    let left = doc.add_atom("O", Point::new(-18., 0.));
    let right = doc.add_atom("O", Point::new(18., 0.));
    doc.atom_mut(left).unwrap().label_h = 1;
    doc.atom_mut(left).unwrap().display.hydrogen_position = HydrogenPosition::Right;
    doc.atom_mut(right).unwrap().label_h = 1;
    doc.atom_mut(right).unwrap().display.hydrogen_position = HydrogenPosition::Left;
    let ids = vec![left, right];
    let mut target_doc = doc.clone();
    editing::scale_axes_about(&mut target_doc, &ids, Point::default(), 0.65, 1.);
    let target = extent(&target_doc, &ids, Field::Width).unwrap();
    let changed = transformed(&doc, &ids, Field::Width, &target.to_string(), false).unwrap();
    assert!((extent(&changed, &ids, Field::Width).unwrap() - target).abs() < 0.005);
}

#[tokio::test]
async fn exact_numeric_transforms_keep_tetrahedral_and_double_bond_identity() {
    let engine = LocalEngine::default();
    let original = engine
        .execute(Request::import_smiles("C[C@H](O)/C=C/F"))
        .await
        .unwrap()
        .document
        .unwrap();
    let expected = engine
        .execute(Request::molecule("analyze", original.clone()))
        .await
        .unwrap()
        .analysis
        .unwrap();
    let (mut app, _) = App::new();
    app.tab.doc = original.clone();
    app.tab.selected = app.tab.doc.all_ids();
    app.sync_numeric_transforms();
    for (field, input) in [
        (Field::Rotation, "72"),
        (Field::TiltX, "21.5"),
        (Field::TiltY, "-17.25"),
        (Field::Scale, "125"),
        (Field::Width, "100"),
        (Field::Height, "65"),
    ] {
        let _ = app.update(Message::NumericTransform(Action::Proportional(false)));
        apply(&mut app, field, input);
        assert!(!app.error, "{}", app.status);
        let bonds_without_computed_labels = |doc: &Document| {
            doc.bonds
                .iter()
                .cloned()
                .map(|mut bond| {
                    bond.cip_label = None;
                    bond
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            bonds_without_computed_labels(&app.tab.doc),
            bonds_without_computed_labels(&original)
        );
        for (now, old) in app.tab.doc.atoms.iter().zip(&original.atoms) {
            assert_eq!(now.stereo, old.stereo);
        }
        let actual = engine
            .execute(Request::molecule("analyze", app.tab.doc.clone()))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(actual.inchikey, expected.inchikey, "{field:?}");
        assert_eq!(actual.formula, expected.formula);
    }
}

#[tokio::test]
#[ignore = "Manual GPU input and layout check; writes actual renderer evidence"]
async fn numeric_panel_layout_input_and_renderer_evidence() {
    use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
    use iced::keyboard::{self, Key, key};
    let directory = std::env::temp_dir().join("reshiki-numeric-transforms-qa");
    std::fs::create_dir_all(&directory).unwrap();
    let mut app = fixture();
    // Tilt shown, and an edit that enables Apply.
    app.tab.numeric_transforms.more = true;
    app.tab.numeric_transforms.scale = "125".into();
    std::fs::write(
        directory.join("numeric-transforms.rsk"),
        serde_json::to_vec_pretty(&app.tab.doc).unwrap(),
    )
    .unwrap();
    for width in [246, 268] {
        let height = 460;
        let size = iced::Size::new(width as f32, height as f32);
        let mut renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        let mut view = app.numeric_transform_panel();
        let mut tree = Tree::new(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(iced::Size::ZERO, size),
        );
        assert!(node.size().height <= size.height);
        let layout = iced::advanced::Layout::new(&node);
        fn inside(layout: iced::advanced::Layout<'_>, width: f32) {
            let bounds = layout.bounds();
            assert!(bounds.x >= 0. && bounds.x + bounds.width <= width + 0.01);
            layout.children().for_each(|child| inside(child, width));
        }
        inside(layout, width as f32);
        // Rotate | Scale, W | 🔒 H, Tilt X | Tilt Y, then More … Apply.
        let rows: Vec<_> = layout.children().collect();
        let input = |row: usize, column: usize| {
            let cell = rows[row].children().nth(column).unwrap();
            let stack = cell.children().nth(1).unwrap();
            stack.children().next().unwrap().bounds()
        };
        let fields = [
            (Field::Rotation, input(0, 0)),
            (Field::Scale, input(0, 1)),
            (Field::Width, input(1, 0)),
            (Field::Height, input(1, 1)),
            (Field::TiltX, input(2, 0)),
            (Field::TiltY, input(2, 1)),
        ];
        fn first(layout: iced::advanced::Layout<'_>) -> iced::advanced::Layout<'_> {
            layout.children().next().unwrap()
        }
        // The lock leads the H label: cell → label container → row → button.
        let lock = first(first(first(rows[1].children().nth(1).unwrap()))).bounds();
        let more = rows[3].children().next().unwrap().bounds();
        let apply_all = rows[3].children().nth(2).unwrap().bounds();
        let mut messages = Vec::new();
        let mut event = |event, cursor| {
            view.as_widget_mut().update(
                &mut tree,
                &event,
                layout,
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
        };
        event(
            iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            mouse::Cursor::Unavailable,
        );
        let click = |event: &mut dyn FnMut(iced::Event, mouse::Cursor), bounds: iced::Rectangle| {
            let cursor = mouse::Cursor::Available(bounds.center());
            event(
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                cursor,
            );
            event(
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                cursor,
            );
        };
        let press = |event: &mut dyn FnMut(iced::Event, mouse::Cursor),
                     key: Key,
                     physical_key,
                     modifiers,
                     text| {
            event(
                iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
                mouse::Cursor::Unavailable,
            );
            event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    modified_key: key.clone(),
                    key,
                    physical_key: key::Physical::Code(physical_key),
                    location: keyboard::Location::Standard,
                    modifiers,
                    text,
                    repeat: false,
                }),
                mouse::Cursor::Unavailable,
            );
        };
        let enter = |event: &mut dyn FnMut(iced::Event, mouse::Cursor)| {
            press(
                event,
                Key::Named(key::Named::Enter),
                key::Code::Enter,
                keyboard::Modifiers::empty(),
                None,
            )
        };
        click(&mut event, fields[0].1);
        press(
            &mut event,
            Key::Character("a".into()),
            key::Code::KeyA,
            keyboard::Modifiers::COMMAND,
            None,
        );
        press(
            &mut event,
            Key::Character("7".into()),
            key::Code::Digit7,
            keyboard::Modifiers::empty(),
            Some("7".into()),
        );
        press(
            &mut event,
            Key::Character("2".into()),
            key::Code::Digit2,
            keyboard::Modifiers::empty(),
            Some("2".into()),
        );
        for (_, bounds) in fields {
            click(&mut event, bounds);
            enter(&mut event);
        }
        for bounds in [lock, more, apply_all] {
            click(&mut event, bounds);
        }
        assert!(messages.iter().any(|m| matches!(m, Message::NumericTransform(Action::Input(Field::Rotation, value)) if value == "72")), "{messages:?}");
        for field in Field::ALL {
            assert!(messages.iter().any(|m| matches!(m, Message::NumericTransform(Action::Apply(actual)) if *actual == field)), "Enter in {field:?}: {messages:?}");
        }
        let sent = |wanted: fn(&Action) -> bool| {
            messages
                .iter()
                .any(|m| matches!(m, Message::NumericTransform(action) if wanted(action)))
        };
        assert!(sent(|a| matches!(a, Action::Proportional(false))));
        assert!(sent(|a| matches!(a, Action::More(false))));
        assert!(sent(|a| matches!(a, Action::ApplyAll)), "{messages:?}");
        let theme = app.theme();
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style::default(),
            layout,
            mouse::Cursor::Unavailable,
            &iced::Rectangle::with_size(size),
        );
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(width, height),
            1.,
            theme.palette().background,
        );
        image::save_buffer(
            directory.join(format!("numeric-panel-{width}.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    let original = app.tab.doc.clone();
    apply(&mut app, Field::Rotation, "72");
    apply(&mut app, Field::Scale, "125");
    for (name, doc) in [
        ("original", original),
        ("rotated-scaled", app.tab.doc.clone()),
    ] {
        std::fs::write(
            directory.join(format!("{name}.png")),
            reshiki::export::drawing(&doc, "png").unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "Manual explicit-Apply latency check on a 1000-atom document"]
fn numeric_size_apply_latency() {
    let mut doc = Document::default();
    for index in 0..1000 {
        let id = doc.add_atom(
            "C",
            Point::new(index as f32 * 36., (index % 2) as f32 * 21.),
        );
        if index > 0 {
            doc.add_bond(id - 1, id, 1, "plain");
        }
    }
    let all = doc.all_ids();
    for (name, ids) in [
        ("small selection", all[..6].to_vec()),
        ("whole selection", all),
    ] {
        let target = extent(&doc, &ids, Field::Width).unwrap() * 1.5;
        let start = std::time::Instant::now();
        let changed = transformed(&doc, &ids, Field::Width, &target.to_string(), false).unwrap();
        println!("1000 atoms, {name}: {:?}", start.elapsed());
        assert!((extent(&changed, &ids, Field::Width).unwrap() - target).abs() < 0.005);
    }
    let left = doc.add_atom("O", Point::new(-18., -100.));
    let right = doc.add_atom("O", Point::new(18., -100.));
    for (id, position) in [
        (left, reshiki::atom_labels::HydrogenPosition::Right),
        (right, reshiki::atom_labels::HydrogenPosition::Left),
    ] {
        let atom = doc.atom_mut(id).unwrap();
        atom.label_h = 1;
        atom.display.hydrogen_position = position;
    }
    let ids = [left, right];
    let mut sample = doc.clone();
    editing::scale_axes_about(&mut sample, &ids, Point::new(0., -100.), 0.65, 1.);
    let target = extent(&sample, &ids, Field::Width).unwrap();
    let start = std::time::Instant::now();
    let changed = transformed(&doc, &ids, Field::Width, &target.to_string(), false).unwrap();
    println!("1002 atoms, inward-label fallback: {:?}", start.elapsed());
    assert!((extent(&changed, &ids, Field::Width).unwrap() - target).abs() < 0.005);
}
