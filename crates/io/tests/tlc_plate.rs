//! Original ChemDraw 26.1 plate, through the same format routes as the editor.
use reshiki_io::{engine::LocalEngine, formats};
use reshiki_model::{
    document::{Document, Point},
    graphics::{GraphicKind, LinePattern},
    scene::{self, Primitive},
    typography::Script,
};
const XML: &[u8] = include_bytes!("fixtures/tlc/chemdraw26-mixed-spots.cdxml");
const CDX: &[u8] = include_bytes!("fixtures/tlc/chemdraw26-mixed-spots.cdx");
const REACTION_XML: &[u8] = include_bytes!("fixtures/tlc/chemdraw26-reaction-check.cdxml");
const REACTION_CDX: &[u8] = include_bytes!("fixtures/tlc/chemdraw26-reaction-check.cdx");

/// Read the tagged property framing independently of the production decoder.
/// Its permissive legacy Boolean reads cannot prove a native writer is valid.
fn native_flag_lengths(bytes: &[u8], rf_size: u16) -> Vec<(u16, usize)> {
    assert_eq!(&bytes[..12], b"VjCD0100\x04\x03\x02\x01");
    let mut cursor = 22;
    let mut flags = vec![];
    let mut objects = vec![];
    while cursor < bytes.len() {
        let code = u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
        cursor += 2;
        if code >= 0x8000 {
            cursor += 4;
            objects.push(code);
        } else if code == 0 {
            objects.pop();
        } else if code != 0 {
            let mut length = usize::from(u16::from_le_bytes(
                bytes[cursor..cursor + 2].try_into().unwrap(),
            ));
            cursor += 2;
            if length == 0xffff {
                length = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 4;
            }
            if matches!(code, 0x0aa2..=0x0aa5 | 0x0ab2) {
                flags.push((code, length));
            }
            if code == 0x0008 && objects.last() == Some(&0x8011) {
                // A prefixed name is tolerated by our reader but ChemDraw
                // misses the authored Rf record and regenerates its styling.
                assert_eq!(&bytes[cursor..cursor + length], b"rf");
            }
            if code == 0x0700 && objects.contains(&0x801f) {
                let data = &bytes[cursor..cursor + length];
                assert_eq!(u16::from_le_bytes(data[..2].try_into().unwrap()), 3);
                for run in 0..3 {
                    let offset = 2 + run * 10 + 6;
                    assert_eq!(
                        u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap()),
                        rf_size
                    );
                }
            }
            cursor += length;
        }
    }
    assert_eq!(cursor, bytes.len());
    flags.sort_unstable();
    flags
}

#[tokio::test]
async fn native_plate_normal_cdx_export_matches_sdk_flag_bytes_for_original_fixtures() {
    let engine = LocalEngine::in_process();
    for (xml, cdx, count, rf_size) in [(XML, CDX, 3, 150), (REACTION_XML, REACTION_CDX, 4, 180)] {
        let expected = vec![(0x0aa2, 0), (0x0aa3, 0), (0x0aa4, 0), (0x0ab2, 0)];
        assert_eq!(native_flag_lengths(cdx, rf_size), expected);
        for (format, bytes) in [("cdxml", xml), ("cdx", cdx)] {
            let (mut doc, warnings) = formats::read_with_warnings(bytes.to_vec(), format, &engine)
                .await
                .unwrap();
            assert!(warnings.is_empty(), "{format}: {warnings:?}");
            assert_eq!(doc.graphics.len(), 1);
            let plate = doc.graphics[0].tlc.as_deref().unwrap();
            assert_eq!(plate.lanes.len(), 3);
            assert_eq!(
                plate.lanes.iter().map(|l| l.spots.len()).sum::<usize>(),
                count
            );
            let bytes = formats::write_with_engine(doc.clone(), "cdx", &engine)
                .await
                .unwrap();
            assert_eq!(native_flag_lengths(&bytes, rf_size), expected);
            for shown in [false, true] {
                let plate = doc.graphics[0].tlc.as_deref_mut().unwrap();
                plate.show_origin = shown;
                plate.show_solvent_front = shown;
                plate.show_borders = shown;
                // Keep the one authored label; other spots have no label to show.
                for spot in plate.lanes.iter_mut().flat_map(|l| &mut l.spots) {
                    spot.show_rf = shown && spot.label.is_some();
                }
                let bytes = formats::write_with_engine(doc.clone(), "cdx", &engine)
                    .await
                    .unwrap();
                assert_eq!(
                    native_flag_lengths(&bytes, rf_size),
                    if shown { expected.clone() } else { vec![] }
                );
                let back = formats::read_with_engine(bytes, "cdx", &engine)
                    .await
                    .unwrap();
                let plate = back.graphics[0].tlc.as_deref().unwrap();
                assert_eq!(plate.show_origin, shown);
                assert_eq!(plate.show_solvent_front, shown);
                assert_eq!(plate.show_borders, shown);
                assert_eq!(
                    plate
                        .lanes
                        .iter()
                        .flat_map(|l| &l.spots)
                        .filter(|s| s.show_rf)
                        .count(),
                    usize::from(shown)
                );
            }
        }
    }
}

