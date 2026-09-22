//! Custom backend behavior is tested without a chemistry reference runtime.
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::engine::{ChemistryEngine, LocalEngine, Request, Response};
use std::sync::{Arc, Mutex};

type TestResult = anyhow::Result<()>;

#[derive(Clone, Default)]
struct RecordingBackend {
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
}
impl ChemistryEngine for RecordingBackend {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        self.requests
            .lock()
            .map_err(|_| "Poisoned test lock")?
            .push(serde_json::to_value(&request).map_err(|e| e.to_string())?);
        Ok(Response {
            document: None,
            analysis: None,
            output: Some("<CDXML><page id=\"1\"/></CDXML>".into()),
            engine_version: "reference".into(),
            warnings: vec!["kept".into()],
        })
    }
}
#[tokio::test]
async fn backend_receives_cdxml_and_unchanged_chemistry_requests() -> TestResult {
    let backend = RecordingBackend::default();
    let local = LocalEngine::with_backend(backend.clone());
    let input = "<CDXML><page id=\"1\"/></CDXML>";
    let bytes = reshiki::exchange::to_cdx(input).map_err(anyhow::Error::msg)?;
    local
        .execute(Request::import("cdx", &STANDARD.encode(&bytes)))
        .await
        .map_err(anyhow::Error::msg)?;
    let mut export = Request::molecule("export", Default::default());
    export.format = Some("cdx".into());
    let out = local.execute(export).await.map_err(anyhow::Error::msg)?;
    assert_eq!(out.output, Some(STANDARD.encode(bytes)));
    assert_eq!(out.warnings, vec!["kept"]);
    let request = Request::import_smiles("N[C@@H](C)C(=O)O");
    local
        .execute(request.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let requests = backend
        .requests
        .lock()
        .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?;
    assert_eq!(
        requests
            .first()
            .and_then(|r| r.get("format"))
            .and_then(|v| v.as_str()),
        Some("cdxml")
    );
    assert_eq!(
        requests
            .get(1)
            .and_then(|r| r.get("format"))
            .and_then(|v| v.as_str()),
        Some("cdxml")
    );
    assert_eq!(requests.get(2), Some(&serde_json::to_value(request)?));
    Ok(())
}

#[tokio::test]
async fn malformed_binary_never_reaches_backend_and_next_request_succeeds() -> TestResult {
    let backend = RecordingBackend::default();
    let local = LocalEngine::with_backend(backend.clone());
    for text in ["", "!!!", "VmpDRDAxMDA=", "A"] {
        assert!(local.execute(Request::import("cdx", text)).await.is_err());
    }
    let mut invalid = Request::import_smiles("CCO");
    invalid.protocol = 2;
    assert!(local.execute(invalid).await.is_err());
    assert!(
        backend
            .requests
            .lock()
            .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?
            .is_empty()
    );
    local
        .execute(Request::import_smiles("CCO"))
        .await
        .map_err(anyhow::Error::msg)?;
    assert_eq!(
        backend
            .requests
            .lock()
            .map_err(|_| anyhow::anyhow!("Poisoned test lock"))?
            .len(),
        1
    );
    Ok(())
}
