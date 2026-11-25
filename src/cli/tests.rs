use super::{
    args::{
        Calc, CalcTask, Compose, Convert, Export, Input, Labels, Name, Output, PredictNmr, Preview,
        Render, Schema, Skill, Source, SvgText, Ts, parse,
    },
    *,
};
use reshiki_agent::ops::{
    budget::Budgets,
    documents::{EXPORT_FORMATS, IMPORT_FORMATS},
};
use reshiki_xtb_worker::protocol::{
    Conventions, Method, ScfRetry, ScfSettings, SolvationState, Solvent, SolventCorrection,
    SolventModel,
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
        svg_text: SvgText::Outline,
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
        labels: None,
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
        ("predict-nmr", "Usage: reshiki --cli predict-nmr "),
        ("export-input", "Usage: reshiki --cli export-input "),
        ("name", "Usage: reshiki --cli name "),
        ("calc", "Usage: reshiki --cli calc "),
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
    // The info tool's envelope, with the command line's formats beside it.
    assert_eq!(info["validation"], json!({"status": "valid"}));
    assert_eq!(info["warnings"], json!([]));
    assert_eq!(
        info["value"]["operation_api"],
        crate::envelope::OPERATION_API_VERSION
    );
    assert_eq!(info["value"]["formats"]["import"], json!(IMPORT_FORMATS));
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
    let mut lines = out.lines();
    assert_eq!(
        lines.next(),
        Some(format!("reshiki {}", crate::updates::CURRENT_VERSION).as_str())
    );
    #[cfg(feature = "xtb")]
    {
        use reshiki_xtb_worker::{AXTB_LICENSE, AXTB_REV, AXTB_SOURCE, AXTB_VERSION};
        assert_eq!(lines.next(), Some(format!("xTB: included (axtb {AXTB_VERSION}, {AXTB_LICENSE}, source {AXTB_SOURCE}/tree/{AXTB_REV})").as_str()));
    }
    #[cfg(not(feature = "xtb"))]
    assert_eq!(lines.next(), Some("xTB: not included in this build"));
    assert_eq!(lines.next(), None);
    assert!(out.ends_with('\n'));
    assert!(err.is_empty());
    let (code, out, err) = call(&["--version", "x"]).await;
    assert_eq!(code, USAGE);
    assert!(out.is_empty());
    assert_eq!(err, "reshiki: --version takes no arguments\n");
}

#[test]
fn calc_parses_its_default_task_compatibility_flags_and_optional_settings() {
    assert_eq!(
        parsed(&["calc", "--energy", "water.xyz"]),
        Ok(Command::Calc(Calc {
            file: "water.xyz".into(),
            task: CalcTask::Energy,
            method: Method::Gfn2,
            gradient: false,
            bond_orders: false,
            charge: 0,
            uhf: None,
            conventions: Conventions::Axtb,
            solvent: None,
            scf: None,
            threads: 1,
        }))
    );
    assert_eq!(
        parsed(&[
            "calc",
            "--gradient",
            "--charge",
            "-1",
            "water.XYZ",
            "--uhf",
            "1",
            "--conventions",
            "xtb-6.8",
            "--energy"
        ]),
        Ok(Command::Calc(Calc {
            file: "water.XYZ".into(),
            task: CalcTask::Energy,
            method: Method::Gfn2,
            gradient: true,
            bond_orders: false,
            charge: -1,
            uhf: Some(1),
            conventions: Conventions::Xtb68,
            solvent: None,
            scf: None,
            threads: 1,
        }))
    );
    assert_eq!(
        parsed(&[
            "calc",
            "--energy",
            "--conventions",
            "axtb",
            "--",
            "-water.xyz"
        ]),
        Ok(Command::Calc(Calc {
            file: "-water.xyz".into(),
            task: CalcTask::Energy,
            method: Method::Gfn2,
            gradient: false,
            bond_orders: false,
            charge: 0,
            uhf: None,
            conventions: Conventions::Axtb,
            solvent: None,
            scf: None,
            threads: 1,
        }))
    );
    for tokens in [
        vec!["calc"],
        vec!["calc", "--energy"],
        vec!["calc", "--energy", "-"],
        vec!["calc", "--energy", "water.mol"],
        vec!["calc", "--energy", "a.xyz", "b.xyz"],
        vec!["calc", "--energy", "--energy", "water.xyz"],
        vec!["calc", "--energy", "--charge", "2147483648", "water.xyz"],
        vec!["calc", "--energy", "--charge", "1.5", "water.xyz"],
        vec!["calc", "--energy", "--uhf", "-1", "water.xyz"],
        vec!["calc", "--energy", "--uhf", "4294967296", "water.xyz"],
        vec!["calc", "--energy", "--uhf", "1.5", "water.xyz"],
        vec!["calc", "--energy", "--charge"],
        vec!["calc", "--energy", "--uhf"],
        vec!["calc", "--energy", "--from", "xyz", "water.xyz"],
    ] {
        assert!(parsed(&tokens).is_err(), "{tokens:?}");
    }
}

#[test]
fn calc_parses_tasks_solvent_and_a_repeatable_scf_ladder() {
    let Command::Calc(default) = parsed(&["calc", "water.xyz"]).unwrap() else {
        panic!("calc command");
    };
    assert_eq!(default.task, CalcTask::Energy);
    assert_eq!(default.method, Method::Gfn2);
    assert!(!default.gradient);
    for (name, task) in [
        ("energy", CalcTask::Energy),
        ("gradient", CalcTask::Gradient),
        ("hessian", CalcTask::Hessian),
        ("orbitals", CalcTask::Orbitals),
    ] {
        let Command::Calc(command) = parsed(&["calc", "--task", name, "water.xyz"]).unwrap() else {
            panic!("calc command");
        };
        assert_eq!(command.task, task);
    }
    let Command::Calc(command) = parsed(&[
        "calc",
        "--task",
        "orbitals",
        "--method",
        "gxtb",
        "--gbsa",
        "water",
        "--solvent-correction",
        "gfn2",
        "--etemp",
        "0",
        "--accuracy",
        "0.5",
        "--max-iter",
        "50",
        "--scf-retry",
        "300:100",
        "--scf-retry",
        "1000",
        "--threads",
        "4",
        "--gradient",
        "--wbo",
        "water.xyz",
    ])
    .unwrap() else {
        panic!("calc command");
    };
    assert_eq!(command.task, CalcTask::Orbitals);
    assert_eq!(command.method, Method::Gxtb);
    assert_eq!(
        command.solvent,
        Some(Solvent {
            model: SolventModel::Gbsa,
            name: "water".into(),
            state: SolvationState::Gsolv,
            correction: Some(SolventCorrection::Gfn2),
        })
    );
    assert_eq!(
        command.scf,
        Some(ScfSettings {
            accuracy: Some(0.5),
            electronic_temperature_k: Some(0.0),
            max_iterations: Some(50),
            retries: vec![
                ScfRetry {
                    electronic_temperature_k: Some(300.0),
                    max_iterations: Some(100)
                },
                ScfRetry {
                    electronic_temperature_k: Some(1000.0),
                    max_iterations: None
                },
            ],
        })
    );
    assert!(command.gradient && command.bond_orders);
    assert_eq!(command.threads, 4);
    assert!(parsed(&["calc", "--etemp", "300", "--scf-retry", "300", "water.xyz"]).is_ok());
}

