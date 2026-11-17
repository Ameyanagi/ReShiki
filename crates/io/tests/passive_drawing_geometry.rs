use reshiki_io::{engine::LocalEngine, export, formats};
use reshiki_model::{
    document::{Document, History, Point},
    transaction,
};

const NATIVE: &[u8] =
    include_bytes!("../../../tests/fixtures/passive-drawing-geometry/unsettled-native.rsk");

fn source() -> Document {
    let doc: Document = serde_json::from_slice(NATIVE).unwrap();
    doc.validate().unwrap();
    doc
}

fn assert_geometry(actual: &Document, expected: &Document) {
    assert_eq!(
        actual.atoms.iter().map(|a| a.position).collect::<Vec<_>>(),
        expected
            .atoms
            .iter()
            .map(|a| a.position)
            .collect::<Vec<_>>()
    );
    assert_eq!(actual.arrows, expected.arrows);
    assert_eq!(actual.annotations, expected.annotations);
    assert_eq!(actual.graphics, expected.graphics);
    assert_eq!(actual.arrow_labels, expected.arrow_labels);
    assert_eq!(actual.molecule_names, expected.molecule_names);
    assert_eq!(actual.property_captions, expected.property_captions);
}

#[tokio::test]
async fn passive_native_and_checked_export_snapshots_keep_authored_geometry() {
    let engine = LocalEngine::in_process();
    let source = source();
    let opened = formats::read_with_engine(NATIVE.to_vec(), "rsk", &engine)
        .await
        .unwrap();
    assert_geometry(&opened, &source);
    let checked = export::checked_document(&engine, opened.clone())
        .await
        .unwrap();
    assert_geometry(&checked, &source);
    let (figure, warning) = export::figure_document(&engine, opened).await.unwrap();
    assert!(warning.is_none(), "{warning:?}");
    assert_geometry(&figure, &source);
    let saved = formats::write_with_engine(checked, "rsk", &engine)
        .await
        .unwrap();
    let reopened = Document::from_native_file(&saved).unwrap();
    assert_geometry(&reopened, &source);
}

#[tokio::test]
async fn passive_linked_property_import_keeps_source_text_and_position_until_edit() {
    let engine = LocalEngine::in_process();
    for (format, bytes) in [
        ("cdxml", include_bytes!("../../../tests/fixtures/passive-drawing-geometry/remote-property-caption.cdxml").as_slice()),
        ("cdx", include_bytes!("../../../tests/fixtures/passive-drawing-geometry/remote-property-caption.drawing.cds").as_slice()),
    ] {
        let (mut doc, warnings) = formats::read_with_warnings(bytes.to_vec(), format, &engine).await.unwrap();
        assert!(warnings.is_empty(), "{format}: {warnings:?}");
        assert_eq!(doc.property_captions.len(), 1);
        let link = &doc.property_captions[0];
        let caption = doc.annotations.iter().find(|a| a.id == link.annotation).unwrap();
        assert_eq!(caption.text, "Formula: author positioned source text");
        assert_eq!(link.text, caption.text);
        assert!(caption.position.distance(Point::new(250. * (42. / 14.4), 300. * (42. / 14.4))) < 0.001, "{format}: {:?}", caption.position);
        let annotation = link.annotation;
        let atom = link.atoms[0];
        let before = doc.clone();
        doc.atom_mut(atom).unwrap().position.x -= 70.;
        doc.bonds[0].order = 2;
        transaction::apply(&mut doc, &mut History::default(), before, false).unwrap();
        let caption = doc.annotations.iter().find(|a| a.id == annotation).unwrap();
        assert_eq!(caption.text, "Formula: CH2O");
        assert_ne!(caption.position, Point::new(250. * (42. / 14.4), 300. * (42. / 14.4)));
        assert_eq!(doc.property_captions.len(), 1);
    }
}
