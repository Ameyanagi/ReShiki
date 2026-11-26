use super::*;

fn invalid(settings: &Settings, geometry: &Geometry<'_>, expected: &str) {
    let error = write(settings, geometry, "water", GENERATOR).unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(error.to_string(), expected);
}

#[test]
fn malformed_frames_are_refused_by_every_writer() {
    let message = "The structure has an unknown element or a missing position.";
    let mut missing = water();
    missing.positions_angstrom.pop();
    let mut unknown = water();
    unknown.numbers[0] = 0;
    let mut infinite = water();
    infinite.positions_angstrom[1][0] = f64::NAN;
    for frame in [&missing, &unknown, &infinite] {
        for program in Program::ALL {
            invalid(
                &settings(program, Job::SinglePoint),
                &geometry(frame),
                message,
            );
        }
        let entry = SdfEntry {
            frame,
            title: "water",
            bonds: &[],
            formal_charges: &[],
            properties: &[],
        };
        assert_eq!(write_sdf(&[entry]).unwrap_err().to_string(), message);
        let hessian = vec![0.0; 81];
        assert_eq!(
            write_orca_hess(frame, &hessian).unwrap_err().to_string(),
            message
        );
    }
}

#[test]
fn validation_messages() {
    let frame = water();
    let geometry = geometry(&frame);
    let base = settings(Program::Gaussian, Job::SinglePoint);
    let mut settings = base.clone();
    settings.method = " \n ".into();
    invalid(&settings, &geometry, "Enter a method.");
    settings = base.clone();
    settings.multiplicity = 0;
    invalid(&settings, &geometry, "Multiplicity must be at least 1.");
    for (cores, memory) in [(0, 16), (8, 0)] {
        settings = base.clone();
        settings.cores = cores;
        settings.memory_gb = memory;
        invalid(&settings, &geometry, "Cores and memory must be at least 1.");
    }
    let mut empty = frame.clone();
    empty.numbers.clear();
    empty.positions_angstrom.clear();
    invalid(
        &base,
        &super::geometry(&empty),
        "The structure has no atoms.",
    );

    for (program, job, expected) in [
        (
            Program::Gaussian,
            Job::NebTs,
            "Gaussian offers Single point, Optimize, Opt + freq, Transition state, QST2, QST3, IRC.",
        ),
        (
            Program::Orca,
            Job::Qst2,
            "ORCA offers Single point, Optimize, Opt + freq, Transition state, IRC, NEB-TS.",
        ),
        (
            Program::Gamess,
            Job::Irc,
            "GAMESS offers Single point, Optimize, Opt + freq, Transition state.",
        ),
    ] {
        invalid(&super::settings(program, job), &geometry, expected);
    }
    for program in [Program::Gaussian, Program::Gamess] {
        let mut settings = super::settings(program, Job::SinglePoint);
        settings.method = "r2SCAN-3C".into();
        invalid(
            &settings,
            &geometry,
            &format!(
                "r2SCAN-3C is an ORCA composite method; {} does not offer it.",
                program.label()
            ),
        );
    }
    let mut settings = super::settings(Program::Gamess, Job::SinglePoint);
    settings.basis = "def2-SVP".into();
    invalid(
        &settings,
        &geometry,
        "GAMESS exports use built-in bases only: 6-31G(d), 6-31G or STO-3G.",
    );
}

#[test]
fn orca_companion_names_preserve_safe_stems_and_reject_unsafe_quoted_paths() {
    let frame = water();
    let product = shifted_water(-0.05);
    let result = result(&frame);
    let geometry = Geometry {
        frame: &frame,
        reactant: Some(&frame),
        product: Some(&product),
        result: Some(&result),
    };
    for job in [Job::NebTs, Job::TransitionState, Job::Irc] {
        let settings = settings(Program::Orca, job);
        for stem in ["sample_a", "sample-a", "sample a", "試料 a"] {
            let output = write(&settings, &geometry, stem, GENERATOR).unwrap();
            let suffixes: &[&str] = if job == Job::NebTs {
                &["_product.xyz", "_ts.xyz"]
            } else {
                &["_xtb.hess"]
            };
            let expected: Vec<_> = suffixes
                .iter()
                .map(|suffix| format!("{stem}{suffix}"))
                .collect();
            assert_eq!(
                output
                    .companions
                    .iter()
                    .map(|file| &file.name)
                    .collect::<Vec<_>>(),
                expected.iter().collect::<Vec<_>>()
            );
            for name in expected {
                assert!(output.main.text.contains(&format!("\"{name}\"")));
            }
        }
        for stem in ["a\"b", "a/b", "a\\b", "a\nb", "a\rb", "a\0b"] {
            assert_eq!(
                write(&settings, &geometry, stem, GENERATOR)
                    .unwrap_err()
                    .to_string(),
                "ORCA companion file names cannot contain quotes, path separators or control characters."
            );
        }
    }
}

