use crate::document::{Annotation, Document, Point};
use reshiki_model::typography::TextFormat;

/// A turned caption beside an upright one.
fn turned(angle: f32) -> Document {
    let mut doc = Document::default();
    for (id, x, angle) in [(1, 40., angle), (2, 200., 0.)] {
        doc.annotations.push(Annotation {
            id,
            position: Point::new(x, 60.),
            text: "hν, 25 °C".into(),
            format: TextFormat::default(),
            angle,
        });
    }
    doc
}

#[tokio::test]
async fn turned_captions_keep_their_turn_and_centre_through_cdxml_and_cdx() {
    let doc = turned(300.);
    // Where the turned text sits beside the upright caption: a file may
    // place the whole drawing elsewhere.
    let offset = |doc: &Document| {
        let (centre, upright) = (doc.annotations[0].centre(), doc.annotations[1].position);
        Point::new(centre.x - upright.x, centre.y - upright.y)
    };
    let expected = offset(&doc);
    for format in ["cdxml", "cdx"] {
        let bytes = crate::formats::write(doc.clone(), format).await.unwrap();
        if format == "cdxml" {
            let xml = String::from_utf8(bytes.clone()).unwrap();
            // ChemDraw's 16.16 fixed-point degrees.
            assert!(
                xml.contains(&format!("RotationAngle=\"{}\"", 300 * 65536)),
                "{xml}"
            );
        }
        let back = crate::formats::read(bytes, format).await.unwrap();
        let [a, b] = back.annotations.as_slice() else {
            panic!("two captions from {format}");
        };
        assert!((a.angle - 300.).abs() < 0.01, "{format}: {}", a.angle);
        assert_eq!(b.angle, 0.);
        assert!(
            offset(&back).distance(expected) < 0.5,
            "{format}: {:?} vs {expected:?}",
            offset(&back)
        );
    }
}

#[tokio::test]
async fn chemdraw_turned_captions_open_turned_about_their_box_centre() {
    // From ChemDraw: "LRP" read bottom to top, 272° clockwise, beside two
    // upright captions that show where ChemDraw's points land.
    let xml = r#"<CDXML BondLength="14.40"><page id="1">
<t id="2" p="287.59 86" BoundingBox="276.59 63.96 291.49 86.11" RotationAngle="17825792" InterpretChemically="no"><s font="3" size="10">LRP</s></t>
<t id="3" p="200 110" BoundingBox="200 100 210 112" InterpretChemically="no"><s font="3" size="10">A</s></t>
<t id="4" p="300 110" BoundingBox="300 100 310 112" InterpretChemically="no"><s font="3" size="10">B</s></t>
</page></CDXML>"#;
    let doc = crate::formats::read(xml.into(), "cdxml").await.unwrap();
    let [lrp, a, b] = doc.annotations.as_slice() else {
        panic!("three captions");
    };
    assert!((lrp.angle - 272.).abs() < 1e-3);
    assert_eq!((a.angle, b.angle), (0., 0.));
    let scale = (b.position.x - a.position.x) / 100.;
    let expected = a
        .position
        .offset((284.04 - 200.) * scale, (75.035 - 100.) * scale);
    assert!(
        lrp.centre().distance(expected) < 0.01,
        "{:?} vs {expected:?}",
        lrp.centre()
    );
}
