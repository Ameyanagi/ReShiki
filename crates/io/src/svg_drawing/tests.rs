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
        pixels.data().as_chunks::<4>().0.iter().any(|pixel| {
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

#[test]
fn css_filter_references_keep_artwork_in_inline_and_stylesheet_forms() {
    for body in [
        r##"<defs><filter id="f"/></defs><rect width="20" height="20" style="fill:#123456;filter:url(#f)"/>"##,
        r##"<defs><filter id="f"/></defs><style>.piece { fill:#123456; filter:url('#f') !important; }</style><rect class="piece" width="20" height="20"/>"##,
        r##"<defs><filter id="f"/></defs><style><![CDATA[.piece { fill:#123456; filter:url("#f"); }]]></style><rect class="piece" width="20" height="20"/>"##,
    ] {
        let imported = read_svg(body);
        assert_eq!(imported.drawing.graphics.len(), 1);
        assert_eq!(
            imported.drawing.graphics[0].style.fill,
            Some(Color::Custom([0x12, 0x34, 0x56]))
        );
        assert!(
            imported.notice.contains("filter styles"),
            "{}",
            imported.notice
        );
    }
}

#[test]
fn unused_branching_definitions_cannot_reach_the_parser_unbounded() {
    let mut source = String::from("<defs><path id='p0' d='M0 0H1V1Z'/>");
    for i in 1..22 {
        source.push_str(&format!(
            "<g id='p{i}'><use href='#p{}'/><use href='#p{}'/></g>",
            i - 1,
            i - 1
        ));
    }
    source.push_str("</defs><path d='M0 0H10V10H0Z'/>");
    assert_eq!(
        read_svg(&source).drawing.graphics.len(),
        1,
        "unused definitions must not reject ordinary artwork"
    );
}

#[test]
fn non_normal_blend_modes_have_a_counted_approximation_notice() {
    let imported = read_svg(
        "<rect width='20' height='20' fill='red'/><g style='mix-blend-mode:multiply'><rect x='10' width='20' height='20' fill='blue'/></g>",
    );
    assert_eq!(imported.drawing.graphics.len(), 2);
    assert!(
        imported
            .notice
            .contains("1 blend modes replaced by normal compositing"),
        "{}",
        imported.notice
    );
}

#[test]
fn css_effect_cleanup_keeps_selector_lists_entities_and_unrelated_paints() {
    let imported = read_svg(
        r##"<defs><filter id="f"/></defs><style><![CDATA[.a,.b{filter:url('#f');fill:#123456} [data-tag="a&b"]{stroke:#123456}]]></style><rect class="a" data-tag="a&amp;b" width="20" height="20"/><rect class="b" x="30" width="20" height="20" style="filter:url(&quot;#f&quot;);fill:#123456"/>"##,
    );
    assert!(
        imported
            .drawing
            .graphics
            .iter()
            .all(|g| g.style.fill == Some(Color::Custom([0x12, 0x34, 0x56])))
    );
    assert!(
        imported.notice.contains("2 filter styles"),
        "{}",
        imported.notice
    );
}

#[test]
fn paint_and_css_marker_resources_keep_their_used_paths() {
    for marker in ["marker-end='url(#m)'", "class='marked'"] {
        let source = format!(
            r##"<defs><path id="tip" d="M0 0L4 2L0 4Z" fill="blue"/><marker id="m" markerWidth="4" markerHeight="4" refX="4" refY="2" orient="auto"><use href="#tip"/></marker><linearGradient id="g"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><style>.marked{{marker-end:url(#m)}}</style><path d="M0 10H50" stroke="black" fill="none" {marker}/><rect y="20" width="20" height="20" fill="url(#g) red"/>"##
        );
        let imported = read_svg(&source);
        assert!(
            imported
                .drawing
                .graphics
                .iter()
                .any(|g| g.style.fill == Some(Color::Custom([0, 0, 255]))),
            "the marker's use must remain"
        );
        assert!(
            imported
                .drawing
                .graphics
                .iter()
                .any(|g| g.style.fill == Some(Color::Custom([128, 0, 128])))
        );
    }
}

#[test]
fn use_reachability_matches_xlink_priority_first_ids_and_dormant_children() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="96" height="96"><defs><path id="p" d="M0 0H10V10Z" fill="red"/><path id="p" d="M20 20H30V30Z" fill="blue"/><use id="u" xlink:href="#p" href="#missing"><use href="#u"/></use></defs><use href="#u"/></svg>"##;
    let imported = read(source.as_bytes(), DrawingStyle::default()).unwrap();
    assert_eq!(imported.drawing.graphics.len(), 1);
    assert_eq!(
        imported.drawing.graphics[0].style.fill,
        Some(Color::Custom([255, 0, 0]))
    );
}

#[test]
fn marker_conversion_budget_bounds_real_fanout_and_default_options_still_work() {
    let points = (0..24)
        .map(|i| format!("{i},{}", i % 2))
        .collect::<Vec<_>>()
        .join(" ");
    let marker = "<path d='M0 0L1 0L1 1Z'/>".repeat(4);
    let source = svg(&format!(
        "<defs><marker id='m' markerWidth='2' markerHeight='2'>{marker}</marker></defs><polyline points='{points}' fill='none' stroke='black' marker-mid='url(#m)'/>"
    ));
    // The source is small. Only actual marker conversion can exceed this cap.
    assert!(usvg::Tree::from_str(&source, &Default::default()).is_ok());
    for (elements, segments, depth) in [(64, 10000, 64), (50000, 64, 64), (50000, 20000, 3)] {
        let options = usvg::Options {
            conversion_limits: Some(usvg::ConversionLimits {
                max_elements: elements,
                max_path_segments: segments,
                max_depth: depth,
            }),
            ..Default::default()
        };
        assert!(matches!(
            usvg::Tree::from_str(&source, &options),
            Err(usvg::Error::ConversionLimitReached)
        ));
    }
}

#[test]
fn normal_marker_shorthand_inheritance_use_and_cycles_remain_supported() {
    for definition in ["", " marker-end='url(#m)'"] {
        let source = format!(
            "<defs><path id='tip' d='M0 0L4 2L0 4Z' fill='blue' stroke='none'{definition}/><marker id='m' markerWidth='4' markerHeight='4' orient='auto'><use href='#tip'/></marker><path id='line' d='M0 10L20 10L30 20' fill='none' stroke='black'/></defs><style>.marked{{marker:url(#m);marker-mid:none}}</style><g class='marked'><use href='#line'/></g>"
        );
        let imported = read_svg(&source);
        assert_eq!(
            imported.drawing.graphics.len(),
            3,
            "two end markers plus the stroke; recursive marker skipped"
        );
    }
}

#[test]
fn nested_marker_fanout_is_rejected_without_returning_partial_artwork() {
    let mut source = String::from("<defs>");
    for i in 0..8 {
        let next = if i == 7 {
            String::new()
        } else {
            format!(" marker-mid='url(#m{})'", i + 1)
        };
        source.push_str(&format!("<marker id='m{i}' markerWidth='4' markerHeight='4'><path d='M0 0L1 1L2 0L3 1L4 0'{next}/></marker>"));
    }
    source.push_str(
        "</defs><path d='M0 0L1 1L2 0L3 1L4 0' fill='none' stroke='black' marker-mid='url(#m0)'/>",
    );
    let error = read(svg(&source).as_bytes(), DrawingStyle::default())
        .err()
        .expect("fanout must exceed the shared budget");
    assert!(error.contains("20,000"), "{error}");
}

#[test]
fn adjacent_stylesheet_text_and_cdata_cannot_restore_removed_filters() {
    for style in [
        "<![CDATA[.a{fill:red}]]><![CDATA[.a{filter:url(#f)}]]>",
        ".a{fill:red}<![CDATA[.a{filter:url(#f)}]]>",
        "<![CDATA[.a{fill:red}]]>.a{filter:url(#f)}",
    ] {
        let imported = read_svg(&format!(
            "<defs><filter id='f'/></defs><style>{style}</style><rect class='a' width='20' height='20'/>"
        ));
        assert_eq!(imported.drawing.graphics.len(), 1);
        assert_eq!(
            imported.drawing.graphics[0].style.fill,
            Some(Color::Custom([255, 0, 0]))
        );
    }
}

#[test]
fn resources_keep_ancestor_dependencies_and_duplicate_id_targets() {
    let inherited = "<defs marker-end='url(#n)'><path id='tip' d='M0 0L1 1L0 2Z' fill='blue' stroke='none'/><marker id='m' markerWidth='4' markerHeight='4'><path d='M0 0L2 1' stroke='black'/></marker><marker id='n' markerWidth='2' markerHeight='2'><use href='#tip'/></marker></defs><path d='M0 0H20' fill='none' stroke='black' marker-end='url(#m)'/>";
    let duplicate = "<defs><path id='tip' d='M0 0L1 1L0 2Z' fill='blue' stroke='none'/><marker id='m' markerWidth='2' markerHeight='2'/><marker id='m' markerWidth='2' markerHeight='2'><use href='#tip'/></marker></defs><path d='M0 0H20' fill='none' stroke='black' marker-end='url(#m)'/>";
    for source in [inherited, duplicate] {
        let imported = read_svg(source);
        assert!(
            imported
                .drawing
                .graphics
                .iter()
                .any(|g| g.style.fill == Some(Color::Custom([0, 0, 255]))),
            "live blue marker must remain"
        );
    }
}

#[test]
fn extreme_arcs_and_rounded_primitives_are_stopped_before_geometry_allocation() {
    for body in [
        "<path d='M1e60 0 A1e60 1e60 0 0 1 -1e60 0'/>",
        "<path d='M1e36 0 A1e36 1e36 0 0 1 -1e36 0'/>",
        "<circle r='1e36'/>",
        "<ellipse rx='1e36' ry='1e36'/>",
        "<rect width='1e36' height='1e36' rx='5e35'/>",
    ] {
        // Only the bounded import path is allowed to process these inputs.
        let error = read(svg(body).as_bytes(), DrawingStyle::default())
            .err()
            .expect("geometry exceeds the configured limit");
        assert!(error.contains("20,000"), "{body}: {error}");
    }
}

#[test]
fn ordinary_relative_degenerate_and_chained_arcs_match_default_geometry() {
    let options = usvg::Options {
        conversion_limits: Some(usvg::ConversionLimits {
            max_elements: MAX_ELEMENTS,
            max_path_segments: MAX_COMMANDS,
            max_depth: MAX_DEPTH,
        }),
        ..Default::default()
    };
    for d in [
        "M10 10h20v10a15 12 20 0 1 20 5a15 12 20 0 0 -20 -5z l5 0",
        "M10 10A0 0 0 0 1 20 20A10 10 0 0 0 20 20ZZm4 5h10",
        "M10 10C10 20 20 30 30 20S40 10 45 20Q50 30 55 20T65 20a10 10 0 0 1 5 5",
    ] {
        let source = svg(&format!("<path d='{d}'/>"));
        let ordinary = usvg::Tree::from_str(&source, &Default::default()).unwrap();
        let bounded = usvg::Tree::from_str(&source, &options).unwrap();
        assert_eq!(
            bounded.to_string(&Default::default()),
            ordinary.to_string(&Default::default())
        );
    }
    let small = usvg::Options {
        conversion_limits: Some(usvg::ConversionLimits {
            max_elements: 50000,
            max_path_segments: 8,
            max_depth: 64,
        }),
        ..Default::default()
    };
    let many_arcs = svg(
        "<path d='M10 10a10 10 0 0 1 20 0a10 10 0 0 1 -20 0a10 10 0 0 1 20 0a10 10 0 0 1 -20 0'/>",
    );
    assert!(matches!(
        usvg::Tree::from_str(&many_arcs, &small),
        Err(usvg::Error::ConversionLimitReached)
    ));
}

#[test]
fn nested_marker_conversion_depth_is_bounded_even_for_shallow_source_definitions() {
    let mut source = String::from("<defs>");
    for i in 0..40 {
        let next = if i == 39 {
            String::new()
        } else {
            format!(" marker-end='url(#m{})'", i + 1)
        };
        source.push_str(&format!("<marker id='m{i}' markerWidth='4' markerHeight='4'><path d='M0 0L2 1' stroke='black'{next}/></marker>"));
    }
    source
        .push_str("</defs><path d='M0 0L20 1' fill='none' stroke='black' marker-end='url(#m0)'/>");
    let error = read(svg(&source).as_bytes(), DrawingStyle::default())
        .err()
        .expect("nested conversion depth exceeds64");
    assert!(error.contains("64 levels"), "{error}");
}

#[test]
fn nested_style_sanitizer_edits_cannot_panic_or_restore_filters() {
    let nested = "<g style='filter:url(#f)'/>".repeat(100);
    let source = svg(&format!(
        "<defs><filter id='f'/></defs><style>.a{{filter:url(#f);fill:red}}{nested}</style><rect class='a' width='20' height='20'/>"
    ));
    let imported = read(source.as_bytes(), DrawingStyle::default()).unwrap();
    assert_eq!(imported.drawing.graphics.len(), 1);
    assert_eq!(
        imported.drawing.graphics[0].style.fill,
        Some(Color::Custom([255, 0, 0]))
    );
}

#[test]
fn suppressed_path_commands_cannot_exhaust_the_simplifier_stack() {
    let source = svg(&format!("<path d='M0 0Z{}'/>", "Z".repeat(1000)));
    let error = read(source.as_bytes(), DrawingStyle::default())
        .err()
        .expect("empty-command recursion must be bounded");
    assert!(error.contains("64 levels"), "{error}");
}
