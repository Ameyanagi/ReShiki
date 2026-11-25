use super::*;

#[test]
fn electron_check_names_the_count_or_the_parity_problem() {
    // Water: 10 protons.
    assert_eq!(
        electrons(10, 0, 0).as_deref(),
        Ok("10 electrons · multiplicity 1")
    );
    assert_eq!(
        electrons(10, 1, 1).as_deref(),
        Ok("9 electrons · multiplicity 2")
    );
    assert_eq!(
        electrons(10, 0, 1).unwrap_err(),
        "Even electron count needs an even number of unpaired electrons"
    );
    assert_eq!(
        electrons(10, 1, 0).unwrap_err(),
        "Odd electron count needs an odd number of unpaired electrons"
    );
    assert_eq!(
        electrons(10, 0, 12).unwrap_err(),
        "Unpaired electrons must be between 0 and 10"
    );
    assert!(electrons(10, 11, 0).is_err());
    assert_eq!(
        electrons(1, 0, 1).as_deref(),
        Ok("1 electron · multiplicity 2")
    );
}

#[test]
fn typed_values_win_over_the_drawing_until_the_target_changes() {
    let mut settings = Settings::default();
    assert_eq!(settings.state((-1, 0)), Ok((-1, 0)));
    settings.retarget(Some([1, 0, 7]));
    settings.charge = "−2".into();
    settings.unpaired = " 1 ".into();
    assert_eq!(settings.state((0, 0)), Ok((-2, 1)));
    settings.unpaired = "-1".into();
    assert!(settings.state((0, 0)).is_err());
    settings.charge = "1.5".into();
    assert_eq!(settings.state((0, 0)), Err("Charge must be a whole number"));
    // The same molecule keeps them; another starts from its drawing.
    settings.retarget(Some([1, 0, 7]));
    assert_eq!(settings.charge, "1.5");
    // The same atom ID in another tab is another molecule.
    settings.retarget(Some([2, 0, 7]));
    assert_eq!(settings.state((3, 2)), Ok((3, 2)));
}

#[test]
fn hill_formula_puts_carbon_and_hydrogen_first() {
    let caffeine = [
        6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ];
    assert_eq!(formula(&caffeine), "C8H10N4O2");
    assert_eq!(formula(&[8, 1, 1]), "H2O");
    assert_eq!(formula(&[17, 6, 1, 9, 35]), "CHBrClF");
}

#[test]
fn scientific_notation_uses_superscripts_and_a_real_minus() {
    assert_eq!(scientific(-2.1e-5), "−2.1 × 10⁻⁵");
    assert_eq!(scientific(3.0e-7), "3.0 × 10⁻⁷");
    // Rounding up to the next decade moves the exponent.
    assert_eq!(scientific(9.96e-3), "1.0 × 10⁻²");
    assert_eq!(scientific(12.0), "1.2 × 10¹");
    assert_eq!(scientific(0.0), "0");
    assert_eq!(signed(-5.0701764, 6), "−5.070176");
}

#[test]
fn progress_line_follows_the_steps_and_replaces_repeats() {
    let mut progress = Progress::default();
    assert_eq!(progress.line(Task::Optimize, 0.42), "Starting xTB · 0.4 s");
    progress.push(0, &[-5.0]);
    assert_eq!(
        progress.line(Task::Optimize, 0.5),
        "Optimizing · step 0 · 0.5 s"
    );
    progress.push(1, &[-5.01, -5.010_021]);
    assert_eq!(progress.step(), 2);
    assert_eq!(
        progress.line(Task::Optimize, 3.21),
        "Optimizing · step 2 · ΔE −2.1 × 10⁻⁵ Eh · 3.2 s"
    );
    // A batch that starts at an earlier step replaces what followed it.
    progress.push(2, &[-5.0101]);
    assert_eq!(progress.energies.len(), 3);
    let deltas: Vec<f64> = progress.deltas().collect();
    assert_eq!(deltas.len(), 2);
    assert!((deltas[1] + 1e-4).abs() < 1e-12);
    assert_eq!(progress.line(Task::Energy, 1.0), "Computing energy · 1.0 s");
}

