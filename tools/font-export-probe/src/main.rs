//! Hermetic backend reproduction for issue #103, independent of the app build.

use std::path::{Path, PathBuf};

#[path = "../../../crates/model/src/style/font.rs"]
mod style_font;

const VARIABLE: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf");
const STATIC_100: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/static-100.subset.ttf");
const STATIC_400: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/static-400.subset.ttf");
const STATIC_700: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/static-700.subset.ttf");
const CFF2_VARIABLE: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/cff2-variable-default-100.subset.otf");
const CFF_100: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/cff-static-100.subset.otf");
const CFF_400: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/cff-static-400.subset.otf");
const CFF_700: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/cff-static-700.subset.otf");
const ISSUE: &str = include_str!("../../../tests/fixtures/font-export-103/issue.svg");
const FAMILY: &str = "ReShiki Font Export Fixture";

struct Rendered {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl Rendered {
    fn dark_coverage_sum(&self) -> u64 {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                // Pixels are premultiplied. Exclude white backgrounds from the
                // coverage measurement while retaining antialiased dark edges.
                u64::from(p[3].saturating_sub(p[0].min(p[1]).min(p[2])))
            })
            .sum()
    }

    fn same_pixels(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.pixels == other.pixels
    }

    fn difference_ratio(&self, other: &Self) -> f64 {
        if self.width != other.width || self.height != other.height {
            return f64::INFINITY;
        }
        self.pixels
            .iter()
            .zip(&other.pixels)
            .map(|(&a, &b)| u64::from(a.abs_diff(b)))
            .sum::<u64>() as f64
            / other.dark_coverage_sum().max(1) as f64
    }

    fn matches_control(&self, other: &Self) -> bool {
        // validate_instance also requires matching commands, advances and points
        // within 0.5 font unit. This allows only their antialiasing difference.
        self.difference_ratio(other) <= 0.02
    }
}

macro_rules! render_with {
    ($backend:ident, $font:expr, $svg:expr, $scale:expr, $path:expr) => {{
        use $backend::{tiny_skia, usvg};
        let mut options = usvg::Options::default();
        // Never enumerate or load host fonts. Each render gets just one face.
        options.fontdb_mut().load_font_data($font.to_vec());
        assert_eq!(options.fontdb.faces().count(), 1);
        assert!(
            options
                .fontdb
                .faces()
                .next()
                .unwrap()
                .families
                .iter()
                .any(|(name, _)| name == FAMILY)
        );
        let tree = usvg::Tree::from_str($svg, &options).expect("parse fixture SVG");
        let size = tree.size();
        let width = (size.width() * $scale).ceil() as u32;
        let height = (size.height() * $scale).ceil() as u32;
        let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("allocate tiny probe image");
        $backend::render(
            &tree,
            tiny_skia::Transform::from_scale($scale, $scale),
            &mut pixmap.as_mut(),
        );
        pixmap.save_png($path).expect("save probe image");
        Rendered {
            pixels: pixmap.data().to_vec(),
            width,
            height,
        }
    }};
}

fn render(backend: &str, font: &[u8], svg: &str, scale: f32, path: &Path) -> Rendered {
    match backend {
        "current" => render_with!(resvg, font, svg, scale, path),
        #[cfg(feature = "oracle")]
        "oracle" => render_with!(resvg_oracle, font, svg, scale, path),
        _ => panic!("renderer must be current, or oracle with --features oracle"),
    }
}

