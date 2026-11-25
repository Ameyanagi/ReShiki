use super::{Position, cross, sub, unit};
use crate::qcread::{
    Error, Format, Job, JobKind, read,
    tests::{fixture, open},
};
use reshiki_qcdata::ANGSTROM_PER_BOHR;

fn one_job(jobs: &[Job], atoms: usize, charge: i32, mult: u32) -> &Job {
    let [job] = jobs else {
        panic!("expected one geometry job")
    };
    assert_eq!(job.index, 0);
    assert_eq!(job.kind, JobKind::Geometry);
    assert!(job.completion.is_none() && job.result.is_none());
    let [step] = &job.steps[..] else {
        panic!("expected one geometry")
    };
    assert!(step.energy_hartree.is_none());
    assert_eq!(step.frame.numbers.len(), atoms);
    assert_eq!(step.frame.charge, charge);
    assert_eq!(step.frame.multiplicity, mult);
    job
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} differs from {expected}"
    );
}

fn dot(a: Position, b: Position) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}

fn distance(a: Position, b: Position) -> f64 {
    let v = sub(a, b);
    dot(v, v).sqrt()
}

fn angle(a: Position, center: Position, b: Position) -> f64 {
    dot(unit(sub(a, center)).unwrap(), unit(sub(b, center)).unwrap())
        .acos()
        .to_degrees()
}

fn dihedral(a: Position, b: Position, c: Position, d: Position) -> f64 {
    let axis = sub(c, b);
    let n1 = cross(sub(b, a), axis);
    let n2 = cross(axis, sub(d, c));
    dot(cross(n1, n2), unit(axis).unwrap())
        .atan2(dot(n1, n2))
        .to_degrees()
}

#[test]
fn gaussian_dvb_reads_a_z_matrix_and_its_route() {
    let file = open("cclib/g16_dvb_gopt.gjf");
    assert_eq!(file.format, Format::GaussianInput);
    assert!(file.program.is_none());
    // 0 1
    let job = one_job(&file.jobs, 20, 0, 1);
    // #p b3lyp/sto-3g opt
    assert_eq!(job.keywords, ["p", "b3lyp/sto-3g", "opt"]);
    assert_eq!(job.method.as_deref(), Some("b3lyp"));
    assert_eq!(job.basis.as_deref(), Some("sto-3g"));
    let xyz = &job.steps[0].frame.positions_angstrom;
    // B1             1.39155762
    close(distance(xyz[0], xyz[1]), 1.39155762);
    // A1           120.26061806
    close(angle(xyz[2], xyz[1], xyz[0]), 120.26061806);
}

#[test]
fn gaussian_water_reads_equals_delimited_variables() {
    let file = open("cclib/g16_water_mp2.com");
    // 0 1
    let job = one_job(&file.jobs, 3, 0, 1);
    let step = &job.steps[0];
    assert_eq!(step.frame.numbers, [8, 1, 1]);
    assert_eq!(step.label.as_deref(), Some("Water"));
    let xyz = &step.frame.positions_angstrom;
    // R1=0.99
    // A1=106.0
    close(distance(xyz[0], xyz[1]), 0.99);
    close(distance(xyz[0], xyz[2]), 0.99);
    close(angle(xyz[1], xyz[0], xyz[2]), 106.);
}

#[test]
fn gaussian_methylpyrrolidine_keeps_charge_spin_and_signed_torsions() {
    let file = open("pymatgen/MethylPyrrolidine_drawn.gjf");
    // -1 2
    let xyz = &one_job(&file.jobs, 17, -1, 2).steps[0]
        .frame
        .positions_angstrom;
    // B1             1.52185800
    // A1           107.94031680
    // D1             4.10236229
    // D3          -116.64610589
    close(distance(xyz[0], xyz[1]), 1.521858);
    close(angle(xyz[2], xyz[1], xyz[0]), 107.9403168);
    close(dihedral(xyz[3], xyz[0], xyz[1], xyz[2]), 4.10236229);
    close(dihedral(xyz[5], xyz[0], xyz[1], xyz[2]), -116.64610589);
}

