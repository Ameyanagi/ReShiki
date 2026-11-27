use super::*;

#[test]
fn saved_gfn2_coordinates_seed_gxtb_and_both_results_survive_a_failed_refinement() {
    let (app, arrow) = irc_profile::finished();
    let mut app = reaction_app(
        Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap(),
        arrow,
    );
    let source = app.tab.doc.science.results[0].result.clone();
    send(
        &mut app,
        XtbAction::Ts(Action::RefineGxtb(arrow, source.clone())),
    );
    let run = app.tab.xtb.run.as_ref().unwrap();
    let request = &run.ts.as_ref().unwrap().request;
    assert_eq!(request.method, XtbMethod::GXtb);
    assert_eq!(run.config.method, request.method);
    let launch = run.launch.as_ref().unwrap();
    let open = job::worker_open(&launch.frame, request.method, Default::default());
    assert_eq!(open.method, reshiki_xtb_worker::protocol::Method::Gxtb);
    let Some(job::SearchPath::Refine {
        parent,
        reactant,
        product,
        saddle,
    }) = app.xtb_ts_retry_path(request, launch)
    else {
        panic!("expected a saved saddle")
    };
    assert_eq!(Some(parent), source.transition_state_id());
    let positions = |hash| job::bohr(&app.tab.doc.science.frame(&hash).unwrap().positions_angstrom);
    let endpoints = source.endpoints.unwrap();
    assert_eq!(reactant, positions(endpoints.reactant));
    assert_eq!(product, positions(endpoints.product));
    assert_eq!(saddle, positions(source.frame));
    let serial = run.serial;
    let mut report = Arc::unwrap_or_clone(example_report(launch));
    let reshiki::qcdata::Settings::Xtb(settings) = &mut report.result.settings else {
        unreachable!()
    };
    settings.method = XtbMethod::GXtb;
    report.result.provenance.method = Some(settings.method.label().into());
    report.result.trajectory = None;
    report.extras.image_positions.clear();
    report.extras.saddle_converged = false;
    report.extras.connects = false;
    report.result.status = Status::NotConverged {
        reason: "saddle refinement did not converge".into(),
    };
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(Arc::new(report)))),
    );

    let mut app = reaction_app(
        Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap(),
        arrow,
    );
    let stored = &app.tab.doc.science.results;
    assert_eq!(stored.len(), 2);
    let [parent, refined] = app
        .xtb_ts_linked_pair(&stored[1].result)
        .unwrap()
        .map(Arc::clone);
    assert_eq!(*parent, *source);
    assert!(view::presentation(&parent, None).reliable());
    let failed = view::presentation(&refined, None);
    assert!(failed.refined_kcal.is_none());
    assert!(failed.reason.unwrap().contains("did not converge"));
    assert_eq!(saved_search(&refined).unwrap().method, XtbMethod::GXtb);
    assert_eq!(
        saved_search(&refined).unwrap().refined_from,
        parent.transition_state_id()
    );

    app.xtb_show_result(&refined);
    for result in [parent, refined] {
        send(
            &mut app,
            XtbAction::Ts(Action::Card(card::Action::Result(result.clone()))),
        );
        assert!(Arc::ptr_eq(
            &app.xtb_shown().unwrap().stored.result,
            &result
        ));
        assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    }
}

