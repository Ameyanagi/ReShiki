//! The results palette after runs answered by hand, as in the card's tests.
use super::super::results::Evaluation;
use super::super::tests::{
    MOVED, START, event, f32s, halocarbon, model, optfreq, optimized, send, water,
};
use super::super::{
    Action as XtbAction, CalcTask, Outcome, RunEvent as Event, RunReport as Report,
};
use super::*;
use crate::app::prompt::{Answer, Prompt};
use reshiki::document::Point;
use reshiki::geometry::Point3;

const MIDDLE: [[f64; 3]; 3] = [[0., 0., 0.005], [0.958, 0., 0.], [-0.24, 0.928, 0.]];

fn palette(app: &mut App, action: Action) {
    send(app, XtbAction::Palette(action));
}

/// A finished optimization through `frames`, the first being the start.
fn path(frames: &[&[[f64; 3]]]) -> Event {
    let energies: Vec<f64> = (0..frames.len())
        .map(|i| -5.06 - 0.005 * i as f64)
        .collect();
    let last = frames.last().expect("frames");
    Event::Done(Ok(std::sync::Arc::new(Report {
        engine: super::super::example_engine(),
        computed: {
            // An optimization's last compute carries the gradient.
            let mut computed =
                super::super::example_computed(*energies.last().expect("energies"), 0.05);
            computed.gradient = Some(vec![[0.; 3]; last.len()]);
            computed
        },
        energy: *energies.last().expect("energies"),
        converged: true,
        positions: last.to_vec(),
        steps: frames.len() - 1,
        optimizers: vec![reshiki_optimize::Method::Internal],
        evaluations: energies
            .iter()
            .map(|&energy| Evaluation {
                energy,
                scf_iterations: 6,
                seconds: 0.003,
            })
            .collect(),
        energies,
        path: frames
            .iter()
            .flat_map(|f| f.as_flattened())
            .copied()
            .collect(),
    })))
}

fn edit(app: &mut App, o: u64) {
    let before = app.tab.doc.clone();
    let carbon = app.tab.doc.add_atom("C", Point::new(40., 0.));
    app.tab.doc.add_bond(o, carbon, 1, "plain");
    app.changed(before);
}

#[test]
fn ir_export_completion_has_a_short_summary_and_path_details() {
    let (mut app, _) = water(CalcTask::Optimize);
    let path = std::path::PathBuf::from(format!("/tmp/{}/spectrum.svg", "journal-".repeat(12)));
    let receipt = crate::app::figure_export::Prepared::new(
        vec![],
        "svg",
        None,
        vec![],
        crate::app::figure_export::Kind::Ir,
    )
    .completed(Ok(Some(path.clone())));
    let _ = app.update(receipt);
    let (summary, details) = app.status.split_once('\n').unwrap();
    assert_eq!(summary, "Exported the IR spectrum");
    assert!(summary.chars().count() <= 60);
    assert_eq!(details, path.to_string_lossy());
    assert!(!app.error);

    let error = format!("Could not create a temporary file at {}", path.display());
    palette(&mut app, Action::IrExported(Err(error.clone())));
    let (summary, details) = app.status.split_once('\n').unwrap();
    assert_eq!(summary, "Could not export the IR spectrum");
    assert_eq!(details, error);
    assert!(app.error);
}

#[test]
fn a_finished_run_opens_the_palette_and_the_card_defers_to_it() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    assert!(app.xtb_palette_visible());
    let shown = app.xtb_shown().expect("the new result");
    assert!(!shown.stale);
    assert!(
        shown
            .method()
            .starts_with("GFN2-xTB · axtb 0.2.0 · gas phase · charge 0 · multiplicity 1 · "),
        "{}",
        shown.method()
    );
    let rows = shown.rows();
    let labels: Vec<_> = rows.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        &labels[..3],
        ["Total energy", "HOMO–LUMO gap", "Optimization"]
    );
    assert!(matches!(app.tab.xtb.outcome, Some(Outcome::Done { .. })));
    assert!(app.xtb_bottom_palette().is_some());

    palette(&mut app, Action::CopyMethods);
    assert_eq!(
        app.status,
        "Copied the methods text (BibTeX keys from ReShiki's xtb.bib)"
    );
    palette(&mut app, Action::CopyTable);
    assert!(app.status.starts_with("Copied 6 rows"), "{}", app.status);

    palette(&mut app, Action::Close);
    assert!(!app.xtb_palette_visible());
    assert!(app.xtb_bottom_palette().is_none());
    palette(&mut app, Action::Show);
    assert!(app.xtb_palette_visible());

    // Docked in the inspector, the bottom dock is empty.
    palette(&mut app, Action::Dock);
    assert!(app.xtb_bottom_palette().is_none());
    assert!(app.xtb_inspector_palette().is_some());
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Predict);
}