#[test]
fn gaussian_style_inp_reads_variables_without_a_blank_separator() {
    let file = open("pymatgen/l-cysteine.inp");
    assert_eq!(file.format, Format::GaussianInput);
    // 0  1
    let job = one_job(&file.jobs, 14, 0, 1);
    // #opt=(maxcycles=80,tight,Z-Matrix)
    assert!(
        job.keywords
            .iter()
            .any(|word| word == "opt=(maxcycles=80,tight,Z-Matrix)")
    );
    let xyz = &job.steps[0].frame.positions_angstrom;
    // r2= 1.3066
    // a3= 122.23
    close(distance(xyz[0], xyz[1]), 1.3066);
    close(angle(xyz[2], xyz[0], xyz[1]), 122.23);
}

#[test]
fn gaussian_c60_reads_cartesian_coordinates_and_multiline_route() {
    let file = open("pymatgen/C60_gau.com");
    // 0 1
    let job = one_job(&file.jobs, 60, 0, 1);
    assert_eq!(job.steps[0].frame.numbers, [6; 60]);
    // C    -2.877247335830    -1.161378164125    -6.518472458604
    assert_eq!(
        job.steps[0].frame.positions_angstrom[59],
        [-2.877247335830, -1.161378164125, -6.518472458604]
    );
    //    Guess=Huckel Scfcyc=1500 SCF=Tight
    assert!(job.keywords.iter().any(|w| w == "Scfcyc=1500"));
}

#[test]
fn gaussian_finds_geometry_after_a_checkpoint_only_link() {
    // Replace the water route with a checkpoint reference, then append the original water job.
    let bytes = fixture("cclib/g16_water_mp2.com");
    let text = std::str::from_utf8(&bytes).unwrap();
    for geom in ["Geom=Check", "Geom=(AllCheck,NewDefinition)"] {
        let checkpoint = text.replace("Density", geom);
        assert_eq!(read("checkpoint.com", checkpoint.as_bytes()), Err(Error::Unsupported(
            "The geometry is read from the checkpoint file (Geom=Check); open the log or fchk instead".into()
        )));
        let linked = format!("{checkpoint}\n--Link1--\n{text}");
        let file = read("linked.com", linked.as_bytes()).unwrap();
        one_job(&file.jobs, 3, 0, 1);
        assert_eq!(file.warnings, ["Read the geometry of the first job"]);
    }
}

#[test]
fn gaussian_bohr_and_angstrom_inputs_agree() {
    // Re-express the Gaussian water fixture's R1 variable in bohr.
    let bytes = fixture("cclib/g16_water_mp2.com");
    let text = std::str::from_utf8(&bytes).unwrap();
    let expected = open("cclib/g16_water_mp2.com");
    for units in ["Units=Bohr", "units=au"] {
        let bohr = text
            .replace("#P ", &format!("#P {units} "))
            .replace("R1=0.99", &format!("R1={}", 0.99 / ANGSTROM_PER_BOHR));
        let actual = read("bohr.com", bohr.as_bytes()).unwrap();
        for (a, b) in actual.jobs[0].steps[0]
            .frame
            .positions_angstrom
            .iter()
            .flatten()
            .zip(
                expected.jobs[0].steps[0]
                    .frame
                    .positions_angstrom
                    .iter()
                    .flatten(),
            )
        {
            close(*a, *b);
        }
    }
}

#[test]
fn gaussian_parenthesized_units_convert_bohr() {
    let bytes = fixture("cclib/g16_water_mp2.com");
    let text = std::str::from_utf8(&bytes).expect("Gaussian water fixture is UTF-8");
    let expected = open("cclib/g16_water_mp2.com");
    for units in ["Units(Bohr)", "Units(AU)"] {
        // "#P MP2/STO-3G Density Pop=(Full,NaturalOrbitals)"; "R1=0.99".
        let bohr = text
            .replace("#P ", &format!("#P {units} "))
            .replace("R1=0.99", &format!("R1={}", 0.99 / ANGSTROM_PER_BOHR));
        let actual = read("bohr.com", bohr.as_bytes()).expect("parenthesized units");
        let actual = &one_job(&actual.jobs, 3, 0, 1).steps[0].frame;
        for (a, b) in actual.positions_angstrom.iter().flatten().zip(
            expected.jobs[0].steps[0]
                .frame
                .positions_angstrom
                .iter()
                .flatten(),
        ) {
            close(*a, *b);
        }
    }
}

