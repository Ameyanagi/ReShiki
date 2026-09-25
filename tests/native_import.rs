//! Complete import responses against the unchanged worker and pinned chemistry.
use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::{
    chemistry::inchi::generator,
    engine::{
        ChemistryEngine, PythonEngine, Request, Response,
        native_import::{self, Deferred, Error, Outcome},
        native_response::Config,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

fn helper(name: &str) -> anyhow::Result<Option<PathBuf>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts/inchi-helper")
        .join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
    if path.is_file() {
        return Ok(Some(path));
    }
    anyhow::ensure!(
        std::env::var_os("RESHIKI_REQUIRE_INCHI_HELPER").is_none(),
        "Build the native helper first"
    );
    eprintln!("Skipping optional native import checks; build the standalone helper first");
    Ok(None)
}

#[derive(Deserialize)]
struct Case {
    name: String,
    text: String,
    format: Option<String>,
    expected: Option<Value>,
    failure: Option<String>,
    restriction: Option<String>,
}

fn compare(actual: Response, expected: Response) -> anyhow::Result<()> {
    let mut a = serde_json::to_value(actual)?;
    let mut b = serde_json::to_value(expected)?;
    for key in ["mass", "exact_mass", "logp", "tpsa"] {
        if let (Some(x), Some(y)) = (a["analysis"][key].as_f64(), b["analysis"][key].as_f64()) {
            anyhow::ensure!(
                (x - y).abs() <= y.abs().max(1.) * 1e-12,
                "{key}: {x} != {y}"
            );
            a["analysis"][key] = Value::Null;
            b["analysis"][key] = Value::Null;
        }
    }
    anyhow::ensure!(a == b, "Full response changed:\nactual={a}\nexpected={b}");
    Ok(())
}

// Match the real bridge's deferred-image and Value -> Response boundary.
fn wire_response(mut value: Value) -> anyhow::Result<Response> {
    if let Some(graphics) = value
        .get_mut("document")
        .and_then(|v| v.get_mut("graphics"))
        .and_then(Value::as_array_mut)
    {
        let mut budget = reshiki::pictures::exchange::Budget::default();
        for graphic in graphics {
            if let Some(source) = graphic.get("picture_source") {
                let bytes =
                    STANDARD.decode(source["data"].as_str().context("Missing picture bytes")?)?;
                let picture = reshiki::pictures::exchange::import(
                    &bytes,
                    source["format"]
                        .as_str()
                        .context("Missing picture format")?,
                    source["opacity"]
                        .as_f64()
                        .context("Missing picture opacity")?,
                    &mut budget,
                )
                .map_err(anyhow::Error::msg)?;
                let object = graphic.as_object_mut().context("Invalid picture")?;
                object.remove("picture_source");
                object.insert("picture".into(), STANDARD.encode(picture.png()).into());
            }
        }
    }
    let response: Response = serde_json::from_value(value)?;
    if let Some(document) = &response.document {
        document.validate().map_err(anyhow::Error::msg)?;
    }
    Ok(response)
}

async fn reference_cases(format: &str, script: &str) -> anyhow::Result<()> {
    let Some(helper) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    };
    let mut process = Command::new(root.join(python))
        .arg(root.join(script))
        .env("PYTHONUTF8", "1")
        .stdout(Stdio::piped())
        .spawn()?;
    let lines = BufReader::new(process.stdout.take().context("Missing reference output")?).lines();
    let (mut accepted, mut rejected, mut restricted) = (0, 0, 0);
    let mut failures = Vec::new();
    for line in lines {
        let record: Value = serde_json::from_str(&line?)?;
        if record.get("name").is_none() {
            assert_eq!(record["rdkit_version"], reshiki::chemistry::RDKIT_VERSION);
            continue;
        }
        let case: Case = serde_json::from_value(record)?;
        let format = case.format.as_deref().unwrap_or(format);
        let request = Request::import(format, &case.text);
        let before = serde_json::to_value(&request)?;
        let actual =
            native_import::execute(request.clone(), Some(Config::new(helper.clone()))).await;
        assert_eq!(
            serde_json::to_value(request)?,
            before,
            "{} request changed",
            case.name
        );
        if case.restriction.is_some() {
            anyhow::ensure!(
                actual.is_err(),
                "{} lost explicit import restriction",
                case.name
            );
            restricted += 1;
            continue;
        }
        let expected = case.expected.and_then(|v| wire_response(v).ok());
        let error = match (actual, expected) {
            (Ok(Outcome::Complete(a)), None) if format == "cdxml" => {
                // Drawing interchange now retains unresolved chemical input.
                // Strict molecular preparation must still reject the same
                // assignment, and no properties may be presented as validated.
                let strict = reshiki::chemistry::cdxml::prepare_cdxml(&case.text)
                    .err()
                    .context("Unexpected relaxation of drawing validation")?;
                anyhow::ensure!(matches!(
                    strict.cause,
                    reshiki::chemistry::cdxml::PreparationCause::Sanitization(_)
                ));
                anyhow::ensure!(a.analysis.is_none() && !a.warnings.is_empty());
                let doc = a.document.context("Missing retained drawing")?;
                doc.validate().map_err(anyhow::Error::msg)?;
                anyhow::ensure!(reshiki::chemistry::document::prepare(&doc).is_err());
                accepted += 1;
                None
            }
            (Ok(Outcome::Complete(a)), Some(b)) => {
                accepted += 1;
                compare(*a, b).err().map(|e| e.to_string())
            }
            (Err(_), None) => {
                rejected += 1;
                None
            }
            (a, b) => Some(format!(
                "Outcome changed: {a:?} != {b:?}; native error {:?}",
                case.failure
            )),
        };
        if let Some(error) = error {
            failures.push(format!("{}: {error}", case.name));
        }
    }
    anyhow::ensure!(process.wait()?.success(), "Reference capture failed");
    if !failures.is_empty() {
        std::fs::write(
            root.join(format!("artifacts/native-import-{format}-failures.log")),
            failures.join("\n"),
        )?;
    }
    println!(
        "Native {format} import: {accepted} complete responses, {rejected} matching failures, {restricted} explicit bounds"
    );
    anyhow::ensure!(
        failures.is_empty(),
        "{} response differences; see artifacts/native-import-{format}-failures.log",
        failures.len()
    );
    anyhow::ensure!(
        accepted > 50 && rejected > 0,
        "Insufficient complete-response cases"
    );
    Ok(())
}

