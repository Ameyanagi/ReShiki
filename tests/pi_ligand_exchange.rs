//! Regression captures from an independent desktop CDX reader/writer.
use anyhow::{Context, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::{
    document::Document,
    engine::{LocalEngine, Request},
};

#[tokio::test]
async fn native_cp_and_returned_two_cp_drawing_keep_aromatic_editable_rings() -> anyhow::Result<()>
{
    for (bytes, atoms, aromatic, charges) in [
        (
            include_bytes!("fixtures/ligand-exchange/native.cdx").as_slice(),
            7,
            5,
            0,
        ),
        (
            include_bytes!("fixtures/ligand-exchange/returned.cdx").as_slice(),
            13,
            10,
            2,
        ),
    ] {
        let xml = reshiki::exchange::from_cdx(bytes).map_err(anyhow::Error::msg)?;
        let tree = roxmltree::Document::parse(&xml)?;
        ensure!(!tree.descendants().any(|n| n.has_tag_name("embeddedobject")));
        ensure!(
            tree.descendants()
                .filter(|n| n.has_tag_name("b") && n.attribute("Order") == Some("1.5"))
                .count()
                == aromatic
        );
        let response = LocalEngine::default()
            .request(Request::import("cdx", &STANDARD.encode(bytes)))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = response.document.context("Imported editable drawing")?;
        ensure!(doc.atoms.len() == atoms && doc.bonds.len() == atoms - 1);
        ensure!(doc.bonds.iter().filter(|b| b.order == 4).count() == aromatic);
        ensure!(
            doc.atoms
                .iter()
                .filter(|a| a.charge == -1 && a.display.hide_charge)
                .count()
                == charges
        );
        ensure!(
            doc.atoms
                .iter()
                .filter(|a| a.attachment.is_some() && a.centroid.len() == 5)
                .count()
                == aromatic / 5
        );
        ensure!(doc.atoms.iter().all(|a| a.stereo.is_none()));
        ensure!(
            doc.graphics.is_empty(),
            "Circles {} duplicate ellipse or picture: {:?}",
            reshiki::aromatic::circles(&doc).len(),
            doc.graphics
        );
        ensure!(reshiki::aromatic::circles(&doc).len() == aromatic / 5);
    }
    Ok(())
}

#[tokio::test]
async fn exported_cp_retains_charge_and_closed_curve_without_forced_carbon_labels()
-> anyhow::Result<()> {
    let source: Document =
        serde_json::from_str(include_str!("../docs/changes/fixtures/pi-ligands-copy.rsk"))?;
    let xml = reshiki::exchange::drawing::write(&source, Default::default())?;
    let bytes = reshiki::exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
    let decoded = reshiki::exchange::from_cdx(&bytes).map_err(anyhow::Error::msg)?;
    let tree = roxmltree::Document::parse(&decoded)?;
    let curves: Vec<_> = tree
        .descendants()
        .filter(|n| n.has_tag_name("curve"))
        .collect();
    ensure!(curves.len() == 2);
    for curve in curves {
        ensure!(curve.attribute("Closed") == Some("yes"));
        ensure!(
            curve
                .attribute("CurvePoints")
                .context("Curve points")?
                .split_whitespace()
                .count()
                == 24
        );
    }
    for carbon in tree
        .descendants()
        .filter(|n| n.has_tag_name("n") && n.attribute("Element") == Some("6"))
    {
        ensure!(carbon.attribute("NumHydrogens").is_none());
        ensure!(!carbon.children().any(|n| n.has_tag_name("t")));
    }
    let response = LocalEngine::default()
        .request(Request::import("cdx", &STANDARD.encode(bytes)))
        .await
        .map_err(anyhow::Error::msg)?;
    let doc = response.document.context("Imported copy")?;
    ensure!(doc.graphics.is_empty());
    ensure!(doc.bonds.iter().filter(|b| b.order == 4).count() == 10);
    ensure!(
        doc.atoms
            .iter()
            .filter(|a| a.charge == -1 && a.display.hide_charge)
            .count()
            == 2
    );
    // The complete original, including true projection depth, stays native.
    ensure!(source.atoms.iter().any(|a| a.depth != 0.));
    Ok(())
}

#[tokio::test]
async fn external_gallery_pastes_back_with_all_pi_ligands() -> anyhow::Result<()> {
    let data = include_bytes!("fixtures/ligand-exchange/gallery-returned.cdx");
    let response = LocalEngine::default()
        .request(Request::import("cdx", &STANDARD.encode(data)))
        .await
        .map_err(anyhow::Error::msg)?;
    let doc = response.document.context("Returned gallery drawing")?;
    ensure!(
        doc.atoms.len() == 483 && doc.bonds.len() == 429,
        "Returned topology: {} atoms / {} bonds",
        doc.atoms.len(),
        doc.bonds.len()
    );
    ensure!(
        doc.annotations.len() == 132,
        "Captions: {}",
        doc.annotations.len()
    );
    ensure!(doc.graphics.iter().all(|g| g.picture.is_none()));
    ensure!(doc.atoms.iter().filter(|a| a.attachment.is_some()).count() == 5);
    ensure!(doc.bonds.iter().filter(|b| b.order == 4).count() == 33);
    ensure!(
        doc.atoms
            .iter()
            .filter(|a| a.charge == -1 && a.display.hide_charge)
            .count()
            == 3
    );
    ensure!(doc.atoms.iter().any(|a| a.element == "Fe" && a.charge == 2));
    Ok(())
}

#[tokio::test]
async fn query_labels_are_explicitly_drawing_only() -> anyhow::Result<()> {
    let xml = "<CDXML><page id='1'><fragment id='2'><n id='3' p='0 0' NodeType='GenericNickname' GenericNickname='R'><t><s>R</s></t></n></fragment></page></CDXML>";
    let response = LocalEngine::default()
        .request(Request::import("cdxml", xml))
        .await
        .map_err(anyhow::Error::msg)?;
    ensure!(response.analysis.is_none());
    ensure!(
        response
            .warnings
            .iter()
            .any(|w| w.contains("query semantics"))
    );
    let doc = response.document.context("Variable drawing")?;
    ensure!(
        doc.atoms
            .first()
            .context("Variable atom")?
            .display
            .variable
            .as_deref()
            == Some("R")
    );
    // The chemical parser still rejects unsupported query meaning.
    ensure!(reshiki::chemistry::cdxml::read(xml).is_err());
    Ok(())
}

#[tokio::test]
async fn an_unrelated_closed_ellipse_remains_an_editable_graphic() -> anyhow::Result<()> {
    let xml = reshiki::exchange::from_cdx(include_bytes!("fixtures/ligand-exchange/native.cdx"))
        .map_err(anyhow::Error::msg)?;
    let tree = roxmltree::Document::parse(&xml)?;
    let points = tree
        .descendants()
        .find(|n| n.has_tag_name("curve"))
        .and_then(|n| n.attribute("CurvePoints"))
        .context("Native ellipse")?;
    let moved = points
        .split_whitespace()
        .map(|value| value.parse::<f64>().map(|value| (value + 200.).to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    let moved_xml = xml.replacen(points, &moved, 1);
    let response = LocalEngine::default()
        .request(Request::import("cdxml", &moved_xml))
        .await
        .map_err(anyhow::Error::msg)?;
    let doc = response.document.context("Imported drawing")?;
    ensure!(doc.graphics.len() == 1 && doc.graphics.iter().all(|g| g.picture.is_none()));
    ensure!(doc.bonds.iter().filter(|b| b.order == 4).count() == 5);
    Ok(())
}