fn charged_halocarbon() -> (App, u64) {
    use reshiki::qcdata::{AtomicCharges, ChargeScheme};

    let (mut app, carbon, start) = halocarbon();
    let Event::Done(Ok(mut report)) = optimized(&start, &start) else {
        unreachable!()
    };
    Arc::get_mut(&mut report).unwrap().computed.charges = Some(AtomicCharges {
        scheme: ChargeScheme::Cm5,
        values: vec![-0.561, 0.2805, -0.0001, 0.0, 0.2806],
    });
    event(&mut app, Event::Done(Ok(report)));
    (app, carbon)
}

#[test]
fn placing_charges_labels_drawn_atoms_in_one_undo_step() {
    use reshiki::scene::{Primitive, primitives};

    let (mut app, _) = charged_halocarbon();
    let shown = app.xtb_shown().expect("the new result");
    let ids: Vec<_> = shown.atoms.iter().flatten().copied().collect();
    assert_eq!(ids.len(), 4);
    assert_eq!(shown.atoms.last(), Some(&None));
    let before = app.tab.doc.clone();
    palette(&mut app, Action::PlaceCharges);
    assert_eq!(
        app.status,
        "Placed 4 CM5 charges as atom labels · ⌘Z undoes"
    );
    let style = shift_label_style(&app.tab.doc);
    let drawing = primitives(&app.tab.doc);
    for (id, expected) in ids.into_iter().zip(["−0.56", "+0.28", "+0.00", "+0.00"]) {
        let label = app
            .tab
            .doc
            .atom(id)
            .unwrap()
            .display
            .number
            .as_ref()
            .unwrap();
        assert_eq!(label.text, expected);
        assert_eq!(label.offset, None);
        assert_eq!(label.style, style);
        assert!(
            drawing
                .iter()
                .any(|item| matches!(item, Primitive::Text { text, .. } if text == expected))
        );
    }
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atoms, before.atoms);
    assert!(app.xtb_shown().is_some_and(|shown| !shown.stale));
    edit(&mut app, before.atoms[0].id);
    palette(&mut app, Action::PlaceCharges);
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .all(|atom| atom.display.number.is_none())
    );
}

#[test]
fn placing_charges_preserves_other_labels() {
    for (text, custom_style, offset) in [
        ("#3 · 7.21 ppm", false, None),
        ("#3", false, None),
        ("7.21", false, None),
        ("+1", false, None),
        ("−2", false, None),
        ("custom label", false, None),
        ("0.00", false, None),
        ("+0.28", true, None),
        ("−0.56", false, Some(Point::new(17., -23.))),
        ("+0.00", false, Some(Point::default())),
    ] {
        let (mut app, carbon) = charged_halocarbon();
        let own = Number {
            text: text.into(),
            style: reshiki::typography::TextStyle {
                bold: custom_style,
                ..shift_label_style(&app.tab.doc)
            },
            offset,
        };
        app.tab.doc.atom_mut(carbon).unwrap().display.number = Some(own.clone());
        palette(&mut app, Action::PlaceCharges);
        assert_eq!(
            app.status,
            "Placed 3 CM5 charges as atom labels · ⌘Z undoes · 1 atoms kept their own labels"
        );
        assert_eq!(app.tab.doc.atom(carbon).unwrap().display.number, Some(own));
    }
}

#[test]
fn placing_charges_refreshes_previous_charges() {
    let (mut app, _) = charged_halocarbon();
    let shown = app.xtb_shown().expect("the new result");
    let ids: Vec<_> = shown.atoms.iter().flatten().copied().collect();
    let style = shift_label_style(&app.tab.doc);
    for (id, text) in ids
        .iter()
        .copied()
        .zip(["+0.00", "−0.56", "+0.28", "−0.03"])
    {
        app.tab.doc.atom_mut(id).unwrap().display.number = Some(Number {
            text: text.into(),
            style: style.clone(),
            offset: None,
        });
    }
    let before = app.tab.doc.clone();
    palette(&mut app, Action::PlaceCharges);
    assert_eq!(
        app.status,
        "Placed 4 CM5 charges as atom labels · ⌘Z undoes"
    );
    for (id, text) in ids.into_iter().zip(["−0.56", "+0.28", "+0.00", "+0.00"]) {
        assert_eq!(
            app.tab.doc.atom(id).unwrap().display.number,
            Some(Number {
                text: text.into(),
                style: style.clone(),
                offset: None,
            })
        );
    }
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atoms, before.atoms);
}

