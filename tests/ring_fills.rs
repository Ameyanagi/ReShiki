use reshiki::{
    document::{Document, Point},
    editing,
    engine::{LocalEngine, Request},
    ring_fills,
    scene::Primitive,
};
fn ring() -> Document {
    let mut doc = Document::default();
    editing::ring(&mut doc, Point::default(), 6, false, 0.);
    doc
}
#[test]
fn fill_tracks_geometry_copy_delete_and_rejects_invalid_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = ring();
    let ids = doc.all_ids();
    let chemistry = doc.bonds.clone();
    assert_eq!(ring_fills::apply(&mut doc, &ids, Some([201, 224, 248])), 1);
    assert_eq!(doc.bonds, chemistry);
    let original = doc.ring_fills.clone();
    editing::scale_axes_about(&mut doc, &ids, Point::default(), 1.6, 0.7);
    reshiki::projection::tilt(&mut doc, &ids, 35., true);
    doc.translate(&ids, 30., -20.);
    assert_eq!(doc.ring_fills, original);
    let fill = doc.ring_fills.first().ok_or("fill")?;
    let commands = fill.commands(&doc);
    assert_eq!(commands.len(), 7);
    for (command, id) in commands.iter().zip(&fill.atoms) {
        let p = match command {
            reshiki::graphics::PathCommand::Move(p) | reshiki::graphics::PathCommand::Line(p) => p,
            _ => return Err("Unexpected path".into()),
        };
        assert_eq!(*p, doc.atom(*id).ok_or("atom")?.position);
    }
    let scene = reshiki::scene::primitives(&doc);
    assert!(matches!(
        scene.first(),
        Some(Primitive::Path { filled: true, .. })
    ));
    let mut pasted = ring();
    editing::append(&mut pasted, &doc, Point::new(200., 0.));
    pasted.validate()?;
    assert!(
        pasted
            .ring_fills
            .first()
            .ok_or("paste")?
            .atoms
            .iter()
            .all(|id| !ids.contains(id))
    );
    let partial = editing::selection(&doc, ids.get(..3).ok_or("partial")?);
    assert!(partial.ring_fills.is_empty());
    assert_eq!(editing::selection(&doc, &ids).ring_fills, doc.ring_fills);
    let mut duplicate = doc.clone();
    duplicate.ring_fills.extend(original);
    assert!(duplicate.validate().is_err());
    let mut broken = doc.clone();
    broken.bonds.pop();
    assert!(broken.validate().is_err());
    ring_fills::prune(&mut broken);
    broken.validate()?;
    assert!(broken.ring_fills.is_empty());
    doc.delete(ids.get(..1).ok_or("one")?);
    assert!(doc.ring_fills.is_empty());
    Ok(())
}
#[tokio::test]
async fn colors_survive_native_clean_and_editable_roundtrip()
-> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::default();
    for aromatic in [false, true] {
        let mut doc = Document::default();
        let ids = editing::ring(&mut doc, Point::default(), 6, aromatic, 0.);
        ring_fills::apply(&mut doc, &ids, Some([198, 233, 220]));
        let saved: Document = serde_json::from_str(&serde_json::to_string(&doc)?)?;
        assert_eq!(doc, saved);
        for action in ["analyze", "clean"] {
            let response = engine
                .request(Request::molecule(action, doc.clone()))
                .await?;
            assert_eq!(
                response.document.ok_or("drawing")?.ring_fills,
                doc.ring_fills
            );
            assert_eq!(
                response.analysis.ok_or("analysis")?.formula,
                if aromatic { "C6H6" } else { "C6H12" }
            );
        }
        for format in ["cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let output = engine.request(request).await?.output.ok_or("export")?;
            if format == "cdxml" {
                let xml = roxmltree::Document::parse(&output)?;
                for curve in xml
                    .descendants()
                    .filter(|n| n.attribute("Name") == Some("ReShiki ring fill"))
                {
                    assert_eq!(curve.attribute("CurveType"), Some("129"));
                    assert!(
                        curve.attribute("Closed").is_none(),
                        "Closure is encoded in CurveType for binary-reader compatibility"
                    );
                }
                let z: Vec<usize> = xml
                    .descendants()
                    .filter_map(|n| n.attribute("Z"))
                    .map(str::parse)
                    .collect::<Result<_, _>>()?;
                assert_eq!(
                    z.len(),
                    z.iter().collect::<std::collections::HashSet<_>>().len(),
                    "Stacking ordinals must be unique for opaque fills"
                );
                let fill_z = xml
                    .descendants()
                    .find(|n| n.attribute("Name") == Some("ReShiki ring fill"))
                    .and_then(|n| n.attribute("Z"))
                    .ok_or("fill z")?
                    .parse::<usize>()?;
                for bond in xml.descendants().filter(|n| n.has_tag_name("b")) {
                    assert!(bond.attribute("Z").ok_or("bond z")?.parse::<usize>()? > fill_z);
                }
            }
            let back = engine
                .request(Request::import(format, &output))
                .await?
                .document
                .ok_or("import")?;
            back.validate()?;
            assert_eq!(back.ring_fills.len(), 1, "{format}");
            assert_eq!(
                back.ring_fills.first().ok_or("fill")?.color,
                [198, 233, 220]
            );
            assert!(
                back.graphics.is_empty(),
                "Owned fill should not duplicate a loose graphic"
            );
        }
        for format in ["svg", "png", "pdf"] {
            assert!(!reshiki::export::drawing(&doc, format)?.is_empty());
        }
    }
    Ok(())
}
#[test]
fn fused_rings_are_independently_colored_and_partial_selections_do_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = ring();
    let initial = doc.all_ids();
    let a = *initial.first().ok_or("a")?;
    let b = *initial.get(1).ok_or("b")?;
    let c = doc.add_atom("C", Point::new(90., -30.));
    doc.add_bond(a, c, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    let all = doc.all_ids();
    assert_eq!(ring_fills::apply(&mut doc, &all, Some([255, 241, 174])), 2);
    assert_eq!(
        ring_fills::apply(&mut doc, &[a, b, c], Some([201, 224, 248])),
        1
    );
    assert_eq!(ring_fills::apply(&mut doc, &[a, b], None), 0);
    assert_eq!(ring_fills::apply(&mut doc, &[a, b, c], None), 1);
    assert_eq!(doc.ring_fills.len(), 1);
    assert_eq!(doc.ring_fills.first().ok_or("fill")?.atoms.len(), 6);
    doc.validate()?;
    // Saturated chair rings must not depend on aromatic-circle clearance.
    let mut chair = reshiki::rings::Preset::ChairUp.document(42., false);
    let ids = chair.all_ids();
    assert_eq!(
        ring_fills::apply(&mut chair, &ids, Some([226, 211, 245])),
        1
    );
    Ok(())
}
