use super::*;
use crate::ops::{
    exec::Hooks,
    headless::HeadlessHost,
    host::{Call, ToolHost},
    policy::Access,
    wire::{RequestId, versions_json},
};
use serde_json::json;
use std::sync::atomic::{AtomicI64, Ordering};

const ETHANOL_MOL: &str = include_str!("../../../../../tests/fixtures/ethanol.mol");
const AROMATIC_CDXML: &str =
    include_str!("../../../../../tests/fixtures/aromatic-circle-native.cdxml");
const ABBREVIATIONS_CDX: &[u8] =
    include_bytes!("../../../../../tests/fixtures/abbreviations-native.cdx");
const BOND_JOIN_RSK: &str = include_str!("../../../../../tests/fixtures/bond-join-regression.rsk");

fn host() -> HeadlessHost {
    HeadlessHost::new("9.8.7", Budgets::default())
}

fn call(tool: &str, arguments: Value) -> Call {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    Call {
        principal: Principal::local(),
        request: RequestId::Int(NEXT.fetch_add(1, Ordering::Relaxed)),
        tool: tool.into(),
        arguments,
        progress: None,
    }
}

/// Standard base64 with padding, for the cdx fixture.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0_u32, |word, (i, byte)| {
            word | u32::from(*byte) << (16 - 8 * i)
        });
        for i in 0..4 {
            text.push(if i <= chunk.len() {
                char::from(ALPHABET[(word >> (18 - 6 * i) & 63) as usize])
            } else {
                '='
            });
        }
    }
    text
}

/// The `import` result value of a successful call.
async fn import_ok(host: &HeadlessHost, format: &str, text: &str) -> Value {
    let result = host
        .call(call("import", json!({"format": format, "text": text})))
        .await
        .unwrap();
    assert!(!result.is_error, "{format}: {:?}", result.value);
    assert!(result.images.is_empty() && result.files.is_empty());
    Value::Object(result.value)
}

/// The `inspect` result value for the whole drawing.
async fn inspect_ok(host: &HeadlessHost, document: &Value) -> Value {
    let result = host
        .call(call("inspect", json!({"document": document, "ids": null})))
        .await
        .unwrap();
    assert!(!result.is_error, "{:?}", result.value);
    Value::Object(result.value)
}

/// The stored document behind an import result.
fn stored(host: &HeadlessHost, imported: &Value) -> Arc<Document> {
    let handle = DocHandle::new(imported["value"]["document"].as_str().unwrap()).unwrap();
    host.store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap()
        .doc
}

fn counts(atoms: usize, bonds: usize, arrows: usize, annotations: usize, graphics: usize) -> Value {
    json!({"atoms": atoms, "bonds": bonds, "arrows": arrows, "annotations": annotations, "graphics": graphics})
}

#[test]
fn formats_are_exactly_the_published_import_formats() {
    for name in IMPORT_FORMATS {
        let format: Format = serde_json::from_value(json!(name)).unwrap();
        assert_eq!(format.name(), *name);
    }
    assert!(serde_json::from_value::<Format>(json!("emf")).is_err());
    assert!(serde_json::from_value::<Format>(json!("SMILES")).is_err());
}