#[tokio::test]
async fn native_wide_reaction_plate_geometry_and_authored_rf_anchor_survive_normal_exchange() {
    let engine = LocalEngine::in_process();
    let scale = 42. / 14.4;
    for (format, source) in [("cdxml", REACTION_XML), ("cdx", REACTION_CDX)] {
        let doc = formats::read_with_engine(source.to_vec(), format, &engine)
            .await
            .unwrap();
        let g = &doc.graphics[0];
        let plate = g.tlc.as_deref().unwrap();
        for (lane, x) in [193.33333, 270., 346.66666].into_iter().enumerate() {
            for spot in &plate.lanes[lane].spots {
                let center = plate.spot_center(g, lane, spot.rf);
                let y = 151. + 200. * (0.9 - spot.rf * 0.8);
                assert!(
                    center.distance(Point::new(x * scale, (y * f64::from(scale)) as f32)) < 0.002
                );
            }
        }
        let label = plate
            .label_annotation(g, 2, &plate.lanes[2].spots[0])
            .unwrap();
        assert_eq!(label.format.style.size_pt, 9.);
        let baseline = rf_anchor(&scene::primitives(&doc)).unwrap();
        assert!(baseline.distance(Point::new(350.47 * scale, 276. * scale)) < 0.025);
        let label_offset = label.position.offset(-g.origin.x, -g.origin.y);
        let baseline_offset = baseline.offset(-g.origin.x, -g.origin.y);
        // Move the plate, then serialize through the app's normal format API.
        // Neither lane recalculation nor native writing may shift its Rf record.
        let mut moved = doc.clone();
        moved.graphics[0].map_positions(|p| p.offset(40., 30.));
        for format in ["rsk", "cdxml", "cdx"] {
            let bytes = formats::write_with_engine(moved.clone(), format, &engine)
                .await
                .unwrap();
            let back = formats::read_with_engine(bytes, format, &engine)
                .await
                .unwrap();
            let p = &back.graphics[0];
            let t = p.tlc.as_deref().unwrap();
            let restored = t.label_annotation(p, 2, &t.lanes[2].spots[0]).unwrap();
            assert_eq!(restored.text, label.text);
            assert_eq!(restored.format.style.size_pt, 9.);
            // Editable exchange applies one document-wide page translation;
            // compare the owned record relative to its plate frame.
            assert!(
                restored
                    .position
                    .offset(-p.origin.x, -p.origin.y)
                    .distance(label_offset)
                    < 0.002
            );
            assert!(
                rf_anchor(&scene::primitives(&back))
                    .unwrap()
                    .offset(-p.origin.x, -p.origin.y)
                    .distance(baseline_offset)
                    < 0.002
            );
            if format == "rsk" {
                assert!(restored.position.distance(label.position.offset(40., 30.)) < 0.002);
            }
        }
    }
}

