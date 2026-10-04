//! Opt-in transcripts and workloads for comparing candidate ownership changes.
use super::*;
use serde_json::{Value, json};

fn state(app: &App) -> Value {
    json!({
        "document": app.tab.doc,
        "selection": app.tab.selected,
        "revision": app.tab.revision,
        "error": app.error,
        "status": app.status,
        "caption": app.tab.caption,
        "caption_editor": app.tab.caption_editor.text(),
        "caption_format": app.tab.caption_format,
        "caption_target": app.tab.caption_target,
        "inline_open": app.tab.inline_text.is_some(),
        "inline_undo": app.text_history_available(false),
        "inline_redo": app.text_history_available(true),
        "drawing_undo": app.tab.history.can_undo(),
        "drawing_redo": app.tab.history.can_redo(),
        "transform_error": app.tab.numeric_transforms.error,
        "inputs": Field::ALL.map(|field| (field.name(), app.tab.numeric_transforms.value(field))),
    })
}

fn input(app: &mut App, field: Field, value: &str) {
    let _ = app.update(Message::NumericTransform(Action::Input(
        field,
        value.into(),
    )));
}

fn draft(app: &mut App) {
    use iced::widget::text_editor::{Action as TextAction, Edit as TextEdit};
    let id = app.tab.doc.annotations[0].id;
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::Begin(Some(id), Point::default()),
    ));
    let _ = app.update(Message::CaptionAction(TextAction::Edit(TextEdit::Paste(
        " draft".to_owned().into(),
    ))));
}

fn capture(name: &str, mut app: App, expected: Result<bool, &str>) -> Value {
    let fields = app.tab.numeric_transforms.to_apply();
    let before = state(&app);
    let result = app.numeric_transform_candidate(&fields);
    match (&result, expected) {
        (Ok(document), Ok(changed)) => assert_eq!(document.is_some(), changed, "{name}"),
        (Err(error), Err(expected)) => assert_eq!(error, expected, "{name}"),
        (actual, expected) => panic!("{name}: {actual:?}, expected {expected:?}"),
    }
    let candidate = match result {
        Ok(document) => json!({ "document": document }),
        Err(error) => json!({ "error": error }),
    };
    assert_eq!(
        state(&app),
        before,
        "{name}: candidate creation is isolated"
    );
    let _ = app.update(Message::NumericTransform(Action::ApplyAll));
    let applied = state(&app);
    if expected != Ok(true) {
        for key in [
            "document",
            "selection",
            "revision",
            "caption",
            "caption_editor",
            "caption_format",
            "inline_open",
            "inline_undo",
            "inline_redo",
            "drawing_undo",
            "drawing_redo",
        ] {
            assert_eq!(applied[key], before[key], "{name}: preserved {key}");
        }
    }
    // Read drawing history directly so an open caption cannot intercept Undo.
    let mut undo = vec![];
    while app.tab.history.undo(&mut app.tab.doc) {
        undo.push(app.tab.doc.clone());
    }
    let mut redo = vec![];
    while app.tab.history.redo(&mut app.tab.doc) {
        redo.push(app.tab.doc.clone());
    }
    json!({
        "name": name,
        "fields": fields.iter().map(|field| field.name()).collect::<Vec<_>>(),
        "before": before,
        "candidate": candidate,
        "applied": applied,
        "undo_documents": undo,
        "redo_documents": redo,
    })
}

