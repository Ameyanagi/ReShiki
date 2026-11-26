use super::*;
use crate::{
    document::Annotation,
    graphics::{Graphic, MoDiagram, MoPicture},
    models3d::surface::{Figure, OrbitalRef},
    orbital_levels::Column,
};
use reshiki_qcdata::Spin;

fn diagram() -> Document {
    let mut doc = Document::default();
    let molecule = doc.add_atom("C", Point::default());
    let result = "0123456789abcdef".to_owned();
    let mut mo = MoDiagram::new(
        molecule,
        result.clone(),
        vec![Column {
            spin: Spin::Alpha,
            label: String::new(),
            energies_ev: vec![-15., -10.95, -6.02, -1.],
            occupations: vec![2., 2., 0., 0.],
            occupied: Some(2),
        }],
    );
    mo.pictures.push(MoPicture {
        column: 0,
        figure: Figure {
            orbital: OrbitalRef {
                result,
                spin: Spin::Alpha,
                index: 1,
            },
            iso: 0.05,
            look: Default::default(),
            snapshot: None,
        },
    });
    let id = doc.next_id();
    doc.graphics.push(Graphic::mo_diagram(
        id,
        Point::new(40., 20.),
        Point::new(360., 300.),
        mo,
        Default::default(),
    ));
    doc
}

fn scaled_diagram(font_size_pt: f32, scale: f32, angle: f32) -> Document {
    let mut doc = diagram();
    doc.drawing_style.font_size_pt = font_size_pt;
    let g = &mut doc.graphics[0];
    let (width, height) = g.mo.as_ref().unwrap().natural_size(&doc.drawing_style);
    g.axis_x = Point::new(width * scale, 0.).turned(Point::default(), angle);
    g.axis_y = Point::new(0., height * scale).turned(Point::default(), angle);
    doc
}

fn left_midpoint(a: &Annotation) -> Point {
    let (_, height) = a.size();
    a.position
        .offset(0., height / 2.)
        .turned(a.centre(), a.angle)
}

#[test]
fn mo_diagram_exports_grouped_lines_and_text_through_cdxml_and_cdx() {
    let doc = diagram();
    doc.validate().unwrap();
    let expected: Vec<_> = doc.graphics[0]
        .mo_parts(&doc.drawing_style)
        .1
        .into_iter()
        .map(|a| a.text)
        .collect();
    assert!(expected.iter().any(|text| text.contains("HOMO")));
    assert!(expected.iter().any(|text| text.contains("gap 4.93 eV")));
    // No caller-supplied vector geometry is needed for the diagram.
    let xml = write(&doc, Options::default()).unwrap();
    let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &decoded] {
        let tree = roxmltree::Document::parse(xml).unwrap();
        let groups: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("group"))
            .collect();
        assert_eq!(groups.len(), 1);
        let group = groups[0];
        assert!(group.children().any(|n| n.has_tag_name("curve")));
        let labels: Vec<_> = group
            .children()
            .filter(|n| n.has_tag_name("t"))
            .map(|n| {
                assert_eq!(n.attribute("InterpretChemically"), Some("no"));
                n.descendants()
                    .filter(|n| n.has_tag_name("s"))
                    .filter_map(|n| n.text())
                    .collect::<String>()
            })
            .collect();
        assert_eq!(labels, expected);
        assert!(!tree.descendants().any(|n| n.has_tag_name("embeddedobject")));
    }
}

#[test]
fn resized_mo_diagram_labels_roundtrip_through_the_strict_cdxml_and_cdx_reader() {
    // A default diagram at 40% and a natural-size 4 pt diagram both have
    // 3.28 pt tick labels. An enlarged, turned diagram also exercises the
    // upper limit and the new box placement about its row's anchor.
    for (font, scale, angle) in [(10., 0.4, 0.), (4., 1., 0.), (10., 20., 37.)] {
        let doc = scaled_diagram(font, scale, angle);
        doc.validate().unwrap();
        let expected = doc.graphics[0].mo_parts(&doc.drawing_style).1;
        assert!(
            expected
                .iter()
                .any(|a| !(4. ..=144.).contains(&a.format.style.size_pt))
        );
        let xml = write(&doc, Options::default()).unwrap();
        let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
        for (format, xml) in [("cdxml", &xml), ("cdx", &decoded)] {
            // This entry point does not apply the app importer's text-size
            // approximation, so an unsupported exported size fails here.
            let back = crate::chemistry::cdxml::import_cdxml(xml)
                .unwrap_or_else(|error| panic!("{format}, {font} pt at {scale}: {error}"))
                .document;
            back.validate().unwrap();
            assert_eq!(back.annotations.len(), expected.len());
            let atom = back.atoms[0].position;
            for (source, actual) in expected.iter().zip(&back.annotations) {
                assert_eq!(actual.text, source.text, "{format}");
                assert!(
                    (actual.format.style.size_pt - source.format.style.size_pt.clamp(4., 144.))
                        .abs()
                        < 0.026,
                    "{format}: {}",
                    source.text
                );
                let actual = left_midpoint(actual).offset(-atom.x, -atom.y);
                let expected = left_midpoint(source)
                    .offset(-doc.atoms[0].position.x, -doc.atoms[0].position.y);
                assert!(
                    actual.distance(expected) < 0.1,
                    "{format}: {} moved from {expected:?} to {actual:?}",
                    source.text
                );
            }
        }
        // Only the export approximation changes; the source diagram still
        // uses its original generated font sizes.
        assert_eq!(doc.graphics[0].mo_parts(&doc.drawing_style).1, expected);
    }
}

#[test]
fn mo_diagram_size_approximation_notice_reaches_editable_files_and_clipboard() {
    use crate::engine::export_notices::{Route, notices};
    let doc = scaled_diagram(10., 0.4, 0.);
    let notice = mo_diagram_export_notice(&doc).unwrap();
    assert!(notice.contains(MO_DIAGRAM_TEXT_SIZE_NOTICE));
    for format in ["cdxml", "cdx"] {
        let warnings = notices(
            &doc,
            format,
            Route::Prepared {
                source: &doc,
                figure_only: false,
            },
        );
        assert!(warnings.contains(&notice), "{format}");
    }
    let (_, warnings) = write_clipboard(&doc).unwrap();
    assert!(warnings.contains(&notice));
}

#[test]
fn mo_diagram_loss_notice_reaches_editable_files_and_clipboard() {
    use crate::engine::export_notices::{Route, notices};
    let doc = scaled_diagram(10., 1., 0.);
    for format in ["cdxml", "cdx", "mol"] {
        let warnings = notices(
            &doc,
            format,
            Route::Prepared {
                source: &doc,
                figure_only: false,
            },
        );
        assert_eq!(
            warnings.iter().any(|w| w == MO_DIAGRAM_EXPORT_NOTICE),
            format != "mol"
        );
    }
    let (_, warnings) = write_clipboard(&doc).unwrap();
    assert!(warnings.iter().any(|w| w == MO_DIAGRAM_EXPORT_NOTICE));
    assert!(mo_diagram_export_notice(&Document::default()).is_none());
}