#[tokio::test]
async fn native_tlc_ten_point_margin_boundary_stays_stable_after_coordinate_rounding() {
    let engine = LocalEngine::in_process();
    for width in [9.99, 10., 10.000001, 10.01] {
        let source = format!(
            "<CDXML><page id='1'><tlcplate id='2' TopLeft='0 0' TopRight='{width} 0' BottomRight='{width} 200' BottomLeft='0 200' OriginFraction='0.1' SolventFrontFraction='0.1'><tlclane id='3'><tlcspot id='4' Width='65536' Height='65536' Rf='0.5'/></tlclane><tlclane id='5'/><tlclane id='6'><tlcspot id='7' Width='65536' Height='65536' Rf='0.5'/></tlclane></tlcplate></page></CDXML>"
        );
        let doc = formats::read_with_engine(source.into_bytes(), "cdxml", &engine)
            .await
            .unwrap();
        let g = &doc.graphics[0];
        let t = g.tlc.as_deref().unwrap();
        let before =
            [0, 2].map(|lane| t.spot_center(g, lane, 0.5).offset(-g.origin.x, -g.origin.y));
        for target in ["cdxml", "cdx"] {
            let bytes = formats::write_with_engine(doc.clone(), target, &engine)
                .await
                .unwrap();
            let back = formats::read_with_engine(bytes, target, &engine)
                .await
                .unwrap();
            let g = &back.graphics[0];
            let t = g.tlc.as_deref().unwrap();
            for (lane, expected) in [0, 2].into_iter().zip(before) {
                assert!(
                    t.spot_center(g, lane, 0.5)
                        .offset(-g.origin.x, -g.origin.y)
                        .distance(expected)
                        < 0.002,
                    "{width} {target}"
                );
            }
        }
    }
}

