//! Rule-inventoried parity corpora between a hand-written JSON Schema and the
//! Rust decoder that consumes the same JSON. Each contract names its cases
//! file under tests/fixtures/agent-contract; README.md there documents the
//! format, the classification vocabulary and the exemption policy.
use super::json_schema;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// A schema, the decoder that consumes the same JSON, and the cases file
/// (a name under tests/fixtures/agent-contract) that pins their parity.
/// Later contracts add an entry and a cases file only.
pub struct Contract {
    pub name: &'static str,
    pub schema: fn() -> Value,
    pub decode: fn(Value) -> Result<(), String>,
    pub cases: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    BothAccept,
    BothReject,
    SchemaRejectsOnly,
    DecoderRejectsOnly,
}
impl Class {
    fn name(self) -> &'static str {
        match self {
            Self::BothAccept => "both_accept",
            Self::BothReject => "both_reject",
            Self::SchemaRejectsOnly => "schema_rejects_only",
            Self::DecoderRejectsOnly => "decoder_rejects_only",
        }
    }
}

/// How the schema and the decoder each judged one value.
struct Outcome {
    schema: Vec<json_schema::Error>,
    decoded: Result<(), String>,
}
impl Outcome {
    fn new(schema: &Value, decode: fn(Value) -> Result<(), String>, value: &Value) -> Self {
        Self {
            schema: json_schema::validate(schema, value),
            decoded: decode(value.clone()),
        }
    }
    fn class(&self) -> Class {
        match (self.schema.is_empty(), self.decoded.is_ok()) {
            (true, true) => Class::BothAccept,
            (false, false) => Class::BothReject,
            (false, true) => Class::SchemaRejectsOnly,
            (true, false) => Class::DecoderRejectsOnly,
        }
    }
    fn describe(&self) -> String {
        let schema = if self.schema.is_empty() {
            "accepts".to_string()
        } else {
            let shown: Vec<String> = self.schema.iter().take(3).map(|e| e.to_string()).collect();
            let more = self.schema.len().saturating_sub(3);
            let more = if more > 0 {
                format!(" (+{more} more)")
            } else {
                String::new()
            };
            format!("rejects: {}{more}", shown.join(" | "))
        };
        let decoder = match &self.decoded {
            Ok(()) => "accepts".to_string(),
            Err(message) => format!("rejects: {message}"),
        };
        format!("    schema {schema}\n    decoder {decoder}")
    }
}

/// Classify `value`: does the schema accept it, does the decoder?
pub fn classify(schema: &Value, decode: fn(Value) -> Result<(), String>, value: &Value) -> Class {
    Outcome::new(schema, decode, value).class()
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    rules: Vec<Rule>,
    #[serde(default)]
    import: Option<Import>,
    bases: BTreeMap<String, Value>,
    cases: Vec<Case>,
}

/// Reuses a sibling cases file, moved under the JSON pointer `at`: each of
/// its bases and literal case values replaces the value at `at` in a copy of
/// `into`, and each patch path gains `at` as its prefix. Its rules and cases
/// keep their names. `adjust` restates the class and message of the imported
/// cases this contract's decoder judges differently.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Import {
    file: String,
    into: Value,
    at: String,
    #[serde(default)]
    adjust: BTreeMap<String, Adjust>,
}

/// An imported case's class and exact decoder message in this contract.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Adjust {
    expect: Class,
    message: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    id: String,
    cite: String,
    #[serde(default)]
    exempt_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    rule: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default, deserialize_with = "present")]
    value: Option<Value>,
    #[serde(default)]
    patch: Vec<Patch>,
    expect: Class,
    /// The exact decoder error; pins validate() precedence.
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    op: Op,
    path: String,
    #[serde(default, deserialize_with = "present")]
    value: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Op {
    Add,
    Replace,
    Remove,
}

