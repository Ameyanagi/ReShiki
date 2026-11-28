//! The Vibrations tab on a water Opt + freq result answered by hand.
use super::super::run::Report;
use super::super::tests::{
    MOVED, START, duplicate_molecule, event, f32s, model, optfreq, optimized, send, water,
};
use super::super::{Action as XtbAction, CalcTask, Outcome, palette, results::View, run};
use super::*;
use crate::app::{InspectorTab, Message};
use reshiki::graphics::{Columns, Frame, GraphicKind};
use reshiki::models3d::{self, Display};
use reshiki::typography::Script;

fn vibrations(app: &mut App, action: Action) {
    send(app, XtbAction::Vibrations(action));
}

/// Water after Opt + freq, with the Vibrations tab open.
fn opened() -> App {
    opened_with(|_| {})
}

fn opened_with(edit: impl FnOnce(&mut Report)) -> App {
    let mut app = completed_with(edit);
    app.tab.xtb.vibrations.playing = true;
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    app
}

fn completed_with(edit: impl FnOnce(&mut Report)) -> App {
    let (mut app, _) = water(CalcTask::OptFreq);
    let mut report = optfreq(&START, &MOVED);
    edit(&mut report);
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    app
}

/// The model positions the canvas shows now.
fn shown(app: &App) -> Vec<[f32; 3]> {
    model(app.display_document())
}

#[test]
fn opening_is_paused_then_play_animates_without_touching_the_document() {
    let mut app = opened();
    let frames = app
        .tab
        .xtb
        .vibrations
        .animation
        .as_ref()
        .map(|a| a.frames.len());
    assert_eq!(frames, Some(FRAMES), "{}", app.status);
    // Phase 0 is the equilibrium geometry.
    assert_eq!(app.status, "Mode 1 · 1590 cm⁻¹ · O–H stretch · paused");
    assert!(!app.tab.xtb.vibrations.playing);
    vibrations(&mut app, Action::Tick);
    assert_eq!(shown(&app), f32s(&MOVED));
    vibrations(&mut app, Action::Play(true));
    vibrations(&mut app, Action::Tick);
    vibrations(&mut app, Action::Tick);
    let moved = shown(&app);
    assert_ne!(moved, f32s(&MOVED), "a tick moves the model");
    // Mode 1 of the example moves atom 1 along x only.
    let [first, second, ..] = MOVED;
    let [x, ..] = first;
    assert_ne!(moved.first().unwrap().first().copied(), Some(x as f32));
    assert_eq!(moved.get(1).copied(), Some(second.map(|x| x as f32)));
    assert_eq!(
        model(&app.tab.doc),
        f32s(&MOVED),
        "the document never moves"
    );
    assert_eq!(app.status, "Mode 1 · 1590 cm⁻¹ · O–H stretch · animating");

    vibrations(&mut app, Action::Play(false));
    assert!(app.status.contains("paused"));
    assert_eq!(shown(&app), moved, "Pause keeps the current frame");

    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Summary)),
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
    assert_eq!(shown(&app), f32s(&MOVED));
    assert!(app.status.is_empty());
}

#[test]
fn opening_picks_imaginary_then_strongest_then_highest_and_pauses() {
    for (frequencies, intensities, selected) in [
        ([-212., -40., 1800.], vec![0., 1., 411.], 0),
        ([40., 1756., 1788.], vec![1., 471., 411.], 1),
        ([40., 1756., 1788.], vec![1., 411., 411.], 2),
        ([40., 1756., 1788.], vec![], 2),
    ] {
        let app = opened_with(|report| {
            let data = report.computed.vibrations.as_mut().unwrap();
            data.frequencies_cm1 = frequencies.to_vec();
            data.ir_intensities_km_mol = (!intensities.is_empty()).then_some(intensities);
        });
        assert_eq!(app.tab.xtb.vibrations.mode, selected, "{frequencies:?}");
        assert!(!app.tab.xtb.vibrations.playing);
    }
}

#[test]
fn the_list_sorts_only_real_modes_and_uses_frequency_without_intensities() {
    let mut data = reshiki::qcdata::Vibrations {
        frequencies_cm1: vec![-212., -40., 20., 40., 1756., 1788.],
        ir_intensities_km_mol: vec![1., 999., 0., 411., 471., 411.],
        modes: vec![],
        reduced_masses_amu: vec![],
        masses_amu: vec![],
    };
    assert_eq!(mode_list::order(&data, Sort::Intensity), [0, 1, 4, 5, 3, 2]);
    assert_eq!(
        mode_list::order(&data, Sort::Wavenumber),
        [0, 1, 5, 4, 3, 2]
    );
    data.ir_intensities_km_mol.clear();
    assert_eq!(mode_list::order(&data, Sort::Intensity), [0, 1, 5, 4, 3, 2]);
}

#[test]
fn reopening_keeps_the_mode_play_state_sort_and_cached_names() {
    let mut app = opened();
    let names = app.xtb_motion_names(&app.xtb_shown().unwrap()).unwrap();
    vibrations(&mut app, Action::Sort(Sort::Wavenumber));
    vibrations(&mut app, Action::Mode(2));
    vibrations(&mut app, Action::Play(true));
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Summary)),
    );
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    assert_eq!(app.tab.xtb.vibrations.mode, 2);
    assert!(app.tab.xtb.vibrations.playing);
    assert_eq!(app.tab.xtb.vibrations.sort, Sort::Wavenumber);
    assert!(Arc::ptr_eq(
        &names,
        &app.xtb_motion_names(&app.xtb_shown().unwrap()).unwrap(),
    ));
}

#[test]
fn returning_to_each_result_restores_its_mode_and_play_state() {
    let mut app = opened();
    let first = app.xtb_shown().unwrap().stored.result.clone();
    let mut second_stored = app.xtb_shown().unwrap().stored.clone();
    let mut second = (*first).clone();
    second.vibrations.as_mut().unwrap().ir_intensities_km_mol = vec![1., 100., 10.];
    let second = Arc::new(second);
    second_stored.result = second.clone();
    app.tab.doc.science.results.push(second_stored);

    vibrations(&mut app, Action::Mode(2));
    vibrations(&mut app, Action::Play(true));
    app.xtb_show_result(&second);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    assert_eq!(
        app.tab.xtb.vibrations.mode, 1,
        "a new result picks its strongest band"
    );
    assert!(
        !app.tab.xtb.vibrations.playing,
        "a new result starts paused"
    );
    vibrations(&mut app, Action::Mode(0));

    for (result, mode, playing) in [(&first, 2, true), (&second, 0, false), (&first, 2, true)] {
        app.xtb_show_result(result);
        send(
            &mut app,
            XtbAction::Palette(palette::Action::View(View::Vibrations)),
        );
        assert_eq!(app.tab.xtb.vibrations.mode, mode);
        assert_eq!(app.tab.xtb.vibrations.playing, playing);
        assert_eq!(app.tab.xtb.vibrations.selections.len(), 1);
    }

    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Summary)),
    );
    let discarded = Arc::downgrade(&second);
    app.tab
        .doc
        .science
        .results
        .retain(|stored| !Arc::ptr_eq(&stored.result, &second));
    drop(second);
    assert!(
        discarded.upgrade().is_none(),
        "the selection memory only holds weak references"
    );
    let mut third = app.xtb_shown().unwrap().stored.clone();
    third.result = Arc::new((*first).clone());
    let result = third.result.clone();
    app.tab.doc.science.results.push(third);
    app.xtb_show_result(&result);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    assert_eq!(
        app.tab.xtb.vibrations.selections.len(),
        1,
        "discarded results are pruned"
    );
}

