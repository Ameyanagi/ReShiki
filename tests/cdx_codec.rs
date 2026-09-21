//! Exact binary and semantic XML comparison against the pre-migration Python codec.
use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki::exchange::{from_cdx, to_cdx};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, process::Command};

type TestResult = anyhow::Result<()>;

#[derive(Deserialize)]
struct Case {
    name: String,
    xml: Option<String>,
    binary: String,
    decoded: String,
}

fn same_xml(left: &str, right: &str, name: &str) -> TestResult {
    fn compare(a: roxmltree::Node<'_, '_>, b: roxmltree::Node<'_, '_>, name: &str) {
        assert_eq!(a.tag_name().name(), b.tag_name().name(), "{name}");
        let attrs = |n: roxmltree::Node<'_, '_>| {
            n.attributes()
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect::<BTreeMap<_, _>>()
        };
        let a_attrs = attrs(a);
        let b_attrs = attrs(b);
        assert_eq!(
            a_attrs.keys().collect::<Vec<_>>(),
            b_attrs.keys().collect::<Vec<_>>(),
            "{name}"
        );
        for (key, av) in a_attrs {
            let bv = &b_attrs[&key];
            // Font sizes and table colors may have equivalent decimal spelling.
            if let (Ok(x), Ok(y)) = (av.parse::<f64>(), bv.parse::<f64>()) {
                assert!(
                    (x - y).abs() <= 1e-12 * x.abs().max(1.),
                    "{name}: {key}: {av} != {bv}"
                );
            } else {
                assert_eq!(&av, bv, "{name}: {key}");
            }
        }
        if a.tag_name().name() == "s" {
            assert_eq!(a.text().unwrap_or(""), b.text().unwrap_or(""), "{name}");
        }
        let ac = a.children().filter(|n| n.is_element()).collect::<Vec<_>>();
        let bc = b.children().filter(|n| n.is_element()).collect::<Vec<_>>();
        assert_eq!(ac.len(), bc.len(), "{name}");
        for (a, b) in ac.into_iter().zip(bc) {
            compare(a, b, name);
        }
    }
    compare(
        roxmltree::Document::parse(left)?.root_element(),
        roxmltree::Document::parse(right)?.root_element(),
        name,
    );
    Ok(())
}

#[test]
fn matches_python_reference_for_native_files_all_properties_and_charsets() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let output = Command::new(python)
        .arg(root.join("tests/cdx_reference.py"))
        .env("PYTHONUTF8", "1")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout)?;
    assert!(cases.len() > 400);
    for case in cases {
        let bytes = STANDARD.decode(&case.binary)?;
        if let Some(xml) = case.xml {
            assert_eq!(
                to_cdx(&xml).map_err(anyhow::Error::msg)?,
                bytes,
                "{}",
                case.name
            );
        }
        let decoded = from_cdx(&bytes)
            .map_err(anyhow::Error::msg)
            .with_context(|| case.name.clone())?;
        same_xml(&decoded, &case.decoded, &case.name)?;
    }
    Ok(())
}

#[test]
fn corrupt_inputs_and_limits_return_errors_without_partial_output() -> TestResult {
    let data = to_cdx(
        "<CDXML><page id=\"1\"><fragment id=\"2\"><n id=\"3\" p=\"0 0\" Element=\"8\"/></fragment></page></CDXML>",
    ).map_err(anyhow::Error::msg)?;
    for end in 0..data.len() - 2 {
        assert!(from_cdx(&data[..end]).is_err(), "truncated at {end}");
    }
    assert!(from_cdx(&[data.clone(), b"junk".to_vec()].concat()).is_err());
    for xml in [
        "<CDXML><page id=\"1\"><n id=\"1\"/></page></CDXML>",
        "<CDXML><n Element=\"NaN\"/></CDXML>",
        "<CDXML><n Element=\"32768\"/></CDXML>",
        "<CDXML><n p=\"inf 0\"/></CDXML>",
        "<CDXML><n p=\"32768 0\"/></CDXML>",
        "<CDXML><n Bogus=\"1\"/></CDXML>",
        "<CDXML><unsupported/></CDXML>",
        "<CDXML><page id=\"4294967296\"/></CDXML>",
        "<CDXML><embeddedobject PNG=\"xyz\"/></CDXML>",
        "<!DOCTYPE CDXML [<!ENTITY x 'expanded'>]><CDXML Name=\"&x;\"/>",
    ] {
        assert!(to_cdx(xml).is_err(), "{xml}");
    }
    assert!(
        to_cdx(&format!(
            "<CDXML>{}{} </CDXML>",
            "<group>".repeat(66),
            "</group>".repeat(66)
        ))
        .is_err()
    );
    assert!(
        to_cdx(&format!(
            "<CDXML><t><s>{}</s></t></CDXML>",
            "x".repeat(65536)
        ))
        .is_err()
    );
    assert!(from_cdx(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
    // Deterministic mutations exercise lengths, tags, IDs, and text bounds.
    for i in 0..data.len() {
        for value in [0, 0x7f, 0xff] {
            let mut changed = data.clone();
            changed[i] = value;
            let _ = from_cdx(&changed);
        }
    }
    Ok(())
}

#[test]
fn malformed_properties_nesting_and_object_counts_are_bounded() -> TestResult {
    fn object(code: u16, id: u32, body: &[u8]) -> Vec<u8> {
        [
            code.to_le_bytes().as_slice(),
            id.to_le_bytes().as_slice(),
            body,
            &[0, 0],
        ]
        .concat()
    }
    fn property(code: u16, body: &[u8]) -> Vec<u8> {
        [
            code.to_le_bytes().as_slice(),
            (body.len() as u16).to_le_bytes().as_slice(),
            body,
        ]
        .concat()
    }
    fn drawing(body: &[u8]) -> Vec<u8> {
        [
            b"VjCD0100\x04\x03\x02\x01".as_slice(),
            &[0; 10],
            object(0x8000, 0, body).as_slice(),
            &[0, 0],
        ]
        .concat()
    }
    for body in [
        property(0x7777, &[]),          // Unknown feature on a drawing object.
        property(0xF, &[2]),            // Invalid boolean.
        property(0x200, &[0; 7]),       // Truncated coordinate pair.
        property(0x700, &[0xff, 0xff]), // Truncated style array.
        property(0x700, &[1, 0, 0xff, 0xff, 3, 0, 0, 0, 200, 0, 3, 0, b'x']), // Bad text offset.
        property(0x431, &[1, 0, 0]),    // Partial object ID.
        property(0x80A, &[0; 7]),       // Truncated default font style.
        property(0x504, &f64::NAN.to_le_bytes()),
        property(0x504, &f64::INFINITY.to_le_bytes()),
    ] {
        assert!(from_cdx(&drawing(&object(0x8004, 1, &body))).is_err());
    }
    let mut nested = Vec::new();
    for id in 1..=65 {
        nested = object(0x8002, id, &nested);
    }
    assert!(from_cdx(&drawing(&nested)).is_err());
    let mut objects = Vec::new();
    for id in 1..=100_000 {
        objects.extend(object(0x8004, id, &[]));
    }
    assert!(from_cdx(&drawing(&objects)).is_err()); // Root counts too.
    assert!(to_cdx(&format!("<CDXML>{}</CDXML>", "<n/>".repeat(100_000))).is_err());
    let properties = property(0x7777, &[]).repeat(1_000_001);
    assert!(from_cdx(&drawing(&properties)).is_err());
    Ok(())
}
