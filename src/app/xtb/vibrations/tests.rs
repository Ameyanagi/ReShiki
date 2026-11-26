//! The Vibrations tab on a water Opt + freq result answered by hand.
use super::super::tests::{MOVED, START, event, f32s, model, optfreq, optimized, send, water};
use super::super::{Action as XtbAction, CalcTask, Outcome, palette, results::View, run};
use super::*;
use crate::app::Message;
use reshiki::graphics::GraphicKind;
use reshiki::models3d::{self, Display};

fn vibrations(app: &mut App, action: Action) {
    send(app, XtbAction::Vibrations(action));
}

/// Water after Opt + freq, with the Vibrations tab open.
fn opened() -> App {
    let (mut app, _) = water(CalcTask::OptFreq);
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(optfreq(&START, &MOVED)))),
    );
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    app
}

/// The model positions the canvas shows now.
fn shown(app: &App) -> Vec<[f32; 3]> {
    model(app.display_document())
}

#[test]
fn opening_the_tab_animates_the_selected_mode_without_touching_the_document() {
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
    assert_eq!(shown(&app), f32s(&MOVED));
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
    assert!(app.status.starts_with("Mode 1 · 1590 cm⁻¹ · animating"));

    vibrations(&mut app, Action::Play(false));
    assert!(app.status.contains("paused"));
    assert_eq!(shown(&app), moved, "Pause keeps the current frame");

    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Summary)),
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
    assert_eq!(shown(&app), f32s(&MOVED));
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
fn another_mode_or_amplitude_rebuilds_the_frames() {
    let mut app = opened();
    vibrations(&mut app, Action::Mode(2));
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
    let bond = before.drawing_style.bond_length_world;

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
    assert!((900..=908).contains(&curve.path.len()));
    assert_eq!(curve.style.width_pt, doc.drawing_style.line_width_pt);
    assert!(lines.iter().all(|g| g.kind == GraphicKind::Line));
    let points: Vec<_> = curve
        .commands()
        .iter()
        .flat_map(|command| command.points())
        .collect();
    let width = points.last().unwrap().x - points[0].x;
    assert!((width - super::figure::PLOT_WIDTH_BONDS * bond).abs() < 1e-3);
    assert!(points.windows(2).all(|pair| pair[0].x <= pair[1].x));
    let tick_labels: Vec<_> = doc
        .annotations
        .iter()
        .filter(|a| a.text.parse::<u32>().is_ok() && a.position.y > lines[0].origin.y)
        .collect();
    assert_eq!(
        tick_labels
            .iter()
            .map(|a| a.text.as_str())
            .collect::<Vec<_>>(),
        ["4000", "3000", "2000", "1000"]
    );
    assert!(
        tick_labels
            .windows(2)
            .all(|pair| pair[0].position.x < pair[1].position.x)
    );
    assert!(doc.annotations.iter().any(|a| a.text == "1590"));
    assert_eq!(
        doc.annotations[0].text,
        "GFN2-xTB · scale 1.00 · FWHM 15 cm⁻¹ · Lorentzian"
    );
    assert_eq!(app.status, "Placed the IR spectrum · ⌘Z undoes");
    vibrations(&mut app, Action::Tick);
    assert_eq!(app.status, "Placed the IR spectrum · ⌘Z undoes");

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
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
    assert!((lo.x - expected_x).abs() < 1e-3);
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
    assert!((lo.x - farthest_x - bond).abs() < 1e-3);
}