/// Count reveals aimed at a mode row, without building a renderer.
async fn mode_reveals(task: Task<Message>, mode: usize) -> usize {
    use iced::advanced::widget::{Id, operation};
    use iced::futures::StreamExt;

    struct Focus;
    impl operation::Focusable for Focus {
        fn is_focused(&self) -> bool {
            false
        }
        fn focus(&mut self) {}
        fn unfocus(&mut self) {}
    }

    let id = Id::from(format!("xtb-mode-{}", mode + 1));
    let mut reveals = 0;
    if let Some(mut stream) = iced_runtime::task::into_stream(task) {
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Widget(mut operation) = action {
                operation.focusable(Some(&id), iced::Rectangle::default(), &mut Focus);
                if matches!(operation.finish(), operation::Outcome::Chain(_)) {
                    reveals += 1;
                }
            }
        }
    }
    reveals
}

#[tokio::test]
async fn first_visibility_selects_reveals_and_reports_the_mode_once() {
    for route in ["view", "expand", "inspector"] {
        let mut app = completed_with(|report| {
            report
                .computed
                .vibrations
                .as_mut()
                .unwrap()
                .ir_intensities_km_mol = Some(vec![1., 10., 100.]);
        });
        let collapsed = route != "view";
        app.tab.xtb.palette.collapsed = collapsed;
        app.tab.xtb.vibrations.playing = true;
        let opening = app.update(Message::Xtb(XtbAction::Palette(palette::Action::View(
            View::Vibrations,
        ))));
        if collapsed {
            assert_eq!(mode_reveals(opening, 2).await, 0, "{route}");
            assert!(app.tab.xtb.vibrations.opened.is_none(), "{route}");
            let action = if route == "expand" {
                palette::Action::Collapse
            } else {
                palette::Action::Dock
            };
            let task = app.update(Message::Xtb(XtbAction::Palette(action)));
            assert_eq!(mode_reveals(task, 2).await, 1, "{route}");
        } else {
            assert_eq!(mode_reveals(opening, 2).await, 1, "{route}");
        }
        assert!(app.xtb_vibrations_visible(), "{route}");
        assert_eq!(app.tab.xtb.vibrations.mode, 2, "{route}");
        assert!(!app.tab.xtb.vibrations.playing, "{route}");
        assert!(
            app.status.starts_with("Mode 3 · 3480 cm⁻¹"),
            "{route}: {}",
            app.status
        );
        assert!(app.status.ends_with("paused"), "{route}: {}", app.status);

        app.status = "Saved the drawing".into();
        let repeated = app.update(Message::Xtb(XtbAction::Palette(palette::Action::View(
            View::Vibrations,
        ))));
        assert_eq!(mode_reveals(repeated, 2).await, 0, "{route}");
        assert_eq!(app.status, "Saved the drawing", "{route}");
    }
}

#[tokio::test]
async fn editing_a_visible_result_keeps_the_edit_status_and_scroll_position() {
    let mut app = opened();
    let revision = app.tab.revision;
    let task = app.update(Message::Xtb(XtbAction::Vibrations(Action::PlaceSpectrum)));
    assert_ne!(app.tab.revision, revision);
    assert_eq!(mode_reveals(task, 0).await, 0);
    assert_eq!(app.status, "Placed the IR spectrum · ⌘Z undoes");
}

#[tokio::test]
async fn a_failed_rerun_shows_recovery_without_revealing_the_previous_mode() {
    for collapsed in [false, true] {
        let mut app = opened();
        if collapsed {
            send(&mut app, XtbAction::Palette(palette::Action::Collapse));
        }
        send(&mut app, XtbAction::Run);
        let serial = app.tab.xtb.run.as_ref().unwrap().serial;
        let failure = super::super::Failure::crashed("worker exited");
        let task = app.update(Message::Xtb(XtbAction::Event(
            serial,
            run::Event::Done(Err(failure)),
        )));
        assert!(app.xtb_palette_body_visible());
        assert!(!app.xtb_vibrations_visible());
        assert!(app.error && app.status.starts_with("xTB calculation failed"));
        assert_eq!(mode_reveals(task, 0).await, 0);
    }
}

#[test]
fn leaving_vibrations_preserves_other_status_text() {
    let mut app = opened();
    app.status = "Saved the drawing".into();
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Summary)),
    );
    assert_eq!(app.status, "Saved the drawing");
}

#[test]
fn selecting_another_result_clears_a_paused_vibration_and_switching_back_resyncs() {
    let (mut app, a) = water(CalcTask::OptFreq);
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(optfreq(&START, &MOVED)))),
    );
    let before = app.tab.doc.clone();
    let b = app.tab.doc.add_atom("O", Point::new(200., 0.));
    app.changed(before);
    let select = |app: &mut App, id| {
        let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![id])));
    };
    select(&mut app, b);
    app.xtb.settings.task = CalcTask::Energy;
    send(&mut app, XtbAction::Run);
    let positions = START.map(|[x, y, z]| reshiki::geometry::Point3 { x, y, z });
    let _ = app.finish_model(positions.to_vec(), vec![0, 0]);
    event(&mut app, optimized(&START, &START));
    select(&mut app, a);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    vibrations(&mut app, Action::Arrows(true));
    vibrations(&mut app, Action::Play(true));
    for _ in 0..6 {
        vibrations(&mut app, Action::Tick);
    }
    vibrations(&mut app, Action::Play(false));
    assert_ne!(shown(&app), f32s(&MOVED));
    assert!(!app.display_document().graphics.is_empty());

    app.tab.selected = vec![b];
    assert!(app.xtb_vibration_preview().is_none());
    app.tab.selected = vec![a];

    let revision = app.tab.revision;
    select(&mut app, b);
    assert_eq!(app.tab.revision, revision);
    assert!(
        app.xtb_shown()
            .is_some_and(|entry| entry.atoms.contains(&Some(b)))
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
    assert_eq!(shown(&app), f32s(&MOVED));
    assert!(app.display_document().graphics.is_empty());

    select(&mut app, a);
    assert!(app.xtb_vibration_preview().is_some());
    assert_eq!(shown(&app), f32s(&MOVED));
}

#[test]
fn selecting_a_duplicate_rebuilds_the_paused_vibration_on_the_copy() {
    let mut app = opened();
    let copied = duplicate_molecule(&mut app);
    vibrations(&mut app, Action::Mode(2));
    vibrations(&mut app, Action::Play(true));
    for _ in 0..6 {
        vibrations(&mut app, Action::Tick);
    }
    vibrations(&mut app, Action::Play(false));
    assert_ne!(shown(&app), f32s(&MOVED));

    let original = std::mem::replace(&mut app.tab.selected, copied.clone());
    assert_eq!(app.xtb_shown_entry().map(|(index, _)| index), Some(1));
    assert!(app.xtb_vibration_preview().is_none());
    app.tab.selected.clone_from(&original);

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(copied)));
    assert_eq!(app.xtb_shown_entry().map(|(index, _)| index), Some(1));
    assert!(app.xtb_vibration_preview().is_some());
    let animation = app.tab.xtb.vibrations.animation.as_ref().unwrap();
    assert_eq!(animation.model, 1);
    assert_eq!(shown(&app), f32s(&MOVED));
    assert_eq!(app.tab.xtb.vibrations.mode, 0);
    assert!(!app.tab.xtb.vibrations.playing);

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(original)));
    assert_eq!(app.tab.xtb.vibrations.mode, 2);
    assert!(!app.tab.xtb.vibrations.playing);
}

#[test]
fn another_mode_or_amplitude_rebuilds_the_frames() {
    let mut app = opened();
    vibrations(&mut app, Action::Mode(2));
    vibrations(&mut app, Action::Play(true));
    for _ in 0..6 {
        vibrations(&mut app, Action::Tick);
    }
    let third = shown(&app);
    // Mode 3 of the example moves atom 1 along z.
    let [[x, _, z], ..] = MOVED;
    let [third_x, _, third_z] = *third.first().unwrap();
    assert_eq!(third_x, x as f32);
    assert_ne!(third_z, z as f32);
    let small = third_z - z as f32;
    vibrations(&mut app, Action::Amplitude(0.6));
    let [_, _, large_z] = *shown(&app).first().unwrap();
    let large = large_z - z as f32;
    assert!((large / small - 2.).abs() < 1e-3, "{small} → {large}");
}

