use super::*;

#[test]
fn gaussian_maps_basis_names_and_preserves_existing_spin_prefixes() {
    let frame = water();
    for (basis, expected) in [
        ("def2-SV(P)", "Def2SVPP"),
        ("def2-TZVPP", "Def2TZVPP"),
        ("cc-pVDZ", "cc-pVDZ"),
    ] {
        let mut settings = settings(Program::Gaussian, Job::SinglePoint);
        settings.basis = basis.into();
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert!(
            output
                .main
                .text
                .contains(&format!("B3LYP/{expected} EmpiricalDispersion=GD3BJ SP"))
        );
    }
    for method in ["UB3LYP", "uB3LYP", "ROHF"] {
        let mut settings = settings(Program::Gaussian, Job::SinglePoint);
        settings.method = method.into();
        settings.multiplicity = 2;
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert!(
            output
                .main
                .text
                .contains(&format!("#p {method}/Def2SVP SP"))
        );
    }
}

#[test]
fn orca_uses_the_right_open_shell_reference_and_memory_per_core() {
    let frame = water();
    for (method, prefix, needs_auxiliary_basis) in [
        ("HF", "UHF", false),
        ("MP2", "UHF", false),
        ("RI-MP2", "UHF", true),
        ("DLPNO-CCSD(T)", "UHF", true),
        ("CCSD(T)", "UHF", false),
        ("B2PLYP", "UKS", false),
        ("PBE0", "UKS", false),
    ] {
        let mut settings = settings(Program::Orca, Job::SinglePoint);
        settings.method = method.into();
        settings.multiplicity = 2;
        settings.cores = 7;
        settings.memory_gb = 2;
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        let text = &output.main.text;
        assert!(text.contains(&format!("! {prefix} {method} def2-SVP SP\n")));
        assert!(text.contains("%pal nprocs 7 end\n%maxcore 219\n"));
        let warning = format!(
            "{method} needs an auxiliary basis: add e.g. def2-SVP/C or AutoAux under Extra keywords."
        );
        assert_eq!(output.warnings.contains(&warning), needs_auxiliary_basis);
        assert!(
            output
                .warnings
                .iter()
                .all(|warning| !warning.contains("no analytic gradients"))
        );
    }
    let mut settings = settings(Program::Orca, Job::SinglePoint);
    settings.cores = 1;
    settings.memory_gb = 1;
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert!(!output.main.text.contains("%pal"));
    assert!(output.main.text.contains("%maxcore 768\n"));
}

#[test]
fn orca_gradient_and_frequency_jobs_use_method_specific_keywords_and_warnings() {
    let frame = water();
    let reactant = shifted_water(0.05);
    let product = shifted_water(-0.05);
    for (job, keyword, frequencies) in [
        (Job::Optimize, "Opt", false),
        (Job::OptFreq, "Opt", true),
        (Job::TransitionState, "OptTS", true),
        (Job::Irc, "IRC", false),
        (Job::NebTs, "NEB-TS", true),
    ] {
        for (method, frequency, needs_numgrad, needs_auxiliary_basis) in [
            ("MP2", "NumFreq", false, false),
            ("ri-mp2", "NumFreq", false, true),
            ("rimp2", "NumFreq", false, true),
            ("SCS-MP2", "NumFreq", false, false),
            ("DLPNO-MP2", "NumFreq", false, true),
            ("B2PLYP", "NumFreq", false, false),
            ("DLPNO-CCSD(T)", "NumFreq", true, true),
            ("CCSD(T)", "NumFreq", true, false),
            ("QCISD(T)", "NumFreq", true, false),
            ("RI-B3LYP", "Freq", false, true),
            ("HF", "Freq", false, false),
            ("B3LYP", "Freq", false, false),
        ] {
            let mut settings = settings(Program::Orca, job);
            settings.method = method.into();
            let geometry = Geometry {
                reactant: Some(&reactant),
                product: Some(&product),
                ..geometry(&frame)
            };
            let output = write(&settings, &geometry, "water", GENERATOR).unwrap();
            let route = if frequencies {
                format!("! {method} def2-SVP {keyword} {frequency}\n")
            } else {
                format!("! {method} def2-SVP {keyword}\n")
            };
            assert!(output.main.text.contains(&route), "{route}");
            assert_eq!(
                output.warnings.contains(&format!(
                    "ORCA has no analytic gradients for {method}; add NumGrad under Extra keywords."
                )),
                needs_numgrad,
                "{job:?}, {method}"
            );
            assert_eq!(
                output.warnings.contains(&format!(
                    "{method} needs an auxiliary basis: add e.g. def2-SVP/C or AutoAux under Extra keywords."
                )),
                needs_auxiliary_basis,
                "{job:?}, {method}"
            );
        }
    }
}

