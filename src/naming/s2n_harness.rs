//! Test-only measurements; Phase 2 changes only `namer` below.
mod generator;

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    path::Path,
    time::{Duration, Instant},
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/s2n");
const BAKEOFF: &str = include_str!("../../tests/fixtures/s2n/bakeoff_corpus.tsv");
const OPENCLATURA: &str = include_str!("../../tests/fixtures/s2n/openclatura_tests.tsv");
const RULES_CASES: &str = include_str!("../../tests/fixtures/s2n/rules_cases.tsv");

fn namer(smiles: &str) -> Result<super::openclatura::Name, String> {
    super::openclatura::name_smiles(smiles)
}

struct Case {
    smiles: String,
    category: String,
    pubchem_name: String,
    golden: String,
}

fn corpus(fast: bool) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut random_index = 0;
    for line in BAKEOFF.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 6, "Malformed bakeoff row: {line}");
        if fields[1] == "pubchem_random" {
            random_index += 1;
            if fast && (random_index - 1) % 10 != 0 {
                continue;
            }
        }
        cases.push(Case {
            smiles: fields[0].into(),
            category: fields[1].into(),
            pubchem_name: fields[3].into(),
            golden: fields[4].into(),
        });
    }
    // Sample within each source category so even small categories are retained.
    let mut indices = BTreeMap::new();
    for line in OPENCLATURA.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "Malformed openclatura row: {line}");
        let category = fields[2].split('/').next().unwrap();
        let index = indices.entry(category).or_insert(0);
        let include = !fast || *index % 5 == 0;
        *index += 1;
        if include {
            cases.push(Case {
                smiles: fields[0].into(),
                category: category.into(),
                pubchem_name: String::new(),
                golden: fields[1].into(),
            });
        }
    }
    cases
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Correct,
    Wrong,
    Declined,
    Unparseable,
}

#[derive(Clone)]
struct Outcome {
    class: Class,
    name: String,
    status: Option<super::NameStatus>,
    expected: String,
    actual: String,
    reason: String,
}

#[derive(Default)]
struct Measurements {
    // Repeated fixture structures and parity still count as separate rows, but
    // need not invoke the same deterministic namer/parser again in one run.
    outcomes: HashMap<String, Outcome>,
    parsed_names: HashMap<String, Result<String, String>>,
    naming_time: Duration,
    parsing_time: Duration,
    naming_calls: usize,
    parsing_calls: usize,
}

impl Measurements {
    fn parse_name(&mut self, name: &str) -> Result<String, String> {
        if let Some(result) = self.parsed_names.get(name) {
            return result.clone();
        }
        let start = Instant::now();
        let result = (|| {
            let request = super::local::verification_request(name).map_err(|e| e.to_string())?;
            let reply = super::worker::respond(&request)?;
            let parsed = super::local::verification_result(name, &reply)
                .map_err(|e| format!("{name}: {e}"))?;
            if parsed.smiles.contains('|') {
                Ok(parsed.smiles)
            } else {
                super::canonical_smiles(&parsed.smiles)
            }
        })();
        self.parsing_time += start.elapsed();
        self.parsing_calls += 1;
        self.parsed_names.insert(name.into(), result.clone());
        result
    }

    /// Classify a name produced elsewhere for `smiles`; empty means declined.
    fn classify_given(&mut self, smiles: &str, name: &str) -> Outcome {
        let mut result = Outcome {
            class: Class::Declined,
            name: name.into(),
            status: None,
            expected: super::canonical_smiles(smiles).unwrap_or_default(),
            actual: String::new(),
            reason: String::new(),
        };
        if name.is_empty() || result.expected.is_empty() {
            return result;
        }
        match self.parse_name(name) {
            Err(reason) => {
                result.class = Class::Unparseable;
                result.reason = reason;
            }
            Ok(actual) => {
                result.class = if actual == result.expected {
                    Class::Correct
                } else {
                    Class::Wrong
                };
                result.actual = actual;
            }
        }
        result
    }

