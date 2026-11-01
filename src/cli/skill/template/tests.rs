use super::*;
use crate::cli::{
    SUCCESS,
    skill::{FILES, SKILL_MD},
};
use std::collections::BTreeSet;

/// Spaces, an apostrophe, `$`, `&`, `;` and a backslash.
const HOSTILE_POSIX: &str = r"/tmp/it's a $dir & co; x\y/reshiki";
const HOSTILE_WINDOWS: &str = r"C:\Program Files\O'Brien & Co; $env\reshiki.exe";
/// The two paths as POSIX recipes and PowerShell recipes start.
const POSIX_SH: &str = r"'/tmp/it'\''s a $dir & co; x\y/reshiki'";
const POSIX_POWERSHELL: &str = r"& '/tmp/it''s a $dir & co; x\y/reshiki'";
const WINDOWS_SH: &str = r"'C:\Program Files\O'\''Brien & Co; $env\reshiki.exe'";
const WINDOWS_POWERSHELL: &str = r"& 'C:\Program Files\O''Brien & Co; $env\reshiki.exe'";

/// The recipes after the executable, then the regeneration command.
const RECIPE_TAILS: [&str; 11] = [
    "--cli info",
    "--cli tools",
    "--cli help convert",
    "--cli analyze --smiles 'CC(=O)OCC'",
    "--cli name --smiles 'CC(=O)O' --json",
    "--cli convert --smiles 'CC(=O)OCC' -o ethyl-acetate.mol",
    "--cli convert reaction.cdxml -o reaction.pdf",
    "--cli render reaction.cdxml -o preview-1.png",
    "--cli compose proposal.json -o scheme.svg --receipt",
    "--cli schema proposal",
    "--cli skill -o FOLDER --name reshiki --force",
];

fn inputs(executable: &str) -> SkillInputs {
    SkillInputs {
        name: "reshiki".into(),
        executable: executable.into(),
        app_version: "0.0.0-test".into(),
        operation_api: 7,
        tools: vec![
            ToolSummary {
                name: "info",
                title: "Server info",
            },
            ToolSummary {
                name: "render",
                title: "Render preview",
            },
        ],
        commands: vec![
            CommandSummary {
                name: "convert",
                summary: "Convert a file",
            },
            CommandSummary {
                name: "info",
                summary: "Print versions",
            },
        ],
    }
}

/// The frontmatter's `(key, value)` lines in order, metadata entries as
/// `metadata.key`, each value decoded from its JSON (YAML double-quoted)
/// string; and the body after it.
fn split(text: &str) -> (Vec<(String, String)>, &str) {
    let (frontmatter, body) = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .expect("frontmatter between --- lines");
    let fields = frontmatter
        .lines()
        .filter(|line| *line != "metadata:")
        .map(|line| {
            let (key, value) = line.split_once(": ").expect("key: value");
            let key = match key.strip_prefix("  ") {
                Some(key) => format!("metadata.{key}"),
                None => key.to_owned(),
            };
            let value: String = serde_json::from_str(value).expect("a JSON string value");
            (key, value)
        })
        .collect();
    (fields, body)
}

/// The lines of every fenced block in `language`.
fn block_lines<'a>(text: &'a str, language: &str) -> Vec<&'a str> {
    let opening = format!("```{language}\n");
    text.split(opening.as_str())
        .skip(1)
        .flat_map(|rest| rest.split_once("```").expect("closed fence").0.lines())
        .collect()
}

#[test]
fn frontmatter_has_only_the_portable_fields_as_strings() {
    let text = render_skill_md(&inputs("/opt/reshiki/reshiki"));
    let (fields, _) = split(&text);
    let keys: Vec<_> = fields.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "name",
            "description",
            "license",
            "compatibility",
            "metadata.reshiki-version",
            "metadata.reshiki-operation-api",
            "metadata.generated-by",
        ]
    );
    let value = |key: &str| {
        let (_, value) = fields.iter().find(|(name, _)| name == key).unwrap();
        value.as_str()
    };
    assert_eq!(value("name"), "reshiki");
    assert_eq!(value("license"), "MIT OR Apache-2.0");
    assert_eq!(value("metadata.reshiki-version"), "0.0.0-test");
    assert_eq!(value("metadata.reshiki-operation-api"), "7");
    assert_eq!(value("metadata.generated-by"), "reshiki --cli skill");
    let description = value("description");
    assert!((1..=1024).contains(&description.chars().count()));
    assert!(description.starts_with("Draws, converts, renders and analyzes"));
    for text in [value("name"), description] {
        assert!(!text.contains(['<', '>']), "{text}");
        let lower = text.to_lowercase();
        assert!(!lower.contains("claude") && !lower.contains("anthropic"));
    }
}

