use super::*;
use reshiki_qcdata::{
    ANGSTROM_PER_BOHR, Provenance, Settings as ResultSettings, Source, Status, Task, Vibrations,
};
use std::{collections::BTreeMap, path::PathBuf};

mod hessian;
mod keywords;
mod roundtrip;
mod structure;
mod validation;

const GENERATOR: &str = "ReShiki 0.17.1";

#[test]
fn parse_xyz_keeps_only_the_first_frame_and_respects_extended_columns() {
    let frame = parse_xyz(
        "2\nProperties=charge:R:1:pos:R:3:species:S:1\n0.2 1 -2 3 O\n-0.2 4 5 -6 H\n\ninvalid next frame",
    )
    .unwrap();
    assert_eq!(frame.numbers, [8, 1]);
    assert_eq!(
        frame.positions_angstrom,
        [[1.0, -2.0, 3.0], [4.0, 5.0, -6.0]]
    );
    assert_eq!((frame.charge, frame.multiplicity), (0, 1));
    assert_eq!(parse_xyz("1\nname\nH 0 0 0\n").unwrap().numbers, [1]);
}

#[test]
fn parse_xyz_rejects_invalid_first_frames() {
    for (input, message) in [
        (
            "water",
            "The XYZ input needs an atom count on the first line.",
        ),
        ("1\n", "The XYZ input needs a comment line."),
        ("10001\n", "The XYZ input exceeds 10000 atoms."),
        (
            "1\nProperties=species:S:1\nH",
            "XYZ Properties needs species:S:1 and pos:R:3 columns.",
        ),
        (
            "1\n\nH NaN 0 0\n",
            "Invalid XYZ atom 1: expected an element and finite x y z coordinates.",
        ),
    ] {
        assert_eq!(parse_xyz(input).unwrap_err(), message);
    }
}

fn water() -> Frame {
    Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: vec![
            [0.0, 0.0, 0.0],
            [0.9572, 0.0, 0.0],
            [-0.2399872, 0.9266272, 0.0],
        ],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    }
}

fn shifted_water(dx: f64) -> Frame {
    let mut frame = water();
    for position in &mut frame.positions_angstrom[1..] {
        position[0] += dx;
    }
    frame
}

fn geometry(frame: &Frame) -> Geometry<'_> {
    Geometry {
        frame,
        reactant: None,
        product: None,
        result: None,
    }
}

fn settings(program: Program, job: Job) -> Settings {
    let mut settings =
        Settings::from_template(&builtin_templates()[0], program, 0, 1, "water".into());
    settings.job = job;
    // The first template's def2-SVP is unavailable in GAMESS built-in bases.
    if program == Program::Gamess {
        settings.basis = "6-31G(d)".into();
    }
    settings
}

fn hessian(frame: &Frame) -> Vec<f64> {
    let dimension = frame.numbers.len() * 3;
    (0..dimension)
        .flat_map(|row| (0..dimension).map(move |column| 0.1 / (1 + row.abs_diff(column)) as f64))
        .collect()
}

fn result(frame: &Frame) -> CalcResult {
    CalcResult {
        frame: frame.hash(),
        input_frame: None,
        task: Task::Hessian,
        endpoints: None,
        settings: ResultSettings::Imported { keywords: vec![] },
        provenance: Provenance {
            program: "xTB".into(),
            program_version: None,
            method: Some("GFN2-xTB".into()),
            basis: None,
            source: Source::Calculated {
                engine_revision: "test".into(),
            },
        },
        status: Status::Converged,
        energy_hartree: None,
        energy_terms: BTreeMap::new(),
        gradient_hartree_per_bohr: vec![],
        hessian_hartree_per_bohr2: hessian(frame),
        charges: vec![],
        dipole_e_bohr: None,
        bond_orders: vec![],
        orbitals: None,
        vibrations: Some(Vibrations {
            frequencies_cm1: vec![-412.3, 1600.0, 3700.0],
            ir_intensities_km_mol: vec![],
            modes: vec![],
            reduced_masses_amu: vec![],
            masses_amu: vec![],
        }),
        thermo: None,
        trajectory: None,
        keep_full: false,
        omitted_bytes: None,
        run: None,
    }
}

fn key(program: Program) -> &'static str {
    match program {
        Program::Gaussian => "gaussian",
        Program::Orca => "orca",
        Program::Gamess => "gamess",
        Program::Xyz => "xyz",
    }
}

