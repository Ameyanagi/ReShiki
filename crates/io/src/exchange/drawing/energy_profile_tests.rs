use super::*;
use crate::{
    arrows::{ArrowStyle, Preset},
    document::Arrow,
    export::{self, Background},
    graphics::{EnergyProfile, Graphic},
};
use serde_json::json;

fn profile() -> Document {
    let mut doc = Document::default();
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(40., 0.),
        Point::new(160., 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    let states = serde_json::from_value(json!([
        {
            "kind": "reactant", "label": "R", "energy_kcal": 0.,
            "geometry": {
                "frame": {
                    "numbers": [1, 1], "positions_angstrom": [[0., 0., 0.], [0.74, 0., 0.]],
                    "charge": 0, "multiplicity": 1
                },
                "bonds": [[0, 1]], "forming": [], "breaking": []
            }
        },
        { "kind": "transition_state", "label": "TS‡", "energy_kcal": 18.5 },
        { "kind": "product", "label": "P", "energy_kcal": -4.25 }
    ]))
    .unwrap();
    let steps = serde_json::from_value(json!([
        {
            "from": 0, "transition": 1, "to": 2,
            "irc": [
                { "x": 0.08, "energy_kcal": 1.25 },
                { "x": 0.51, "energy_kcal": 18.4 },
                { "x": 0.94, "energy_kcal": -3.9 }
            ]
        }
    ]))
    .unwrap();
    let profile = EnergyProfile::new(arrow, "a".repeat(64), "GFN2-xTB".into(), states, steps);
    let (width, height) = profile.natural_size(&doc.drawing_style);
    let id = doc.next_id();
    doc.graphics.push(Graphic::energy_profile(
        id,
        Point::new(20., 60.),
        Point::new(width, height),
        profile,
        Default::default(),
    ));
    doc.current()
}

fn scaled_profile(font_size_pt: f32, scale: f32, angle: f32) -> Document {
    let mut doc = profile();
    doc.drawing_style.font_size_pt = font_size_pt;
    let g = &mut doc.graphics[0];
    let (width, height) = g
        .energy_profile
        .as_ref()
        .unwrap()
        .natural_size(&doc.drawing_style);
    g.axis_x = Point::new(width * scale, 0.).turned(Point::default(), angle);
    g.axis_y = Point::new(0., height * scale).turned(Point::default(), angle);
    doc
}

#[test]
fn energy_profile_exports_grouped_vectors_and_labels_through_cdxml_and_cdx() {
    let doc = profile();
    doc.validate().unwrap();
    let expected: Vec<_> = doc.graphics[0]
        .energy_profile_parts(&doc.drawing_style)
        .1
        .into_iter()
        .map(|a| a.text)
        .collect();
    assert!(expected.iter().any(|text| text.contains("TS‡")));
    assert!(expected.iter().any(|text| text.contains("18.5")));
    // Generated diagram vectors need no caller-supplied geometry.
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
        let reopened = crate::chemistry::cdxml::import_cdxml(xml).unwrap().document;
        reopened.validate().unwrap();
        assert!(reopened.graphics.iter().all(|g| g.energy_profile.is_none()));
        assert_eq!(reopened.annotations.len(), expected.len());
    }
}

#[test]
fn resized_energy_profile_labels_roundtrip_in_the_strict_cdxml_and_cdx_reader() {
    for (font, scale, angle) in [(10., 0.1, 0.), (4., 1., 0.), (10., 40., 37.)] {
        let doc = scaled_profile(font, scale, angle);
        doc.validate().unwrap();
        let expected = doc.graphics[0].energy_profile_parts(&doc.drawing_style).1;
        assert!(
            expected
                .iter()
                .any(|a| !(4. ..=144.).contains(&a.format.style.size_pt))
        );
        let xml = write(&doc, Options::default()).unwrap();
        let decoded = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
        for (format, xml) in [("cdxml", &xml), ("cdx", &decoded)] {
            let back = crate::chemistry::cdxml::import_cdxml(xml)
                .unwrap_or_else(|error| panic!("{format}, {font} pt at {scale}: {error}"))
                .document;
            back.validate().unwrap();
            assert_eq!(back.annotations.len(), expected.len());
            // The reader applies one common page translation.
            let offset = back.annotations[0].centre();
            let source_offset = expected[0].centre();
            for (source, actual) in expected.iter().zip(&back.annotations) {
                assert_eq!(actual.text, source.text, "{format}");
                assert!(
                    (actual.format.style.size_pt - source.format.style.size_pt.clamp(4., 144.))
                        .abs()
                        < 0.026,
                    "{format}: {}",
                    source.text
                );
                let actual = actual.centre().offset(-offset.x, -offset.y);
                let expected = source.centre().offset(-source_offset.x, -source_offset.y);
                assert!(
                    actual.distance(expected) < 0.1,
                    "{format}: {} moved from {expected:?} to {actual:?}",
                    source.text
                );
            }
        }
        assert_eq!(
            doc.graphics[0].energy_profile_parts(&doc.drawing_style).1,
            expected
        );
    }
}

