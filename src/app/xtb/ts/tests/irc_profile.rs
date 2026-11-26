use super::*;
use crate::appearance::EnergyUnit;
use reshiki::qcdata::{IrcEndpoint, IrcPath, IrcRelaxedEnd, KCAL_PER_HARTREE};

pub(in crate::app::xtb::ts) fn finished() -> (App, u64) {
    let (mut doc, arrow) = card::tests::claisen();
    name_reactant(&mut doc, arrow, "Allyl vinyl ether");
    let mut app = reaction_app(doc, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = Arc::unwrap_or_clone(example_report(run.launch.as_ref().unwrap()));
    for frame in &mut report.frames {
        frame.masses_amu = frame
            .numbers
            .iter()
            .map(|&z| match z {
                1 => 1.008,
                6 => 12.011,
                8 => 15.999,
                _ => unreachable!(),
            })
            .collect();
    }
    let [ts, r, p] = report.frames.as_slice() else {
        unreachable!()
    };
    report.result.frame = ts.hash();
    report.result.endpoints = Some(Endpoints {
        reactant: r.hash(),
        product: p.hash(),
    });
    let band = report.result.trajectory.as_mut().unwrap();
    band.masses_amu.clone_from(&ts.masses_amu);
    band.energies_hartree[0] = -5.0002;
    let mut trajectory = band.clone();
    trajectory.positions_angstrom = [r, ts, p]
        .into_iter()
        .flat_map(|frame| frame.positions_angstrom.iter().flatten().copied())
        .collect();
    trajectory.energies_hartree[0] = -5.001;
    trajectory.energies_hartree[1] = report.result.energy_hartree.unwrap();
    let relaxed = [(r, -5.003), (p, -5.024)].map(|(frame, energy_hartree)| {
        let mut positions_angstrom: Vec<_> =
            frame.positions_angstrom.iter().flatten().copied().collect();
        positions_angstrom[0] += 0.3;
        IrcRelaxedEnd {
            energy_hartree,
            positions_angstrom,
        }
    });
    let mut path = IrcPath::new(
        trajectory,
        1,
        [IrcEndpoint::Reactant, IrcEndpoint::Product],
        [true; 2],
        relaxed,
    )
    .unwrap();
    path.neb_arc_lengths_sqrt_amu_bohr = path.arc_lengths_sqrt_amu_bohr.clone();
    report.extras.irc = Some(path);
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(Arc::new(report)))),
    );
    assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
    (app, arrow)
}

#[test]
fn default_profile_uses_a_completed_or_connected_irc_and_keeps_partial_traces_selectable() {
    use IrcEndpoint::{Other, Product, Reactant};
    let (mut app, _) = finished();
    let mut result = (*app.xtb_shown().unwrap().stored.result).clone();
    for (completed, reached, default) in [
        ([false; 2], [Reactant, Product], irc::Profile::Irc),
        ([true; 2], [Other, Other], irc::Profile::Irc),
        ([true, false], [Reactant, Other], irc::Profile::Neb),
    ] {
        let search = result
            .run
            .as_mut()
            .unwrap()
            .transition_state
            .as_mut()
            .unwrap();
        let path = search.irc.as_mut().unwrap();
        path.completed = completed;
        path.reached = reached;
        app.tab.xtb.palette.profile = None;
        assert_eq!(app.xtb_profile(&result), default);
        app.tab.xtb.palette.profile = Some(irc::Profile::Irc);
        assert_eq!(app.xtb_profile(&result), irc::Profile::Irc);
    }
    result
        .run
        .as_mut()
        .unwrap()
        .transition_state
        .as_mut()
        .unwrap()
        .irc = None;
    assert_eq!(app.xtb_profile(&result), irc::Profile::Neb);
}

#[test]
fn animate_irc_previews_only_traced_frames_and_stop_restores_the_model() {
    let (mut app, _) = finished();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let result = app.xtb_shown().unwrap().stored.result.clone();
    let path = irc::saved(&result).unwrap();
    assert_ne!(path.frame(0), path.relaxed_frame(0));
    assert_ne!(path.frame(path.steps() - 1), path.relaxed_frame(1));
    assert_eq!(app.xtb_profile(&result), irc::Profile::Irc);
    send(&mut app, XtbAction::Ts(Action::PlayIrc));
    for step in 0..path.steps() {
        let expected = path.frame(step).unwrap().positions_angstrom;
        assert_eq!(
            model(app.xtb_irc_preview().unwrap()),
            crate::app::xtb::tests::f32s(&expected)
        );
        assert_eq!(app.tab.doc, before);
        send(&mut app, XtbAction::Ts(Action::IrcTick));
    }
    assert!(app.xtb_irc_preview().is_none());
    send(&mut app, XtbAction::Ts(Action::PlayIrc));
    send(&mut app, XtbAction::Ts(Action::IrcTick));
    send(&mut app, XtbAction::Ts(Action::PlayIrc));
    assert_eq!(model(app.display_document()), model(&before));
    assert_eq!(app.tab.revision, revision);

    send(&mut app, XtbAction::Ts(Action::PlayIrc));
    app.tab.revision += 1;
    assert!(app.xtb_irc_preview().is_none());
    send(&mut app, XtbAction::Ts(Action::PlayIrc));
    assert!(app.xtb_irc_preview().is_some());

    send(&mut app, XtbAction::Ts(Action::Profile(irc::Profile::Neb)));
    assert_eq!(app.xtb_profile(&result), irc::Profile::Neb);
    send(&mut app, XtbAction::Ts(Action::PlayIrc));
    assert!(app.xtb_irc_preview().is_none());
}

