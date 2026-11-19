use super::*;
use crate::{chemistry::cdxml, document::Annotation, typography::TextFormat};

/// Three text intermediates made into a cycle, the first step named.
fn cycle_document() -> Document {
    let mut document = Document::default();
    let mut ids = vec![];
    for (x, y, text) in [
        (0., -200., "Pd(0)L2"),
        (170., 90., "ArPdX"),
        (-230., 90., "ArPdAr"),
    ] {
        let id = document.next_id();
        document.annotations.push(Annotation {
            id,
            position: Point::new(x, y),
            text: text.into(),
            format: TextFormat {
                style: document.drawing_style.text_style(),
                ..Default::default()
            },
        });
        ids.push(id);
    }
    let cycle = crate::cycles::make(&mut document, &ids).unwrap();
    if let Some(c) = document
        .graphics
        .iter_mut()
        .find(|g| g.id == cycle)
        .and_then(|g| g.cycle.as_mut())
    {
        c.steps[0].name = "Oxidative addition".into();
    }
    document
}

#[test]
fn a_cycle_exports_as_its_arrows_and_text_grouped_with_its_parts() {
    let document = cycle_document();
    let request = Request::molecule("export", document.clone());
    let xml = write(&document, (&request).into()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(xml).unwrap();
        // Three arc arrows and no oval for the circle itself.
        let arrows = tree
            .descendants()
            .filter(|n| n.has_tag_name("arrow"))
            .count();
        assert_eq!(arrows, 3);
        assert!(
            !tree
                .descendants()
                .any(|n| n.has_tag_name("graphic") && n.attribute("GraphicType") == Some("Oval"))
        );
        let imported = cdxml::import_cdxml(xml).unwrap().document;
        assert_eq!(imported.arrows.len(), 3);
        assert!(imported.arrows.iter().all(|a| a.arc.is_some()));
        assert!(
            imported
                .annotations
                .iter()
                .any(|a| a.text.replace('\n', " ") == "Oxidative addition")
        );
        assert!(imported.graphics.iter().all(|g| g.cycle.is_none()));
    }
    let notice = crate::arrows::export_notice(&document).unwrap();
    assert!(notice.contains("1 catalytic cycle as plain arrows and text"));
}