#[test]
fn the_editable_figure_keeps_the_palette_peaks_and_is_deterministic() {
    let mut app = opened();
    let mut result = (*app.xtb_shown().unwrap().stored.result).clone();
    result.vibrations.as_mut().unwrap().ir_intensities_km_mol = vec![127.68, 14.05, 10.];
    let result = Arc::new(result);
    app.tab.doc.science.results.first_mut().unwrap().result = result.clone();
    app.tab.xtb.palette.show(&result);

    let settings = &mut app.tab.xtb.vibrations;
    settings.scale = "1.104".into();
    settings.shape = BandShape::Gaussian;
    settings.fwhm = 5.;
    let data = view::SpectrumData::cached(&result, settings);
    let doc = &app.tab.doc;
    let at = Point::new(13., 42.);
    let build = || figure::spectrum(doc, &data, "GFN2-xTB", at);
    let figure = build();
    assert_eq!(figure, build());
    assert_eq!(
        figure.annotations[0].text,
        "GFN2-xTB · scale 1.104 · FWHM 5 cm⁻¹ · Gaussian"
    );

    let curve = &figure.graphics[0];
    let points: Vec<_> = curve
        .commands()
        .iter()
        .flat_map(|command| command.points())
        .collect();
    let bond = doc.drawing_style.bond_length_world;
    let base = figure.graphics[1].origin.y;
    let top = data
        .curve
        .iter()
        .chain(&data.peaks)
        .map(|p| p[1])
        .fold(0., f64::max);
    let [weaker, stronger, bend] = data.peaks.as_slice() else {
        panic!("expected three separated maxima");
    };
    assert!(weaker[1] < stronger[1]);
    for &[wavenumber, intensity] in &data.peaks {
        let peak_x = points[0].x + data.x(wavenumber) * super::figure::PLOT_WIDTH_BONDS * bond;
        let peak_y = base - (intensity / top) as f32 * super::figure::PLOT_HEIGHT_BONDS * bond;
        assert!(
            points
                .iter()
                .any(|p| p.distance(Point::new(peak_x, peak_y)) < 1e-3)
        );
    }
    for &[wavenumber, intensity] in [stronger, bend] {
        let peak_x = points[0].x + data.x(wavenumber) * super::figure::PLOT_WIDTH_BONDS * bond;
        let peak_y = base - (intensity / top) as f32 * super::figure::PLOT_HEIGHT_BONDS * bond;
        let text = format!("{wavenumber:.0}");
        assert!(figure.annotations.iter().any(|a| {
            a.text == text && (a.centre().x - peak_x).abs() < 1e-3 && a.position.y < peak_y
        }));
    }
    let skipped = format!("{:.0}", weaker[0]);
    assert!(figure.annotations.iter().all(|a| a.text != skipped));
}

#[test]
fn the_ninth_band_keeps_its_sampled_height_without_a_label() {
    let app = opened();
    let mut result = (*app.xtb_shown().unwrap().stored.result).clone();
    let bands = result.vibrations.as_mut().unwrap();
    bands.frequencies_cm1 = [750, 650, 550, 449, 300, 250, 200, 150, 100]
        .map(|index| 4000. - 3600. * (f64::from(index) / 899.))
        .into();
    bands.ir_intensities_km_mol = vec![100., 100., 100., 99., 100., 100., 100., 100., 100.];
    let settings = State {
        shape: BandShape::Gaussian,
        fwhm: 5.,
        ..State::default()
    };
    let data = view::SpectrumData::cached(&Arc::new(result), &settings);
    let [frequency, intensity] = data.curve[449];
    let top = data.curve.iter().map(|p| p[1]).fold(0., f64::max);
    assert!((intensity / top - 0.99).abs() < 1e-8);
    let label = format!("{frequency:.0}");
    assert_eq!(data.peaks.len(), 8);
    assert!(data.peaks.iter().all(|p| format!("{:.0}", p[0]) != label));

    let doc = &app.tab.doc;
    let figure = figure::spectrum(doc, &data, "GFN2-xTB", Point::default());
    let axis = &figure.graphics[1];
    let bond = doc.drawing_style.bond_length_world;
    let apex = axis.origin.offset(
        data.x(frequency) * super::figure::PLOT_WIDTH_BONDS * bond,
        -(intensity / top) as f32 * super::figure::PLOT_HEIGHT_BONDS * bond,
    );
    assert!(figure.graphics[0].commands().iter().any(|command| {
        command
            .points()
            .iter()
            .any(|point| point.distance(apex) < 1e-3)
    }));
    assert!(figure.annotations.iter().all(|a| a.text != label));
}

