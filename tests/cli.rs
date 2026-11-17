//! `reshiki --cli` through the real binary: the stdout/stderr and exit-code
//! contract.
//!
//! Every run has an empty data folder, a temporary working folder and a
//! watchdog, and inherits RESHIKI_INCHI_HELPER as CI sets it
//! (.github/workflows/checks.yml).
use crate::headless;

use reshiki::assistant::Proposal;
use std::{fs, path::Path, process::Output, time::Duration};
use tempfile::TempDir;

/// Long enough for a debug build's first chemistry engine call on CI.
const CONVERT: Duration = Duration::from_secs(120);
/// Usage errors and discovery commands exit before any operation work.
const USAGE: Duration = Duration::from_secs(30);

fn cli(dir: &Path, args: &[&str]) -> Output {
    headless::run_cli(dir, args, b"", CONVERT)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Asserts a usage error: exit 2, nothing on stdout, and no file created.
fn assert_usage(dir: &TempDir, args: &[&str]) -> String {
    let before = files(dir);
    let output = headless::run_cli(dir.path(), args, b"", USAGE);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
    assert!(
        output.stdout.is_empty(),
        "{args:?}: {}",
        text(&output.stdout)
    );
    assert_eq!(files(dir), before, "{args:?} created a file");
    stderr
}

fn files(dir: &TempDir) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .expect("list the folder")
        .map(|entry| entry.expect("entry").file_name().to_string_lossy().into())
        .collect();
    names.sort();
    names
}