#[test]
fn gaussian_parenthesized_checkpoint_routes_are_unsupported() {
    let bytes = fixture("cclib/g16_water_mp2.com");
    let text = std::str::from_utf8(&bytes).expect("Gaussian water fixture is UTF-8");
    for geom in ["Geom(Check)", "Geom(AllCheck)"] {
        // Replace "Density" in "#P MP2/STO-3G Density Pop=(Full,NaturalOrbitals)".
        let input = text.replace("Density", geom);
        assert_eq!(
            read("checkpoint.com", input.as_bytes()),
            Err(Error::Unsupported(super::CHECKPOINT.into()))
        );
    }
}

#[test]
fn gaussian_mixes_cartesian_labels_dummy_atoms_and_signed_variables() {
    // Adapt the water fixture: use a dummy on +z to set the angle, and put one H
    // on the x axis in Cartesian form. Keep the fixture's R1 and A1 values.
    let bytes = fixture("cclib/g16_water_mp2.com");
    let text = std::str::from_utf8(&bytes).unwrap();
    let text = text
        .replace(
            "0 1\nO\nH 1 R1\nH 1 R1 2 A1",
            "0 1 0 1\nO1\nX1 O1 R1\nH1(Iso=2, Fragment=1) 0 0.99 0 0\nH2 O1 R1 X1 A1 H1 -D1",
        )
        .replace("A1=106.0", "Constants:\nA1=106.0\nD1 30.0");
    let file = read("dummy.com", text.as_bytes()).unwrap();
    let step = &one_job(&file.jobs, 3, 0, 1).steps[0];
    assert_eq!(step.frame.numbers, [8, 1, 1]);
    assert_eq!(step.frame.isotopes, [0, 2, 0]);
    assert_eq!(
        file.warnings,
        ["Read the first Gaussian charge and multiplicity pair"]
    );
    let xyz = &step.frame.positions_angstrom;
    close(distance(xyz[0], xyz[2]), 0.99);
    close(angle(xyz[2], xyz[0], [0., 0., 0.99]), 106.);
    close(dihedral(xyz[2], xyz[0], [0., 0., 0.99], xyz[1]), -30.);
    let ghost = text.replace("H2 O1", "H-Bq O1");
    assert_eq!(
        read("ghost.com", ghost.as_bytes()),
        Err(Error::Unsupported(super::GHOST.into()))
    );
}

#[test]
fn orca_dvb_reads_cartesian_geometry_between_keyword_blocks() {
    let file = open("cclib/orca60_dvb_gopt.inp");
    assert_eq!(file.format, Format::OrcaInput);
    // * xyz 0 1
    let job = one_job(&file.jobs, 20, 0, 1);
    // ! rks b3lyp sto-3g usesym opt
    assert_eq!(job.keywords, ["rks", "b3lyp", "sto-3g", "usesym", "opt"]);
    //  C       -1.4152533224   0.2302217854   0.0000000000
    assert_eq!(
        job.steps[0].frame.positions_angstrom[0],
        [-1.4152533224, 0.2302217854, 0.]
    );
}

#[test]
fn orca_water_reads_the_compact_xyz_header() {
    let file = open("cclib/orca60_water_mp2.inp");
    // *xyz 0 1
    let job = one_job(&file.jobs, 3, 0, 1);
    let step = &job.steps[0];
    assert_eq!(step.frame.numbers, [8, 1, 1]);
    // H      1.183168    1.256816   -0.238835
    assert_eq!(
        step.frame.positions_angstrom[2],
        [1.183168, 1.256816, -0.238835]
    );
}

#[test]
fn orca_new_jobs_keep_the_first_jobs_geometry_keywords_and_units() {
    let bytes = fixture("cclib/orca60_water_mp2.inp");
    let text = std::str::from_utf8(&bytes).expect("ORCA water fixture is UTF-8");
    let mut expected = open("cclib/orca60_water_mp2.inp");
    expected
        .warnings
        .push("Read the geometry of the first job".into());
    for next in [
        "! HF Bohrs\n*xyz 0 1\nHe 1 0 0\n*\n",
        "%coords\nUnits Bohrs\nCoords\nHe 1 0 0\nend\nend\n",
        "%paras\nB1 = 1.0, 2.0, 3\nend\n*xyzfile 0 1 absent.xyz\n",
    ] {
        // Keep "!  MP2 sto-3g", "*xyz 0 1", and
        // "O      1.784914    1.262422    0.511985" from the first job.
        let linked = format!("{text}\n$NeW_JoB # a separate calculation\n{next}");
        assert_eq!(
            read("linked.inp", linked.as_bytes()).expect("first ORCA job"),
            expected
        );
    }
}

