use reshiki::document::{Document, Point};

#[tokio::test]
async fn native_clipboard_round_trip_retains_editable_bold_arene() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use base64::Engine;
    use reshiki::engine::{LocalEngine, Request};
    let engine = LocalEngine::default();
    for bytes in [
        include_bytes!("fixtures/bold-arene-clipboard.cdx").as_slice(),
        include_bytes!("fixtures/bold-arene-clipboard-return.cdx").as_slice(),
    ] {
        let response = engine
            .request(Request::import(
                "cdx",
                &base64::engine::general_purpose::STANDARD.encode(bytes),
            ))
            .await
            .map_err(anyhow::Error::msg)?;
        let doc = response.document.context("Editable document")?;
        ensure!(doc.atoms.len() == 7 && doc.bonds.len() == 7 && doc.graphics.is_empty());
        ensure!(doc.bonds.iter().filter(|b| b.order == 2).count() == 3);
        ensure!(
            doc.bonds
                .iter()
                .filter(|b| b.display == "bold" && b.order == 1)
                .count()
                == 1
        );
        let fluorine = doc
            .atoms
            .iter()
            .find(|a| a.element == "F")
            .context("Fluorine")?
            .id;
        let branch = doc
            .bonds
            .iter()
            .find(|b| b.a == fluorine || b.b == fluorine)
            .context("C–F")?;
        ensure!(branch.color == [43, 112, 97] && branch.order == 1);
        ensure!(doc.atoms.iter().all(|a| a.stereo.is_none()));
        ensure!(response.analysis.context("Analysis")?.formula == "C6H5F");
    }
    Ok(())
}

#[tokio::test]
async fn aromatic_bold_front_edge_copies_as_editable_bonds_without_stereo() -> anyhow::Result<()> {
    use anyhow::Context;
    use reshiki::engine::{LocalEngine, Request};
    let source: Document =
        serde_json::from_str(include_str!("../docs/changes/fixtures/arene-bold-join.rsk"))?;
    let engine = LocalEngine::default();
    let reference = engine
        .request(Request::molecule("analyze", source.clone()))
        .await
        .map_err(anyhow::Error::msg)?
        .analysis
        .context("Reference analysis")?;
    for circular in [false, true] {
        let mut doc = source.clone();
        if circular {
            let ring: Vec<_> = doc
                .atoms
                .iter()
                .filter(|a| a.element == "C")
                .map(|a| a.id)
                .collect();
            for bond in doc
                .bonds
                .iter_mut()
                .filter(|b| ring.contains(&b.a) && ring.contains(&b.b))
            {
                bond.order = 4;
            }
        }
        for format in ["cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let data = engine
                .request(request)
                .await
                .map_err(anyhow::Error::msg)?
                .output
                .context("Export")?;
            let response = engine
                .request(Request::import(format, &data))
                .await
                .map_err(anyhow::Error::msg)?;
            let imported = response.document.context("Imported drawing")?;
            assert_eq!(imported.atoms.len(), doc.atoms.len());
            assert_eq!(imported.bonds.len(), doc.bonds.len());
            assert!(imported.atoms.iter().all(|a| a.stereo.is_none()));
            assert_eq!(
                response
                    .analysis
                    .with_context(|| format!(
                        "Analysis {format} circular={circular}: {:?}",
                        response.warnings
                    ))?
                    .inchikey,
                reference.inchikey
            );
            for (a, b) in doc.bonds.iter().zip(&imported.bonds) {
                assert_eq!(
                    (&a.display, a.color, a.order),
                    (&b.display, b.color, b.order)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn projection_emphasis_does_not_invent_stereochemistry_in_cdxml() -> anyhow::Result<()> {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    for (element, x, y) in [
        ("F", -40., 0.),
        ("Cl", 40., 0.),
        ("Br", 0., -40.),
        ("I", 0., 40.),
    ] {
        let atom = doc.add_atom(element, Point::new(x, y));
        doc.add_bond(center, atom, 1, "plain");
    }
    let bond = doc
        .bonds
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("bond"))?;
    bond.projection = true;
    bond.display = "bold".into();
    let error = reshiki::exchange::drawing::write(&doc, Default::default())
        .err()
        .ok_or_else(|| anyhow::anyhow!("Expected unsupported projection export"))?;
    assert!(error.to_string().contains("front-bond emphasis"));
    for format in ["svg", "png", "pdf"] {
        assert!(
            !reshiki::export::drawing(&doc, format)
                .map_err(anyhow::Error::msg)?
                .is_empty()
        );
    }
    let restored: Document = serde_json::from_str(&serde_json::to_string(&doc)?)?;
    assert_eq!(restored, doc);
    let bond = doc
        .bonds
        .first_mut()
        .ok_or_else(|| anyhow::anyhow!("bond"))?;
    bond.display = "plain".into();
    let xml = reshiki::exchange::drawing::write(&doc, Default::default())?;
    let restored = reshiki::chemistry::cdxml::import_cdxml(&xml)?.document;
    assert!(restored.atoms.iter().all(|atom| atom.stereo.is_none()));
    Ok(())
}