#[test]
fn text_budgets_are_checked_at_decode_time() {
    let budgets = Budgets::default();
    let decode_text = |format: &str, len: usize| {
        decode(json!({"format": format, "text": "C".repeat(len)}), &budgets).map(drop)
    };
    for format in [
        "auto", "smiles", "mol", "rxn", "rsmi", "inchi", "cdxml", "reshiki",
    ] {
        assert_eq!(
            decode_text(format, budgets.max_text_bytes),
            Ok(()),
            "{format}"
        );
        let error = decode_text(format, budgets.max_text_bytes + 1).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Budget, "{format}");
        assert_eq!(
            error.message,
            format!(
                "The text is {} bytes; max_text_bytes allows at most {}",
                budgets.max_text_bytes + 1,
                budgets.max_text_bytes
            )
        );
    }
    // cdx is held only to its base64 limit, which is larger.
    assert!(budgets.max_cdx_base64 > budgets.max_text_bytes);
    assert_eq!(decode_text("cdx", budgets.max_text_bytes + 1), Ok(()));
    assert_eq!(decode_text("cdx", budgets.max_cdx_base64), Ok(()));
    let error = decode_text("cdx", budgets.max_cdx_base64 + 1).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    assert_eq!(
        error.message,
        format!(
            "The text is {} bytes; max_cdx_base64 allows at most {}",
            budgets.max_cdx_base64 + 1,
            budgets.max_cdx_base64
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_budget_error_never_reaches_the_executor() {
    let host = host();
    // The executor now refuses every call as busy.
    host.drained().await;
    let text = "C".repeat(Budgets::default().max_text_bytes + 1);
    let result = host
        .call(call("import", json!({"format": "smiles", "text": text})))
        .await
        .unwrap();
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "budget");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smiles_imports_into_a_new_document_with_an_analysis() {
    let host = host();
    let imported = import_ok(&host, "smiles", "CCO").await;
    assert_eq!(imported["validation"], json!({"status": "valid"}));
    assert_eq!(
        imported["versions"],
        versions_json(&Versions::current("9.8.7"))
    );
    let value = &imported["value"];
    assert!(value["document"].as_str().unwrap().starts_with("doc_"));
    assert_eq!(value["revision"], "0");
    assert_eq!(value["counts"], counts(3, 2, 0, 0, 0));
    assert_eq!(value["analysis"]["formula"], "C2H6O");
    assert_eq!(value["analysis"]["smiles"], "CCO");
    assert_eq!(
        value["analysis"].as_object().unwrap().len(),
        12,
        "{}",
        value["analysis"]
    );
    let summary = inspect_ok(&host, &value["document"]).await;
    assert_eq!(summary["counts"]["atoms"], 3);
    assert_eq!(summary["counts"]["bonds"], 2);
    let elements: Vec<_> = summary["atoms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|atom| atom["element"].as_str().unwrap())
        .collect();
    assert_eq!(elements, ["C", "C", "O"]);
    let doc = stored(&host, &imported);
    let ids: Vec<_> = doc.atoms.iter().map(|atom| atom.id.to_string()).collect();
    let listed: Vec<_> = summary["atoms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|atom| atom["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(listed, ids);
    // Every call creates another document.
    let again = import_ok(&host, "smiles", "CCO").await;
    assert_ne!(again["value"]["document"], value["document"]);
    assert_eq!(host.store().list(&Principal::local()).len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mol_and_auto_detected_mol_import_alike() {
    let host = host();
    for format in ["mol", "auto"] {
        let imported = import_ok(&host, format, ETHANOL_MOL).await;
        assert_eq!(
            imported["value"]["counts"],
            counts(3, 2, 0, 0, 0),
            "{format}"
        );
        assert_eq!(
            imported["value"]["analysis"]["formula"], "C2H6O",
            "{format}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cdxml_and_base64_cdx_fixtures_import() {
    let host = host();
    for (format, text, atoms, formula, abbreviations) in [
        ("cdxml", AROMATIC_CDXML.to_owned(), 6, "C6H6", 0),
        ("auto", AROMATIC_CDXML.to_owned(), 6, "C6H6", 0),
        ("cdx", base64(ABBREVIATIONS_CDX), 16, "C12H17NO3", 2),
    ] {
        let imported = import_ok(&host, format, &text).await;
        let value = &imported["value"];
        assert_eq!(value["counts"], counts(atoms, atoms, 0, 0, 0), "{format}");
        assert_eq!(value["analysis"]["formula"], formula, "{format}");
        let summary = inspect_ok(&host, &value["document"]).await;
        assert_eq!(
            summary["counts"]["abbreviations"], abbreviations,
            "{format}"
        );
        let doc = stored(&host, &imported);
        let anchors: Vec<_> = doc
            .abbreviations
            .iter()
            .map(|abbreviation| Value::from(abbreviation.anchor.to_string()))
            .collect();
        let listed: Vec<_> = summary["abbreviations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|abbreviation| abbreviation["anchor"].clone())
            .collect();
        assert_eq!(listed, anchors, "{format}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_native_drawing_opens_as_the_app_opens_a_file() {
    let host = host();
    let imported = import_ok(&host, "reshiki", BOND_JOIN_RSK).await;
    let expected = Document::from_native_file(BOND_JOIN_RSK.as_bytes()).unwrap();
    assert_eq!(*stored(&host, &imported), expected);
    assert!(expected.version >= 15);
    assert_eq!(imported["value"]["analysis"], Value::Null);
    assert_eq!(imported["warnings"], json!([]));
    assert_eq!(
        imported["value"]["counts"],
        json!({
            "atoms": expected.atoms.len(),
            "bonds": expected.bonds.len(),
            "arrows": expected.arrows.len(),
            "annotations": expected.annotations.len(),
            "graphics": expected.graphics.len(),
        })
    );
    let broken = host
        .call(call("import", json!({"format": "reshiki", "text": "{"})))
        .await
        .unwrap();
    assert!(broken.is_error);
    assert_eq!(broken.value["error"]["code"], "failed");
    assert!(
        broken.value["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("Could not open document: ")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_smiles_fails_with_the_engine_message() {
    let expected = LocalEngine::default()
        .request(Request::import("smiles", "C1CC(("))
        .await
        .unwrap_err();
    let host = host();
    let result = host
        .call(call(
            "import",
            json!({"format": "smiles", "text": "C1CC(("}),
        ))
        .await
        .unwrap();
    assert!(result.is_error);
    assert_eq!(
        Value::Object(result.value),
        json!({
            "error": {"code": "failed", "message": expected},
            "versions": versions_json(&Versions::current("9.8.7")),
        })
    );
    assert!(host.store().list(&Principal::local()).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_import_creates_nothing() {
    let host = host();
    let exec = host.exec().clone();
    let request = call("import", json!({"format": "smiles", "text": "CCO"}));
    let id = request.request.clone();
    exec.set_hooks(Hooks {
        acquired: Some(Arc::new({
            let exec = exec.clone();
            move || exec.cancel(&Principal::local(), &id)
        })),
        ..Hooks::default()
    });
    let error = host.call(request).await.unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert!(host.store().list(&Principal::local()).is_empty());
}

#[test]
fn finalize_reconciles_and_restores_computed_labels() {
    let mut response = Document::default();
    let a = response.add_atom("C", crate::document::Point::new(0., 0.));
    let b = response.add_atom("O", crate::document::Point::new(30., 0.));
    response.add_bond(a, b, 1, "plain");
    response.atoms[0].label_h = 3;
    response.atoms[1].label_h = 1;
    let finalized = finalize(&response).unwrap();
    // The chemistry changed from the empty drawing, so the commit cleared the
    // computed labels and the refresh copied the response's back.
    assert_eq!(finalized, response);
}

#[test]
fn a_rejected_drawing_is_an_error_envelope() {
    let mut response = Document::default();
    response.add_atom("C", crate::document::Point::new(f32::NAN, 0.));
    let rejection = finalize(&response).unwrap_err();
    let result = rejected(
        rejection.clone(),
        vec!["engine note".into()],
        &Versions::current("9.8.7"),
    );
    assert!(result.is_error);
    assert_eq!(
        Value::Object(result.value),
        json!({
            "value": null,
            "warnings": [{"message": "engine note"}],
            "validation": {"status": "rejected", "reason": "invalid", "message": rejection.message()},
            "versions": versions_json(&Versions::current("9.8.7")),
            "error": {"code": "rejected", "message": rejection.message()},
        })
    );
}

#[test]
fn analysis_json_keeps_every_engine_field() {
    let analysis = Analysis {
        smiles: "CC".into(),
        formula: "C2H6".into(),
        mass: 30.07,
        exact_mass: 30.047,
        logp: 1.02,
        tpsa: 0.,
        donors: 0,
        acceptors: 0,
        rings: 0,
        unpaired_electrons: 0,
        inchi: "InChI=1S/C2H6/c1-2/h1-2H3".into(),
        inchikey: "OTMSDBZUPAUEDD-UHFFFAOYSA-N".into(),
    };
    assert_eq!(
        serde_json::to_value(AnalysisJson::from(analysis.clone())).unwrap(),
        serde_json::to_value(analysis).unwrap()
    );
}