#[test]
fn energy_profile_loss_and_size_notices_reach_editable_files_and_clipboard() {
    use crate::engine::export_notices::{Route, notices};
    for (scale, size_notice) in [(1., false), (0.1, true)] {
        let doc = scaled_profile(10., scale, 0.);
        let notice = energy_profile_export_notice(&doc).unwrap();
        assert_eq!(
            notice.contains(ENERGY_PROFILE_TEXT_SIZE_NOTICE),
            size_notice
        );
        for format in ["cdxml", "cdx", "mol"] {
            let warnings = notices(
                &doc,
                format,
                Route::Prepared {
                    source: &doc,
                    figure_only: false,
                },
            );
            assert_eq!(warnings.contains(&notice), format != "mol", "{format}");
        }
        let (_, warnings) = write_clipboard(&doc).unwrap();
        assert!(warnings.contains(&notice));
    }
    assert!(energy_profile_export_notice(&Document::default()).is_none());
}

#[tokio::test]
async fn energy_profile_native_files_and_figure_payloads_keep_the_complete_linked_object() {
    let doc = profile();
    let engine = crate::engine::LocalEngine::in_process();
    let saved = crate::formats::write_with_engine(doc.clone(), "rsk", &engine)
        .await
        .unwrap();
    let reopened = crate::formats::read_with_engine(saved, "rsk", &engine)
        .await
        .unwrap();
    assert_eq!(reopened.graphics, doc.graphics);
    let profile = reopened.graphics[0].energy_profile.as_ref().unwrap();
    assert!(profile.linked);
    assert_eq!(profile.source, Some(doc.arrows[0].id));
    assert!(profile.states[0].geometry.is_some());
    let embedded = crate::payload::decode(&crate::payload::encode(&doc).unwrap()).unwrap();
    assert_eq!(embedded.graphics, doc.graphics);

    let mut clipboard = embedded;
    crate::payload::unlink_energy_profiles(&mut clipboard);
    clipboard.validate().unwrap();
    let snapshot = clipboard.graphics[0].energy_profile.as_ref().unwrap();
    assert!(!snapshot.linked);
    assert_eq!(snapshot.source, None);
    assert_eq!(snapshot.states, profile.states);
    assert_eq!(snapshot.steps, profile.steps);
    assert_eq!(snapshot.steps[0].irc.len(), 3);
    assert_eq!(snapshot.source_key, profile.source_key);
    assert!(doc.graphics[0].energy_profile.as_ref().unwrap().linked);
}

fn assert_figure(format: &str, bytes: &[u8]) {
    match format {
        "svg" => {
            let svg = std::str::from_utf8(bytes).unwrap();
            assert!(svg.contains("TS‡"));
            assert!(svg.contains("18.5"));
            assert!(
                svg.contains("radialGradient"),
                "owned 3D structure is drawn"
            );
        }
        "svg-office" => {
            let svg = std::str::from_utf8(bytes).unwrap();
            assert!(!svg.contains("<text"));
            assert!(svg.matches("<path").count() > 5);
            assert!(
                svg.contains("radialGradient"),
                "owned 3D structure is drawn"
            );
        }
        "png" => {
            let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
                .read_info()
                .unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let frame = reader.next_frame(&mut pixels).unwrap();
            assert!(frame.width > 100 && frame.height > 100);
            assert!(
                pixels[..frame.buffer_size()]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| {
                        pixel[3] != 0 && pixel[..3].iter().any(|component| *component < 128)
                    })
            );
        }
        "pdf" => {
            assert!(bytes.starts_with(b"%PDF-"));
            assert!(bytes.windows(11).any(|window| window == b"/Type /Page"));
        }
        _ => panic!("Unexpected figure format {format}"),
    }
}

#[tokio::test]
async fn energy_profile_renders_through_publication_format_and_clipboard_export_paths() {
    let doc = profile();
    let original = doc.clone();
    let engine = crate::engine::LocalEngine::in_process();
    for format in ["svg", "svg-office", "pdf", "png"] {
        let figure = export::publication(
            &engine,
            doc.clone(),
            format,
            false,
            true,
            Background::Canvas,
            crate::style::DEFAULT.png_dpi,
            export::FILE_PIXELS,
        )
        .await
        .unwrap();
        assert_figure(format, &figure.bytes);
        let embedded = crate::payload::read(&figure.bytes).unwrap().unwrap();
        assert_eq!(embedded.graphics, doc.graphics);
        assert!(embedded.graphics[0].energy_profile.as_ref().unwrap().linked);
        let copied = export::clipboard_figure_with_drawing(
            &doc,
            format,
            Background::Transparent,
            crate::style::DEFAULT.png_dpi,
            None,
        )
        .unwrap();
        assert_figure(format, &copied.bytes);
    }
    let vectors = export::prepare_clipboard_vectors(&doc, Background::Transparent).unwrap();
    assert_figure("pdf", &vectors.pdf(None).unwrap().bytes);
    assert_figure("svg-office", &vectors.svg(None).unwrap().bytes);
    // The format API is shared by the CLI, MCP and browser export surfaces.
    for format in ["svg", "png", "pdf"] {
        let bytes = crate::formats::write_with_engine(doc.clone(), format, &engine)
            .await
            .unwrap();
        assert_figure(format, &bytes);
    }
    assert_eq!(doc, original);
}