#[tokio::test]
async fn mol_import_matches_complete_original_responses() -> anyhow::Result<()> {
    reference_cases("mol", "tests/molfile_engine_reference.py").await
}
#[tokio::test]
async fn rxn_import_matches_complete_original_responses() -> anyhow::Result<()> {
    reference_cases("rxn", "tests/reaction_engine_reference.py").await
}
#[tokio::test]
async fn drawing_and_coordinate_reactions_match_complete_original_responses() -> anyhow::Result<()>
{
    reference_cases("cdxml", "tests/native_import_reference.py").await
}

const MOLECULE: &str = "<CDXML BondLength='14.4'><page id='10'><fragment id='7000'><n id='1' p='0 0'/><n id='2' p='14.4 0' Element='8'/><b id='3' B='1' E='2'/></fragment></page></CDXML>";
const FIGURE: &str =
    "<CDXML><page><t id='1' p='12 24'><s>Reaction conditions</s></t></page></CDXML>";

#[tokio::test]
async fn routing_distinguishes_layouts_errors_and_helper_failures() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let missing = Config::new(dir.path().join("missing-helper"));
    for (format, text) in [
        ("smiles", "C"),
        ("inchi", "InChI=1S/CH4/h1H4"),
        ("rsmi", "C>>O"),
    ] {
        assert!(matches!(
            native_import::execute(Request::import(format, text), Some(missing.clone())).await,
            Err(Error::Analysis(
                reshiki::engine::native_response::Error::Helper(generator::Error::Io(_))
            ))
        ));
    }
    let request = Request::molecule("analyze", Default::default());
    assert!(matches!(
        native_import::execute(request, Some(missing.clone())).await?,
        Outcome::Deferred(Deferred::OtherOperation)
    ));
    for (format, text) in [
        ("mol", "invalid"),
        ("rxn", "invalid"),
        ("cdxml", "<bad"),
        ("cdx", "!"),
        ("rsmi", "invalid"),
        ("unknown", "x"),
    ] {
        assert!(
            native_import::execute(Request::import(format, text), Some(missing.clone()))
                .await
                .is_err(),
            "{format} error became deferred"
        );
    }
    let mut protocol = Request::import("cdxml", FIGURE);
    protocol.protocol = 2;
    assert!(matches!(
        native_import::execute(protocol, None).await,
        Err(Error::Protocol)
    ));
    let Outcome::Complete(figure) =
        native_import::execute(Request::import("cdxml", FIGURE), Some(missing.clone())).await?
    else {
        anyhow::bail!("Empty figure was deferred");
    };
    assert!(figure.analysis.is_none());
    assert_eq!(
        figure.document.context("Missing figure")?.annotations.len(),
        1
    );
    assert!(matches!(
        native_import::execute(Request::import("cdxml", MOLECULE), Some(missing)).await,
        Err(Error::Analysis(
            reshiki::engine::native_response::Error::Helper(generator::Error::Io(_))
        ))
    ));
    Ok(())
}