#[test]
fn direct_gxtb_search_keeps_the_selected_method() {
    let (doc, arrow) = card::tests::claisen();
    let mut app = reaction_app(doc, arrow);
    send(
        &mut app,
        XtbAction::Ts(Action::Card(card::Action::Method(XtbMethod::GXtb))),
    );
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let request = &run.ts.as_ref().unwrap().request;
    assert_eq!(run.config.method, XtbMethod::GXtb);
    assert_eq!(request.method, run.config.method);
    let open = job::worker_open(
        &run.launch.as_ref().unwrap().frame,
        request.method,
        Default::default(),
    );
    assert_eq!(open.method, reshiki_xtb_worker::protocol::Method::Gxtb);
    assert!(matches!(
        app.xtb_ts_retry_path(request, run.launch.as_ref().unwrap()),
        Some(job::SearchPath::New(_))
    ));
    assert!(request.refined_from().is_none());
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn running_gxtb_search_and_refine_keep_saved_gfn2_rows_labeled() {
    let ui = card::tests::ViewHarness::new().await;
    for refine in [false, true] {
        let (doc, arrow) = card::tests::claisen();
        let mut app = reaction_app(doc, arrow);
        send(
            &mut app,
            XtbAction::Ts(Action::Card(card::Action::FreeEnergy)),
        );
        send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
        finish_model(&mut app);
        let run = app.tab.xtb.run.as_ref().unwrap();
        let serial = run.serial;
        let mut report = Arc::unwrap_or_clone(example_report(run.launch.as_ref().unwrap()));
        let thermo = reshiki::qcdata::Thermo {
            temperature_k: 298.15,
            pressure_pa: 101_325.,
            zero_point_energy_hartree: 0.,
            enthalpy_correction_hartree: 0.01,
            entropy_hartree_per_k: 0.,
            gibbs_correction_hartree: 0.01,
            heat_capacity_hartree_per_k: None,
            frequency_scale: 1.,
            rotor_cutoff_cm1: 50.,
            imaginary_cutoff_cm1: 20.,
            symmetry_number: 1,
            point_group: None,
        };
        report.result.thermo = Some(thermo.clone());
        report.extras.endpoint_thermo = Some([Some(thermo.clone()), Some(thermo)]);
        send(
            &mut app,
            XtbAction::TsEvent(serial, Event::Done(Ok(Arc::new(report)))),
        );
        let [stored] = app.tab.doc.science.results.as_slice() else {
            panic!("expected one saved GFN2-xTB result: {}", app.status);
        };
        let source = Arc::clone(&stored.result);
        if refine {
            send(&mut app, XtbAction::Ts(Action::RefineGxtb(arrow, source)));
        } else {
            send(
                &mut app,
                XtbAction::Ts(Action::Card(card::Action::Method(XtbMethod::GXtb))),
            );
            send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
        }
        let live = app.xtb_live_ts().unwrap();
        assert_eq!(live.request.method, XtbMethod::GXtb);
        let texts = ui.inspect(app.xtb_reaction_card(arrow)).texts;
        for prefix in [
            "GFN2-xTB Barrier ΔE‡",
            "GFN2-xTB Reaction ΔE",
            "GFN2-xTB ΔG‡ ",
            "GFN2-xTB ΔG ",
            "GFN2-xTB ✓ one imaginary frequency",
        ] {
            assert!(
                texts.iter().any(|text| text.starts_with(prefix)),
                "{prefix}: {texts:?}"
            );
        }
        assert!(
            texts
                .iter()
                .any(|text| text == &format!("g-xTB · {}", live.line()))
        );
        assert!(
            texts
                .iter()
                .filter(|text| text.contains("gas phase") || text.contains("Gas phase"))
                .all(|text| !text.contains("xTB"))
        );
    }
}

#[test]
fn show_where_previews_the_failed_gxtb_endpoint_with_a_verified_parent() {
    let (mut app, arrow) = irc_profile::finished();
    send(
        &mut app,
        XtbAction::Ts(Action::PlaceEnergyProfile(Some(arrow))),
    );
    let original_profile = app.tab.doc.graphics.last().unwrap().clone();
    select(&mut app, arrow);
    let parent = app.tab.doc.science.results[0].result.clone();
    send(
        &mut app,
        XtbAction::Ts(Action::RefineGxtb(arrow, parent.clone())),
    );
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = Arc::unwrap_or_clone(example_report(run.launch.as_ref().unwrap()));
    let reshiki::qcdata::Settings::Xtb(settings) = &mut report.result.settings else {
        unreachable!()
    };
    settings.method = XtbMethod::GXtb;
    report.result.provenance.method = Some(settings.method.label().into());
    report.result.trajectory = None;
    report.extras.image_positions.clear();
    report.extras.connects = false;
    report.result.status = Status::NotConverged {
        reason: "IRC reaches the reactant and a different minimum".into(),
    };
    let mut reached = report.frames[2].positions_angstrom.clone();
    reached[0][2] += 0.25;
    report.extras.reached_positions_angstrom = reached.clone();
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(Arc::new(report)))),
    );

    let child = app.tab.doc.science.results[1].result.clone();
    assert_eq!(app.tab.doc.graphics.last(), Some(&original_profile));
    assert_eq!(
        view::presentation(&child, None).recovery,
        view::Recovery::ShowWhere
    );
    assert!(app.xtb_show_reaction_path(arrow));
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &parent
    ));
    send(&mut app, XtbAction::Ts(Action::ShowWhere(arrow)));
    assert!(Arc::ptr_eq(&app.xtb_shown().unwrap().stored.result, &child));
    assert!(app.xtb_reached_endpoint_selected());
    assert_eq!(
        model(app.display_document())[0],
        reached[0].map(|x| x as f32)
    );
    assert_ne!(model(&app.tab.doc)[0], reached[0].map(|x| x as f32));
}

