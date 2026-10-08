use super::{
    args::{Convert, Input, Output, Source, parse},
    *,
};
use reshiki_agent::ops::documents::{EXPORT_FORMATS, IMPORT_FORMATS};
use std::path::PathBuf;

async fn call(tokens: &[&str]) -> (i32, String, String) {
    let args = tokens.iter().map(OsString::from).collect();
    call_os(args).await
}

async fn call_os(args: Vec<OsString>) -> (i32, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(args, &mut out, &mut err).await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn parsed(tokens: &[&str]) -> Result<Command, String> {
    let (name, rest) = tokens.split_first().unwrap();
    let rest: Vec<OsString> = rest.iter().map(OsString::from).collect();
    parse(name.as_ref(), &rest)
}

fn convert(source: Source, from: &str, to: &str, output: Output) -> Command {
    Command::Convert(Convert {
        input: Input {
            source,
            format: from.into(),
        },
        format: to.into(),
        output,
        pages: false,
        force: false,
        receipt: false,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn help_prints_the_experimental_usage_to_stdout() {
    for name in ["help", "--help", "-h"] {
        let (code, out, err) = call(&[name]).await;
        assert_eq!(code, SUCCESS, "{name}");
        assert!(out.starts_with("Experimental:"), "{name}: {out}");
        assert_eq!(
            out.lines().nth(1),
            Some("Usage: reshiki --cli <command> [options]")
        );
        assert!(err.is_empty(), "{name}: {err}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn help_describes_one_command() {
    for (topic, usage) in [
        ("convert", "Usage: reshiki --cli convert "),
        ("analyze", "Usage: reshiki --cli analyze "),
        ("info", "Usage: reshiki --cli info"),
        ("help", "Usage: reshiki --cli <command> [options]"),
    ] {
        let (code, out, err) = call(&["help", topic]).await;
        assert_eq!(code, SUCCESS, "{topic}");
        assert!(out.starts_with("Experimental:"), "{topic}: {out}");
        assert!(out.lines().nth(1).unwrap().starts_with(usage), "{out}");
        assert!(err.is_empty(), "{topic}: {err}");
    }
    let (_, out, _) = call(&["help", "convert"]).await;
    assert!(out.contains(&EXPORT_FORMATS.join(", ")), "{out}");
    assert!(out.contains(&IMPORT_FORMATS.join(", ")), "{out}");
}

#[tokio::test(flavor = "multi_thread")]
async fn help_takes_at_most_one_known_command() {
    let (code, out, err) = call(&["help", "convert", "analyze"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(err, "reshiki: help takes at most one command\n");
    let (code, out, err) = call(&["help", "x"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(
        err,
        "reshiki: unknown command `x`; run `reshiki --cli help`\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn info_prints_one_json_object_line() {
    let (code, out, err) = call(&["info"]).await;
    assert_eq!(code, SUCCESS);
    assert!(err.is_empty(), "{err}");
    let line = out.strip_suffix('\n').unwrap();
    assert!(!line.contains('\n'), "{out}");
    let info: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(info["versions"]["document"], crate::document::VERSION);
    assert_eq!(info["versions"]["app"], crate::updates::CURRENT_VERSION);
    assert_eq!(
        info["operation_api"],
        crate::envelope::OPERATION_API_VERSION
    );
    assert_eq!(info["formats"]["import"], json!(IMPORT_FORMATS));
    assert_eq!(info["cli"]["input_formats"], json!(IMPORT_FORMATS));
    assert_eq!(info["cli"]["output_formats"], json!(EXPORT_FORMATS));
    assert_eq!(info["api"]["stability"], "experimental");
    assert_eq!(
        info["api"]["operation_api"],
        crate::envelope::OPERATION_API_VERSION
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn info_takes_no_arguments() {
    let (code, out, err) = call(&["info", "x"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(err, "reshiki: info takes no arguments\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_command_prints_usage_to_stderr() {
    let (code, out, err) = call(&[]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert!(err.starts_with("Experimental:"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_commands_point_to_help() {
    let (code, out, err) = call(&["bogus"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(
        err,
        "reshiki: unknown command `bogus`; run `reshiki --cli help`\n"
    );
}

#[test]
fn the_format_sets_are_the_ops_enums() {
    assert_eq!(input_formats(), IMPORT_FORMATS);
    assert_eq!(output_formats(), EXPORT_FORMATS);
}

#[test]
fn input_files_pick_the_format_the_app_picks() {
    for (extension, format) in [
        ("rsk", "reshiki"),
        ("RSK", "reshiki"),
        ("reshiki", "reshiki"),
        ("moruno", "reshiki"),
        ("mol", "mol"),
        ("MOL", "mol"),
        ("rxn", "rxn"),
        ("rsmi", "rsmi"),
        ("cdxml", "cdxml"),
        ("cdx", "cdx"),
        ("inchi", "inchi"),
        ("smi", "smiles"),
        ("smiles", "smiles"),
        ("txt", "smiles"),
        ("", "smiles"),
    ] {
        assert_eq!(input_format(extension), format, "{extension:?}");
    }
}

#[test]
fn convert_command_lines_parse() {
    let file = |path: &str| Source::File(PathBuf::from(path));
    let to_file = |path: &str| Output::File(PathBuf::from(path));
    for (tokens, expected) in [
        (
            &["convert", "a.mol", "--to", "svg"][..],
            convert(file("a.mol"), "mol", "svg", Output::Stdout),
        ),
        (
            &["convert", "a.CDXML", "-o", "b.SMI"],
            convert(file("a.CDXML"), "cdxml", "smiles", to_file("b.SMI")),
        ),
        (
            &[
                "convert", "a.rsk", "--from", "auto", "-o", "-", "--to", "mol",
            ],
            convert(file("a.rsk"), "auto", "mol", Output::Stdout),
        ),
        (
            &["convert", "-", "--from", "mol", "-o", "b.png"],
            convert(Source::Stdin, "mol", "png", to_file("b.png")),
        ),
        (
            &["convert", "--smiles", "CCO", "--to", "inchi"],
            convert(
                Source::Smiles("CCO".into()),
                "smiles",
                "inchi",
                Output::Stdout,
            ),
        ),
        // A value is the next token, whatever it looks like.
        (
            &["convert", "--smiles", "-C", "--to", "mol"],
            convert(Source::Smiles("-C".into()), "smiles", "mol", Output::Stdout),
        ),
        // `--` ends the options; `-` still means standard input.
        (
            &["convert", "--to", "svg", "--", "--pages"],
            convert(file("--pages"), "smiles", "svg", Output::Stdout),
        ),
        (
            &["convert", "--from", "smiles", "--to", "svg", "--", "-"],
            convert(Source::Stdin, "smiles", "svg", Output::Stdout),
        ),
        // --to wins over the output's extension.
        (
            &["convert", "a.smi", "--to", "svg", "-o", "b.out"],
            convert(file("a.smi"), "smiles", "svg", to_file("b.out")),
        ),
    ] {
        assert_eq!(parsed(tokens), Ok(expected), "{tokens:?}");
    }
    let flags = parsed(&[
        "convert",
        "a.mol",
        "-o",
        "b.pdf",
        "--pages",
        "--force",
        "--receipt",
    ]);
    let Ok(Command::Convert(flags)) = flags else {
        panic!("{flags:?}");
    };
    assert!(flags.pages && flags.force && flags.receipt);
}

#[test]
fn analyze_command_lines_parse() {
    assert_eq!(
        parsed(&["analyze", "--smiles", "CCO"]),
        Ok(Command::Analyze(Input {
            source: Source::Smiles("CCO".into()),
            format: "smiles".into(),
        }))
    );
    assert_eq!(
        parsed(&["analyze", "-", "--from", "inchi"]),
        Ok(Command::Analyze(Input {
            source: Source::Stdin,
            format: "inchi".into(),
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_errors_exit_2_before_any_work() {
    let unsupported = |format: &str| {
        format!(
            "unsupported output format `{format}`; supported: {}",
            EXPORT_FORMATS.join(", ")
        )
    };
    let unknown_from = format!(
        "unknown input format `xyz`; supported: {}",
        IMPORT_FORMATS.join(", ")
    );
    let cases: Vec<(&[&str], String)> = vec![
        (
            &["convert"],
            "convert needs an input: a file, - for standard input, or --smiles TEXT".into(),
        ),
        (
            &["convert", "a.mol", "b.mol", "--to", "svg"],
            "convert takes one input; unexpected `b.mol`".into(),
        ),
        (
            &["convert", "a.mol", "--smiles", "C", "--to", "svg"],
            "convert takes one input: INPUT, - or --smiles, not both".into(),
        ),
        (
            &["convert", "-", "--smiles", "C", "--to", "svg"],
            "convert takes one input: INPUT, - or --smiles, not both".into(),
        ),
        (
            &["convert", "-", "--to", "svg"],
            "reading standard input (-) needs --from FORMAT".into(),
        ),
        (
            &[
                "convert", "--smiles", "C", "--from", "smiles", "--to", "svg",
            ],
            "--smiles cannot be combined with --from".into(),
        ),
        (
            &["convert", "a.mol", "--from", "xyz", "--to", "svg"],
            unknown_from,
        ),
        (
            &["convert", "a.mol", "--to", "svg", "--to", "png"],
            "--to given more than once".into(),
        ),
        (
            &["convert", "a.mol", "--force", "--to", "svg", "--force"],
            "--force given more than once".into(),
        ),
        (
            &["convert", "a.mol", "--bogus"],
            "unknown option `--bogus`".into(),
        ),
        (&["convert", "a.mol", "--to"], "--to needs a value".into()),
        (
            &["convert", "a.mol"],
            "writing to standard output needs --to FORMAT".into(),
        ),
        (
            &["convert", "a.mol", "-o", "-"],
            "writing to standard output needs --to FORMAT".into(),
        ),
        (
            &["convert", "a.mol", "-o", "b"],
            "cannot tell the output format of `b`; pass --to FORMAT".into(),
        ),
        (
            &["convert", "a.mol", "-o", ""],
            "-o needs a file name".into(),
        ),
        (&["convert", "a.mol", "--to", "xyz"], unsupported("xyz")),
        (&["convert", "a.mol", "-o", "b.rsk"], unsupported("rsk")),
        (&["convert", "a.mol", "-o", "b.emf"], unsupported("emf")),
        (&["convert", "a.mol", "-o", "b.cdx"], unsupported("cdx")),
        (&["convert", "a.mol", "-o", "b.rxn"], unsupported("rxn")),
        (&["convert", "a.mol", "--to", "rsmi"], unsupported("rsmi")),
        (
            &["convert", "a.mol", "--to", "svg", "--pages"],
            "--pages is valid only with pdf output".into(),
        ),
        (
            &["convert", "a.mol", "--to", "png", "--pages"],
            "--pages is valid only with pdf output".into(),
        ),
        (
            &["convert", "a.mol", "--to", "svg", "--receipt"],
            "--receipt needs -o FILE".into(),
        ),
        (
            &["convert", "a.mol", "--to", "svg", "-o", "-", "--receipt"],
            "--receipt needs -o FILE".into(),
        ),
        (
            &["analyze"],
            "analyze needs an input: a file, - for standard input, or --smiles TEXT".into(),
        ),
        (
            &["analyze", "--smiles", "C", "--to", "mol"],
            "analyze does not take --to".into(),
        ),
        (
            &["analyze", "--smiles", "C", "-o", "x.mol"],
            "analyze does not take -o".into(),
        ),
        (
            &["analyze", "--smiles", "C", "--pages"],
            "analyze does not take --pages".into(),
        ),
        (
            &["analyze", "--smiles", "C", "--force"],
            "analyze does not take --force".into(),
        ),
        (
            &["analyze", "--smiles", "C", "--receipt"],
            "analyze does not take --receipt".into(),
        ),
        (
            &["analyze", "-"],
            "reading standard input (-) needs --from FORMAT".into(),
        ),
    ];
    for (tokens, message) in cases {
        let (code, out, err) = call(tokens).await;
        assert_eq!(code, USAGE, "{tokens:?}: {err}");
        assert!(out.is_empty(), "{tokens:?}: {out}");
        assert_eq!(err, format!("reshiki: {message}\n"), "{tokens:?}");
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn paths_need_not_be_utf8() {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::OsStr::from_bytes(b"/nonexistent/\xffdrawing.mol");
    let rest = [name.to_owned(), "-o".into(), name.to_owned()];
    let Ok(Command::Convert(convert)) = parse("convert".as_ref(), &rest) else {
        panic!("non-UTF-8 paths parse");
    };
    assert_eq!(convert.input.source, Source::File(PathBuf::from(name)));
    assert_eq!(convert.input.format, "mol");
    assert_eq!(convert.output, Output::File(PathBuf::from(name)));
    assert_eq!(convert.format, "mol");
    // Reading it fails as an input error, not a usage error.
    let args = vec![
        "convert".into(),
        name.to_owned(),
        "--to".into(),
        "svg".into(),
    ];
    let (code, out, err) = call_os(args).await;
    assert_eq!(code, FAILURE, "{err}");
    assert!(out.is_empty());
    assert!(
        err.starts_with("reshiki: error: cannot read /nonexistent/"),
        "{err}"
    );
    // Non-UTF-8 option values are reported lossily.
    let rest = ["a.mol".into(), "--to".into(), name.to_owned()];
    let message = parse("convert".as_ref(), &rest).unwrap_err();
    assert!(message.starts_with("unsupported output format `/nonexistent/\u{fffd}drawing.mol`"));
    let rest = [
        "--smiles".into(),
        name.to_owned(),
        "--to".into(),
        "svg".into(),
    ];
    let message = parse("convert".as_ref(), &rest).unwrap_err();
    assert_eq!(message, "--smiles text must be UTF-8");
}

#[test]
fn input_is_limited_by_the_ops_budgets() {
    let budgets = Budgets::default();
    assert_eq!(io::limit("smiles", &budgets), budgets.max_text_bytes);
    assert_eq!(io::limit("reshiki", &budgets), budgets.max_text_bytes);
    let cdx = io::limit("cdx", &budgets);
    assert!(cdx.div_ceil(3) * 4 <= budgets.max_cdx_base64);
    assert!(cdx >= budgets.max_text_bytes);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.smi");
    std::fs::write(&path, b"CCCC").unwrap();
    assert_eq!(io::read_input(Some(&path), 4).unwrap(), b"CCCC");
    assert_eq!(
        io::read_input(Some(&path), 3).unwrap_err(),
        "input exceeds 3 bytes"
    );
    assert_eq!(io::text("cdx", vec![0xff, 0]).unwrap(), "/wA=");
    assert!(
        io::text("mol", vec![0xff])
            .unwrap_err()
            .starts_with("The file is not valid UTF-8: ")
    );
}

#[test]
fn output_files_are_kept_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.mol");
    io::write_output(&path, b"first", false).unwrap();
    let kept = io::write_output(&path, b"second", false).unwrap_err();
    assert_eq!(
        kept,
        format!("{} exists; pass --force to replace", path.display())
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"first");
    io::write_output(&path, b"third", true).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"third");
    let missing = dir.path().join("missing").join("out.mol");
    assert!(io::write_output(&missing, b"x", false).is_err());
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["out.mol"], "no temporary file is left behind");
}

/// Fails every write when `on_write`, and every flush, counting the flushes.
struct Broken {
    on_write: bool,
    flushes: usize,
}

impl Write for Broken {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.on_write {
            Err(std::io::Error::other("closed"))
        } else {
            Ok(bytes.len())
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flushes += 1;
        Err(std::io::Error::other("closed"))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn output_write_and_flush_errors_fail_with_one_line() {
    for on_write in [true, false] {
        for name in ["help", "info"] {
            let mut out = Broken {
                on_write,
                flushes: 0,
            };
            let mut err = Vec::new();
            let args = vec![OsString::from(name)];
            let code = run_with(args, &mut out, &mut err).await;
            assert_eq!(code, FAILURE, "{name}, on_write {on_write}");
            assert_eq!(out.flushes, 1, "{name}, on_write {on_write}");
            let err = String::from_utf8(err).unwrap();
            assert_eq!(err.lines().count(), 1, "{err}");
            assert!(err.starts_with("reshiki: could not write output:"), "{err}");
        }
    }
}
