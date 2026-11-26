use super::*;

#[test]
fn reopened_and_stale_profiles_explain_why_the_model_does_not_preview() {
    assert_eq!(profile_note(false, false), Some(RELOAD_NOTE));
    assert_eq!(profile_note(false, true), Some(RELOAD_NOTE));
    assert!(profile_note(true, true).unwrap().contains("saved energy"));
    assert_eq!(profile_note(true, false), None);
}

#[test]
fn profile_selection_keeps_search_details_out_of_the_readout() {
    let energies = [0., 1., 3., 2., -1.];
    assert_eq!(climbing_image(&energies, None), 2);
    assert_eq!(climbing_image(&energies, Some(1)), 1);
    assert_eq!(climbing_image(&energies, Some(99)), 2);
    assert_eq!(climbing_image(&energies, Some(usize::MAX)), 2);
    assert_eq!(climbing_image(&[0., 3., 3., 0.], None), 1);
    assert_eq!(climbing_image(&[], None), 0);
    assert_eq!(selection_label(26.5), "ΔE 26.5 kcal/mol");
    assert_eq!(selection_label(-8.2), "ΔE −8.2 kcal/mol");
}

#[test]
fn chart_clicks_follow_the_nearest_image_even_for_reopened_profiles() {
    let chart = PathChart {
        deltas: vec![0., 2., 5., 1., -1.],
        refined_delta: Some(4.),
        selected: Some(2),
        climbing: 2,
        interactive: true,
        stale: false,
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(420., 190.));
    assert_eq!(chart.nearest(chart.x(0, bounds), bounds), Some(0));
    assert_eq!(chart.nearest(chart.x(3, bounds) + 1., bounds), Some(3));
    assert_eq!(chart.nearest(bounds.width, bounds), Some(4));
}