#[test]
fn orca_parameterized_scan_reports_unsupported_geometry() {
    let name = "cclib/orca60_dvb_scan_unrelaxed.inp";
    let bytes = fixture(name);
    // %paras
    // C                  1    0    0       {B1}                 0.0                   0.0
    let error = Error::Unsupported("Parameterized coordinates (%paras) are not read".into());
    assert_eq!(read(name, &bytes), Err(error.clone()));
    // Drop the %paras block from the same fixture, leaving the references themselves.
    let text = std::str::from_utf8(&bytes).unwrap();
    let (before, after) = text.split_once("%paras").unwrap();
    let (_, after) = after.split_once("\nend\n").unwrap();
    assert_eq!(
        read("references.inp", format!("{before}{after}").as_bytes()),
        Err(error)
    );
}

#[test]
fn orca_xyzfile_reports_the_referenced_path() {
    // Replace the water fixture's embedded XYZ block with an external XYZ reference.
    let bytes = fixture("cclib/orca60_water_mp2.inp");
    let text = std::str::from_utf8(&bytes).unwrap();
    let (before, _) = text.split_once("*xyz").unwrap();
    let external = format!("{before}*xyzfile 0 1 \"water geometry.xyz\"\n");
    assert_eq!(
        read("external.inp", external.as_bytes()),
        Err(Error::Unsupported(
            "The geometry is in water geometry.xyz; open that file".into()
        ))
    );
}

#[test]
fn orca_z_matrices_match_in_both_notations() {
    // Re-express the Gaussian water fixture's R1=0.99 and A1=106.0
    // in the two ORCA internal-coordinate notations.
    let bytes = fixture("cclib/g16_water_mp2.com");
    let text = std::str::from_utf8(&bytes).unwrap();
    let (_, vars) = text.split_once("R1=").unwrap();
    let (r, angle_value) = vars.split_once("\nA1=").unwrap();
    let angle_value = angle_value.trim();
    let internal =
        format!("*internal 0 1\nO 0 0 0 0 0 0\nH 1 0 0 {r} 0 0\nH 1 2 0 {r} {angle_value} 0\n*");
    let gzmt = format!("* gzmt 0 1\nO\nH 1 {r}\nH 1 {r} 2 {angle_value}\n*");
    let internal = read("water.inp", internal.as_bytes()).unwrap();
    let gzmt = read("water.inp", gzmt.as_bytes()).unwrap();
    assert_eq!(internal, gzmt);
    let gaussian = open("cclib/g16_water_mp2.com");
    assert_eq!(
        internal.jobs[0].steps[0].frame,
        gaussian.jobs[0].steps[0].frame
    );
}

#[test]
fn orca_coords_converts_bohr_and_skips_atom_basis_and_mass_overrides() {
    // Re-express the ORCA water fixture's geometry in %coords and bohr,
    // adding fragment labels, per-atom basis overrides and one mass override.
    let source = open("cclib/orca60_water_mp2.inp");
    let frame = &source.jobs[0].steps[0].frame;
    let rows = frame
        .positions_angstrom
        .iter()
        .zip(["O(1)", "H(1)", "H(1)"])
        .enumerate()
        .map(|(i, (xyz, atom))| {
            let [x, y, z] = xyz.map(|v| v / ANGSTROM_PER_BOHR);
            let suffix = match i {
                0 => " NewGTO \"STO-3G\" end",
                1 => " M = 2.014\nNewGTO\n\"STO-3G\"\nend",
                _ => "",
            };
            format!("{atom} {x} {y} {z}{suffix}\n")
        })
        .collect::<String>();
    let block = format!(
        "! MP2 STO-3G\n%coords\nCTyp xyz\nCharge 0\nMult 1\nUnits Bohrs\nCoords\n{rows}end\nend\n"
    );
    let file = read("coords.inp", block.as_bytes()).unwrap();
    let actual = &one_job(&file.jobs, 3, 0, 1).steps[0].frame;
    for (a, b) in actual
        .positions_angstrom
        .iter()
        .flatten()
        .zip(frame.positions_angstrom.iter().flatten())
    {
        close(*a, *b);
    }
    assert!(actual.masses_amu.is_empty());
    assert_eq!(
        file.warnings,
        ["ORCA atom mass overrides (M =) were ignored"]
    );

    // Keep the same converted atom rows and specify bohr through the keyword instead.
    let star = format!("! MP2 STO-3G Bohrs\n*xyz 0 1\n{rows}*\n");
    assert_eq!(
        read("bohrs.inp", star.as_bytes()).unwrap().jobs[0].steps[0].frame,
        *actual
    );
    let ghost = block.replace("H(1)", "H:");
    assert_eq!(
        read("ghost.inp", ghost.as_bytes()),
        Err(Error::Unsupported(super::GHOST.into()))
    );
}