#[test]
fn summary_reads_energy_gap_and_convergence() {
    assert_eq!(
        summary(-5.070176, Some(14.01), None),
        "E −5.070176 Eh · gap 14.01 eV"
    );
    assert_eq!(
        summary(-5.0701, None, Some((true, 1))),
        "E −5.070100 Eh · converged in 1 step"
    );
    assert_eq!(
        summary(-5.0, Some(1.0), Some((false, 500))),
        "E −5.000000 Eh · gap 1.00 eV · not converged after 500 steps"
    );
}

#[test]
fn stereo_changes_read_as_the_card_says_them() {
    let name = |id: u64| match id {
        1 => ("C3".to_owned(), Some("R".to_owned())),
        2 => ("C4".to_owned(), None),
        _ => ("C5".to_owned(), None),
    };
    let centre = |atom, inverted| StereoChange::Centre { atom, inverted };
    assert_eq!(
        describe(&centre(1, true), name),
        "Optimization changed the configuration at C3 (R → S)."
    );
    assert_eq!(
        describe(&centre(1, false), name),
        "Optimization changed the configuration at C3 (R → flat)."
    );
    assert_eq!(
        describe(&centre(2, true), name),
        "Optimization changed the configuration at C4 (inverted)."
    );
    assert_eq!(
        describe(&StereoChange::DoubleBond { atoms: [2, 3] }, name),
        "Optimization changed the E/Z configuration of the C4=C5 double bond."
    );
}

#[test]
fn the_config_follows_the_method_capabilities() {
    let mut settings = Settings {
        task: Task::Energy,
        solvent: Some("water"),
        temperature: "500".into(),
        ..Settings::default()
    };
    let config = settings.config((0, 0), false, vec![7]).expect("GFN2-xTB");
    assert_eq!(config.method, XtbMethod::Gfn2Xtb);
    assert_eq!(config.solvent, Some("water"));
    assert_eq!(config.temperature_k, Some(500.));
    assert!(
        config.fixed.is_empty(),
        "atoms are held only when optimizing"
    );

    // GFN-FF: no solvent, no SCF settings, closed shells only.
    settings.method = XtbMethod::GfnFf;
    settings.task = Task::Optimize;
    let config = settings.config((0, 0), false, vec![7]).expect("GFN-FF");
    assert_eq!(
        (
            config.solvent,
            config.temperature_k,
            config.fixed.as_slice()
        ),
        (None, None, &[7][..])
    );
    assert_eq!(
        settings.config((0, 1), false, vec![]),
        Err("GFN-FF calculates closed shells only: set Unpaired electrons to 0")
    );
    assert_eq!(
        method_note(XtbMethod::GfnFf, false).map(|n| n.contains("closed shells")),
        Some(true)
    );

    // g-xTB keeps the solvent and says how it is treated.
    settings.method = XtbMethod::GXtb;
    assert_eq!(
        settings.config((0, 0), false, vec![]).map(|c| c.solvent),
        Ok(Some("water"))
    );
    assert!(method_note(XtbMethod::GXtb, true).is_some_and(|n| n.contains("GFN2-xTB solvation")));
    assert_eq!(method_note(XtbMethod::GXtb, false), None);
}

#[test]
fn typed_scf_settings_are_checked() {
    let settings = |accuracy: &str, temperature: &str, iterations: &str| Settings {
        accuracy: accuracy.into(),
        temperature: temperature.into(),
        iterations: iterations.into(),
        ..Settings::default()
    };
    assert_eq!(settings("", "", "").scf(), Ok((None, None, None)));
    assert_eq!(
        settings(" 0.5 ", "1000", "400").scf(),
        Ok((Some(0.5), Some(1000.), Some(400)))
    );
    assert!(settings("0", "", "").scf().is_err());
    assert!(settings("", "−5", "").scf().is_err());
    assert!(settings("", "", "1.5").scf().is_err());
    // A bad field stops Run with its reason.
    assert_eq!(
        settings("", "", "0").config((0, 0), false, vec![]),
        Err("Max SCF iterations must be a whole number from 1 to 10000")
    );
}