fn job_key(job: Job) -> String {
    serde_json::from_value(serde_json::to_value(job).unwrap()).unwrap()
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/qc_input")
        .join(name)
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).unwrap()
}

fn golden(program: Program, scenario: &str, stem: &str, output: &Output) {
    if program == Program::Orca {
        for name in output.main.text.split('"').skip(1).step_by(2) {
            assert!(
                output.companions.iter().any(|file| file.name == name),
                "Missing ORCA companion file {name}"
            );
        }
    }
    for file in std::iter::once(&output.main).chain(&output.companions) {
        let suffix = file.name.strip_prefix(stem).unwrap();
        let path = fixture_path(&format!("{scenario}{suffix}"));
        if std::env::var("RESHIKI_BLESS").as_deref() == Ok("1") {
            std::fs::write(&path, &file.text).unwrap();
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("{}: {error}; run with RESHIKI_BLESS=1", path.display())
        });
        assert_eq!(file.text, expected, "{}", path.display());
        assert!(file.text.ends_with('\n'), "{}", file.name);
        assert!(!file.text.ends_with("\n\n\n"), "{}", file.name);
    }
}

#[test]
fn water_goldens_cover_every_program_and_available_job() {
    let frame = water();
    let reactant = shifted_water(0.05);
    let product = shifted_water(-0.05);
    let result = result(&frame);
    result.validate(&frame).unwrap();
    for program in Program::ALL {
        let jobs = if program == Program::Xyz {
            &[Job::SinglePoint][..]
        } else {
            program.jobs()
        };
        for &job in jobs {
            let settings = settings(program, job);
            let geometry = Geometry {
                frame: &frame,
                reactant: Some(&reactant),
                product: Some(&product),
                result: job.is_transition_state().then_some(&result),
            };
            let output = write(&settings, &geometry, "water", GENERATOR).unwrap();
            golden(
                program,
                &format!("water_{}_{}", key(program), job_key(job)),
                "water",
                &output,
            );
            let sections = match job {
                Job::Qst2 => 2,
                Job::Qst3 => 3,
                _ => 1,
            };
            structure::check(program, &output.main.text, 3, sections);
        }
    }
}

#[test]
#[should_panic(expected = "Missing ORCA companion file water_xtb.hess")]
fn orca_golden_rejects_missing_companions() {
    let frame = water();
    let result = result(&frame);
    let geometry = Geometry {
        result: Some(&result),
        ..geometry(&frame)
    };
    let mut output = write(
        &settings(Program::Orca, Job::TransitionState),
        &geometry,
        "water",
        GENERATOR,
    )
    .unwrap();
    output.companions.clear();
    golden(
        Program::Orca,
        "water_orca_transition-state",
        "water",
        &output,
    );
}

#[test]
fn orca_without_a_hessian_goldens() {
    let frame = water();
    for job in [Job::TransitionState, Job::Irc] {
        let output = write(
            &settings(Program::Orca, job),
            &geometry(&frame),
            "water",
            GENERATOR,
        )
        .unwrap();
        assert!(output.companions.is_empty());
        golden(
            Program::Orca,
            &format!("water_orca_{}_no_hessian", job_key(job)),
            "water",
            &output,
        );
        structure::check(Program::Orca, &output.main.text, 3, 1);
    }
}

#[test]
fn composite_and_open_shell_goldens() {
    let frame = water();
    let settings =
        Settings::from_template(&builtin_templates()[1], Program::Orca, 0, 1, "water".into());
    let output = write(&settings, &geometry(&frame), "water", GENERATOR).unwrap();
    golden(Program::Orca, "water_orca_r2scan-3c", "water", &output);
    structure::check(Program::Orca, &output.main.text, 3, 1);

    let mut radical = frame;
    radical.numbers.pop();
    radical.positions_angstrom.pop();
    radical.multiplicity = 2;
    for program in [Program::Gaussian, Program::Orca, Program::Gamess] {
        let mut settings = self::settings(program, Job::SinglePoint);
        settings.multiplicity = 2;
        settings.title = "OH radical".into();
        let output = write(&settings, &geometry(&radical), "oh", GENERATOR).unwrap();
        golden(program, &format!("oh_{}", key(program)), "oh", &output);
        structure::check(program, &output.main.text, 2, 1);
        let keyword = match program {
            Program::Gaussian => "UB3LYP",
            Program::Orca => "UKS B3LYP",
            Program::Gamess => "SCFTYP=UHF",
            Program::Xyz => unreachable!(),
        };
        assert!(output.main.text.contains(keyword));
    }
}
