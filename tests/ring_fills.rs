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
                assert_eq!(
                    xml.descendants()
                        .filter(|n| n.has_tag_name("curve"))
                        .count(),
                    0
                );
                let area = xml
                    .descendants()
                    .find(|n| n.has_tag_name("ColoredMolecularArea"))
                    .ok_or("native fill")?;
                let fragment = area.parent().ok_or("fragment")?;
                assert!(fragment.has_tag_name("fragment"));
                let basis: Vec<_> = area
                    .attribute("BasisObjects")
                    .ok_or("basis")?
                    .split_whitespace()
                    .collect();
                assert_eq!(basis.len(), 6);
                for id in basis {
                    assert!(
                        fragment
                            .children()
                            .any(|n| n.has_tag_name("b") && n.attribute("id") == Some(id)),
                        "Native ring fills must reference bonds in their own fragment"
                    );
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
                ring_fills::palette_color([198, 233, 220], doc.canvas_theme)
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

#[tokio::test]
async fn reads_chemdraw_created_native_fill() -> Result<(), Box<dyn std::error::Error>> {
    let xml = include_str!("fixtures/ring-fills/chemdraw-native.cdxml");
    let binary =
        reshiki::exchange::from_cdx(include_bytes!("fixtures/ring-fills/chemdraw-native.cdx"))?;
    let engine = LocalEngine::default();
    for source in [xml, binary.as_str()] {
        let doc = engine
            .request(Request::import("cdxml", source))
            .await?
            .document
            .ok_or("import")?;
        assert_eq!(
            (doc.atoms.len(), doc.bonds.len(), doc.ring_fills.len()),
            (6, 6, 1)
        );
        assert!(doc.graphics.is_empty());
        let fill = &doc.ring_fills[0];
        assert_eq!(fill.color, [29, 139, 39]);
        assert!(fill.fixed_color);
        let before = fill.commands(&doc);
        let id = fill.atoms[0];
        let mut moved = doc.clone();
        moved.translate(&[id], 20., -10.);
        assert_eq!(moved.ring_fills, doc.ring_fills);
        assert_ne!(moved.ring_fills[0].commands(&moved), before);
    }
    // ChemDraw need not write the bond references in geometric order.
    let shuffled = xml.replace(
        "6494 6495 6496 6497 6498 6499",
        "6497 6494 6499 6495 6498 6496",
    );
    assert_eq!(
        engine
            .request(Request::import("cdxml", &shuffled))
            .await?
            .document
            .ok_or("shuffled")?
            .ring_fills
            .len(),
        1
    );
    for invalid in [
        "6494 6495 6496",
        "6494 6495 6496 6497 6498 6498",
        "6482 6484 6486 6488 6490 6492",
    ] {
        let broken = xml.replace("6494 6495 6496 6497 6498 6499", invalid);
        assert!(
            engine
                .request(Request::import("cdxml", &broken))
                .await
                .is_err(),
            "Reject invalid native fill: {invalid}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn native_fills_follow_grouped_fragments() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = ring();
    let second = editing::ring(&mut doc, Point::new(200., 0.), 5, false, 0.);
    let ids = doc.all_ids();
    ring_fills::apply(&mut doc, &ids, Some([198, 233, 220]));
    doc.groups.push(reshiki::grouping::Group {
        id: 1000,
        members: second,
        integral: false,
    });
    let xml = reshiki::exchange::drawing::write(&doc, Default::default())?;
    let parsed = roxmltree::Document::parse(&xml)?;
    let areas: Vec<_> = parsed
        .descendants()
        .filter(|n| n.has_tag_name("ColoredMolecularArea"))
        .collect();
    assert_eq!(areas.len(), 2);
    assert_ne!(areas[0].parent(), areas[1].parent());
    for area in areas {
        for id in area
            .attribute("BasisObjects")
            .ok_or("basis")?
            .split_whitespace()
        {
            assert!(
                area.parent()
                    .ok_or("fragment")?
                    .children()
                    .any(|n| n.has_tag_name("b") && n.attribute("id") == Some(id))
            );
        }
    }
    let back = LocalEngine::default()
        .request(Request::import("cdxml", &xml))
        .await?
        .document
        .ok_or("import")?;
    assert_eq!(back.ring_fills.len(), 2);
    assert_eq!(back.groups.len(), 1);
    Ok(())
}

#[tokio::test]
async fn imports_actual_chemdraw_return_clipboard() -> Result<(), Box<dyn std::error::Error>> {
    let xml =
        reshiki::exchange::from_cdx(include_bytes!("fixtures/ring-fills/chemdraw-return.cdx"))?;
    let doc = LocalEngine::default()
        .request(Request::import("cdxml", &xml))
        .await?
        .document
        .ok_or("return paste")?;
    assert_eq!(
        (doc.atoms.len(), doc.bonds.len(), doc.ring_fills.len()),
        (14, 15, 2)
    );
    assert!(doc.graphics.is_empty());
    assert!(
        doc.ring_fills
            .iter()
            .all(|f| f.color == [67, 99, 132] && f.fixed_color)
    );
    doc.validate()?;
    Ok(())
}