#[test]
fn retry_runs_hotter_only_below_1000_k() {
    let config = Settings::default().config((0, 0), false, vec![]).unwrap();
    let hotter = config.hotter().expect("300 K by default");
    assert_eq!(hotter.temperature_k, Some(RETRY_TEMPERATURE_K));
    assert_eq!(
        Config {
            temperature_k: None,
            ..hotter.clone()
        },
        config
    );
    assert_eq!(hotter.hotter(), None, "already at 1000 K");
    let force_field = Config {
        method: XtbMethod::GfnFf,
        ..config
    };
    assert_eq!(force_field.hotter(), None, "GFN-FF has no SCF");
}

#[test]
fn failures_name_the_atom_the_limit_and_the_spin() {
    let (title, detail) = Failure::Element {
        z: 92,
        by: Some(XtbMethod::Gfn2Xtb),
    }
    .explain(XtbMethod::Gfn2Xtb, &[8, 92, 8]);
    assert_eq!(
        title,
        "Atom 2 is U (Z = 92). GFN2-xTB has no parameters for it."
    );
    assert!(detail.contains("radon (Z = 86)"));
    // g-xTB in water: its GFN2-xTB solvation part lacks uranium.
    let (title, detail) = Failure::Element {
        z: 92,
        by: Some(XtbMethod::Gfn2Xtb),
    }
    .explain(XtbMethod::GXtb, &[8, 92, 8]);
    assert!(title.ends_with("GFN2-xTB has no parameters for it."));
    assert!(detail.contains("gas-phase g-xTB covers this element"));
    let (title, _) = Failure::TooMany {
        count: 812,
        limit: 500,
    }
    .explain(XtbMethod::Gfn2Xtb, &[]);
    assert_eq!(
        title,
        "This molecule has 812 atoms. The limit is 500 atoms."
    );
    let (title, detail) = Failure::Spin { valence: 0 }.explain(XtbMethod::Gfn2Xtb, &[]);
    assert_eq!(
        title,
        "xTB's valence model gives 0 unpaired electrons for this molecule, which its full electron count cannot have."
    );
    assert!(detail.contains("Use g-xTB, or GFN-FF"));
    let (title, detail) = Failure::Scf("SCF did not converge in 250 iterations".into())
        .explain(XtbMethod::Gfn2Xtb, &[]);
    assert_eq!(title, "The SCF did not converge.");
    assert_eq!(
        detail,
        "SCF did not converge in 250 iterations. Nothing was applied to the drawing."
    );
}

#[test]
fn recalculate_reads_the_settings_the_result_stored() {
    let frame = qcdata::Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: vec![[0.; 3], [0.96, 0., 0.], [-0.24, 0.93, 0.]],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let result: qcdata::CalcResult = serde_json::from_value(serde_json::json!({
        "frame": frame.hash(),
        "task": "energy",
        "settings": { "xtb": {
            "method": "g_xtb",
            "conventions": "xtb-6.8",
            "solvent": { "model": "alpb", "name": "water", "correction": "gfn2" },
            "electronic_temperature_k": 1000.0,
            "accuracy": 0.5,
            "max_iterations": 400
        } },
        "provenance": { "program": "axtb", "source": { "calculated": { "engine_revision": "9d0388c" } } },
        "status": "converged"
    }))
    .expect("a stored result");
    let config = Config::from_result(&result, &frame, None, 3);
    assert_eq!(
        config,
        Config {
            task: Task::Energy,
            method: XtbMethod::GXtb,
            charge: 0,
            unpaired: 0,
            solvent: Some("water"),
            xtb68: true,
            accuracy: Some(0.5),
            temperature_k: Some(1000.),
            max_iterations: Some(400),
            threads: 3,
            tight: false,
            fixed: vec![],
        }
    );
}