#[test]
fn gamess_nh3_reads_c1_geometry_with_default_control_values() {
    let file = open("cclib/gamess_nh3_ts_ir.inp");
    assert_eq!(file.format, Format::GamessInput);
    let job = one_job(&file.jobs, 4, 0, 1);
    let step = &job.steps[0];
    assert_eq!(step.frame.numbers, [7, 1, 1, 1]);
    // H    1.0  -0.4882960784   0.8457536168   0.0000000000
    assert_eq!(
        step.frame.positions_angstrom[1],
        [-0.4882960784, 0.8457536168, 0.]
    );
}

#[test]
fn gamess_carbon_reads_prinaxis_cartesian_rows_as_given() {
    let name = "cclib/gamess_C_bigbasis.inp";
    // "$CONTRL COORD=PRINAXIS UNITS=ANGS"; "C1".
    let file = open(name);
    let step = &one_job(&file.jobs, 1, 0, 1).steps[0];
    // C           6.0  0.0000000000   0.0000000000   0.0000000000
    assert_eq!(step.frame.numbers, [6]);
    assert_eq!(step.frame.positions_angstrom, [[0., 0., 0.]]);
}

#[test]
fn gamess_group_cards_are_active_only_with_a_dollar_in_column_two() {
    let bytes = fixture("cclib/gamess_nh3_ts_ir.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS ammonia fixture is UTF-8");
    // Synthetic inactive cards precede the fixture's active " $CONTRL ... $END".
    // Its four C1 atoms include "H 1.0 -0.4882960784 0.8457536168 0.0000000000".
    for prefix in ["", "--", "  ", "text ", "\t"] {
        let input = format!("{prefix}$CONTRL COORD=FRAGONLY ICHARG=8 $END\n{text}");
        let file = read("inactive-control.inp", input.as_bytes())
            .expect("GAMESS ignores group cards outside column two");
        let step = &one_job(&file.jobs, 4, 0, 1).steps[0];
        assert_eq!(step.frame.numbers, [7, 1, 1, 1]);
        assert_eq!(
            step.frame.positions_angstrom[1],
            [-0.4882960784, 0.8457536168, 0.]
        );

        // A synthetic inactive DATA group must not hide the fixture's C1 geometry.
        let input = format!("{prefix}$DATA\ninactive geometry\nC2\n $END\n{text}");
        let file = read("inactive-data.inp", input.as_bytes())
            .expect("GAMESS ignores inactive DATA cards");
        one_job(&file.jobs, 4, 0, 1);
    }
}

#[test]
fn gamess_unique_consumes_named_acct_basis_as_part_of_the_carbon_atom() {
    let bytes = fixture("cclib/gamess_C_bigbasis.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS carbon fixture is UTF-8");
    // Synthetic: remove " $BASIS GBASIS=ACCQC $END", use COORD=UNIQUE,
    // and append ACCT to "C 6.0 0.0000000000 0.0000000000 0.0000000000".
    let (before, after) = text
        .rsplit_once(" $END")
        .expect("fixture ends its DATA group");
    let input = format!("{before}ACCT 6 1.0 1.0 1.0\n\n $END{after}")
        .replace("COORD=PRINAXIS", "COORD=UNIQUE")
        .replace(" $BASIS  GBASIS=ACCQC $END\n", "");
    let file = read("inline-acct.inp", input.as_bytes()).expect("inline ACCT basis");
    let step = &one_job(&file.jobs, 1, 0, 1).steps[0];
    assert_eq!(step.frame.numbers, [6]);
    assert_eq!(step.frame.positions_angstrom, [[0., 0., 0.]]);
}