#[test]
fn arrows_show_on_the_animation_and_add_to_the_drawing_as_one_undo_step() {
    let mut app = opened();
    models3d::set_display(&mut app.tab.doc, 0, Display::TwoD);
    vibrations(&mut app, Action::Play(true));
    vibrations(&mut app, Action::Arrows(true));
    assert_eq!(
        app.display_document().graphics.len(),
        1,
        "one atom moves in mode 1"
    );
    assert!(app.tab.doc.graphics.is_empty());
    vibrations(&mut app, Action::PlaceArrows);
    let arrows = &app.tab.doc.graphics;
    assert_eq!(arrows.len(), 1);
    let arrow = arrows.first().unwrap();
    assert!(arrow.heads.is_some(), "a pen path with an arrowhead");
    assert!(arrow.layer > 0, "in front of the model");
    assert!(app.tab.doc.arrows.is_empty(), "no reaction arrows");
    assert_eq!(app.tab.doc.models.first().unwrap().display, Display::Both);
    assert!(app.status.starts_with("Added 1 displacement arrows"));
    assert!(
        !app.tab.xtb.vibrations.playing,
        "the model stays at equilibrium under the placed arrows"
    );
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.graphics.is_empty());
    assert_eq!(app.tab.doc.models.first().unwrap().display, Display::TwoD);
}