#[test]
fn the_executable_stays_out_of_the_frontmatter_however_long() {
    let executable = format!("/{}/reshiki", "a".repeat(4096 - "//reshiki".len()));
    assert_eq!(executable.len(), 4096);
    let text = render_skill_md(&inputs(&executable));
    let (fields, body) = split(&text);
    for (key, value) in &fields {
        assert!(!value.contains(&executable), "{key}");
    }
    let (_, compatibility) = fields
        .iter()
        .find(|(key, _)| key == "compatibility")
        .unwrap();
    assert!(compatibility.chars().count() <= 500);
    assert!(body.contains(&format!("ReShiki executable: `{executable}`\n")));
}

#[test]
fn skill_md_stays_under_300_lines() {
    for executable in ["/opt/reshiki/reshiki", HOSTILE_POSIX, HOSTILE_WINDOWS] {
        let lines = render_skill_md(&inputs(executable)).lines().count();
        assert!(lines <= 300, "{lines} lines");
    }
    let live = SkillInputs::live(
        "reshiki".into(),
        "/opt/reshiki/reshiki".into(),
        "0.0.0-test".into(),
    );
    let lines = render_skill_md(&live).lines().count();
    assert!(lines <= 300, "{lines} lines with the live catalog");
}

#[test]
fn every_relative_link_is_a_generated_file_and_each_one_is_linked() {
    let text = render_skill_md(&inputs("/opt/reshiki/reshiki"));
    let links: BTreeSet<_> = text
        .split("](")
        .skip(1)
        .map(|rest| rest.split_once(')').expect("closed link").0)
        .collect();
    let generated: BTreeSet<_> = FILES.into_iter().filter(|file| *file != SKILL_MD).collect();
    assert_eq!(links, generated);
}

#[test]
fn posix_recipes_single_quote_the_executable() {
    for (executable, start) in [(HOSTILE_POSIX, POSIX_SH), (HOSTILE_WINDOWS, WINDOWS_SH)] {
        let text = render_skill_md(&inputs(executable));
        let expected = RECIPE_TAILS.map(|tail| format!("{start} {tail}"));
        assert_eq!(block_lines(&text, "sh"), expected);
    }
    assert_eq!(posix_word(HOSTILE_POSIX), POSIX_SH);
    assert_eq!(posix_word(HOSTILE_WINDOWS), WINDOWS_SH);
}

#[test]
fn powershell_recipes_call_the_executable_with_doubled_single_quotes() {
    for (executable, start) in [
        (HOSTILE_POSIX, POSIX_POWERSHELL),
        (HOSTILE_WINDOWS, WINDOWS_POWERSHELL),
    ] {
        let text = render_skill_md(&inputs(executable));
        let expected = RECIPE_TAILS.map(|tail| format!("{start} {tail} 2>&1 | Write-Output"));
        assert_eq!(block_lines(&text, "powershell"), expected);
    }
    assert_eq!(powershell_call(HOSTILE_POSIX), POSIX_POWERSHELL);
    assert_eq!(powershell_call(HOSTILE_WINDOWS), WINDOWS_POWERSHELL);
}

#[test]
fn powershell_doubles_typographic_single_quotes_too() {
    assert_eq!(
        powershell_call("l\u{2019}x\u{2018}y\u{201A}\u{201B}"),
        "& 'l\u{2019}\u{2019}x\u{2018}\u{2018}y\u{201A}\u{201A}\u{201B}\u{201B}'"
    );
    assert_eq!(posix_word("/l\u{2019}a/x"), "'/l\u{2019}a/x'");
}

#[test]
fn backticks_in_the_executable_lengthen_code_delimiters() {
    let text = render_skill_md(&inputs("/x/a```b/reshiki"));
    assert!(text.contains("ReShiki executable: ````/x/a```b/reshiki````\n"));
    assert!(text.contains("\n````sh\n'/x/a```b/reshiki' --cli info\n"));
    assert!(text.contains("\n````powershell\n& '/x/a```b/reshiki' --cli info"));
    assert_eq!(code("`a`"), "`` `a` ``");
}