#[test]
fn gamess_unique_skips_builtin_basis_and_numbered_primitive_cards() {
    let bytes = fixture("cclib/gamess_C_bigbasis.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS carbon fixture is UTF-8");
    let (control, _) = text.split_once("$DATA").expect("fixture has $DATA");
    // "$CONTRL COORD=PRINAXIS UNITS=ANGS" and
    // "C           6.0  0.0000000000   0.0000000000   0.0000000000".
    // Synthetic: adapt the fixture's C1 Cartesian input to H2 with inline basis
    // cards and no active BASIS group. The inactive card must not select a basis.
    for coord in ["UNIQUE", "CART"] {
        for builtin in ["STO 3", "N21 3", "N31 6", "N311 6", "ACCT 6"] {
            let input = format!(
                "{}\n$BASIS GBASIS=N31 NGAUSS=6 $END\n $DATA\nH2\nC1\n\
         H1 1.0 0.0 0.0 0.0\n{builtin} 1.0 1.0 1.0\n\n\
         H2 1.0 0.0 0.0 1.1\nS 3\n\
         1 3.42525091 0.15432897 0.1 0.2\n\
         2 0.62391373 0.53532814 0.2 0.3\n\
         3 0.16885540 0.44463454 0.3 0.4\n\n $END\n",
                control
                    .replace("COORD=PRINAXIS", &format!("cOoRd = {coord}"))
                    .replace(" $BASIS  GBASIS=ACCQC $END\n", "")
            );
            let file = read("inline-basis.inp", input.as_bytes()).expect("inline GAMESS basis");
            let step = &one_job(&file.jobs, 2, 0, 1).steps[0];
            assert_eq!(step.frame.numbers, [1, 1]);
            assert_eq!(step.frame.positions_angstrom, [[0., 0., 0.], [0., 0., 1.1]]);
        }
    }
}

#[test]
fn gamess_unique_rejects_a_shell_missing_its_numbered_primitives() {
    let bytes = fixture("cclib/gamess_C_bigbasis.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS carbon fixture is UTF-8");
    // Synthetic: append a malformed shell after the fixture's "C 6.0 0 0 0" row
    // and remove its active BASIS group so basis cards belong to the atom.
    let (before, _) = text
        .rsplit_once("$END")
        .expect("fixture ends its DATA group");
    let input = format!(
        "{}S 3 1.0 1.0 1.0\n $END\n",
        before
            .replace("COORD=PRINAXIS", "COORD=UNIQUE MULT=2")
            .replace(" $BASIS  GBASIS=ACCQC $END\n", "")
    );
    assert!(matches!(
        read("missing-primitives.inp", input.as_bytes()),
        Err(Error::Invalid(message)) if message.contains("GAMESS basis is missing primitive 1")
    ));
}

#[test]
fn gamess_merges_control_groups_and_ignores_bang_comments() {
    let bytes = fixture("cclib/gamess_nh3_ts_ir.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS ammonia fixture is UTF-8");
    // The fixture's "$CONTRL SCFTYP=RHF RUNTYP=HESSIAN AIMPAC=.t. $END"
    // leaves these geometry options at their defaults; supply repeated assignments.
    let input = format!(
        " $CONTRL COORD=ZMTMPC UNITS=BOHR ICHARG=8 MULT=1 $END\n\
         \x20$CONTRL COORD=UNIQUE UNITS=ANGS ICHARG=0 ICHARG=1 MULT=2\n\
         ! COORD=FRAGONLY UNITS=BOHR ICHARG=9 $END !\n\
         \x20$END\n{text}"
    );
    let file = read("control.inp", input.as_bytes()).expect("merged GAMESS control");
    let actual = &one_job(&file.jobs, 4, 1, 2).steps[0].frame;
    let expected = open("cclib/gamess_nh3_ts_ir.inp");
    assert_eq!(actual.numbers, [7, 1, 1, 1]);
    assert_eq!(
        actual.positions_angstrom,
        expected.jobs[0].steps[0].frame.positions_angstrom
    );
}

