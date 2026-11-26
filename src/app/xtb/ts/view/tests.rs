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
    use reshiki::qcdata::{Task, Trajectory, Vibrations};

    fn result() -> CalcResult {
        let frame = Frame {
            numbers: vec![3, 17],
            positions_angstrom: vec![[0., 0., 0.], [0., 0., 2.]],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        };
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
    fn nonconverged_or_wrong_mode_count_has_no_barrier_or_ts_marker() {
        for frequencies in [vec![], vec![-49.9], vec![-50.], vec![-120., -60.]] {
            let mut result = result();
            result.vibrations.as_mut().unwrap().frequencies_cm1 = frequencies;
            let presentation = presentation(&result, None);
            assert!(!presentation.reliable());
            assert_eq!(presentation.refined_kcal, None);
            assert!(!presentation.status_line().contains("kcal/mol"));
            let chart = PathChart {
                deltas: vec![0., 30.2, -8.2],
                refined_delta: presentation.refined_kcal,
                selected: None,
                climbing: 1,
                interactive: true,
                stale: false,
            };
            assert_eq!(chart.refined_ts_marker(), None);
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
    fn wrong_endpoint_results_are_failures_while_irc_errors_keep_a_warning() {
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
        let mut result = result();
        result.status = Status::NotConverged {
            reason: "IRC failed: step limit reached".into(),
        };
        let presentation = presentation(&result, None);
        assert!(presentation.reliable());
        assert!(
            presentation
                .warning
                .unwrap()
                .contains("could not be checked")
        );
        assert!(
            presentation
                .verification
                .contains("reaction path not verified")
        );
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
