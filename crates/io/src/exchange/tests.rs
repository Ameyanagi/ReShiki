use super::*;
use crate::allocation_metrics;
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

#[test]
fn shared_schema_preserves_last_property_precedence() {
    assert_eq!(SCHEMA.names.len(), 267);
    assert_eq!(SCHEMA.codes.len(), 269);
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