#[test]
fn orca_accepts_an_auxiliary_basis_or_autoaux_in_extra_keywords() {
    let frame = water();
    let mut settings = settings(Program::Orca, Job::SinglePoint);
    settings.method = "SCS-RI-MP2".into();
    for basis in ["def2-svp/c", "aUtOaUx"] {
        settings.extra = format!("TightSCF\r\n{basis}");
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert!(output.main.text.contains(&format!("SP TightSCF {basis}\n")));
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
    }
}

#[test]
fn orca_warns_when_maxcore_is_below_1000_mb() {
    let frame = water();
    let mut settings = settings(Program::Orca, Job::SinglePoint);
    settings.memory_gb = 121;
    settings.cores = 93;
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert!(output.main.text.contains("%maxcore 999\n"));
    assert_eq!(
        output.warnings,
        ["ORCA gets only 999 MB per core; raise Memory or lower Cores."]
    );

    settings.memory_gb = 125;
    settings.cores = 96;
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert!(output.main.text.contains("%maxcore 1000\n"));
    assert!(output.warnings.is_empty());
}

#[test]
fn gamess_basis_substitution_keeps_supported_names_and_aliases() {
    for basis in [
        "6-31G(d)",
        "6-31g(D)",
        "6-31G*",
        "6-31G",
        "STO-3G",
        " sto-3g\t",
    ] {
        assert!(gamess::basis_keywords(basis.trim()).is_some());
        assert_eq!(nearest_gamess_basis(basis), None, "{basis}");
    }
}