#[test]
fn hidden_abbreviation_members_wait_for_expansion_to_get_charges() {
    let (mut app, c) = charged_halocarbon();
    let hidden = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|a| a.element == "F")
        .unwrap()
        .id;
    let before = app.tab.doc.clone();
    app.tab.doc.contract(&[c, hidden], "CF", "FC").unwrap();
    app.changed(before);
    app.tab.xtb.palette.labels = true;
    let preview = app.xtb_label().unwrap();
    assert_eq!(preview.atom_labels.len(), 3);
    assert!(preview.atom_labels.iter().all(|(id, _)| *id != hidden));
    palette(&mut app, Action::PlaceCharges);
    assert_eq!(
        app.status,
        "Placed 3 CM5 charges as atom labels · ⌘Z undoes"
    );
    assert!(app.tab.doc.atom(hidden).unwrap().display.number.is_none());

    let before = app.tab.doc.clone();
    assert_eq!(app.tab.doc.expand_abbreviations(&[c]), 1);
    app.changed(before);
    assert_eq!(app.xtb_label().unwrap().atom_labels.len(), 4);
    palette(&mut app, Action::PlaceCharges);
    assert_eq!(
        app.status,
        "Placed 4 CM5 charges as atom labels · ⌘Z undoes"
    );
    assert!(app.tab.doc.atom(hidden).unwrap().display.number.is_some());
}

#[tokio::test]
#[ignore = "Opt-in layout check. Use --features iced/tiny-skia without a GPU"]
async fn bottom_dock_keeps_its_tabs_and_footer_at_the_same_height() {
    use iced::advanced::{
        renderer::Headless,
        widget::{Id, Operation, operation},
    };
    use iced::{Rectangle, Size};

    #[derive(Default)]
    struct TextBounds(Vec<(String, Rectangle)>);

    impl Operation for TextBounds {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
            self.0.push((text.into(), bounds));
        }
    }

    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let (mut optimization, _) = water(CalcTask::OptFreq);
    let mut report = optfreq(&START, &MOVED);
    report.computed.orbitals = Some(
        serde_json::from_value(serde_json::json!({
            "core_electrons": [2, 0, 0],
            "alpha": { "energies_hartree": [-0.7, -0.5, -0.4, -0.3, 0.2],
                       "occupations": [2., 2., 2., 2., 0.] },
            "beta": "restricted"
        }))
        .unwrap(),
    );
    event(
        &mut optimization,
        Event::Done(Ok(std::sync::Arc::new(report))),
    );
    let (conformers, _) = App::xtb_searched_water();
    for (mut app, expected) in [
        (
            optimization,
            &[
                View::Summary,
                View::Vibrations,
                View::Steps,
                View::Orbitals,
                View::Log,
            ][..],
        ),
        (
            conformers,
            &[View::Summary, View::Conformers, View::Log][..],
        ),
    ] {
        assert_eq!(
            View::available(&app.xtb_shown().unwrap().stored.result),
            expected
        );
        let mut roomy_actions = None;
        for size in [
            Size::new(1280., 820.),
            Size::new(1040., 680.),
            Size::new(480., 680.),
        ] {
            let mut before = None;
            for &view in expected {
                palette(&mut app, Action::View(view));
                let mut ui = iced_runtime::UserInterface::build(
                    app.view(),
                    size,
                    Default::default(),
                    &mut renderer,
                );
                let mut collect = reshiki::accessibility::Collect::new(Rectangle::with_size(size));
                ui.operate(&renderer, &mut operation::black_box(&mut collect));
                let node = |id| {
                    collect
                        .snapshot()
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .unwrap()
                };
                let keep = node("xtb-keep-full").bounds;
                let found = (node("xtb-results-summary").bounds.y, keep);
                if let Some(before) = before {
                    assert_eq!(found, before, "{size:?}, {view:?}");
                }
                before = Some(found);
                let note_prefix = match view {
                    View::Vibrations => Some("Normal modes are recomputed"),
                    View::Steps => Some("The file keeps the first and last steps"),
                    View::Orbitals => Some("Orbitals are recomputed"),
                    View::Conformers => Some("Kept conformers are saved"),
                    _ => None,
                };
                if let Some(prefix) = note_prefix {
                    let mut texts = TextBounds::default();
                    ui.operate(&renderer, &mut texts);
                    let (_, note) = texts
                        .0
                        .iter()
                        .find(|(text, _)| text.starts_with(prefix))
                        .expect("the footer explains how the result is saved");
                    assert!(
                        note.width > 0.
                            && note.height > 0.
                            && note.x >= 16.
                            && note.x + note.width <= size.width - 16. + 0.01
                            && note.y + note.height <= keep.y,
                        "{size:?}, {view:?} footer note is clipped: {note:?}, keep: {keep:?}"
                    );
                    if size.width == 480. && view == View::Vibrations {
                        assert!(
                            note.height > 20.,
                            "the full Vibrations note should wrap at {size:?}: {note:?}"
                        );
                    }
                }
                if view == View::Summary {
                    let actions = [
                        "xtb-copy-methods",
                        "xtb-copy-table",
                        "xtb-export-table",
                        "xtb-labels",
                        "xtb-place-charges",
                    ]
                    .map(|id| {
                        let action = node(id);
                        let bounds = action.bounds;
                        let visible = action.visible_bounds.unwrap();
                        assert!(
                            (visible.width - bounds.width).abs() < 0.01
                                && (visible.height - bounds.height).abs() < 0.01,
                            "{size:?}, {id} is clipped: {visible:?} of {bounds:?}"
                        );
                        assert!(bounds.width > 0. && bounds.y + bounds.height <= keep.y);
                        bounds.size()
                    });
                    if let Some(roomy) = roomy_actions {
                        assert_eq!(actions, roomy, "action text shrank at {size:?}");
                    } else {
                        roomy_actions = Some(actions);
                    }
                    let copy = node("xtb-copy-methods").bounds;
                    let place = node("xtb-place-charges").bounds;
                    if size.width == 480. {
                        assert!(place.y >= copy.y + copy.height, "charges should wrap");
                    } else {
                        assert_eq!(place.y, copy.y);
                    }
                }
            }
        }
    }
}