#[test]
fn endpoint_validation_checks_presence_and_atom_order() {
    let frame = water();
    let product = shifted_water(-0.05);
    let mut wrong = frame.clone();
    wrong.numbers.swap(0, 1);
    for (program, job) in [
        (Program::Gaussian, Job::Qst2),
        (Program::Gaussian, Job::Qst3),
        (Program::Orca, Job::NebTs),
    ] {
        let settings = settings(program, job);
        for (reactant, product) in [(None, None), (Some(&frame), None), (None, Some(&product))] {
            invalid(
                &settings,
                &Geometry {
                    reactant,
                    product,
                    ..geometry(&frame)
                },
                &format!(
                    "{} needs the reactant and product of a transition-state search.",
                    job.label()
                ),
            );
        }
        for (reactant, product) in [(&wrong, &product), (&frame, &wrong)] {
            invalid(
                &settings,
                &Geometry {
                    reactant: Some(reactant),
                    product: Some(product),
                    ..geometry(&frame)
                },
                "The reactant, product and TS guess must list the same atoms in the same order.",
            );
        }
    }
}

#[test]
fn calculation_result_must_match_the_geometry() {
    let frame = water();
    let result = result(&shifted_water(0.05));
    let with_result = Geometry {
        result: Some(&result),
        ..geometry(&frame)
    };
    for program in Program::ALL {
        invalid(
            &settings(program, Job::TransitionState),
            &with_result,
            "The calculation result belongs to another geometry.",
        );
    }
}

#[test]
fn calculation_result_must_match_the_requested_charge_and_multiplicity() {
    let mut frame = water();
    frame.charge = 1;
    frame.multiplicity = 2;
    let result = result(&frame);
    let with_result = Geometry {
        result: Some(&result),
        ..geometry(&frame)
    };
    for program in Program::ALL {
        let mut settings = settings(program, Job::TransitionState);
        for (charge, multiplicity) in [(0, 2), (1, 1)] {
            settings.charge = charge;
            settings.multiplicity = multiplicity;
            invalid(
                &settings,
                &with_result,
                "The calculation result is for charge 1 and multiplicity 2.",
            );
        }
        settings.charge = frame.charge;
        settings.multiplicity = frame.multiplicity;
        write(&settings, &with_result, "water", GENERATOR).unwrap();
    }
}

#[test]
fn endpoint_drawing_ids_must_match_when_present() {
    let mut frame = water();
    frame.drawing_atoms = vec![Some(1), Some(2), Some(3)];
    let mut wrong = frame.clone();
    wrong.drawing_atoms.swap(1, 2);
    let without_ids = water();
    let settings = settings(Program::Gaussian, Job::Qst3);
    for (guess, reactant, product) in [
        (&frame, &wrong, &frame),
        (&frame, &frame, &wrong),
        (&without_ids, &frame, &wrong),
    ] {
        let geometry = Geometry {
            reactant: Some(reactant),
            product: Some(product),
            ..geometry(guess)
        };
        for settings in [&settings, &super::settings(Program::Xyz, Job::SinglePoint)] {
            invalid(
                settings,
                &geometry,
                "The reactant, product and TS guess must list the same atoms in the same order.",
            );
        }
    }
    for (frame, endpoint) in [
        (&frame, &frame),
        (&frame, &without_ids),
        (&without_ids, &frame),
    ] {
        let geometry = Geometry {
            reactant: Some(endpoint),
            product: Some(endpoint),
            ..geometry(frame)
        };
        write(&settings, &geometry, "water", GENERATOR).unwrap();
    }
}