fn validate_fixtures() {
    let variable = ttf_parser::Face::parse(VARIABLE, 0).unwrap();
    let axis = variable.variation_axes().into_iter().next().unwrap();
    assert_eq!(variable.variation_axes().len(), 1);
    assert_eq!(axis.tag, ttf_parser::Tag::from_bytes(b"wght"));
    assert_eq!(
        (axis.min_value, axis.def_value, axis.max_value),
        (100., 100., 900.)
    );
    assert!(variable.is_variable());
    for (bytes, weight) in [(STATIC_100, 100), (STATIC_400, 400), (STATIC_700, 700)] {
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        assert!(!face.is_variable());
        assert_eq!(face.weight().to_number(), weight);
        for c in "HNO".chars() {
            assert!(face.glyph_index(c).is_some());
        }
    }
    for (variable, controls) in [
        (VARIABLE, [STATIC_100, STATIC_400, STATIC_700]),
        (CFF2_VARIABLE, [CFF_100, CFF_400, CFF_700]),
    ] {
        for (bytes, weight) in controls.into_iter().zip([100, 400, 700]) {
            validate_instance(variable, bytes, weight);
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3
        || !["current", "oracle"].contains(&args[0].as_str())
        || !["bug", "fixed"].contains(&args[1].as_str())
    {
        eprintln!("Usage: reshiki-font-export-probe <current|oracle> <bug|fixed> <output-dir>");
        eprintln!("'fixed' is the acceptance check; 'bug' verifies the known Thin-weight failure.");
        std::process::exit(2);
    }
    validate_fixtures();
    let backend = &args[0];
    let expectation = &args[1];
    let destination = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&destination).expect("create output directory");
    let mut report = String::from(
        "case\trequested\texpected\tmatched_static_weight\twidth\theight\tdark_coverage_sum\tdifference_ratio\n",
    );
    let mut failed = false;
    for (case, scale, variable_bytes, static_bytes) in [
        (
            "labels",
            1.0_f32,
            VARIABLE,
            [STATIC_100, STATIC_400, STATIC_700],
        ),
        (
            "reported-svg",
            12.5,
            VARIABLE,
            [STATIC_100, STATIC_400, STATIC_700],
        ),
        (
            "cff2-labels",
            1.0_f32,
            CFF2_VARIABLE,
            [CFF_100, CFF_400, CFF_700],
        ),
        (
            "cff2-reported-svg",
            12.5,
            CFF2_VARIABLE,
            [CFF_100, CFF_400, CFF_700],
        ),
    ] {
        for (requested, weight) in [
            ("unset", 400),
            ("normal", 400),
            ("400", 400),
            ("bold", 700),
            ("700", 700),
        ] {
            let attribute = if requested == "unset" {
                String::new()
            } else {
                format!("font-weight=\"{requested}\"")
            };
            let svg = if case.ends_with("labels") {
                format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"180\"><text x=\"20\" y=\"130\" font-size=\"140\" font-family=\"{FAMILY}\" {attribute}>HNO</text></svg>"
                )
            } else {
                // Preserve the supplied geometry and physical size; substitute only
                // the fixture family and weight. 12.5 = ReShiki file PNG's 1200/96 dpi.
                ISSUE
                    .replace("Noto Sans CJK JP", FAMILY)
                    .replace("font-weight=\"normal\"", &attribute)
            };
            let stem = format!("{case}-{requested}");
            let variable = render(
                backend,
                variable_bytes,
                &svg,
                scale,
                &destination.join(format!("{stem}-variable.png")),
            );
            let mut controls = Vec::new();
            for (font, control_weight) in static_bytes.into_iter().zip([100, 400, 700]) {
                let control = render(
                    backend,
                    font,
                    &svg,
                    scale,
                    &destination.join(format!("{stem}-static-{control_weight}.png")),
                );
                assert!(
                    control.dark_coverage_sum() > 1_000,
                    "control must contain visible ink"
                );
                controls.push((control_weight, control));
            }
            for a in 0..controls.len() {
                for b in a + 1..controls.len() {
                    assert!(
                        !controls[a].1.matches_control(&controls[b].1),
                        "independent weights must render differently"
                    );
                }
            }
            let matched = controls
                .iter()
                .find(|(_, control)| {
                    if expectation == "bug" {
                        variable.same_pixels(control)
                    } else {
                        variable.matches_control(control)
                    }
                })
                .map(|(weight, _)| *weight);
            let expected = if expectation == "bug" { 100 } else { weight };
            failed |= matched != Some(expected);
            report.push_str(&format!(
                "{case}\t{requested}\t{expected}\t{}\t{}\t{}\t{}\t{:.6}\n",
                matched.map_or_else(|| "none".into(), |w| w.to_string()),
                variable.width,
                variable.height,
                variable.dark_coverage_sum(),
                variable
                    .difference_ratio(&controls.iter().find(|(w, _)| *w == expected).unwrap().1)
            ));
        }
    }
    print!("{report}");
    std::fs::write(destination.join("report.tsv"), report).expect("write report");
    if backend == "current" && expectation == "fixed" {
        write_pdf_suite(&destination);
    }
    if failed {
        eprintln!(
            "FAIL: {backend} does not satisfy the '{expectation}' expectation; see report.tsv."
        );
        std::process::exit(1);
    }
    eprintln!(
        "PASS: {backend} satisfies the '{expectation}' expectation. This is a backend check, not native Fedora verification."
    );
}

mod outlines {
    #[derive(Default)]
    pub(super) struct Outline(pub(super) Vec<(u8, Vec<f32>)>);

    impl ttf_parser::OutlineBuilder for Outline {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.push((0, vec![x, y]));
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.push((1, vec![x, y]));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.0.push((2, vec![x1, y1, x, y]));
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.0.push((3, vec![x1, y1, x2, y2, x, y]));
        }
        // CFF2 contours close implicitly; CFF1/TrueType report an explicit close.
        // Moves and segments still require identical contour topology.
        fn close(&mut self) {}
    }
}

