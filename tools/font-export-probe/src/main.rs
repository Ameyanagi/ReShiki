//! Hermetic backend reproduction for issue #103, independent of the app build.

use std::path::{Path, PathBuf};

const VARIABLE: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf");
const STATIC_100: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/static-100.subset.ttf");
const STATIC_400: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/static-400.subset.ttf");
const STATIC_700: &[u8] =
    include_bytes!("../../../tests/fixtures/font-export-103/static-700.subset.ttf");
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
            .chunks_exact(4)
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
        "case\trequested\texpected\tmatched_static_weight\twidth\theight\tdark_coverage_sum\n",
    );
    let mut failed = false;
    for (case, scale) in [("labels", 1.0_f32), ("reported-svg", 12.5)] {
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
            let svg = if case == "labels" {
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
                VARIABLE,
                &svg,
                scale,
                &destination.join(format!("{stem}-variable.png")),
            );
            let mut controls = Vec::new();
            for (font, control_weight) in [(STATIC_100, 100), (STATIC_400, 400), (STATIC_700, 700)]
            {
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
                        !controls[a].1.same_pixels(&controls[b].1),
                        "independent weights must render differently"
                    );
                }
            }
            let matched = controls
                .iter()
                .find(|(_, control)| variable.same_pixels(control))
                .map(|(weight, _)| *weight);
            let expected = if expectation == "bug" { 100 } else { weight };
            failed |= matched != Some(expected);
            report.push_str(&format!(
                "{case}\t{requested}\t{expected}\t{}\t{}\t{}\t{}\n",
                matched.map_or_else(|| "none".into(), |w| w.to_string()),
                variable.width,
                variable.height,
                variable.dark_coverage_sum()
            ));
        }
    }
    print!("{report}");
    std::fs::write(destination.join("report.tsv"), report).expect("write report");
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

#[cfg(test)]
mod tests {
    #[test]
    fn pinned_fixture_has_independent_static_controls_and_a_non_400_default() {
        super::validate_fixtures();
    }
}
