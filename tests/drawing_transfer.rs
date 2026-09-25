use anyhow::{Context, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::{
    document::Document,
    engine::{LocalEngine, Request},
    exchange,
};

async fn export(engine: &LocalEngine, doc: &Document, format: &str) -> anyhow::Result<String> {
    let mut request = Request::molecule("export", doc.clone());
    request.format = Some(format.into());
    engine
        .request(request)
        .await
        .map_err(anyhow::Error::msg)?
        .output
        .context("Missing export")
}

#[tokio::test]
async fn aromatic_five_member_drawing_transfers_without_inventing_chemistry() -> anyhow::Result<()>
{
    for bytes in [
        include_bytes!("fixtures/aromatic-five-attachment.cdx").as_slice(),
        include_bytes!("fixtures/aromatic-five-attachment-return.cdx").as_slice(),
    ] {
        let xml = exchange::from_cdx(bytes).map_err(anyhow::Error::msg)?;
        ensure!(
            reshiki::chemistry::cdxml::prepare_cdxml(&xml).is_err(),
            "Fixture must exercise rejected chemistry"
        );
        let engine = LocalEngine::default();
        for (format, input) in [("cdx", STANDARD.encode(bytes)), ("cdxml", xml)] {
            let imported = engine
                .request(Request::import(format, &input))
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(imported.analysis.is_none() && !imported.warnings.is_empty());
            let doc = imported.document.context("Missing preserved drawing")?;
            ensure!(doc.atoms.len() == 7, "{} atoms", doc.atoms.len());
            ensure!(doc.bonds.iter().filter(|b| b.order == 4).count() == 5);
            ensure!(
                doc.atoms
                    .iter()
                    .all(|a| a.charge == 0 && a.cip_label.is_none())
            );
            let before = serde_json::to_value(&doc)?;
            ensure!(
                engine
                    .request(Request::molecule("analyze", doc.clone()))
                    .await
                    .is_err()
            );
            for format in ["cdx", "cdxml"] {
                let output = export(&engine, &doc, format).await?;
                let back = engine
                    .request(Request::import(format, &output))
                    .await
                    .map_err(anyhow::Error::msg)?;
                ensure!(back.analysis.is_none() && !back.warnings.is_empty());
                let back = back.document.context("Missing round-trip drawing")?;
                ensure!(doc.atoms.len() == back.atoms.len());
                ensure!(doc.bonds.len() == back.bonds.len());
                for (a, b) in doc.atoms.iter().zip(&back.atoms) {
                    ensure!(
                        (
                            &a.element,
                            a.charge,
                            a.explicit_h,
                            a.attachment,
                            &a.centroid
                        ) == (
                            &b.element,
                            b.charge,
                            b.explicit_h,
                            b.attachment,
                            &b.centroid
                        )
                    );
                }
                for (a, b) in doc.bonds.iter().zip(&back.bonds) {
                    ensure!((a.a, a.b, a.order, &a.display) == (b.a, b.b, b.order, &b.display));
                }
            }
            ensure!(before == serde_json::to_value(&doc)?);
        }
    }
    Ok(())
}

#[tokio::test]
async fn malformed_drawing_is_still_rejected() -> anyhow::Result<()> {
    let xml = "<CDXML><page><fragment><n id=\"1\" p=\"0 0\"/><b id=\"2\" B=\"1\" E=\"99\"/></fragment></page></CDXML>";
    ensure!(
        LocalEngine::default()
            .request(Request::import("cdxml", xml))
            .await
            .is_err()
    );
    Ok(())
}