    /// All four checks share the same structural classification. Text parity
    /// is an additional metric: alternative valid names are not wrong graphs.
    fn classify(&mut self, smiles: &str) -> Outcome {
        let expected = match super::canonical_smiles(smiles) {
            Ok(value) => value,
            Err(reason) => {
                return Outcome {
                    class: Class::Declined,
                    name: String::new(),
                    status: None,
                    expected: String::new(),
                    actual: String::new(),
                    reason: format!("Input canonicalization: {reason}"),
                };
            }
        };
        if let Some(result) = self.outcomes.get(&expected) {
            return result.clone();
        }
        let mut result = Outcome {
            class: Class::Declined,
            name: String::new(),
            status: None,
            expected: expected.clone(),
            actual: String::new(),
            reason: String::new(),
        };
        let start = Instant::now();
        let generated = namer(&expected);
        self.naming_time += start.elapsed();
        self.naming_calls += 1;
        match generated {
            Err(reason) => result.reason = reason,
            Ok(name) => {
                for alternative in &name.alternatives {
                    let actual = self
                        .parse_name(&alternative.name)
                        .unwrap_or_else(|error| panic!("{}: {error}", alternative.name));
                    super::reverse::verify_alternative_identity(
                        &expected,
                        &actual,
                        alternative.status,
                    )
                    .unwrap_or_else(|error| panic!("{}: {error}", alternative.name));
                }
                result.status = Some(name.status);
                result.name = name.text;
                match self.parse_name(&result.name) {
                    Err(reason) => {
                        result.class = Class::Unparseable;
                        result.reason = reason;
                    }
                    Ok(actual) => {
                        result.class = if actual == expected {
                            Class::Correct
                        } else {
                            Class::Wrong
                        };
                        result.actual = actual;
                    }
                }
            }
        }
        self.outcomes.insert(expected, result.clone());
        result
    }
}

#[derive(Default)]
struct Counts {
    correct: usize,
    wrong: usize,
    declined: usize,
    unparseable: usize,
    input_unparseable: usize,
    exact: usize,
}

impl Counts {
    fn total(&self) -> usize {
        self.correct + self.wrong + self.declined + self.unparseable
    }

    fn add(&mut self, outcome: &Outcome, golden: Option<&str>) {
        match outcome.class {
            Class::Correct => self.correct += 1,
            Class::Wrong => self.wrong += 1,
            Class::Declined => self.declined += 1,
            Class::Unparseable => self.unparseable += 1,
        }
        // Goldens are openclatura's spelling, which encloses with parentheses only.
        self.exact += usize::from(golden.is_some_and(|g| {
            !g.is_empty() && super::openclatura::nest_enclosing_marks(g) == outcome.name
        }));
    }
}

#[derive(Default)]
struct Report {
    categories: BTreeMap<String, Counts>,
    total: Counts,
    declines: BTreeMap<String, usize>,
    details: Vec<String>,
    rows: Vec<String>,
    status_rows: Vec<String>,
    missing_golden: usize,
}

