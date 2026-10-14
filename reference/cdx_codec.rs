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

fn binary_differences(actual: &[u8], expected: &[u8]) -> Vec<(&'static str, String)> {
    let mut differences = Vec::new();
    if actual.len() != expected.len() {
        differences.push((
            "binary length",
            format!(
                "encoded binary lengths {} != {}",
                actual.len(),
                expected.len()
            ),
        ));
    }
    for offset in 0..actual.len().max(expected.len()) {
        if actual.get(offset) != expected.get(offset) {
            let byte = |value: Option<&u8>| {
                value
                    .map(|value| format!("0x{value:02x}"))
                    .unwrap_or_else(|| "<missing>".into())
            };
            differences.push((
                "binary byte",
                format!(
                    "encoded binary byte {offset}: actual {}, expected {}",
                    byte(actual.get(offset)),
                    byte(expected.get(offset))
                ),
            ));
        }
    }
    differences
}

fn xml_differences(left: &str, right: &str) -> anyhow::Result<Vec<(&'static str, String)>> {
    fn compare(
        a: roxmltree::Node<'_, '_>,
        b: roxmltree::Node<'_, '_>,
        path: &str,
        differences: &mut Vec<(&'static str, String)>,
    ) {
        if a.tag_name().name() != b.tag_name().name() {
            differences.push((
                "XML tag",
                format!(
                    "{path}: tag {} != {}",
                    a.tag_name().name(),
                    b.tag_name().name()
                ),
            ));
        }
        let attrs = |n: roxmltree::Node<'_, '_>| {
            n.attributes()
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect::<BTreeMap<_, _>>()
        };
        let a_attrs = attrs(a);
        let b_attrs = attrs(b);
        for (key, av) in &a_attrs {
            let Some(bv) = b_attrs.get(key) else {
                differences.push((
                    "XML unexpected attribute",
                    format!("{path}/@{key}: unexpected={av:?}"),
                ));
                continue;
            };
            // Font sizes and table colors may have equivalent decimal spelling.
            let equal = if let (Ok(x), Ok(y)) = (av.parse::<f64>(), bv.parse::<f64>()) {
                (x - y).abs() <= 1e-12 * x.abs().max(1.)
            } else {
                av == bv
            };
            if !equal {
                differences.push((
                    "XML attribute value",
                    format!("{path}/@{key}: {av:?} != {bv:?}"),
                ));
            }
        }
        for (key, bv) in &b_attrs {
            if !a_attrs.contains_key(key) {
                differences.push((
                    "XML missing attribute",
                    format!("{path}/@{key}: missing, expected={bv:?}"),
                ));
            }
        }
        if (a.tag_name().name() == "s" || b.tag_name().name() == "s")
            && a.text().unwrap_or("") != b.text().unwrap_or("")
        {
            differences.push((
                "XML text",
                format!(
                    "{path}: text {:?} != {:?}",
                    a.text().unwrap_or(""),
                    b.text().unwrap_or("")
                ),
            ));
        }
        let ac = a.children().filter(|n| n.is_element()).collect::<Vec<_>>();
        let bc = b.children().filter(|n| n.is_element()).collect::<Vec<_>>();
        if ac.len() != bc.len() {
            differences.push((
                "XML child count",
                format!("{path}: child counts {} != {}", ac.len(), bc.len()),
            ));
        }
        for (index, (a, b)) in ac.iter().zip(&bc).enumerate() {
            compare(
                *a,
                *b,
                &format!("{path}/{}[{index}]", a.tag_name().name()),
                differences,
            );
        }
        for (index, a) in ac.iter().enumerate().skip(bc.len()) {
            differences.push((
                "XML unexpected child",
                format!(
                    "{path}/{}[{index}]: unexpected={:?}",
                    a.tag_name().name(),
                    &a.document().input_text()[a.range()]
                ),
            ));
        }
        for (index, b) in bc.iter().enumerate().skip(ac.len()) {
            differences.push((
                "XML missing child",
                format!(
                    "{path}/{}[{index}]: missing, expected={:?}",
                    b.tag_name().name(),
                    &b.document().input_text()[b.range()]
                ),
            ));
        }
    }
    let actual = roxmltree::Document::parse(left).context("Invalid actual decoded XML")?;
    let expected = roxmltree::Document::parse(right).context("Invalid expected decoded XML")?;
    let mut differences = Vec::new();
    compare(
        actual.root_element(),
        expected.root_element(),
        "/CDXML",
        &mut differences,
    );
    Ok(differences)
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
        .arg(root.join("reference/cdx_reference.py"))
        .env("PYTHONUTF8", "1")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout)?;
    assert!(cases.len() > 400);
    let mut failures = Vec::new();
    let mut categories = BTreeMap::<&str, usize>::new();
    let (mut encoder_checks, mut failing_cases) = (0, 0);
    let decoder_checks = cases.len();
    for case in cases {
        let failures_before = failures.len();
        let bytes = STANDARD.decode(&case.binary)?;
        if let Some(xml) = case.xml {
            encoder_checks += 1;
            match to_cdx(&xml) {
                Ok(actual) if actual == bytes => {}
                Ok(actual) => {
                    for (category, difference) in binary_differences(&actual, &bytes) {
                        *categories.entry(category).or_default() += 1;
                        failures.push(format!("{}: {difference}", case.name));
                    }
                }
                Err(error) => {
                    *categories.entry("encode rejection").or_default() += 1;
                    failures.push(format!("{}: encode rejected: {error}", case.name));
                }
            }
        }
        match from_cdx(&bytes) {
            Ok(decoded) => match xml_differences(&decoded, &case.decoded) {
                Ok(differences) => {
                    for (category, difference) in differences {
                        *categories.entry(category).or_default() += 1;
                        failures.push(format!("{}: decoded {difference}", case.name));
                    }
                }
                Err(error) => {
                    *categories.entry("XML parse rejection").or_default() += 1;
                    failures.push(format!("{}: decoded XML: {error:#}", case.name));
                }
            },
            Err(error) => {
                *categories.entry("decode rejection").or_default() += 1;
                failures.push(format!("{}: decode rejected: {error}", case.name));
            }
        }
        failing_cases += usize::from(failures.len() != failures_before);
    }
    eprintln!(
        "CDX codec: {encoder_checks} encoder comparisons, {decoder_checks} decoder comparisons; {failing_cases} failing cases, {} mismatches; categories {categories:?}",
        failures.len()
    );
    if !failures.is_empty() {
        std::fs::create_dir_all(root.join("artifacts"))?;
        std::fs::write(
            root.join("artifacts/cdx-codec-failures.txt"),
            format!(
                "{failing_cases} failing cases, {} mismatches; categories {categories:?}\n\n{}\n",
                failures.len(),
                failures.join("\n")
            ),
        )?;
    }
    anyhow::ensure!(
        failures.is_empty(),
        "{} mismatches across {failing_cases} cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
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