#[test]
fn reopened_irc_exports_ordered_xyz_in_the_selected_unit_only_from_a_full_save() {
    let (mut app, arrow) = finished();
    for full in [true, false] {
        reshiki::science::set_keep_full(&mut app.tab.doc, 0, full);
        let serialized = app.tab.doc.file_json().unwrap();
        let document = Document::from_native_file(&serialized).unwrap();
        let mut reopened = reaction_app(document, arrow);
        send(&mut reopened, XtbAction::Palette(palette::Action::Show));
        for unit in EnergyUnit::ALL {
            reopened.appearance.energy_unit = unit;
            let shown = reopened.xtb_shown().unwrap();
            let result = &shown.stored.result;
            let path = irc::saved(result).unwrap();
            assert_eq!(reopened.xtb_profile(result), irc::Profile::Irc);
            assert_eq!(path.has_frames(), full);
            let export = reopened.xtb_prepare_irc(&shown);
            if !full {
                assert!(export.is_none());
                send(&mut reopened, XtbAction::Ts(Action::PlayIrc));
                assert!(reopened.xtb_irc_preview().is_none());
                continue;
            }
            let export = export.unwrap().unwrap();
            assert_eq!(export.name, "allyl-vinyl-ether-irc.xyz");
            let xyz = std::str::from_utf8(&export.bytes).unwrap();
            let count = shown.frame.numbers.len();
            let lines: Vec<_> = xyz.lines().collect();
            assert_eq!(lines.len(), (count + 2) * 3);
            for (i, lines) in lines.chunks_exact(count + 2).enumerate() {
                assert_eq!(lines[0], count.to_string());
                let s = path.arc_lengths_sqrt_amu_bohr[i];
                let energy = path.trajectory.energies_hartree[i];
                let delta = (energy + 5.) * unit.convert_kcal(KCAL_PER_HARTREE);
                assert_eq!(
                    lines[1],
                    format!(
                        "E = {energy} Eh · s = {s:.6} amu½·bohr · ΔE = {delta:.6} {}",
                        unit.label()
                    )
                );
                let parsed = reshiki::qc_input::parse_xyz(&lines.join("\n")).unwrap();
                let frame = path.frame(i).unwrap();
                assert_eq!(parsed.numbers, frame.numbers);
                assert!(
                    parsed
                        .positions_angstrom
                        .iter()
                        .flatten()
                        .zip(frame.positions_angstrom.iter().flatten())
                        .all(|(a, b)| (a - b).abs() < 1e-6)
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn path_controls_offer_irc_animation_and_export_only_with_saved_frames() {
    let ui = card::tests::ViewHarness::new().await;
    let (mut app, arrow) = finished();
    for full in [true, false] {
        reshiki::science::set_keep_full(&mut app.tab.doc, 0, full);
        let mut reopened = reaction_app(
            Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap(),
            arrow,
        );
        send(&mut reopened, XtbAction::Palette(palette::Action::Show));
        let snapshot = ui.inspect(reopened.xtb_bottom_palette().unwrap());
        let nodes = &snapshot.semantics.nodes;
        assert!(
            nodes
                .iter()
                .any(|n| n.id == "xtb-path-irc" && n.checked == Some(true))
        );
        assert!(
            nodes
                .iter()
                .any(|n| n.id == "xtb-path-neb" && n.checked == Some(false))
        );
        assert_eq!(nodes.iter().any(|n| n.id == "xtb-animate-irc"), full);
        assert_eq!(nodes.iter().any(|n| n.id == "xtb-export-irc"), full);
        let action = ui
            .activate(reopened.xtb_bottom_palette().unwrap(), "xtb-path-neb")
            .unwrap();
        let _ = reopened.update(action);
        assert_eq!(
            reopened.xtb_profile(&reopened.xtb_shown().unwrap().stored.result),
            irc::Profile::Neb
        );
    }
}