#[tokio::test]
async fn cdx_conversion_and_kernel_budget_keep_import_atomic() -> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let config = Config::new(path);
    let reference = PythonEngine::default();
    for xml in [
        MOLECULE,
        include_str!("fixtures/ui-drawn-ethanol.cdxml"),
        include_str!("fixtures/atom-labels-chemdraw.cdxml"),
        include_str!("fixtures/graphics-chemdraw.cdxml"),
    ] {
        let document = reference
            .execute(Request::import("cdxml", xml))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Missing reference drawing")?;
        let mut export = Request::molecule("export", document);
        export.format = Some("cdx".into());
        let binary = reference
            .execute(export)
            .await
            .map_err(anyhow::Error::msg)?
            .output
            .context("Missing original binary export")?;
        let request = Request::import("cdx", &binary);
        let expected = reference
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let Outcome::Complete(actual) =
            native_import::execute(request, Some(config.clone())).await?
        else {
            anyhow::bail!("Binary drawing deferred");
        };
        // CDX deliberately quantizes coordinates/styles to signed 16.16. Compare
        // its complete original import, not the pre-quantized XML drawing.
        compare(*actual, expected)?;
    }
    let tiny = Config {
        limits: generator::Limits {
            kernel_heap_bytes: 1,
            timeout: Duration::from_secs(5),
        },
        ..config.clone()
    };
    assert!(matches!(
        native_import::execute(Request::import("cdxml", MOLECULE), Some(tiny)).await,
        Err(Error::Analysis(
            reshiki::engine::native_response::Error::Helper(generator::Error::ResourceLimit { .. })
        ))
    ));
    assert!(matches!(
        native_import::execute(Request::import("cdxml", MOLECULE), Some(config)).await?,
        Outcome::Complete(_)
    ));
    assert!(matches!(
        native_import::execute(
            Request::import(
                "cdx",
                &"A".repeat(reshiki::exchange::LIMIT.div_ceil(3) * 4 + 1)
            ),
            None
        )
        .await,
        Err(Error::BinaryLimit)
    ));
    Ok(())
}

#[tokio::test]
async fn layouts_match_complete_original_import_responses() -> anyhow::Result<()> {
    reference_cases("layout", "tests/native_import_layout_reference.py").await
}

#[tokio::test]
async fn layout_and_inchi_reader_failures_preserve_the_entire_request() -> anyhow::Result<()> {
    let Some(path) = helper("reshiki-inchi-helper")? else {
        return Ok(());
    };
    let config = Config::new(path);
    for (format, text) in [
        ("smiles", "CC |atomProp:1._CIPRank.bad|"),
        ("rsmi", "CC>>O |atomProp:1._chiralAtomRank.bad|"),
    ] {
        let source = std::sync::Arc::new(Request::import(format, text));
        let before = serde_json::to_value(source.as_ref())?;
        assert!(matches!(
            native_import::execute(source.clone(), Some(config.clone())).await,
            Err(Error::Layout(_))
        ));
        assert_eq!(serde_json::to_value(source.as_ref())?, before);
    }
    let source = std::sync::Arc::new(Request::import(
        "inchi",
        "InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3",
    ));
    let before = serde_json::to_value(source.as_ref())?;
    let tiny = Config {
        limits: generator::Limits {
            kernel_heap_bytes: 1,
            ..config.limits
        },
        ..config.clone()
    };
    assert!(matches!(
        native_import::execute(source.clone(), Some(tiny)).await,
        Err(Error::Analysis(
            reshiki::engine::native_response::Error::Helper(generator::Error::ResourceLimit { .. })
        ))
    ));
    assert_eq!(serde_json::to_value(source.as_ref())?, before);
    let Outcome::Complete(response) =
        native_import::execute(source.clone(), Some(config.clone())).await?
    else {
        anyhow::bail!("Recovered InChI import was deferred");
    };
    assert_eq!(
        response
            .document
            .context("Missing recovered drawing")?
            .atoms
            .len(),
        3
    );
    assert_eq!(serde_json::to_value(source.as_ref())?, before);
    assert!(matches!(
        native_import::execute(Request::import("inchi", "InChI=1S/invalid"), Some(config)).await,
        Err(Error::Parse)
    ));
    Ok(())
}