#[test]
fn verified_refinement_reopens_on_its_own_irc_and_switches_back_to_the_source() {
    let (mut app, arrow) = irc_profile::finished();
    send(
        &mut app,
        XtbAction::Ts(Action::PlaceEnergyProfile(Some(arrow))),
    );
    select(&mut app, arrow);
    let original_profile = app.tab.doc.graphics.last().unwrap().clone();
    let source = app.tab.doc.science.results[0].result.clone();
    send(
        &mut app,
        XtbAction::Ts(Action::RefineGxtb(arrow, source.clone())),
    );
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = irc_profile::connected_report(run.launch.as_ref().unwrap());
    let reshiki::qcdata::Settings::Xtb(settings) = &mut report.result.settings else {
        unreachable!()
    };
    settings.method = XtbMethod::GXtb;
    report.result.provenance.method = Some(settings.method.label().into());
    report.result.energy_hartree = Some(-6.9);
    report.result.trajectory = None;
    report.extras.endpoint_energies_hartree = [-7., -7.01];
    report.extras.image_positions.clear();
    let irc = report.extras.irc.as_mut().unwrap();
    irc.trajectory.energies_hartree = vec![-7.0, -6.9, -7.01];
    irc.neb_arc_lengths_sqrt_amu_bohr.clear();
    for (end, energy) in irc
        .relaxed_ends
        .as_mut()
        .unwrap()
        .iter_mut()
        .zip([-7., -7.01])
    {
        end.energy_hartree = energy;
    }
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(Arc::new(report)))),
    );
    let saved = app.tab.doc.clone();
    let updated = saved.graphics.last().unwrap();
    assert_eq!(
        (updated.origin, updated.axis_x, updated.axis_y),
        (
            original_profile.origin,
            original_profile.axis_x,
            original_profile.axis_y
        ),
    );
    assert_eq!(updated.energy_profile.as_ref().unwrap().method, "g-xTB");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.graphics.last(), Some(&original_profile));
    let mut app = reaction_app(
        Document::from_native_file(&saved.file_json().unwrap()).unwrap(),
        arrow,
    );
    assert_eq!(app.tab.doc.science.results.len(), 2);
    let target = app.tab.doc.science.results[1].result.clone();
    let [parent, refined] = app.xtb_ts_linked_pair(&target).unwrap().map(Arc::clone);
    assert_eq!(parent.transition_state_id(), source.transition_state_id());
    assert_ne!(parent.transition_state_id(), refined.transition_state_id());
    app.xtb_show_result(&refined);
    for (result, method, delta) in [(parent, "GFN2-xTB", 26.5), (refined, "g-xTB", 62.8)] {
        send(
            &mut app,
            XtbAction::Ts(Action::Card(card::Action::Result(result.clone()))),
        );
        let shown = app.xtb_shown().unwrap();
        assert!(Arc::ptr_eq(&shown.stored.result, &result));
        assert!((view::presentation(&result, None).refined_kcal.unwrap() - delta).abs() < 0.1);
        assert_eq!(app.xtb_profile(&result), irc::Profile::Irc);
        assert!(app.status.contains(method), "{}", app.status);
    }
}
