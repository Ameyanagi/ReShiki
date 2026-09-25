//! Complete aromatic responses compared with the retained original worker.
use anyhow::Context;
use reshiki::{
    document::{Document, History, Point},
    engine::{
        ChemistryEngine, PythonEngine, Request, Response, native_aromatic, native_response::Config,
    },
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Instant,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    task::JoinSet,
};

#[path = "common/fixture.rs"]
mod fixture;

fn helper() -> anyhow::Result<Option<PathBuf>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts/inchi-helper")
        .join(if cfg!(windows) {
            "reshiki-inchi-helper.exe"
        } else {
            "reshiki-inchi-helper"
        });
    if path.is_file() {
        return Ok(Some(path));
    }
    anyhow::ensure!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "Build the pinned native helper first"
    );
    eprintln!("Skipping optional native aromatic responses; build the helper first");
    Ok(None)
}

fn request(document: Document, selected: Vec<u64>) -> Request {
    let mut request = Request::molecule("aromatic", document);
    request.selected_ids = Some(selected);
    request
}

fn equal(actual: Response, expected: Response) -> anyhow::Result<()> {
    let mut actual = serde_json::to_value(actual)?;
    let mut expected = serde_json::to_value(expected)?;
    for field in ["mass", "exact_mass", "logp", "tpsa"] {
        if let (Some(a), Some(e)) = (
            actual["analysis"][field].as_f64(),
            expected["analysis"][field].as_f64(),
        ) {
            anyhow::ensure!(
                (a - e).abs() <= e.abs().max(1.) * 1e-12,
                "{field}: {a} != {e}"
            );
            actual["analysis"][field] = serde_json::Value::Null;
            expected["analysis"][field] = serde_json::Value::Null;
        }
    }
    anyhow::ensure!(
        actual == expected,
        "Response changed: {actual}\nExpected: {expected}"
    );
    Ok(())
}

#[derive(Deserialize)]
struct Case {
    name: String,
    document: Document,
    selection: Vec<u64>,
}

// A clone of PythonEngine shares one locked child. Construct independent engines
// instead, retaining each worker across cases while bounding processes and memory.
const MAX_REFERENCE_WORKERS: usize = 4;
const GOLDEN_CASES: usize = 13_425;

fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |count| count.get())
        .min(MAX_REFERENCE_WORKERS)
}

struct Comparison {
    index: usize,
    name: String,
    result: anyhow::Result<bool>,
}

async fn compare_request(
    request: Arc<Request>,
    config: Config,
    expected: impl std::future::Future<Output = Result<Response, String>>,
) -> anyhow::Result<bool> {
    let before = serde_json::to_value(&*request)?;
    let expected = expected.await;
    let actual = native_aromatic::execute(Arc::clone(&request), Some(config)).await;
    anyhow::ensure!(before == serde_json::to_value(&*request)?, "Input changed");
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => {
            equal(actual, expected)?;
            Ok(true)
        }
        (Err(actual), Err(expected)) => {
            anyhow::ensure!(
                actual.to_string() == expected,
                "Error changed: {actual} != {expected}"
            );
            Ok(false)
        }
        (actual, expected) => Err(anyhow::anyhow!(
            "Outcome changed: {actual:?} != {expected:?}"
        )),
    }
}

#[derive(Default)]
struct Comparisons {
    accepted: usize,
    rejected: usize,
    failures: Vec<(usize, String)>,
}

impl Comparisons {
    fn record(&mut self, comparison: Comparison) {
        match comparison.result {
            Ok(true) => self.accepted += 1,
            Ok(false) => self.rejected += 1,
            Err(error) => {
                self.failures
                    .push((comparison.index, format!("{}: {error:#}", comparison.name)));
                self.failures.sort_by_key(|(index, _)| *index);
                self.failures.truncate(12);
            }
        }
    }