#[test]
fn warnings_have_exact_text_and_order() {
    let mut frame = water();
    frame.isotopes = vec![18, 0, 0];
    let mut gaussian = settings(Program::Gaussian, Job::SinglePoint);
    gaussian.charge = 1;
    gaussian.basis.clear();
    assert_eq!(
        write(&gaussian, &geometry(&frame), "water", GENERATOR)
            .unwrap()
            .warnings,
        [
            "Charge 1 and multiplicity 1 do not fit 9 electrons.",
            "No basis set: Gaussian uses STO-3G.",
        ]
    );
    let mut orca = settings(Program::Orca, Job::TransitionState);
    orca.charge = 1;
    orca.method = "r2SCAN-3c".into();
    assert_eq!(
        write(&orca, &geometry(&frame), "water", GENERATOR)
            .unwrap()
            .warnings,
        [
            "Charge 1 and multiplicity 1 do not fit 9 electrons.",
            "Isotopes are not written; set them in ORCA if you need them.",
            "r2SCAN-3c brings its own basis; the Basis field is ignored.",
            "No xTB Hessian: ORCA computes the first Hessian itself.",
        ]
    );
    let mut gamess = settings(Program::Gamess, Job::SinglePoint);
    gamess.charge = 1;
    assert_eq!(
        write(&gamess, &geometry(&frame), "water", GENERATOR)
            .unwrap()
            .warnings,
        [
            "Charge 1 and multiplicity 1 do not fit 9 electrons.",
            "Isotopes are not written; set them in GAMESS if you need them.",
            "GAMESS takes the number of cores when you start it (rungms), not from the file.",
        ]
    );
    let frame = water();
    for (charge, multiplicity, electrons) in [(11, 1, -1), (0, 12, 10)] {
        let mut settings = settings(Program::Orca, Job::SinglePoint);
        settings.charge = charge;
        settings.multiplicity = multiplicity;
        assert_eq!(
            write(&settings, &geometry(&frame), "water", GENERATOR)
                .unwrap()
                .warnings,
            [format!(
                "Charge {charge} and multiplicity {multiplicity} do not fit {electrons} electrons."
            ),]
        );
    }
    for job in [Job::TransitionState, Job::Irc] {
        let settings = settings(Program::Orca, job);
        assert_eq!(
            write(&settings, &geometry(&frame), "water", GENERATOR)
                .unwrap()
                .warnings,
            ["No xTB Hessian: ORCA computes the first Hessian itself.",]
        );
        let result = result(&frame);
        let with_result = Geometry {
            result: Some(&result),
            ..geometry(&frame)
        };
        assert!(
            write(&settings, &with_result, "water", GENERATOR)
                .unwrap()
                .warnings
                .is_empty()
        );
    }
}

#[test]
fn xyz_ignores_settings_and_preserves_isotopes() {
    let mut frame = water();
    frame.isotopes = vec![18, 2, 3];
    let mut settings = settings(Program::Xyz, Job::Qst3);
    settings.method.clear();
    settings.basis.clear();
    settings.multiplicity = 0;
    settings.cores = 0;
    settings.memory_gb = 0;
    settings.charge = i32::MAX;
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert!(output.warnings.is_empty());
    assert!(output.companions.is_empty());
    assert_eq!(output.main.name, "water.xyz");
    let restored = crate::qcread::read(&output.main.name, output.main.text.as_bytes()).unwrap();
    assert_eq!(restored.jobs[0].steps[0].frame.isotopes, [18, 2, 3]);
    assert_eq!(parse_xyz(&output.main.text).unwrap().isotopes, [18, 2, 3]);
    frame.numbers.clear();
    frame.positions_angstrom.clear();
    frame.isotopes.clear();
    invalid(&settings, &geometry(&frame), "The structure has no atoms.");
}