#[test]
fn gamess_paired_control_comments_preserve_following_assignments() {
    let bytes = fixture("cclib/gamess_nh3_ts_ir.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS ammonia fixture is UTF-8");
    // Add paired namelist comments to the fixture's "$CONTRL ... $END".
    // Both the inline and multiline comment contain fake group boundaries.
    let input = format!(
        " $CONTRL ! COORD=FRAGONLY $END ! COORD=UNIQUE ICHARG=0\n\
         ! a multiline comment\n$END $CONTRL COORD=ZMTMPC\n\
         ! ICHARG=1 MULT=2 UNITS=ANGS $END\n{text}"
    );
    let file = read("comments.inp", input.as_bytes()).expect("paired GAMESS comments");
    let actual = &one_job(&file.jobs, 4, 1, 2).steps[0].frame;
    let expected = open("cclib/gamess_nh3_ts_ir.inp");
    assert_eq!(
        actual.positions_angstrom,
        expected.jobs[0].steps[0].frame.positions_angstrom
    );
}

#[test]
fn gamess_data_titles_do_not_start_namelists_or_end_data() {
    let bytes = fixture("cclib/gamess_nh3_ts_ir.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS ammonia fixture is UTF-8");
    for title in [
        "$CONTRL COORD=FRAGONLY ! title",
        "$END is part of the title !",
    ] {
        // Replace "ammonia...at the planar transition state"; keep "C1" and
        // "N    7.0   0.0000000000   0.0000000000   0.0000000000" unchanged.
        let input = text.replace("ammonia...at the planar transition state", title);
        let file = read("title.inp", input.as_bytes()).expect("GAMESS DATA title");
        let job = one_job(&file.jobs, 4, 0, 1);
        // Preserve the reader's existing line-comment treatment in card groups.
        assert_eq!(
            job.steps[0].label.as_deref(),
            title.split('!').next().map(str::trim)
        );
    }
}

#[test]
fn gamess_c1_zmt_uses_the_existing_variable_z_matrix_converter() {
    let bytes = fixture("cclib/gamess_nh3_ts_ir.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS ammonia fixture is UTF-8");
    let (control, _) = text.split_once("$DATA").expect("fixture has $DATA");
    let water = fixture("cclib/g16_water_mp2.com");
    let water = std::str::from_utf8(&water).expect("Gaussian water fixture is UTF-8");
    // Reuse "O\nH 1 R1\nH 1 R1 2 A1", "R1=0.99", and "A1=106.0".
    let (_, rows) = water
        .split_once("0 1\n")
        .expect("fixture has water geometry");
    let input = format!(
        "{}\n $DATA\nWater\nC1\n{rows}\n $END\n",
        control.replace("$CONTRL", "$CONTRL COORD=ZMT")
    );
    let file = read("zmt.inp", input.as_bytes()).expect("C1 GAMESS Z-matrix");
    let actual = &one_job(&file.jobs, 3, 0, 1).steps[0].frame;
    let expected = open("cclib/g16_water_mp2.com");
    assert_eq!(*actual, expected.jobs[0].steps[0].frame);
}

fn gamess_dvb_zmt_with_flag(flag: &str) -> String {
    let bytes = fixture("cclib/gamess_nh3_ts_ir.inp");
    let control = std::str::from_utf8(&bytes).expect("GAMESS ammonia fixture is UTF-8");
    let (control, _) = control.split_once("$DATA").expect("fixture has $DATA");
    let bytes = fixture("cclib/g16_dvb_gopt.gjf");
    let text = std::str::from_utf8(&bytes).expect("Gaussian DVB fixture is UTF-8");
    let (_, geometry) = text.split_once("0 1\n").expect("fixture has DVB geometry");
    let (rows, variables) = geometry
        .split_once("\n\n")
        .expect("fixture separates atoms and variables");
    // Synthetic GAMESS C1 ZMT input reuses all 20 Gaussian fixture atoms and
    // variables, adding a flag only to the rows with three reference atoms.
    let rows = rows
        .lines()
        .map(|row| {
            if row.split_whitespace().count() == 7 {
                format!("{row} {flag}\n")
            } else {
                format!("{row}\n")
            }
        })
        .collect::<String>();
    format!(
        "{}\n $DATA\nDVB with a Z-matrix flag\nC1\n{rows}\n{variables}\n $END\n",
        control.replace("$CONTRL", "$CONTRL COORD=ZMT")
    )
}

