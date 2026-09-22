use anyhow::Context;
use reshiki::chemistry::{
    RDKIT_VERSION,
    inchi::{
        INCHI_VERSION,
        key::{self, Error},
    },
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Deserialize)]
struct Header {
    rdkit_version: String,
    inchi_version: String,
    c_long_bytes: usize,
    hard_set_count: usize,
    system_locale: String,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    input: String,
    status: i32,
    key: Option<String>,
    original_locale: bool,
}

#[test]
fn keys_and_errors_match_direct_native_api() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("tests/inchi_key_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut lines = BufReader::new(child.stdout.take().context("Missing native output")?).lines();
    let header: Header = serde_json::from_str(&lines.next().context("Missing native header")??)?;
    assert_eq!(header.rdkit_version, RDKIT_VERSION);
    assert_eq!(header.inchi_version, INCHI_VERSION);
    assert_eq!(header.c_long_bytes, size_of::<std::os::raw::c_long>());
    let mut counts = BTreeMap::new();
    let mut corpus = BTreeSet::new();
    let mut triplets = BTreeSet::new();
    let mut doublets = BTreeSet::new();
    let mut failures = Vec::new();
    let mut cases = 0;
    let mut locale_rejections = 0;
    let mut original_cases = 0;
    for line in lines {
        let case: Case = serde_json::from_str(&line?)?;
        if let Some(expected) = &case.key {
            for start in [0, 3, 6, 9, 15, 18] {
                triplets.insert(
                    expected
                        .get(start..start + 3)
                        .context("Invalid native key")?
                        .to_owned(),
                );
            }
            for start in [12, 21] {
                doublets.insert(
                    expected
                        .get(start..start + 2)
                        .context("Invalid native key")?
                        .to_owned(),
                );
            }
        }
        if let Some(name) = case.name.strip_prefix("hard-set/") {
            corpus.insert(name.to_owned());
        }
        let before = case.input.clone();
        let actual = key::from_inchi(&case.input);
        assert_eq!(case.input, before, "Mutated input: {}", case.name);
        assert_eq!(
            actual,
            key::from_inchi(&case.input),
            "Repeated input: {}",
            case.name
        );
        let non_ascii_initial = ["InChI=1/", "InChI=1S/", "InChI=1B/"]
            .iter()
            .find_map(|prefix| case.input.strip_prefix(prefix))
            .and_then(|body| body.as_bytes().first())
            .is_some_and(|first| !first.is_ascii());
        // Native non-C locales can accept a UTF-8 lead byte and then hash an
        // empty layer. Keep that malformed-input restriction explicit; every
        // C-locale result and every ASCII/system-locale result must match.
        let locale_rejection = case.original_locale
            && non_ascii_initial
            && actual == Err(Error::InvalidInchi)
            && case.status == 0
            && case.key.is_some();
        locale_rejections += usize::from(locale_rejection);
        original_cases += usize::from(case.original_locale);
        let (status, key) = match actual {
            Ok(value) => (0, Some(value)),
            Err(error) => (
                error.native_code().context("Unexpected resource limit")?,
                None,
            ),
        };
        if !locale_rejection && (status, &key) != (case.status, &case.key) && failures.len() < 20 {
            failures.push(format!(
                "{}: {:?}: expected ({}, {:?}), got ({status}, {key:?})",
                case.name, case.input, case.status, case.key
            ));
        }
        *counts.entry(case.status).or_insert(0_usize) += 1;
        cases += 1;
    }
    assert!(child.wait()?.success(), "Native oracle failed");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(cases > 20000, "Missing generated coverage");
    assert_eq!(original_cases * 2, cases, "Missing original locale audit");
    assert_eq!(
        counts.keys().copied().collect::<Vec<_>>(),
        vec![0, 3, 20, 21]
    );
    assert_eq!(corpus.len(), header.hard_set_count);
    assert_eq!(triplets.len(), 16384, "Missing native base-26 triplets");
    assert_eq!(doublets.len(), 512, "Missing native base-26 doublets");
    if std::env::var("RESHIKI_REQUIRE_INCHI_HARD_SET").as_deref() == Ok("1") {
        assert_eq!(corpus.len(), 1181);
    }
    eprintln!(
        "InChIKey: {cases} native cases, repeated twice; statuses {counts:?}; {} hard-set InChIs; all 16,384 triplets and 512 doublets; {locale_rejections} explicit malformed non-ASCII restrictions in original locale {:?}",
        corpus.len(),
        header.system_locale
    );
    Ok(())
}

#[test]
fn input_limit_is_checked_and_does_not_change_input() {
    let prefix = "InChI=1S/CH4/h1H4";
    let exact = format!(
        "{prefix}{}",
        "C".repeat(key::MAX_INPUT_BYTES - prefix.len())
    );
    assert!(key::from_inchi(&exact).is_ok());
    let over = format!("{exact}C");
    assert_eq!(key::from_inchi(&over), Err(Error::Limit));
    assert_eq!(over.len(), key::MAX_INPUT_BYTES + 1);
    assert_eq!(
        key::from_inchi(prefix).as_deref(),
        Ok("VNWKTOKETHGBQD-UHFFFAOYSA-N")
    );
}