#[test]
fn a_reopened_light_optfreq_result_shows_the_vibrations_tab() {
    let (mut app, _) = water(CalcTask::OptFreq);
    event(&mut app, Event::Done(Ok(Arc::new(optfreq(&START, &MOVED)))));
    let bytes = app.tab.doc.file_json().expect("save");
    let (mut reopened, _) = App::new();
    reopened.tab.doc = Document::from_native_file(&bytes).expect("reopen");
    let result = reopened.tab.doc.science.results[0].result.clone();
    assert!(result.vibrations.as_ref().unwrap().modes.is_empty());
    assert!(View::available(&result).contains(&View::Vibrations));
    reopened.tab.xtb.palette.show(&result);
    let shown = reopened.xtb_shown().expect("a saved result");
    assert!(shown.info.is_none());
    let rows = shown.rows();
    for (label, value) in [
        ("Frequencies", "lowest 1590 cm⁻¹"),
        ("ZPVE", "0.019453 Eh (12.21 kcal/mol)"),
        ("G (298.15 K)", "−5.068407 Eh"),
        ("Point group", "C2v"),
    ] {
        assert!(
            rows.iter().any(|(l, v)| l == label && v == value),
            "{rows:?}"
        );
    }
    palette(&mut reopened, Action::View(View::Vibrations));
    assert!(reopened.xtb_bottom_palette().is_some());
}