    fn finish(mut self) -> anyhow::Result<(usize, usize)> {
        self.failures.sort_by_key(|(index, _)| *index);
        anyhow::ensure!(
            self.failures.is_empty(),
            "{}",
            self.failures
                .into_iter()
                .take(12)
                .map(|(_, failure)| failure)
                .collect::<Vec<_>>()
                .join("\n")
        );
        anyhow::ensure!(
            self.accepted > 5000 && self.rejected > 500,
            "Insufficient coverage: {} accepted, {} rejected",
            self.accepted,
            self.rejected
        );
        Ok((self.accepted, self.rejected))
    }
}

async fn compare_corpus(
    output: tokio::process::ChildStdout,
    config: Config,
    worker_count: usize,
) -> anyhow::Result<(usize, usize)> {
    let mut lines = BufReader::new(output).lines();
    let header: serde_json::Value =
        serde_json::from_str(&lines.next_line().await?.context("Missing corpus version")?)?;
    anyhow::ensure!(
        header["rdkit_version"] == reshiki::chemistry::RDKIT_VERSION,
        "Independent corpus version changed: {}",
        header["rdkit_version"]
    );
    let mut available: Vec<_> = (0..worker_count).map(|_| PythonEngine::default()).collect();
    let mut pending = JoinSet::new();
    let mut comparisons = Comparisons::default();
    let mut index = 0;
    while let Some(line) = lines.next_line().await? {
        let case: Case =
            serde_json::from_str(&line).with_context(|| format!("Invalid corpus case {index}"))?;
        if available.is_empty() {
            let (reference, comparison) = pending
                .join_next()
                .await
                .context("Missing active reference worker")?
                .context("Reference comparison task failed")?;
            available.push(reference);
            comparisons.record(comparison);
            if comparisons.failures.len() >= 12 {
                break;
            }
        }
        let reference = available.pop().context("Missing idle reference worker")?;
        let config = config.clone();
        pending.spawn(async move {
            let request = Arc::new(request(case.document, case.selection));
            let result = compare_request(
                Arc::clone(&request),
                config,
                reference.execute((*request).clone()),
            )
            .await;
            (
                reference,
                Comparison {
                    index,
                    name: case.name,
                    result,
                },
            )
        });
        index += 1;
    }
    // Joining every dispatched case releases each independent PythonEngine. On
    // an infrastructure error JoinSet aborts its tasks; their children already
    // use kill_on_drop, as do the native helper and the corpus producer below.
    while let Some(result) = pending.join_next().await {
        let (_, comparison) = result.context("Reference comparison task failed")?;
        comparisons.record(comparison);
    }
    comparisons.finish()
}