fn field(text: &str) -> String {
    text.replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

impl Report {
    fn add(&mut self, case: &Case, outcome: &Outcome, golden: Option<&str>) {
        self.categories
            .entry(case.category.clone())
            .or_default()
            .add(outcome, golden);
        self.total.add(outcome, golden);
        self.rows.push(format!(
            "{:?}\t{}\t{}\t{}\t{}\t{}",
            outcome.class,
            case.category,
            field(&case.smiles),
            field(&outcome.name),
            field(golden.unwrap_or_default()),
            field(&outcome.reason),
        ));
        if let Some(status) = outcome.status {
            let status = serde_json::to_value(status).expect("serialize name status");
            self.status_rows.push(format!(
                "{}\t{}\t{}",
                field(&case.smiles),
                status["status"].as_str().expect("name status tag"),
                status["reason"].as_str().unwrap_or_default(),
            ));
        }
        if outcome.class == Class::Declined {
            *self.declines.entry(outcome.reason.clone()).or_default() += 1;
        }
        if matches!(outcome.class, Class::Wrong | Class::Unparseable) {
            self.details.push(format!(
                "{:?}\t{}\tsmiles={}\tname={}\texpected={}\tactual={}\treason={}",
                outcome.class,
                case.category,
                field(&case.smiles),
                field(&outcome.name),
                field(&outcome.expected),
                field(&outcome.actual),
                field(&outcome.reason),
            ));
        }
    }

    fn input_unparseable(&mut self, case: &Case, reason: &str) {
        self.categories
            .entry(case.category.clone())
            .or_default()
            .input_unparseable += 1;
        self.total.input_unparseable += 1;
        self.details.push(format!(
            "InputUnparseable\t{}\tsmiles={}\tname={}\treason={}",
            case.category,
            field(&case.smiles),
            field(&case.pubchem_name),
            field(reason),
        ));
    }

    fn rows(&self) -> impl Iterator<Item = (&str, &Counts)> {
        self.categories
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .chain(std::iter::once(("total", &self.total)))
    }
}

fn percentage(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * count as f64 / total as f64
    }
}

fn render(
    reports: &BTreeMap<&str, Report>,
    measurements: &Measurements,
    elapsed: Duration,
) -> String {
    let mut text = String::from(
        "check\tcategory\tcorrect\twrong\tdeclined\tunparseable\ttotal\tinput_unparseable\tcorrect_%\texact\texact_%\n",
    );
    for (check, report) in reports {
        for (category, counts) in report.rows() {
            writeln!(
                text,
                "{check}\t{category}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{:.2}",
                counts.correct,
                counts.wrong,
                counts.declined,
                counts.unparseable,
                counts.total(),
                counts.input_unparseable,
                percentage(counts.correct, counts.total()),
                counts.exact,
                percentage(counts.exact, counts.total())
            )
            .unwrap();
        }
    }
    writeln!(text, "\nTiming: elapsed={:.3}s namer={:.3}s/{} calls OPSIN_adapter={:.3}s/{} calls (cached within each run)",
        elapsed.as_secs_f64(), measurements.naming_time.as_secs_f64(), measurements.naming_calls,
        measurements.parsing_time.as_secs_f64(), measurements.parsing_calls).unwrap();
    for (check, report) in reports {
        writeln!(
            text,
            "\n{check}: missing golden={} (excluded from parity); top decline reasons:",
            report.missing_golden
        )
        .unwrap();
        let mut reasons: Vec<_> = report.declines.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (reason, count) in reasons.into_iter().take(10) {
            writeln!(text, "{count}\t{}", field(reason)).unwrap();
        }
        for detail in &report.details {
            writeln!(text, "{check}\t{detail}").unwrap();
        }
    }
    text
}