#[test]
fn a_failure_blocks_actions_on_the_previous_result() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    palette(&mut app, Action::Labels(true));
    assert!(app.xtb_label().is_some());

    app.xtb.settings.iterations = "1".into();
    send(&mut app, XtbAction::Run);
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("1 iteration".into()))),
    );
    let failure_status = app.status.clone();
    assert_eq!(
        failure_status,
        "xTB calculation failed: The electronic structure (SCF) did not converge within 1 iteration, the Max SCF iterations you set."
    );
    let failed = app.xtb_failed().expect("the selected molecule's failure");
    assert!(app.xtb_can_retry(failed));
    assert_eq!(failed.scf_iteration_limit(), Some(1));
    assert!(!app.tab.xtb.palette.previous);
    for action in [Action::CopyTable, Action::CopyLog, Action::Export] {
        palette(&mut app, action);
        assert_eq!(app.status, failure_status);
    }
    assert!(app.xtb_label().is_none());

    palette(&mut app, Action::Previous);
    assert!(app.tab.xtb.palette.previous);
    palette(&mut app, Action::CopyTable);
    assert_eq!(
        app.status, failure_status,
        "the expanded summary is read-only"
    );
    palette(&mut app, Action::Show);
    assert!(!app.tab.xtb.palette.previous);

    palette(&mut app, Action::Previous);
    send(&mut app, XtbAction::ResetRetry);
    let run = app
        .tab
        .xtb
        .run
        .as_ref()
        .expect("Reset and retry starts a run");
    assert_eq!(run.config.max_iterations, None);
    assert_eq!(run.config.task, CalcTask::Optimize);
    assert!(app.xtb.settings.iterations.is_empty());
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("250 iterations".into()))),
    );
    assert!(
        !app.tab.xtb.palette.previous,
        "a new failure closes the disclosure"
    );
}

#[test]
fn selecting_a_molecule_without_results_hides_the_other_molecules_result() {
    let (mut app, water) = water(CalcTask::Optimize);
    event(&mut app, path(&[&START, &MIDDLE, &MOVED]));
    let before = app.tab.doc.clone();
    let other = app.tab.doc.add_atom("O", Point::new(200., 0.));
    app.changed(before);
    palette(&mut app, Action::View(View::Steps));
    palette(&mut app, Action::Step(1));
    assert!(app.xtb_step_preview().is_some());

    app.tab.selected = vec![other];
    assert!(app.xtb_outcome().is_none());
    assert!(app.xtb_shown().is_none());
    assert!(
        app.xtb_bottom_palette().is_some(),
        "the open palette stays visible"
    );
    assert!(app.xtb_step().is_none());
    assert!(app.xtb_step_preview().is_none());
    app.tab.selected = vec![water];
    assert!(matches!(app.xtb_outcome(), Some(Outcome::Done { .. })));
    assert!(app.xtb_shown().is_some());
    assert_eq!(app.xtb_step(), Some(1));
}

#[test]
fn the_palette_follows_the_selected_molecule() {
    let (mut app, water) = water(CalcTask::Energy);
    event(&mut app, path(&[&START]));
    let first = app.xtb_shown().expect("water").stored.result.clone();
    // A second water, calculated: the palette follows it.
    let before = app.tab.doc.clone();
    let other = app.tab.doc.add_atom("O", Point::new(200., 0.));
    app.changed(before);
    app.tab.selected = vec![other];
    app.xtb.settings.task = CalcTask::Optimize;
    send(&mut app, XtbAction::Run);
    let positions = START.map(|[x, y, z]| Point3 { x, y, z }).to_vec();
    let _ = app.finish_model(positions, vec![0, 0]);
    assert!(app.xtb_running(), "{}", app.status);
    event(&mut app, path(&[&START, &MIDDLE, &MOVED]));
    palette(&mut app, Action::View(View::Steps));
    palette(&mut app, Action::Step(1));
    assert_eq!(app.xtb_step(), Some(1));
    assert!(
        !app.xtb_can_show(),
        "the selected molecule's result is shown"
    );
    app.tab.selected = vec![water];
    let shown = app.xtb_shown().expect("water again");
    assert!(std::sync::Arc::ptr_eq(&shown.stored.result, &first));
    assert!(!app.xtb_can_show());
    assert!(app.xtb_step_preview().is_none());
    assert!(app.xtb_outcome().is_none());

    app.tab.selected = vec![other];
    send(&mut app, XtbAction::Run);
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("250 iterations".into()))),
    );
    assert!(app.xtb_failed().is_some());
    app.tab.selected = vec![water];
    assert!(app.xtb_outcome().is_none());
    palette(&mut app, Action::CopyTable);
    assert!(app.status.starts_with("Copied "));
}