#[test]
fn calc_allows_lower_retry_temperatures_when_etemp_is_omitted() {
    let Command::Calc(command) = parsed(&[
        "calc",
        "--scf-retry",
        "0",
        "--scf-retry",
        "299:100",
        "water.xyz",
    ])
    .unwrap() else {
        panic!("calc command");
    };
    assert_eq!(
        command.scf,
        Some(ScfSettings {
            accuracy: None,
            electronic_temperature_k: None,
            max_iterations: None,
            retries: vec![
                ScfRetry {
                    electronic_temperature_k: Some(0.0),
                    max_iterations: None,
                },
                ScfRetry {
                    electronic_temperature_k: Some(299.0),
                    max_iterations: Some(100),
                },
            ],
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn calc_rejects_retries_below_explicit_etemp_before_io() {
    for options in [
        vec!["--etemp", "1000", "--scf-retry", "300"],
        vec!["--scf-retry", "300:100", "--etemp", "1000"],
        vec![
            "--etemp",
            "1000",
            "--scf-retry",
            "2000",
            "--scf-retry",
            "300",
        ],
    ] {
        let mut tokens = vec!["calc", "unread.xyz"];
        tokens.extend(options);
        let (code, out, err) = call(&tokens).await;
        assert_eq!(code, USAGE, "{tokens:?}");
        assert!(out.is_empty());
        assert_eq!(
            err,
            "reshiki: --scf-retry temperature must be at least the explicit --etemp\n"
        );
    }
}

#[test]
fn calc_rejects_invalid_task_solvent_and_scf_syntax() {
    for options in [
        vec!["--task", "optimize"],
        vec!["--method", "gxtb-v1.1"],
        vec!["--alpb", "water", "--gbsa", "water"],
        vec!["--alpb", ""],
        vec!["--solvent-correction", "gfn2"],
        vec!["--alpb", "water", "--solvent-correction", "gxtb"],
        vec!["--etemp", "-1"],
        vec!["--etemp", "NaN"],
        vec!["--accuracy", "0"],
        vec!["--accuracy", "inf"],
        vec!["--max-iter", "0"],
        vec!["--max-iter", "1.5"],
        vec!["--scf-retry", ""],
        vec!["--scf-retry", "300:"],
        vec!["--scf-retry", "300:1:2"],
        vec!["--threads", "0"],
        vec!["--threads", "65"],
        vec!["--method", "gfn2", "--method", "gfnff"],
        vec!["--wbo", "--wbo"],
    ] {
        let mut tokens = vec!["calc", "water.xyz"];
        tokens.extend(options);
        assert!(parsed(&tokens).is_err(), "{tokens:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn calc_conventions_usage_errors() {
    for (options, expected) in [
        (
            &["--conventions", "xtb"][..],
            "--conventions must be axtb or xtb-6.8",
        ),
        (&["--conventions"][..], "--conventions needs a value"),
        (
            &["--conventions", "axtb", "--conventions", "xtb-6.8"][..],
            "--conventions given more than once",
        ),
    ] {
        let mut tokens = vec!["calc", "--energy", "water.xyz"];
        tokens.extend_from_slice(options);
        let (code, out, err) = call(&tokens).await;
        assert_eq!(code, USAGE);
        assert!(out.is_empty());
        assert_eq!(err, format!("reshiki: {expected}\n"));
    }
}

#[test]
fn calc_is_an_explicit_cli_only_exception_to_mcp_parity() {
    let spec = commands::find("calc").unwrap();
    assert!(spec.tools.is_empty());
    let catalog = reshiki_agent::ops::catalog::catalog_json();
    assert!(
        !catalog
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "calc")
    );
    assert!(help(Some(spec)).contains("See docs/xtb-worker.md (Command line)"));
    assert!(include_str!("../../docs/xtb-worker.md").contains("`calc` is a CLI-only command."));
}

#[cfg(not(feature = "xtb"))]
#[tokio::test(flavor = "multi_thread")]
async fn calc_without_xtb_reports_unavailability_and_no_stdout() {
    let (code, out, err) = call(&["calc", "--energy", "--gradient", "unread.xyz"]).await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty());
    assert_eq!(err, "reshiki: xTB is not included in this build\n");
}

#[test]
fn ts_parses_mapped_endpoints_and_defaults() {
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let expected = Ts {
        reactant: Some("r.xyz".into()),
        product: Some("p.xyz".into()),
        form: Vec::new(),
        break_: Vec::new(),
        charge: 0,
        uhf: None,
        images: 15,
        workers: 4.min(cpus),
        remap_hydrogens: false,
        keep_endpoints: false,
        out_dir: None,
        json: None,
    };
    assert_eq!(
        parsed(&["ts", "--reactant", "r.xyz", "--product", "p.xyz"]),
        Ok(Command::Ts(expected)),
    );
    let Command::Ts(command) = parsed(&[
        "ts",
        "--reactant",
        "r.xyz",
        "--product",
        "p.xyz",
        "--images",
        "5",
    ])
    .unwrap() else {
        panic!("ts command");
    };
    assert_eq!(command.workers, 3.min(cpus));
}

#[test]
fn ts_parses_repeatable_bond_edits_and_output_paths() {
    assert_eq!(
        parsed(&[
            "ts",
            "--product",
            "p.xyz",
            "--form",
            "3-7",
            "--form",
            "5-3",
            "--break",
            "1-3",
            "--break",
            "3-2",
            "--charge",
            "-1",
            "--uhf",
            "2",
            "--images",
            "24",
            "--workers",
            "8",
            "--keep-endpoints",
            "--out-dir",
            "results",
            "--json",
            "report.json",
        ]),
        Ok(Command::Ts(Ts {
            reactant: None,
            product: Some("p.xyz".into()),
            form: vec![(3, 7), (3, 5)],
            break_: vec![(1, 3), (2, 3)],
            charge: -1,
            uhf: Some(2),
            images: 24,
            workers: 8,
            remap_hydrogens: false,
            keep_endpoints: true,
            out_dir: Some("results".into()),
            json: Some("report.json".into()),
        })),
    );
    assert!(
        parsed(&[
            "ts",
            "--reactant",
            "r.xyz",
            "--break",
            "1-2",
            "--images",
            "5",
            "--workers",
            "8"
        ])
        .is_ok()
    );
}

#[test]
fn ts_rejects_hydrogen_remapping_with_explicit_bond_edits() {
    let message = "--remap-hydrogens requires two XYZ endpoints without --form or --break";
    for (endpoint, edit) in [("--reactant", "--form"), ("--product", "--break")] {
        assert_eq!(
            parsed(&["ts", endpoint, "end.xyz", edit, "1-2", "--remap-hydrogens"]),
            Err(message.into()),
        );
    }
    let Command::Ts(command) = parsed(&[
        "ts",
        "--reactant",
        "r.xyz",
        "--product",
        "p.xyz",
        "--remap-hydrogens",
    ])
    .unwrap() else {
        panic!("ts command");
    };
    assert!(command.remap_hydrogens);
}

#[test]
fn ts_rejects_invalid_options_and_endpoint_combinations() {
    for tokens in [
        &[][..],
        &["--form", "1-2"],
        &["--reactant", "r.xyz"],
        &["--product", "p.xyz"],
        &["--reactant", "r.xyz", "--product", "p.xyz", "--form", "1-2"],
        &[
            "--reactant",
            "r.xyz",
            "--product",
            "p.xyz",
            "--break",
            "1-2",
        ],
        &["--reactant", "r.xyz", "--form", "3-7", "--break", "7-3"],
        &["--reactant", "-", "--form", "1-2"],
        &["--reactant", "", "--form", "1-2"],
        &["r.xyz", "--product", "p.xyz", "--form", "1-2"],
    ] {
        assert!(parsed(&[&["ts"], tokens].concat()).is_err(), "{tokens:?}");
    }
    for pair in [
        "3:7", "3", "3-", "3--7", "3-7-2", "0-7", "3-0", "3-3", "+3-7", "3 -7", "3-X",
    ] {
        for flag in ["--form", "--break"] {
            assert!(
                parsed(&["ts", "--reactant", "r.xyz", flag, pair]).is_err(),
                "{flag} {pair}"
            );
        }
    }
    for options in [
        &["--images", "4"][..],
        &["--images", "25"],
        &["--images", "5.5"],
        &["--workers", "0"],
        &["--workers", "9"],
        &["--workers", "-1"],
        &["--charge", "2147483648"],
        &["--charge", "1.5"],
        &["--uhf", "-1"],
        &["--uhf", "4294967296"],
        &["--uhf", "1.5"],
        &["--uhf"],
        &["--reactant", "again.xyz"],
        &["--product", "again.xyz"],
        &["--json"],
        &["--json", "a", "--json", "b"],
        &["--keep-endpoints", "--keep-endpoints"],
        &["--out-dir", ""],
        &["--method", "gfn2"],
        &["--alpb", "water"],
        &["--conventions", "xtb-6.8"],
    ] {
        let tokens = [
            &["ts", "--reactant", "r.xyz", "--product", "p.xyz"],
            options,
        ]
        .concat();
        assert!(parsed(&tokens).is_err(), "{options:?}");
    }
}

#[cfg(feature = "xtb")]
#[tokio::test]
async fn ts_preflight_rejects_json_collisions_with_the_output_directory_and_its_files() {
    let temp = tempfile::tempdir().unwrap();
    let missing_xyz = temp.path().join("missing.xyz");
    let out_dir = temp.path().join("out");
    let structure = out_dir.join("ts.xyz");
    let cases = [
        (out_dir.clone(), &out_dir),
        (temp.path().join("absent/../out"), &out_dir),
        (temp.path().join("absent/../out/ts.xyz"), &structure),
    ];
    let mut results = Vec::new();
    for (json, target) in cases {
        let (code, out, err) = call_os(vec![
            "ts".into(),
            "--reactant".into(),
            missing_xyz.clone().into_os_string(),
            "--product".into(),
            missing_xyz.clone().into_os_string(),
            "--out-dir".into(),
            out_dir.clone().into_os_string(),
            "--json".into(),
            json.into_os_string(),
        ])
        .await;
        results.push((
            code,
            out,
            err,
            format!("--json and --out-dir both target {}", target.display()),
        ));
    }
    for (code, out, err, expected) in results {
        assert_eq!(code, FAILURE);
        assert!(out.is_empty());
        assert!(err.contains(&expected), "expected {expected}, got {err}");
    }
    assert!(!out_dir.exists());
}

#[test]
fn ts_is_cli_only() {
    let spec = commands::find("ts").unwrap();
    assert!(spec.tools.is_empty());
    assert!(
        reshiki_agent::ops::catalog::catalog_json()
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "ts")
    );
}

#[cfg(not(feature = "xtb"))]
#[tokio::test]
async fn ts_without_xtb_reports_unavailability() {
    let (code, out, err) =
        call(&["ts", "--reactant", "unread.xyz", "--product", "unread.xyz"]).await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty());
    assert_eq!(err, "reshiki: xTB is not included in this build\n");
}

#[cfg(feature = "xtb")]
#[test]
fn ts_worker_failure_keeps_characterized_candidate_and_files() {
    ts::tests::worker_failure_keeps_characterized_candidate_and_files();
}

#[cfg(feature = "xtb")]
#[test]
fn ts_worker_count_disagreement_fails_with_diagnostics() {
    ts::tests::worker_count_disagreement_fails_with_diagnostics();
}

#[cfg(feature = "xtb")]
#[test]
fn ts_irc_failure_preserves_candidate_and_explains_missing_branches() {
    ts::tests::irc_failure_preserves_candidate_and_explains_missing_branches();
}

#[cfg(feature = "xtb")]
#[test]
fn ts_worker_hessian_uses_engine_masses_and_explicit_symmetry() {
    ts::tests::worker_hessian_uses_engine_masses_and_explicit_symmetry();
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
        ("cds", "cdx"),
        ("CDS", "cdx"),
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
fn predict_nmr_command_lines_parse() {
    for nucleus in ["1H", "13C"] {
        assert_eq!(
            parsed(&["predict-nmr", "--smiles", "CCO", "--nucleus", nucleus]),
            Ok(Command::PredictNmr(PredictNmr {
                input: Input {
                    source: Source::Smiles("CCO".into()),
                    format: "smiles".into(),
                },
                nucleus,
            }))
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn predict_nmr_prints_the_mcp_value_with_smiles_atom_ids() {
    let (code, out, err) = call(&["predict-nmr", "--smiles", "CCOC(C)=O", "--nucleus", "1H"]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(err.is_empty(), "{err}");
    let result: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["experimental"], true);
    let value = &result["value"];
    assert_eq!(value["revision"], "0");
    assert_eq!(value["nucleus"], "1H");
    assert_eq!(value["credit"], crate::nmr::ATTRIBUTION);
    // The SMILES atoms 1, 2 and 5 carry H: CH3, CH2 and the acetyl CH3.
    let atoms: Vec<_> = value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["atom"].as_str().unwrap())
        .collect();
    assert_eq!(atoms, ["1", "2", "5"]);
    let (code, out, err) = call(&["predict-nmr", "--smiles", "CCO.CC", "--nucleus", "13C"]).await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty(), "{out}");
    assert_eq!(
        err,
        "reshiki: error[failed]: NMR prediction needs exactly one molecule; this drawing has 2\n"
    );
}

#[test]
fn convert_labels_parse_with_their_nucleus() {
    let Ok(Command::Convert(convert)) = parsed(&[
        "convert",
        "--smiles",
        "CCO",
        "-o",
        "a.svg",
        "--nucleus",
        "13C",
        "--labels",
        "value",
    ]) else {
        panic!("convert with labels did not parse");
    };
    assert_eq!(
        convert.labels,
        Some(Labels {
            nucleus: "13C",
            format: "value".into(),
            decimals: 1,
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn convert_writes_predicted_shifts_as_atom_numbers() {
    let svg_args = [
        "convert",
        "--smiles",
        "CCOC(C)=O",
        "--to",
        "svg",
        "--svg-text",
        "live",
        "--nucleus",
        "13C",
        "--labels",
        "both",
    ];
    let (code, out, err) = call(&svg_args).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(err.is_empty(), "{err}");
    // 13C labels get one decimal place by default.
    for label in ["#1 · 14.2 ppm", "#4 · 170.7 ppm", "#5 · 20.9 ppm"] {
        assert!(out.contains(&format!(">{label}</text>")), "{label}");
    }
    // The journal style: the value alone.
    let value_args = svg_args.map(|arg| if arg == "both" { "value" } else { arg });
    let (code, out, err) = call(&value_args).await;
    assert_eq!(code, SUCCESS, "{err}");
    for label in ["14.2", "170.7", "20.9"] {
        assert!(out.contains(&format!(">{label}</text>")), "{label}");
    }
    assert!(!out.contains(" ppm<"), "{out}");
    let cdxml_args = [
        "convert",
        "--smiles",
        "CCOC(C)=O",
        "--to",
        "cdxml",
        "--nucleus",
        "1H",
        "--labels",
        "ppm",
    ];
    let (code, out, err) = call(&cdxml_args).await;
    assert_eq!(code, SUCCESS, "{err}");
    // 1H labels get two.
    assert!(out.contains("AtomNumber=\"1.28 ppm\""), "{out}");
    let (code, out, err) = call(&[
        "convert",
        "--smiles",
        "CCOC(C)=O",
        "--to",
        "cdxml",
        "--nucleus",
        "13C",
        "--labels",
        "ppm",
        "--decimals",
        "3",
    ])
    .await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(out.contains("AtomNumber=\"170.700 ppm\""), "{out}");
    let (code, out, err) = call(&[
        "convert",
        "--smiles",
        "CCO.CC",
        "--to",
        "svg",
        "--nucleus",
        "1H",
        "--labels",
        "numbers",
    ])
    .await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty(), "{out}");
    assert_eq!(
        err,
        "reshiki: error[failed]: NMR prediction needs exactly one molecule; this drawing has 2\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn analyze_json_preserves_rdkit_formula_and_adds_display_formula() {
    let (code, out, err) = call(&["analyze", "--smiles", "[13CH3][2H]"]).await;
    assert_eq!(code, SUCCESS, "{err}");
    let result: Value = serde_json::from_str(&out).unwrap();
    let analysis = &result["value"]["analysis"];
    assert_eq!(analysis["formula"], "CH4");
    assert_eq!(analysis["display_formula"], "¹³CH₃²H");
    assert!(analysis.get("mass_percent").is_none());
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
                locants: false,
                isotope_style: crate::naming::IsotopeStyle::Iupac,
            })),
            "{tokens:?}"
        );
    }
}

#[test]
fn name_locants_implies_json_output_and_rejects_duplicate_flags() {
    assert_eq!(
        parsed(&["name", "--smiles", "OCC", "--locants"]),
        Ok(Command::Name(Name {
            input: Input {
                source: Source::Smiles("OCC".into()),
                format: "smiles".into(),
            },
            json: false,
            locants: true,
            isotope_style: crate::naming::IsotopeStyle::Iupac,
        }))
    );
    assert_eq!(
        parsed(&["name", "--smiles", "C", "--locants", "--locants"]),
        Err("--locants given more than once".into())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn name_locants_reports_parent_atoms_in_imported_atom_order() {
    for flags in [vec!["--locants"], vec!["--json", "--locants"]] {
        let mut tokens = vec!["name", "--smiles", "OCC"];
        tokens.extend(flags);
        let (code, out, err) = call(&tokens).await;
        assert_eq!(code, SUCCESS, "{err}");
        assert!(err.is_empty(), "{err}");
        let result: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(result["value"]["name"], "ethanol");
        assert_eq!(result["value"]["locants"], json!([null, "1", "2"]));
        // The JSON line is wrapped like the other commands' results.
        assert_eq!(result["warnings"], json!([]));
        assert_eq!(result["versions"]["app"], crate::updates::CURRENT_VERSION);
        assert_eq!(result["versions"]["document"], crate::document::VERSION);
        assert_eq!(result["experimental"], true);
    }
}

#[test]
fn isotope_style_command_line_is_explicit_and_checked() {
    for (value, expected) in [
        ("iupac", crate::naming::IsotopeStyle::Iupac),
        ("boughton", crate::naming::IsotopeStyle::Boughton),
    ] {
        let Ok(Command::Name(name)) =
            parsed(&["name", "--smiles", "[2H]C", "--isotope-style", value])
        else {
            panic!("{value}")
        };
        assert_eq!(name.isotope_style, expected);
    }
    assert_eq!(
        parsed(&["name", "--smiles", "C", "--isotope-style", "cas"]),
        Err("--isotope-style must be iupac or boughton".into())
    );
    assert!(parsed(&["name", "--smiles", "C", "--isotope-style"]).is_err());
    assert!(
        parsed(&[
            "name",
            "--smiles",
            "C",
            "--isotope-style",
            "iupac",
            "--isotope-style",
            "boughton"
        ])
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn name_prints_one_plain_name_without_a_banner() {
    let (code, out, err) = call(&["name", "--smiles", "CC(=O)O"]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert_eq!(out, "acetic acid\n");
    assert!(err.is_empty(), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn name_preserves_unspecified_smiles_double_bonds() {
    for (smiles, expected) in [
        ("CC=CC", "but-2-ene"),
        ("CCC=CC", "pent-2-ene"),
        ("C/C=C/C=CC", "(4E)-hexa-2,4-diene"),
        ("FC=CCl", "1-chloro-2-fluoroethene"),
        ("C1=CCCCCCC1", "cyclooctene"),
        ("C/C=C/C", "(2E)-but-2-ene"),
        (r"C/C=C\C", "(2Z)-but-2-ene"),
        ("C1=CCCCC1", "cyclohexene"),
        ("C1=CCCCCC1", "cycloheptene"),
        ("CC=C=C=CC", "hexa-2,3,4-triene"),
        ("C/C=C=C=CC", "hexa-2,3,4-triene"),
        ("CC=C=C=C/C", "hexa-2,3,4-triene"),
        ("CC=C=CC", "penta-2,3-diene"),
        ("C/C=C=C/C", "penta-2,3-diene"),
        ("C/C=C=C=C=C/C", "hepta-2,3,4,5-tetraene"),
        ("C/C=C/C=C/C", "(2E,4E)-hexa-2,4-diene"),
        // Cumulenes with identical ligands at one end have no E/Z.
        ("C/C(C)=C=C=C/C", "2-methylhexa-2,3,4-triene"),
        ("C/C(C)=C=C=C(/C)C", "2,5-dimethylhexa-2,3,4-triene"),
        ("[CH3:1]/C([CH3:2])=C=C=C/C", "2-methylhexa-2,3,4-triene"),
        ("[2H]/C([2H])=C=C=C/C", "(1,1-2H2)penta-1,2,3-triene"),
        (
            "C/C=C/C(/C=C/C)=C=C=C/C=C/C",
            "(2E,8E)-4-[(1E)-prop-1-en-1-yl]deca-2,4,5,6,8-pentaene",
        ),
    ] {
        let (code, out, err) = call(&["name", "--smiles", smiles, "--json"]).await;
        assert_eq!(code, SUCCESS, "{smiles}: {err}");
        assert!(err.is_empty(), "{smiles}: {err}");
        let result: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(result["value"]["name"], expected, "{smiles}: {out}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn name_rejects_specified_cumulene_ez_without_printing_a_name() {
    for smiles in [
        "C/C=C=C=C/C",
        r"C/C=C=C=C\C",
        "C/C=C=C=C=C=C/C",
        r"C/C=C=C=C=C=C\C",
        "[2H]/C=C=C=C/C",
        r"[2H]/C=C=C=C\C",
        "[3H]/C=C=C=C/C",
        "[2H]/C=C=C=C=C=C/C",
        "[2H]/C([H])=C=C=C/C",
        "F/C=C=C=C/Cl",
        "C/C(Cl)=C=C=C(/C)F",
        "C(=C=C=C/C)/C",
        r"C/C=C/C(=C=C=C/F)/C=C\C",
        r"C/C=C/C(=C=C=C\F)/C=C\C",
        "[H]/N=C=C=C/C",
        r"[H]/N=C=C=C\C",
        "C/N=C=C=C/C",
        "C/C=C=C=C/C.CC",
        "C/1=C=C=C/CCCC1",
        "C/C=C=C=c1/cccc[nH]1",
        "C/C=C=C=c1cccc[nH]/1",
    ] {
        for json in [false, true] {
            let mut tokens = vec!["name", "--smiles", smiles];
            if json {
                tokens.push("--json");
            }
            let (code, out, err) = call(&tokens).await;
            assert_eq!(code, FAILURE, "{smiles}: {err}");
            assert!(out.is_empty(), "{smiles}: {out}");
            assert!(
                err.contains("Naming does not support cumulene E/Z configuration yet; no less specific name was substituted"),
                "{smiles}: {err}"
            );
        }
    }
    let (code, out, err) = call(&["name", "--smiles", "CC=[C@AL1]=CC", "--json"]).await;
    assert_eq!(code, FAILURE, "{err}");
    assert!(out.is_empty(), "{out}");
    assert!(
        err.contains("Naming does not support allene or coordination stereo classes"),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn name_checks_raw_smiles_files_including_auto_format() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cumulene.smi");
    std::fs::write(&path, "C/C=C=C=C/C cumulene\n").unwrap();
    for format in [None, Some("smiles"), Some("auto")] {
        let mut tokens = vec!["name", path.to_str().unwrap(), "--json"];
        if let Some(format) = format {
            tokens.extend(["--from", format]);
        }
        let (code, out, err) = call(&tokens).await;
        assert_eq!(code, FAILURE, "{format:?}: {err}");
        assert!(out.is_empty(), "{format:?}: {out}");
        assert!(
            err.contains("Naming does not support cumulene E/Z configuration yet"),
            "{format:?}: {err}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn name_json_reports_preferred_and_nonpreferred_rule_paths() {
    use crate::naming::{NameReason, NameStatus};

    for (smiles, expected, status, reason, aliases, alternatives) in [
        (
            "CCO",
            "ethanol",
            NameStatus::Preferred,
            Value::Null,
            json!(["ethanol", "ethyl alcohol"]),
            json!([]),
        ),
        (
            "CC(=O)[O-].[Na+]",
            "sodium acetate",
            NameStatus::Preferred,
            Value::Null,
            json!([]),
            json!([]),
        ),
        (
            "[13CH3]CO",
            "(2-13C)ethan-1-ol",
            NameStatus::Unverified(NameReason::IsotopePlacement),
            json!("isotope_placement"),
            json!([]),
            json!([]),
        ),
        (
            "O=[PH]1C=CC1",
            "1lambda^5-phosphet-1(2H)-one",
            NameStatus::Unverified(NameReason::LambdaParentHydrogen),
            json!("lambda_parent_hydrogen"),
            json!([]),
            json!([]),
        ),
        (
            "CC(=NN)C",
            "propan-2-one hydrazone",
            NameStatus::NotPreferred(NameReason::FunctionalClass),
            json!("functional_class"),
            json!([]),
            json!([]),
        ),
        (
            "C1Cc2ccc(cc2)CCc2ccc1cc2",
            "tricyclo[8.2.2.2^{4,7}]hexadeca-1(12),4,6,10,13,15-hexaene",
            NameStatus::NotPreferred(NameReason::Phane),
            json!("phane"),
            json!([]),
            json!([{"name": "1,4(1,4)-dibenzenacyclohexaphane", "status": "unverified", "reason": "phane"}]),
        ),
        (
            "C1Cc2cccc(c2)CCc2cccc1c2",
            "tricyclo[9.3.1.1^{4,8}]hexadeca-1(15),4(16),5,7,11,13-hexaene",
            NameStatus::NotPreferred(NameReason::Phane),
            json!("phane"),
            json!([]),
            json!([{"name": "1,4(1,3)-dibenzenacyclohexaphane", "status": "unverified", "reason": "phane"}]),
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
        assert_eq!(value["aliases"], aliases, "{smiles}");
        assert_eq!(value["alternatives"], alternatives, "{smiles}");
        assert_eq!(result["experimental"], true);
        assert_eq!(result["api"]["stability"], "experimental");
        assert_eq!(
            result["api"]["operation_api"],
            crate::envelope::OPERATION_API_VERSION
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn name_json_keeps_biochemical_aliases_stereospecific() {
    for (smiles, aliases) in [
        ("N[C@@H](C)C(=O)O", json!(["L-alanine"])),
        ("N[C@H](C)C(=O)O", json!([])),
        ("NC(C)C(=O)O", json!([])),
    ] {
        let (code, out, err) = call(&["name", "--smiles", smiles, "--json"]).await;
        assert_eq!(code, SUCCESS, "{smiles}: {err}");
        let result: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(result["value"]["aliases"], aliases, "{smiles}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn name_declined_inputs_fail_without_stdout() {
    // Import would drop the allene stereo; the raw SMILES check declines it.
    for smiles in ["CC.O", "[13CH3][NH]", "FC=[C@AL1]=CF"] {
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

#[tokio::test(flavor = "multi_thread")]
async fn name_boughton_option_controls_the_generated_name_and_status() {
    let (code, out, err) = call(&[
        "name",
        "--smiles",
        "[2H]C([2H])([2H])O[2H]",
        "--isotope-style",
        "boughton",
        "--json",
    ])
    .await;
    assert_eq!(code, SUCCESS, "{err}");
    let result: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(result["value"]["name"], "methanol-d4");
    assert_eq!(result["value"]["status"], "not_preferred");
    assert_eq!(result["value"]["reason"], "boughton_isotope");
}

#[test]
fn render_command_lines_parse() {
    let render = |source: Source, from: &str, to: &str, output: Output| Render {
        input: Input {
            source,
            format: from.into(),
        },
        preview: Preview {
            format: to.into(),
            svg_text: SvgText::Outline,
            output,
            width: None,
            height: None,
            force: false,
        },
    };
    let mut sized = render(Source::Stdin, "mol", "png", Output::Stdout);
    sized.preview.width = Some(640);
    sized.preview.height = Some(480);
    sized.preview.force = true;
    // The render tool checks the size's range.
    let mut zero = render(Source::File("a.mol".into()), "mol", "svg", Output::Stdout);
    zero.preview.width = Some(0);
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
            sized,
        ),
        (&["render", "a.mol", "--to", "svg", "--width", "0"], zero),
    ] {
        assert_eq!(parsed(tokens), Ok(Command::Render(expected)), "{tokens:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn svg_text_defaults_to_outlines_and_live_keeps_labels() {
    let directory = tempfile::tempdir().unwrap();
    let proposal = directory.path().join("proposal.json");
    std::fs::write(&proposal, include_str!("skill/esterification.json")).unwrap();
    for command in ["convert", "render", "compose"] {
        let input = if command == "compose" {
            vec![proposal.clone().into_os_string()]
        } else {
            vec!["--smiles".into(), "CCO".into()]
        };
        for live in [false, true] {
            let mut tokens = vec![command.into()];
            tokens.extend(input.clone());
            tokens.extend(["--to".into(), "svg".into()]);
            if live {
                tokens.extend(["--svg-text".into(), "live".into()]);
            }
            let (code, svg, err) = call_os(tokens).await;
            assert_eq!(code, SUCCESS, "{command}, live={live}: {err}");
            assert_eq!(svg.contains("<text"), live, "{command}: {svg}");
            if !live {
                assert!(svg.contains("<path"), "{command}: {svg}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn outlined_svg_receipt_counts_the_written_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("drawing.svg");
    let (code, out, err) = call_os(vec![
        "convert".into(),
        "--smiles".into(),
        "CCO".into(),
        "-o".into(),
        path.clone().into_os_string(),
        "--receipt".into(),
    ])
    .await;
    assert_eq!(code, SUCCESS, "{err}");
    let bytes = std::fs::read(path).unwrap();
    let receipt: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(receipt["receipt"]["byte_len"], json!(bytes.len()));
    assert!(!String::from_utf8(bytes).unwrap().contains("<text"));
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_svg_text_is_a_usage_error_for_every_svg_command() {
    for command in ["convert", "render", "compose"] {
        let (code, out, err) = call(&[
            command,
            "missing.file",
            "--to",
            "svg",
            "--svg-text",
            "invalid",
        ])
        .await;
        assert_eq!(code, USAGE, "{command}");
        assert!(out.is_empty(), "{command}: {out}");
        assert_eq!(err, "reshiki: --svg-text must be live or outline\n");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn steroid_alpha_beta_survives_cleanup_exchange_and_cli_render() {
    use crate::{
        chemistry::{self, document as molecular},
        document::Document,
    };

    for bytes in [
        include_bytes!("../../crates/model/fonts/LiberationSans-Regular.ttf").as_slice(),
        include_bytes!("../../crates/model/fonts/LiberationSans-Italic.ttf").as_slice(),
    ] {
        crate::style::add_font(bytes.to_vec()).unwrap();
    }
    let parsed = nazuchem_n2s::Parser::new().unwrap().parse(
        "estra-1,3,5(10)-triene-3,17beta-diol",
        &nazuchem_n2s::ParseOptions::default(),
    );
    let smiles = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
    let input = chemistry::smiles::read(&smiles).unwrap();
    let state = input.prepared.state;
    let positions = chemistry::depict::compute(
        &state,
        &vec![None; state.graph.atoms.len()],
        None,
        Default::default(),
    )
    .unwrap()
    .positions;
    let molecule = molecular::Molecule {
        rdkit_version: chemistry::RDKIT_VERSION,
        ids: (1..=positions.len() as u64).collect(),
        state,
        positions,
    };
    let drawing = molecular::for_import(&molecule, false, &input.prepared.dummy_labels).unwrap();
    let labels = drawing.labels().unwrap();
    let mut doc = drawing.finish(labels).unwrap();
    doc.atom_labels.stereo = true;
    doc.atom_labels.steroid_alpha_beta = true;
    doc.atom_labels.roman_stereo_parentheses = true;
    doc.drawing_style.font_family = "Liberation Sans".into();
    for atom in &mut doc.atoms {
        atom.display.stereo.style.family = "Liberation Sans".into();
    }
    let mut request = crate::engine::Request::molecule("clean", doc);
    request.cleanup = Some(crate::cleanup::Options {
        iupac_orientation: true,
        ..Default::default()
    });
    let engine = crate::engine::LocalEngine::in_process();
    doc = engine.request(request).await.unwrap().document.unwrap();
    assert!(
        doc.atoms
            .iter()
            .any(|atom| atom.steroid_label.as_deref() == Some("17β"))
    );
    for format in ["cdxml", "cdx"] {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let mut request = crate::engine::Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let output = engine.request(request).await.unwrap().output.unwrap();
        let xml = if format == "cdx" {
            crate::exchange::from_cdx(&STANDARD.decode(output).unwrap()).unwrap()
        } else {
            output
        };
        assert!(xml.contains("17β"), "{xml}");
        assert!(!xml.contains("(17β)"));
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("estradiol.rsk");
    std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
    assert!(
        Document::from_native_file(&std::fs::read(&path).unwrap())
            .unwrap()
            .atoms
            .iter()
            .all(|atom| atom.steroid_label.is_none())
    );
    let (code, svg, err) = call_os(vec![
        "render".into(),
        path.into_os_string(),
        "--to".into(),
        "svg".into(),
        "--svg-text".into(),
        "live".into(),
    ])
    .await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(svg.contains("17β"), "{svg}");
    assert!(!svg.contains("(17β)"));
}

#[tokio::test(flavor = "multi_thread")]
async fn undefined_xi_survives_native_reload_and_figure_exports() {
    use crate::document::{Document, Point};

    for bytes in [
        include_bytes!("../../crates/model/fonts/LiberationSans-Regular.ttf").as_slice(),
        include_bytes!("../../crates/model/fonts/LiberationSans-Italic.ttf").as_slice(),
    ] {
        crate::style::add_font(bytes.to_vec()).unwrap();
    }
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    for (element, position) in [
        ("F", Point::new(28., 0.)),
        ("Cl", Point::new(-14., -24.)),
        ("Br", Point::new(-14., 24.)),
    ] {
        let id = doc.add_atom(element, position);
        doc.add_bond(center, id, 1, "plain");
    }
    doc.atom_labels.stereo = true;
    doc.atom_labels.mark_undefined_stereocentres = true;
    doc.drawing_style.font_family = "Liberation Sans".into();
    doc.atom_mut(center).unwrap().display.stereo.style.family = "Liberation Sans".into();
    let engine = crate::engine::LocalEngine::in_process();
    let checked = crate::export::checked_document(&engine, doc.clone())
        .await
        .unwrap();
    let question_pdf = crate::export::drawing(&checked, "pdf").unwrap();
    doc.atom_labels.undefined_xi = true;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("undefined.rsk");
    for roman in [false, true] {
        doc.atom_labels.roman_stereo_parentheses = roman;
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let (code, svg, err) = call_os(vec![
            "render".into(),
            path.clone().into_os_string(),
            "--to".into(),
            "svg".into(),
            "--svg-text".into(),
            "live".into(),
        ])
        .await;
        assert_eq!(code, SUCCESS, "{err}");
        let xml = roxmltree::Document::parse(&svg).unwrap();
        let labels: String = xml
            .descendants()
            .filter(|node| node.is_text())
            .filter_map(|node| node.text())
            .collect();
        assert!(labels.contains('Ξ'), "{svg}");
        assert!(!labels.contains('?'), "{svg}");
        let opened = Document::from_native_file(&std::fs::read(&path).unwrap()).unwrap();
        let checked = crate::export::checked_document(&engine, opened)
            .await
            .unwrap();
        let pdf = crate::export::drawing(&checked, "pdf").unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert_ne!(pdf, question_pdf);
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
        (&["convert", "a.mol", "--to", "name"], unsupported("name")),
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
            &["analyze", "--smiles", "C", "--nucleus", "1H"],
            "analyze does not take --nucleus".into(),
        ),
        (
            &["render", "--smiles", "C", "--to", "svg", "--nucleus", "1H"],
            "render does not take --nucleus".into(),
        ),
        (
            &["render", "--smiles", "C", "--to", "svg", "--labels", "both"],
            "render does not take --labels".into(),
        ),
        (
            &[
                "convert", "--smiles", "C", "--to", "svg", "--labels", "both",
            ],
            "--labels needs --nucleus 1H or 13C".into(),
        ),
        (
            &["convert", "--smiles", "C", "--to", "svg", "--decimals", "2"],
            "--decimals needs --labels value, numbers, ppm or both".into(),
        ),
        (
            &[
                "convert",
                "--smiles",
                "C",
                "--to",
                "svg",
                "--nucleus",
                "1H",
                "--labels",
                "ppm",
                "--decimals",
                "5",
            ],
            "--decimals must be a whole number from 0 to 4".into(),
        ),
        (
            &[
                "convert",
                "--smiles",
                "C",
                "--to",
                "svg",
                "--nucleus",
                "1H",
                "--labels",
                "ppm",
                "--decimals",
                "-1",
            ],
            "--decimals must be a whole number from 0 to 4".into(),
        ),
        (
            &["convert", "--smiles", "C", "--to", "svg", "--nucleus", "1H"],
            "--nucleus needs --labels value, numbers, ppm or both".into(),
        ),
        (
            &[
                "convert",
                "--smiles",
                "C",
                "--to",
                "svg",
                "--nucleus",
                "1H",
                "--labels",
                "atom",
            ],
            "--labels must be value, numbers, ppm or both".into(),
        ),
        (
            &[
                "convert",
                "--smiles",
                "C",
                "--to",
                "svg",
                "--nucleus",
                "H1",
                "--labels",
                "ppm",
            ],
            "--nucleus must be 1H or 13C".into(),
        ),
        (
            &[
                "convert",
                "--smiles",
                "C",
                "--to",
                "mol",
                "--nucleus",
                "1H",
                "--labels",
                "ppm",
            ],
            "--labels needs an output that keeps atom numbers: svg, png, pdf, cdxml, cdx".into(),
        ),
        (
            &["predict-nmr", "--smiles", "C"],
            "predict-nmr needs --nucleus 1H or 13C".into(),
        ),
        (
            &["predict-nmr", "--smiles", "C", "--nucleus", "H1"],
            "--nucleus must be 1H or 13C".into(),
        ),
        (
            &["predict-nmr", "--nucleus", "1H"],
            "predict-nmr needs an input: a file, - for standard input, or --smiles TEXT".into(),
        ),
        (
            &[
                "predict-nmr",
                "--smiles",
                "C",
                "--nucleus",
                "1H",
                "--to",
                "svg",
            ],
            "predict-nmr does not take --to".into(),
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
        assert_eq!(
            err,
            format!("reshiki: error[invalid_arguments]: {message}\n")
        );
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
        "reshiki: error[failed]: This structure has too much stereochemistry to process · Try a shorter chain or a fragment\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn binary_output_never_goes_to_a_terminal() {
    for tokens in [
        &["render", "--smiles", "C", "--to", "png"][..],
        &["pieces", "render", "unused.reshiki-piece", "--to", "png"],
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
        "predict-nmr",
        include_str!("../../tests/fixtures/cli-help/predict-nmr.txt"),
    ),
    (
        "export-input",
        include_str!("../../tests/fixtures/cli-help/export-input.txt"),
    ),
    (
        "name",
        include_str!("../../tests/fixtures/cli-help/name.txt"),
    ),
    (
        "calc",
        include_str!("../../tests/fixtures/cli-help/calc.txt"),
    ),
    ("ts", include_str!("../../tests/fixtures/cli-help/ts.txt")),
    (
        "pieces",
        include_str!("../../tests/fixtures/cli-help/pieces.txt"),
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
fn convert_help_lists_all_label_formats() {
    let help = help(commands::find("convert"));
    let (_, labels) = help
        .split_once("  --nucleus NUCLEUS --labels FORMAT\n")
        .unwrap();
    let (labels, _) = labels.split_once("\n  --").unwrap();
    for format in ["value", "numbers", "ppm", "both"] {
        assert!(
            labels.split_whitespace().any(|word| word == format),
            "missing {format} in --labels help"
        );
    }
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

#[tokio::test(flavor = "multi_thread")]
async fn name_json_reports_relative_ring_alternatives() {
    let smiles = "C[C@H]1C[C@@H](C)C1";
    let (code, out, err) = call(&["name", "--smiles", smiles, "--json"]).await;
    assert_eq!(code, SUCCESS, "{err}");
    let result: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(result["value"]["name"], "(1s,3s)-1,3-dimethylcyclobutane");
    assert_eq!(
        result["value"]["alternatives"],
        json!([{
            "name": "cis-1,3-dimethylcyclobutane",
            "status": "not_preferred",
            "reason": "relative_ring_stereo",
        }])
    );
    let parsed = crate::naming::resolve_name(
        "cis-1,3-dimethylcyclobutane",
        crate::naming::Cancel::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        parsed.canonical_smiles,
        crate::naming::canonical_smiles(smiles).unwrap()
    );
    let (code, out, err) = call(&["name", "--smiles", smiles]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert_eq!(out, "(1s,3s)-1,3-dimethylcyclobutane\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn name_json_reports_endo_exo_alternatives() {
    for (smiles, primary, alternative) in [
        (
            "O[C@H]1C[C@@H]2CC[C@H]1C2",
            "(1S,2S,4R)-bicyclo[2.2.1]heptan-2-ol",
            "bicyclo[2.2.1]heptan-2-exo-ol",
        ),
        (
            "O[C@@H]1C[C@@H]2CC[C@H]1C2",
            "(1S,2R,4R)-bicyclo[2.2.1]heptan-2-ol",
            "bicyclo[2.2.1]heptan-2-endo-ol",
        ),
        (
            "CN1[C@@H]2CC[C@H]1C[C@@H](O)C2",
            "(1R,3r,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
            "8-methyl-8-azabicyclo[3.2.1]octan-3-endo-ol",
        ),
        (
            "CN1[C@@H]2CC[C@H]1C[C@H](O)C2",
            "(1R,3s,5S)-8-methyl-8-azabicyclo[3.2.1]octan-3-ol",
            "8-methyl-8-azabicyclo[3.2.1]octan-3-exo-ol",
        ),
    ] {
        let (code, out, err) = call(&["name", "--smiles", smiles, "--json"]).await;
        assert_eq!(code, SUCCESS, "{err}");
        let result: Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(result["value"]["name"], primary);
        assert_eq!(result["value"]["status"], "preferred");
        assert_eq!(result["value"]["preferred"], true);
        assert_eq!(
            result["value"]["alternatives"],
            json!([{
                "name": alternative,
                "status": "not_preferred",
                "reason": "endo_exo_stereo",
            }]),
            "{smiles}"
        );
    }
}

/// A picture ReShiki exported gives back the drawing inside it; one without
/// a drawing says so, where it used to fail as invalid UTF-8 or SMILES.
#[tokio::test(flavor = "multi_thread")]
async fn pictures_convert_from_the_editable_drawing_inside_them() {
    use crate::{
        document::{Document, Point},
        export::{self, Background},
    };
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let oxygen = doc.add_atom("O", Point::new(40., 0.));
    doc.add_bond(carbon, oxygen, 1, "plain");
    let engine = crate::engine::LocalEngine::in_process();
    let directory = tempfile::tempdir().unwrap();
    for format in ["png", "svg", "pdf"] {
        for editable in [true, false] {
            let picture = export::publication(
                &engine,
                doc.clone(),
                format,
                false,
                editable,
                Background::Canvas,
                72,
                export::FILE_PIXELS,
            )
            .await
            .unwrap();
            let path = directory.path().join(format!("picture.{format}"));
            std::fs::write(&path, picture.bytes).unwrap();
            for command in ["convert", "name"] {
                let mut tokens = vec![command.into(), path.clone().into_os_string()];
                if command == "convert" {
                    tokens.extend(["--to".into(), "smiles".into()]);
                }
                let (code, out, err) = call_os(tokens).await;
                if editable {
                    assert_eq!(code, SUCCESS, "{command} {format}: {err}");
                    let expected = if command == "convert" {
                        "CO"
                    } else {
                        "methanol"
                    };
                    assert_eq!(out.trim(), expected, "{command} {format}");
                } else {
                    assert_eq!(code, FAILURE, "{command} {format}");
                    assert_eq!(
                        err, "reshiki: error: This picture has no ReShiki drawing inside\n",
                        "{command} {format}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn pieces_check_reports_each_file_and_fails_on_any_error() {
    let good = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/model/src/equipment/fixtures/rbf-300-29.reshiki-piece"
    );
    let folder = tempfile::tempdir().unwrap();
    let bad = folder.path().join("bad.reshiki-piece");
    std::fs::write(&bad, r#"{"format": "theme"}"#).unwrap();
    let (code, out, err) = call(&["pieces", "check", good]).await;
    assert_eq!((code, err.as_str()), (SUCCESS, ""));
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["pieces"][0]["ok"], true);
    assert_eq!(value["pieces"][0]["id"], "reshiki/rbf-300-29");
    assert_eq!(value["pieces"][0]["hash"].as_str().unwrap().len(), 64);
    let (code, out, err) = call(&["pieces", "check", good, bad.to_str().unwrap()]).await;
    assert_eq!(code, FAILURE);
    assert_eq!(err, "reshiki: error: 1 of 2 piece files did not pass\n");
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        value["pieces"][1]["error"],
        "This is not a ReShiki piece file."
    );
    for (tokens, message) in [
        (
            &["pieces"][..],
            "pieces needs a subcommand: check or render",
        ),
        (
            &["pieces", "list"],
            "unknown pieces subcommand `list`; the subcommands are check and render",
        ),
        (&["pieces", "check"], "pieces check needs at least one FILE"),
        (
            &["pieces", "check", "--json", good],
            "unknown option `--json`",
        ),
    ] {
        let (code, out, err) = call(tokens).await;
        assert_eq!((code, out.as_str()), (USAGE, ""), "{tokens:?}");
        assert_eq!(err, format!("{USAGE_PREFIX}{message}\n"));
    }
}

const BUILTIN_PIECE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/pieces/rbf-300-29.reshiki-piece"
);

#[tokio::test]
async fn pieces_render_writes_svg_to_stdout_and_honors_dark() {
    let (code, light, err) = call(&["pieces", "render", BUILTIN_PIECE]).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(light.starts_with("<svg ") && light.contains("<path "));

    let (code, dark, err) = call(&[
        "pieces",
        "render",
        BUILTIN_PIECE,
        "--dark",
        "--to",
        "svg",
        "-o",
        "-",
    ])
    .await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(dark.starts_with("<svg ") && dark.contains("<path "));
    assert_ne!(light, dark);
}

#[tokio::test]
async fn pieces_render_refuses_a_dark_png() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("dark.png");
    let (code, _, err) = call(&[
        "pieces",
        "render",
        BUILTIN_PIECE,
        "--to",
        "png",
        "--dark",
        "-o",
        path.to_str().unwrap(),
    ])
    .await;
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("--dark needs --to svg"), "{err}");
    assert!(!path.exists());
}

#[tokio::test]
async fn pieces_render_writes_png_and_respects_force() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("preview.png");
    let args = [
        "pieces",
        "render",
        BUILTIN_PIECE,
        "--to",
        "png",
        "-o",
        path.to_str().unwrap(),
    ];
    let (code, out, err) = call(&args).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(out.is_empty());
    assert!(
        std::fs::read(&path)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );

    std::fs::write(&path, b"existing output").unwrap();
    let (code, out, err) = call(&args).await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty());
    assert!(err.contains("exists; pass --force to replace"), "{err}");
    assert_eq!(std::fs::read(&path).unwrap(), b"existing output");

    let (code, out, err) = call(&[args.as_slice(), &["--force"]].concat()).await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(out.is_empty());
    assert!(
        std::fs::read(&path)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
}

#[tokio::test]
async fn pieces_render_reports_the_piece_check_error() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("bad.reshiki-piece");
    std::fs::write(&path, r#"{"format": "theme"}"#).unwrap();
    let (code, out, err) = call(&["pieces", "render", path.to_str().unwrap()]).await;
    assert_eq!((code, out.as_str()), (FAILURE, ""));
    assert_eq!(err, "reshiki: error: This is not a ReShiki piece file.\n");
}

const WATER_XYZ: &str = "3\nwater\nO 0 0 0\nH 0.9572 0 0\nH -0.2399872 0.9266272 0\n";

#[test]
fn export_input_parses_defaults_and_orca_composite_template() {
    use reshiki_io::qc_input::{Job, Program};
    let Command::ExportInput(command) =
        parsed(&["export-input", "water.xyz", "--program", "gaussian"]).unwrap()
    else {
        panic!("expected export-input");
    };
    assert_eq!(command.input, PathBuf::from("water.xyz"));
    assert_eq!(
        (command.program, command.job),
        (Program::Gaussian, Job::OptFreq)
    );
    assert_eq!(command.method, "B3LYP-D3BJ");
    assert_eq!(command.basis, "def2-SVP");
    assert_eq!((command.charge, command.multiplicity), (0, 1));
    assert_eq!((command.cores, command.memory_gb), (8, 16));
    assert_eq!(command.title, None);
    assert_eq!(command.output, Output::Stdout);
    assert_eq!(
        (command.reactant, command.product, command.hessian),
        (None, None, None)
    );
    let Command::ExportInput(command) = parsed(&[
        "export-input",
        "water.xyz",
        "--program",
        "orca",
        "--template",
        "2",
    ])
    .unwrap() else {
        panic!("expected export-input");
    };
    assert_eq!(command.method, "r2SCAN-3c");
    assert!(command.basis.is_empty());
    assert_eq!(command.program, Program::Orca);
}

#[test]
fn export_input_parses_transition_state_paths() {
    use reshiki_io::qc_input::Job;
    for (program, job, expected, options) in [
        ("gaussian", "qst2", Job::Qst2, &[][..]),
        ("gaussian", "qst3", Job::Qst3, &[][..]),
        ("orca", "neb-ts", Job::NebTs, &["--out", "job.inp"][..]),
        (
            "orca",
            "ts",
            Job::TransitionState,
            &["--hessian", "ts.hess", "--out", "job.inp"],
        ),
        (
            "orca",
            "irc",
            Job::Irc,
            &["--hessian", "ts.hess", "--out", "job.inp"],
        ),
    ] {
        let tokens = [
            &[
                "export-input",
                "--program",
                program,
                "--job",
                job,
                "--reactant",
                "reactant.xyz",
                "--product",
                "product.xyz",
                "ts.xyz",
            ][..],
            options,
        ]
        .concat();
        let Command::ExportInput(command) = parsed(&tokens).unwrap() else {
            panic!("expected export-input");
        };
        assert_eq!(command.job, expected);
        assert_eq!(command.reactant, Some(PathBuf::from("reactant.xyz")));
        assert_eq!(command.product, Some(PathBuf::from("product.xyz")));
        assert_eq!(
            command.hessian,
            options
                .contains(&"--hessian")
                .then(|| PathBuf::from("ts.hess"))
        );
    }
}

#[test]
fn export_input_rejects_invalid_options_before_reading() {
    for (options, message) in [
        (
            &["--job", "qst2"][..],
            "QST2 requires --reactant and --product",
        ),
        (
            &["--job", "qst3", "--reactant", "missing-reactant.xyz"],
            "QST3 requires --product",
        ),
        (
            &["--job", "neb-ts", "--product", "missing-product.xyz"],
            "NEB-TS requires --reactant",
        ),
        (
            &["--reactant", "missing-reactant.xyz"],
            "--reactant and --product require a transition-state job",
        ),
        (
            &["--job", "opt", "--product", "missing-product.xyz"],
            "--reactant and --product require a transition-state job",
        ),
        (
            &["--job", "ts", "--hessian", "missing.hess"],
            "--hessian requires --program orca with --job ts or irc",
        ),
        (
            &["--job", "invalid"],
            "--job must be single-point, optimize, opt-freq, transition-state (ts), qst2, qst3, irc or neb-ts",
        ),
        (&["--template", "0"], "--template must be from 1 to 3"),
        (&["--template", "4"], "--template must be from 1 to 3"),
        (
            &["--template", "2"],
            "r2SCAN-3c opt+freq is available only for ORCA",
        ),
        (
            &["--template", "1", "--method", "HF", "--basis", "STO-3G"],
            "--template cannot be combined with --method or --basis",
        ),
        (
            &["--method", "HF"],
            "--method and --basis must be given together",
        ),
        (
            &["--basis", "STO-3G"],
            "--method and --basis must be given together",
        ),
        (&["--cores", "-1"], "--cores must be a whole number"),
        (&["--charge", "0.5"], "--charge must be a whole number"),
        (
            &["--cores", "2", "--cores", "3"],
            "--cores given more than once",
        ),
        (&["--out", ""], "--out needs a file path"),
    ] {
        let tokens = [
            &["export-input", "missing.xyz", "--program", "gaussian"][..],
            options,
        ]
        .concat();
        assert_eq!(parsed(&tokens), Err(message.into()), "{tokens:?}");
    }
    assert_eq!(
        parsed(&["export-input"]),
        Err("export-input needs INPUT.xyz".into())
    );
    assert_eq!(
        parsed(&["export-input", "water.xyz"]),
        Err("--program must be gaussian, orca, gamess or xyz".into())
    );
}

#[tokio::test]
async fn export_input_rejects_misplaced_hessian_and_missing_companion_destination() {
    for (options, message) in [
        (
            &["--job", "opt-freq", "--hessian", "missing.hess"][..],
            "--hessian requires --program orca with --job ts or irc",
        ),
        (
            &["--job", "ts", "--hessian", "missing.hess"],
            "use --out PATH to write the main input and its companion files",
        ),
        (
            &[
                "--job",
                "neb-ts",
                "--reactant",
                "missing-reactant.xyz",
                "--product",
                "missing-product.xyz",
                "--out",
                "-",
            ],
            "use --out PATH to write the main input and its companion files",
        ),
    ] {
        let tokens = [
            &["export-input", "missing.xyz", "--program", "orca"][..],
            options,
        ]
        .concat();
        let (code, out, err) = call(&tokens).await;
        assert_eq!((code, out.as_str()), (USAGE, ""));
        assert!(err.contains(message), "{tokens:?}: {err}");
    }
}

#[tokio::test]
async fn export_input_writes_gaussian_qst_sections_in_order() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("ts.xyz");
    let reactant = directory.path().join("reactant.xyz");
    let product = directory.path().join("product.xyz");
    std::fs::write(&input, WATER_XYZ).unwrap();
    std::fs::write(
        &reactant,
        "3\nreactant\nO 1 0 0\nH 1 1 0\nH 1 0 1\ninvalid second frame\n",
    )
    .unwrap();
    std::fs::write(
        &product,
        "3\nProperties=pos:R:3:species:S:1\n2 0 0 O\n2 1 0 H\n2 0 1 H\ninvalid second frame\n",
    )
    .unwrap();
    for (job, keyword, count) in [("qst2", "Opt=QST2 Freq", 2), ("qst3", "Opt=QST3 Freq", 3)] {
        let (code, out, err) = call(&[
            "export-input",
            "--program",
            "gaussian",
            "--job",
            job,
            "--reactant",
            reactant.to_str().unwrap(),
            "--product",
            product.to_str().unwrap(),
            input.to_str().unwrap(),
        ])
        .await;
        assert_eq!((code, err.as_str()), (SUCCESS, ""));
        assert!(out.contains(keyword), "{out}");
        let sections: Vec<_> = out.split("\n\n").collect();
        assert_eq!(sections[1], "ts reactant");
        assert!(sections[2].contains("1.00000000"));
        assert_eq!(sections[3], "ts product");
        assert!(sections[4].contains("2.00000000"));
        if count == 3 {
            assert_eq!(sections[5], "ts TS guess");
            assert!(sections[6].contains("0.95720000"));
        }
        assert_eq!(out.lines().filter(|line| *line == "0 1").count(), count);
    }

    for bad_product in [
        "2\nproduct\nO 0 0 0\nH 1 0 0\n",
        "3\nproduct\nH 0 0 0\nO 1 0 0\nH 1 1 0\n",
    ] {
        std::fs::write(&product, bad_product).unwrap();
        let (code, out, err) = call(&[
            "export-input",
            input.to_str().unwrap(),
            "--program",
            "gaussian",
            "--job",
            "qst2",
            "--reactant",
            reactant.to_str().unwrap(),
            "--product",
            product.to_str().unwrap(),
        ])
        .await;
        assert_eq!((code, out.as_str()), (FAILURE, ""));
        assert!(
            err.contains("must list the same atoms in the same order"),
            "{err}"
        );
    }
}

#[tokio::test]
async fn export_input_writes_orca_neb_ts_companions() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("ts.xyz");
    let reactant = directory.path().join("reactant.xyz");
    let product = directory.path().join("product.xyz");
    let output = directory.path().join("job.inp");
    std::fs::write(&input, WATER_XYZ).unwrap();
    std::fs::write(&reactant, WATER_XYZ.replace("O 0 0 0", "O 1 0 0")).unwrap();
    std::fs::write(&product, WATER_XYZ.replace("O 0 0 0", "O 2 0 0")).unwrap();
    let (code, out, err) = call(&[
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "orca",
        "--job",
        "neb-ts",
        "--reactant",
        reactant.to_str().unwrap(),
        "--product",
        product.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
    ])
    .await;
    assert_eq!((code, out.as_str(), err.as_str()), (SUCCESS, "", ""));
    let text = std::fs::read_to_string(&output).unwrap();
    for expected in [
        " NEB-TS Freq\n",
        "NEB_End_XYZFile \"job_product.xyz\"",
        "NEB_TS_XYZFile \"job_ts.xyz\"",
    ] {
        assert!(text.contains(expected), "{text}");
    }
    let (_, inline) = text.split_once("* xyz 0 1\n").unwrap();
    assert!(inline.lines().next().unwrap().contains("1.00000000"));
    let product = std::fs::read_to_string(output.with_file_name("job_product.xyz")).unwrap();
    assert!(product.lines().nth(2).unwrap().contains("2.00000000"));
    let guess = std::fs::read_to_string(output.with_file_name("job_ts.xyz")).unwrap();
    assert!(guess.lines().nth(3).unwrap().contains("0.95720000"));
}

#[tokio::test]
async fn export_input_writes_orca_ts_and_irc_with_imported_hessian() {
    use reshiki_io::{qc_input, qcdata::Frame};
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("ts.xyz");
    let hessian = directory.path().join("source.hess");
    let frame = Frame {
        numbers: vec![1, 1],
        positions_angstrom: vec![[0.0, 0.0, 0.0], [0.75, 0.0, 0.0]],
        charge: 1,
        multiplicity: 2,
        isotopes: vec![],
        masses_amu: vec![],
        drawing_atoms: vec![],
    };
    let values: Vec<_> = (0..36).map(|value| f64::from(value - 18) / 16.0).collect();
    std::fs::write(&input, "2\nTS guess\nH 0 0 0\nH 0.75 0 0\n").unwrap();
    std::fs::write(
        &hessian,
        qc_input::write_orca_hess(&frame, &values).unwrap(),
    )
    .unwrap();
    for (job, keywords) in [
        (
            "ts",
            "%geom\n  InHess Read\n  InHessName \"ts_xtb.hess\"\nend",
        ),
        (
            "irc",
            "%irc\n  MaxIter 30\n  InitHess read\n  Hess_Filename \"irc_xtb.hess\"\nend",
        ),
    ] {
        let output = directory.path().join(format!("{job}.inp"));
        let (code, out, err) = call(&[
            "export-input",
            input.to_str().unwrap(),
            "--program",
            "orca",
            "--job",
            job,
            "--charge",
            "1",
            "--multiplicity",
            "2",
            "--hessian",
            hessian.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
        ])
        .await;
        assert_eq!((code, out.as_str(), err.as_str()), (SUCCESS, "", ""));
        let text = std::fs::read_to_string(&output).unwrap();
        assert!(
            text.contains("# TS guess from the Hessian file source.hess (ReShiki "),
            "{text}"
        );
        assert!(text.contains("* xyz 1 2\n"), "{text}");
        assert!(text.contains(keywords), "{text}");
        let companion =
            std::fs::read_to_string(output.with_file_name(format!("{job}_xtb.hess"))).unwrap();
        assert_eq!(qc_input::read_orca_hess(&companion, 2).unwrap(), values);
    }
}

#[tokio::test]
async fn export_input_reports_bad_hessians_without_writing_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("ts.xyz");
    let hessian = directory.path().join("bad.hess");
    let output = directory.path().join("job.inp");
    std::fs::write(&input, "1\nTS guess\nHe 0 0 0\n").unwrap();
    for (text, message) in [
        ("$hessian\n9\n$end\n", "dimension"),
        (
            "$hessian\n3\n0 1 2\n0 NaN 0 0\n1 0 1 0\n2 0 0 1\n$end\n",
            "finite",
        ),
        ("$orca_hessian_file\n$end\n", "$hessian"),
    ] {
        std::fs::write(&hessian, text).unwrap();
        let (code, out, err) = call(&[
            "export-input",
            input.to_str().unwrap(),
            "--program",
            "orca",
            "--job",
            "ts",
            "--hessian",
            hessian.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
        ])
        .await;
        assert_eq!((code, out.as_str()), (FAILURE, ""));
        assert!(err.contains("bad.hess") && err.contains(message), "{err}");
        assert!(!output.exists());
        assert!(!output.with_file_name("job_xtb.hess").exists());
    }
}

#[tokio::test]
async fn export_input_writes_only_the_first_xyz_frame_to_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("water.xyz");
    std::fs::write(&input, format!("{WATER_XYZ}not a valid second frame\n")).unwrap();
    let (code, out, err) = call(&[
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "gaussian",
    ])
    .await;
    assert_eq!((code, err.as_str()), (SUCCESS, ""));
    assert!(out.contains("%chk=water.chk\n"), "{out}");
    assert!(
        out.contains("#p B3LYP/Def2SVP EmpiricalDispersion=GD3BJ Opt Freq\n\nwater\n\n0 1\n"),
        "{out}"
    );
    assert_eq!(
        out.lines().filter(|line| line.starts_with(" H ")).count(),
        2
    );
    assert!(!out.contains("second frame"));
}

#[tokio::test]
async fn export_input_honours_extended_xyz_columns_and_plain_xyz_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("hydrogen.xyz");
    for text in [
        "1\nProperties=species:S:1:charge:R:1:pos:R:3\nH 0.2 0 0 0.74\n",
        "1\nProperties=charge:R:1:pos:R:3:species:S:1\n0.2 0 0 0.74 H\n",
        "1\n\"Properties\"=species:S:1:charge:R:1:pos:R:3\nH 0.2 0 0 0.74\n",
        "1\n\"Properties\"=\"charge:R:1:pos:R:3:species:S:1\"\n0.2 0 0 0.74 H\n",
        "1\nplain XYZ\nH 0 0 0.74\n",
    ] {
        std::fs::write(&input, text).unwrap();
        let (code, out, err) =
            call(&["export-input", input.to_str().unwrap(), "--program", "xyz"]).await;
        assert_eq!((code, err.as_str()), (SUCCESS, ""), "{text}");
        let fields: Vec<_> = out.lines().nth(2).unwrap().split_whitespace().collect();
        assert_eq!(fields[0], "H", "{out}");
        let position: Vec<f64> = fields[1..]
            .iter()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(position, [0.0, 0.0, 0.74], "{text}");
    }
}

#[tokio::test]
async fn export_input_writes_a_file_and_uses_its_stem_for_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("water.xyz");
    let output = directory.path().join("calculation.gjf");
    std::fs::write(&input, WATER_XYZ).unwrap();
    let args = [
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "gaussian",
        "--out",
        output.to_str().unwrap(),
    ];
    let (code, out, err) = call(&args).await;
    assert_eq!((code, out.as_str(), err.as_str()), (SUCCESS, "", ""));
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains("%chk=calculation.chk\n"));
    assert!(written.contains("\nwater\n"));
    std::fs::write(&output, "existing output").unwrap();
    let (code, out, err) = call(&args).await;
    assert_eq!((code, out.as_str(), err.as_str()), (SUCCESS, "", ""));
    assert_eq!(std::fs::read_to_string(&output).unwrap(), written);
}

#[tokio::test]
async fn export_input_sanitizes_generated_file_stems_and_keeps_the_chosen_output_path() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("water sample.xyz");
    std::fs::write(&input, WATER_XYZ).unwrap();
    let args = [
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "gaussian",
    ];
    let (code, out, err) = call(&args).await;
    assert_eq!((code, err.as_str()), (SUCCESS, ""));
    assert!(out.contains("%chk=water_sample.chk\n"), "{out}");
    assert!(out.contains("\nwater sample\n"), "{out}");

    for (name, sanitized) in [
        ("calculation sample.gjf", "calculation_sample"),
        #[cfg(unix)]
        (
            "calculation \t\n\u{2003}\u{7}name.gjf",
            "calculation_____name",
        ),
    ] {
        let output = directory.path().join(name);
        let (code, out, err) =
            call(&[&args[..], &["--out", output.to_str().unwrap()][..]].concat()).await;
        assert_eq!((code, out.as_str(), err.as_str()), (SUCCESS, "", ""));
        let written = std::fs::read_to_string(&output).unwrap();
        assert!(
            written.contains(&format!("%chk={sanitized}.chk\n")),
            "{written}"
        );
        assert!(written.contains("\nwater sample\n"), "{written}");
        assert!(!directory.path().join(format!("{sanitized}.gjf")).exists());
    }
}

#[test]
fn export_input_preflights_every_companion_before_writing_any_file() {
    use reshiki_io::qc_input::File;
    let companions = [
        File {
            name: "calculation_product.xyz".into(),
            text: "new product".into(),
        },
        File {
            name: "calculation_ts.xyz".into(),
            text: "new TS guess".into(),
        },
    ];
    for (existing, companion) in companions.iter().enumerate() {
        for main_exists in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let output = directory.path().join("calculation.inp");
            if main_exists {
                std::fs::write(&output, "existing input").unwrap();
            }
            let existing_path = directory.path().join(&companion.name);
            std::fs::write(&existing_path, "existing companion").unwrap();
            let error = export_input::write_files(&output, "new input", &companions).unwrap_err();
            assert_eq!(
                error,
                format!(
                    "{} already exists; remove it or choose another --out name.",
                    companion.name
                )
            );
            assert_eq!(
                std::fs::read_to_string(&output).ok(),
                main_exists.then(|| "existing input".into())
            );
            assert_eq!(
                std::fs::read_to_string(existing_path).unwrap(),
                "existing companion"
            );
            assert!(
                !directory
                    .path()
                    .join(&companions[1 - existing].name)
                    .exists()
            );
        }
    }
}

#[test]
fn export_input_writes_new_companions_beside_the_chosen_main_path() {
    use reshiki_io::qc_input::File;
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("chosen.inp");
    let companions = [
        File {
            name: "generated_product.xyz".into(),
            text: "new product".into(),
        },
        File {
            name: "generated_ts.xyz".into(),
            text: "new TS guess".into(),
        },
    ];
    std::fs::write(&output, "existing input").unwrap();
    export_input::write_files(&output, "new input", &companions).unwrap();
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "new input");
    for file in &companions {
        assert_eq!(
            std::fs::read_to_string(directory.path().join(&file.name)).unwrap(),
            file.text
        );
    }
}

#[cfg(unix)]
#[test]
fn export_input_preflights_dangling_companion_symlinks() {
    use reshiki_io::qc_input::File;
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("calculation.inp");
    let companion = File {
        name: "calculation_xtb.hess".into(),
        text: "new Hessian".into(),
    };
    let link = directory.path().join(&companion.name);
    std::os::unix::fs::symlink(directory.path().join("missing.hess"), &link).unwrap();
    let error = export_input::write_files(&output, "new input", &[companion]).unwrap_err();
    assert_eq!(
        error,
        "calculation_xtb.hess already exists; remove it or choose another --out name."
    );
    assert!(!output.exists());
    assert!(std::fs::symlink_metadata(link).unwrap().is_symlink());
}

#[tokio::test]
async fn export_input_applies_custom_settings() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("water.xyz");
    std::fs::write(&input, WATER_XYZ).unwrap();
    let (code, out, err) = call(&[
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "orca",
        "--job",
        "single-point",
        "--method",
        "HF",
        "--basis",
        "6-31G(d)",
        "--charge",
        "1",
        "--multiplicity",
        "2",
        "--cores",
        "4",
        "--memory-gb",
        "4",
        "--title",
        "ion",
        "--extra",
        "TightSCF",
    ])
    .await;
    assert_eq!(code, SUCCESS);
    assert_eq!(
        err,
        "warning: ORCA gets only 768 MB per core; raise Memory or lower Cores.\n"
    );
    for expected in [
        "# ion\n",
        "! UHF HF 6-31G(d) SP TightSCF\n",
        "%pal nprocs 4 end\n",
        "%maxcore 768\n",
        "* xyz 1 2\n",
    ] {
        assert!(out.contains(expected), "missing {expected:?}: {out}");
    }
}

#[tokio::test]
async fn export_input_supports_all_programs_and_warns_on_stderr() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("water.xyz");
    std::fs::write(&input, WATER_XYZ).unwrap();
    for (program, template, expected, warning) in [
        ("orca", "2", "! r2SCAN-3c Opt Freq\n", ""),
        (
            "gamess",
            "3",
            " $BASIS GBASIS=N31 NGAUSS=6 NDFUNC=1 $END\n",
            "warning: GAMESS takes the number of cores when you start it (rungms), not from the file.\n",
        ),
        ("xyz", "1", "3\nwater\n", ""),
    ] {
        let (code, out, err) = call(&[
            "export-input",
            input.to_str().unwrap(),
            "--program",
            program,
            "--template",
            template,
        ])
        .await;
        assert_eq!(code, SUCCESS, "{program}: {err}");
        assert_eq!(err, warning, "{program}");
        assert!(out.contains(expected), "{program}: {out}");
    }
    let (code, _, err) = call(&[
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "gamess",
        "--method",
        "MP2",
        "--basis",
        "6-31G(d)",
    ])
    .await;
    assert_eq!(code, SUCCESS);
    assert_eq!(
        err,
        concat!(
            "warning: GAMESS takes the number of cores when you start it (rungms), not from the file.\n",
            "warning: Parallel GAMESS MP2 needs MEMDDI: add a line such as  $SYSTEM MEMDDI=100 $END under Extra keywords.\n",
        )
    );
    let (code, out, err) = call(&[
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "orca",
        "--job",
        "transition-state",
    ])
    .await;
    assert_eq!(code, SUCCESS, "{err}");
    assert!(out.contains("%geom\n  Calc_Hess true\nend\n"), "{out}");
    assert_eq!(
        err,
        "warning: No xTB Hessian: ORCA computes the first Hessian itself.\n"
    );
    let (code, _, err) = call(&[
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "gaussian",
        "--charge",
        "1",
    ])
    .await;
    assert_eq!(code, SUCCESS, "{err}");
    assert_eq!(
        err,
        "warning: Charge 1 and multiplicity 1 do not fit 9 electrons.\n"
    );
}

#[tokio::test]
async fn export_input_reports_writer_errors_and_ignores_xyz_settings() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("water.xyz");
    std::fs::write(&input, WATER_XYZ).unwrap();
    for (options, message) in [
        (
            &["--multiplicity", "0"][..],
            "Multiplicity must be at least 1.",
        ),
        (&["--cores", "0"], "Cores and memory must be at least 1."),
        (&["--method", "", "--basis", ""], "Enter a method."),
    ] {
        let tokens = [
            &[
                "export-input",
                input.to_str().unwrap(),
                "--program",
                "gaussian",
            ][..],
            options,
        ]
        .concat();
        let (code, out, err) = call(&tokens).await;
        assert_eq!(code, FAILURE, "{tokens:?}: {err}");
        assert!(out.is_empty());
        assert_eq!(err, format!("reshiki: error: {message}\n"));
    }
    let (code, out, err) = call(&[
        "export-input",
        input.to_str().unwrap(),
        "--program",
        "xyz",
        "--cores",
        "0",
        "--memory-gb",
        "0",
        "--multiplicity",
        "0",
        "--method",
        "",
        "--basis",
        "",
    ])
    .await;
    assert_eq!((code, err.as_str()), (SUCCESS, ""));
    assert!(out.starts_with("3\nwater\n"));
}

#[tokio::test]
async fn export_input_reports_invalid_xyz_and_missing_files() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("invalid.xyz");
    for (text, message) in [
        (
            "invalid\n",
            "The XYZ input needs an atom count on the first line.",
        ),
        ("1\n", "The XYZ input needs a comment line."),
        (
            "1\nProperties=species:S:1:charge:R:1\nH 0.2 0 0 0.74\n",
            "XYZ Properties needs species:S:1 and pos:R:3 columns.",
        ),
        (
            "1\ncomment\n",
            "Invalid XYZ atom 1: expected an element and finite x y z coordinates.",
        ),
        (
            "1\ncomment\nUnknown 0 0 0\n",
            "Invalid XYZ atom 1: expected an element and finite x y z coordinates.",
        ),
        (
            "1\ncomment\nH NaN 0 0\n",
            "Invalid XYZ atom 1: expected an element and finite x y z coordinates.",
        ),
        ("0\ncomment\n", "The structure has no atoms."),
    ] {
        std::fs::write(&input, text).unwrap();
        let (code, out, err) =
            call(&["export-input", input.to_str().unwrap(), "--program", "xyz"]).await;
        assert_eq!(code, FAILURE, "{text:?}: {err}");
        assert!(out.is_empty());
        assert_eq!(err, format!("reshiki: error: {message}\n"));
    }
    let (code, out, err) = call(&[
        "export-input",
        directory.path().join("missing.xyz").to_str().unwrap(),
        "--program",
        "gaussian",
    ])
    .await;
    assert_eq!(code, FAILURE);
    assert!(out.is_empty());
    assert!(err.starts_with("reshiki: error: cannot read "), "{err}");
}
