//! Custom backends receive unmodified requests without entering native dispatch.
use super::*;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct DeferredBackend(Arc<Mutex<Option<serde_json::Value>>>);

impl ChemistryEngine for DeferredBackend {
    async fn execute(&self, request: Request) -> Result<Response, String> {
        *self.0.lock().await = Some(serde_json::to_value(request).map_err(|e| e.to_string())?);
        Ok(Response {
            document: None,
            analysis: None,
            output: None,
            engine_version: "deferred test backend".into(),
            warnings: vec![],
        })
    }
}

#[tokio::test]
async fn custom_backends_preserve_the_original_request() -> anyhow::Result<()> {
    let backend = DeferredBackend::default();
    let local = LocalEngine::with_backend(backend.clone());
    for (format, text) in [
        ("smiles", " \t C[C@H](O)F\r\n"),
        ("inchi", " \t InChI=1S/CH4/h1H4\r\n"),
        ("rsmi", " \t C>>O\r\n"),
    ] {
        let mut request = Request::import(format, text);
        request.selected_ids = Some(vec![9_007_199_254_740_993, 4]);
        request.text_layout = Some(std::collections::HashMap::from([(
            7,
            TextMetrics {
                width: 0.1,
                height: -0.2,
                baseline: 0.3,
            },
        )]));
        request.document = Some(Document::default());
        let expected = serde_json::to_value(&request)?;
        let result = local
            .execute(request.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(result.engine_version == "deferred test backend");
        anyhow::ensure!(backend.0.lock().await.as_ref() == Some(&expected));
        anyhow::ensure!(serde_json::to_value(request)? == expected);
    }
    Ok(())
}