#[test]
fn public_catalogs_and_template_defaults() {
    assert_eq!(
        Program::ALL,
        [
            Program::Gaussian,
            Program::Orca,
            Program::Gamess,
            Program::Xyz
        ]
    );
    assert_eq!(
        Program::Gaussian.jobs(),
        &[
            Job::SinglePoint,
            Job::Optimize,
            Job::OptFreq,
            Job::TransitionState,
            Job::Qst2,
            Job::Qst3,
            Job::Irc
        ]
    );
    assert_eq!(
        Program::Orca.jobs(),
        &[
            Job::SinglePoint,
            Job::Optimize,
            Job::OptFreq,
            Job::TransitionState,
            Job::Irc,
            Job::NebTs
        ]
    );
    assert_eq!(
        Program::Gamess.jobs(),
        &[
            Job::SinglePoint,
            Job::Optimize,
            Job::OptFreq,
            Job::TransitionState
        ]
    );
    assert!(Program::Xyz.jobs().is_empty());
    for (program, label, extension) in [
        (Program::Gaussian, "Gaussian", "gjf"),
        (Program::Orca, "ORCA", "inp"),
        (Program::Gamess, "GAMESS", "inp"),
        (Program::Xyz, "XYZ", "xyz"),
    ] {
        assert_eq!(program.label(), label);
        assert_eq!(program.extension(), extension);
        assert_eq!(serde_json::to_value(program).unwrap(), key(program));
    }
    let names = [
        "single-point",
        "optimize",
        "opt-freq",
        "transition-state",
        "qst2",
        "qst3",
        "irc",
        "neb-ts",
    ];
    let labels = [
        "Single point",
        "Optimize",
        "Opt + freq",
        "Transition state",
        "QST2",
        "QST3",
        "IRC",
        "NEB-TS",
    ];
    for ((job, name), label) in Job::ALL.into_iter().zip(names).zip(labels) {
        assert_eq!(job_key(job), name);
        assert_eq!(Job::parse(name), Some(job));
        assert_eq!(job.label(), label);
        assert_eq!(
            job.is_transition_state(),
            !matches!(job, Job::SinglePoint | Job::Optimize | Job::OptFreq)
        );
    }
    for (name, job) in [
        ("sp", Job::SinglePoint),
        ("opt", Job::Optimize),
        ("opt+freq", Job::OptFreq),
        ("ts", Job::TransitionState),
    ] {
        assert_eq!(Job::parse(name), Some(job));
    }
    assert_eq!(Job::parse("unsupported"), None);
    let templates = builtin_templates();
    assert_eq!(templates.len(), 3);
    for (template, (name, method, basis, job, only)) in templates.iter().zip([
        (
            "B3LYP-D3BJ/def2-SVP opt+freq",
            "B3LYP-D3BJ",
            "def2-SVP",
            Job::OptFreq,
            None,
        ),
        (
            "r2SCAN-3c opt+freq",
            "r2SCAN-3c",
            "",
            Job::OptFreq,
            Some(Program::Orca),
        ),
        (
            "HF/6-31G(d) single point",
            "HF",
            "6-31G(d)",
            Job::SinglePoint,
            None,
        ),
    ]) {
        assert_eq!(
            (
                &*template.name,
                &*template.method,
                &*template.basis,
                template.job,
                template.only
            ),
            (name, method, basis, job, only)
        );
        assert_eq!(
            (template.cores, template.memory_gb, &*template.extra),
            (8, 16, "")
        );
        let settings = Settings::from_template(template, Program::Orca, -1, 2, "title".into());
        assert_eq!(
            (
                settings.program,
                settings.job,
                settings.charge,
                settings.multiplicity
            ),
            (Program::Orca, job, -1, 2)
        );
        assert_eq!(
            (
                settings.method.as_str(),
                settings.basis.as_str(),
                settings.title.as_str()
            ),
            (method, basis, "title")
        );
        assert_eq!(
            (
                settings.cores,
                settings.memory_gb,
                settings.irc_points,
                settings.neb_images
            ),
            (8, 16, 30, 12)
        );
        assert_eq!(
            serde_json::from_value::<Settings>(serde_json::to_value(&settings).unwrap()).unwrap(),
            settings
        );
        assert_eq!(
            serde_json::from_value::<Template>(serde_json::to_value(template).unwrap()).unwrap(),
            *template
        );
    }
    let template: Template = serde_json::from_str(r#"{"name":"minimal","method":"HF","basis":"","job":"single-point","cores":1,"memory_gb":1}"#).unwrap();
    assert_eq!(template.extra, "");
    assert_eq!(template.only, None);
    assert!(
        serde_json::to_value(template)
            .unwrap()
            .get("only")
            .is_none()
    );
}
