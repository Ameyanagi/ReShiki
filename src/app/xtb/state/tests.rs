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
    settings.retarget(Some(7));
    settings.charge = "−2".into();
    settings.unpaired = " 1 ".into();
    assert_eq!(settings.state((0, 0)), Ok((-2, 1)));
    settings.unpaired = "-1".into();
    assert!(settings.state((0, 0)).is_err());
    settings.charge = "1.5".into();
    assert_eq!(settings.state((0, 0)), Err("Charge must be a whole number"));
    // The same molecule keeps them; another starts from its drawing.
    settings.retarget(Some(7));
    assert_eq!(settings.charge, "1.5");
    settings.retarget(Some(8));
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