#[test]
fn gamess_zmt_accepts_an_optional_zero_dihedral_flag() {
    let input = gamess_dvb_zmt_with_flag("0");
    let file = read("zmt-flag-zero.inp", input.as_bytes()).expect("GAMESS ZMT flag 0");
    let step = &one_job(&file.jobs, 20, 0, 1).steps[0];
    let expected = open("cclib/g16_dvb_gopt.gjf");
    assert_eq!(step.frame, expected.jobs[0].steps[0].frame);
    // Fixture evidence: "B1 1.39155762", "A1 120.26061806", "D1 0.00000000".
    let xyz = &step.frame.positions_angstrom;
    close(distance(xyz[0], xyz[1]), 1.39155762);
    close(angle(xyz[2], xyz[1], xyz[0]), 120.26061806);
    close(dihedral(xyz[3], xyz[2], xyz[1], xyz[0]), 0.);
}

#[test]
fn gamess_zmt_rejects_second_angle_flags_with_a_specific_unsupported_error() {
    for flag in ["1", "+1", "-1"] {
        let input = gamess_dvb_zmt_with_flag(flag);
        assert_eq!(
            read("zmt-second-angle.inp", input.as_bytes()),
            Err(Error::Unsupported(
                "GAMESS Z-matrix second-angle coordinates (flag +1 or -1) are not supported; use dihedral coordinates with flag 0".into()
            ))
        );
    }
}

#[test]
fn gamess_unsupported_coordinate_modes_have_clear_errors() {
    let bytes = fixture("cclib/gamess_C_bigbasis.inp");
    let text = std::str::from_utf8(&bytes).expect("GAMESS carbon fixture is UTF-8");
    for coord in ["ZMTMPC", "FRAGONLY"] {
        // Replace "COORD=PRINAXIS" while keeping the fixture's "C1" symmetry.
        let input = text.replace("COORD=PRINAXIS", &format!("COORD={coord}"));
        assert_eq!(
            read("unsupported.inp", input.as_bytes()),
            Err(Error::Unsupported(format!(
                "GAMESS COORD={coord} is not supported; save Cartesian coordinates with COORD=UNIQUE or CART"
            )))
        );
    }
}

#[test]
fn gamess_dvb_rejects_non_c1_symmetry() {
    let name = "cclib/gamess_dvb_gopt_a.inp";
    // cnh 2
    assert_eq!(read(name, &fixture(name)), Err(Error::Unsupported(
        "GAMESS input with symmetry C2H lists only the unique atoms; save it with C1 symmetry to open it".into()
    )));
}

#[test]
fn gamess_cart_reads_bohr_and_explicit_charge_and_spin() {
    // Re-express the NH3 fixture's C1 geometry in bohr, use COORD=CART,
    // and supply an explicit charge and multiplicity.
    let file = open("cclib/gamess_nh3_ts_ir.inp");
    let xyz = &file.jobs[0].steps[0].frame.positions_angstrom;
    let rows = xyz
        .iter()
        .zip([7, 1, 1, 1])
        .map(|(xyz, nuclear)| {
            let [x, y, z] = xyz.map(|v| v / ANGSTROM_PER_BOHR);
            format!("atom {nuclear} {x} {y} {z}\n")
        })
        .collect::<String>();
    let input = format!(
        " $contrl iChArG=1 MuLt = 2 units=bohr coord=cart $end\n $basis GBASIS=AM1 $end\n $data\nNH3\nc1\n{rows} $end"
    );
    let actual = read("cart.inp", input.as_bytes()).expect("GAMESS Cartesian coordinates in bohr");
    let frame = &one_job(&actual.jobs, 4, 1, 2).steps[0].frame;
    assert_eq!(frame.numbers, [7, 1, 1, 1]);
    for (a, b) in frame
        .positions_angstrom
        .iter()
        .flatten()
        .zip(xyz.iter().flatten())
    {
        close(*a, *b);
    }
}