#[tokio::test]
async fn complete_aromatic_responses_match_original_worker() -> anyhow::Result<()> {
    let Some(helper) = helper()? else {
        return Ok(());
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    // This existing independent corpus includes full/partial selections,
    // individual fused rings, both toggle directions and abbreviation failures.
    let mut child = Command::new(python)
        .arg(root.join("tests/aromatic_display_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let worker_count = worker_count();
    let started = Instant::now();
    let result = compare_corpus(
        child.stdout.take().context("Missing corpus")?,
        Config::new(helper),
        worker_count,
    )
    .await;
    if result.is_err() {
        // Explicitly kill and reap even if parsing/comparison stopped before
        // EOF, so a producer blocked on its full stdout pipe cannot outlive us.
        child.kill().await.context("Stopping failed corpus")?;
    }
    let status = child
        .wait()
        .await
        .context("Waiting for independent corpus")?;
    let (accepted, rejected) = result?;
    anyhow::ensure!(status.success(), "Independent corpus failed: {status}");
    eprintln!(
        "Native aromatic responses: {accepted} exact responses, {rejected} matching rejections; \
         {worker_count} workers, {:.2}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

#[derive(Deserialize)]
struct GoldenHeader {
    format_version: u32,
    cases: usize,
    rdkit_version: String,
    rdkit_source: String,
    accepted: usize,
    rejected: usize,
    records_sha256: String,
    source_sha256: BTreeMap<String, String>,
    request_encoding: String,
    preflight_rejections: usize,
    worker_accepted: usize,
    worker_rejected: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapturedRequest {
    protocol: u32,
    operation: String,
    document: Document,
    selected_ids: Vec<u64>,
}

fn restore_request(value: serde_json::Value) -> anyhow::Result<Request> {
    let captured: CapturedRequest = serde_json::from_value(value.clone())?;
    let mut request = request(captured.document, captured.selected_ids);
    request.protocol = captured.protocol;
    request.operation = captured.operation;
    anyhow::ensure!(
        serde_json::to_value(&request)? == value,
        "Captured request no longer round-trips through the exact worker input schema"
    );
    Ok(request)
}

// This opt-in step captures Request serialization and PythonEngine's preceding
// Rust document-validation boundary. Molecular worker responses are generated
// separately by native_aromatic_reference.py; no native_aromatic output is used.
#[test]
#[ignore = "explicit fixture input serialization; requires input/output paths"]
fn serialize_aromatic_requests() -> anyhow::Result<()> {
    let input = PathBuf::from(
        std::env::var_os("RESHIKI_AROMATIC_REQUESTS_INPUT")
            .context("Set RESHIKI_AROMATIC_REQUESTS_INPUT")?,
    );
    let output = PathBuf::from(
        std::env::var_os("RESHIKI_AROMATIC_REQUESTS_OUTPUT")
            .context("Set RESHIKI_AROMATIC_REQUESTS_OUTPUT")?,
    );
    let mut reader =
        std::io::BufReader::new(flate2::read::GzDecoder::new(std::fs::File::open(input)?));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut header: serde_json::Value = serde_json::from_str(&line)?;
    anyhow::ensure!(
        header["request_encoding"] == "raw-independent-corpus-v1",
        "Not a raw corpus"
    );
    let expected_hash = header["records_sha256"]
        .as_str()
        .context("Missing input hash")?
        .to_owned();
    let mut hashes = BTreeMap::new();
    for name in [
        "tests/native_aromatic.rs",
        "src/engine.rs",
        "src/engine/reference.rs",
        "src/document.rs",
        "src/typography.rs",
        "src/style.rs",
        "src/atom_labels.rs",
    ] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        hashes.insert(
            name,
            format!("{:x}", Sha256::digest(std::fs::read(source)?)),
        );
    }
    header["raw_records_sha256"] = expected_hash.clone().into();
    header
        .as_object_mut()
        .context("Invalid corpus header")?
        .remove("records_sha256");
    header["request_encoding"] = "reshiki-serde-request-v1".into();
    header["serializer_sha256"] = serde_json::to_value(hashes)?;
    let destination =
        tempfile::NamedTempFile::new_in(output.parent().context("Missing output parent")?)?;
    let mut writer = flate2::write::GzEncoder::new(destination, flate2::Compression::default());
    serde_json::to_writer(&mut writer, &header)?;
    writer.write_all(b"\n")?;
    let mut count = 0;
    let mut preflight = 0;
    let mut hash = Sha256::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        hash.update(line.as_bytes());
        let mut record: serde_json::Value = serde_json::from_str(&line)?;
        anyhow::ensure!(
            record.get("expected").is_none(),
            "Input contains expected results"
        );
        let captured: CapturedRequest = serde_json::from_value(record["request"].clone())?;
        let mut request = request(captured.document, captured.selected_ids);
        request.protocol = captured.protocol;
        request.operation = captured.operation;
        // This is the same preflight that the existing live PythonEngine runs
        // before contacting its child. Freeze its diagnostics independently of
        // future runtime changes; do not derive expected strings during replay.
        let error = if request.protocol != 1 {
            Some("Unsupported protocol version".to_owned())
        } else {
            request
                .document
                .as_ref()
                .and_then(|document| document.validate().err())
        };
        preflight += usize::from(error.is_some());
        record["preflight_error"] = serde_json::to_value(error)?;
        // PythonEngine::request also uses to_value before framing its message.
        record["request"] = serde_json::to_value(&request)?;
        serde_json::to_writer(&mut writer, &record)?;
        writer.write_all(b"\n")?;
        count += 1;
    }
    anyhow::ensure!(
        format!("{:x}", hash.finalize()) == expected_hash,
        "Input corpus hash changed"
    );
    anyhow::ensure!(
        count > 5500 && header["cases"].as_u64() == Some(count),
        "Insufficient request corpus"
    );
    writer.finish()?.persist(output)?;
    eprintln!(
        "Serialized {count} complete aromatic requests; {preflight} preflight rejections; no worker expectations generated"
    );
    Ok(())
}

#[derive(Deserialize)]
struct CapturedExpected {
    ok: bool,
    result: Option<Response>,
    error: Option<String>,
}

impl CapturedExpected {
    fn result(self) -> anyhow::Result<Result<Response, String>> {
        match (self.ok, self.result, self.error) {
            (true, Some(response), None) => Ok(Ok(response)),
            (false, None, Some(error)) => Ok(Err(error)),
            _ => anyhow::bail!("Invalid captured worker response"),
        }
    }
}

#[derive(Deserialize)]
struct GoldenCase {
    name: String,
    request: serde_json::Value,
    expected: CapturedExpected,
    preflight_error: Option<String>,
}

/// Fast CI entry point: only Rust, checked-in pinned references, and the native
/// InChI helper. This test never starts Python or generates expected responses.
#[tokio::test]
async fn complete_aromatic_responses_match_goldens() -> anyhow::Result<()> {
    let Some(helper) = helper()? else {
        return Ok(());
    };
    let filename = if cfg!(windows) {
        "native-aromatic-windows.json.gz"
    } else if cfg!(target_os = "macos") {
        "native-aromatic-macos.json.gz"
    } else {
        "native-aromatic-linux.json.gz"
    };
    let mut stream = fixture::open(filename)?;
    let mut line = String::new();
    stream.read_line(&mut line)?;
    let header: GoldenHeader = serde_json::from_str(&line)?;
    anyhow::ensure!(header.format_version == 1, "Unsupported fixture format");
    anyhow::ensure!(
        header.rdkit_version == reshiki::chemistry::RDKIT_VERSION
            && header.rdkit_source == "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
        "Fixture RDKit identity differs"
    );
    anyhow::ensure!(
        header.request_encoding == "reshiki-serde-request-v1",
        "Unserialized golden inputs"
    );
    for (name, expected) in &header.source_sha256 {
        let bytes = if name == "assets/templates.json" {
            // Captured requests contain the complete historical template input.
            // Preserve its exact source bytes separately as the live library grows.
            let mut bytes = Vec::new();
            fixture::open("aromatic-template-inputs.json.gz")?.read_to_end(&mut bytes)?;
            bytes
        } else {
            std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(name))?
        };
        let actual = format!("{:x}", Sha256::digest(bytes));
        anyhow::ensure!(actual == *expected, "Reference source changed: {name}");
    }
    let config = Config::new(helper);
    let mut pending = JoinSet::new();
    let mut comparisons = Comparisons::default();
    let workers = worker_count();
    let started = Instant::now();
    let mut hash = Sha256::new();
    let mut index = 0;
    let mut preflight = 0;
    let (mut worker_accepted, mut worker_rejected) = (0, 0);
    loop {
        line.clear();
        if stream.read_line(&mut line)? == 0 {
            break;
        }
        hash.update(line.as_bytes());
        let case: GoldenCase =
            serde_json::from_str(&line).with_context(|| format!("Invalid golden case {index}"))?;
        if pending.len() == workers {
            let comparison = pending
                .join_next()
                .await
                .context("Missing golden comparison")?
                .context("Golden comparison task failed")?;
            comparisons.record(comparison);
        }
        let config = config.clone();
        let worker_expected = case.expected.result()?;
        if worker_expected.is_ok() {
            worker_accepted += 1;
        } else {
            worker_rejected += 1;
        }
        // The live reference rejects malformed documents before Python runs.
        // These fixed, separately counted diagnostics are captured once from
        // that pinned boundary, never obtained from runtime validation here.
        let expected = if let Some(error) = case.preflight_error {
            preflight += 1;
            Err(error)
        } else {
            worker_expected
        };
        let request = restore_request(case.request)?;
        pending.spawn(async move {
            let result =
                compare_request(Arc::new(request), config, std::future::ready(expected)).await;
            Comparison {
                index,
                name: case.name,
                result,
            }
        });
        index += 1;
    }
    while let Some(result) = pending.join_next().await {
        comparisons.record(result.context("Golden comparison task failed")?);
    }
    anyhow::ensure!(
        format!("{:x}", hash.finalize()) == header.records_sha256,
        "Golden record checksum changed"
    );
    anyhow::ensure!(
        preflight == header.preflight_rejections && preflight > 0,
        "Preflight boundary coverage differs: {preflight} != {}",
        header.preflight_rejections
    );
    anyhow::ensure!(
        (worker_accepted, worker_rejected) == (header.worker_accepted, header.worker_rejected),
        "Independent worker capture coverage differs"
    );
    anyhow::ensure!(
        header.cases == GOLDEN_CASES && index == GOLDEN_CASES,
        "Complete corpus changed: header {}, replayed {index}, expected {GOLDEN_CASES}",
        header.cases
    );
    let (accepted, rejected) = comparisons.finish()?;
    anyhow::ensure!(
        (accepted, rejected) == (header.accepted, header.rejected),
        "Golden coverage differs from captured counts"
    );
    eprintln!(
        "Golden aromatic responses: {accepted} exact responses, {rejected} matching rejections; \
         {preflight} fixed preflight rejections; {workers} workers, {:.2}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn benzene() -> Document {
    let mut document = Document::default();
    reshiki::editing::ring(&mut document, Point::default(), 6, false, 42.);
    for (index, bond) in document.bonds.iter_mut().enumerate() {
        bond.order = if index % 2 == 0 { 2 } else { 1 };
        bond.color = [17, 126, 108];
    }
    document
}

#[tokio::test]
async fn display_response_is_one_undoable_edit_and_failures_publish_nothing() -> anyhow::Result<()>
{
    let original = benzene();
    let selected = original.atoms.iter().map(|a| a.id).collect();
    let request = Arc::new(request(original.clone(), selected));
    let before = serde_json::to_value(&*request)?;
    let missing = Config::new(PathBuf::from("missing-helper"));
    let error = native_aromatic::execute(Arc::clone(&request), Some(missing)).await;
    assert!(matches!(error, Err(native_aromatic::Error::Analysis(_))));
    assert_eq!(before, serde_json::to_value(&*request)?);
    let Some(helper) = helper()? else {
        return Ok(());
    };
    let response = native_aromatic::execute(request, Some(Config::new(helper))).await?;
    let mut document = response.document.context("Missing display edit")?;
    assert!(document.bonds.iter().all(|b| b.order == 4));
    let changed = document.clone();
    let mut history = History::default();
    assert!(history.commit(original.clone(), &document));
    assert!(history.undo(&mut document));
    assert_eq!(document, original);
    assert!(!history.can_undo());
    assert!(history.redo(&mut document));
    assert_eq!(document, changed);
    Ok(())
}

#[tokio::test]
async fn validation_does_not_resolve_a_helper_and_matches_original_errors() -> anyhow::Result<()> {
    let reference = PythonEngine::default();
    let document = benzene();
    let selected: Vec<_> = document.atoms.iter().map(|a| a.id).collect();
    let first = *selected.first().context("Missing benzene atom")?;
    let mut cases = vec![
        request(document.clone(), vec![]),
        request(document.clone(), vec![first, u64::MAX]),
        request(Document::default(), vec![1]),
    ];
    let mut invalid = document.clone();
    let duplicate = invalid.atoms.get(1).context("Missing second atom")?.id;
    invalid.atoms.first_mut().context("Missing first atom")?.id = duplicate;
    cases.push(request(invalid, selected.clone()));
    let mut missing = request(document.clone(), selected.clone());
    missing.document = None;
    cases.push(missing);
    let mut protocol = request(document, selected);
    protocol.protocol = 2;
    cases.push(protocol);
    for request in cases {
        let expected = reference
            .execute(request.clone())
            .await
            .err()
            .context("Expected rejection")?;
        let error = native_aromatic::execute(
            request,
            Some(Config::new(PathBuf::from("helper-must-not-be-discovered"))),
        )
        .await
        .err()
        .context("Native validation accepted request")?;
        assert_eq!(error.to_string(), expected);
    }
    Ok(())
}
