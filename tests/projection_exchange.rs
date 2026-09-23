use reshiki::document::{Document, Point};

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
