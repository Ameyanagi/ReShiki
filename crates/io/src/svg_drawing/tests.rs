use super::*;
use std::io::Write;
fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96">{body}</svg>"#)
}
fn read_svg(body: &str) -> Imported {
    read(svg(body).as_bytes(), DrawingStyle::default()).unwrap()
}
#[test]
fn normal_paths_are_editable_grouped_and_survive_native_roundtrip() {
    let imported = read_svg(
        r##"<path d="M0 0L10 0L10 10Z" fill="#f00"/><path d="M20 0L30 10" fill="none" stroke="#00f" stroke-width="2"/>"##,
    );
    let doc = imported.drawing;
    assert_eq!(doc.graphics.len(), 2);
    assert_eq!(doc.groups.len(), 1);
    assert!(
        doc.graphics
            .iter()
            .all(|g| g.kind == GraphicKind::Path && !g.path.is_empty())
    );
    assert_eq!(doc.graphics[0].style.fill, Some(Color::Custom([255, 0, 0])));
    assert_eq!(doc.graphics[1].style.fill, Some(Color::Custom([0, 0, 255])));
    assert_eq!(
        Document::from_native_file(&serde_json::to_vec(&doc).unwrap()).unwrap(),
        doc
    );
}
#[test]
fn physical_units_viewbox_transforms_css_and_use_are_resolved() {
    let text = r##"<svg xmlns="http://www.w3.org/2000/svg" width="1in" height="1in" viewBox="0 0 96 96"><style>.piece {fill:#123456}</style><defs><path id="p" d="M0 0H24V12H0Z"/></defs><g transform="translate(12 24) scale(2)"><use href="#p" class="piece"/></g></svg>"##;
    let doc = read(text.as_bytes(), DrawingStyle::default())
        .unwrap()
        .drawing;
    assert_eq!(doc.graphics.len(), 1);
    assert_eq!(
        doc.graphics[0].style.fill,
        Some(Color::Custom([0x12, 0x34, 0x56]))
    );
    let (lo, hi) = doc.graphics[0].shape_bounds();
    let k = doc.drawing_style.world(0.75);
    let pad = doc.graphics[0].style.width() * 0.5;
    assert!((lo.x + pad - 12. * k).abs() < 0.001);
    assert!((lo.y + pad - 24. * k).abs() < 0.001);
    assert!((hi.x - pad - 60. * k).abs() < 0.001);
    assert!((hi.y - pad - 48. * k).abs() < 0.001);
}
#[test]
fn gradients_are_midpoint_colors_and_unsupported_content_is_counted() {
    let imported = read_svg(
        r##"<defs><linearGradient id="g"><stop stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient><clipPath id="c"><rect width="1" height="1"/></clipPath><filter id="f"/></defs><script>bad()</script><text>hello</text><image href="file:///does-not-exist"/><foreignObject/><animate/><path d="M0 0H10V10H0Z" fill="url(#g)" clip-path="url(#c)" filter="url(#f)"/>"##,
    );
    assert_eq!(imported.drawing.graphics.len(), 1);
    assert_eq!(
        imported.drawing.graphics[0].style.fill,
        Some(Color::Custom([128, 0, 128]))
    );
    for expected in [
        "1 scripts",
        "1 text elements",
        "1 images",
        "1 clip paths",
        "1 filters",
        "1 animations",
        "1 foreign objects",
        "middle color",
        "Outline text",
    ] {
        assert!(
            imported.notice.contains(expected),
            "{}: {}",
            expected,
            imported.notice
        );
    }
}
#[test]
fn svgz_is_capped_after_inflation() {
    let mut zip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    zip.write_all(svg("<path d='M0 0H10V10Z'/>").as_bytes())
        .unwrap();
    let bytes = zip.finish().unwrap();
    assert_eq!(
        read(&bytes, DrawingStyle::default())
            .unwrap()
            .drawing
            .graphics
            .len(),
        1
    );
    let mut zip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    zip.write_all(&vec![b' '; MAX_BYTES + 1]).unwrap();
    assert!(
        read(&zip.finish().unwrap(), DrawingStyle::default())
            .err()
            .unwrap()
            .contains("decompressed")
    );
}
#[test]
fn depth_elements_recursive_and_exponential_uses_are_bounded() {
    let deep = format!(
        "{}<path d='M0 0H1V1Z'/>{}",
        "<g>".repeat(65),
        "</g>".repeat(65)
    );
    assert!(
        read(svg(&deep).as_bytes(), DrawingStyle::default())
            .err()
            .unwrap()
            .contains("64")
    );
    let many = "<g/>".repeat(MAX_ELEMENTS);
    assert!(
        read(svg(&many).as_bytes(), DrawingStyle::default())
            .err()
            .unwrap()
            .contains("50,000")
    );
    let cyclic = r##"<defs><g id="a"><use href="#a"/></g></defs><use href="#a"/>"##;
    assert!(
        read(svg(cyclic).as_bytes(), DrawingStyle::default())
            .err()
            .unwrap()
            .contains("recursive")
    );
    let mut exp = String::from("<defs><path id='p0' d='M0 0H1V1Z'/>");
    for i in 1..18 {
        exp.push_str(&format!(
            "<g id='p{i}'><use href='#p{}'/><use href='#p{}'/></g>",
            i - 1,
            i - 1
        ));
    }
    exp.push_str("</defs><use href='#p17'/>");
    assert!(
        read(svg(&exp).as_bytes(), DrawingStyle::default())
            .err()
            .unwrap()
            .contains("50,000")
    );
}
#[test]
fn external_uses_and_doctypes_are_not_resolved() {
    let imported = read_svg(r##"<use href="file:///tmp/private.svg#x"/><path d="M0 0H1V1Z"/>"##);
    assert_eq!(imported.drawing.graphics.len(), 1);
    assert!(imported.notice.contains("external references"));
    let text = "<!DOCTYPE svg SYSTEM 'file:///tmp/private'><svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><path d='M0 0H5V5Z'/></svg>";
    let imported = read(text.as_bytes(), DrawingStyle::default()).unwrap();
    assert!(imported.notice.contains("document type declarations"));
}
#[test]
fn even_odd_holes_are_kept_as_opposite_windings() {
    let imported = read_svg("<path fill-rule='evenodd' d='M0 0H30V30H0Z M10 10H20V20H10Z'/>");
    let g = &imported.drawing.graphics[0];
    assert_eq!(
        g.path
            .iter()
            .filter(|c| matches!(c, PathCommand::Close))
            .count(),
        2
    );
    let source = g.commands();
    let areas: Vec<_> = crate::graphics::flattened(&source)
        .iter()
        .map(|points| {
            let xy: Vec<_> = points.iter().map(|p| [p.x as f64, p.y as f64]).collect();
            crate::equipment::geometry::area(&xy)
        })
        .collect();
    assert!(areas[0] * areas[1] < 0.);
}

#[test]
fn source_points_dash_expansion_and_use_chain_depth_are_capped() {
    let d = format!("M0 0{}", "L1 0L1 1".repeat(MAX_COMMANDS / 2));
    assert!(
        read(
            svg(&format!("<path d='{d}'/>")).as_bytes(),
            DrawingStyle::default()
        )
        .err()
        .unwrap()
        .contains("20,000")
    );
    let dashed =
        "<path d='M0 0H1000' fill='none' stroke='black' stroke-dasharray='.00001 .00001'/>";
    assert!(
        read(svg(dashed).as_bytes(), DrawingStyle::default())
            .err()
            .unwrap()
            .contains("dashes")
    );
    let mut chain = String::from("<defs><path id='p0' d='M0 0H1V1Z'/>");
    for i in 1..70 {
        chain.push_str(&format!("<g id='p{i}'><use href='#p{}'/></g>", i - 1));
    }
    chain.push_str("</defs><use href='#p69'/>");
    assert!(
        read(svg(&chain).as_bytes(), DrawingStyle::default())
            .err()
            .unwrap()
            .contains("deeply nested")
    );
}

#[test]
fn ordinary_overlapping_translucent_paths_keep_their_paint_alpha() {
    let imported = read_svg(
        "<path d='M0 0H20V20H0Z' fill='red' fill-opacity='.5'/><path d='M10 0H30V20H10Z' fill='blue' fill-opacity='.5'/>",
    );
    let svg = crate::export::drawing(&imported.drawing, "svg").unwrap();
    let tree = usvg::Tree::from_data(&svg, &Default::default()).unwrap();
    let size = tree.size().to_int_size();
    let mut pixels = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
    pixels.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, Default::default(), &mut pixels.as_mut());
    assert!(
        pixels.data().chunks_exact(4).any(|pixel| {
            pixel[..3]
                .iter()
                .zip([128i16, 64, 192])
                .all(|(got, expected)| (i16::from(*got) - expected).abs() <= 3)
        }),
        "the overlap must composite blue over translucent red, not over white"
    );
}

#[test]
fn ordinary_artwork_ignores_unused_recursive_definitions() {
    let imported = read_svg(
        "<defs><symbol id='unused'><use href='#unused'/></symbol></defs><path d='M0 0H10V10H0Z'/>",
    );
    assert_eq!(imported.drawing.graphics.len(), 1);
}

#[test]
fn group_opacity_is_named_separately_from_paint_opacity() {
    let imported =
        read_svg("<g opacity='.5'><path d='M0 0H20V20H0Z'/><path d='M10 0H30V20H10Z'/></g>");
    assert!(
        imported
            .notice
            .contains("groups with opacity applied to each path")
    );
    assert!(
        imported
            .drawing
            .graphics
            .iter()
            .all(|g| g.style.effects.opacity == Some(0.5))
    );
}