#[test]
fn gamess_basis_substitutions_write_with_the_nearest_builtin() {
    let frame = water();
    for (basis, substitute, group) in [
        ("STO-2G", "STO-3G", "GBASIS=STO NGAUSS=3"),
        (" sto-6g ", "STO-3G", "GBASIS=STO NGAUSS=3"),
        ("3-21G", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("4-31G", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("6-21G", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("6-31+G", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("6-31++G", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("6-311G", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("6-311++G", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("SV", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("def-SV", "6-31G", "GBASIS=N31 NGAUSS=6"),
        (" def2-sv ", "6-31G", "GBASIS=N31 NGAUSS=6"),
        ("def2-SVP", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("def2-SV(P)", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("def2-TZVP", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("cc-pVDZ", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("6-31G**", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("6-311G(d,p)", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("3-21G*", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("6-31+G(d)", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("custom", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("", "6-31G(d)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
    ] {
        assert_eq!(nearest_gamess_basis(basis), Some(substitute), "{basis}");
        let mut settings = settings(Program::Gamess, Job::SinglePoint);
        settings.basis = nearest_gamess_basis(basis).unwrap().into();
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert!(
            output
                .main
                .text
                .contains(&format!(" $BASIS {group} $END\n"))
        );
    }
}

#[test]
fn gamess_maps_builtin_bases_hf_mp2_and_per_process_memory() {
    let frame = water();
    for (basis, group) in [
        ("6-31g(D)", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("6-31G*", "GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        ("6-31G", "GBASIS=N31 NGAUSS=6"),
        ("sto-3g", "GBASIS=STO NGAUSS=3"),
    ] {
        let mut settings = settings(Program::Gamess, Job::SinglePoint);
        settings.basis = basis.into();
        settings.method = "HF".into();
        settings.memory_gb = 2;
        settings.cores = 7;
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        let text = &output.main.text;
        assert!(text.contains(&format!(" $BASIS {group} $END\n")));
        assert!(!text.contains("DFTTYP="));
        assert!(!text.contains("MPLEVL="));
        assert!(text.contains(" $SYSTEM MWORDS=35 $END\n"));
        structure::check(Program::Gamess, text, 3, 1);
    }
    let mut settings = settings(Program::Gamess, Job::SinglePoint);
    settings.method = "mp2".into();
    settings.memory_gb = 1;
    settings.cores = 1000;
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert!(output.main.text.contains("MPLEVL=2"));
    assert!(!output.main.text.contains("DFTTYP="));
    assert!(output.main.text.contains(" $SYSTEM MWORDS=1 $END\n"));
}

#[test]
fn gamess_warns_about_memddi_for_parallel_mp2() {
    let frame = water();
    let mut settings = settings(Program::Gamess, Job::SinglePoint);
    settings.method = "mp2".into();
    settings.cores = 2;
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert_eq!(
        output.warnings,
        [
            "GAMESS takes the number of cores when you start it (rungms), not from the file.",
            "Parallel GAMESS MP2 needs MEMDDI: add a line such as  $SYSTEM MEMDDI=100 $END under Extra keywords.",
        ]
    );
    settings.extra = "$SYSTEM memddi=100 $END".into();
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert_eq!(output.warnings.len(), 1);
    assert!(output.main.text.contains(" $SYSTEM memddi=100 $END\n"));

    settings.cores = 1;
    settings.extra.clear();
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert_eq!(output.warnings.len(), 1);
}

#[test]
fn gamess_warns_about_written_lines_over_80_columns() {
    let frame = water();
    let mut settings = settings(Program::Gamess, Job::SinglePoint);
    settings.title = "x".repeat(78);
    settings.extra = "x".repeat(79);
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert_eq!(output.warnings.len(), 1);

    for (title_chars, extra_chars) in [(79, 79), (78, 80)] {
        settings.title = "x".repeat(title_chars);
        settings.extra = "x".repeat(extra_chars);
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert_eq!(
            output.warnings.last().unwrap(),
            "GAMESS reads 80 columns; shorten the title or Extra keywords."
        );
    }
}

#[test]
fn gaussian_and_orca_write_method_basis_and_extra_on_one_line() {
    let frame = water();
    for (program, route) in [
        (
            Program::Gaussian,
            "#p HF A B C/X Y Z W SP Grid Tight SCF Keep",
        ),
        (Program::Orca, "! HF A B C X Y Z W SP Grid Tight SCF Keep"),
    ] {
        let mut settings = settings(program, Job::SinglePoint);
        settings.method = " HF\r\nA\rB\nC ".into();
        settings.basis = " X\r\nY\rZ\nW ".into();
        settings.extra = " Grid\r\nTight\rSCF\nKeep ".into();
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert!(output.main.text.lines().any(|line| line == route));
    }
}

#[test]
fn gamess_writes_dft_method_on_one_line() {
    let frame = water();
    let mut settings = settings(Program::Gamess, Job::SinglePoint);
    settings.method = " b3lyp\r\nA\rB\nC ".into();
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert!(
        output
            .main
            .text
            .lines()
            .any(|line| line == " $CONTRL SCFTYP=RHF RUNTYP=ENERGY DFTTYP=B3LYP A B C")
    );
}

#[test]
fn gamess_indents_every_nonempty_extra_line() {
    let frame = water();
    let mut settings = settings(Program::Gamess, Job::SinglePoint);
    settings.extra =
        "  $SCF DIRSCF=.T. $END  \r\n\t\n $SYSTEM MEMDDI=100 $END\r  $GUESS GUESS=HUCKEL $END "
            .into();
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    assert!(output.main.text.contains(
        "\n $SCF DIRSCF=.T. $END\n $SYSTEM MEMDDI=100 $END\n $GUESS GUESS=HUCKEL $END\n $DATA\n"
    ));
}

#[test]
fn transition_state_headers_use_provenance_and_the_most_negative_frequency() {
    let frame = water();
    let mut result = result(&frame);
    result.provenance.method = Some("GFN2-xTB\r\nA\rB\nC".into());
    result.vibrations.as_mut().unwrap().frequencies_cm1 = vec![-600.7, -412.3, 3700.0];
    for program in [Program::Gaussian, Program::Orca, Program::Gamess] {
        let settings = settings(program, Job::TransitionState);
        let with_result = Geometry {
            result: Some(&result),
            ..geometry(&frame)
        };
        let output = write(&settings, &with_result, "water", GENERATOR).unwrap();
        let prefix = if program == Program::Orca { '#' } else { '!' };
        assert_eq!(
            output.main.text.lines().next().unwrap(),
            format!(
                "{prefix} TS guess from GFN2-xTB A B C ({GENERATOR}), imaginary frequency -601 cm-1"
            )
        );
        let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
        assert_eq!(
            output.main.text.lines().next().unwrap(),
            format!("{prefix} TS guess from the 3D model ({GENERATOR})")
        );
    }
    result.provenance.method = None;
    result.vibrations.as_mut().unwrap().frequencies_cm1 = vec![100.0, 1600.0, 3700.0];
    let with_result = Geometry {
        result: Some(&result),
        ..geometry(&frame)
    };
    let output = write(
        &settings(Program::Gaussian, Job::TransitionState),
        &with_result,
        "water",
        GENERATOR,
    )
    .unwrap();
    assert_eq!(
        output.main.text.lines().next().unwrap(),
        format!("! TS guess from the 3D model ({GENERATOR})")
    );
    for program in Program::ALL {
        let output = write(
            &settings(program, Job::SinglePoint),
            &with_result,
            "water",
            GENERATOR,
        )
        .unwrap();
        assert!(!output.main.text.contains(GENERATOR));
    }
}
