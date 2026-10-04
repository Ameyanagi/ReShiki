use resvg::{tiny_skia, usvg};

#[path = "font_export_103/pdf.rs"]
mod pdf;

const FAMILY: &str = "ReShiki Font Export Fixture";
const TTF: &[u8] = include_bytes!("fixtures/font-export-103/variable-default-100.subset.ttf");
const CFF2: &[u8] = include_bytes!("fixtures/font-export-103/cff2-variable-default-100.subset.otf");

fn tree(font: &[u8], weight: u16) -> usvg::Tree {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_font_data(font.to_vec());
    usvg::Tree::from_str(
        &format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"180\"><text x=\"20\" y=\"130\" font-family=\"{FAMILY}\" font-size=\"140\" font-weight=\"{weight}\">HNO</text></svg>"),
        &options,
    ).unwrap()
}

fn pixels(tree: &usvg::Tree) -> Vec<u8> {
    let mut image = tiny_skia::Pixmap::new(480, 180).unwrap();
    resvg::render(tree, tiny_skia::Transform::default(), &mut image.as_mut());
    image.take()
}

#[test]
fn variable_font_exports_use_requested_weight_instead_of_thin_default() {
    for (variable, regular, bold) in [
        (
            TTF,
            include_bytes!("fixtures/font-export-103/static-400.subset.ttf").as_slice(),
            include_bytes!("fixtures/font-export-103/static-700.subset.ttf").as_slice(),
        ),
        (
            CFF2,
            include_bytes!("fixtures/font-export-103/cff-static-400.subset.otf").as_slice(),
            include_bytes!("fixtures/font-export-103/cff-static-700.subset.otf").as_slice(),
        ),
    ] {
        for (control, weight) in [(regular, 400), (bold, 700)] {
            let actual_tree = tree(variable, weight);
            let control_tree = tree(control, weight);
            let actual = pixels(&actual_tree);
            let expected = pixels(&control_tree);
            let coverage: u64 = expected
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| u64::from(p[3]))
                .sum();
            assert!(coverage > 1_000);
            let difference: u64 = actual
                .iter()
                .zip(&expected)
                .map(|(&a, &b)| u64::from(a.abs_diff(b)))
                .sum();
            // The separate font probe verifies identical commands and independent
            // FontTools coordinates. Allow static-font rounding at antialiased edges.
            assert!((difference as f64 / coverage as f64) < 0.02);
            let mut glyphs = 0;
            for node in actual_tree.root().children() {
                if let usvg::Node::Text(text) = node {
                    for span in text.layouted() {
                        for glyph in &span.positioned_glyphs {
                            assert_eq!(glyph.variation_weight, Some(weight));
                            glyphs += 1;
                        }
                    }
                }
            }
            assert_eq!(glyphs, 3);
        }
    }
}

#[test]
fn mixed_variable_weights_keep_two_selectable_pdf_font_instances() {
    for font in [TTF, CFF2] {
        let mut options = usvg::Options::default();
        options.fontdb_mut().load_font_data(font.to_vec());
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"360\"><text x=\"20\" y=\"130\" font-family=\"{FAMILY}\" font-size=\"140\" font-weight=\"400\">HNO</text><text x=\"20\" y=\"310\" font-family=\"{FAMILY}\" font-size=\"140\" font-weight=\"700\">HNO</text></svg>"
        );
        let tree = usvg::Tree::from_str(&svg, &options).unwrap();
        let pdf = svg2pdf::to_pdf(
            &tree,
            svg2pdf::ConversionOptions::default(),
            svg2pdf::PageOptions { dpi: 96.0 },
        )
        .unwrap();
        let syntax = String::from_utf8_lossy(&pdf);
        assert_eq!(syntax.matches("/Subtype /Type0").count(), 2);
        assert_eq!(syntax.matches("/ToUnicode").count(), 2);
        assert_eq!(syntax.matches("/Subtype /CIDFontType2").count(), 2);
        assert_eq!(syntax.matches("/FontFile2").count(), 2);
        assert!(!syntax.contains("/FontFile3"));
        let controls = if font == TTF {
            [
                include_bytes!("fixtures/font-export-103/static-400.subset.ttf").as_slice(),
                include_bytes!("fixtures/font-export-103/static-700.subset.ttf").as_slice(),
            ]
        } else {
            [
                include_bytes!("fixtures/font-export-103/cff-static-400.subset.otf").as_slice(),
                include_bytes!("fixtures/font-export-103/cff-static-700.subset.otf").as_slice(),
            ]
        };
        pdf::assert_embedded_weights(&pdf, controls);
    }
}