/// A present `null` is `Some(Value::Null)`, not a missing value.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// Test fixtures and cited sources are read directly; tests run outside the
// agent's filesystem grants.
#[allow(clippy::disallowed_methods)]
fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Expand the generators `{"$repeat": v, "count": n}` (an array of n copies),
/// `{"$text": s, "count": n}` (s repeated n times) and `{"$range": [lo, hi]}`
/// (the integers lo..=hi), so boundary values stay readable in the file.
fn expand(value: Value) -> Result<Value, String> {
    Ok(match value {
        Value::Array(items) => {
            Value::Array(items.into_iter().map(expand).collect::<Result<_, _>>()?)
        }
        Value::Object(map) if map.keys().any(|key| key.starts_with('$')) => generate(&map)?,
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| Ok((key, expand(value)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other,
    })
}

fn generate(map: &Map<String, Value>) -> Result<Value, String> {
    let invalid = || format!("invalid generator {}", Value::Object(map.clone()));
    let count = map
        .get("count")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok());
    if let (Some(item), Some(count), 2) = (map.get("$repeat"), count, map.len()) {
        return Ok(Value::Array(vec![expand(item.clone())?; count]));
    }
    if let (Some(text), Some(count), 2) =
        (map.get("$text").and_then(Value::as_str), count, map.len())
    {
        return Ok(Value::String(text.repeat(count)));
    }
    if let (Some(Value::Array(range)), 1) = (map.get("$range"), map.len())
        && let [lo, hi] = range.as_slice()
        && let (Some(lo), Some(hi)) = (lo.as_u64(), hi.as_u64())
    {
        return Ok(Value::Array((lo..=hi).map(Value::from).collect()));
    }
    Err(invalid())
}

/// RFC 6902 add, replace and remove on a JSON pointer.
fn apply(target: &mut Value, patch: &Patch) -> Result<(), String> {
    let value = match (patch.op, &patch.value) {
        (Op::Remove, None) => Value::Null,
        (Op::Add | Op::Replace, Some(value)) => expand(value.clone())?,
        _ => {
            return Err(format!(
                "{:?} {}: value is required for add/replace only",
                patch.op, patch.path
            ));
        }
    };
    let missing = || format!("{:?} {}: no such location", patch.op, patch.path);
    if patch.op == Op::Replace {
        *target.pointer_mut(&patch.path).ok_or_else(missing)? = value;
        return Ok(());
    }
    let (parent, last) = patch.path.rsplit_once('/').ok_or_else(missing)?;
    let last = last.replace("~1", "/").replace("~0", "~");
    match target.pointer_mut(parent).ok_or_else(missing)? {
        Value::Object(map) if patch.op == Op::Add => {
            map.insert(last, value);
        }
        Value::Object(map) => {
            map.remove(&last).ok_or_else(missing)?;
        }
        Value::Array(items) => {
            let index = if last == "-" && patch.op == Op::Add {
                items.len()
            } else {
                last.parse::<usize>().map_err(|_| missing())?
            };
            match patch.op {
                Op::Add if index <= items.len() => items.insert(index, value),
                Op::Remove if index < items.len() => {
                    items.remove(index);
                }
                _ => return Err(missing()),
            }
        }
        _ => return Err(missing()),
    }
    Ok(())
}

/// `path:line[-line][,line…]` references separated by `; `, each naming a
/// repository file that has at least that many lines.
fn check_cite(cite: &str, sources: &mut BTreeMap<String, usize>) -> Result<(), String> {
    for reference in cite.split("; ") {
        let (path, lines) = reference
            .rsplit_once(':')
            .ok_or_else(|| format!("{reference:?} is not path:line"))?;
        let mut highest = 0;
        for span in lines.split(',') {
            for line in span.split('-') {
                let line: usize = line
                    .parse()
                    .map_err(|_| format!("{reference:?} has a bad line {line:?}"))?;
                if line == 0 {
                    return Err(format!("{reference:?} cites line 0"));
                }
                highest = highest.max(line);
            }
        }
        let length = match sources.get(path) {
            Some(length) => *length,
            None => {
                let length = read(&repository().join(path))?.lines().count();
                sources.insert(path.to_string(), length);
                length
            }
        };
        if highest > length {
            return Err(format!(
                "{reference:?} is past the end of {path} ({length} lines)"
            ));
        }
    }
    Ok(())
}

/// serde's own error prefixes. The report folds these into one count per
/// kind so the decoder's own messages stand out.
const SERDE_ERRORS: &[&str] = &[
    "missing field",
    "unknown field",
    "unknown variant",
    "invalid type",
    "invalid value",
    "invalid length",
];

/// Run the contract's gate. `Ok` carries the coverage report, `Err` every
/// failure: keyword allowlist, corpus structure, misclassified cases, wrong
/// precedence messages and rules without boundary coverage.
pub fn check(contract: &Contract) -> Result<String, String> {
    let mut failures = Vec::new();
    let schema = (contract.schema)();
    for error in json_schema::check_keywords(&schema) {
        failures.push(format!("schema keyword allowlist: {error}"));
    }
    let corpus = load(contract.cases)?;
    let sibling = match &corpus.import {
        Some(import) => imported(import, &mut failures)?,
        None => Corpus::default(),
    };
    let mut sources = BTreeMap::new();
    let mut rules = BTreeSet::new();
    for rule in corpus.rules.iter().chain(&sibling.rules) {
        if !rules.insert(rule.id.as_str()) {
            failures.push(format!("rule {:?} is defined twice", rule.id));
        }
        if let Err(problem) = check_cite(&rule.cite, &mut sources) {
            failures.push(format!("rule {:?} cite: {problem}", rule.id));
        }
    }
    let mut bases = BTreeMap::new();
    for (name, base) in corpus.bases.iter().chain(&sibling.bases) {
        match expand(base.clone()) {
            Ok(base) => {
                if bases.insert(name.as_str(), base).is_some() {
                    failures.push(format!("base {name:?} is defined twice"));
                }
            }
            Err(problem) => failures.push(format!("base {name:?}: {problem}")),
        }
    }
    let mut names = BTreeSet::new();
    let mut tally = Tally::default();
    for case in corpus.cases.iter().chain(&sibling.cases) {
        let label = format!("case {:?} (rule {:?})", case.name, case.rule);
        if !names.insert(case.name.as_str()) {
            failures.push(format!("{label}: the name is used twice"));
        }
        if !rules.contains(case.rule.as_str()) {
            failures.push(format!("{label}: unknown rule"));
        }
        let value = match instantiate(case, &bases) {
            Ok(value) => value,
            Err(problem) => {
                failures.push(format!("{label}: {problem}"));
                continue;
            }
        };
        let outcome = Outcome::new(&schema, contract.decode, &value);
        let actual = outcome.class();
        if actual != case.expect {
            failures.push(format!(
                "{label}: expected {}, got {}\n{}",
                case.expect.name(),
                actual.name(),
                outcome.describe()
            ));
        }
        if let Some(expected) = &case.message
            && outcome.decoded.as_ref().err() != Some(expected)
        {
            failures.push(format!(
                "{label}: expected decoder message {expected:?}\n{}",
                outcome.describe()
            ));
        }
        tally.add(case, outcome.decoded.err());
    }
    // An imported rule's coverage is gated by the contract that defines it.
    failures.extend(coverage_failures(&corpus.rules, &tally.covered));
    if !failures.is_empty() {
        return Err(format!(
            "{} contract: {} failure(s)\n{}",
            contract.name,
            failures.len(),
            failures.join("\n")
        ));
    }
    Ok(tally.report(
        contract.name,
        corpus.cases.len() + sibling.cases.len(),
        corpus.rules.len() + sibling.rules.len(),
    ))
}

/// The cases file `file` under tests/fixtures/agent-contract.
fn load(file: &str) -> Result<Corpus, String> {
    let path = repository()
        .join("tests/fixtures/agent-contract")
        .join(file);
    serde_json::from_str(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// The sibling corpus `import` names, moved under `at` and adjusted. An
/// adjustment that names no imported case or changes nothing fails.
fn imported(import: &Import, failures: &mut Vec<String>) -> Result<Corpus, String> {
    let mut sibling = load(&import.file)?;
    if sibling.import.is_some() {
        return Err(format!("{}: imports do not chain", import.file));
    }
    let place = |value: Value| {
        let mut wrapped = import.into.clone();
        *wrapped
            .pointer_mut(&import.at)
            .ok_or_else(|| format!("import {}: no such location", import.at))? = value;
        Ok::<_, String>(wrapped)
    };
    sibling.bases = std::mem::take(&mut sibling.bases)
        .into_iter()
        .map(|(name, base)| Ok((name, place(base)?)))
        .collect::<Result<_, String>>()?;
    let mut adjust: BTreeMap<&str, &Adjust> = import
        .adjust
        .iter()
        .map(|(name, adjust)| (name.as_str(), adjust))
        .collect();
    for case in &mut sibling.cases {
        case.value = case.value.take().map(place).transpose()?;
        for patch in &mut case.patch {
            patch.path.insert_str(0, &import.at);
        }
        if let Some(Adjust { expect, message }) = adjust.remove(case.name.as_str()) {
            if case.expect == *expect && case.message.as_ref() == Some(message) {
                failures.push(format!("adjust {:?} changes nothing", case.name));
            }
            case.expect = *expect;
            case.message = Some(message.clone());
        }
    }
    for name in adjust.keys() {
        failures.push(format!("adjust {name:?} names no imported case"));
    }
    Ok(sibling)
}

/// The case's base or literal value with its patches applied.
fn instantiate(case: &Case, bases: &BTreeMap<&str, Value>) -> Result<Value, String> {
    let mut value = match (&case.base, &case.value) {
        (Some(base), None) => bases
            .get(base.as_str())
            .cloned()
            .ok_or_else(|| format!("unknown base {base:?}"))?,
        (None, Some(value)) => expand(value.clone())?,
        _ => return Err("needs exactly one of base and value".into()),
    };
    for patch in &case.patch {
        apply(&mut value, patch)?;
    }
    Ok(value)
}

/// Each rule needs an accepted (both_accept) case and a rejected case, or an
/// exempt_reason; an exemption on a fully covered rule is stale.
fn coverage_failures(rules: &[Rule], covered: &BTreeMap<String, (bool, bool)>) -> Vec<String> {
    let mut failures = Vec::new();
    for rule in rules {
        let (accepted, rejected) = covered.get(&rule.id).copied().unwrap_or_default();
        match (&rule.exempt_reason, accepted && rejected) {
            (None, false) => failures.push(format!(
                "rule {:?} needs an accepted boundary case and a rejected just-beyond case, or an exempt_reason (accepted: {accepted}, rejected: {rejected})",
                rule.id
            )),
            (Some(_), true) => failures.push(format!(
                "rule {:?} is fully covered; drop its stale exempt_reason",
                rule.id
            )),
            (Some(reason), false) if reason.trim().is_empty() => {
                failures.push(format!("rule {:?} has an empty exempt_reason", rule.id));
            }
            _ => {}
        }
    }
    failures
}

/// Rule coverage plus the class and decoder-message counts for the report.
#[derive(Default)]
struct Tally {
    covered: BTreeMap<String, (bool, bool)>,
    classes: BTreeMap<Class, usize>,
    messages: BTreeMap<String, usize>,
    serde: BTreeMap<&'static str, usize>,
}
impl Tally {
    fn add(&mut self, case: &Case, error: Option<String>) {
        let entry = self.covered.entry(case.rule.clone()).or_default();
        if case.expect == Class::BothAccept {
            entry.0 = true;
        } else {
            entry.1 = true;
        }
        *self.classes.entry(case.expect).or_default() += 1;
        let Some(error) = error else { return };
        match SERDE_ERRORS.iter().find(|kind| error.starts_with(**kind)) {
            Some(kind) => *self.serde.entry(kind).or_default() += 1,
            None => *self.messages.entry(error).or_default() += 1,
        }
    }

    /// Message coverage is reported, not gated.
    fn report(&self, name: &str, cases: usize, rules: usize) -> String {
        let mut report = format!("{name} contract: {cases} cases over {rules} rules;");
        for (class, count) in &self.classes {
            let _ = write!(report, " {} {count}", class.name());
        }
        let serde: Vec<String> = self
            .serde
            .iter()
            .map(|(kind, count)| format!("{kind} {count}"))
            .collect();
        let _ = write!(
            report,
            "\nserde errors: {}\ndecoder messages reached ({} distinct):",
            serde.join(", "),
            self.messages.len()
        );
        for (message, count) in &self.messages {
            let _ = write!(report, "\n  {count:>4} x {message}");
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn byte(value: Value) -> Result<(), String> {
        serde_json::from_value::<u8>(value)
            .map(drop)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn classify_names_all_four_classes() {
        let schema = json!({"type":"integer","minimum":1});
        assert_eq!(classify(&schema, byte, &json!(1)), Class::BothAccept);
        assert_eq!(classify(&schema, byte, &json!("1")), Class::BothReject);
        assert_eq!(classify(&schema, byte, &json!(0)), Class::SchemaRejectsOnly);
        assert_eq!(
            classify(&schema, byte, &json!(300)),
            Class::DecoderRejectsOnly
        );
        assert_eq!(
            classify(&schema, byte, &json!(1.0)),
            Class::DecoderRejectsOnly
        );
    }

    #[test]
    fn generators_expand_recursively() {
        let value =
            json!({"a":{"$repeat":{"b":{"$text":"é","count":2}},"count":2},"c":{"$range":[3,5]}});
        assert_eq!(
            expand(value).unwrap(),
            json!({"a":[{"b":"éé"},{"b":"éé"}],"c":[3,4,5]})
        );
        assert!(expand(json!({"$repeat":1})).is_err());
        assert!(expand(json!({"$unknown":1,"count":1})).is_err());
    }

    #[test]
    fn patches_follow_rfc_6902() {
        let mut value = json!({"a":[1,2],"b":{"c/d":1}});
        let patch = |op, path: &str, value: Option<Value>| Patch {
            op,
            path: path.into(),
            value,
        };
        let steps = [
            patch(Op::Add, "/a/-", Some(json!(3))),
            patch(Op::Add, "/a/0", Some(json!(0))),
            patch(Op::Remove, "/a/1", None),
            patch(Op::Replace, "/b/c~1d", Some(json!(null))),
            patch(Op::Add, "/e", Some(json!({"$text":"x","count":3}))),
            patch(Op::Remove, "/b/c~1d", None),
        ];
        for step in &steps {
            apply(&mut value, step).unwrap();
        }
        assert_eq!(value, json!({"a":[0,2,3],"b":{},"e":"xxx"}));
        for step in [
            patch(Op::Remove, "/missing", None),
            patch(Op::Replace, "/missing", Some(json!(1))),
            patch(Op::Add, "/a/9", Some(json!(1))),
            patch(Op::Add, "/a/0", None),
            patch(Op::Remove, "/a/0", Some(json!(1))),
        ] {
            assert!(
                apply(&mut value, &step).is_err(),
                "{:?} {}",
                step.op,
                step.path
            );
        }
    }

    #[test]
    fn cites_must_name_existing_lines() {
        let mut sources = BTreeMap::new();
        assert_eq!(
            check_cite(
                "crates/agent/Cargo.toml:1-3,5; crates/agent/Cargo.toml:2",
                &mut sources
            ),
            Ok(())
        );
        assert!(check_cite("crates/agent/Cargo.toml:100000", &mut sources).is_err());
        assert!(check_cite("crates/agent/Cargo.toml", &mut sources).is_err());
        assert!(check_cite("crates/agent/Cargo.toml:0", &mut sources).is_err());
        assert!(check_cite("crates/agent/missing.rs:1", &mut sources).is_err());
    }
}
