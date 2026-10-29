//! Regressions for standalone PDF page scaling versus Form XObject coordinates.
use reshiki_io::export;
use reshiki_model::{
    document::{Document, Point},
    models3d::{Display, ForceField, Look, Model3d, Placed, Style, set_display},
};

const GRADIENTS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80">
<defs>
  <linearGradient id="linear" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="40" y2="0">
    <stop stop-color="white"/><stop offset="1" stop-color="red"/>
  </linearGradient>
  <radialGradient id="radial" gradientUnits="userSpaceOnUse" cx="20" cy="20" r="20">
    <stop stop-color="white"/><stop offset="1" stop-color="blue"/>
  </radialGradient>
</defs>
<rect width="40" height="30" fill="url(#linear)"/>
<g transform="translate(50 5)"><g transform="scale(1.2 .8)">
  <circle cx="20" cy="20" r="20" fill="url(#radial)"/>
</g></g>
<g transform="translate(5 40)" opacity=".6">
  <rect width="40" height="30" fill="url(#linear)"/>
</g>
<g transform="translate(65 40)" opacity=".7"><g transform="scale(.8)">
  <circle cx="20" cy="20" r="20" fill="url(#radial)"/>
</g></g>
</svg>"##;

fn options() -> svg2pdf::ConversionOptions {
    svg2pdf::ConversionOptions {
        compress: false,
        ..Default::default()
    }
}

fn tree() -> resvg::usvg::Tree {
    resvg::usvg::Tree::from_str(GRADIENTS, &Default::default()).unwrap()
}

// Follow the writer's xref offsets. Compressed profiles and streams must not
// accidentally be interpreted as object dictionaries.
fn objects(pdf: &[u8]) -> Vec<&[u8]> {
    let marker = b"startxref\n";
    let start = pdf
        .windows(marker.len())
        .rposition(|b| b == marker)
        .unwrap();
    let xref: usize = std::str::from_utf8(&pdf[start + marker.len()..])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut lines = std::str::from_utf8(&pdf[xref..]).unwrap().lines();
    assert_eq!(lines.next(), Some("xref"));
    let mut section = lines.next().unwrap().split_whitespace();
    assert_eq!(section.next(), Some("0"));
    let count: usize = section.next().unwrap().parse().unwrap();
    let mut offsets: Vec<usize> = lines
        .take(count)
        .filter_map(|line| {
            let mut entry = line.split_whitespace();
            let offset = entry.next().unwrap().parse().unwrap();
            entry.next();
            (entry.next() == Some("n")).then_some(offset)
        })
        .collect();
    offsets.sort_unstable();
    offsets.push(xref);
    offsets.windows(2).map(|p| &pdf[p[0]..p[1]]).collect()
}

fn dictionary(object: &[u8]) -> &str {
    let end = object
        .windows(7)
        .position(|b| b == b"stream\n")
        .unwrap_or(object.len());
    std::str::from_utf8(&object[..end]).unwrap()
}

fn array(dict: &str, key: &str) -> Vec<f32> {
    dict.split_once(key)
        .unwrap()
        .1
        .split_once('[')
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

fn patterns(pdf: &[u8]) -> Vec<Vec<f32>> {
    objects(pdf)
        .into_iter()
        .map(dictionary)
        .filter(|dict| dict.contains("/PatternType 2"))
        .map(|dict| array(dict, "/Matrix"))
        .collect()
}

fn close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 0.0001, "{actual:?} != {expected:?}");
    }
}

#[test]
fn page_dpi_scales_gradient_coordinates_once_and_preserves_isolated_groups() {
    for dpi in [72., 96., 144.] {
        let scale = 72. / dpi;
        let pdf = svg2pdf::to_pdf(&tree(), options(), svg2pdf::PageOptions { dpi }).unwrap();
        let matrices = patterns(&pdf);
        assert_eq!(matrices.len(), 4);
        close(&matrices[0], &[scale, 0., 0., -scale, 0., 80. * scale]);
        close(
            &matrices[1],
            &[1.2 * scale, 0., 0., -0.8 * scale, 50. * scale, 75. * scale],
        );
        // Transparency groups use local coordinates: the invoking content
        // already applies the page transform, so scaling these doubles it.
        close(&matrices[2], &[1., 0., 0., 1., 5., 40.]);
        close(&matrices[3], &[0.8, 0., 0., 0.8, 65., 40.]);
        let page = objects(&pdf)
            .into_iter()
            .map(dictionary)
            .find(|dict| dict.contains("/MediaBox"))
            .unwrap();
        close(
            &array(page, "/MediaBox"),
            &[0., 0., 120. * scale, 80. * scale],
        );
        let content = objects(&pdf)
            .into_iter()
            .find(|object| {
                object.windows(7).any(|b| b == b"stream\n")
                    && String::from_utf8_lossy(object).contains("80 cm")
            })
            .unwrap();
        let text = String::from_utf8_lossy(content);
        let prefix = format!("stream\nq\n{scale} 0 0 {scale} 0 0 cm\nq\n1 0 0 -1 0 80 cm\n");
        assert!(
            text.contains(&prefix),
            "Page content was scaled more than once: {text}"
        );
    }
}