#[test]
fn placing_the_spectrum_selects_one_group_and_undo_removes_it() {
    let mut app = opened();
    let before = app.tab.doc.clone();
    let settings = &app.tab.xtb.vibrations;
    assert_eq!(settings.y, YAxis::Transmittance);
    assert_eq!(settings.range(), Some((4000., 400.)));
    assert_eq!((settings.fwhm, settings.scale.as_str()), (20., "1.000"));

    vibrations(&mut app, Action::PlaceSpectrum);
    let doc = &app.tab.doc;
    let [group] = doc.groups.as_slice() else {
        panic!("Expected one spectrum group. Status was {}", app.status);
    };
    assert!(!group.integral);
    assert_eq!(app.tab.selected, group.members);
    let mut object_ids: Vec<_> = doc
        .graphics
        .iter()
        .map(|g| g.id)
        .chain(doc.annotations.iter().map(|a| a.id))
        .collect();
    object_ids.sort_unstable();
    assert_eq!(group.members, object_ids);
    let (curve, lines) = doc.graphics.split_first().unwrap();
    assert_eq!(curve.kind, GraphicKind::Path);
    let ir = curve.ir.as_ref().unwrap();
    assert_eq!(ir.y, YAxis::Transmittance);
    assert_eq!((ir.from, ir.to, ir.fwhm, ir.scale), (4000., 400., 20., 1.));
    assert_eq!(ir.method, "GFN2-xTB");
    assert_eq!(curve.style.width_pt, ir.journal.metrics().3);
    assert!(
        lines
            .iter()
            .all(|g| g.kind == GraphicKind::Line && g.ir.is_none())
    );
    assert!(doc.annotations.iter().any(|a| a.text == "1590"));
    assert!(
        doc.annotations
            .iter()
            .all(|a| !a.text.starts_with("GFN2-xTB"))
    );
    assert_eq!(app.status, "Placed the IR spectrum · ⌘Z undoes");
    vibrations(&mut app, Action::Tick);
    assert_eq!(app.status, "Placed the IR spectrum · ⌘Z undoes");

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn placing_the_spectrum_reveals_the_whole_figure_and_keeps_zoom_when_it_fits() {
    for rulers in [false, true] {
        for (viewport, zoom, fits) in [
            (iced::Size::new(900., 600.), 1., true),
            // It still fits with rulers, but has less than the reveal margin.
            (iced::Size::new(720., 490.), 1., true),
            (iced::Size::new(900., 600.), 2.5, false),
        ] {
            let mut app = opened();
            app.viewport = viewport;
            app.guides.rulers = rulers;
            app.tab.camera.zoom = zoom;
            app.tab.camera.center = Point::new(-2000., -1500.);
            app.tab.fit_to_view = true;
            let camera = app.tab.camera;

            vibrations(&mut app, Action::PlaceSpectrum);

            assert!(!app.error, "{}", app.status);
            let (lo, hi) =
                reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
            let paper = app.guides.paper(iced::Rectangle::with_size(app.viewport));
            let visible = iced::Rectangle::with_size(paper.size());
            let context = format!("{viewport:?}, zoom={zoom}, rulers={rulers}");
            assert_eq!(
                (hi.x - lo.x) * zoom <= paper.width && (hi.y - lo.y) * zoom <= paper.height,
                fits,
                "{context}"
            );
            assert!(
                !visible.contains(camera.screen(lo, paper))
                    && !visible.contains(camera.screen(hi, paper)),
                "the placed figure starts off screen: {context}"
            );
            for point in [lo, hi] {
                assert!(
                    visible.contains(app.tab.camera.screen(point, paper)),
                    "the whole placed figure must be visible: {context}, {point:?}"
                );
            }
            if fits {
                assert_eq!(app.tab.camera.zoom, zoom, "scroll only: {context}");
            } else {
                assert!(app.tab.camera.zoom < zoom, "zoom out to fit: {context}");
            }
            assert!(!app.tab.fit_to_view);
        }
    }
}

#[test]
fn placing_the_spectrum_fits_after_properties_opens() {
    let mut app = opened();
    app.inspector_open = false;
    app.inspector_tab = InspectorTab::Predict;
    assert!(app.xtb_bottom_palette().is_some());
    let initial_size = iced::Size::new(900., 600.);
    app.viewport = initial_size;
    app.guides.rulers = false;
    app.tab.camera.zoom = 1.;
    app.tab.camera.center = Point::new(-2000., -1500.);
    app.tab.fit_to_view = true;

    vibrations(&mut app, Action::PlaceSpectrum);

    assert!(!app.error, "{}", app.status);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    let size = iced::Size::new(580., 600.);
    let _ = app.update(Message::Viewport(size));
    assert_eq!(app.viewport, size);
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
    let figure_width = hi.x - lo.x;
    assert!(size.width < figure_width && figure_width < initial_size.width);
    let paper = app.guides.paper(iced::Rectangle::with_size(size));
    let visible = iced::Rectangle::with_size(paper.size());
    for point in [lo, hi] {
        assert!(
            visible.contains(app.tab.camera.screen(point, paper)),
            "the resized canvas must show the whole figure: {point:?}"
        );
    }
    assert!(app.tab.camera.zoom < 1.);
    assert!(!app.tab.fit_to_view);
}

#[test]
fn the_spectrum_clears_a_2d_component_extended_after_calculation() {
    let mut app = opened();
    let oxygen = *app
        .xtb_shown()
        .unwrap()
        .atoms
        .iter()
        .flatten()
        .next()
        .unwrap();
    let before = app.tab.doc.clone();
    models3d::set_display(&mut app.tab.doc, 0, Display::TwoD);
    let carbon = app.tab.doc.add_atom("C", Point::new(1000., -200.));
    app.tab.doc.add_bond(oxygen, carbon, 1, "plain");
    app.changed(before);
    assert!(!app.error, "{}", app.status);
    assert!(app.xtb_shown().unwrap().stale);
    assert!(app.xtb_vibration_preview().is_none());
    let doc = &app.tab.doc;
    let (_, carbon_hi) = reshiki::scene::selection_bounds(doc, &[carbon]).unwrap();
    let expected_x = carbon_hi.x + doc.drawing_style.bond_length_world;

    vibrations(&mut app, Action::PlaceSpectrum);
    assert!(!app.error, "{}", app.status);
    let (lo, _) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
    // The outer box starts at the gap; the title sits inside its margin.
    assert!((expected_x..expected_x + app.tab.doc.drawing_style.world(6.)).contains(&lo.x));
}

#[test]
fn the_spectrum_clears_the_shown_3d_model_frame_throughout_animation() {
    let mut app = opened();
    app.tab.doc.models[0].size = 600.;
    vibrations(&mut app, Action::Amplitude(0.6));
    let drawn = app.display_document();
    assert_ne!(drawn.models[0].display, Display::TwoD);
    let (_, initial_hi) = models3d::frame(drawn, &drawn.models[0]).unwrap();
    let animation = app.xtb_current_animation().unwrap();
    let farthest_x = animation
        .frames
        .iter()
        .map(|(model, _)| models3d::frame(&animation.doc, model).unwrap().1.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let bond = app.tab.doc.drawing_style.bond_length_world;
    assert!(farthest_x > initial_hi.x + bond);

    vibrations(&mut app, Action::PlaceSpectrum);
    assert!(!app.error, "{}", app.status);
    let (lo, _) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
    let expected_x = farthest_x + bond;
    assert!((expected_x..expected_x + app.tab.doc.drawing_style.world(6.)).contains(&lo.x));
}

fn acetic_acid() -> IrFigure {
    IrFigure::with_defaults(
        "acetic acid".into(),
        "GFN2-xTB".into(),
        vec![
            [3448., 0.065],
            [3082., 0.014],
            [3036., 0.012],
            [2975., 0.006],
            [1803., 1.],
            [1436., 0.07],
            [1381., 0.11],
            [1243., 0.36],
            [1159., 0.42],
            [1002., 0.11],
            [905., 0.012],
            [657., 0.37],
            [592., 0.10],
            [556., 0.035],
            [418., 0.012],
        ],
        Journal::Acs,
    )
}

fn aspirin() -> IrFigure {
    IrFigure::with_defaults(
        "aspirin".into(),
        "GFN2-xTB".into(),
        vec![
            [39.7, 5.39],
            [76.2, 5.21],
            [83.4, 2.92],
            [104.6, 0.8],
            [111.4, 0.25],
            [137.8, 1.11],
            [193.8, 0.14],
            [246.3, 1.45],
            [290.5, 1.82],
            [335.0, 1.4],
            [378.1, 2.37],
            [423.0, 8.3],
            [470.8, 3.76],
            [507.9, 9.74],
            [537.1, 0.64],
            [561.6, 23.37],
            [596.0, 68.59],
            [605.1, 33.78],
            [648.6, 9.28],
            [682.1, 86.59],
            [733.8, 62.07],
            [756.5, 25.92],
            [789.4, 6.09],
            [831.4, 23.9],
            [883.5, 1.54],
            [918.4, 1.07],
            [928.7, 0.24],
            [936.5, 41.55],
            [994.5, 57.45],
            [1007.9, 30.93],
            [1071.4, 119.07],
            [1086.2, 13.97],
            [1137.1, 209.52],
            [1159.0, 93.46],
            [1163.0, 448.63],
            [1178.1, 2.16],
            [1218.6, 74.47],
            [1240.5, 8.0],
            [1277.3, 215.13],
            [1320.8, 0.5],
            [1358.7, 60.36],
            [1427.5, 19.23],
            [1434.2, 13.28],
            [1439.9, 14.97],
            [1455.2, 46.54],
            [1573.3, 16.19],
            [1590.5, 29.93],
            [1756.1, 470.92],
            [1788.3, 410.89],
            [3021.9, 1.12],
            [3053.4, 2.61],
            [3068.6, 9.71],
            [3088.2, 4.48],
            [3097.2, 21.93],
            [3106.4, 19.47],
            [3110.2, 11.52],
            [3450.5, 44.2],
        ],
        Journal::Acs,
    )
}

#[test]
fn peak_selection_reserves_weak_stretches_before_fingerprint_bands() {
    let peaks = [
        [1700., 100.],
        [1200., 60.],
        [1100., 50.],
        [1000., 40.],
        [900., 35.],
        [800., 30.],
        [700., 25.],
        [600., 20.],
        [3400., 3.],
        [2950., 1.5],
        [3000., 2.5],
        [3050., 2.2],
    ];
    let expected = [
        (peaks[0], true),
        (peaks[8], true),
        (peaks[10], true),
        (peaks[1], false),
        (peaks[2], false),
        (peaks[3], false),
        (peaks[4], false),
        (peaks[5], false),
    ];
    let mut ordered = peaks;
    ordered.sort_by(|a, b| f64::total_cmp(&b[0], &a[0]));
    for count in [8, 6] {
        let selected: Vec<_> = figure::select_labels(&ordered, 100., count, |_| true)
            .into_iter()
            .map(|label| (label.peak, label.required))
            .collect();
        assert_eq!(selected, expected[..count]);
    }
}

#[test]
fn weak_stretch_labels_need_a_visible_dip_at_the_printed_size() {
    let doc = Document::default();
    let mut ir = IrFigure {
        bands: vec![[3060., 1.4], [3015., 0.4], [1050., 100.]],
        ..IrFigure::default()
    };
    let labels = |ir: &IrFigure| figure::spectrum(&doc, ir, Point::default()).labelled_peaks;
    assert_eq!(labels(&ir), [(3060, 3060), (1050, 1050)]);

    // The closer stretches have shallower dips even though their tips are higher.
    ir.bands[1] = [3037., 1.35];
    assert_eq!(labels(&ir), [(1050, 1050)]);
    ir.columns = Columns::Two;
    assert_eq!(labels(&ir), [(3059, 3059), (1050, 1050)]);
}

#[test]
fn hiding_the_strongest_label_preserves_baseline_absorbance_scaling() {
    let doc = Document::default();
    let mut ir = IrFigure {
        bands: vec![[3060., 1.4], [3036., 1.35], [1050., 100.]],
        y: YAxis::Absorbance,
        frame: Frame::Baseline,
        ..IrFigure::default()
    };
    let build = |ir: &IrFigure| figure::spectrum(&doc, ir, Point::default());
    let visible = build(&ir);
    assert_eq!(visible.labelled_peaks, [(1050, 1050)]);

    ir.hidden_labels.push(1050);
    let hidden = build(&ir);
    assert!(hidden.labelled_peaks.is_empty());
    assert_eq!(hidden.graphics[0].path, visible.graphics[0].path);
}

#[test]
fn doublet_partners_can_fall_below_an_anchor_cutoff_or_cross_a_region_boundary() {
    let doc = Document::default();
    let mut ir = IrFigure {
        bands: vec![[1788., 1.7], [1756., 5.], [1050., 100.]],
        columns: Columns::Two,
        fwhm: 2.,
        ..IrFigure::default()
    };
    let labels = |ir: &IrFigure| figure::spectrum(&doc, ir, Point::default()).labelled_peaks;
    assert_eq!(labels(&ir), [(1756, 1756), (1788, 1788), (1050, 1050)]);

    for (anchor, partner) in [(1516, 1484), (1984, 2016)] {
        ir.bands[0][0] = f64::from(partner);
        ir.bands[1][0] = f64::from(anchor);
        assert_eq!(
            labels(&ir),
            [(anchor, anchor), (partner, partner), (1050, 1050)]
        );
    }
}

#[test]
fn separated_doublets_take_consecutive_slots_in_reserved_and_fill_labels() {
    let doc = Document::default();
    let mut ir = IrFigure {
        bands: vec![
            [1788., 80.],
            [1756., 90.],
            [1300., 200.],
            [1100., 150.],
            [900., 120.],
            [650., 100.],
            [500., 93.],
        ],
        labels: 6,
        ..IrFigure::default()
    };
    let labels = |ir: &IrFigure| figure::spectrum(&doc, ir, Point::default()).labelled_peaks;
    assert_eq!(
        labels(&ir),
        [1756, 1788, 1300, 1100, 900, 650].map(|peak| (peak, peak))
    );

    ir.bands[0][0] = 1488.;
    ir.bands[1][0] = 1456.;
    assert_eq!(
        labels(&ir),
        [1456, 1488, 1300, 1100, 900, 650].map(|peak| (peak, peak))
    );
}

#[test]
fn paper_figure_axes_and_ticks() {
    let doc = Document::default();
    let at = Point::new(13., 42.);
    let mut ir = acetic_acid();
    for columns in [Columns::One, Columns::Two] {
        ir.columns = columns;
        ir.minor_ticks = true;
        let figure = figure::spectrum(&doc, &ir, at);
        let bottom = &figure.graphics[1];
        let ticks: Vec<_> = figure
            .annotations
            .iter()
            .filter(|a| a.position.y > bottom.origin.y && a.text.parse::<u32>().is_ok())
            .collect();
        assert_eq!(
            ticks.iter().map(|a| a.text.as_str()).collect::<Vec<_>>(),
            [
                "4000", "3500", "3000", "2500", "2000", "1500", "1000", "500"
            ]
        );
        assert!(
            ticks
                .windows(2)
                .all(|pair| pair[0].bounds().1.x < pair[1].bounds().0.x)
        );
        let curve = figure.graphics[0].commands();
        let left = curve.first().unwrap().points()[0].x;
        let right = curve.last().unwrap().points()[0].x;
        assert!((ticks[0].centre().x - left).abs() < 1e-3);
        assert!((right - left - bottom.axis_x.x).abs() < 1e-3);
        assert!(
            curve
                .windows(2)
                .all(|pair| pair[0].points()[0].x <= pair[1].points()[0].x)
        );
        let title = figure
            .annotations
            .iter()
            .find(|a| a.text.starts_with("Wavenumber"))
            .unwrap();
        let [span] = title.format.spans.as_slice() else {
            panic!("expected a superscript")
        };
        assert_eq!(span.style.script, Script::Superscript);
        assert_eq!(&title.text[span.start..span.end], "−1");

        ir.minor_ticks = false;
        let major_only = super::figure::spectrum(&doc, &ir, at);
        assert_eq!(figure.graphics.len() - major_only.graphics.len(), 29);
    }
}

#[test]
fn ordinary_exports_fit_the_journal_dimensions() {
    let mut ir = acetic_acid();
    for columns in [Columns::One, Columns::Two] {
        ir.columns = columns;
        let (width, height) = columns.size_pt();
        for y in [YAxis::Transmittance, YAxis::Absorbance] {
            ir.y = y;
            for frame in [Frame::Box, Frame::Axes, Frame::Baseline] {
                ir.frame = frame;
                for vertical in [false, true] {
                    ir.vertical_labels = vertical;
                    let mut doc = Document::default();
                    figure::spectrum(&doc, &ir, Point::new(13., 42.)).add_to(&mut doc);
                    let svg = reshiki::export::figure(&doc, "svg").unwrap();
                    let xml = roxmltree::Document::parse(std::str::from_utf8(&svg.bytes).unwrap())
                        .unwrap();
                    let dimension = |attr| {
                        xml.root_element()
                            .attribute(attr)
                            .unwrap()
                            .trim_end_matches("pt")
                            .parse::<f32>()
                            .unwrap()
                    };
                    let (w, h) = (dimension("width"), dimension("height"));
                    let case = format!("{columns:?} {y:?} {frame:?} vertical={vertical}");
                    println!("{case}: {w:.3} × {h:.3} pt");
                    assert!(
                        (w - width).abs() <= 1. && (h - height).abs() <= 1.,
                        "{case}"
                    );
                }
            }
        }
    }
}

#[test]
fn strongest_band_reaches_twenty_percent_or_unit_absorbance() {
    let doc = Document::default();
    let mut ir = acetic_acid();
    for (y, gridline) in [(YAxis::Transmittance, "20"), (YAxis::Absorbance, "1.0")] {
        ir.y = y;
        let figure = figure::spectrum(&doc, &ir, Point::default());
        let base = figure.graphics[1].origin.y;
        let height = base - figure.graphics[2].origin.y;
        let strongest = figure.graphics[0]
            .commands()
            .iter()
            .flat_map(|c| c.points())
            .map(|p| p.y)
            .reduce(if y == YAxis::Transmittance {
                f32::max
            } else {
                f32::min
            })
            .unwrap();
        let tick = figure
            .annotations
            .iter()
            .find(|a| a.text == gridline)
            .unwrap();
        assert!((strongest - tick.centre().y).abs() < height * 1e-3);
        if y == YAxis::Transmittance {
            let top = figure.graphics[2].origin.y;
            let baseline = figure
                .annotations
                .iter()
                .find(|a| a.text == "100")
                .unwrap()
                .centre()
                .y;
            assert!(((baseline - top) / height - 5. / 105.).abs() < 1e-3);
        }
    }
}

#[test]
fn peak_labels_clear_labels_leaders_and_curve() {
    check_peak_label_layouts(acetic_acid(), |case, peaks| {
        let mut peaks = peaks.to_vec();
        peaks.sort_unstable();
        assert_eq!(
            peaks,
            [657, 1159, 1243, 1381, 1436, 1803, 3082, 3448],
            "{case}"
        );
    });
}

#[test]
fn aspirin_peak_labels_clear_labels_leaders_and_curve() {
    check_peak_label_layouts(aspirin(), |case, peaks| {
        println!("{case}: {peaks:?}");
        for number in [1788, 1756] {
            assert!(peaks.contains(&number), "{case}: missing {number}");
        }
        assert!(
            peaks.iter().any(|peak| (3450..=3452).contains(peak)),
            "{case}: missing O–H stretch label"
        );
        assert!(
            peaks.iter().any(|peak| (3000..=3120).contains(peak)),
            "{case}: missing C–H stretch label"
        );
    });
}

fn check_peak_label_layouts(mut ir: IrFigure, check_peaks: impl Fn(&str, &[u32])) {
    let doc = Document::default();
    let at = Point::new(13., 42.);
    let pad = doc.drawing_style.world(1.);
    for (columns, y, vertical) in [Columns::One, Columns::Two]
        .into_iter()
        .flat_map(|columns| {
            [YAxis::Transmittance, YAxis::Absorbance]
                .into_iter()
                .flat_map(move |y| [false, true].map(|vertical| (columns, y, vertical)))
        })
    {
        ir.columns = columns;
        ir.y = y;
        ir.vertical_labels = vertical;
        let case = format!("{columns:?} {y:?} vertical={vertical}");
        let chart = figure::spectrum(&doc, &ir, at);
        assert_eq!(chart, figure::spectrum(&doc, &ir, at));
        let printed: Vec<_> = chart
            .labelled_peaks
            .iter()
            .map(|&(_, printed)| printed)
            .collect();
        check_peaks(&case, &printed);
        let labels = &chart.annotations[chart.annotations.len() - chart.labelled_peaks.len()..];
        let leaders = &chart.graphics[chart.graphics.len() - labels.len()..];
        if columns == Columns::One {
            assert!(leaders.iter().any(|leader| leader.axis_x.x != 0.), "{case}");
        }
        let curve = chart.graphics[0].commands();
        for (i, label) in labels.iter().enumerate() {
            assert_eq!(label.angle, if vertical { -90. } else { 0. });
            let (lo, hi) = label.bounds();
            let (lo, hi) = (lo.offset(-pad, -pad), hi.offset(pad, pad));
            assert!(
                labels[..i].iter().all(|other| {
                    let (a, b) = other.bounds();
                    hi.x < a.x || hi.y < a.y || lo.x > b.x || lo.y > b.y
                }),
                "{case}: {} hits another label",
                label.text
            );
            assert!(
                leaders.iter().all(|leader| {
                    let a = leader.origin;
                    let b = a.offset(leader.axis_x.x, leader.axis_x.y);
                    !figure::hits_box(a, b, lo, hi)
                }),
                "{case}: {} hits a leader",
                label.text
            );
            assert!(
                curve
                    .windows(2)
                    .all(|s| { !figure::hits_box(s[0].points()[0], s[1].points()[0], lo, hi) }),
                "{case}: {} hits the curve",
                label.text
            );
            let a = leaders[i].origin;
            let b = a.offset(leaders[i].axis_x.x, leaders[i].axis_x.y);
            assert!((b.x - label.centre().x).abs() < 1e-3, "{case}");
            assert!(
                leaders[..i].iter().all(|other| {
                    let c = other.origin;
                    let d = c.offset(other.axis_x.x, other.axis_x.y);
                    !figure::segments_cross(a, b, c, d)
                }),
                "{case}: leader for {} hits another leader",
                label.text
            );
            let clear_start = a.lerp(b, doc.drawing_style.world(0.7) / a.distance(b));
            assert!(
                curve.windows(2).all(|s| {
                    !figure::segments_cross(clear_start, b, s[0].points()[0], s[1].points()[0])
                }),
                "{case}: leader for {} hits the curve",
                label.text
            );
            let axis_text = &chart.annotations[..chart.annotations.len() - labels.len()];
            assert!(
                axis_text.iter().all(|other| {
                    let (a, b) = other.bounds();
                    hi.x < a.x || hi.y < a.y || lo.x > b.x || lo.y > b.y
                }),
                "{case}: {} hits axis text",
                label.text
            );
            let axes = &chart.graphics[1..chart.graphics.len() - leaders.len()];
            assert!(
                axes.iter().enumerate().all(|(i, axis)| {
                    let a = axis.origin;
                    let b = a.offset(axis.axis_x.x, axis.axis_x.y);
                    let inset = if i < 4 { 0. } else { pad };
                    !figure::hits_box(a, b, lo.offset(inset, inset), hi.offset(-inset, -inset))
                }),
                "{case}: {} hits an axis",
                label.text
            );
        }
    }
}

#[test]
fn hidden_peaks_are_not_replaced_and_method_text_fits_one_line() {
    use reshiki::typography::layout;

    let doc = Document::default();
    let mut ir = acetic_acid();
    ir.labels = 1;
    ir.hidden_labels.push(1803);
    ir.vertical_labels = true;
    ir.frame = Frame::Baseline;
    ir.method_line = true;
    let build = |ir: &IrFigure| figure::spectrum(&doc, ir, Point::default());
    let edited = build(&ir);
    assert!(edited.labelled_peaks.is_empty());
    assert!(
        edited
            .annotations
            .iter()
            .all(|a| a.text != "Transmittance (%)" && a.text != "1803")
    );
    assert_eq!(
        edited.annotations[0].text,
        "GFN2-xTB · Lorentzian · FWHM 20 cm−1 · scale 1.000"
    );
    let method = &edited.annotations[0];
    let [span] = method.format.spans.as_slice() else {
        panic!("expected a superscript")
    };
    assert_eq!(span.style.script, Script::Superscript);
    assert_eq!(&method.text[span.start..span.end], "−1");
    ir.method = format!("{}\nsecond line", "W".repeat(70));
    for frame in [Frame::Box, Frame::Axes, Frame::Baseline] {
        ir.frame = frame;
        let chart = build(&ir);
        let method = &chart.annotations[0];
        assert!(method.text.contains('…'));
        assert!(!method.text.contains('\n'));
        assert!(!method.text.contains("second line"));
        let plot = &chart.graphics[1];
        assert!(method.bounds().1.x <= plot.origin.x + plot.axis_x.x);
        let height = layout(&method.text, &method.format).height;
        assert_eq!(method.size().1, height);
        let plain = build(&IrFigure {
            method_line: false,
            ..ir.clone()
        });
        for (with, without) in chart.graphics[0].path.iter().zip(&plain.graphics[0].path) {
            let delta = with.points()[0].y - without.points()[0].y;
            assert!((0. ..=height + 1e-3).contains(&delta));
        }
    }
    ir.bands = vec![[3000., 1.]];
    ir.hidden_labels.clear();
    ir.y = YAxis::Absorbance;
    assert_eq!(build(&ir).labelled_peaks, [(3000, 3000)]);
}

#[tokio::test]
async fn placement_and_caption_use_the_palette_settings() {
    let mut app = opened();
    app.tab.path = Some("water.rsk".into());
    let unscaled = app
        .xtb_shown()
        .unwrap()
        .stored
        .result
        .vibrations
        .as_ref()
        .unwrap()
        .frequencies_cm1[0];
    for action in [
        Action::Y(YAxis::Absorbance),
        Action::From("3900".into()),
        Action::To("600".into()),
        Action::Shape(BandShape::Gaussian),
        Action::Fwhm(25.),
        Action::Scale("0.960".into()),
    ] {
        vibrations(&mut app, action);
    }
    use iced::futures::StreamExt;
    let caption = app.xtb_copy_caption_with(|contents| async move {
        assert_eq!(
            contents,
            "Calculated IR spectrum of water (GFN2-xTB, Gaussian broadening, FWHM 25 cm⁻¹, frequencies scaled by 0.960)."
        );
        Ok(())
    });
    let mut stream = iced_runtime::task::into_stream(caption).unwrap();
    assert_eq!(app.status, "Copying the IR caption…");
    assert!(app.copy_as_busy);
    let Some(iced_runtime::Action::Output(message)) = stream.next().await else {
        panic!("expected a clipboard receipt")
    };
    let _ = app.update(message);
    assert_eq!(app.status, "Copied the IR caption");
    assert!(!app.error && !app.copy_as_busy);

    vibrations(&mut app, Action::PlaceSpectrum);
    let ir = app.tab.doc.graphics[0].ir.as_ref().unwrap();
    assert_eq!(ir.y, YAxis::Absorbance);
    assert_eq!(ir.shape, BandShape::Gaussian);
    assert_eq!(
        (ir.from, ir.to, ir.fwhm, ir.scale),
        (3900., 600., 25., 0.96)
    );
    assert_eq!(ir.bands[0][0], unscaled);
}

#[tokio::test]
async fn caption_copy_reports_failure_only_after_the_clipboard_receipt() {
    use iced::futures::{FutureExt, StreamExt};

    let mut app = opened();
    let (send, receive) = tokio::sync::oneshot::channel();
    let copy = app.xtb_copy_caption_with(|_| async move { receive.await.unwrap() });
    let mut stream = iced_runtime::task::into_stream(copy).unwrap();
    assert!(stream.next().now_or_never().is_none());
    assert_eq!(app.status, "Copying the IR caption…");
    send.send(Err("The browser did not allow clipboard access".into()))
        .unwrap();
    let Some(iced_runtime::Action::Output(message)) = stream.next().await else {
        panic!("expected a failed clipboard receipt")
    };
    let _ = app.update(message);
    assert!(app.error && !app.copy_as_busy);
    assert_eq!(
        app.status,
        "Could not copy the IR caption\nThe browser did not allow clipboard access"
    );
}

#[test]
fn placement_keeps_only_the_most_intense_3000_bands() {
    let mut app = opened();
    let mut result = (*app.xtb_shown().unwrap().stored.result).clone();
    let bands = result.vibrations.as_mut().unwrap();
    bands.frequencies_cm1 = vec![1500.; 3002];
    bands.ir_intensities_km_mol = (1..=3002).map(f64::from).collect();
    let result = Arc::new(result);
    app.tab.doc.science.results.first_mut().unwrap().result = result.clone();
    app.tab.xtb.palette.show(&result);

    vibrations(&mut app, Action::PlaceSpectrum);
    let ir = app
        .tab
        .doc
        .graphics
        .iter()
        .find_map(|g| g.ir.as_deref())
        .unwrap();
    assert_eq!(ir.bands.len(), 3000);
    assert!(
        ir.bands
            .iter()
            .map(|band| band[1])
            .eq((3..=3002).rev().map(f64::from))
    );
}

#[test]
fn spectrum_actions_are_inert_when_the_spectrum_cannot_be_plotted() {
    for modify in [
        (|v: &mut reshiki::qcdata::Vibrations| v.ir_intensities_km_mol.clear()) as fn(&mut _),
        |v| v.ir_intensities_km_mol.fill(0.),
    ] {
        let mut app = opened();
        let mut result = (*app.xtb_shown().unwrap().stored.result).clone();
        modify(result.vibrations.as_mut().unwrap());
        let result = Arc::new(result);
        app.tab.doc.science.results.first_mut().unwrap().result = result.clone();
        app.tab.xtb.palette.show(&result);
        let before = app.tab.doc.clone();
        vibrations(&mut app, Action::PlaceSpectrum);
        for format in ["svg", "pdf"] {
            let export = app.xtb_vibrations_action(Action::ExportSpectrum(format));
            assert_eq!(export.units(), 0);
        }
        assert_eq!(app.tab.doc, before);
    }
    for invalid in [Action::To("3900".into()), Action::Scale("2".into())] {
        let mut app = opened();
        vibrations(&mut app, invalid);
        vibrations(&mut app, Action::PlaceSpectrum);
        assert!(app.tab.doc.graphics.is_empty());
        assert_eq!(
            app.xtb_vibrations_action(Action::ExportSpectrum("svg"))
                .units(),
            0
        );
        assert_eq!(app.xtb_vibrations_action(Action::CopyCaption).units(), 0);
    }
}

#[tokio::test]
async fn spectrum_exports_use_publication_for_svg_and_pdf() {
    use iced::futures::StreamExt;

    let mut app = opened();
    app.tab.path = Some("water.rsk".into());
    app.appearance.export_editable_drawing = true;
    let before = app.tab.doc.clone();
    for (format, live_text) in [("svg", false), ("svg", true), ("pdf", false)] {
        app.appearance.export_svg_live_text = live_text;
        let task = app.xtb_vibrations_action(Action::ExportSpectrum(format));
        let mut stream = iced_runtime::task::into_stream(task).unwrap();
        let Some(iced_runtime::Action::Output(Message::ExportReady(export))) = stream.next().await
        else {
            panic!("Expected a prepared {format} spectrum export");
        };
        assert_eq!(export.name, format!("water IR spectrum.{format}"));
        assert!(matches!(export.kind, Kind::Ir));
        if format == "svg" {
            let svg = std::str::from_utf8(&export.bytes).unwrap();
            assert!(svg.starts_with("<svg") || svg.starts_with("<?xml"));
            let xml = roxmltree::Document::parse(svg).unwrap();
            for (attr, expected) in [("width", 234.), ("height", 150.)] {
                let points = xml
                    .root_element()
                    .attribute(attr)
                    .unwrap()
                    .trim_end_matches("pt")
                    .parse::<f32>()
                    .unwrap();
                assert!((points - expected).abs() <= 1., "{attr}: {points}");
            }
            assert_eq!(
                xml.descendants().any(|node| node.has_tag_name("text")),
                live_text
            );
            assert!(xml.descendants().any(|node| node.has_tag_name("path")));
        } else {
            assert!(export.bytes.starts_with(b"%PDF"));
            let pdf = String::from_utf8_lossy(&export.bytes);
            let media_box = pdf
                .split_once("/MediaBox [")
                .unwrap()
                .1
                .split_once(']')
                .unwrap()
                .0;
            let size: Vec<f32> = media_box
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            assert!((size[2] - 234.).abs() <= 1.);
            assert!((size[3] - 150.).abs() <= 1.);
        }
        let standalone = reshiki::payload::read(&export.bytes).unwrap().unwrap();
        assert!(standalone.atoms.is_empty() && standalone.models.is_empty());
        assert_eq!(standalone.groups.len(), 1);
        assert_eq!(standalone.graphics[0].ir.as_ref().unwrap().name, "water");
        assert!(standalone.annotations.iter().any(|a| a.text == "1590"));
        let file_name = format!("water-ir.{format}");
        let path = std::path::Path::new("/saved/calculations").join(&file_name);
        let _ = app.update(export.completed(Ok(Some(path.clone()))));
        assert_eq!(
            app.status,
            format!("Exported {file_name}\n{}", path.display())
        );
    }
    assert_eq!(app.tab.doc, before);
}

#[test]
fn thermochemistry_changes_conditions_only_if_it_reproduces_the_saved_gibbs_correction() {
    let app = opened();
    let shown = app.xtb_shown().unwrap();
    let result = &*shown.stored.result;
    let vibrations = result.vibrations.as_ref().unwrap();
    let mut input = vibrational::ThermoInput {
        frequencies_cm1: &vibrations.frequencies_cm1,
        masses_amu: &vibrations.masses_amu,
        positions_angstrom: &shown.frame.positions_angstrom,
        linear: None,
        symmetry_number: 2,
        temperature_k: 298.15,
        pressure_pa: 101_325.,
        frequency_scale: 1.,
        rotor_cutoff_cm1: 50.,
        imaginary_cutoff_cm1: 20.,
    };
    assert!(!input.is_linear());
    let mut result = result.clone();
    for linear in [None, Some(true)] {
        input.linear = linear;
        input.temperature_k = 298.15;
        let mut saved = vibrational::thermochemistry(&input);
        saved.point_group = Some("c2v".into());
        result.thermo = Some(saved.clone());

        let hot = thermo_at(&result, shown.frame, 500., 101_325.).unwrap();
        input.temperature_k = 500.;
        assert_eq!(
            hot.gibbs_correction_hartree,
            vibrational::thermochemistry(&input).gibbs_correction_hartree
        );
        assert!(hot.entropy_hartree_per_k > saved.entropy_hartree_per_k);
        assert_eq!(hot.point_group, saved.point_group);
        assert!(thermo_at(&result, shown.frame, 0., 101_325.).is_none());

        saved.gibbs_correction_hartree += 0.01;
        result.thermo = Some(saved);
        assert!(thermo_at(&result, shown.frame, 500., 101_325.).is_none());
    }
    result.thermo.as_mut().unwrap().symmetry_number = 0;
    assert!(thermo_at(&result, shown.frame, 500., 101_325.).is_none());
}

#[test]
fn typed_conditions_have_finite_physical_bounds() {
    let mut settings = State {
        pressure: "1e-7".into(),
        ..State::default()
    };
    assert!(settings.conditions().is_none());
    settings.pressure = "2".into();
    assert_eq!(settings.conditions(), Some((298.15, 202_650.)));
    settings.temperature = "0.9".into();
    assert!(settings.conditions().is_none());
}

/// Water whose Frequencies result has one imaginary mode.
fn saddle() -> App {
    let (mut app, _) = water(CalcTask::Frequencies);
    let mut report = optfreq(&START, &START);
    report.steps = 0;
    report.path.clear();
    report.energies = vec![-5.070176];
    if let Some(v) = &mut report.computed.vibrations {
        v.frequencies_cm1 = vec![-212., 1590.13, 3468.58];
    }
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    app
}

#[test]
fn push_starts_opt_freq_from_the_displaced_geometry() {
    let mut app = saddle();
    let accepted = app.xtb_shown().unwrap().frame.positions_angstrom.clone();
    vibrations(&mut app, Action::Push);
    let run = app.tab.xtb.run.as_ref().expect("Push starts a run");
    assert_eq!(run.config.task, CalcTask::OptFreq);
    let start = run.start.as_ref().expect("a pushed start");
    // Mode 1 moves atom 1 along x by the push distance.
    let [[x, y, z], second, third] = accepted.as_slice() else {
        panic!("expected three atoms");
    };
    assert_eq!(
        start.positions,
        [[x + PUSH_ANGSTROM, *y, *z], *second, *third]
    );
    assert_eq!(start.from, accepted);
    assert_eq!(
        run.launch.as_ref().unwrap().frame.positions_angstrom,
        start.positions
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
}

#[test]
fn push_is_refused_if_the_model_moved_after_calculation() {
    let mut app = saddle();
    let shown = app.xtb_shown().unwrap();
    let mut config =
        super::super::results::config(&shown.stored.result, shown.frame, shown.atoms, 1).unwrap();
    config.task = CalcTask::OptFreq;
    let atom = *shown.atoms.iter().flatten().next().unwrap();
    let request = Request {
        atoms: vec![atom],
        config,
        spin_typed: false,
        keep_full: false,
        start: Some(Start {
            positions: vibrational::displaced(
                &START,
                &[1., 0., 0., 0., 0., 0., 0., 0., 0.],
                PUSH_ANGSTROM,
            ),
            from: START.to_vec(),
        }),
        search: None,
        ts: None,
        reaction: None,
    };
    app.tab.doc = super::super::preview(&app.tab.doc, atom, &MOVED).unwrap();
    vibrations(&mut app, Action::Push);
    assert!(app.tab.xtb.run.is_none());
    assert_eq!(app.status, "The 3D model moved after this calculation");

    // Reject a request made before the move even at the worker boundary.
    let _ = app.xtb_begin(request);
    assert!(app.tab.xtb.run.is_none());
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Failed(failed))
            if matches!(failed.failure.reason(), state::Failure::Other(message)
                if message == "The 3D model moved after this calculation")
    ));
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
}