#[test]
fn wide_spectrum_tick_labels_have_clearance_and_keep_the_minor_ticks() {
    let app = opened();
    let mut result = (*app.xtb_shown().unwrap().stored.result).clone();
    result.vibrations.as_mut().unwrap().frequencies_cm1 = vec![1590., 3469., 9900.];
    let data = view::SpectrumData::cached(&Arc::new(result), &State::default());
    let doc = &app.tab.doc;
    let figure = figure::spectrum(doc, &data, "GFN2-xTB", Point::default());
    let axis = &figure.graphics[1];
    let ticks = &figure.graphics[2..];
    assert_eq!(ticks.len(), 20);
    let step = super::figure::PLOT_WIDTH_BONDS * doc.drawing_style.bond_length_world * 500. / 9600.;
    assert!(
        ticks
            .windows(2)
            .all(|pair| (pair[1].origin.x - pair[0].origin.x - step).abs() < 1e-3)
    );
    let labels: Vec<_> = figure
        .annotations
        .iter()
        .filter(|a| a.position.y > axis.origin.y && a.text.parse::<u32>().is_ok())
        .collect();
    assert!(labels.len() > 1);
    assert!(
        labels
            .windows(2)
            .all(|pair| pair[0].bounds().1.x < pair[1].bounds().0.x)
    );
}

#[test]
fn spectrum_actions_are_inert_when_the_spectrum_cannot_be_plotted() {
    for modify in [
        (|v: &mut reshiki::qcdata::Vibrations| v.ir_intensities_km_mol.clear()) as fn(&mut _),
        |v| v.ir_intensities_km_mol.fill(0.),
        |v| *v.frequencies_cm1.first_mut().unwrap() = 20_000.,
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
        assert!(matches!(export.kind, Kind::Xtb));
        if format == "svg" {
            let svg = std::str::from_utf8(&export.bytes).unwrap();
            assert!(svg.starts_with("<svg") || svg.starts_with("<?xml"));
            let xml = roxmltree::Document::parse(svg).unwrap();
            assert_eq!(
                xml.descendants().any(|node| node.has_tag_name("text")),
                live_text
            );
            assert!(xml.descendants().any(|node| node.has_tag_name("path")));
        } else {
            assert!(export.bytes.starts_with(b"%PDF"));
        }
        let standalone = reshiki::payload::read(&export.bytes).unwrap().unwrap();
        assert!(standalone.atoms.is_empty() && standalone.models.is_empty());
        assert_eq!(standalone.groups.len(), 1);
        assert!(standalone.annotations.iter().any(|a| a.text == "1590"));
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
        Some(Outcome::Failed(failed)) if failed.failure == state::Failure::Other(
            "The 3D model moved after this calculation".into()
        )
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
#[cfg(all(not(windows), feature = "xtb"))]
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
#[cfg(all(not(windows), feature = "xtb"))]
fn a_light_result_recomputes_its_modes_when_the_tab_opens() {
    let (mut app, light) = light_result(1590.13);
    assert!(app.tab.xtb.vibrations.recomputing(&light));
    recomputed(&mut app, 1592.13);
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.xtb_modes(&light).is_some());
    assert!(app.tab.xtb.vibrations.animation.is_some());
    assert!(app.status.contains("differ by up to 2.0 cm⁻¹"));
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
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
#[cfg(all(not(windows), feature = "xtb"))]
fn a_recompute_reply_from_a_previous_file_epoch_is_ignored() {
    let (mut app, light) = light_result(1590.13);
    app.tab.file_epoch += 1;
    recomputed(&mut app, 1590.13);
    assert!(app.xtb_modes(&light).is_none());
    assert!(app.xtb_vibration_preview().is_none());
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
fn switching_away_from_docked_vibrations_cancels_recomputation() {
    let (mut app, light) = light_result(1590.13);
    send(&mut app, XtbAction::Palette(palette::Action::Dock));
    send(&mut app, XtbAction::Page(Page::Nmr));
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    send(&mut app, XtbAction::Page(Page::Xtb));
    assert!(app.tab.xtb.vibrations.recomputing(&light));
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
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

/// A build without the engine (or Windows, until code signing) keeps the
/// saved frequencies and explains why nothing animates.
#[test]
#[cfg(any(windows, not(feature = "xtb")))]
fn without_the_engine_a_light_result_does_not_recompute() {
    let (app, light) = light_result(1590.13);
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.xtb_modes(&light).is_none());
}
