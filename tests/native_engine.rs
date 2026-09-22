//! Application workflows that run with only the native chemistry runtime.
use anyhow::{Context, ensure};
use reshiki::{
    document::Document,
    engine::{LocalEngine, Request, Response},
};

async fn execute(engine: &LocalEngine, request: Request) -> anyhow::Result<Response> {
    engine.request(request).await.map_err(anyhow::Error::msg)
}

fn ethanol(response: &Response) -> anyhow::Result<()> {
    let analysis = response.analysis.as_ref().context("Missing analysis")?;
    ensure!(analysis.formula == "C2H6O");
    ensure!(analysis.smiles == "CCO");
    ensure!(analysis.inchikey == "LFQSCWFLJHTTHZ-UHFFFAOYSA-N");
    let document = response.document.as_ref().context("Missing drawing")?;
    ensure!(document.atoms.len() == 3 && document.bonds.len() == 2);
    document.validate().map_err(anyhow::Error::msg)
}

#[tokio::test]
async fn molecule_import_cleanup_and_exchange_need_only_the_native_runtime() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    let imported = execute(&engine, Request::import_smiles("CCO")).await?;
    ethanol(&imported)?;
    let drawing = imported.document.context("Missing imported drawing")?;
    let original = serde_json::to_value(&drawing)?;
    let cleaned = execute(&engine, Request::molecule("clean", drawing.clone())).await?;
    ethanol(&cleaned)?;
    for format in ["smiles", "mol", "inchi", "cdxml", "cdx"] {
        let mut request = Request::molecule("export", drawing.clone());
        request.format = Some(format.into());
        let exported = execute(&engine, request)
            .await
            .with_context(|| format!("Exporting {format}"))?;
        let text = exported.output.context("Missing exported structure")?;
        let restored = execute(&engine, Request::import(format, &text))
            .await
            .with_context(|| format!("Importing {format}"))?;
        ethanol(&restored)?;
    }
    ensure!(serde_json::to_value(drawing)? == original);
    Ok(())
}

#[tokio::test]
async fn reaction_layout_and_exchange_preserve_roles() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    let imported = execute(&engine, Request::import("rsmi", "CCO>>CC=O")).await?;
    let drawing = imported.document.context("Missing reaction drawing")?;
    ensure!(drawing.reactions.len() == 1);
    ensure!(drawing.atoms.len() == 6 && drawing.bonds.len() == 4);
    for format in ["rxn", "rsmi"] {
        let mut request = Request::molecule("export", drawing.clone());
        request.format = Some(format.into());
        let output = execute(&engine, request)
            .await?
            .output
            .context("Missing exported reaction")?;
        let restored = execute(&engine, Request::import(format, &output))
            .await?
            .document
            .context("Missing restored reaction")?;
        ensure!(restored.reactions.len() == 1);
        ensure!(restored.atoms.len() == 6 && restored.bonds.len() == 4);
        restored.validate().map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

#[tokio::test]
async fn empty_exports_and_invalid_requests_have_native_responses() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", Document::default());
        request.format = Some(format.into());
        let exported = execute(&engine, request).await?;
        ensure!(exported.analysis.is_none());
        let text = exported.output.context("Missing empty figure")?;
        // An empty page is exportable, but has no object for an import to add.
        ensure!(matches!(
            engine.request(Request::import(format, &text)).await,
            Err(error) if error == "No supported drawing objects found"
        ));
    }
    let mut invalid = Request::import_smiles("CCO");
    invalid.protocol = 2;
    ensure!(
        matches!(engine.request(invalid).await, Err(error) if error == "Unsupported protocol version")
    );
    let unknown = Request::molecule("unknown", Document::default());
    ensure!(
        matches!(engine.request(unknown).await, Err(error) if error == "Unknown chemistry operation")
    );
    Ok(())
}