#[test]
fn an_edit_greys_the_results_undo_restores_them_and_recalculate_reruns() {
    let (mut app, o) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    palette(&mut app, Action::Labels(true));
    assert!(
        app.xtb_label()
            .is_some_and(|label| label.text == "E −5.070176 Eh")
    );

    edit(&mut app, o);
    assert!(app.xtb_shown().is_some_and(|shown| shown.stale));
    assert!(app.xtb_label().is_none(), "stale results draw no labels");

    let _ = app.update(Message::Undo);
    assert!(app.xtb_shown().is_some_and(|shown| !shown.stale));
    assert!(app.xtb_label().is_some());

    let _ = app.update(Message::Redo);
    assert!(app.xtb_shown().is_some_and(|shown| shown.stale));
    assert!(!app.outdated_models().is_empty());
    palette(&mut app, Action::KeepFull(true));
    palette(&mut app, Action::Close);
    app.xtb.settings.task = CalcTask::Energy;
    palette(&mut app, Action::RecalculateMolecule);
    assert!(app.xtb_running());
    assert!(app.xtb_palette_visible());
    let run = app.tab.xtb.run.as_ref().expect("Recalculate starts a run");
    assert_eq!(
        run.config.task,
        CalcTask::Optimize,
        "the result's task, not the card's"
    );
    assert_eq!(
        (run.config.charge, run.config.unpaired, run.config.solvent),
        (0, 0, None)
    );
    assert_eq!(run.atoms.len(), 2, "the molecule as it is now");
    assert!(run.keep_full, "the new result keeps \"Keep full\" too");
    assert!(
        app.tab.model_job.is_some(),
        "the changed molecule gets a new model first"
    );
}

#[test]
fn the_floating_notice_yields_to_the_visible_stale_result_banner() {
    let (mut app, o) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    edit(&mut app, o);
    app.tab.selected = vec![o];
    assert!(app.xtb_shown().is_some_and(|shown| shown.stale));
    let with_banner = if super::super::available() { 1 } else { 2 };
    let canvas_children = |app: &App| {
        app.with_model_notice(iced::widget::Space::new().into())
            .as_widget()
            .children()
            .len()
    };

    assert!(app.xtb_palette_can_repeat_stale(&[o]));
    assert_eq!(canvas_children(&app), with_banner);
    palette(&mut app, Action::Collapse);
    assert!(!app.xtb_palette_can_repeat_stale(&[o]));
    assert_eq!(canvas_children(&app), 2);
    palette(&mut app, Action::Collapse);
    palette(&mut app, Action::Dock);
    assert!(app.xtb_palette_can_repeat_stale(&[o]));
    assert_eq!(canvas_children(&app), with_banner);
    app.inspector_tab = crate::app::InspectorTab::Properties;
    assert!(!app.xtb_palette_can_repeat_stale(&[o]));
    assert_eq!(canvas_children(&app), 2);
    palette(&mut app, Action::Close);
    assert_eq!(canvas_children(&app), 2);
}

#[test]
fn a_result_from_a_late_run_arrives_stale() {
    let (mut app, o) = water(CalcTask::Optimize);
    edit(&mut app, o);
    event(&mut app, optimized(&START, &MOVED));
    assert!(app.xtb_shown().is_some_and(|shown| shown.stale));
    palette(&mut app, Action::Step(0));
    assert!(app.xtb_step().is_none(), "stale steps do not preview");
}

#[test]
fn a_step_previews_on_the_model_and_use_this_geometry_is_one_undo_step() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, path(&[&START, &MIDDLE, &MOVED]));
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
    let shown = app.xtb_shown().expect("result");
    assert_eq!(
        results::warning(&shown.stored.result, shown.info),
        Some(state::NEAR_DEGENERATE)
    );

    palette(&mut app, Action::View(View::Steps));
    palette(&mut app, Action::Step(1));
    assert_eq!(model(app.display_document()), f32s(&MIDDLE));
    assert_eq!(
        model(&app.tab.doc),
        f32s(&MOVED),
        "a preview is not an edit"
    );
    let _ = app.update(Message::Escape);
    assert_eq!(model(app.display_document()), f32s(&MOVED));

    palette(&mut app, Action::Step(1));
    palette(&mut app, Action::UseStep);
    assert_eq!(model(&app.tab.doc), f32s(&MIDDLE));
    assert_eq!(app.xtb_step(), None);
    assert!(
        app.status.starts_with("Applied xTB geometry of step 1"),
        "{}",
        app.status
    );
    assert!(
        app.xtb_shown().is_some_and(|shown| !shown.stale),
        "moving the model keeps the result current"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));

    // After another step, the last one can be used again.
    palette(&mut app, Action::Step(1));
    palette(&mut app, Action::UseStep);
    palette(&mut app, Action::Step(2));
    palette(&mut app, Action::UseStep);
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));

    // A run owns the model preview: no step previews meanwhile.
    send(&mut app, XtbAction::Run);
    palette(&mut app, Action::Step(1));
    assert_eq!(app.xtb_step(), None);
}