fn ratchet(reports: &BTreeMap<&str, Report>) {
    let path = Path::new(FIXTURES).join("baseline.tsv");
    let mut current = BTreeMap::new();
    let mut text = String::from("check\tcategory\tcorrect\ttotal\n");
    for (check, report) in reports {
        for (category, counts) in report.rows() {
            current.insert(
                (check.to_string(), category.to_string()),
                (counts.correct, counts.total()),
            );
            writeln!(
                text,
                "{check}\t{category}\t{}\t{}",
                counts.correct,
                counts.total()
            )
            .unwrap();
        }
    }
    // Preserve both guarantees: correct chemistry and progress toward the
    // exact openclatura spelling. Text differences never become Wrong graphs.
    // The frozen upstream goldens retain cis/trans ring names. The intentional
    // r/s departures are pinned, with complete identity roundtrips, by
    // ring_r_s_names_intentionally_depart_from_upstream_spelling below.
    // FR-3.5 component changes are likewise pinned by the fr35_* identity tests.
    for (category, counts) in reports["parity"].rows() {
        current.insert(
            ("parity_text".into(), category.into()),
            (counts.exact, counts.total()),
        );
        writeln!(
            text,
            "parity_text\t{category}\t{}\t{}",
            counts.exact,
            counts.total()
        )
        .unwrap();
    }
    if std::env::var("S2N_UPDATE_BASELINE").as_deref() == Ok("1") {
        std::fs::write(&path, text).expect("write s2n baseline");
        println!(
            "Updated {} (zero-Wrong assertion still applies)",
            path.display()
        );
        return;
    }
    let baseline = std::fs::read_to_string(&path)
        .expect("Missing s2n baseline; run with S2N_UPDATE_BASELINE=1");
    let mut failures = Vec::new();
    for line in baseline.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "Malformed baseline row: {line}");
        let key = (fields[0].to_string(), fields[1].to_string());
        let expected: usize = fields[2].parse().unwrap();
        let total: usize = fields[3].parse().unwrap();
        let (correct, actual_total) = current.remove(&key).expect("Baseline category disappeared");
        if actual_total != total || correct < expected {
            failures.push(format!(
                "{key:?}: {correct}/{actual_total}, baseline {expected}/{total}"
            ));
        } else if correct > expected {
            println!(
                "s2n improvement {key:?}: {expected} -> {correct}; raise baseline with S2N_UPDATE_BASELINE=1"
            );
        }
    }
    assert!(
        current.is_empty(),
        "New s2n categories; update baseline: {current:?}"
    );
    assert!(
        failures.is_empty(),
        "s2n baseline regression:\n{}",
        failures.join("\n")
    );
}

fn run(fast: bool) {
    let start = Instant::now();
    let cases = corpus(fast);
    let generated: Vec<_> = generator::generate(if fast { 500 } else { 3_000 })
        .into_iter()
        .map(|(smiles, category)| Case {
            smiles,
            category,
            pubchem_name: String::new(),
            golden: String::new(),
        })
        .collect();
    if fast {
        assert!(cases.len() + generated.len() <= 1_500);
    }
    let mut measurements = Measurements::default();
    let mut reports = BTreeMap::new();
    let mut structure = Report::default();
    let mut parity = Report::default();
    for case in &cases {
        let outcome = measurements.classify(&case.smiles);
        structure.add(case, &outcome, None);
        if case.golden.is_empty() {
            parity.missing_golden += 1;
        } else {
            parity.add(case, &outcome, Some(&case.golden));
        }
    }
    reports.insert("structure", structure);
    reports.insert("parity", parity);
    let mut names = Report::default();
    for case in cases.iter().filter(|c| !c.pubchem_name.is_empty()) {
        match measurements.parse_name(&case.pubchem_name) {
            Ok(smiles) => names.add(case, &measurements.classify(&smiles), None),
            Err(reason) => names.input_unparseable(case, &reason),
        }
    }
    reports.insert("name", names);
    let mut stress = Report::default();
    for case in &generated {
        stress.add(case, &measurements.classify(&case.smiles), None);
    }
    reports.insert("generated", stress);
    let text = render(&reports, &measurements, start.elapsed());
    println!("s2n {} report\n{text}", if fast { "fast" } else { "full" });
    // Only full writes this path: CI may run both tests concurrently.
    if !fast && let Some(path) = std::env::var_os("S2N_REPORT") {
        std::fs::write(path, &text).expect("write S2N_REPORT");
    }
    // Every classified row, for parity and decline analysis.
    if let Some(dir) = std::env::var_os("S2N_DUMP") {
        let mut status_rows = Vec::new();
        for (check, report) in &reports {
            let path = Path::new(&dir).join(format!(
                "{}-{check}.tsv",
                if fast { "fast" } else { "full" }
            ));
            std::fs::write(path, report.rows.join("\n")).expect("write S2N_DUMP");
            status_rows.extend(
                report
                    .status_rows
                    .iter()
                    .map(|row| format!("{check}\t{row}")),
            );
        }
        let path =
            Path::new(&dir).join(format!("{}-status.tsv", if fast { "fast" } else { "full" }));
        std::fs::write(path, status_rows.join("\n")).expect("write S2N_DUMP status");
    }
    if fast {
        ratchet(&reports);
    }
    for (check, report) in &reports {
        assert_eq!(
            report.total.wrong, 0,
            "{check}: wrong structures; see report above"
        );
    }
}