#[test]
fn embedded_form_keeps_unscaled_gradients_and_unit_sized_placement() {
    let (chunk, _) = svg2pdf::to_chunk(&tree(), options()).unwrap();
    let mut pdf = pdf_writer::Pdf::new();
    pdf.extend(&chunk);
    let pdf = pdf.finish();
    let standalone =
        svg2pdf::to_pdf(&tree(), options(), svg2pdf::PageOptions { dpi: 72. }).unwrap();
    assert_eq!(patterns(&pdf), patterns(&standalone));
    let root = objects(&pdf)
        .into_iter()
        .map(dictionary)
        .find(|dict| dict.contains("/Subtype /Form") && dict.contains("/Matrix"))
        .unwrap();
    close(&array(root, "/BBox"), &[0., 0., 120., 80.]);
    close(
        &array(root, "/Matrix"),
        &[1. / 120., 0., 0., 1. / 80., 0., 0.],
    );
}

#[test]
fn ordinary_2d_pdf_keeps_its_physical_size() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let svg = export::drawing(&doc, "svg").unwrap();
    let tree = export::parse_svg(String::from_utf8(svg).unwrap()).unwrap();
    let pdf = export::drawing(&doc, "pdf").unwrap();
    let page = objects(&pdf)
        .into_iter()
        .map(dictionary)
        .find(|dict| dict.contains("/MediaBox"))
        .unwrap();
    let bounds = array(page, "/MediaBox");
    close(
        &bounds,
        &[
            0.,
            0.,
            tree.size().width() * 0.75,
            tree.size().height() * 0.75,
        ],
    );
    assert!((bounds[2] - 22.4).abs() < 0.001);
    assert!(patterns(&pdf).is_empty());
}

fn ethanol() -> Document {
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", Point::new(0., 0.));
    let c2 = doc.add_atom("C", Point::new(36., -21.));
    let o = doc.add_atom("O", Point::new(72., 0.));
    doc.add_bond(c1, c2, 1, "plain");
    doc.add_bond(c2, o, 1, "plain");
    let atoms = [
        (c1, [-1.2, -0.2, 0.1]),
        (c2, [0.1, 0.5, -0.1]),
        (o, [1.2, -0.3, 0.2]),
    ]
    .map(|(id, xyz)| Placed { id, xyz })
    .to_vec();
    doc.models
        .push(Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s));
    reshiki_model::models3d::require_version(&mut doc);
    assert!(set_display(&mut doc, 0, Display::ThreeD));
    doc
}

#[test]
#[ignore = "writes actual renderer/PDF comparison artifacts to RESHIKI_PDF_QA_DIR"]
fn export_pdf_gradient_evidence() {
    let root = std::path::PathBuf::from(std::env::var_os("RESHIKI_PDF_QA_DIR").unwrap());
    std::fs::create_dir_all(&root).unwrap();
    let flat = ethanol();
    let mut two_d = flat.clone();
    assert!(set_display(&mut two_d, 0, Display::TwoD));
    let mut glossy = flat.clone();
    glossy.models[0].style = Style::SpaceFilling;
    glossy.models[0].look = Look::Glossy;
    let (mut crystal, warnings) = reshiki_io::crystal::read(
        "cif",
        include_str!("../../../tests/fixtures/crystal/nacl.cif"),
    )
    .unwrap();
    assert!(warnings.is_empty());
    assert!(set_display(&mut crystal, 0, Display::ThreeD));
    crystal.models[0].repeat = [2, 1, 1];
    crystal.models[0].charges = true;
    for (name, doc) in [
        ("ethanol-flat", flat),
        ("ethanol-glossy", glossy),
        ("nacl", crystal),
        ("ethanol-2d", two_d),
    ] {
        doc.validate().unwrap();
        std::fs::write(root.join(format!("{name}.rsk")), doc.file_json().unwrap()).unwrap();
        let svg = export::drawing(&doc, "svg").unwrap();
        let tree = export::parse_svg(String::from_utf8(svg.clone()).unwrap()).unwrap();
        std::fs::write(root.join(format!("{name}.svg")), svg).unwrap();
        std::fs::write(
            root.join(format!("{name}.pdf")),
            export::drawing(&doc, "pdf").unwrap(),
        )
        .unwrap();
        let mut pixels = resvg::tiny_skia::Pixmap::new(
            (tree.size().width() * 2.).ceil() as u32,
            (tree.size().height() * 2.).ceil() as u32,
        )
        .unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(2., 2.),
            &mut pixels.as_mut(),
        );
        pixels
            .save_png(root.join(format!("{name}-svg.png")))
            .unwrap();
        let mut paged = doc;
        paged.page_layout = Some(Default::default());
        std::fs::write(
            root.join(format!("{name}-pages.pdf")),
            export::pages_pdf(&paged).unwrap(),
        )
        .unwrap();
    }
    std::fs::write(root.join("transforms.svg"), GRADIENTS).unwrap();
    let mut pixels = resvg::tiny_skia::Pixmap::new(240, 160).unwrap();
    resvg::render(
        &tree(),
        resvg::tiny_skia::Transform::from_scale(2., 2.),
        &mut pixels.as_mut(),
    );
    pixels.save_png(root.join("transforms-svg.png")).unwrap();
    for dpi in [72., 96., 144.] {
        std::fs::write(
            root.join(format!("transforms-{dpi}.pdf")),
            svg2pdf::to_pdf(&tree(), options(), svg2pdf::PageOptions { dpi }).unwrap(),
        )
        .unwrap();
    }
}