#[test]
fn each_result_keeps_its_own_run_details_through_undo() {
    let (mut app, _) = water(CalcTask::Energy);
    let energy = || {
        let mut done = path(&[&START]);
        if let Event::Done(Ok(report)) = &mut done {
            let report = std::sync::Arc::get_mut(report).expect("unshared");
            report.positions = START.to_vec();
            report.path.clear();
            report.steps = 0;
        }
        done
    };
    let threads = |app: &App| match &app.xtb_shown()?.stored.result.settings {
        reshiki::qcdata::Settings::Xtb(settings) => settings.threads,
        _ => None,
    };
    event(&mut app, energy());
    let first = app.xtb_shown().expect("first").stored.result.clone();
    let first_threads = threads(&app);
    let second_threads = if first_threads == Some(2) { 3 } else { 2 };
    app.xtb.settings.threads = second_threads;
    send(&mut app, XtbAction::Run);
    event(&mut app, energy());
    assert_eq!(threads(&app), Some(second_threads));
    assert_eq!(
        app.tab.doc.science.results.len(),
        2,
        "same geometry and energy, two results"
    );
    let _ = app.update(Message::Undo);
    let shown = app.xtb_shown().expect("the first result again");
    assert!(std::sync::Arc::ptr_eq(&shown.stored.result, &first));
    assert_eq!(threads(&app), first_threads);
}

#[test]
fn keep_model_ends_the_preview() {
    let (mut app, _, start) = halocarbon();
    let mirrored: Vec<[f64; 3]> = start.iter().map(|&[x, y, z]| [-x, y, z]).collect();
    event(&mut app, path(&[&start, &mirrored, &start]));
    palette(&mut app, Action::Step(1));
    palette(&mut app, Action::UseStep);
    assert!(!app.tab.xtb.palette.confirm.is_empty());
    palette(&mut app, Action::KeepModel);
    assert_eq!(app.xtb_step(), None);
    assert_eq!(model(app.display_document()), f32s(&start));
}

#[test]
fn a_step_with_another_configuration_asks_first() {
    let (mut app, _, start) = halocarbon();
    let mirrored: Vec<[f64; 3]> = start.iter().map(|&[x, y, z]| [-x, y, z]).collect();
    event(&mut app, path(&[&start, &mirrored, &start]));
    palette(&mut app, Action::Step(1));
    palette(&mut app, Action::UseStep);
    assert!(
        app.tab.xtb.palette.confirm[0].starts_with("Step 1 changes the configuration at C"),
        "{:?}",
        app.tab.xtb.palette.confirm
    );
    assert_eq!(model(&app.tab.doc), f32s(&start));
    palette(&mut app, Action::UseStepAnyway);
    assert_eq!(model(&app.tab.doc), f32s(&mirrored));
}

#[test]
fn keep_full_results_grows_the_saved_size_and_undoes() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, path(&[&START, &MIDDLE, &MOVED]));
    let (light, full) = {
        let shown = app.xtb_shown().expect("result");
        app.xtb_sizes(&shown)
    };
    assert!(full > 0, "steps and the gradient are full data");
    let saved = app.tab.doc.science.saved();
    let path = saved.results[0].result.trajectory.as_ref().expect("path");
    assert!(
        path.positions_angstrom.is_empty(),
        "light results keep no steps"
    );

    palette(&mut app, Action::KeepFull(true));
    let shown = app.xtb_shown().expect("result");
    assert!(shown.stored.keep_full);
    let (kept, _) = app.xtb_sizes(&shown);
    assert!(kept > light, "{kept} > {light}");
    let _ = app.update(Message::Undo);
    assert!(app.xtb_shown().is_some_and(|shown| !shown.stored.keep_full));
}