#[test]
fn a_different_model_geometry_disables_animation() {
    let mut app = opened();
    let result = app.xtb_shown().unwrap();
    let atom = *result.atoms.iter().flatten().next().unwrap();
    app.tab.doc = super::super::preview(&app.tab.doc, atom, &START).unwrap();
    app.xtb_vibrations_sync();
    assert!(app.xtb_vibration_preview().is_none());
    assert_eq!(shown(&app), f32s(&START));
}

#[test]
fn an_unconverged_push_keeps_the_accepted_structure() {
    let mut app = saddle();
    vibrations(&mut app, Action::Push);
    let start = app.tab.xtb.run.as_ref().unwrap().start.as_ref().unwrap();
    let mut report = optfreq(&start.positions, &MOVED);
    report.converged = false;
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    assert_eq!(model(&app.tab.doc), f32s(&START), "{}", app.status);
    assert_eq!(app.tab.xtb.palette.view, View::Summary);
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { notes, .. })
            if notes.iter().any(|n| n.contains("accepted structure is unchanged"))
    ));
    assert_eq!(app.tab.doc.science.results.len(), 2, "the result is kept");
}

#[test]
fn a_converged_push_replaces_the_structure_and_shows_the_vibrations() {
    let mut app = saddle();
    vibrations(&mut app, Action::Push);
    let displaced = app
        .tab
        .xtb
        .run
        .as_ref()
        .unwrap()
        .start
        .as_ref()
        .unwrap()
        .positions
        .clone();
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(optfreq(&displaced, &MOVED)))),
    );
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
    assert_eq!(app.tab.xtb.palette.view, View::Vibrations);
    let shown = app.xtb_shown().unwrap();
    assert_eq!(shown.input.positions_angstrom, displaced);
    let light = shown.stored.result.light(&[]);
    assert_eq!(light.input_frame, Some(shown.input.hash()));
    assert_eq!(
        super::super::results::step_positions(&light, 0, shown.input, shown.frame),
        Some(displaced)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), f32s(&START), "one Undo step");
}