/// Recover the native baseline anchor of R from the shared rendered text.
fn rf_anchor(primitives: &[Primitive]) -> Option<Point> {
    primitives.iter().find_map(|p| match p {
        Primitive::Text {
            position,
            text,
            style,
            ..
        } if text == "R" => Some(position.offset(0., reshiki_model::style::text_ascent(style))),
        Primitive::Rotated {
            pivot,
            angle,
            primitives,
        } => rf_anchor(primitives).map(|p| p.turned(*pivot, *angle)),
        Primitive::Opacity { primitive, .. } => rf_anchor(std::slice::from_ref(primitive.as_ref())),
        _ => None,
    })
}
fn assert_plate(doc: &Document) {
    doc.validate().unwrap();
    assert!(doc.atoms.is_empty() && doc.annotations.is_empty());
    assert_eq!(doc.graphics.len(), 1);
    let g = &doc.graphics[0];
    assert_eq!(g.kind, GraphicKind::TlcPlate);
    let t = g.tlc.as_deref().unwrap();
    assert_eq!(t.lanes.len(), 3);
    assert!(t.show_borders && t.show_origin && t.show_solvent_front);
    assert_eq!(t.origin_fraction, 0.1);
    assert_eq!(t.solvent_front_fraction, 0.1);
    for (i, (lane, rf)) in t.lanes.iter().zip([0.28, 0.51, 0.77]).enumerate() {
        let s = &lane.spots[0];
        assert_eq!(s.rf, rf);
        assert_eq!(s.width_pt, 325095. / 65536.);
        assert_eq!(s.height_pt, s.width_pt);
        assert_eq!(s.style.fill.is_some(), i != 2);
        assert_eq!(s.show_rf, i == 0);
        assert_eq!(s.style.pattern, LinePattern::Solid);
    }
    let label = t.lanes[0].spots[0].label.as_ref().unwrap();
    assert_eq!(label.text, "Rf = 0.28");
    assert!(
        label
            .format
            .spans
            .iter()
            .any(|s| s.style.script == Script::Subscript)
    );
    let p = t.spot_center(g, 0, 0.28);
    assert!(p.is_finite());
    assert_eq!(
        scene::primitives(doc)
            .iter()
            .filter(|p| matches!(p, Primitive::Path { .. }))
            .count(),
        9
    );
}
#[tokio::test]
async fn original_chemdraw_tlc_imports_as_one_editable_graphic_in_xml_and_binary() {
    let engine = LocalEngine::in_process();
    for (format, bytes) in [("cdxml", XML), ("cdx", CDX)] {
        let (doc, warnings) = formats::read_with_warnings(bytes.to_vec(), format, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{format}: {warnings:?}");
        assert_plate(&doc);
        let t = doc.graphics[0].tlc.as_deref().unwrap();
        let scale = 42. / 14.4;
        assert!(
            doc.graphics[0]
                .origin
                .distance(Point::new(330. * scale, 383. * scale))
                < 0.002
        );
        let parts = doc.graphics[0].parts();
        let first = match parts[6].commands[0] {
            reshiki_model::graphics::PathCommand::Move(p) => p,
            _ => panic!("spot start"),
        };
        assert!(
            (first.x
                - t.spot_center(&doc.graphics[0], 0, 0.28).x
                - (325095. / 65536. + f64::from(doc.graphics[0].style.width_pt)) as f32
                    * scale
                    * 0.5)
                .abs()
                < 0.002
        );
        let anchor = rf_anchor(&scene::primitives(&doc)).unwrap();
        // The original XML rounds its display coordinates to two decimals;
        // the original CDX retains the authored signed 16.16 anchor.
        let source_anchor = if format == "cdx" {
            Point::new(
                (22_477_227. / 65536. * f64::from(scale)) as f32,
                (33_765_767. / 65536. * f64::from(scale)) as f32,
            )
        } else {
            Point::new(342.98 * scale, 515.22 * scale)
        };
        assert!(anchor.distance(source_anchor) < 0.002);
        let ink: Vec<_> = scene::primitives(&doc)
            .iter()
            .flat_map(|p| match p {
                Primitive::Text {
                    position,
                    text,
                    size,
                    style,
                    ..
                } => reshiki_model::style::text_ink_boxes(text, *size, style)
                    .into_iter()
                    .flat_map(|(lo, hi)| [position.offset(lo.x, lo.y), position.offset(hi.x, hi.y)])
                    .collect::<Vec<_>>(),
                _ => vec![],
            })
            .collect();
        let lo = ink
            .iter()
            .fold(Point::new(f32::INFINITY, f32::INFINITY), |a, b| {
                Point::new(a.x.min(b.x), a.y.min(b.y))
            });
        let hi = ink
            .iter()
            .fold(Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY), |a, b| {
                Point::new(a.x.max(b.x), a.y.max(b.y))
            });
        eprintln!(
            "{format} original Rf ink in pt: ({:.5},{:.5})..({:.5},{:.5}); ChemDraw authored BBox (343.56,509.83)..(372.77,516.81)",
            lo.x / scale,
            lo.y / scale,
            hi.x / scale,
            hi.y / scale
        );
        for (i, y) in [518.2, 481.4, 439.8].into_iter().enumerate() {
            assert!(
                (t.spot_center(&doc.graphics[0], i, t.lanes[i].spots[0].rf).y - y * scale).abs()
                    < 0.002
            );
        }
    }
}
#[tokio::test]
async fn native_plate_roundtrips_normal_rsk_cdxml_and_cdx_without_flattening() {
    let engine = LocalEngine::in_process();
    let doc = formats::read_with_engine(XML.to_vec(), "cdxml", &engine)
        .await
        .unwrap();
    for target in ["rsk", "cdxml", "cdx"] {
        let bytes = formats::write_with_engine(doc.clone(), target, &engine)
            .await
            .unwrap();
        if target == "cdxml" {
            let xml = std::str::from_utf8(&bytes).unwrap();
            assert!(
                xml.contains("<tlcplate") && xml.contains("<tlclane") && xml.contains("<tlcspot")
            );
            assert!(!xml.contains("<curve"));
        }
        let (restored, warnings) = formats::read_with_warnings(bytes, target, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{target}: {warnings:?}");
        assert_plate(&restored);
        if target == "rsk" {
            assert_eq!(restored.graphics, doc.graphics);
        }
    }
}
#[tokio::test]
async fn native_plate_reflected_convex_fourth_corner_and_rich_label_survive_editable_exchange() {
    let engine = LocalEngine::in_process();
    let mut doc = formats::read_with_engine(XML.to_vec(), "cdxml", &engine)
        .await
        .unwrap();
    let g = &mut doc.graphics[0];
    g.tlc.as_deref_mut().unwrap().bottom_right = Point::new(1.2, 0.95);
    g.tlc.as_deref_mut().unwrap().lanes[0].spots[0]
        .label
        .as_mut()
        .unwrap()
        .angle = 30.;
    g.map_positions(|p| Point::new(1500. - p.x, p.y + 50.));
    for target in ["cdxml", "cdx"] {
        let bytes = formats::write_with_engine(doc.clone(), target, &engine)
            .await
            .unwrap();
        let (restored, warnings) = formats::read_with_warnings(bytes, target, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{target}: {warnings:?}");
        assert_plate(&restored);
        let (a, b) = (&doc.graphics[0], &restored.graphics[0]);
        let (ta, tb) = (a.tlc.as_deref().unwrap(), b.tlc.as_deref().unwrap());
        // Export translates the drawing onto the page. Edge vectors and local
        // fourth-corner semantics survive that normal origin normalization.
        assert!(a.axis_x.distance(b.axis_x) < 0.002 && a.axis_y.distance(b.axis_y) < 0.002);
        assert!(ta.bottom_right.distance(tb.bottom_right) < 0.0001);
        assert!(
            a.tlc_labels()[0]
                .position
                .offset(-a.origin.x, -a.origin.y)
                .distance(b.tlc_labels()[0].position.offset(-b.origin.x, -b.origin.y))
                < 0.01
        );
    }
}
#[tokio::test]
async fn default_original_plate_has_three_filled_rf_point15_spots_without_labels() {
    let engine = LocalEngine::in_process();
    let (doc, warnings) = formats::read_with_warnings(
        include_bytes!("fixtures/tlc/chemdraw26-default.cdxml").to_vec(),
        "cdxml",
        &engine,
    )
    .await
    .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let g = &doc.graphics[0];
    let t = g.tlc.as_deref().unwrap();
    assert_eq!(t.lanes.len(), 3);
    for lane in &t.lanes {
        assert_eq!(lane.spots[0].rf, 0.15);
        assert!(lane.spots[0].style.fill.is_some());
        assert!(!lane.spots[0].show_rf);
    }
    assert!(g.tlc_labels().is_empty());
}
#[tokio::test]
async fn native_plate_shared_figure_exports_include_geometry_and_label() {
    let engine = LocalEngine::in_process();
    let doc = formats::read_with_engine(XML.to_vec(), "cdxml", &engine)
        .await
        .unwrap();
    let svg = formats::write_with_engine(doc.clone(), "svg", &engine)
        .await
        .unwrap();
    let svg = std::str::from_utf8(&svg).unwrap();
    assert!(svg.contains("<path") && svg.contains("0.28"));
    let pdf = formats::write_with_engine(doc, "pdf", &engine)
        .await
        .unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}

#[tokio::test]
async fn native_plate_inherits_plate_color_and_width_and_warns_for_approximated_artwork() {
    let engine = LocalEngine::in_process();
    let sheet = |attributes: &str, child: &str| {
        format!(
            "<CDXML LineWidth='0.6' color='3'><colortable><color r='1' g='1' b='1'/><color r='0' g='0' b='0'/><color r='1' g='0' b='0'/><color r='0' g='0' b='1'/></colortable><page id='1'><tlcplate id='2' TopLeft='0 0' TopRight='40 0' BottomRight='40 200' BottomLeft='0 200' OriginFraction='0.1' SolventFrontFraction='0.1' LineWidth='1.5' color='5' ShowOrigin='yes' ShowSolventFront='yes' ShowBorders='yes'><tlclane id='3'><tlcspot id='4' Rf='0.3' Width='65536' Height='131072' CurveType='128' {attributes}>{child}</tlcspot></tlclane></tlcplate></page></CDXML>"
        )
    };
    let (doc, warnings) = formats::read_with_warnings(sheet("", "").into_bytes(), "cdxml", &engine)
        .await
        .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let g = &doc.graphics[0];
    let spot = &g.tlc.as_deref().unwrap().lanes[0].spots[0];
    assert_eq!(spot.style.width_pt, 1.5);
    assert_eq!(spot.style.stroke.rgb(), [0, 0, 255]);
    assert_eq!(spot.width_pt, 1.);
    assert_eq!(spot.height_pt, 2.);
    for (attributes, child) in [
        ("Tail='42'", ""),
        ("ShowRf='yes'", ""),
        ("", "<unknownspot id='9'/>"),
    ] {
        let (doc, warnings) =
            formats::read_with_warnings(sheet(attributes, child).into_bytes(), "cdxml", &engine)
                .await
                .unwrap();
        assert_eq!(doc.graphics.len(), 1);
        assert!(doc.graphics[0].tlc.is_some());
        assert!(
            warnings.iter().any(|w| w.contains("TLC")),
            "{attributes}: {warnings:?}"
        );
    }
}

#[tokio::test]
async fn malformed_plate_warns_and_keeps_other_supported_drawing_objects() {
    let engine = LocalEngine::in_process();
    let source = String::from_utf8(XML.to_vec())
        .unwrap()
        .replace("BottomRight=\"370 583\"", "BottomRight=\"NaN 583\"")
        .replace(
            "</page>",
            "<graphic id='900' GraphicType='Rectangle' BoundingBox='0 0 10 10'/></page>",
        );
    let (doc, warnings) = formats::read_with_warnings(source.into_bytes(), "cdxml", &engine)
        .await
        .unwrap();
    assert_eq!(doc.graphics.len(), 1);
    assert_eq!(doc.graphics[0].kind, GraphicKind::Rectangle);
    assert!(
        warnings.iter().any(|w| w.contains("TLC plate")),
        "{warnings:?}"
    );
}

#[tokio::test]
async fn native_rf_baseline_follows_authored_rotation_and_inherited_text_style() {
    let engine = LocalEngine::in_process();
    let source = String::from_utf8(XML.to_vec()).unwrap().replace(
        "<t\n p=",
        "<t RotationAngle='17776640' CaptionJustification='Right'\n p=",
    );
    let (doc, warnings) = formats::read_with_warnings(source.into_bytes(), "cdxml", &engine)
        .await
        .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let label = doc.graphics[0].tlc_labels().remove(0);
    assert!((label.angle - 271.25).abs() < 0.001);
    // For right-aligned text the native p anchor is its baseline's right end.
    let layout = reshiki_model::typography::layout(&label.text, &label.format);
    let primitive_anchor = rf_anchor(&scene::primitives(&doc)).unwrap();
    let native_anchor = primitive_anchor
        .offset(layout.width, 0.)
        .turned(primitive_anchor, label.angle);
    assert!(
        native_anchor.distance(Point::new(342.98 * (42. / 14.4), 515.22 * (42. / 14.4))) < 0.002
    );
    for target in ["cdxml", "cdx"] {
        let bytes = formats::write_with_engine(doc.clone(), target, &engine)
            .await
            .unwrap();
        let (back, warnings) = formats::read_with_warnings(bytes, target, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{target}: {warnings:?}");
        let a = &doc.graphics[0];
        let b = &back.graphics[0];
        assert!((a.tlc_labels()[0].angle - b.tlc_labels()[0].angle).abs() < 0.001);
        assert!(
            a.tlc_labels()[0]
                .position
                .offset(-a.origin.x, -a.origin.y)
                .distance(b.tlc_labels()[0].position.offset(-b.origin.x, -b.origin.y))
                < 0.01
        );
    }
}

#[tokio::test]
async fn native_tlc_hidden_rf_keeps_authored_text_style_and_position_records() {
    let engine = LocalEngine::in_process();
    let mut doc = formats::read_with_engine(XML.to_vec(), "cdxml", &engine)
        .await
        .unwrap();
    doc.graphics[0].tlc.as_deref_mut().unwrap().lanes[0].spots[0].show_rf = false;
    assert!(doc.graphics[0].tlc_labels().is_empty());
    for target in ["rsk", "cdxml", "cdx"] {
        let bytes = formats::write_with_engine(doc.clone(), target, &engine)
            .await
            .unwrap();
        let (back, warnings) = formats::read_with_warnings(bytes, target, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{target}: {warnings:?}");
        assert!(back.graphics[0].tlc_labels().is_empty());
        let a = &doc.graphics[0].tlc.as_deref().unwrap().lanes[0].spots[0];
        let b = &back.graphics[0].tlc.as_deref().unwrap().lanes[0].spots[0];
        assert!(!b.show_rf);
        assert_eq!(
            a.label.as_ref().unwrap().text,
            b.label.as_ref().unwrap().text
        );
        assert_eq!(
            a.label.as_ref().unwrap().format,
            b.label.as_ref().unwrap().format
        );
        assert!(
            a.label
                .as_ref()
                .unwrap()
                .offset
                .distance(b.label.as_ref().unwrap().offset)
                < 0.01
        );
    }
}

#[tokio::test]
async fn native_plate_group_membership_survives_normal_editable_exchange() {
    let engine = LocalEngine::in_process();
    let source=String::from_utf8(XML.to_vec()).unwrap().replace("<tlcplate\n","<group id='900'><tlcplate\n")
        .replace("</tlcplate>","</tlcplate><graphic id='901' GraphicType='Rectangle' BoundingBox='300 300 320 320'/></group>");
    let doc = formats::read_with_engine(source.into_bytes(), "cdxml", &engine)
        .await
        .unwrap();
    assert_eq!(doc.graphics.len(), 2);
    assert_eq!(doc.groups.len(), 1);
    for target in ["cdxml", "cdx"] {
        let bytes = formats::write_with_engine(doc.clone(), target, &engine)
            .await
            .unwrap();
        let (back, warnings) = formats::read_with_warnings(bytes, target, &engine)
            .await
            .unwrap();
        assert!(warnings.is_empty(), "{target}: {warnings:?}");
        assert_eq!(back.graphics.len(), 2);
        assert_eq!(back.groups.len(), 1);
        let mut actual = back.groups[0].members.clone();
        actual.sort_unstable();
        let mut expected: Vec<_> = back.graphics.iter().map(|g| g.id).collect();
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }
}
#[tokio::test]
async fn hidden_native_plate_uses_the_existing_hidden_artwork_notice() {
    let engine = LocalEngine::in_process();
    let source = String::from_utf8(XML.to_vec())
        .unwrap()
        .replace("<tlcplate\n", "<tlcplate Visible='no'\n")
        .replace(
            "</page>",
            "<graphic id='900' GraphicType='Rectangle' BoundingBox='0 0 10 10'/></page>",
        );
    let (doc, warnings) = formats::read_with_warnings(source.into_bytes(), "cdxml", &engine)
        .await
        .unwrap();
    assert_eq!(doc.graphics.len(), 1);
    assert_eq!(doc.graphics[0].kind, GraphicKind::Rectangle);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Hidden drawing objects")),
        "{warnings:?}"
    );
}
#[tokio::test]
async fn native_plate_can_be_an_external_reaction_condition_drawing_with_existing_metadata_notice()
{
    let engine = LocalEngine::in_process();
    let source = String::from_utf8(XML.to_vec()).unwrap().replace(
        "</page>",
        "<scheme id='900'><step id='901' ReactionStepObjectsAboveArrow='6'/></scheme></page>",
    );
    let (doc, warnings) = formats::read_with_warnings(source.into_bytes(), "cdxml", &engine)
        .await
        .unwrap();
    assert_plate(&doc);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("external reaction roles")),
        "{warnings:?}"
    );
}

#[tokio::test]
async fn native_rf_tag_exception_does_not_accept_an_unowned_indicator() {
    let engine = LocalEngine::in_process();
    let source = "<CDXML><page><graphic GraphicType='Rectangle' BoundingBox='0 0 10 10'/><objecttag Name='rf'><t p='10 20'><s size='9'>unowned</s></t></objecttag></page></CDXML>";
    let error = formats::read_with_engine(source.as_bytes().to_vec(), "cdxml", &engine)
        .await
        .unwrap_err();
    assert!(error.contains("Unattached object tags"), "{error}");
}