/// The working folder with `eth.mol`, made by `convert --smiles CCO -o
/// eth.mol`, which leaves stdout empty.
fn with_ethanol() -> TempDir {
    let dir = tempfile::tempdir().expect("working folder");
    let output = cli(dir.path(), &["convert", "--smiles", "CCO", "-o", "eth.mol"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty(), "{}", text(&output.stdout));
    let molfile = fs::read_to_string(dir.path().join("eth.mol")).expect("eth.mol");
    assert!(molfile.contains("M  END"), "{molfile}");
    dir
}

#[test]
fn discovery_prints_one_json_line_quickly() {
    let dir = tempfile::tempdir().expect("working folder");
    for args in [
        &["tools"][..],
        &["help", "--json"][..],
        &["schema", "proposal"][..],
    ] {
        let output = headless::run_cli(dir.path(), args, b"", USAGE);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            text(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "{args:?}: {:?}", output.stderr);
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 JSON");
        let line = stdout.strip_suffix('\n').expect("one LF-terminated line");
        assert!(!line.contains('\n'), "{args:?}: {stdout}");
        let value: serde_json::Value = serde_json::from_str(line).expect("JSON line");
        assert_eq!(line, value.to_string(), "{args:?}: minified JSON");
        assert!(files(&dir).is_empty(), "{args:?} created a file");
    }
}

#[test]
fn convert_writes_a_file_or_stdout() {
    let dir = with_ethanol();
    let output = cli(dir.path(), &["convert", "eth.mol", "--to", "mol"]);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(text(&output.stdout).contains("M  END"));
    assert!(!stderr.contains("reshiki: error"), "{stderr}");
    assert_eq!(files(&dir), ["eth.mol"]);
}

#[test]
fn convert_reads_a_pipeline_from_stdin() {
    let dir = tempfile::tempdir().expect("working folder");
    let first = cli(dir.path(), &["convert", "--smiles", "CCO", "--to", "mol"]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let second = headless::run_cli(
        dir.path(),
        &["convert", "-", "--from", "mol", "-o", "eth.svg"],
        &first.stdout,
        CONVERT,
    );
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    assert!(second.stdout.is_empty(), "{}", text(&second.stdout));
    let svg = fs::read(dir.path().join("eth.svg")).expect("eth.svg");
    assert!(svg.starts_with(b"<svg"), "{}", text(&svg));
}

#[test]
fn convert_usage_errors_exit_2() {
    let dir = with_ethanol();
    assert_usage(&dir, &["convert", "eth.mol"]);
    let stdin = assert_usage(&dir, &["convert", "-"]);
    assert!(stdin.contains("needs --from"), "{stdin}");
    for output in ["x.rsk", "x.emf", "x.xyz"] {
        let stderr = assert_usage(&dir, &["convert", "eth.mol", "-o", output]);
        assert!(stderr.contains("unsupported output format"), "{stderr}");
    }
    assert_usage(&dir, &["convert", "eth.mol", "--pages", "--to", "png"]);
}

#[test]
fn an_existing_output_is_replaced_only_with_force() {
    let dir = with_ethanol();
    let kept = dir.path().join("kept.mol");
    fs::write(&kept, b"keep me").expect("existing output");
    let args = ["convert", "eth.mol", "-o", "kept.mol"];
    let output = cli(dir.path(), &args);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("exists; pass --force to replace"),
        "{stderr}"
    );
    assert_eq!(fs::read(&kept).expect("kept.mol"), b"keep me");
    assert_eq!(
        files(&dir),
        ["eth.mol", "kept.mol"],
        "no temporary file is left"
    );
    let output = cli(dir.path(), &[&args[..], &["--force"][..]].concat());
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(output.stdout.is_empty());
    let replaced = fs::read_to_string(&kept).expect("kept.mol");
    assert!(replaced.contains("M  END"), "{replaced}");
}

#[test]
fn unreadable_inputs_exit_1() {
    let dir = tempfile::tempdir().expect("working folder");
    fs::create_dir(dir.path().join("folder.mol")).expect("folder");
    for input in ["missing.mol", "folder.mol"] {
        let output = cli(dir.path(), &["convert", input, "--to", "mol"]);
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{input}: {stderr}");
        assert!(output.stdout.is_empty(), "{input}");
        assert!(
            stderr.starts_with("reshiki: error: cannot read"),
            "{stderr}"
        );
    }
}

#[test]
fn a_receipt_is_the_only_stdout_line() {
    let dir = tempfile::tempdir().expect("working folder");
    let output = cli(
        dir.path(),
        &["convert", "--smiles", "CCO", "-o", "eth.svg", "--receipt"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let line = stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\n'), "{stdout}");
    let receipt: serde_json::Value = serde_json::from_str(line).expect("receipt JSON");
    assert_eq!(receipt["receipt"]["format"], "svg");
    let size = fs::metadata(dir.path().join("eth.svg"))
        .expect("eth.svg")
        .len();
    assert_eq!(receipt["receipt"]["byte_len"], size);
    assert_eq!(receipt["api"]["stability"], "experimental");
}

#[test]
fn analyze_prints_one_json_line() {
    let dir = tempfile::tempdir().expect("working folder");
    let output = cli(dir.path(), &["analyze", "--smiles", "CCO"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let line = stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\n'), "{stdout}");
    let result: serde_json::Value = serde_json::from_str(line).expect("analyze JSON");
    assert_eq!(result["value"]["analysis"]["formula"], "C2H6O");
    assert_eq!(result["api"]["stability"], "experimental");
    let to = assert_usage(&dir, &["analyze", "--smiles", "CCO", "--to", "mol"]);
    assert!(to.contains("analyze does not take --to"), "{to}");
}

#[test]
fn axial_smiles_analysis_notice_and_convert_warning() {
    const SOURCE: &str = "c1(-c2c(Br)cccc2Cl)c([C@H](O)C)cccc1[C@@H](O)C";
    const EXPECTED: &str = "C[C@H](O)c1cccc([C@@H](C)O)c1-c1c(Cl)cccc1Br";
    const NOTICE: &str =
        "SMILES has no notation for atropisomer axes; the axial configuration was dropped.";
    let dir = tempfile::tempdir().expect("working folder");
    for annotation in [" |wU:0.9|", " |wD:0.9|", ""] {
        let source = format!("{SOURCE}{annotation}");
        let notice = (!annotation.is_empty()).then_some(NOTICE);
        let output = cli(dir.path(), &["analyze", "--smiles", &source]);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        assert!(output.stderr.is_empty());
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["value"]["analysis"]["smiles"], EXPECTED);
        assert_eq!(
            result["value"]["analysis"]["smiles_notice"].as_str(),
            notice
        );
        assert_eq!(result["warnings"], serde_json::json!([]));

        let output = cli(
            dir.path(),
            &["convert", "--smiles", &source, "--to", "smiles"],
        );
        assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout), EXPECTED);
        assert_eq!(
            text(&output.stderr),
            notice
                .map(|n| format!("reshiki: warning: {n}\n"))
                .unwrap_or_default()
        );
    }
}

#[test]
fn analyze_reads_cpp7_mol_with_trimmed_leading_spaces() {
    let dir = tempfile::tempdir().expect("working folder");
    fs::write(
        dir.path().join("cpp7-trimmed.mol"),
        include_bytes!("fixtures/mol/cpp7-trimmed.mol"),
    )
    .expect("trimmed MOL fixture");
    let output = cli(dir.path(), &["analyze", "cpp7-trimmed.mol"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let line = stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\n'), "{stdout}");
    let result: serde_json::Value = serde_json::from_str(line).expect("analyze JSON");
    assert_eq!(result["value"]["analysis"]["formula"], "C42H28");
}

#[test]
fn unknown_commands_exit_2_quickly() {
    let dir = tempfile::tempdir().expect("working folder");
    let stderr = assert_usage(&dir, &["bogus"]);
    assert!(stderr.contains("unknown command `bogus`"), "{stderr}");
}

/// Runs a command that writes `name` in `dir` and must leave stdout empty;
/// returns the file.
fn to_file(dir: &Path, args: &[&str], name: &str) -> Vec<u8> {
    let output = cli(dir, args);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        output.stdout.is_empty(),
        "{args:?}: {}",
        text(&output.stdout)
    );
    fs::read(dir.join(name)).expect("the output file")
}

/// A PNG's width and height, from its IHDR chunk.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
    assert_eq!(png.get(12..16), Some(&b"IHDR"[..]));
    let side = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().expect("4 bytes"));
    (side(16), side(20))
}

#[test]
fn render_writes_a_png_or_svg_preview() {
    let dir = tempfile::tempdir().expect("working folder");
    let png = to_file(
        dir.path(),
        &["render", "--smiles", "CCO", "-o", "eth.png"],
        "eth.png",
    );
    let (width, height) = png_size(&png);
    assert!(width <= 1600 && height <= 1000, "{width} × {height}");
    let output = cli(
        dir.path(),
        &[
            "render", "--smiles", "CCO", "--to", "png", "--width", "120", "--height", "90",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let (width, height) = png_size(&output.stdout);
    assert!(width <= 120 && height <= 90, "{width} × {height}");
    let output = cli(dir.path(), &["render", "--smiles", "CCO", "--to", "svg"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        output.stdout.starts_with(b"<svg"),
        "{}",
        text(&output.stdout)
    );
    assert_eq!(files(&dir), ["eth.png"]);
}

#[test]
fn render_usage_and_size_errors() {
    let dir = tempfile::tempdir().expect("working folder");
    let pdf = assert_usage(&dir, &["render", "--smiles", "CCO", "-o", "x.pdf"]);
    assert!(
        pdf.contains("unsupported output format `pdf`; supported: png, svg"),
        "{pdf}"
    );
    let width = assert_usage(
        &dir,
        &["render", "--smiles", "CCO", "--to", "png", "--width", "1.5"],
    );
    assert!(width.contains("--width must be a whole number"), "{width}");
    // The render tool checks the size: a budget error exits 1.
    let output = cli(
        dir.path(),
        &[
            "render", "--smiles", "CCO", "-o", "big.png", "--width", "8192", "--height", "8192",
        ],
    );
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(output.stdout.is_empty());
    assert!(
        stderr.starts_with("reshiki: error[budget]: 8192 × 8192 is 67108864 pixels;"),
        "{stderr}"
    );
    assert!(files(&dir).is_empty(), "no file is written");
}

/// The working folder with the esterification Proposal as `proposal.json`.
fn with_proposal() -> TempDir {
    let dir = tempfile::tempdir().expect("working folder");
    let proposal = headless::esterification().to_string();
    fs::write(dir.path().join("proposal.json"), proposal).expect("proposal.json");
    dir
}

#[test]
fn compose_writes_the_scheme_as_convert_does() {
    let dir = with_proposal();
    let pdf = to_file(
        dir.path(),
        &["compose", "proposal.json", "-o", "scheme.pdf"],
        "scheme.pdf",
    );
    assert!(pdf.starts_with(b"%PDF"));
    let proposal = fs::read(dir.path().join("proposal.json")).expect("proposal.json");
    let output = headless::run_cli(
        dir.path(),
        &["compose", "-", "--to", "svg"],
        &proposal,
        CONVERT,
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        output.stdout.starts_with(b"<svg"),
        "{}",
        text(&output.stdout)
    );
    let pages = assert_usage(
        &dir,
        &["compose", "proposal.json", "--pages", "--to", "svg"],
    );
    assert!(
        pages.contains("--pages is valid only with pdf output"),
        "{pages}"
    );
    assert_eq!(files(&dir), ["proposal.json", "scheme.pdf"]);
}

#[test]
fn compose_rejects_an_invalid_proposal_as_the_assistant_does() {
    let dir = tempfile::tempdir().expect("working folder");
    let mut proposal = headless::esterification();
    proposal["reactions"][0]["reactants"][0]["rotation"] = 45.into();
    let expected = serde_json::from_value::<Proposal>(proposal.clone())
        .expect("a Proposal")
        .validate()
        .expect_err("45 degrees is off the 30-degree grid");
    fs::write(dir.path().join("p.json"), proposal.to_string()).expect("p.json");
    let output = cli(dir.path(), &["compose", "p.json", "-o", "s.svg"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        text(&output.stderr),
        format!("reshiki: error[invalid_arguments]: {expected}\n")
    );
    fs::write(dir.path().join("p.json"), b"{").expect("p.json");
    let output = cli(dir.path(), &["compose", "p.json", "-o", "s.svg"]);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.starts_with("reshiki: error: PROPOSAL is not valid JSON: "),
        "{stderr}"
    );
    assert_eq!(files(&dir), ["p.json"]);
}

/// A 3D model is presentation only: molecular exports, analysis and figures of
/// a drawing that carries one work as they do without it.
#[test]
fn a_drawing_with_a_3d_model_still_converts_and_analyzes() {
    use reshiki::document::{Document, MODELS_3D, Point};
    use reshiki::models3d::{Display, ForceField, Model3d, Placed};
    let dir = tempfile::tempdir().expect("working folder");
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", Point::new(0., 0.));
    let c2 = doc.add_atom("C", Point::new(36., -21.));
    let o = doc.add_atom("O", Point::new(72., 0.));
    doc.add_bond(c1, c2, 1, "plain");
    doc.add_bond(c2, o, 1, "plain");
    let placed = [
        (c1, [-1.2, -0.2, 0.1]),
        (c2, [0.1, 0.5, -0.1]),
        (o, [1.2, -0.3, 0.2]),
    ]
    .map(|(id, xyz)| Placed { id, xyz })
    .to_vec();
    doc.models
        .push(Model3d::new(&doc, placed, vec![], ForceField::Mmff94s));
    doc.require_version(MODELS_3D);
    assert!(reshiki::models3d::set_display(&mut doc, 0, Display::Both));
    let bytes = doc.file_json().expect("native JSON");
    fs::write(dir.path().join("eth.rsk"), bytes).expect("write eth.rsk");

    let output = cli(dir.path(), &["convert", "eth.rsk", "--to", "smiles"]);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert_eq!(text(&output.stdout).trim(), "CCO");
    for to in ["mol", "cdxml"] {
        let output = cli(dir.path(), &["convert", "eth.rsk", "--to", to]);
        let stderr = text(&output.stderr);
        assert_eq!(output.status.code(), Some(0), "{to}: {stderr}");
    }
    let output = cli(dir.path(), &["analyze", "eth.rsk"]);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    let result: serde_json::Value =
        serde_json::from_str(text(&output.stdout).trim()).expect("analyze JSON");
    assert_eq!(result["value"]["analysis"]["formula"], "C2H6O");
    let output = cli(dir.path(), &["convert", "eth.rsk", "-o", "eth.svg"]);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("needs review"), "{stderr}");
}

/// One input, the same answers on every surface: the fixture is also checked
/// by crates/python/tests/test_parity.py and packages/editor/test/
/// parity.test.ts, and rewritten with scripts/agent_parity_fixture.py.
#[test]
fn the_cross_surface_parity_fixture_holds_for_the_cli() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/agent-api/parity.json")).expect("the fixture");
    let dir = tempfile::tempdir().expect("working folder");
    let ok = |args: &[&str], input: &str| {
        let output = headless::run_cli(dir.path(), args, input.as_bytes(), CONVERT);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            text(&output.stderr)
        );
        text(&output.stdout)
    };
    let molecule = fixture["molecule"]["smiles"].as_str().expect("SMILES");
    for (format, expected) in fixture["molecule"]["outputs"].as_object().expect("outputs") {
        assert_eq!(
            ok(&["convert", "--smiles", molecule, "--to", format], ""),
            expected.as_str().expect("text"),
            "{format}"
        );
    }
    let analyzed: serde_json::Value =
        serde_json::from_str(&ok(&["analyze", "--smiles", molecule], "")).expect("one JSON line");
    for (name, expected) in fixture["molecule"]["properties"]
        .as_object()
        .expect("properties")
    {
        assert_eq!(analyzed["value"]["analysis"][name], *expected, "{name}");
    }
    let reaction = fixture["reaction"]["rsmi"]
        .as_str()
        .expect("reaction SMILES");
    assert_eq!(
        ok(
            &["convert", "-", "--from", "rsmi", "--to", "rsmi"],
            reaction
        ),
        fixture["reaction"]["outputs"]["rsmi"]
    );
    let nmr = fixture["nmr"]["smiles"].as_str().expect("NMR SMILES");
    for nucleus in ["1H", "13C"] {
        let predicted: serde_json::Value = serde_json::from_str(&ok(
            &["predict-nmr", "--smiles", nmr, "--nucleus", nucleus],
            "",
        ))
        .expect("one JSON line");
        assert_eq!(
            predicted["value"]["rows"], fixture["nmr"][nucleus],
            "{nucleus}"
        );
    }
    let rxn = ok(&["convert", "-", "--from", "rsmi", "--to", "rxn"], reaction);
    assert!(rxn.starts_with("$RXN"), "{rxn}");
    // SVG text is outlined unless live text is asked for, as on every surface.
    let outlined = ok(&["convert", "--smiles", molecule, "--to", "svg"], "");
    assert!(outlined.contains("<path") && !outlined.contains("<text"));
    let live = ok(
        &[
            "convert",
            "--smiles",
            molecule,
            "--to",
            "svg",
            "--svg-text",
            "live",
        ],
        "",
    );
    assert!(live.contains("<text"));
}

#[test]
fn chemical_names_are_read_offline_and_cdx_is_written() {
    let dir = tempfile::tempdir().expect("working folder");
    let named = headless::run_cli(
        dir.path(),
        &["convert", "-", "--from", "name", "--to", "smiles"],
        b"acetylsalicylic acid\n",
        CONVERT,
    );
    assert_eq!(named.status.code(), Some(0), "{}", text(&named.stderr));
    assert_eq!(text(&named.stdout), "CC(=O)Oc1ccccc1C(=O)O");
    // cdx is written as the binary file and reads back.
    let cdx = to_file(
        dir.path(),
        &["convert", "--smiles", "CCO", "-o", "ethanol.cdx"],
        "ethanol.cdx",
    );
    assert!(cdx.starts_with(b"VjCD0100"));
    let back = cli(dir.path(), &["convert", "ethanol.cdx", "--to", "smiles"]);
    assert_eq!(text(&back.stdout), "CCO", "{}", text(&back.stderr));
}

/// An operation error names its code; an input error has none.
#[test]
fn operation_errors_name_their_code() {
    let dir = tempfile::tempdir().expect("working folder");
    let unreadable = cli(dir.path(), &["convert", "--smiles", "C(", "--to", "mol"]);
    assert_eq!(unreadable.status.code(), Some(1));
    assert_eq!(
        text(&unreadable.stderr),
        "reshiki: error[failed]: Invalid SMILES at byte 2\n"
    );
    let unknown = headless::run_cli(
        dir.path(),
        &["convert", "-", "--from", "name", "--to", "smiles"],
        b"notachemicalname",
        CONVERT,
    );
    assert_eq!(unknown.status.code(), Some(1));
    assert!(
        text(&unknown.stderr).starts_with("reshiki: error[failed]: "),
        "{}",
        text(&unknown.stderr)
    );
    let size = cli(
        dir.path(),
        &[
            "render", "--smiles", "CCO", "--to", "png", "--width", "9000", "-o", "big.png",
        ],
    );
    assert_eq!(
        text(&size.stderr),
        "reshiki: error[invalid_arguments]: max_width must be from 1 to 8192 pixels\n"
    );
    let missing = cli(dir.path(), &["convert", "missing.mol", "--to", "svg"]);
    assert!(
        text(&missing.stderr).starts_with("reshiki: error: cannot read missing.mol"),
        "{}",
        text(&missing.stderr)
    );
}