#[test]
#[ignore = "Capture whole document, error, draft and history states before/after a refactor"]
fn numeric_transform_memory_characterization() {
    let mut cases = vec![];
    let mut app = super::tests::fixture();
    let _ = app.update(Message::NumericTransform(Action::Proportional(false)));
    for (field, value) in [
        (Field::Rotation, "72"),
        (Field::TiltX, "25.5"),
        (Field::TiltY, "-33.25"),
        (Field::Scale, "125"),
        (Field::Width, "160"),
        (Field::Height, "90"),
    ] {
        input(&mut app, field, value);
    }
    cases.push(capture("mixed-six-fields", app, Ok(true)));

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
    input(&mut app, Field::Rotation, "33");
    input(&mut app, Field::Scale, "150");
    cases.push(capture("collapsed-abbreviation", app, Ok(true)));

    let mut app = super::tests::fixture();
    let stereo_neighbors = [app.tab.doc.atoms[5].id, app.tab.doc.atoms[2].id];
    let boundary = &mut app.tab.doc.bonds[0];
    boundary.order = 2;
    boundary.stereo = Some("E".into());
    boundary.stereo_atoms = stereo_neighbors.to_vec();
    app.tab.doc.validate().unwrap();
    app.tab.selected = vec![app.tab.doc.atoms[0].id, app.tab.doc.annotations[0].id];
    app.sync_numeric_transforms();
    input(&mut app, Field::Rotation, "45");
    input(&mut app, Field::Scale, "120");
    cases.push(capture("boundary-bonds", app, Ok(true)));

    for (name, scale) in [
        ("late-error-caption-redo", "0"),
        ("noop-caption-redo", "100"),
    ] {
        let mut app = super::tests::fixture();
        input(&mut app, Field::Scale, "125");
        let _ = app.update(Message::NumericTransform(Action::Apply(Field::Scale)));
        let _ = app.update(Message::Undo);
        draft(&mut app);
        input(
            &mut app,
            Field::Rotation,
            if scale == "0" { "72" } else { "360" },
        );
        input(&mut app, Field::Scale, scale);
        let expected = if scale == "0" {
            Err("Scale: Enter a size or percentage greater than zero")
        } else {
            Ok(false)
        };
        cases.push(capture(name, app, expected));
    }

    let mut app = super::tests::fixture();
    draft(&mut app);
    input(&mut app, Field::Scale, "125");
    cases.push(capture("caption-commit-before-transform", app, Ok(true)));

    let mut app = super::tests::fixture();
    app.tab.doc.graphics[0].style.width_pt = 0.;
    input(&mut app, Field::Rotation, "72");
    input(&mut app, Field::Scale, "0");
    cases.push(capture(
        "first-field-validation-error",
        app,
        Err("Rotation: Graphic line width must be 0.1–12 pt"),
    ));

    let mut app = super::tests::fixture();
    app.tab.selected.clear();
    app.sync_numeric_transforms();
    input(&mut app, Field::Rotation, "NaN");
    cases.push(capture(
        "selection-error-before-input-error",
        app,
        Err("Select objects to transform"),
    ));

    let mut app = super::tests::fixture();
    app.tab.selected = vec![app.tab.doc.annotations[0].id];
    app.sync_numeric_transforms();
    input(&mut app, Field::Rotation, "72");
    input(&mut app, Field::Width, "1000");
    cases.push(capture(
        "late-unreachable-width",
        app,
        Err("Width: This size cannot be reached while keeping text and line widths fixed"),
    ));

    let output = std::env::var_os("RESHIKI_NUMERIC_TRANSFORM_CAPTURE")
        .expect("Set RESHIKI_NUMERIC_TRANSFORM_CAPTURE to the transcript path");
    std::fs::write(output, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
}

#[test]
#[ignore = "Allocation profiling workload: 30 isolated four-field numeric candidates"]
fn numeric_transform_candidate_memory_workload() {
    for boundary_stereo in [false, true] {
        let app = profiling_fixture(boundary_stereo);
        let fields = [Field::Rotation, Field::TiltX, Field::TiltY, Field::Scale];
        drop(app.numeric_transform_candidate(&fields).unwrap().unwrap());
        let baseline = crate::allocation_metrics::reset();
        for _ in 0..30 {
            let candidate = app.numeric_transform_candidate(&fields).unwrap().unwrap();
            std::hint::black_box(&candidate);
            drop(candidate);
        }
        let measurements = crate::allocation_metrics::snapshot();
        let workload = if boundary_stereo {
            "numeric-boundary-stereo"
        } else {
            "numeric-whole-selection"
        };
        println!(
            "{}",
            json!({
                "workload": workload,
                "iterations": 30,
                "allocated_bytes": measurements.allocated_bytes,
                "allocation_count": measurements.allocation_count,
                "live_delta": measurements.live_bytes as i128 - baseline as i128,
                "peak_delta": measurements.peak_bytes.saturating_sub(baseline),
            })
        );

        let baseline = crate::allocation_metrics::reset();
        let candidate = app.numeric_transform_candidate(&fields).unwrap().unwrap();
        let retained = crate::allocation_metrics::snapshot();
        let stereo_capacity_bytes = candidate
            .bonds
            .iter()
            .map(|bond| bond.stereo_atoms.capacity() * std::mem::size_of::<u64>())
            .sum::<usize>();
        drop(candidate);
        let released = crate::allocation_metrics::snapshot();
        println!(
            "{}",
            json!({
                "workload": workload,
                "iterations": 1,
                "allocated_bytes": retained.allocated_bytes,
                "allocation_count": retained.allocation_count,
                "candidate_live_delta": retained.live_bytes as i128 - baseline as i128,
                "peak_delta": retained.peak_bytes.saturating_sub(baseline),
                "released_live_delta": released.live_bytes as i128 - baseline as i128,
                "stereo_capacity_bytes": stereo_capacity_bytes,
            })
        );
        assert!(!app.tab.history.can_undo());
    }
}

fn profiling_fixture(boundary_stereo: bool) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    if boundary_stereo {
        for index in 0..250 {
            let x = index as f32 * 180.;
            let ids =
                [0., 42., 84., 126.].map(|dx| app.tab.doc.add_atom("C", Point::new(x + dx, 0.)));
            app.tab.doc.add_bond(ids[0], ids[1], 1, "plain");
            app.tab.doc.add_bond(ids[1], ids[2], 2, "plain");
            app.tab.doc.add_bond(ids[2], ids[3], 1, "plain");
            let double_index = app.tab.doc.bonds.len() - 2;
            let double = &mut app.tab.doc.bonds[double_index];
            double.stereo = Some("E".into());
            double.stereo_atoms = vec![ids[0], ids[3]];
            app.tab.selected.push(ids[1]);
        }
    } else {
        for index in 0..1000 {
            let id = app.tab.doc.add_atom(
                "C",
                Point::new(index as f32 * 36., (index % 2) as f32 * 21.),
            );
            if index > 0 {
                app.tab.doc.add_bond(id - 1, id, 1, "plain");
            }
        }
        app.tab.selected = app.tab.doc.all_ids();
    }
    if boundary_stereo {
        assert_eq!(
            app.tab
                .doc
                .bonds
                .iter()
                .filter(|bond| bond.stereo_atoms.len() == 2)
                .count(),
            250
        );
    }
    app.tab.doc.validate().unwrap();
    app.sync_numeric_transforms();
    for (field, value) in [
        (Field::Rotation, "72"),
        (Field::TiltX, "25.5"),
        (Field::TiltY, "-33.25"),
        (Field::Scale, "125"),
    ] {
        input(&mut app, field, value);
    }
    app
}
