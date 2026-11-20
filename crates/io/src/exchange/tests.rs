use super::*;
use crate::{allocation_metrics, chemistry::cdxml, palette::Color, typography::Script};
use std::{hint::black_box, time::Instant};

fn local_schema() -> Schema {
    Schema {
        names: schema::PROPERTIES.iter().map(|p| (p.name, p)).collect(),
        codes: schema::PROPERTIES.iter().map(|p| (p.code, p)).collect(),
    }
}

fn first_read_frame(code: u16) -> Vec<u8> {
    let mut bytes = HEADER.to_vec();
    for (object, id) in [(0x8000u16, 0u32), (0x8004, 1)] {
        bytes.extend_from_slice(&object.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    property(&mut bytes, code, &8u16.to_le_bytes()).unwrap();
    bytes.extend_from_slice(&[0; 6]);
    bytes
}

fn assert_symbol_caption(xml: &str) {
    let document = cdxml::import_cdxml(xml).unwrap().document;
    let caption = &document.annotations[0];
    let expected = "α β σ Δ ν → ± ′";
    assert_eq!(caption.text, format!("{expected}x"));
    let symbol = &caption.format.style;
    assert_eq!(symbol.family, "Helvetica");
    assert_eq!(symbol.size_pt, 14.0);
    assert!(symbol.bold && symbol.italic);
    assert_eq!(symbol.script, Script::Superscript);
    assert_eq!(symbol.color, Color::Custom([255, 0, 0]));
    assert_eq!(caption.format.spans.len(), 1);
    assert_eq!(caption.format.spans[0].start, expected.len());
    assert_eq!(caption.format.at(expected.len()).family, "Times");
}

#[test]
fn cdxml_imports_symbol_font_as_unicode() {
    let xml = "<CDXML CaptionFont='3'>
        <colortable><color r='1' g='1' b='1'/><color/><color r='1'/></colortable>
        <fonttable><font id='3' name='Helvetica'/>
        <font id='23' charset='x-mac-roman' name='sYmBoL'/>
        <font id='9' name='Times'/></fonttable>
        <page id='1'><t id='2' p='20 40'>
            <s font='23' size='14' face='67' color='4'>a b s D n ® ± ¢</s><s font='9'>x</s>
        </t></page></CDXML>";
    assert_symbol_caption(xml);
}

#[test]
fn cdx_imports_symbol_font_bytes_using_the_legacy_charset() {
    for charset in [10000u16, 1252] {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&0x8000u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let mut fonts = vec![0, 0, 3, 0];
        for (id, name) in [(3u16, "Helvetica"), (23, "Symbol"), (9, "Times")] {
            fonts.extend_from_slice(&id.to_le_bytes());
            fonts.extend_from_slice(&charset.to_le_bytes());
            fonts.extend_from_slice(&word(name.len()).unwrap());
            fonts.extend_from_slice(name.as_bytes());
        }
        property(&mut bytes, 0x0100, &fonts).unwrap();
        property(&mut bytes, 0x081b, &3u16.to_le_bytes()).unwrap();
        let colors: Vec<_> = [3u16, 65535, 65535, 65535, 0, 0, 0, 65535, 0, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        property(&mut bytes, 0x0300, &colors).unwrap();
        for (object, id) in [(0x8001u16, 1u32), (0x8006, 2)] {
            bytes.extend_from_slice(&object.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        property(&mut bytes, 0x0200, &[0; 8]).unwrap();
        // The encoder writes Unicode, so build a legacy Text property.
        let text = b"a b s D n \xae \xb1 \xa2x";
        let mut runs = 2u16.to_le_bytes().to_vec();
        for run in [
            [0u16, 23, 67, 280, 4],
            [text.len() as u16 - 1, 9, 0, 200, 3],
        ] {
            runs.extend(run.into_iter().flat_map(u16::to_le_bytes));
        }
        runs.extend_from_slice(text);
        property(&mut bytes, 0x0700, &runs).unwrap();
        bytes.extend_from_slice(&[0; 8]);
        assert_symbol_caption(&from_cdx_tolerant(&bytes).unwrap());
    }
}

#[test]
fn utf8_symbol_font_keeps_existing_greek() {
    let xml = "<CDXML><fonttable><font id='23' charset='utf-8' name='Symbol'/></fonttable>
        <page id='1'><t id='2' p='20 40'><s font='23'>α</s></t></page></CDXML>";
    let decoded = from_cdx(&to_cdx(xml).unwrap()).unwrap();
    for source in [xml, decoded.as_str()] {
        let document = cdxml::import_cdxml(source).unwrap().document;
        let caption = &document.annotations[0];
        assert_eq!(caption.text, "α");
        assert_eq!(caption.format.style.family, "Arial");
    }
}

#[test]
fn shared_schema_preserves_last_property_precedence() {
    assert_eq!(SCHEMA.names.len(), 268);
    assert_eq!(SCHEMA.codes.len(), 270);
    assert_eq!(SCHEMA.names.get("SupersededBy").unwrap().code, 0x0013);
    assert_eq!(SCHEMA.names.get("Formula").unwrap().code, 0x0503);
    let fields = |p: &Property| (p.code, p.name, p.kind, p.variants);
    for property in schema::PROPERTIES {
        let last = schema::PROPERTIES
            .iter()
            .rev()
            .find(|p| p.name == property.name)
            .unwrap();
        assert_eq!(
            fields(SCHEMA.codes.get(&property.code).unwrap()),
            fields(property)
        );
        assert_eq!(
            fields(SCHEMA.names.get(property.name).unwrap()),
            fields(last)
        );
    }
}

#[test]
fn native_numbered_connections_are_preserved_as_data() {
    for (bytes, numbers) in [
        (
            &include_bytes!("../../../../tests/fixtures/numbered-attachments/single.cdx")[..],
            vec!["1"],
        ),
        (
            &include_bytes!("../../../../tests/fixtures/numbered-attachments/two.cdx")[..],
            vec!["2", "1"],
        ),
        (
            &include_bytes!("../../../../tests/fixtures/numbered-attachments/sparse.cdx")[..],
            vec!["3", "1"],
        ),
    ] {
        let xml = from_cdx(bytes).unwrap();
        let encoded = to_cdx(&xml).unwrap();
        let restored = from_cdx(&encoded).unwrap();
        let doc = roxmltree::Document::parse(&restored).unwrap();
        let actual: Vec<_> = doc
            .descendants()
            .filter_map(|node| node.attribute("ExternalConnectionNum"))
            .collect();
        assert_eq!(actual, numbers);
        if numbers.len() == 2 {
            assert!(restored.contains("ConnectionOrder=\"14 15\""));
            assert!(restored.contains("BondOrdering=\"32 31\""));
        }
    }
}

#[test]
fn numbered_connection_encoding_checks_applicability_and_signed_byte() {
    for (kind, number) in [
        ("ExternalConnectionPoint", "0"),
        ("ExternalConnectionPoint", "-1"),
        ("ExternalConnectionPoint", "128"),
        ("ExternalConnectionPoint", "1.5"),
        ("Element", "1"),
    ] {
        let xml =
            format!("<CDXML><n NodeType=\"{kind}\" ExternalConnectionNum=\"{number}\"/></CDXML>");
        assert!(to_cdx(&xml).is_err(), "accepted {kind}/{number}");
    }
    // Property order does not control applicability checks. NodeType follows
    // the number, so malformed or wrong-node data cannot bypass validation.
    let frame = |number: &[u8], kind: u16| {
        let mut bytes = HEADER.to_vec();
        for (object, id) in [(0x8000u16, 0u32), (0x8004, 1)] {
            bytes.extend_from_slice(&object.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        property(&mut bytes, 0x044b, number).unwrap();
        property(&mut bytes, 0x0400, &kind.to_le_bytes()).unwrap();
        bytes.extend_from_slice(&[0; 6]);
        bytes
    };
    assert!(
        from_cdx(&frame(&[1], 12))
            .unwrap()
            .contains("ExternalConnectionNum=\"1\"")
    );
    for (number, kind) in [
        (&[][..], 12),
        (&[1, 0][..], 12),
        (&[0][..], 12),
        (&[255][..], 12),
        (&[128][..], 12),
        (&[1][..], 1),
    ] {
        assert!(from_cdx(&frame(number, kind)).is_err());
    }
    for (tag, value) in [(0x044b, &[1][..]), (0x0400, &[12, 0][..])] {
        let mut duplicate = frame(&[1], 12);
        duplicate.truncate(duplicate.len() - 6);
        property(&mut duplicate, tag, value).unwrap();
        duplicate.extend_from_slice(&[0; 6]);
        assert!(from_cdx(&duplicate).unwrap_err().contains("unique"));
    }
    let mut missing_type = frame(&[1], 12);
    missing_type.truncate(missing_type.len() - 12);
    missing_type.extend_from_slice(&[0; 6]);
    assert!(from_cdx(&missing_type).unwrap_err().contains("unique"));
    let largest = to_cdx(
        "<CDXML><n NodeType=\"ExternalConnectionPoint\" ExternalConnectionNum=\"127\"/></CDXML>",
    )
    .unwrap();
    assert!(
        from_cdx(&largest)
            .unwrap()
            .contains("ExternalConnectionNum=\"127\"")
    );
    assert_eq!(
        from_cdx(&first_read_frame(0x7777)).unwrap_err(),
        "Unsupported binary drawing property 0x7777"
    );
}

#[test]
fn shared_connection_validation_returns_numbers_and_preserves_error_precedence() {
    for (text, number) in [("1", 1), (" +001 ", 1), ("127", 127)] {
        assert_eq!(
            external_connection_number("n", Some("ExternalConnectionPoint"), text),
            Ok(number)
        );
    }
    for number in ["", "0", "-1", "128", "1.5", "invalid"] {
        assert_eq!(
            external_connection_number("n", Some("ExternalConnectionPoint"), number).unwrap_err(),
            "ExternalConnectionNum must be a positive signed byte"
        );
        for (name, kind) in [
            ("b", Some("ExternalConnectionPoint")),
            ("n", Some("Element")),
            ("n", None),
        ] {
            assert_eq!(
                external_connection_number(name, kind, number).unwrap_err(),
                "ExternalConnectionNum requires an external connection point"
            );
        }
    }
}

#[test]
fn numbered_connection_property_uniqueness_keeps_decode_precedence() {
    let frame = |properties: &[(u16, &[u8])]| {
        let mut bytes = HEADER.to_vec();
        for (object, id) in [(0x8000u16, 0u32), (0x8004, 1)] {
            bytes.extend_from_slice(&object.to_le_bytes());
            bytes.extend_from_slice(&id.to_le_bytes());
        }
        for &(tag, value) in properties {
            property(&mut bytes, tag, value).unwrap();
        }
        bytes.extend_from_slice(&[0; 6]);
        bytes
    };
    // Without a connection number, repeated NodeType properties keep their
    // historical last-value behavior instead of triggering uniqueness checks.
    assert_eq!(
        from_cdx(&frame(&[(0x0400, &[12, 0]), (0x0400, &[1, 0])])).unwrap(),
        "<CDXML><n id=\"1\" NodeType=\"Element\" /></CDXML>"
    );
    for properties in [
        vec![(0x7777, &[][..]), (0x044b, &[0][..])],
        vec![
            (0x7777, &[][..]),
            (0x044b, &[0][..]),
            (0x0400, &[12, 0][..]),
            (0x0400, &[12, 0][..]),
        ],
        vec![
            (0x7777, &[][..]),
            (0x044b, &[0][..]),
            (0x044b, &[0][..]),
            (0x0400, &[12, 0][..]),
        ],
    ] {
        assert_eq!(
            from_cdx(&frame(&properties)).unwrap_err(),
            "Numbered external connection needs unique number and NodeType properties"
        );
    }
    assert_eq!(
        from_cdx(&frame(
            &[(0x7777, &[]), (0x044b, &[0]), (0x0400, &[12, 0]),]
        ))
        .unwrap_err(),
        "Unsupported binary drawing property 0x7777"
    );
}

#[test]
fn concurrent_codec_calls_keep_ids_and_counters_independent() {
    // With this test run alone, the concurrent decoders also initialize SCHEMA.
    let xml = "<CDXML><page id=\"1\"/></CDXML>";
    let mut expected = HEADER.to_vec();
    expected.extend_from_slice(&0x8000u16.to_le_bytes());
    expected.extend_from_slice(&0u32.to_le_bytes());
    expected.extend_from_slice(&0x8001u16.to_le_bytes());
    expected.extend_from_slice(&1u32.to_le_bytes());
    expected.extend_from_slice(&[0; 6]);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..20 {
                    assert_eq!(to_cdx(xml).unwrap(), expected);
                    assert_eq!(from_cdx(&expected).unwrap(), "<CDXML><page id=\"1\" /></CDXML>");
                    let stationery = to_cds(xml).unwrap();
                    assert_eq!(style_from_cdx(&stationery).unwrap(),
                        "<CDXML LabelFont=\"3\" LabelFace=\"0\" LabelSize=\"10\" LabelColor=\"3\" />");
                }
            });
        }
    });
}

#[test]
#[ignore = "isolated requested-Rust-allocation and timing measurement"]
fn measure_schema_retention() {
    const COUNT: usize = 1000;
    let first_use = std::env::var("RESHIKI_SCHEMA_FIRST_USE").unwrap_or_else(|_| "force".into());
    let code = schema::PROPERTIES
        .iter()
        .find(|p| p.name == "Element")
        .unwrap()
        .code;
    let first_read = first_read_frame(if first_use == "failed_read" {
        0x7777
    } else {
        code
    });
    let baseline = allocation_metrics::reset();
    let start = Instant::now();
    match first_use.as_str() {
        "force" => {
            black_box(LazyLock::force(&SCHEMA));
        }
        "valid_read" => {
            drop(from_cdx(&first_read).unwrap());
        }
        "failed_read" => {
            assert_eq!(
                from_cdx(&first_read).unwrap_err(),
                "Unsupported binary drawing property 0x7777"
            );
        }
        "invalid_header" => {
            assert_eq!(
                from_cdx(b"invalid").unwrap_err(),
                "Invalid binary drawing header"
            );
        }
        _ => panic!("Unknown schema first-use workload: {first_use}"),
    }
    let cold_elapsed = start.elapsed();
    let cold = allocation_metrics::snapshot();
    let retained = cold.live_bytes.saturating_sub(baseline);
    let before_warm = cold.live_bytes;
    black_box(LazyLock::force(&SCHEMA));
    let warming_retained = allocation_metrics::snapshot()
        .live_bytes
        .saturating_sub(before_warm);
    for shared in [false, true] {
        let baseline = allocation_metrics::reset();
        let start = Instant::now();
        for _ in 0..COUNT {
            if shared {
                black_box(SCHEMA.names.get(black_box("BondLength")));
            } else {
                let schema = local_schema();
                black_box(schema.names.get(black_box("BondLength")));
            }
        }
        let elapsed = start.elapsed();
        let snapshot = allocation_metrics::snapshot();
        println!(
            "shared={shared} count={COUNT} elapsed={elapsed:?} allocations={} allocated={} peak_extra={} retained_extra={}",
            snapshot.allocation_count,
            snapshot.allocated_bytes,
            snapshot.peak_bytes.saturating_sub(baseline),
            snapshot.live_bytes.saturating_sub(baseline)
        );
    }
    println!(
        "first_use={first_use} cold_elapsed={cold_elapsed:?} cold_allocations={} cold_allocated={} cold_peak_extra={} permanently_retained_after_first_call={retained} warming_added_permanent={warming_retained}; run this exact test alone for cold data",
        cold.allocation_count,
        cold.allocated_bytes,
        cold.peak_bytes.saturating_sub(baseline)
    );
}
