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

#[test]
fn verified_refinement_reopens_on_its_own_irc_and_switches_back_to_the_source() {
    let (mut app, arrow) = irc_profile::finished();
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
    let mut app = reaction_app(
        Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap(),
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