fn light_result(frequency: f64) -> (App, Arc<CalcResult>) {
    let (mut app, _) = water(CalcTask::OptFreq);
    let mut report = optfreq(&START, &MOVED);
    *report
        .computed
        .vibrations
        .as_mut()
        .unwrap()
        .frequencies_cm1
        .first_mut()
        .unwrap() = frequency;
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    let stored = app.tab.doc.science.results.first_mut().unwrap();
    let light = Arc::new(stored.result.light(&[]));
    assert!(light.vibrations.as_ref().unwrap().modes.is_empty());
    stored.result = light.clone();
    app.tab.xtb.palette.show(&light);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
    (app, light)
}

/// Answer the pending recompute; only engine builds start one.
#[cfg(feature = "xtb")]
fn recomputed(app: &mut App, frequency: f64) {
    let mut report = optfreq(&MOVED, &MOVED);
    *report
        .computed
        .vibrations
        .as_mut()
        .unwrap()
        .frequencies_cm1
        .first_mut()
        .unwrap() = frequency;
    let pending = app.tab.xtb.vibrations.recompute.as_ref().unwrap();
    vibrations(
        app,
        Action::Recomputed(pending.epoch, pending.serial, Ok(Arc::new(report))),
    );
}

#[test]
#[cfg(feature = "xtb")]
fn a_light_result_recomputes_its_modes_when_the_tab_opens() {
    let (mut app, light) = light_result(1590.13);
    assert!(app.tab.xtb.vibrations.recomputing(&light));
    recomputed(&mut app, 1592.13);
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.xtb_modes(&light).is_some());
    assert!(app.tab.xtb.vibrations.animation.is_some());
    assert!(!app.tab.xtb.vibrations.playing);
    let names = app.xtb_motion_names(&app.xtb_shown().unwrap()).unwrap();
    assert_eq!(names[0], "O–H stretch");
    assert!(app.status.contains("differ by up to 2.0 cm⁻¹"));
}

