use super::{
    args::{Compose, Convert, Export, Input, Name, Output, Render, Schema, Skill, Source, parse},
    *,
};
use reshiki_agent::ops::{
    budget::Budgets,
    documents::{EXPORT_FORMATS, IMPORT_FORMATS},
};
use serde_json::Value;
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

fn export(to: &str, output: Output) -> Export {
    Export {
        format: to.into(),
        output,
        pages: false,
        force: false,
        receipt: false,
    }
}

fn convert(source: Source, from: &str, to: &str, output: Output) -> Command {
    Command::Convert(Convert {
        input: Input {
            source,
            format: from.into(),
        },
        export: export(to, output),
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
        ("render", "Usage: reshiki --cli render "),
        ("compose", "Usage: reshiki --cli compose "),
        ("analyze", "Usage: reshiki --cli analyze "),
        ("name", "Usage: reshiki --cli name "),
        ("info", "Usage: reshiki --cli info"),
        ("tools", "Usage: reshiki --cli tools"),
        ("schema", "Usage: reshiki --cli schema NAME"),
        ("skill", "Usage: reshiki --cli skill -o FOLDER"),
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
    let (_, out, _) = call(&["help", "compose"]).await;
    assert!(out.contains(&EXPORT_FORMATS.join(", ")), "{out}");
    let (_, out, _) = call(&["help", "render"]).await;
    assert!(out.contains("The image format: png, svg."), "{out}");
}

#[tokio::test(flavor = "multi_thread")]
async fn help_shows_the_examples() {
    let (_, out, _) = call(&["help"]).await;
    for example in [
        "reshiki --cli convert --smiles 'CCO' -o ethanol.mol",
        "reshiki --cli convert scheme.rsk -o figure.pdf",
        "reshiki --cli render scheme.rsk -o preview.png",
        "reshiki --cli analyze --smiles 'c1ccccc1O'",
        "reshiki --cli name --smiles 'CC(=O)O'",
        "reshiki --cli compose proposal.json -o scheme.svg",
    ] {
        assert!(
            out.contains(&format!("\n  {example}\n")),
            "{example}: {out}"
        );
        // Every example parses.
        let tokens: Vec<&str> = example
            .split(' ')
            .skip(2)
            .map(|token| token.trim_matches('\''))
            .collect();
        assert!(parsed(&tokens).is_ok(), "{example}");
    }
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

#[test]
fn discovery_command_lines_parse() {
    assert_eq!(parsed(&["tools"]), Ok(Command::Tools));
    for tokens in [&["schema", "proposal"][..], &["schema", "--", "proposal"]] {
        assert_eq!(parsed(tokens), Ok(Command::Schema(Schema::Proposal)));
    }
    for alias in ["help", "--help", "-h"] {
        for (tokens, topic, json) in [
            (vec![alias], None, false),
            (vec![alias, "convert"], commands::find("convert"), false),
            (vec![alias, "name"], commands::find("name"), false),
            (vec![alias, "--json"], None, true),
            (
                vec![alias, "--json", "convert"],
                commands::find("convert"),
                true,
            ),
            (
                vec![alias, "convert", "--json"],
                commands::find("convert"),
                true,
            ),
            (vec![alias, "--json", "name"], commands::find("name"), true),
            (vec![alias, "name", "--json"], commands::find("name"), true),
        ] {
            assert_eq!(
                parsed(&tokens),
                Ok(Command::Help { topic, json }),
                "{tokens:?}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn discovery_outputs_match_the_contract_goldens() {
    let tools_golden = include_str!("../../tests/fixtures/agent-contract/cli-tools.json");
    let help_golden = include_str!("../../tests/fixtures/agent-contract/cli-help.json");
    let schema_golden = include_str!("../../tests/fixtures/agent-contract/proposal-schema.json");
    for (tokens, golden) in [
        (&["tools"][..], tools_golden),
        (&["help", "--json"][..], help_golden),
        (&["schema", "proposal"][..], schema_golden),
    ] {
        let (code, out, err) = call(tokens).await;
        assert_eq!(code, SUCCESS, "{tokens:?}: {err}");
        assert!(err.is_empty(), "{tokens:?}: {err}");
        assert_help_golden(&out, golden, &tokens.join(" "));
        let line = out.strip_suffix('\n').unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(line, value.to_string(), "one minified JSON line");
    }
    let (_, out, _) = call(&["tools"]).await;
    let tools: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(tools["tools"], reshiki_agent::ops::catalog::catalog_json());
}

#[tokio::test(flavor = "multi_thread")]
async fn json_help_lists_every_command_and_option_in_order() {
    let (_, out, _) = call(&["help", "--json"]).await;
    let help: Value = serde_json::from_str(&out).unwrap();
    let commands = help["commands"].as_array().unwrap();
    let names: Vec<_> = commands
        .iter()
        .map(|command| command["name"].as_str().unwrap())
        .collect();
    let expected: Vec<_> = commands::COMMANDS.iter().map(|spec| spec.name).collect();
    assert_eq!(names, expected);
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "each command appears exactly once"
    );
    for (command, spec) in commands.iter().zip(commands::COMMANDS) {
        let options = command["options"].as_array().unwrap();
        assert_eq!(options.len(), spec.options.len(), "{}", spec.name);
        for (option, spec) in options.iter().zip(spec.options) {
            assert_eq!(option["flag"], spec.flag);
            assert_eq!(option["value"], json!(spec.value));
            assert_eq!(
                option["default"],
                json!(spec.default.map(|default| default()))
            );
            assert_eq!(option["help"], spec.help);
            assert_eq!(option["sets"], json!(spec.sets));
            assert_eq!(option["equivalent"], json!(spec.equivalent));
        }
    }
}

#[cfg(unix)]
const SKILL_EXECUTABLE: &str = "/opt/reshiki/reshiki";
#[cfg(windows)]
const SKILL_EXECUTABLE: &str = r"C:\ReShiki\reshiki.exe";

#[test]
fn skill_command_lines_parse_without_resolving_the_executable() {
    assert_eq!(
        parsed(&["skill", "-o", "FOLDER"]),
        Ok(Command::Skill(Skill {
            folder: "FOLDER".into(),
            name: "reshiki".into(),
            executable: None,
            force: false,
            receipt: false,
        }))
    );
    assert_eq!(
        parsed(&[
            "skill",
            "--name",
            "chem-2",
            "--executable",
            SKILL_EXECUTABLE,
            "--force",
            "-o",
            "FOLDER",
            "--receipt",
            "--"
        ]),
        Ok(Command::Skill(Skill {
            folder: "FOLDER".into(),
            name: "chem-2".into(),
            executable: Some(SKILL_EXECUTABLE.into()),
            force: true,
            receipt: true,
        }))
    );
    for name in ["a", "1", &"a".repeat(64)] {
        assert!(parsed(&["skill", "-o", "FOLDER", "--name", name]).is_ok());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn skill_usage_errors_exit_2_before_writing() {
    for (tokens, message) in [
        (vec!["skill"], "skill writes a folder; pass -o FOLDER"),
        (
            vec!["skill", "-o", "-"],
            "skill writes a folder; pass -o FOLDER",
        ),
        (
            vec!["skill", "-o", ""],
            "skill writes a folder; pass -o FOLDER",
        ),
        (vec!["skill", "-o"], "-o needs a value"),
        (
            vec!["skill", "-o", "FOLDER", "x"],
            "skill takes no input; unexpected `x`",
        ),
        (
            vec!["skill", "-o", "FOLDER", "--", "-x"],
            "skill takes no input; unexpected `-x`",
        ),
        (
            vec!["skill", "-o", "FOLDER", "--executable", "relative"],
            "--executable must be an absolute path",
        ),
        (
            vec!["skill", "-o", "FOLDER", "--executable", "/line\nbreak"],
            "--executable must not contain control characters",
        ),
        (
            vec!["skill", "-o", "FOLDER", "--from", "mol"],
            "skill does not take --from",
        ),
        // Flags only skill takes stay unknown options to the file commands.
        (
            vec!["convert", "a.mol", "--to", "svg", "--name", "reshiki"],
            "unknown option `--name`",
        ),
        (
            vec!["analyze", "a.mol", "--executable", SKILL_EXECUTABLE],
            "unknown option `--executable`",
        ),
        (
            vec!["skill", "-o", "FOLDER", "-o", "FOLDER"],
            "-o given more than once",
        ),
        (
            vec!["skill", "-o", "FOLDER", "--name", "a", "--name", "b"],
            "--name given more than once",
        ),
        (
            vec![
                "skill",
                "-o",
                "FOLDER",
                "--executable",
                SKILL_EXECUTABLE,
                "--executable",
                SKILL_EXECUTABLE,
            ],
            "--executable given more than once",
        ),
        (
            vec!["skill", "-o", "FOLDER", "--force", "--force"],
            "--force given more than once",
        ),
        (
            vec!["skill", "-o", "FOLDER", "--receipt", "--receipt"],
            "--receipt given more than once",
        ),
    ] {
        let (code, out, err) = call(&tokens).await;
        assert_eq!(code, USAGE, "{tokens:?}: {err}");
        assert!(out.is_empty());
        assert_eq!(err, format!("reshiki: {message}\n"));
    }
    for name in [
        "",
        "Reshiki",
        "--",
        "-reshiki",
        "reshiki-",
        "re--shiki",
        "claude",
        "my-claude-tools",
        "anthropic",
        "é",
        "a/b",
        &"a".repeat(65),
    ] {
        let (code, out, err) = call(&["skill", "-o", "FOLDER", "--name", name]).await;
        assert_eq!(code, USAGE, "{name}: {err}");
        assert!(out.is_empty());
        assert!(
            err.starts_with("reshiki: --name must be 1-64 lowercase letters"),
            "{err}"
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn skill_name_and_executable_require_utf8() {
    use std::os::unix::ffi::OsStrExt;
    for (flag, message) in [
        ("--name", "--name must be UTF-8"),
        ("--executable", "--executable must be UTF-8"),
    ] {
        let args = vec![
            "skill".into(),
            "-o".into(),
            "FOLDER".into(),
            flag.into(),
            std::ffi::OsStr::from_bytes(b"/invalid/\xff").to_owned(),
        ];
        let (code, out, err) = call_os(args).await;
        assert_eq!(code, USAGE);
        assert!(out.is_empty());
        assert_eq!(err, format!("reshiki: {message}\n"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn json_help_for_one_command_preserves_the_other_fields() {
    let (_, out, _) = call(&["help", "--json"]).await;
    let full: Value = serde_json::from_str(&out).unwrap();
    for name in ["convert", "name"] {
        let command = full["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["name"] == name)
            .unwrap()
            .clone();
        let mut expected = full.clone();
        expected["commands"] = json!([command]);
        for tokens in [["help", "--json", name], ["help", name, "--json"]] {
            let (code, out, err) = call(&tokens).await;
            assert_eq!(code, SUCCESS, "{tokens:?}");
            assert!(err.is_empty(), "{tokens:?}: {err}");
            assert_eq!(serde_json::from_str::<Value>(&out).unwrap(), expected);
        }
    }
}

#[test]
fn catalog_mappings_name_real_tool_arguments() {
    let catalog = reshiki_agent::ops::catalog::catalog_json();
    let tools = catalog.as_array().unwrap();
    for spec in commands::COMMANDS {
        for name in spec.tools {
            assert!(tools.iter().any(|tool| tool["name"] == *name), "{name}");
        }
        let sets = spec
            .operand
            .and_then(|operand| operand.sets)
            .into_iter()
            .chain(spec.options.iter().filter_map(|option| option.sets));
        let equivalents = spec.options.iter().filter_map(|option| option.equivalent);
        for (mapping, called) in sets
            .map(|mapping| (mapping, true))
            .chain(equivalents.map(|mapping| (mapping, false)))
        {
            let (tool, argument) = mapping.split_once('.').unwrap();
            assert_eq!(
                spec.tools.contains(&tool),
                called,
                "{}: {mapping}",
                spec.name
            );
            let tool = tools.iter().find(|entry| entry["name"] == tool).unwrap();
            assert!(
                tool["inputSchema"]["properties"].get(argument).is_some(),
                "{mapping}"
            );
        }
    }
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

#[tokio::test(flavor = "multi_thread")]
async fn version_prints_the_app_version_and_exits() {
    let (code, out, err) = call(&["--version"]).await;
    assert_eq!(code, SUCCESS);
    assert_eq!(
        out,
        format!("reshiki {}\n", crate::updates::CURRENT_VERSION)
    );
    assert!(err.is_empty());
    let (code, out, err) = call(&["--version", "x"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(err, "reshiki: --version takes no arguments\n");
}

#[test]
fn the_format_sets_are_the_ops_enums() {
    assert_eq!(input_formats(), IMPORT_FORMATS);
    assert_eq!(output_formats(), EXPORT_FORMATS);
    assert_eq!(render_formats(), ["png", "svg"]);
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

#[tokio::test(flavor = "multi_thread")]
async fn crystal_inputs_are_usage_errors_before_io() {
    let folder = tempfile::tempdir().unwrap();
    let output = folder.path().join("never-written.svg");
    // These paths deliberately do not exist: rejection must precede reading.
    for name in [
        "nacl.cif",
        "urea.CIF",
        "urea.pdb",
        "urea.ent",
        "rutile.vasp",
        "cell.poscar",
        "cell.contcar",
        "water.xyz",
        "cell.extxyz",
        "POSCAR",
        "CONTCAR",
        "poscar",
        "contcar",
        "POSCAR.relaxed",
        "CONTCAR.1",
    ] {
        for command in ["convert", "render", "analyze", "name"] {
            for automatic in [false, true] {
                let mut args = vec![OsString::from(command), folder.path().join(name).into()];
                if automatic {
                    args.extend([OsString::from("--from"), OsString::from("auto")]);
                }
                if !matches!(command, "analyze" | "name") {
                    args.extend([OsString::from("-o"), output.clone().into()]);
                }
                let (code, out, err) = call_os(args).await;
                assert_eq!(code, USAGE, "{command} {name}: {err}");
                assert!(out.is_empty(), "{out}");
                assert!(
                    err.contains(&format!("{command} does not support crystal input")),
                    "{err}"
                );
                assert!(err.contains(&input_formats().join(", ")), "{err}");
                assert!(!output.exists());
            }
        }
    }
}

#[test]
fn explicit_crystal_formats_are_rejected_and_molecular_overrides_are_retained() {
    for format in [
        "cif", "pdb", "ent", "vasp", "poscar", "contcar", "xyz", "extxyz",
    ] {
        let error = parsed(&["analyze", "-", "--from", format]).unwrap_err();
        assert!(error.contains("does not support crystal input"), "{error}");
    }
    for name in [
        "molecule.smi",
        "molecule.smiles",
        "molecule.txt",
        "molecule.unknown",
        "molecule",
    ] {
        assert_eq!(
            parsed(&["analyze", name]),
            Ok(Command::Analyze(Input {
                source: Source::File(name.into()),
                format: "smiles".into()
            }))
        );
    }
    for name in ["misnamed.cif", "POSCAR"] {
        assert_eq!(
            parsed(&["analyze", name, "--from", "smiles"]),
            Ok(Command::Analyze(Input {
                source: Source::File(name.into()),
                format: "smiles".into()
            }))
        );
    }
    for (name, format) in [("POSCAR.mol", "mol"), ("CONTCAR.rsk", "reshiki")] {
        assert_eq!(
            parsed(&["analyze", name]),
            Ok(Command::Analyze(Input {
                source: Source::File(name.into()),
                format: format.into()
            }))
        );
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
    assert!(flags.export.pages && flags.export.force && flags.export.receipt);
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

#[test]
fn name_command_lines_parse() {
    for (tokens, source, format, json) in [
        (
            &["name", "--smiles", "CCO"][..],
            Source::Smiles("CCO".into()),
            "smiles",
            false,
        ),
        (&["name", "-", "--from", "mol"], Source::Stdin, "mol", false),
        (
            &["name", "--smiles", "C", "--json"],
            Source::Smiles("C".into()),
            "smiles",
            true,
        ),
        (
            &["name", "a.MOL"],
            Source::File("a.MOL".into()),
            "mol",
            false,
        ),
        (
            &["name", "misnamed.cif", "--from", "smiles"],
            Source::File("misnamed.cif".into()),
            "smiles",
            false,
        ),
    ] {
        assert_eq!(
            parsed(tokens),
            Ok(Command::Name(Name {
                input: Input {
                    source,
                    format: format.into(),
                },
                json,
            })),
            "{tokens:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn name_prints_one_plain_name_without_a_banner() {
    let (code, out, err) = call(&["name", "--smiles", "CC(=O)O"]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert_eq!(out, "acetic acid\n");
    assert!(err.is_empty(), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn name_json_reports_preferred_and_nonpreferred_rule_paths() {
    use crate::naming::{NameReason, NameStatus};

    for (smiles, expected, status, reason) in [
        ("CCO", "ethanol", NameStatus::Preferred, Value::Null),
        (
            "CC(=NN)C",
            "propan-2-one hydrazone",
            NameStatus::NotPreferred(NameReason::FunctionalClass),
            json!("functional_class"),
        ),
    ] {
        let (code, out, err) = call(&["name", "--smiles", smiles, "--json"]).await;
        assert_eq!(code, SUCCESS, "{smiles}: {err}");
        assert!(err.is_empty(), "{smiles}: {err}");
        let line = out.strip_suffix('\n').unwrap();
        let result: Value = serde_json::from_str(line).unwrap();
        assert_eq!(line, result.to_string(), "one minified JSON line");
        let value = &result["value"];
        assert_eq!(value["name"], expected, "{smiles}");
        assert_eq!(
            value["preferred"],
            status == NameStatus::Preferred,
            "{smiles}"
        );
        assert_eq!(
            value["status"],
            serde_json::to_value(status).unwrap()["status"],
            "{smiles}"
        );
        assert_eq!(value["reason"], reason, "{smiles}");
        assert_eq!(value["detail"], status.detail(), "{smiles}");
        assert_eq!(result["experimental"], true);
        assert_eq!(result["api"]["stability"], "experimental");
        assert_eq!(
            result["api"]["operation_api"],
            crate::envelope::OPERATION_API_VERSION
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn name_declined_inputs_fail_without_stdout() {
    // Import would drop the allene stereo; the raw SMILES check declines it.
    for smiles in ["CC.O", "[13CH3]CO", "FC=[C@AL1]=CF"] {
        for json in [false, true] {
            let mut tokens = vec!["name", "--smiles", smiles];
            if json {
                tokens.push("--json");
            }
            let (code, out, err) = call(&tokens).await;
            assert_eq!(code, FAILURE, "{tokens:?}: {err}");
            assert!(out.is_empty(), "{tokens:?}: {out}");
            assert!(err.starts_with("reshiki: error: "), "{tokens:?}: {err}");
        }
    }
}

#[test]
fn render_command_lines_parse() {
    let render = |source: Source, from: &str, to: &str, output: Output| Render {
        input: Input {
            source,
            format: from.into(),
        },
        format: to.into(),
        output,
        width: None,
        height: None,
        force: false,
    };
    for (tokens, expected) in [
        (
            &["render", "a.rsk", "-o", "b.PNG"][..],
            render(
                Source::File("a.rsk".into()),
                "reshiki",
                "png",
                Output::File("b.PNG".into()),
            ),
        ),
        (
            &["render", "--smiles", "CCO", "--to", "svg"],
            render(
                Source::Smiles("CCO".into()),
                "smiles",
                "svg",
                Output::Stdout,
            ),
        ),
        (
            &[
                "render", "-", "--from", "mol", "--to", "png", "-o", "-", "--width", "640",
                "--height", "+480", "--force",
            ],
            Render {
                width: Some(640),
                height: Some(480),
                force: true,
                ..render(Source::Stdin, "mol", "png", Output::Stdout)
            },
        ),
        // The render tool, not the parser, checks the size's range.
        (
            &["render", "a.mol", "--to", "svg", "--width", "0"],
            Render {
                width: Some(0),
                ..render(Source::File("a.mol".into()), "mol", "svg", Output::Stdout)
            },
        ),
    ] {
        assert_eq!(parsed(tokens), Ok(Command::Render(expected)), "{tokens:?}");
    }
}

#[test]
fn compose_command_lines_parse() {
    assert_eq!(
        parsed(&["compose", "p.json", "-o", "s.svg"]),
        Ok(Command::Compose(Compose {
            proposal: Some("p.json".into()),
            export: export("svg", Output::File("s.svg".into())),
        }))
    );
    assert_eq!(
        parsed(&[
            "compose",
            "-",
            "-o",
            "s.pdf",
            "--pages",
            "--force",
            "--receipt"
        ]),
        Ok(Command::Compose(Compose {
            proposal: None,
            export: Export {
                pages: true,
                force: true,
                receipt: true,
                ..export("pdf", Output::File("s.pdf".into()))
            },
        }))
    );
    assert_eq!(
        parsed(&["compose", "p.json", "--to", "smiles"]),
        Ok(Command::Compose(Compose {
            proposal: Some("p.json".into()),
            export: export("smiles", Output::Stdout),
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
    let unsupported_render =
        |format: &str| format!("unsupported output format `{format}`; supported: png, svg");
    let unknown_from = format!(
        "unknown input format `unknown`; supported: {}",
        IMPORT_FORMATS.join(", ")
    );
    let cases: Vec<(&[&str], String)> = vec![
        (&["tools", "x"], "tools takes no arguments".into()),
        (&["tools", "--json"], "tools takes no arguments".into()),
        (&["tools", "tools"], "tools takes no arguments".into()),
        (&["schema"], "schema needs a name: proposal".into()),
        (
            &["schema", "x"],
            "unknown schema `x`; supported: proposal".into(),
        ),
        (
            &["schema", "proposal", "x"],
            "schema takes one input; unexpected `x`".into(),
        ),
        (
            &["schema", "proposal", "proposal"],
            "schema takes one input; unexpected `proposal`".into(),
        ),
        (
            &["schema", "--to", "svg"],
            "schema does not take --to".into(),
        ),
        (&["schema", "--x"], "unknown option `--x`".into()),
        (
            &["help", "--json", "--json"],
            "--json given more than once".into(),
        ),
        (
            &["help", "--json", "a", "b"],
            "help takes at most one command".into(),
        ),
        (
            &["help", "--json", "x"],
            "unknown command `x`; run `reshiki --cli help`".into(),
        ),
        (
            &["help", "--to"],
            "unknown command `--to`; run `reshiki --cli help`".into(),
        ),
        (
            &["help", "--", "convert"],
            "help takes at most one command".into(),
        ),
        // --json stays unknown to file commands other than name.
        (&["convert", "--json"], "unknown option `--json`".into()),
        (&["analyze", "--json"], "unknown option `--json`".into()),
        (&["render", "--json"], "unknown option `--json`".into()),
        (&["compose", "--json"], "unknown option `--json`".into()),
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
            &["convert", "a.mol", "--from", "unknown", "--to", "svg"],
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
            &["name"],
            "name needs an input: a file, - for standard input, or --smiles TEXT".into(),
        ),
        (
            &["name", "--smiles", "C", "--to", "mol"],
            "name does not take --to".into(),
        ),
        (
            &["name", "--smiles", "C", "-o", "x.mol"],
            "name does not take -o".into(),
        ),
        (
            &["name", "--smiles", "C", "--json", "--json"],
            "--json given more than once".into(),
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
        (
            &["analyze", "--smiles", "C", "--width", "5"],
            "analyze does not take --width".into(),
        ),
        (
            &["convert", "a.mol", "--to", "png", "--height", "5"],
            "convert does not take --height".into(),
        ),
        (
            &["render"],
            "render needs an input: a file, - for standard input, or --smiles TEXT".into(),
        ),
        (
            &["render", "a.mol", "-o", "x.pdf"],
            unsupported_render("pdf"),
        ),
        (
            &["render", "a.mol", "--to", "mol"],
            unsupported_render("mol"),
        ),
        (
            &["render", "a.mol", "-o", "x.smi"],
            unsupported_render("smiles"),
        ),
        (
            &["render", "a.mol"],
            "writing to standard output needs --to FORMAT".into(),
        ),
        (
            &["render", "a.mol", "--to", "png", "--width", "wide"],
            "--width must be a whole number of pixels, not `wide`".into(),
        ),
        (
            &["render", "a.mol", "--to", "png", "--height", "-1"],
            "--height must be a whole number of pixels, not `-1`".into(),
        ),
        (
            &["render", "a.mol", "--to", "png", "--width", "4294967296"],
            "--width must be a whole number of pixels, not `4294967296`".into(),
        ),
        (
            &["render", "a.mol", "--to", "png", "--width"],
            "--width needs a value".into(),
        ),
        (
            &["render", "a.mol", "--to", "png", "--pages"],
            "render does not take --pages".into(),
        ),
        (
            &["render", "a.mol", "-o", "x.png", "--receipt"],
            "render does not take --receipt".into(),
        ),
        (
            &["compose"],
            "compose needs a proposal: a JSON file, or - for standard input".into(),
        ),
        (
            &["compose", "a.json", "b.json", "--to", "svg"],
            "compose takes one input; unexpected `b.json`".into(),
        ),
        (
            &["compose", "--smiles", "C", "--to", "svg"],
            "compose does not take --smiles".into(),
        ),
        (
            &["compose", "-", "--from", "mol", "--to", "svg"],
            "compose does not take --from".into(),
        ),
        (
            &["compose", "p.json", "--to", "png", "--width", "5"],
            "compose does not take --width".into(),
        ),
        (
            &["compose", "p.json"],
            "writing to standard output needs --to FORMAT".into(),
        ),
        (&["compose", "p.json", "-o", "s.rsk"], unsupported("rsk")),
        (
            &["compose", "p.json", "--pages", "--to", "svg"],
            "--pages is valid only with pdf output".into(),
        ),
        (
            &["compose", "p.json", "--to", "svg", "--receipt"],
            "--receipt needs -o FILE".into(),
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
    assert_eq!(convert.export.output, Output::File(PathBuf::from(name)));
    assert_eq!(convert.export.format, "mol");
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

#[tokio::test(flavor = "multi_thread")]
async fn compose_input_errors_exit_1() {
    use reshiki_agent::ops::compose::{NO_DRAWING, REPLACE_IDS};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proposal.json");
    let compose = |path: &std::path::Path| -> Vec<OsString> {
        vec!["compose".into(), path.into(), "--to".into(), "svg".into()]
    };
    std::fs::write(&path, b"{\"explanation\": ").unwrap();
    let (code, out, err) = call_os(compose(&path)).await;
    assert_eq!(code, FAILURE, "{err}");
    assert!(out.is_empty());
    assert!(
        err.starts_with("reshiki: error: PROPOSAL is not valid JSON: EOF while parsing"),
        "{err}"
    );
    let proposal = |replace_ids: Value| {
        json!({
            "explanation": "", "replace_ids": replace_ids, "molecules": [], "reactions": [],
            "composition": {"arrangement": "rows", "columns": 2, "width_pt": 540, "preserve_details": false},
            "sketch": null,
        })
    };
    for (replace_ids, message) in [(json!([1]), REPLACE_IDS), (json!([]), NO_DRAWING)] {
        std::fs::write(&path, proposal(replace_ids).to_string()).unwrap();
        let (code, out, err) = call_os(compose(&path)).await;
        assert_eq!(code, FAILURE, "{err}");
        assert!(out.is_empty());
        assert_eq!(err, format!("reshiki: error: {message}\n"));
    }
    let (code, _, err) = call_os(compose(&dir.path().join("missing.json"))).await;
    assert_eq!(code, FAILURE, "{err}");
    assert!(err.starts_with("reshiki: error: cannot read "), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn stereochemistry_limit_reaches_stderr_with_the_shared_message() {
    let smiles = format!("{}O", "N[C@@H](C)C(=O)".repeat(400));
    let (code, out, err) = call(&["analyze", "--smiles", &smiles]).await;
    assert_eq!(code, FAILURE, "{err}");
    assert!(out.is_empty(), "{out}");
    assert_eq!(
        err,
        "reshiki: error: This structure has too much stereochemistry to process · Try a shorter chain or a fragment\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn binary_output_never_goes_to_a_terminal() {
    for tokens in [
        &["render", "--smiles", "C", "--to", "png"][..],
        &["compose", "-", "--to", "pdf"],
        &["convert", "--smiles", "C", "--to", "png"],
    ] {
        let args: Vec<OsString> = tokens.iter().map(OsString::from).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let parsed = super::parse(&args, &mut err);
        let code = execute(parsed, &mut out, &mut err, true).await;
        assert_eq!(code, USAGE, "{tokens:?}");
        assert!(out.is_empty(), "{tokens:?}");
        assert_eq!(
            String::from_utf8(err).unwrap(),
            "reshiki: refusing to write binary data to a terminal; use -o FILE\n"
        );
    }
}

const HELP_GOLDENS: &[(&str, &str)] = &[
    (
        "help",
        include_str!("../../tests/fixtures/cli-help/help.txt"),
    ),
    (
        "convert",
        include_str!("../../tests/fixtures/cli-help/convert.txt"),
    ),
    (
        "render",
        include_str!("../../tests/fixtures/cli-help/render.txt"),
    ),
    (
        "compose",
        include_str!("../../tests/fixtures/cli-help/compose.txt"),
    ),
    (
        "analyze",
        include_str!("../../tests/fixtures/cli-help/analyze.txt"),
    ),
    (
        "name",
        include_str!("../../tests/fixtures/cli-help/name.txt"),
    ),
    (
        "info",
        include_str!("../../tests/fixtures/cli-help/info.txt"),
    ),
    (
        "tools",
        include_str!("../../tests/fixtures/cli-help/tools.txt"),
    ),
    (
        "schema",
        include_str!("../../tests/fixtures/cli-help/schema.txt"),
    ),
    (
        "skill",
        include_str!("../../tests/fixtures/cli-help/skill.txt"),
    ),
];

fn assert_help_golden(actual: &str, golden: &str, case: &str) {
    // The goldens use LF; normalize a CRLF checkout without changing CLI output.
    assert_eq!(actual, golden.replace("\r\n", "\n"), "{case}");
}

#[test]
fn help_builders_match_golden_text() {
    assert_help_golden(
        &help(None),
        include_str!("../../tests/fixtures/cli-help/help.txt"),
        "help",
    );
    for &(name, golden) in HELP_GOLDENS {
        assert_help_golden(&help(commands::find(name)), golden, name);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn help_commands_match_golden_stdout() {
    let (code, out, err) = call(&["help"]).await;
    assert_eq!(code, SUCCESS, "help");
    assert!(err.is_empty(), "help: {err}");
    assert_help_golden(
        &out,
        include_str!("../../tests/fixtures/cli-help/help.txt"),
        "help",
    );
    for &(name, golden) in HELP_GOLDENS {
        let (code, out, err) = call(&["help", name]).await;
        assert_eq!(code, SUCCESS, "help {name}");
        assert!(err.is_empty(), "help {name}: {err}");
        assert_help_golden(&out, golden, name);
    }
}
