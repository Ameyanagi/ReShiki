use reshiki::{
    document::{Document, Point},
    document_styles,
    engine::{LocalEngine, Request},
    export,
    style::{
        DrawingStyle,
        units::{self, Dimension, Unit},
    },
};

fn equivalent_styles() -> Vec<DrawingStyle> {
    [
        ["0.5 cm", "0.025 cm", "0.05 cm", "0.02 cm", "0.04 cm"],
        ["5 mm", "0.25 mm", "0.5 mm", "0.2 mm", "0.4 mm"],
        [
            "14.173228346456693 pt",
            "0.7086614173228346 pt",
            "1.4173228346456692 pt",
            "0.5669291338582677 pt",
            "1.1338582677165354 pt",
        ],
    ]
    .into_iter()
    .map(|inputs| {
        let mut style = DrawingStyle {
            name: "Unit check".into(),
            ..Default::default()
        };
        for (dimension, text) in Dimension::ALL.into_iter().zip(inputs) {
            dimension.set(&mut style, units::parse(text, Unit::Points).unwrap().points);
        }
        style.validate().unwrap();
        style
    })
    .collect()
}

fn bond() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(20., 30.));
    let b = doc.add_atom("C", Point::new(62., 30.));
    doc.add_bond(a, b, 1, "plain");
    doc
}

fn pdf_dimensions(bytes: &[u8]) -> (f64, f64) {
    let text = String::from_utf8_lossy(bytes);
    let numbers: Vec<f64> = text
        .split_once("/MediaBox")
        .unwrap()
        .1
        .split_once('[')
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(numbers.len(), 4);
    (numbers[2] - numbers[0], numbers[3] - numbers[1])
}

#[test]
fn equivalent_units_keep_native_styles_geometry_and_physical_figures() {
    let styles = equivalent_styles();
    assert!(styles.windows(2).all(|pair| pair[0] == pair[1]));
    let original = bond();
    let directory = tempfile::tempdir().unwrap();
    for scale in [false, true] {
        let mut reference = None;
        for style in &styles {
            let doc = document_styles::apply(&original, style.clone(), true, scale).unwrap();
            let distance = doc.atoms[0].position.distance(doc.atoms[1].position);
            let expected_pt = if scale { 5. * 72. / 25.4 } else { 14.4 };
            assert!(
                (f64::from(distance * doc.drawing_style.points_per_world()) - expected_pt).abs()
                    < 0.00001
            );
            let native_path = directory.path().join("units.rsk");
            std::fs::write(&native_path, doc.file_json().unwrap()).unwrap();
            let reopened = Document::from_json(&std::fs::read(native_path).unwrap()).unwrap();
            assert_eq!(reopened.drawing_style, *style);
            assert_eq!(reopened.atoms, doc.atoms);
            assert_eq!(reopened.bonds, doc.bonds);

            let style_path = directory.path().join("units.reshiki-style");
            document_styles::save(&style_path, style).unwrap();
            assert_eq!(document_styles::load(&style_path).unwrap(), *style);
            let json = std::fs::read_to_string(style_path).unwrap();
            assert!(!json.contains("drawing_style_unit"));
            assert!(!json.contains("display_unit"));

            let svg = export::drawing(&doc, "svg").unwrap();
            let text = std::str::from_utf8(&svg).unwrap();
            let tree = roxmltree::Document::parse(text).unwrap();
            let root = tree.root_element();
            let point_size = |name| {
                root.attribute(name)
                    .unwrap()
                    .trim_end_matches("pt")
                    .parse::<f64>()
                    .unwrap()
            };
            let width = point_size("width");
            let height = point_size("height");
            assert!((width - expected_pt - 8.).abs() < 0.001);
            let pdf = export::drawing(&doc, "pdf").unwrap();
            let (pdf_width, pdf_height) = pdf_dimensions(&pdf);
            assert!((pdf_width - width).abs() < 0.001);
            assert!((pdf_height - height).abs() < 0.001);
            if let Some((expected_doc, expected_svg)) = &reference {
                assert_eq!(&doc, expected_doc);
                assert_eq!(
                    &svg, expected_svg,
                    "Compare coordinates and viewBox as well as cropped size"
                );
            } else {
                reference = Some((doc, svg));
            }
        }
    }
}

#[tokio::test]
async fn equivalent_units_keep_cdx_cdxml_and_stationery_dimensions_and_graph() {
    let engine = LocalEngine::default();
    let directory = tempfile::tempdir().unwrap();
    for style in equivalent_styles() {
        let doc = document_styles::apply(&bond(), style.clone(), true, true).unwrap();
        for format in ["cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let output = engine.request(request).await.unwrap().output.unwrap();
            if format == "cdxml" {
                let xml = roxmltree::Document::parse(&output).unwrap();
                for (name, dimension) in [
                    ("BondLength", Dimension::Bond),
                    ("LineWidth", Dimension::Line),
                    ("BoldWidth", Dimension::Bold),
                    ("MarginWidth", Dimension::Margin),
                    ("HashSpacing", Dimension::Hash),
                ] {
                    let value = xml
                        .root_element()
                        .attribute(name)
                        .unwrap()
                        .parse::<f64>()
                        .unwrap();
                    assert!(
                        (value - f64::from(dimension.get(&style))).abs() < 0.00001,
                        "{name}"
                    );
                }
            }
            let result = engine
                .request(Request::import(format, &output))
                .await
                .unwrap();
            assert_eq!(result.analysis.unwrap().formula, "C2H6");
            let back = result.document.unwrap();
            assert_eq!(back.atoms.len(), 2);
            assert_eq!(back.bonds.len(), 1);
            assert_eq!(back.bonds[0].order, 1);
            assert!(
                back.atoms
                    .iter()
                    .all(|atom| atom.element == "C" && atom.charge == 0)
            );
            for dimension in Dimension::ALL {
                assert!(
                    (dimension.get(&back.drawing_style) - dimension.get(&style)).abs() < 0.0011,
                    "{format}: {dimension:?}"
                );
            }
            let old_delta = Point::new(
                doc.atoms[1].position.x - doc.atoms[0].position.x,
                doc.atoms[1].position.y - doc.atoms[0].position.y,
            );
            let new_delta = Point::new(
                back.atoms[1].position.x - back.atoms[0].position.x,
                back.atoms[1].position.y - back.atoms[0].position.y,
            );
            assert!(
                old_delta.distance(new_delta) < 0.001,
                "{format} changes relative geometry"
            );
        }
        let path = directory.path().join("units.cds");
        document_styles::save(&path, &style).unwrap();
        let back = document_styles::load(&path).unwrap();
        for dimension in Dimension::ALL {
            assert!((dimension.get(&back) - dimension.get(&style)).abs() < 0.0011);
        }
    }
}
