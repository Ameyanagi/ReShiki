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
    assert_eq!(
        selection_label(26.5, EnergyUnit::KcalPerMol),
        "ΔE 26.5 kcal/mol"
    );
    assert_eq!(
        selection_label(-8.2, EnergyUnit::KcalPerMol),
        "ΔE −8.2 kcal/mol"
    );
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
        unit: EnergyUnit::KcalPerMol,
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
        unit: EnergyUnit::KcalPerMol,
    };
    assert_eq!(chart.refined_ts_marker().unwrap().1, "TS‡ 26.5");
    assert!(selection_label(chart.deltas[1], chart.unit).ends_with("ΔE 30.2 kcal/mol"));
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
    assert_eq!(
        bond_geometry_rows(&reactant, &ts, &product, &[]),
        vec![
            ["Forming C2···O3", "2.50", "2.10", "1.40"].map(str::to_owned),
            ["Breaking C1···C2", "1.50", "2.00", "3.00"].map(str::to_owned),
        ]
    );
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

    #[test]
    fn path_output_uses_the_app_energy_unit_setting() {
        let mut app = path_app(result());
        let _ = app.update(send(Action::Image(1)));
        for (unit, title, saddle, selected, product) in [
            (EnergyUnit::KjPerMol, "kJ/mol", "110.9", "126.4", "−34.3"),
            (EnergyUnit::Ev, "eV", "1.15", "1.31", "−0.36"),
        ] {
            app.appearance.energy_unit = unit;
            let shown = app.xtb_shown().expect("saved Path result");
            let saved = presentation(&shown.stored.result, shown.ts);
            let chart = app.xtb_ts_path_chart(&shown, saved.refined_kcal);
            assert_eq!(chart.axis_title(), format!("ΔE from reactants ({title})"));
            assert_eq!(
                chart.refined_ts_marker().unwrap().1,
                format!("TS‡ {saddle}")
            );
            assert_eq!(chart.endpoint_marker(2).unwrap(), format!("P {product}"));
            assert!(
                chart
                    .accessible_name()
                    .contains(&format!("Selected ΔE {selected} {title}"))
            );
            assert_eq!(
                selection_label(chart.deltas[1], chart.unit),
                format!("ΔE {selected} {title}")
            );
            let rows = shown.rows(Some(app.appearance.energy_unit));
            let tsv = crate::app::xtb::results::tsv(&shown.method(), &rows);
            assert!(tsv.contains(&format!("TS‡ ΔE\t{saddle} {title}")));
            assert!(tsv.contains(&format!("P ΔE\t{product} {title}")));
            assert!(tsv.contains("Total energy\t−4.957770 Eh"));
            assert!(!tsv.contains("kcal/mol"));
            assert!(
                saved
                    .status_line(unit)
                    .ends_with(&format!("ΔE‡ {saddle} {title}"))
            );
        }
    }

    #[test]
    fn old_saved_path_keeps_optimized_reaction_energy_even_if_saddle_verification_failed() {
        let mut result = result();
        result.status = Status::NotConverged {
            reason: "saddle refinement did not converge".into(),
        };
        assert!((reaction_energy_kcal(&result).unwrap() + 8.2).abs() < 1e-10);
        assert!(
            path_rows(&result, None, EnergyUnit::KjPerMol, None)
                .iter()
                .all(|(label, _)| !label.contains("TS‡"))
        );
        result
            .trajectory
            .as_mut()
            .unwrap()
            .energies_hartree
            .truncate(1);
        assert_eq!(reaction_energy_kcal(&result), None);
    }

    fn result_with_gibbs() -> CalcResult {
        use reshiki::qcdata::{RunSummary, Thermo, TransitionStateMapping, TransitionStateRun};

        let mut result = result();
        let correction = |gibbs_correction_hartree| Thermo {
            temperature_k: 298.15,
            pressure_pa: 101_325.,
            zero_point_energy_hartree: 0.,
            enthalpy_correction_hartree: gibbs_correction_hartree,
            entropy_hartree_per_k: 0.,
            gibbs_correction_hartree,
            heat_capacity_hartree_per_k: None,
            frequency_scale: 1.,
            rotor_cutoff_cm1: 50.,
            imaginary_cutoff_cm1: 20.,
            symmetry_number: 1,
            point_group: None,
        };
        result.thermo = Some(correction(0.03));
        let frame = frame();
        result.endpoints = Some(Endpoints {
            reactant: frame.hash(),
            product: frame.hash(),
        });
        let search = TransitionStateRun {
            arrow: None,
            context: None,
            free_energy: true,
            endpoint_energies_hartree: Some([-5., -5. + -8.2 / KCAL_PER_HARTREE]),
            endpoint_thermo: Some([Some(correction(0.01)), Some(correction(0.02))]),
            saddle_converged: true,
            imaginary: 1,
            imaginary_cm1: Some(-387.),
            climbing_image: 1,
            mapping: TransitionStateMapping {
                formed: vec![],
                broken: vec![],
                hydrogen_moves: vec![],
                automatic: true,
            },
            diagnostics: vec![],
        };
        result.run = Some(RunSummary {
            seconds: 1.,
            scf_iterations: vec![],
            finished: None,
            search: None,
            transition_state: Some(Box::new(search)),
        });
        result.validate(&frame).unwrap();
        result
    }

    #[test]
    fn saved_gibbs_values_need_matching_conditions_and_a_verified_barrier() {
        let mut result = result_with_gibbs();
        assert!(
            (barrier_gibbs_kcal(&result).unwrap() - (26.5 + 0.02 * KCAL_PER_HARTREE)).abs() < 1e-10
        );
        assert!(
            (reaction_gibbs_kcal(&result).unwrap() - (-8.2 + 0.01 * KCAL_PER_HARTREE)).abs()
                < 1e-10
        );
        let rows = path_rows(&result, None, EnergyUnit::KjPerMol, None);
        assert!(rows.contains(&("TS‡ ΔG (298.15 K)".into(), "163.4 kJ/mol".into())));
        assert!(rows.contains(&("P ΔG (298.15 K)".into(), "−8.1 kJ/mol".into())));

        result.thermo.as_mut().unwrap().temperature_k = 300.;
        assert_eq!(barrier_gibbs_kcal(&result), None);
        assert!(reaction_gibbs_kcal(&result).is_some());
        result.thermo.as_mut().unwrap().temperature_k = 298.15;
        result.status = Status::NotConverged {
            reason: "saddle refinement did not converge".into(),
        };
        assert_eq!(barrier_gibbs_kcal(&result), None);
        assert!(reaction_gibbs_kcal(&result).is_some());
    }

    #[test]
    fn second_small_imaginary_mode_suppresses_only_the_gibbs_barrier() {
        let mut result = result_with_gibbs();
        result.vibrations.as_mut().unwrap().frequencies_cm1 = vec![-450., -35.];
        assert!(presentation(&result, None).reliable());
        assert_eq!(barrier_gibbs_kcal(&result), None);
        assert_eq!(
            barrier_gibbs_unavailable(&result).as_deref(),
            Some("ΔG‡ not reported: a second small imaginary mode (35i cm⁻¹)")
        );
        assert!(reaction_gibbs_kcal(&result).is_some());
    }

    #[test]
    fn forward_gibbs_barrier_survives_a_saved_product_frequency_failure() {
        let mut result = result_with_gibbs();
        let expected = barrier_gibbs_kcal(&result).unwrap();
        let search = result
            .run
            .as_mut()
            .unwrap()
            .transition_state
            .as_mut()
            .unwrap();
        search.endpoint_thermo.as_mut().unwrap()[1] = None;
        search.diagnostics.push((
            "Product thermochemistry".into(),
            "Free energy unavailable: endpoint worker timed out".into(),
        ));

        let light = result.light(&[]);
        let decoded: CalcResult =
            serde_json::from_str(&serde_json::to_string(&light).unwrap()).unwrap();
        decoded.validate(&frame()).unwrap();
        assert_eq!(barrier_gibbs_kcal(&decoded), Some(expected));
        assert_eq!(reaction_gibbs_kcal(&decoded), None);
        assert_eq!(
            reaction_gibbs_unavailable(&decoded).as_deref(),
            Some(
                "Reaction ΔG not reported: product frequency analysis failed (endpoint worker timed out)"
            )
        );
    }

    #[test]
    fn path_copy_labels_the_optimized_complex_reference() {
        let mut result = result_with_gibbs();
        result
            .run
            .as_mut()
            .unwrap()
            .transition_state
            .as_mut()
            .unwrap()
            .context = Some(reshiki::qcdata::TransitionStateContext {
            reactant_chemistry: "ab".repeat(32),
            product_chemistry: "cd".repeat(32),
            title: "A + B → C".into(),
            reactant: "A".into(),
            endpoint_complex: Some(true),
        });
        let app = path_app(result);
        let rows = app.xtb_shown().unwrap().rows(Some(EnergyUnit::default()));
        assert!(rows.contains(&("P ΔE (optimized complex)".into(), "−8.2 kcal/mol".into())));
    }

    #[test]
    fn folded_start_log_uses_the_selected_energy_unit() {
        let mut result = result_with_gibbs();
        result
            .run
            .as_mut()
            .unwrap()
            .transition_state
            .as_mut()
            .unwrap()
            .diagnostics
            .push((
                "Folded the starting conformer".into(),
                "C1···C6 4.72 → 3.05 Å (-1.8 kcal/mol)".into(),
            ));
        for (unit, energy) in [
            (EnergyUnit::KcalPerMol, "−1.8 kcal/mol"),
            (EnergyUnit::KjPerMol, "−7.5 kJ/mol"),
            (EnergyUnit::Ev, "−0.08 eV"),
        ] {
            assert!(search_log(&result, None, unit).contains(&format!(
                "Folded the starting conformer: C1···C6 4.72 → 3.05 Å ({energy})"
            )));
        }
    }

    #[tokio::test]
    #[ignore = "Opt-in renderer check: needs a graphics adapter"]
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
    #[ignore = "Opt-in renderer check: needs a graphics adapter"]
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
        let status = presentation.status_line(EnergyUnit::KcalPerMol);
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
            assert!(
                !presentation
                    .status_line(EnergyUnit::KcalPerMol)
                    .contains("kcal/mol")
            );
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
            assert!(
                !presentation
                    .status_line(EnergyUnit::KcalPerMol)
                    .contains("26.5"),
                "{reason}"
            );
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
                presentation.status_line(EnergyUnit::KcalPerMol),
                "No reliable transition state found."
            );
        }
    }

    #[test]
    fn saved_search_log_has_a_technical_block_without_inventing_missing_counters() {
        let result = result();
        let log = search_log(&result, None, EnergyUnit::default());
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