#[test]
#[cfg(feature = "xtb")]
fn recomputed_modes_require_matching_frequency_signs_and_values() {
    for (saved, fresh, drift) in [(-0.5, 0.5, "1.0"), (1590.13, 1593.13, "3.0")] {
        let (mut app, light) = light_result(saved);
        recomputed(&mut app, fresh);
        assert!(app.xtb_modes(&light).is_none());
        assert!(app.xtb_vibration_preview().is_none());
        assert_eq!(
            app.tab.xtb.vibrations.problem.as_ref().unwrap().1,
            format!(
                "The recomputed frequencies differ from the saved ones (up to {drift} cm⁻¹); the modes are not shown."
            )
        );
        send(
            &mut app,
            XtbAction::Palette(palette::Action::View(View::Summary)),
        );
        send(
            &mut app,
            XtbAction::Palette(palette::Action::View(View::Vibrations)),
        );
        assert!(app.tab.xtb.vibrations.problem.is_none());
        assert!(app.tab.xtb.vibrations.recomputing(&light));
    }
}

#[test]
#[cfg(feature = "xtb")]
fn a_recompute_reply_from_a_previous_file_epoch_is_ignored() {
    let (mut app, light) = light_result(1590.13);
    app.tab.file_epoch += 1;
    recomputed(&mut app, 1590.13);
    assert!(app.xtb_modes(&light).is_none());
    assert!(app.xtb_vibration_preview().is_none());
}