fn validate_instance(variable_bytes: &[u8], bytes: &[u8], weight: u16) {
    use outlines::Outline;
    let variable = style_font::face(variable_bytes, 0, weight).unwrap();
    let reference = ttf_parser::Face::parse(bytes, 0).unwrap();
    for c in "HNO".chars() {
        let vg = variable.glyph_index(c).unwrap();
        let sg = reference.glyph_index(c).unwrap();
        assert_eq!(
            variable.glyph_hor_advance(vg),
            reference.glyph_hor_advance(sg),
            "advance {c} {weight}"
        );
        let mut actual = Outline::default();
        let mut expected = Outline::default();
        variable.outline_glyph(vg, &mut actual).unwrap();
        let tolerance = if variable_bytes == CFF2_VARIABLE {
            // FontTools static CFF instancing rounds relative deltas separately.
            // Its independent unrounded glyph evaluator is the stronger oracle.
            for line in
                include_str!("../../../tests/fixtures/font-export-103/cff2-outline-controls.tsv")
                    .lines()
            {
                let mut fields = line.split_whitespace();
                let row_weight: u16 = fields.next().unwrap().parse().unwrap();
                let character = fields.next().unwrap().chars().next().unwrap();
                let kind = fields.next().unwrap().parse().unwrap();
                if row_weight == weight && character == c {
                    expected
                        .0
                        .push((kind, fields.map(|v| v.parse().unwrap()).collect()));
                }
            }
            0.002
        } else {
            reference.outline_glyph(sg, &mut expected).unwrap();
            0.5
        };
        assert_eq!(
            actual.0.len(),
            expected.0.len(),
            "{c} {weight}: {:?} vs {:?}",
            actual.0,
            expected.0
        );
        for ((a_kind, a_coords), (e_kind, e_coords)) in actual.0.iter().zip(&expected.0) {
            assert_eq!(a_kind, e_kind);
            assert_eq!(a_coords.len(), e_coords.len());
            for (&a, &e) in a_coords.iter().zip(e_coords) {
                assert!(
                    (a - e).abs() <= tolerance,
                    "outline {c} {weight}: {a} vs {e}"
                );
            }
        }
    }
}

fn pdf_tree(fonts: &[&[u8]], svg: &str) -> resvg::usvg::Tree {
    let mut options = resvg::usvg::Options::default();
    for font in fonts {
        options.fontdb_mut().load_font_data(font.to_vec());
    }
    assert_eq!(options.fontdb.faces().count(), fonts.len());
    resvg::usvg::Tree::from_str(svg, &options).unwrap()
}

fn pdf_bytes(tree: &resvg::usvg::Tree) -> Vec<u8> {
    svg2pdf::to_pdf(
        tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions { dpi: 96.0 },
    )
    .unwrap()
}

fn mixed_svg() -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"360\"><text x=\"20\" y=\"130\" font-size=\"140\" font-family=\"{FAMILY}\" font-weight=\"400\">HNO</text><text x=\"20\" y=\"310\" font-size=\"140\" font-family=\"{FAMILY}\" font-weight=\"700\">HNO</text></svg>"
    )
}

fn write_pdf_suite(destination: &Path) {
    for (kind, variable, regular, bold) in [
        ("ttf", VARIABLE, STATIC_400, STATIC_700),
        ("cff2", CFF2_VARIABLE, CFF_400, CFF_700),
    ] {
        for (weight, control) in [(400, regular), (700, bold)] {
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"180\"><text x=\"20\" y=\"130\" font-size=\"140\" font-family=\"{FAMILY}\" font-weight=\"{weight}\">HNO</text></svg>"
            );
            for (name, font) in [("variable", variable), ("static", control)] {
                let tree = pdf_tree(&[font], &svg);
                std::fs::write(
                    destination.join(format!("{kind}-{weight}-{name}.pdf")),
                    pdf_bytes(&tree),
                )
                .unwrap();
            }
        }
        for (name, fonts) in [
            ("variable", vec![variable]),
            ("static", vec![regular, bold]),
        ] {
            let tree = pdf_tree(&fonts, &mixed_svg());
            std::fs::write(
                destination.join(format!("{kind}-mixed-{name}.pdf")),
                pdf_bytes(&tree),
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pinned_fixture_has_independent_static_controls_and_a_non_400_default() {
        super::validate_fixtures();
    }

    #[test]
    fn mixed_variable_weights_keep_distinct_embedded_text_resources() {
        use super::*;
        for font in [VARIABLE, CFF2_VARIABLE] {
            let bytes = pdf_bytes(&pdf_tree(&[font], &mixed_svg()));
            let syntax = String::from_utf8_lossy(&bytes);
            assert_eq!(syntax.matches("/Subtype /Type0").count(), 2);
            assert_eq!(syntax.matches("/ToUnicode").count(), 2);
            assert_eq!(syntax.matches("/FontFile2").count(), 2);
            assert_eq!(syntax.matches("/Subtype /CIDFontType2").count(), 2);
            assert!(!syntax.contains("/FontFile3"));
        }
    }
}