/// A 482-atom chain whose kept optimization path (1000 steps) makes the
/// document's results larger than 5 MB.
pub(in crate::app) fn large_results() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbons = 160;
    let ids: Vec<u64> = (0..carbons)
        .map(|i| {
            app.tab
                .doc
                .add_atom("C", Point::new(i as f32 * 25., (i % 2) as f32 * 14.))
        })
        .collect();
    for pair in ids.windows(2) {
        app.tab.doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    app.tab.selected = vec![ids[0]];
    app.xtb.settings.task = CalcTask::Optimize;
    send(&mut app, XtbAction::Run);
    let order = app.model_atoms();
    let mut start: Vec<[f64; 3]> = order
        .iter()
        .map(|id| {
            let i = ids.iter().position(|c| c == id).expect("carbon") as f64;
            [i * 1.27, (i % 2.) * 0.87, 0.]
        })
        .collect();
    let mut parents = vec![];
    for (index, carbon) in start.clone().iter().enumerate() {
        let ends = order[index] == ids[0] || order[index] == ids[carbons - 1];
        for k in 0..if ends { 3 } else { 2 } {
            let side = if k % 2 == 0 { 1. } else { -1. };
            start.push([carbon[0] + 0.3 * k as f64, carbon[1] - 0.6, side * 0.9]);
            parents.push(index);
        }
    }
    let points = start.iter().map(|&[x, y, z]| Point3 { x, y, z }).collect();
    let _ = app.finish_model(points, parents);
    // Small, varied moves, so that the compact encoding cannot shrink them.
    let mut seed = 1u64;
    let frames: Vec<Vec<[f64; 3]>> = (0..1000)
        .map(|_| {
            start
                .iter()
                .map(|p| {
                    p.map(|x| {
                        seed = seed
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        x + ((seed >> 40) as f64 / (1u64 << 24) as f64 - 0.5) * 0.02
                    })
                })
                .collect()
        })
        .collect();
    let mut frames: Vec<&[[f64; 3]]> = frames.iter().map(Vec::as_slice).collect();
    frames[0] = &start;
    event(&mut app, path(&frames));
    palette(&mut app, Action::KeepFull(true));
    app
}

#[test]
fn save_asks_once_about_large_results_and_make_lighter_drops_the_steps() {
    let mut app = large_results();
    let bytes = app.tab.doc.science.file_bytes();
    assert!(bytes > reshiki::science::LIGHTER_BYTES, "{bytes}");
    let _ = app.update(Message::Save);
    let Some(Prompt::LargeResults {
        bytes: asked,
        lighter: Some((1, after)),
        ..
    }) = app.prompt.clone()
    else {
        panic!("Save asks about large results: {:?}", app.prompt.is_some());
    };
    assert_eq!(asked, bytes);
    assert!(after * 20 < bytes, "{after} of {bytes}");
    assert!(!app.file_io.saving, "nothing is written before the answer");

    // Asked about another document (a new file epoch here): nothing happens.
    let prompt = app.prompt.clone().expect("prompt");
    app.tab.file_epoch += 1;
    assert!(app.xtb_size_answer(prompt, Answer::Save).is_none());
    assert!(app.xtb_shown().is_some_and(|shown| shown.stored.keep_full));
    app.tab.file_epoch -= 1;

    let _ = app.update(Message::Prompt(Answer::Save));
    assert!(app.xtb_shown().is_some_and(|shown| !shown.stored.keep_full));
    assert!(app.tab.doc.science.file_bytes() < reshiki::science::LIGHTER_BYTES);

    // Asked once per document.
    palette(&mut app, Action::KeepFull(true));
    app.file_io.saving = false;
    let _ = app.update(Message::Save);
    assert!(app.prompt.is_none());
}

#[test]
fn named_save_as_asks_too_and_keeps_the_name() {
    let mut app = large_results();
    let _ = app.save_drawing_named("chain.rsk".into());
    let Some(Prompt::LargeResults { then, .. }) = app.prompt.clone() else {
        panic!("named Save As asks about large results");
    };
    assert!(matches!(
        *then,
        Message::Xtb(XtbAction::Palette(Action::SaveNamed(name))) if name == "chain.rsk"
    ));
}

/// Writes the large-results document for the Save warning screenshots:
/// `RESHIKI_XTB_LARGE_FIXTURE=/path/large.rsk cargo test … -- --ignored`.
#[test]
#[ignore = "writes an evidence fixture"]
fn manual_write_large_results_fixture() {
    let path = std::env::var("RESHIKI_XTB_LARGE_FIXTURE").expect("fixture path");
    let app = large_results();
    std::fs::write(path, app.tab.doc.file_json().expect("file")).expect("written");
}