#[test]
fn live_inputs_list_every_tool_and_command() {
    let live = SkillInputs::live(
        "reshiki".into(),
        "/opt/reshiki/reshiki".into(),
        "0.0.0-test".into(),
    );
    assert_eq!(live.operation_api, crate::envelope::OPERATION_API_VERSION);
    let text = render_skill_md(&live);
    let listed = block_lines(&text, "text");
    for spec in SPECS {
        let title = spec.title.unwrap();
        assert!(
            listed
                .iter()
                .any(|line| line.starts_with(&format!("{} ", spec.name))
                    && line.ends_with(&format!("  {title}"))),
            "{}",
            spec.name
        );
    }
    for spec in COMMANDS {
        assert!(
            listed
                .iter()
                .any(|line| line.starts_with(&format!("{} ", spec.name))
                    && line.ends_with(&format!("  {}", spec.summary))),
            "{}",
            spec.name
        );
    }
}

#[test]
fn body_states_the_notices_exit_codes_prefixes_and_staleness_rule() {
    let text = render_skill_md(&inputs("/opt/reshiki/reshiki"));
    for line in [NOTICE, DATA_NOT_INSTRUCTIONS, GUIDE_URL] {
        assert!(text.contains(line), "{line}");
    }
    for status in [SUCCESS, FAILURE, USAGE, RESOURCE_EXIT] {
        assert!(text.contains(&format!("\n| {status} | ")), "{status}");
    }
    for prefix in [ERROR_PREFIX, USAGE_PREFIX, WARNING_PREFIX] {
        assert!(text.contains(&format!("`{prefix}MESSAGE`")), "{prefix}");
    }
    assert!(text.contains("If its `operation_api` is not 7, this skill is out of date"));
    assert!(text.contains("Generated by ReShiki 0.0.0-test for operation API 7."));
}

/// Each lowercase identifier in inline code names a tool, a command, or a
/// property or result field of the live contracts, so a rename there
/// cannot leave the prose behind.
#[test]
fn identifiers_in_inline_code_exist_in_the_live_contracts() {
    let contracts = [
        reshiki_agent::ops::catalog::catalog_json().to_string(),
        reshiki_agent::schema().to_string(),
        reshiki_agent::ops::catalog::RESULTS
            .iter()
            .map(|(name, shape)| format!("{name} {shape}"))
            .collect(),
    ]
    .concat();
    let words: BTreeSet<_> = contracts
        .split(|c: char| !(c.is_ascii_lowercase() || c == '_'))
        .collect();
    let text = render_skill_md(&inputs("/opt/reshiki/reshiki"));
    let mut fenced = false;
    let mut checked = 0;
    for line in text.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        for span in line.split('`').skip(1).step_by(2) {
            if !span.is_empty() && span.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                assert!(words.contains(span), "`{span}` is not in the contracts");
                checked += 1;
            }
        }
    }
    assert!(checked > 20, "{checked} identifiers");
}

/// The shell itself splits each rendered POSIX line: the executable comes
/// back verbatim and the CLI parser accepts the rest.
#[cfg(unix)]
#[test]
fn sh_reads_each_posix_recipe_as_the_executable_and_valid_arguments() {
    use std::ffi::OsString;
    for executable in [HOSTILE_POSIX, HOSTILE_WINDOWS, "/opt/a`b\u{2019}c/reshiki"] {
        let text = render_skill_md(&inputs(executable));
        let lines = block_lines(&text, "sh");
        assert_eq!(lines.len(), RECIPE_TAILS.len());
        for line in lines {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf '%s\\n' {line}"))
                .output()
                .unwrap();
            assert!(output.status.success(), "{line}");
            let words: Vec<_> = String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .map(OsString::from)
                .collect();
            let [program, cli, name, rest @ ..] = words.as_slice() else {
                panic!("{line}");
            };
            assert_eq!(program, executable);
            assert_eq!(cli, "--cli");
            let parsed = crate::cli::args::parse(name, rest);
            assert!(parsed.is_ok(), "{line}: {parsed:?}");
        }
    }
}