#[test]
#[cfg(feature = "xtb")]
fn switching_away_from_docked_vibrations_cancels_recomputation() {
    let (mut app, light) = light_result(1590.13);
    send(&mut app, XtbAction::Palette(palette::Action::Dock));
    send(&mut app, XtbAction::Page(crate::app::xtb::Page::Nmr));
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    send(&mut app, XtbAction::Page(crate::app::xtb::Page::Xtb));
    assert!(app.tab.xtb.vibrations.recomputing(&light));
}

#[test]
#[cfg(feature = "xtb")]
fn undoing_the_result_cancels_recomputation() {
    let (mut app, light) = light_result(1590.13);
    let _ = app.update(Message::Undo);
    assert!(app.xtb_shown().is_none());
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
}

#[test]
fn animation_ticks_leave_the_view_and_style_choosers_open() {
    let mut app = opened();
    app.view_open = true;
    app.status_style_open = true;
    let _ = app.update(Message::Xtb(XtbAction::Vibrations(Action::Tick)));
    assert!(app.view_open && app.status_style_open);
}

/// A build without the engine keeps the
/// saved frequencies and explains why nothing animates.
#[test]
#[cfg(not(feature = "xtb"))]
fn without_the_engine_a_light_result_does_not_recompute() {
    let (app, light) = light_result(1590.13);
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.xtb_modes(&light).is_none());
}
