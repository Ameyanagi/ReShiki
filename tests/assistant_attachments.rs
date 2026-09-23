use anyhow::Context;
use base64::Engine;
use reshiki::{
    assistant::{
        self, Proposal,
        canvas_tools::{CanvasTools, Snapshot},
        sketch::{Atom, Bond, Centroid, ContactStyle, Sketch},
    },
    attachments::Kind,
    document::{Document, Point},
    engine::LocalEngine,
};
use serde_json::Value;
use std::sync::{Arc, RwLock};

fn arene_contact(kind: Kind) -> Proposal {
    let atom = |element: &str, x, y, hydrogens| Atom {
        element: element.into(),
        x,
        y,
        hydrogens,
        charge: 0,
        isotope: 0,
        color: None,
        variable: None,
    };
    let mut atoms = Vec::new();
    let mut bonds = Vec::new();
    for i in 0..6 {
        let angle = (i as f32 * 60.).to_radians();
        atoms.push(atom("C", angle.cos(), angle.sin(), 1));
        bonds.push(Bond {
            a: i,
            b: (i + 1) % 6,
            order: if i % 2 == 0 { 2 } else { 1 },
            display: "plain".into(),
            ring_arc: false,
        });
    }
    atoms.push(atom("Ru", 0., 3., 0));
    Proposal {
        sketch: Some(Sketch {
            atoms,
            bonds,
            shapes: vec![],
            tilts: vec![],
            centroids: vec![Centroid {
                kind: Some(kind),
                atoms: (0..6).collect(),
                contact: Some(6),
                contact_style: None,
            }],
            arrows: vec![],
            captions: vec![],
            abbreviations: vec![],
            ligands: vec![],
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn typed_attachment_proposals_preview_without_applying_and_retain_exchange_semantics()
-> anyhow::Result<()> {
    let mut original = Document::default();
    original.add_atom("O", Point::new(300., 0.));
    let canvas = Arc::new(RwLock::new(Snapshot {
        document: original.clone(),
        ..Default::default()
    }));
    let tools = CanvasTools {
        canvas: canvas.clone(),
        settings: Default::default(),
        replace: vec![],
        epoch: 0,
        revision: 0,
    };
    let engine = LocalEngine::default();
    for kind in [Kind::MultiCenter, Kind::Variable] {
        let proposal = arene_contact(kind);
        let preview = tools
            .call("canvas_preview", serde_json::to_value(&proposal)?, &engine)
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(preview["success"], true);
        let description: Value = serde_json::from_str(
            preview["contentItems"][0]["text"]
                .as_str()
                .context("Missing preview description")?,
        )?;
        assert_eq!(description["applied"], false);
        assert_eq!(description["atoms"], 8);
        assert_eq!(description["bonds"], 7);
        let encoded = preview["contentItems"][1]["imageUrl"]
            .as_str()
            .and_then(|url| url.strip_prefix("data:image/png;base64,"))
            .context("Missing preview image")?;
        let png = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let image = image::load_from_memory(&png)?;
        assert!(image.width() > 0 && image.height() > 0);
        assert_eq!(
            canvas
                .read()
                .map_err(|_| anyhow::anyhow!("Canvas lock"))?
                .document,
            original
        );

        let fragment = assistant::render(&engine, &proposal, &Default::default())
            .await
            .map_err(anyhow::Error::msg)?;
        assert!(reshiki::chemistry::document::prepare(&fragment).is_err());
        let (candidate, _) =
            assistant::candidate(&original, &fragment, &[]).map_err(anyhow::Error::msg)?;
        candidate.validate().map_err(anyhow::Error::msg)?;
        assert_eq!(candidate.atoms.len(), 9);
        let xml = reshiki::exchange::drawing::write(&fragment, Default::default())?;
        let binary = reshiki::exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
        for doc in [
            fragment.clone(),
            reshiki::chemistry::cdxml::import_cdxml(&xml)?.document,
            reshiki::chemistry::cdxml::import_cdxml(
                &reshiki::exchange::from_cdx(&binary).map_err(anyhow::Error::msg)?,
            )?
            .document,
        ] {
            let attachment = doc
                .atoms
                .iter()
                .find(|a| a.attachment == Some(kind))
                .context("Lost typed attachment")?;
            assert_eq!(attachment.element, "*");
            assert_eq!(attachment.centroid.len(), 6);
            let metal = doc
                .atoms
                .iter()
                .find(|a| a.element == "Ru")
                .context("Lost Ru")?;
            let contacts: Vec<_> = doc
                .bonds
                .iter()
                .filter(|b| b.a == metal.id || b.b == metal.id)
                .collect();
            assert_eq!(contacts.len(), 1, "No invented metal-carbon sigma bonds");
            let contact = contacts.first().context("Lost contact")?;
            assert!(contact.a == attachment.id || contact.b == attachment.id);
            assert_eq!((contact.order, contact.display.as_str()), (1, "plain"));
        }
        // Reject a partial group that would pull the ligand targets out of the
        // attachment's fragment, instead of writing an unreadable drawing.
        let mut split = fragment.clone();
        split
            .groups
            .first_mut()
            .context("Missing sketch group")?
            .members = fragment
            .atoms
            .iter()
            .filter(|a| a.element == "C")
            .map(|a| a.id)
            .collect();
        split.validate().map_err(anyhow::Error::msg)?;
        let error = reshiki::exchange::drawing::write(&split, Default::default())
            .err()
            .context("Partial attachment group was exported")?;
        assert!(
            error
                .to_string()
                .contains("A group cuts through a molecule")
        );
    }
    Ok(())
}

#[test]
fn contact_appearances_preserve_attachment_targets_through_exchange() -> anyhow::Result<()> {
    for kind in [Kind::MultiCenter, Kind::Variable] {
        for (style, expected) in [
            (ContactStyle::Single, (1, "plain")),
            (ContactStyle::Dashed, (5, "dashed")),
            (ContactStyle::Dative, (5, "plain")),
        ] {
            let mut proposal = arene_contact(kind);
            let sketch = proposal.sketch.as_mut().context("Missing sketch")?;
            sketch
                .centroids
                .first_mut()
                .context("Missing point")?
                .contact_style = Some(style);
            let document = sketch
                .render(&Default::default())
                .map_err(anyhow::Error::msg)?;
            let before = document.clone();
            let xml = reshiki::exchange::drawing::write(&document, Default::default())?;
            let cdx = reshiki::exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?;
            for doc in [
                document.clone(),
                reshiki::chemistry::cdxml::import_cdxml(&xml)?.document,
                reshiki::chemistry::cdxml::import_cdxml(
                    &reshiki::exchange::from_cdx(&cdx).map_err(anyhow::Error::msg)?,
                )?
                .document,
            ] {
                let point = doc
                    .atoms
                    .iter()
                    .find(|a| a.attachment == Some(kind))
                    .context("Lost point")?;
                let contact = doc
                    .bonds
                    .iter()
                    .find(|b| b.a == point.id || b.b == point.id)
                    .context("Lost contact")?;
                assert_eq!((contact.order, contact.display.as_str()), expected);
                assert_eq!(point.centroid.len(), 6);
                assert_eq!(doc.atoms.len(), 8);
                assert_eq!(doc.bonds.len(), 7);
                assert!(
                    doc.atom(contact.b).is_some_and(|a| a.element == "Ru"),
                    "Dative direction must run from the ligand to the metal"
                );
                assert!(
                    !assistant::canvas_tools::image(&doc)
                        .map_err(anyhow::Error::msg)?
                        .is_empty()
                );
            }
            assert_eq!(document, before);
        }
    }
    Ok(())
}

#[test]
fn attachment_review_checks_ligand_errors_without_misreporting_export_limits() -> anyhow::Result<()>
{
    for kind in [Kind::MultiCenter, Kind::Variable] {
        let proposal = arene_contact(kind);
        let mut doc = proposal
            .sketch
            .context("Missing sketch")?
            .render(&Default::default())
            .map_err(anyhow::Error::msg)?;
        let before = doc.clone();
        let issues = assistant::review::quality(&doc, &Default::default());
        assert!(
            issues
                .iter()
                .any(|s| s.contains("full coordination-valence validation is unavailable"))
        );
        assert!(
            !issues
                .iter()
                .any(|s| s.contains("Chemical assignments need review")
                    || s.contains("Invalid drawing")
                    || s.contains("ordinary molecular identifiers")),
            "{issues:?}"
        );
        assert!(
            !assistant::review::Report {
                verified: true,
                issues,
                ..Default::default()
            }
            .can_auto_apply()
        );
        assert_eq!(
            doc, before,
            "Review must not strip the actual attachment targets"
        );

        // The previous check stopped at the attachment export restriction and
        // never reached this genuine valence error in the explicitly drawn graph.
        let n = doc.add_atom("N", Point::new(150., 130.));
        for i in 0..5 {
            let angle = (i as f32 * 72.).to_radians();
            let c = doc.add_atom(
                "C",
                Point::new(150. + 40. * angle.cos(), 130. + 40. * angle.sin()),
            );
            doc.add_bond(n, c, 1, "plain");
        }
        let invalid = doc.clone();
        let issues = assistant::review::quality(&doc, &Default::default());
        assert!(
            issues
                .iter()
                .any(|s| s.starts_with("Chemical assignments need review:")
                    && s.to_lowercase().contains("valence")),
            "{issues:?}"
        );
        assert!(
            !issues
                .iter()
                .any(|s| s.contains("ordinary molecular identifiers"))
        );
        assert_eq!(doc, invalid);
    }
    Ok(())
}

#[test]
fn contact_style_is_backward_compatible_and_requires_a_contact() -> anyhow::Result<()> {
    let proposal = arene_contact(Kind::MultiCenter);
    let mut value = serde_json::to_value(&proposal)?;
    value
        .pointer_mut("/sketch/centroids/0")
        .and_then(Value::as_object_mut)
        .context("Missing point")?
        .remove("contact_style");
    let decoded: Proposal = serde_json::from_value(value)?;
    let mut sketch = decoded.sketch.context("Missing sketch")?;
    let legacy = sketch
        .render(&Default::default())
        .map_err(anyhow::Error::msg)?;
    assert!(
        legacy
            .bonds
            .last()
            .is_some_and(|b| b.order == 1 && b.display == "plain")
    );
    let centroid = sketch.centroids.first_mut().context("Missing point")?;
    centroid.contact = None;
    centroid.contact_style = Some(ContactStyle::Dashed);
    assert!(sketch.validate().is_err());
    Ok(())
}