#[test]
fn s2n_fast_subset() {
    run(true);
}

#[test]
fn ring_r_s_names_intentionally_depart_from_upstream_spelling() {
    let mut measurements = Measurements::default();
    for (smiles, expected, alternative) in [
        (
            "C[C@H]1CC[C@@H](C)CC1",
            "(1s,4s)-1,4-dimethylcyclohexane",
            Some("cis-1,4-dimethylcyclohexane"),
        ),
        (
            "CC(C)(C)[C@H]1CC[C@@H](O)CC1",
            "(1s,4s)-4-(tert-butyl)cyclohexanol",
            Some("cis-4-(tert-butyl)cyclohexanol"),
        ),
        (
            "C[C@H]1C[C@H](C)C1",
            "(1r,3r)-1,3-dimethylcyclobutane",
            Some("trans-1,3-dimethylcyclobutane"),
        ),
    ] {
        let name = namer(smiles).unwrap();
        assert_eq!(name.text, expected, "{smiles}");
        assert_ne!(
            name.status,
            super::NameStatus::NotPreferred(super::NameReason::RelativeRingStereo),
            "{smiles}"
        );
        assert_eq!(
            name.alternatives
                .iter()
                .map(|name| name.name.as_str())
                .collect::<Vec<_>>(),
            alternative.into_iter().collect::<Vec<_>>(),
            "{smiles}"
        );
        for alternative in &name.alternatives {
            assert_eq!(
                alternative.status,
                super::NameStatus::NotPreferred(super::NameReason::RelativeRingStereo),
                "{smiles}"
            );
        }
        let identity = super::canonical_smiles(smiles).unwrap();
        for text in std::iter::once(&name.text).chain(name.alternatives.iter().map(|n| &n.name)) {
            assert_eq!(measurements.parse_name(text).unwrap(), identity, "{text}");
        }
    }
}

#[test]
#[ignore = "full s2n corpus and 3,000 generated structures"]
fn s2n_full_corpus() {
    run(false);
}

/// The cases of the removed "ReShiki organic rules 1" namer (`rules_name`,
/// empty where it declined): the port gives exactly `name`, which round-trips,
/// or declines where `name` is empty.
#[test]
fn s2n_rules_cases() {
    let mut measurements = Measurements::default();
    for line in RULES_CASES.lines().skip(1) {
        let [smiles, _, name] = line.split('\t').collect::<Vec<_>>()[..] else {
            panic!("Malformed rules case: {line}");
        };
        match namer(smiles) {
            Ok(actual) => {
                assert_eq!(actual.text, name, "{smiles}");
                let outcome = measurements.classify_given(smiles, name);
                assert_eq!(
                    outcome.class,
                    Class::Correct,
                    "{smiles}: {}",
                    outcome.reason
                );
            }
            Err(reason) => assert!(name.is_empty(), "{smiles} declined: {reason}"),
        }
    }
}