#[test]
fn refined_ts_label_uses_saddle_energy_while_image_keeps_neb_energy() {
    let reactant = -5.;
    let energy = |delta| reactant + delta / KCAL_PER_HARTREE;
    let (deltas, refined_delta) =
        relative_energies(&[reactant, energy(30.2), energy(-8.2)], Some(energy(26.5)));
    let chart = PathChart {
        deltas,
        refined_delta,
        selected: Some(1),
        climbing: 1,
        interactive: true,
        stale: false,
    };
    assert_eq!(chart.refined_ts_marker().unwrap().1, "TS‡ 26.5");
    assert!(selection_label(chart.deltas[1]).ends_with("ΔE 30.2 kcal/mol"));
    assert_eq!(chart.energy_bounds(), (chart.deltas[2], chart.deltas[1]));

    let higher_saddle = PathChart {
        refined_delta: Some(34.1),
        ..chart
    };
    assert_eq!(
        higher_saddle.energy_bounds(),
        (higher_saddle.deltas[2], 34.1)
    );
    let lower_saddle = PathChart {
        refined_delta: Some(-12.3),
        ..higher_saddle
    };
    assert_eq!(
        lower_saddle.energy_bounds(),
        (-12.3, lower_saddle.deltas[1])
    );
    let no_saddle = PathChart {
        refined_delta: None,
        ..lower_saddle
    };
    assert_eq!(no_saddle.refined_ts_marker(), None);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn changed_bonds_use_saved_endpoints_and_lengths_use_ts_coordinates() {
    let frame = |positions_angstrom| Frame {
        numbers: vec![6, 6, 8],
        positions_angstrom,
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let reactant = frame(vec![[0., 0., 0.], [1.5, 0., 0.], [4., 0., 0.]]);
    let product = frame(vec![[0., 0., 0.], [3., 0., 0.], [4.4, 0., 0.]]);
    assert_eq!(
        changed_bonds(&reactant, &product),
        vec![("Forming", [1, 2]), ("Breaking", [0, 1])]
    );
    let ts = frame(vec![[0., 0., 0.], [2., 0., 0.], [4.1, 0., 0.]]);
    assert_eq!(bond_label(&ts, [1, 2], &[]), "C2···O3");
    assert_eq!(
        bond_label(&ts, [1, 2], &[None, Some(12), Some(6)]),
        "C12···O6"
    );
    assert!((bond_length(&ts, [1, 2]).unwrap() - 2.1).abs() < 1e-12);
    assert_eq!(bond_length(&ts, [1, 99]), None);
}

#[cfg(not(target_arch = "wasm32"))]
mod presentation_tests {
    use super::*;
    use crate::app::xtb::run;
    use crate::app::xtb::ts::card::tests::{ViewHarness, ViewSnapshot};
    use reshiki::document::Document;
    use reshiki::qcdata::{Endpoints, Task, Trajectory, Vibrations};
    use reshiki::science::{self, Launch};
    use std::sync::Arc;

    fn frame() -> Frame {
        Frame {
            numbers: vec![3, 17, 2],
            positions_angstrom: vec![[0., 0., 0.], [0., 0., 2.], [4., 0., 0.]],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }
    }

    fn result() -> CalcResult {
        let frame = frame();
        let energy = |delta| -5. + delta / KCAL_PER_HARTREE;
        let mut result = run::example_computed(energy(26.5), 1.)
            .to_calc_result(&frame, Task::Energy, &run::example_engine(), None)
            .unwrap();
        result.task = Task::TransitionState;
        result.status = Status::Converged;
        result.vibrations = Some(Vibrations {
            frequencies_cm1: vec![-387.],
            ir_intensities_km_mol: vec![],
            modes: vec![],
            reduced_masses_amu: vec![],
            masses_amu: vec![],
        });
        result.trajectory = Some(Trajectory {
            rmsd_to_best_angstrom: Vec::new(),
            numbers: frame.numbers,
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            positions_angstrom: vec![],
            energies_hartree: vec![energy(0.), energy(30.2), energy(-8.2)],
            labels: vec![],
            minimizer_energies_hartree: vec![],
            cluster_sizes: vec![],
        });
        result
    }

    fn path_app(mut result: CalcResult) -> App {
        let mut doc = Document::default();
        let atoms: Vec<_> = ["Li", "Cl", "He"]
            .into_iter()
            .enumerate()
            .map(|(index, element)| {
                doc.add_atom(
                    element,
                    reshiki::document::Point::new(index as f32 * 28., 0.),
                )
            })
            .collect();
        let frame = Arc::new(frame());
        let launch = Launch {
            frame: Arc::clone(&frame),
            atoms: atoms.iter().copied().map(Some).collect(),
            baseline: science::baseline(&doc, &atoms),
        };
        result.endpoints = Some(Endpoints {
            reactant: frame.hash(),
            product: frame.hash(),
        });
        science::attach(&mut doc, &launch, vec![], result).expect("valid saved TS fixture");
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc = doc;
        let result = Arc::clone(&app.tab.doc.science.results[0].result);
        app.tab.xtb.palette.show(&result);
        app
    }

    fn profile(snapshot: &ViewSnapshot) -> &reshiki::accessibility::Node {
        snapshot
            .semantics
            .nodes
            .iter()
            .find(|node| node.id == "xtb-ts-path-profile")
            .expect("the production Path view must expose its chart")
    }

    #[tokio::test]
    async fn production_path_view_uses_the_refined_marker_and_activates_saved_images() {
        let ui = ViewHarness::new().await;
        let mut app = path_app(result());
        let snapshot = ui.inspect(app.xtb_bottom_palette().expect("open Path palette"));
        assert!(snapshot.semantics.duplicate_ids.is_empty());
        assert!(profile(&snapshot).name.contains("TS‡ 26.5 kcal/mol"));
        assert!(!profile(&snapshot).name.contains("TS‡ 30.2"));
        assert!(
            snapshot
                .semantics
                .nodes
                .iter()
                .any(|node| { node.id == "xtb-results-path" && node.checked == Some(true) })
        );
        let image = snapshot
            .semantics
            .nodes
            .iter()
            .find(|node| node.id == "xtb-ts-image-1")
            .expect("the saved image's keyboard control");
        assert_eq!(image.name, "ΔE 30.2 kcal/mol");
        let action = ui
            .activate(
                app.xtb_bottom_palette().expect("open Path palette"),
                "xtb-ts-image-1",
            )
            .expect("the actual image button must dispatch");
        assert!(matches!(
            action,
            Message::Xtb(XtbAction::Ts(Action::Image(1)))
        ));
        let _ = app.update(action);
        let snapshot = ui.inspect(app.xtb_bottom_palette().expect("selected Path image"));
        assert!(snapshot.texts.iter().any(|text| text == "ΔE 30.2 kcal/mol"));
        assert!(
            profile(&snapshot)
                .name
                .contains("Selected ΔE 30.2 kcal/mol")
        );
        assert!(
            snapshot
                .semantics
                .nodes
                .iter()
                .any(|node| { node.id == "xtb-ts-image-1" && node.checked == Some(true) })
        );

        app.xtb.dock.inspector = true;
        let snapshot = ui.inspect(app.xtb_inspector_palette().expect("inspector Path palette"));
        assert!(profile(&snapshot).name.contains("TS‡ 26.5 kcal/mol"));
        assert!(
            snapshot
                .semantics
                .nodes
                .iter()
                .any(|node| node.id == "xtb-ts-back")
        );
    }

    #[tokio::test]
    async fn production_path_view_omits_the_marker_for_failed_verification() {
        let ui = ViewHarness::new().await;
        let mut failures = Vec::new();
        for frequencies in [vec![], vec![-49.9], vec![-50.], vec![-120., -60.]] {
            let mut result = result();
            result.vibrations.as_mut().unwrap().frequencies_cm1 = frequencies;
            failures.push(result);
        }
        for reason in [
            "saddle refinement did not converge",
            "IRC reaches the reactant on both sides",
            "IRC failed: endpoint minimization reached its step limit",
            "worker check failed: timed out",
        ] {
            let mut result = result();
            result.status = Status::NotConverged {
                reason: reason.into(),
            };
            failures.push(result);
        }
        for result in failures {
            let app = path_app(result);
            let snapshot = ui.inspect(app.xtb_bottom_palette().expect("failed Path palette"));
            assert!(!profile(&snapshot).name.contains("TS‡"));
            assert!(
                snapshot
                    .texts
                    .iter()
                    .any(|text| text == "No reliable transition state found.")
            );
            assert!(snapshot.texts.iter().all(|text| !text.contains("ΔE‡")));
            assert!(
                snapshot
                    .semantics
                    .nodes
                    .iter()
                    .any(|node| node.id == "xtb-ts-rerun-20")
            );
        }
    }

    #[test]
    fn saved_result_uses_one_refined_barrier_and_the_same_status_value() {
        let presentation = presentation(&result(), None);
        assert!(presentation.reliable());
        assert!((presentation.refined_kcal.unwrap() - 26.5).abs() < 1e-10);
        assert!((presentation.highest_path_kcal.unwrap() - 30.2).abs() < 1e-10);
        let status = presentation.status_line();
        assert_eq!(status.matches("26.5").count(), 1);
        assert!(!status.contains("30.2"));
        assert!(presentation.verification.contains("387i cm⁻¹"));
        assert!(
            presentation
                .verification
                .contains("connects reactant and product")
        );
    }

    #[test]
    fn nonconverged_or_wrong_mode_count_has_no_barrier() {
        for frequencies in [vec![], vec![-49.9], vec![-50.], vec![-120., -60.]] {
            let mut result = result();
            result.vibrations.as_mut().unwrap().frequencies_cm1 = frequencies;
            let presentation = presentation(&result, None);
            assert!(!presentation.reliable());
            assert_eq!(presentation.refined_kcal, None);
            assert!(!presentation.status_line().contains("kcal/mol"));
        }
        let mut result = result();
        result.vibrations.as_mut().unwrap().frequencies_cm1 = vec![-120., -60.];
        assert!(
            presentation(&result, None)
                .reason
                .unwrap()
                .contains("two imaginary frequencies")
        );
        result.vibrations.as_mut().unwrap().frequencies_cm1 = vec![-387.];
        result.status = Status::NotConverged {
            reason: "saddle refinement did not converge".into(),
        };
        assert_eq!(presentation(&result, None).refined_kcal, None);
    }

    #[test]
    fn wrong_endpoints_and_irc_errors_hide_the_barrier() {
        for reason in [
            "IRC reaches the reactant on both sides",
            "IRC reaches the product on both sides",
            "IRC reaches the product and a different minimum",
            "IRC reaches neither the reactant nor the product",
            "IRC did not connect the reactant and product",
            "worker check failed: timed out",
            "engine/worker imaginary frequency counts disagree: engine 1, worker 0",
        ] {
            let mut result = result();
            result.status = Status::NotConverged {
                reason: reason.into(),
            };
            let presentation = presentation(&result, None);
            assert!(!presentation.reliable(), "{reason}");
            assert!(!presentation.status_line().contains("26.5"), "{reason}");
        }
        for (reason, explanation) in [
            (
                "IRC failed: endpoint minimization reached its step limit",
                "the endpoint optimization reached its step limit",
            ),
            (
                "IRC failed: SCF failure: iteration limit",
                "the electronic calculation did not converge",
            ),
        ] {
            let mut result = result();
            result.status = Status::NotConverged {
                reason: reason.into(),
            };
            let presentation = presentation(&result, None);
            assert_eq!(presentation.refined_kcal, None);
            assert_eq!(presentation.warning, None);
            assert_eq!(
                presentation.reason,
                Some(format!(
                    "The reaction-path check could not finish: {explanation}."
                ))
            );
            assert_eq!(
                presentation.status_line(),
                "No reliable transition state found."
            );
        }
    }

    #[test]
    fn saved_search_log_has_a_technical_block_without_inventing_missing_counters() {
        let result = result();
        let log = search_log(&result, None);
        assert!(log.starts_with("Transition-state search\n"));
        for label in [
            "Method:",
            "Images:",
            "NEB iterations / convergence:",
            "Saddle steps / reason:",
            "Worker check:",
            "IRC branches:",
            "Evaluations:",
            "Hessians:",
        ] {
            assert!(log.contains(label), "missing {label}");
        }
        assert!(log.contains("not saved with this result"));
        assert!(log.contains("IDPP → CI-NEB → P-RFO → IRC"));
        assert!(
            !crate::app::xtb::results::method_line(
                &result,
                &Frame {
                    numbers: vec![3, 17],
                    positions_angstrom: vec![[0.; 3]; 2],
                    isotopes: vec![],
                    masses_amu: vec![],
                    charge: 0,
                    multiplicity: 1,
                    drawing_atoms: vec![],
                }
            )
            .contains("CI-NEB")
        );
    }
}
