use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Arrow, Point},
    purpose::Purpose,
};

#[tokio::test]
async fn reference_and_illustration_purposes_come_back_from_cdxml() {
    let mut document = crate::formats::read(b"CCO.CCN".to_vec(), "smiles")
        .await
        .unwrap();
    document.arrows.push(Arrow::new(
        900,
        Point::new(0., -80.),
        Point::new(120., -80.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    // A retrosynthesis arrow also writes a superseded legacy graphic.
    document.arrows.push(Arrow::new(
        901,
        Point::new(0., -160.),
        Point::new(120., -160.),
        Preset::Retro,
        ArrowStyle::preset(Preset::Retro),
    ));
    let nitrogen = document.atoms.iter().find(|a| a.element == "N").unwrap().id;
    crate::purpose::set(&mut document, &[nitrogen, 901], Purpose::Reference);
    crate::purpose::set(&mut document, &[900], Purpose::Illustration);
    let purposes = |doc: &crate::document::Document| {
        let mut atoms: Vec<_> = doc
            .atoms
            .iter()
            .filter(|a| a.element != "H")
            .map(|a| (a.element.clone(), a.purpose))
            .collect();
        atoms.sort_by(|a, b| a.0.cmp(&b.0));
        (
            atoms,
            doc.arrows.iter().map(|a| a.purpose).collect::<Vec<_>>(),
        )
    };
    let expected = purposes(&document);
    assert!(expected.0.iter().any(|(_, p)| *p == Purpose::Reference));

    let bytes = crate::formats::write(document, "cdxml").await.unwrap();
    let back = crate::formats::read(bytes, "cdxml").await.unwrap();
    assert_eq!(purposes(&back), expected);
}

#[tokio::test]
async fn unreadable_reshiki_data_opens_as_what_chemdraw_sees() {
    let xml = r#"<CDXML><page id="1">
<fragment id="2"><n id="3" p="10 10" ReShikiPurpose="Decoration"/><n id="4" p="24 10"/>
<b id="5" B="3" E="4"/></fragment>
<group id="6" ReShikiPanel="{not json">
<curve id="7" CurvePoints="0 40 0 40 60 40 60 40 60 40 60 80" Closed="yes"/>
<t id="8" p="5 50" InterpretChemically="no"><s>Title</s></t>
</group></page></CDXML>"#;
    let doc = crate::formats::read(xml.into(), "cdxml").await.unwrap();
    assert!(doc.atoms.iter().all(|a| a.purpose == Purpose::Chemistry));
    assert!(doc.graphics.iter().all(|g| g.panel.is_none()));
    assert_eq!(doc.annotations.len(), 1, "the title stays as text");
}