/// Structure round trip of extra SMILES (first column of the `S2N_EXTRA` file),
/// printing every wrong and unparseable row; for probing beyond the fixtures.
#[test]
#[ignore = "needs S2N_EXTRA"]
fn s2n_extra_file() {
    // CI runs every ignored harness test; without a file there is nothing to probe.
    let Some(path) = std::env::var_os("S2N_EXTRA") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("read S2N_EXTRA");
    // S2N_EXTRA_GIVEN round-trips the name in the second column instead of
    // calling the namer, e.g. to measure upstream openclatura's own output.
    let given = std::env::var_os("S2N_EXTRA_GIVEN").is_some();
    let mut measurements = Measurements::default();
    let mut counts = BTreeMap::<_, usize>::new();
    let mut rows = String::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let mut fields = line.split('\t');
        let smiles = fields.next().unwrap_or_default();
        let outcome = if given {
            measurements.classify_given(smiles, fields.next().unwrap_or_default())
        } else {
            measurements.classify(smiles)
        };
        writeln!(
            rows,
            "{:?}\t{smiles}\t{}\t{}\t{}",
            outcome.class,
            outcome.name,
            field(&outcome.reason),
            outcome.actual
        )
        .unwrap();
        *counts.entry(format!("{:?}", outcome.class)).or_default() += 1;
        if matches!(outcome.class, Class::Wrong | Class::Unparseable) {
            println!(
                "{:?}\t{smiles}\t{}\t{}",
                outcome.class,
                outcome.name,
                field(&outcome.reason)
            );
        }
    }
    if let Some(out) = std::env::var_os("S2N_EXTRA_OUT") {
        std::fs::write(out, rows).expect("write S2N_EXTRA_OUT");
    }
    println!("{counts:?}");
}

/// FR-3.5 source names supply independent structures, including the larger
/// examples whose existing supported representations use other locants/forms.
#[test]
fn fr35_source_examples_and_generated_names_preserve_structures() {
    use super::{NameReason, NameStatus};

    let preferred = NameStatus::Preferred;
    let layout = NameStatus::Unverified(NameReason::OpsinFusionLayout);
    let mut measurements = Measurements::default();
    for (source, smiles, status, output) in [
        (
            "thieno[3,2-f][2,1]benzothiazole",
            "c1cc2cc3csnc3cc2s1",
            preferred,
            None,
        ),
        (
            "[1,3]benzoxazolo[6,5-g]quinoline",
            "c1cnc2cc3cc4ncoc4cc3cc2c1",
            layout,
            None,
        ),
        (
            "benzo[1,2-b:4,5-c']difuran",
            "c1cc2cc3cocc3cc2o1",
            preferred,
            None,
        ),
        (
            "6H-benzo[c]chromene",
            "c1ccc2c(c1)COc1ccccc1-2",
            preferred,
            // P-25.3.5.4 (2013): chromene is no longer a PIN component and
            // FR-3.5 must not break the dibenzo multiplicative prefix.
            Some("6H-dibenzo[b,d]pyran"),
        ),
        (
            "10H-furo[3',2':4,5]indeno[2,1-b]pyridine",
            "c1cnc2c(c1)-c1ccc3occc3c1C2",
            layout,
            None,
        ),
        (
            "4H-[1,4]thiazino[2,3-g]quinoline",
            "C1=CSc2cc3cccnc3cc2N1",
            preferred,
            None,
        ),
        (
            "7H-phenanthro[5,4,3-cdef][1]benzoxepine",
            "c1cc2c3c(c1)ccc1ccc4cccc(c4c13)OC2",
            // The FR-3.5 peri form is not yet handled; keep the proved wrapper.
            NameStatus::Unverified(NameReason::BridgedFusedParent),
            Some("1,12-(epoxymethano)benzo[c]phenanthrene"),
        ),
        (
            "9aH-pyrano[2,3-b][1]benzofuran",
            "C1=COC2Oc3ccccc3C2=C1",
            preferred,
            None,
        ),
        (
            "thieno[3',2':5,6]phenanthro[4,3-e][1]benzothiophene",
            "c1cc2c(ccc3ccc4ccc5ccc6sccc6c5c4c32)s1",
            // The current OPSIN layout changes component locants, preserving identity.
            layout,
            Some("thieno[2',3':3,4]phenanthro[5,6-e][1]benzothiophene"),
        ),
    ] {
        let expected = super::canonical_smiles(smiles).unwrap();
        assert_eq!(
            measurements.parse_name(source).unwrap(),
            expected,
            "{source}"
        );
        let name = namer(smiles).unwrap();
        assert_eq!(name.text, output.unwrap_or(source), "{smiles}");
        assert_eq!(name.status, status, "{source}");
        for text in std::iter::once(&name.text).chain(name.alternatives.iter().map(|n| &n.name)) {
            assert_eq!(measurements.parse_name(text).unwrap(), expected, "{text}");
        }
        for alternative in &name.alternatives {
            assert_eq!(
                alternative.status,
                NameStatus::NotPreferred(NameReason::HigherOrderFusion),
                "{}",
                alternative.name
            );
        }
        if source == "benzo[1,2-b:4,5-c']difuran" {
            assert!(name.alternatives.is_empty(), "multiparent name retained");
        }
    }
}

