//! Shared test helpers; each reader's tests live in their own module below.
use super::*;

/// The bytes of `tests/fixtures/qc/<name>`.
pub(super) fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/qc")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `name` read through [`read`], which every test goes through.
pub(super) fn open(name: &str) -> QcFile {
    read(name, &fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn detects_formats_by_extension_and_content() {
    assert_eq!(detect("a.xyz", ""), Some(Format::Xyz));
    assert_eq!(detect("a.FCHK", ""), Some(Format::Fchk));
    assert_eq!(
        detect("a.out", " Entering Gaussian System, Link 0=g16"),
        Some(Format::GaussianLog)
    );
    assert_eq!(
        detect("a.out", "  * O   R   C   A *"),
        Some(Format::OrcaOutput)
    );
    assert_eq!(
        detect("a.inp", " $CONTRL RUNTYP=OPTIMIZE $END\n $DATA\n"),
        Some(Format::GamessInput)
    );
    assert_eq!(
        detect("a.inp", "! B3LYP def2-SVP\n* xyz 0 1\n"),
        Some(Format::OrcaInput)
    );
    assert_eq!(
        detect(
            "a.inp",
            "%mem=1GB\n#p opt b3lyp/6-31g\n\nwater\n\n0 1\nO 0 0 0\n"
        ),
        Some(Format::GaussianInput)
    );
    assert_eq!(
        detect("a.log", " GAMESS VERSION = 30 SEP 2018 (R3)\n $DATA\n"),
        None
    );
    assert_eq!(detect("a.txt", "hello"), None);
    // Markdown headings or shell comments are not a Gaussian route.
    assert_eq!(detect("notes.log", "# Notes\n\nSome text\n"), None);
}

#[test]
fn shell_diagnostics_are_not_orca_inputs() {
    assert_eq!(detect("build.log", "! disk full\n"), None);
}

#[test]
fn orca_detection_requires_a_geometry_directive() {
    for directive in [
        "* xyz",
        "*xyz",
        "* xyzfile",
        "*xyzfile",
        "*int",
        "* int",
        "*gzmt",
        "* gzmt",
        "* internal",
        "%coords",
    ] {
        assert_eq!(
            detect("input", &format!("{directive}\n")),
            Some(Format::OrcaInput),
            "{directive}"
        );
    }
    assert_eq!(detect("notes.out", "* xyzzy\n%coords_backup\n"), None);
}

#[test]
fn gaussian_detection_requires_a_title_and_atom_after_the_route() {
    assert_eq!(detect("notes.out", "# Benchmark\n\n200 1\n"), None);
    assert_eq!(
        detect("notes.out", "# Benchmark\n\nTitle\n\n0 1\noutput\n"),
        None
    );
    // The water fixture has "#P MP2/STO-3G Density Pop=(Full,NaturalOrbitals)",
    // then "Water", "0 1" and "O", separated into route, title and geometry.
    let bytes = fixture("cclib/g16_water_mp2.com");
    let input = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(detect("water.inp", input), Some(Format::GaussianInput));
    // Use a numbered atom in place of the fixture's Z-matrix symbol.
    let numbered = input.replace("\nO\n", "\n8\n");
    assert_eq!(detect("water.inp", &numbered), Some(Format::GaussianInput));
}

#[test]
fn detects_output_banners_after_a_long_preamble() {
    // Prepend 100 KiB to the ORCA fixture before its "* O   R   C   A *" banner.
    let bytes = fixture("cclib/orca50_water_mp2.out");
    let output = std::str::from_utf8(&bytes).unwrap();
    let text = " ".repeat(100 * 1024) + "\n" + output;
    assert_eq!(detect("run.out", &text), Some(Format::OrcaOutput));
}

#[test]
fn banner_mentions_inside_a_line_do_not_identify_the_format() {
    for mention in [
        "Entering Gaussian System",
        "Gaussian, Inc.",
        "* O   R   C   A *",
        "[Molden Format]",
    ] {
        let text = format!("See the {mention} banner in the manual\n");
        assert_eq!(detect("notes.log", &text), None, "{mention}");
    }
    assert_eq!(
        detect("run.inp", "! GAMESS VERSION was updated\n $DATA\n"),
        Some(Format::GamessInput)
    );
}

#[test]
fn molden_sniffing_uses_the_first_nonempty_line() {
    // Replace the Gaussian fixture's "Water" title with a Molden heading in memory.
    let source = String::from_utf8(fixture("cclib/g16_water_mp2.log")).unwrap();
    let titled = source.replace("\n Water\n", "\n [Molden Format]\n");
    // Entering Gaussian System, Link 0=g16_main
    assert_eq!(detect("water.log", &titled), Some(Format::GaussianLog));

    // The real Molden fixture begins with "[Molden Format]". Change its case
    // and prepend blank lines in memory.
    let source = String::from_utf8(fixture("cclib/molden5.7_water_mp2.molden")).unwrap();
    let first = source.replacen("[Molden Format]", "\n  \n[mOLDEN fORMAT]", 1);
    assert_eq!(detect("water", &first), Some(Format::Molden));
    assert_eq!(detect("water", &format!("preamble\n{source}")), None);
}

#[test]
fn gamess_sniffing_uses_a_column_two_data_card() {
    // Add a comment in memory before the ORCA fixture's "!  MP2 sto-3g" route.
    let source = String::from_utf8(fixture("cclib/orca60_water_mp2.inp")).unwrap();
    let commented = format!("# A GAMESS input uses $DATA\n{source}");
    // *xyz 0 1
    assert_eq!(detect("water.inp", &commented), Some(Format::OrcaInput));

    // Change the fixture's " $DATA" card in memory.
    let source = String::from_utf8(fixture("cclib/gamess_C_bigbasis.inp")).unwrap();
    let lowercase = source.replace(" $DATA\n", " $data\n");
    assert_eq!(detect("carbon.inp", &lowercase), Some(Format::GamessInput));
    for card in ["$DATA", "  $DATA", "--$DATA", " $DATABASE"] {
        let modified = source.replace(" $DATA\n", &format!("{card}\n"));
        assert_eq!(detect("carbon.inp", &modified), None, "{card}");
    }
}

#[test]
fn detects_orca_molden_double_suffix_without_a_content_probe() {
    // Give the water Molden fixture another filename in memory. Remove its
    // "[Molden Format]" heading from the probe to isolate filename detection.
    let source = String::from_utf8(fixture("cclib/molden5.7_water_mp2.molden")).unwrap();
    let (_, body) = source.split_once('\n').unwrap();
    assert_eq!(detect("water.MOLDEN.INPUT", body), Some(Format::Molden));
    let file = read("water.MOLDEN.INPUT", source.as_bytes()).unwrap();
    // H      3  1    -0.272880     0.951649     0.000000
    assert_eq!(file.jobs[0].steps[0].frame.numbers, [8, 1, 1]);
}

#[test]
fn oversized_hessian_keeps_the_energy_and_geometry() {
    // Build a synthetic frequency result in memory one atom above the Hessian limit.
    let n = MAX_HESSIAN_ATOMS + 1;
    let frame = frame(
        &vec![(2, 0); n],
        (0..n).map(|i| [i as f64, 0.0, 0.0]).collect(),
        0,
        1,
    )
    .unwrap();
    let hash = frame.hash();
    let mut job = Job {
        index: 0,
        kind: JobKind::Frequencies,
        method: None,
        basis: None,
        keywords: Vec::new(),
        completion: Some(Completion::Normal),
        steps: vec![Step {
            frame,
            energy_hartree: Some(-500.0),
            label: None,
        }],
        result: None,
    };
    let origin = Origin {
        format: Format::GaussianLog,
        file_name: "large.log",
        sha256: "0".repeat(64),
    };
    let program = QcProgram {
        name: "Gaussian".into(),
        version: None,
    };
    let mut result = origin.result(
        &job.steps[0].frame,
        Task::Hessian,
        Status::Converged,
        &program,
        &job,
    );
    result.energy_hartree = Some(-500.0);
    result.hessian_hartree_per_bohr2 = vec![0.0; (3 * n).pow(2)];
    result.vibrations = Some(reshiki_qcdata::Vibrations {
        frequencies_cm1: vec![100.0],
        ir_intensities_km_mol: Vec::new(),
        modes: Vec::new(),
        reduced_masses_amu: Vec::new(),
        masses_amu: Vec::new(),
    });
    job.result = Some(result);
    let file = finish(QcFile {
        format: origin.format,
        program: Some(program.name),
        program_version: None,
        jobs: vec![job],
        warnings: Vec::new(),
    })
    .unwrap();
    let job = &file.jobs[0];
    assert_eq!(job.kind, JobKind::Frequencies);
    assert_eq!(job.result_step().unwrap().frame.numbers.len(), n);
    let result = job.result.as_ref().unwrap();
    assert_eq!(result.frame, hash);
    assert_eq!(result.task, Task::Energy);
    assert_eq!(result.energy_hartree, Some(-500.0));
    assert!(result.hessian_hartree_per_bohr2.is_empty());
    assert!(result.vibrations.is_none());
    assert_eq!(
        file.warnings,
        [format!(
            "Job 1: frequencies are kept for at most {MAX_HESSIAN_ATOMS} atoms"
        )]
    );
}

#[test]
fn elements_and_isotopes() {
    assert_eq!(element("C"), Some((6, 0)));
    assert_eq!(element("cl"), Some((17, 0)));
    assert_eq!(element("D"), Some((1, 2)));
    assert_eq!(element("Og"), Some((118, 0)));
    assert_eq!(element("Xx"), None);
    assert_eq!(lowest_multiplicity(&[(8, 0), (1, 0)], 0), 2);
    assert_eq!(lowest_multiplicity(&[(8, 0), (1, 0)], -1), 1);
}