#[test]
fn fr35_benzoheterocycles_and_higher_order_alternatives_round_trip() {
    use super::{NameReason, NameStatus};

    let mut measurements = Measurements::default();
    for (smiles, preferred, alternative) in [
        (
            "S1CC2=CC=CC3=C2C1=CC=C3",
            "2H-benzo[1,2,3-cd]1-benzothiophene",
            None,
        ),
        (
            "c1nc2ccc3ocnc3c2s1",
            "[1,3]thiazolo[5,4-e][1,3]benzoxazole",
            Some("[1,3]thiazolo[4',5':1,2]benzo[3,4-d][1,3]oxazole"),
        ),
        (
            "c1nc2cc3ocnc3cc2s1",
            "[1,3]thiazolo[5,4-f][1,3]benzoxazole",
            Some("[1,3]thiazolo[4',5':1,2]benzo[4,5-d][1,3]oxazole"),
        ),
        (
            "c1nc2cc3ncsc3cc2s1",
            "benzo[1,2-d:5,4-d']bis[1,3]thiazole",
            None,
        ),
        (
            "O=C(O)Cc1nc2ccc3ocnc3c2s1",
            "2-([1,3]thiazolo[5,4-e][1,3]benzoxazol-7-yl)acetic acid",
            Some("2-([1,3]thiazolo[4',5':1,2]benzo[3,4-d][1,3]oxazol-7-yl)acetic acid"),
        ),
    ] {
        let name = namer(smiles).unwrap();
        assert_eq!(name.text, preferred, "{smiles}");
        assert_eq!(name.status, NameStatus::Preferred, "{smiles}");
        assert_eq!(
            name.alternatives
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            alternative.into_iter().collect::<Vec<_>>(),
            "{smiles}"
        );
        let expected = super::canonical_smiles(smiles).unwrap();
        for text in std::iter::once(&name.text).chain(name.alternatives.iter().map(|n| &n.name)) {
            assert_eq!(measurements.parse_name(text).unwrap(), expected, "{text}");
        }
        for alternative in &name.alternatives {
            assert_eq!(
                alternative.status,
                NameStatus::NotPreferred(NameReason::HigherOrderFusion),
                "{}",
                alternative.name
            );
        }
    }
    // In this four-ring system the selected benzo/pyrazine pair forms quinoxaline.
    let smiles = "c1oc2cc3nc4ccccc4nc3cc2c1";
    let name = namer(smiles).unwrap();
    assert_eq!(name.text, "furo[2,3-g]benzo[b]quinoxaline");
    assert_eq!(
        name.status,
        NameStatus::Unverified(NameReason::OpsinFusionLayout)
    );
    assert!(!name.alternatives.is_empty());
    assert!(name.alternatives.iter().all(|alternative| {
        alternative.status == NameStatus::NotPreferred(NameReason::HigherOrderFusion)
    }));
    let expected = super::canonical_smiles(smiles).unwrap();
    for text in std::iter::once(&name.text).chain(name.alternatives.iter().map(|n| &n.name)) {
        assert_eq!(measurements.parse_name(text).unwrap(), expected, "{text}");
    }
}
