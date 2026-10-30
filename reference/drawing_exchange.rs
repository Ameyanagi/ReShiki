use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    engine::Request,
    exchange::drawing,
};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
};
#[path = "drawing_exchange/cases.rs"]
mod cases;
#[path = "support/reference_presentation.rs"]
#[allow(dead_code)] // This target uses the source identity helper only.
mod reference_presentation;

struct Oracle {
    child: std::process::Child,
    output: BufReader<std::process::ChildStdout>,
}
impl Oracle {
    fn new() -> anyhow::Result<Self> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let python = root.join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        });
        let mut child = Command::new(python)
            .arg(root.join("reference/drawing_exchange_reference.py"))
            .env("PYTHONUTF8", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let output = BufReader::new(child.stdout.take().context("Missing oracle output")?);
        Ok(Self { child, output })
    }
    fn request(&mut self, request: &Request) -> anyhow::Result<Value> {
        let input = self.child.stdin.as_mut().context("Missing oracle input")?;
        serde_json::to_writer(&mut *input, &serde_json::to_value(request)?)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut line = String::new();
        anyhow::ensure!(self.output.read_line(&mut line)? != 0, "Oracle exited");
        Ok(serde_json::from_str(&line)?)
    }
}
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn compare_xml(actual: &str, expected: &str, name: &str) -> anyhow::Result<()> {
    compare_xml_with_source(actual, expected, name, None)
}

fn compare_xml_with_source(
    actual: &str,
    expected: &str,
    name: &str,
    source: Option<&Document>,
) -> anyhow::Result<()> {
    let a = roxmltree::Document::parse(actual)?;
    let e = roxmltree::Document::parse(expected)?;
    let a: Vec<_> = a.descendants().filter(|n| n.is_element()).collect();
    let e: Vec<_> = e.descendants().filter(|n| n.is_element()).collect();
    let counts = |nodes: &[roxmltree::Node<'_, '_>], tag: &str| {
        let mut ids = std::collections::BTreeMap::<String, usize>::new();
        for node in nodes.iter().filter(|node| node.has_tag_name(tag)) {
            if let Some(id) = node.attribute("id") {
                *ids.entry(id.to_owned()).or_default() += 1;
            }
        }
        ids
    };
    let (actual_ids, expected_ids) = (counts(&a, "n"), counts(&e, "n"));
    let (actual_bond_ids, expected_bond_ids) = (counts(&a, "b"), counts(&e, "b"));
    let source_value = source.map(serde_json::to_value).transpose()?;
    let source_dative = reference_presentation::dative_export_bonds(source_value.as_ref())?;
    let mut failures = Vec::new();
    if a.len() != e.len() {
        failures.push(format!(
            "[objects] actual={}, expected={}",
            a.len(),
            e.len()
        ));
    }
    for (index, (a, e)) in a.iter().zip(e.iter()).enumerate() {
        let location = format!(
            "object[{index}]/{:?}[id={:?}]",
            e.tag_name(),
            e.attribute("id")
        );
        if a.tag_name().name() != e.tag_name().name() {
            failures.push(format!(
                "[tag] {location}: actual={:?}, expected={:?}",
                a.tag_name(),
                e.tag_name()
            ));
        }
        // An annotation must not be interpreted as another molecule by
        // ChemDraw; check this exact historical attribute addition.
        let caption = e.has_tag_name("t")
            && e.attribute("CaptionLineHeight").is_some()
            && e.parent()
                .is_some_and(|p| p.has_tag_name("page") || p.has_tag_name("group"));
        if caption
            && (a.attribute("InterpretChemically") != Some("no")
                || e.attribute("InterpretChemically").is_some())
        {
            failures.push(format!(
                "[caption] {location}: actual InterpretChemically={:?}, expected={:?}",
                a.attribute("InterpretChemically"),
                e.attribute("InterpretChemically")
            ));
        }
        // The unchanged Python writer maps atom index i to XML id i+3.
        // Read no_implicit/explicit_h/charge/element from the source request.
        let zero_hydrogen = e.has_tag_name("n")
            && a.has_tag_name("n")
            && a.attribute("id") == e.attribute("id")
            && e.attribute("id").is_some_and(|id| {
                actual_ids.get(id) == Some(&1) && expected_ids.get(id) == Some(&1)
            })
            && e.attribute("NumHydrogens").is_none()
            && a.attribute("NumHydrogens") == Some("0")
            && e.attribute("id")
                .and_then(|id| {
                    id.parse::<usize>()
                        .ok()
                        .filter(|parsed| parsed.to_string() == id)
                })
                .and_then(|id| id.checked_sub(3))
                .and_then(|index| source.and_then(|doc| doc.atoms.get(index)))
                .is_some_and(|atom| {
                    atom.no_implicit
                        && atom.explicit_h == 0
                        && (atom.charge != 0 || atom.element != "C")
                });
        // #300 removes the frozen writer's dashed order-5 Order=1 override.
        // Source indices establish identity, including donor B and acceptor E.
        // Every other attribute must remain exact, even numeric spellings.
        let dative_order = a.has_tag_name("b")
            && e.has_tag_name("b")
            && a.attribute("id") == e.attribute("id")
            && e.attribute("id").is_some_and(|id| {
                actual_bond_ids.get(id) == Some(&1)
                    && expected_bond_ids.get(id) == Some(&1)
                    && source_dative.get(id).is_some_and(|(begin, end)| {
                        e.attribute("B") == Some(begin.as_str())
                            && e.attribute("E") == Some(end.as_str())
                            && [begin, end].into_iter().all(|endpoint| {
                                actual_ids.get(endpoint) == Some(&1)
                                    && expected_ids.get(endpoint) == Some(&1)
                            })
                    })
            })
            && e.attribute("Order") == Some("1")
            && a.attribute("Order") == Some("dative")
            && a.attribute("Display") == Some("Dash")
            && e.attribute("Display") == Some("Dash")
            && a.attributes()
                .filter(|attr| attr.name() != "Order")
                .map(|attr| (attr.name(), attr.value()))
                .collect::<std::collections::BTreeMap<_, _>>()
                == e.attributes()
                    .filter(|attr| attr.name() != "Order")
                    .map(|attr| (attr.name(), attr.value()))
                    .collect::<std::collections::BTreeMap<_, _>>();
        for attr in a.attributes() {
            if e.attribute(attr.name()).is_none()
                && !(caption && attr.name() == "InterpretChemically")
                && !(zero_hydrogen && attr.name() == "NumHydrogens")
            {
                failures.push(format!(
                    "[attribute-added] {location}@{}: actual={:?}",
                    attr.name(),
                    attr.value()
                ));
            }
        }
        for attr in e.attributes() {
            let Some(value) = a.attribute(attr.name()) else {
                failures.push(format!(
                    "[attribute-missing] {location}@{}: expected={:?}",
                    attr.name(),
                    attr.value()
                ));
                continue;
            };
            if dative_order && attr.name() == "Order" {
                continue;
            }
            if let Err(error) =
                compare_attribute(value, attr.value(), attr.name(), e.has_tag_name("color"))
            {
                failures.push(format!(
                    "[attribute-value] {location}@{}: {error:#}",
                    attr.name()
                ));
            }
        }
        let text = |n: &roxmltree::Node<'_, '_>| {
            n.children()
                .filter(|c| c.is_text())
                .filter_map(|c| c.text())
                .collect::<String>()
        };
        let (actual_text, expected_text) = (text(a), text(e));
        if actual_text != expected_text {
            failures.push(format!(
                "[text] {location}: actual={actual_text:?}, expected={expected_text:?}"
            ));
        }
        let children =
            |n: &roxmltree::Node<'_, '_>| n.children().filter(|c| c.is_element()).count();
        if children(a) != children(e) {
            failures.push(format!(
                "[children] {location}: actual={}, expected={}",
                children(a),
                children(e)
            ));
        }
    }
    for (nodes, paired, category) in [
        (&a, e.len(), "object-added"),
        (&e, a.len(), "object-missing"),
    ] {
        for (index, node) in nodes.iter().enumerate().skip(paired) {
            let attributes = node
                .attributes()
                .map(|attribute| (attribute.name(), attribute.value()))
                .collect::<Vec<_>>();
            let text = node
                .children()
                .filter(|child| child.is_text())
                .filter_map(|child| child.text())
                .collect::<String>();
            failures.push(format!(
                "[{category}] object[{index}]/{:?}[id={:?}]: attributes={attributes:?}, text={text:?}",
                node.tag_name(),
                node.attribute("id")
            ));
        }
    }
    anyhow::ensure!(
        failures.is_empty(),
        "{name}: {} mismatches\n{}",
        failures.len(),
        failures.join("\n")
    );
    Ok(())
}

fn compare_attribute(value: &str, expected: &str, name: &str, color: bool) -> anyhow::Result<()> {
    if value == expected {
        return Ok(());
    }
    if name == "PNG" {
        let pixels = |s: &str| -> anyhow::Result<image::RgbaImage> {
            anyhow::ensure!(s.len().is_multiple_of(2), "Invalid PNG hex");
            let bytes = s
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| {
                    u8::from_str_radix(std::str::from_utf8(p)?, 16).map_err(anyhow::Error::from)
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(image::load_from_memory(&bytes)?.to_rgba8())
        };
        anyhow::ensure!(
            pixels(value)? == pixels(expected)?,
            "picture pixels changed"
        );
        return Ok(());
    }
    if name == "BondSpacing" {
        // The historical writer widens f32 before converting to percent.
        let percent = (expected.parse::<f64>()? as f32).to_string();
        anyhow::ensure!(
            value.parse::<f64>()? == percent.parse::<f64>()?,
            "BondSpacing: actual={value:?}, expected={expected:?}"
        );
        return Ok(());
    }
    if color && matches!(name, "r" | "g" | "b") {
        let before = expected.parse::<f64>()?;
        let after = value.parse::<f64>()?;
        let channel = (before * 255.).round();
        anyhow::ensure!((0.0..=1.0).contains(&after), "color range");
        anyhow::ensure!((after * 255.).trunc() == channel, "native RGB changed");
        anyhow::ensure!(
            (after * 65535.).round() == channel * 257.,
            "CDX RGB changed"
        );
        anyhow::ensure!(
            (after - before).abs() <= 0.00000001000001,
            "bounded color precision correction exceeded"
        );
        return Ok(());
    }
    let geometry = matches!(
        name,
        "p" | "BoundingBox"
            | "CurvePoints"
            | "Center3D"
            | "MajorAxisEnd3D"
            | "MinorAxisEnd3D"
            | "Head3D"
            | "Tail3D"
    );
    let numeric = geometry
        || matches!(
            name,
            "BondLength"
                | "LabelSize"
                | "CaptionSize"
                | "LineWidth"
                | "BoldWidth"
                | "MarginWidth"
                | "HashSpacing"
                | "BondSpacing"
                | "size"
                | "CaptionLineHeight"
                | "WordWrapWidth"
                | "RotationAngle"
                | "r"
                | "g"
                | "b"
        );
    if numeric {
        let values = |s: &str| {
            s.split_whitespace()
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()
        };
        if let (Ok(av), Ok(ev)) = (values(value), values(expected)) {
            anyhow::ensure!(
                av.len() == ev.len(),
                "numeric dimensions: actual={}, expected={}",
                av.len(),
                ev.len()
            );
            let mut failures = Vec::new();
            for (index, (a, e)) in av.into_iter().zip(ev).enumerate() {
                let difference = (a - e).abs();
                if !difference.is_finite() || difference > if geometry { 0.0000011 } else { 0. } {
                    failures.push(format!("component[{index}]: {a} != {e}"));
                }
            }
            anyhow::ensure!(failures.is_empty(), "{}", failures.join("; "));
            return Ok(());
        }
    }
    anyhow::ensure!(value == expected, "actual={value:?}, expected={expected:?}");
    Ok(())
}

#[test]
fn rotation_angle_comparison_accepts_signed_zero_only_as_equal_numbers() -> anyhow::Result<()> {
    let expected = r#"<CDXML><page><embeddedobject id="1" RotationAngle="0"/></page></CDXML>"#;
    let actual = expected.replace("RotationAngle=\"0\"", "RotationAngle=\"-0\"");
    compare_xml(&actual, expected, "signed zero")?;
    for angle in ["0.000001", "1", "NaN", "inf"] {
        let actual = expected.replace("RotationAngle=\"0\"", &format!("RotationAngle=\"{angle}\""));
        assert!(
            compare_xml(&actual, expected, "changed angle").is_err(),
            "a distinct rotation must fail: {angle}"
        );
    }
    Ok(())
}

#[test]
fn zero_hydrogen_addition_requires_source_semantics_and_preserves_other_attributes()
-> anyhow::Result<()> {
    let expected = r#"<CDXML><page><fragment><n id="3" Element="7" Charge="1" p="1 2"><t><s>N</s></t></n></fragment></page></CDXML>"#;
    let actual = expected.replace("id=\"3\"", "id=\"3\" NumHydrogens=\"0\"");
    let mut source = Document::default();
    source.add_atom("N", Point::default());
    source.atoms[0].id = 9007199254740993;
    source.atoms[0].charge = 1;
    source.atoms[0].no_implicit = true;
    compare_xml_with_source(&actual, expected, "qualifying source", Some(&source))?;
    assert!(compare_xml(&actual, expected, "missing source").is_err());
    for changed in [
        actual.replace("NumHydrogens=\"0\"", "NumHydrogens=\"00\""),
        actual.replace("NumHydrogens=\"0\"", "NumHydrogens=\"1\""),
        actual.replace("Element=\"7\"", "Element=\"8\""),
        actual.replace("Charge=\"1\"", "Charge=\"2\""),
        actual.replace("p=\"1 2\"", "p=\"2 2\""),
        actual.replace("<s>N</s>", "<s>NH</s>"),
        actual.replace("id=\"3\"", "id=\"4\""),
    ] {
        assert!(
            compare_xml_with_source(&changed, expected, "unrelated change", Some(&source)).is_err()
        );
    }
    let noncanonical_expected = expected.replace("id=\"3\"", "id=\"03\"");
    let noncanonical_actual = actual.replace("id=\"3\"", "id=\"03\"");
    assert!(
        compare_xml_with_source(
            &noncanonical_actual,
            &noncanonical_expected,
            "source IDs must follow the legacy writer exactly",
            Some(&source)
        )
        .is_err()
    );
    let legacy = expected.replace("id=\"3\"", "id=\"3\" NumHydrogens=\"1\"");
    assert!(
        compare_xml_with_source(
            &actual,
            &legacy,
            "legacy must lack attribute",
            Some(&source)
        )
        .is_err()
    );
    for (element, charge, no_implicit, explicit_h) in
        [("N", 1, false, 0), ("N", 1, true, 1), ("C", 0, true, 0)]
    {
        source.atoms[0].element = element.into();
        source.atoms[0].charge = charge;
        source.atoms[0].no_implicit = no_implicit;
        source.atoms[0].explicit_h = explicit_h;
        assert!(
            compare_xml_with_source(&actual, expected, "nonqualifying source", Some(&source))
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn dative_export_order_allowance_preserves_all_other_attributes() -> anyhow::Result<()> {
    let expected = r#"<CDXML><page><fragment><n id="3" Element="7"/><n id="4" Element="29"/><b id="5" B="3" E="4" Order="1" Display="Dash" Display2="Solid" color="3" Z="-2" DoublePosition="Left"/></fragment></page></CDXML>"#;
    let actual = expected.replace("Order=\"1\"", "Order=\"dative\"");
    let mut source = Document::default();
    source.add_atom("N", Point::default());
    source.add_atom("Cu", Point::new(28., 0.));
    source.atoms[0].id = 9007199254740993;
    source.atoms[1].id = u64::MAX;
    source.add_bond(source.atoms[0].id, source.atoms[1].id, 5, "dashed");
    compare_xml_with_source(&actual, expected, "documented dative order", Some(&source))?;
    assert!(compare_xml(&actual, expected, "missing independent source").is_err());
    for (old, new) in [
        ("B=\"3\"", "B=\"4\""),
        ("E=\"4\"", "E=\"3\""),
        ("id=\"5\"", "id=\"6\""),
        (" id=\"5\"", ""),
        ("Display=\"Dash\"", "Display=\"Solid\""),
        ("Display2=\"Solid\"", "Display2=\"Dash\""),
        ("color=\"3\"", "color=\"4\""),
        ("Z=\"-2\"", "Z=\"-1\""),
        ("Z=\"-2\"", "Z=\"-02\""),
        ("DoublePosition=\"Left\"", "DoublePosition=\"Right\""),
        ("color=\"3\"", "color=\"3\" IgnoreWarnings=\"yes\""),
        ("Element=\"7\"", "Element=\"8\""),
    ] {
        assert!(
            compare_xml_with_source(
                &actual.replace(old, new),
                expected,
                "unrelated dative change",
                Some(&source)
            )
            .is_err(),
            "must preserve {old}"
        );
    }
    compare_xml_with_source(&actual, &actual, "unchanged dative output", Some(&source))?;
    for order in ["2", "3", "1.5", "hydrogen", "01"] {
        assert!(
            compare_xml_with_source(
                &actual,
                &expected.replace("Order=\"1\"", &format!("Order=\"{order}\"")),
                "tampered legacy dative order",
                Some(&source)
            )
            .is_err()
        );
    }
    for order in ["2", "3", "1.5", "hydrogen", "Dative"] {
        assert!(
            compare_xml_with_source(
                &actual.replace("Order=\"dative\"", &format!("Order=\"{order}\"")),
                expected,
                "tampered actual dative order",
                Some(&source)
            )
            .is_err()
        );
    }
    for order in [0, 1, 2, 3, 4, 6, 7] {
        let mut changed = source.clone();
        changed.bonds[0].order = order;
        assert!(
            compare_xml_with_source(&actual, expected, "non-dative source", Some(&changed))
                .is_err()
        );
    }
    for display in ["plain", "bold", "dotted", "wedge"] {
        let mut changed = source.clone();
        changed.bonds[0].display = display.into();
        assert!(
            compare_xml_with_source(&actual, expected, "non-dashed source", Some(&changed))
                .is_err()
        );
    }
    for reversed in [false, true] {
        let mut changed = source.clone();
        let mut duplicate = changed.bonds[0].clone();
        if reversed {
            std::mem::swap(&mut duplicate.a, &mut duplicate.b);
        }
        changed.bonds.push(duplicate);
        assert!(
            compare_xml_with_source(&actual, expected, "duplicate source bond", Some(&changed))
                .is_err()
        );
    }
    for mutation in 0..4 {
        let mut changed = source.clone();
        match mutation {
            0 => changed.atoms[0].id = changed.atoms[1].id,
            1 => changed.atoms[0].id = 0,
            2 => changed.bonds[0].a = 17,
            _ => changed.bonds[0].b = changed.bonds[0].a,
        }
        assert!(
            compare_xml_with_source(&actual, expected, "missing source identity", Some(&changed))
                .is_err()
        );
    }
    for extra in [
        r#"<n id="3" Element="7"/>"#,
        r#"<b id="5" B="3" E="4" Order="1" Display="Dash"/>"#,
    ] {
        assert!(
            compare_xml_with_source(
                &actual.replace("</fragment>", &format!("{extra}</fragment>")),
                &expected.replace("</fragment>", &format!("{extra}</fragment>")),
                "ambiguous output identity",
                Some(&source)
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn dative_export_order_allowance_uses_source_index_and_direction() -> anyhow::Result<()> {
    let expected = r#"<CDXML><page><fragment><n id="5" Element="29"/><n id="3" Element="7"/><n id="4" Element="6"/><b id="7" B="3" E="5" Order="1" Display="Dash"/><b id="6" B="3" E="4" Order="1" Display="Solid"/></fragment></page></CDXML>"#;
    let actual = expected.replace("E=\"5\" Order=\"1\"", "E=\"5\" Order=\"dative\"");
    let mut source = Document::default();
    let n = source.add_atom("N", Point::default());
    let c = source.add_atom("C", Point::new(28., 0.));
    let cu = source.add_atom("Cu", Point::new(56., 0.));
    source.add_bond(n, c, 1, "plain");
    source.add_bond(n, cu, 5, "dashed");
    compare_xml_with_source(&actual, expected, "source bond index", Some(&source))?;
    let mut changed = source.clone();
    changed.bonds.reverse();
    assert!(
        compare_xml_with_source(&actual, expected, "changed source order", Some(&changed)).is_err()
    );
    let bond = &mut source.bonds[1];
    std::mem::swap(&mut bond.a, &mut bond.b);
    assert!(
        compare_xml_with_source(&actual, expected, "changed source direction", Some(&source))
            .is_err()
    );
    Ok(())
}

#[test]
fn editable_drawing_output_matches_original_writer() -> anyhow::Result<()> {
    let mut oracle = Oracle::new()?;
    let mut docs = vec![("empty".to_owned(), Document::default())];
    for text in [
        "CCO",
        "c1ccccc1",
        "C[C@H](N)C(=O)O",
        "[13CH3:90][NH3+]",
        "[2H]O[3H]",
        "[CH3]",
        "N->[Cu+2]",
    ] {
        let result = oracle.request(&Request::import_smiles(text))?;
        docs.push((
            text.into(),
            serde_json::from_value(result["document"].clone())?,
        ));
    }
    for entry in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("cdxml") {
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        let result = oracle.request(&Request::import("cdxml", &text))?;
        anyhow::ensure!(
            result.get("error").is_none(),
            "Fixture {}: {result}",
            path.display()
        );
        docs.push((
            path.display().to_string(),
            serde_json::from_value(result["document"].clone())?,
        ));
    }
    let mut text = Document::default();
    text.annotations.push(Annotation {
        id: 1,
        position: Point::new(-37.125, 98.3),
        text: "試料 α & <H₂O>\nLine two".into(),
        format: Default::default(),
    });
    docs.push(("Unicode caption".into(), text));
    docs.extend(
        reshiki::templates::LIBRARY
            .iter()
            // The original writer predates Haworth semantics and explicit
            // contracted-group alignment. Those templates are checked against
            // real ChemDraw exports in haworth_interchange.rs instead.
            .filter(|t| t.group != "Carbohydrates")
            .map(|t| (format!("template/{}", t.id), t.document.clone())),
    );
    use reshiki::{
        arrows::{ArrowStyle, Head, HeadShape, Preset},
        graphics::{Graphic, GraphicKind, GraphicStyle, LinePattern},
        scientific::{OrbitalKind, Phase, SymbolKind},
        typography::{Script, TextAlign, TextSpan, TextStyle},
    };
    for preset in Preset::ALL {
        for head in Head::ALL {
            for tail in Head::ALL {
                for shape in HeadShape::ALL {
                    for pattern in [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted] {
                        let mut doc = Document::default();
                        doc.arrows.push(reshiki::document::Arrow::new(
                            1,
                            Point::new(-12.25, 10.),
                            Point::new(165.4, 87.2),
                            *preset,
                            ArrowStyle {
                                head: *head,
                                tail: *tail,
                                shape: *shape,
                                pattern,
                                ..ArrowStyle::preset(*preset)
                            },
                        ));
                        docs.push((
                            format!("arrow/{preset}/{head}/{tail}/{shape}/{pattern}"),
                            doc,
                        ));
                    }
                }
            }
        }
    }
    let kinds = GraphicKind::DRAWABLE
        .iter()
        .copied()
        .chain(SymbolKind::ALL.iter().copied().map(GraphicKind::Symbol))
        .chain(OrbitalKind::ALL.iter().copied().map(GraphicKind::Orbital));
    for kind in kinds {
        for phase in Phase::ALL {
            for pattern in [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted] {
                for fill in [None, Some(reshiki::palette::Color::Custom([19, 170, 82]))] {
                    let mut doc = Document::default();
                    let mut graphic = Graphic::dragged(
                        1,
                        kind,
                        Point::new(-73.12, 92.3),
                        Point::new(117.8, 215.),
                        GraphicStyle {
                            pattern,
                            fill,
                            stroke: reshiki::palette::Color::Custom([74, 112, 239]),
                            width_pt: 0.83,
                        },
                        Default::default(),
                        false,
                    );
                    graphic.phase = *phase;
                    graphic.phase_flipped = true;
                    graphic.layer = 3;
                    doc.graphics.push(graphic);
                    reshiki::editing::transform(
                        &mut doc,
                        &[1],
                        reshiki::editing::Transform::Rotate(23.5),
                    );
                    docs.push((format!("graphic/{kind}/{phase}/{pattern}/{fill:?}"), doc));
                }
            }
        }
    }
    for alignment in [
        TextAlign::Left,
        TextAlign::Center,
        TextAlign::Right,
        TextAlign::Justified,
    ] {
        let mut doc = Document::default();
        doc.annotations.push(Annotation {
            id: 1,
            position: Point::new(2., 3.),
            text: "試料 H2O\nSecond line".into(),
            format: reshiki::typography::TextFormat {
                alignment,
                width_pt: Some(83.5),
                line_spacing: 1.37,
                name_breaks: false,
                style: TextStyle {
                    family: "Hiragino Sans".into(),
                    color: reshiki::palette::Color::Custom([145, 60, 220]),
                    size_pt: 13.,
                    ..Default::default()
                },
                spans: vec![
                    TextSpan {
                        start: 0,
                        end: 6,
                        style: TextStyle {
                            bold: true,
                            italic: true,
                            underline: true,
                            ..Default::default()
                        },
                    },
                    TextSpan {
                        start: 8,
                        end: 9,
                        style: TextStyle {
                            script: Script::Subscript,
                            ..Default::default()
                        },
                    },
                ],
            },
        });
        docs.push((format!("rich text/{alignment:?}"), doc));
    }
    let originals = docs.iter().take(24).cloned().collect::<Vec<_>>();
    for (name, mut doc) in originals {
        let all = doc.all_ids();
        reshiki::editing::transform(&mut doc, &all, reshiki::editing::Transform::Rotate(37.25));
        reshiki::editing::transform(&mut doc, &all, reshiki::editing::Transform::FlipHorizontal);
        for (i, b) in doc.bonds.iter_mut().enumerate() {
            b.z_order = i as i16;
            b.color = reshiki::palette::Color::Custom([120, 25, 90]);
        }
        doc.atom_labels.carbons = reshiki::atom_labels::Carbons::All;
        doc.atom_labels.stereo = true;
        docs.push((format!("transformed/{name}"), doc));
    }
    docs.extend(cases::documents()?);
    let (mut accepted, mut rejected, mut exact, mut invalid_inputs) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for (name, doc) in docs {
        let request = Request::molecule("export", doc.clone());
        // Both engine backends validate the Rust document before dispatch.
        // The direct Python function alone accepts some forbidden displays.
        if doc.validate().is_err() {
            if drawing::write(&doc, drawing::Options::from(&request)).is_ok() {
                failures.push(format!(
                    "{name}: [validation] invalid source document was accepted"
                ));
            }
            invalid_inputs += 1;
            continue;
        }
        let expected = oracle.request(&request)?;
        let actual = drawing::write(&doc, drawing::Options::from(&request));
        match (actual, expected.get("output").and_then(Value::as_str)) {
            (Ok(actual), Some(expected)) => {
                if let Err(error) = compare_xml_with_source(&actual, expected, &name, Some(&doc)) {
                    failures.push(format!("{error:#}"));
                }
                exact += usize::from(actual == expected);
                accepted += 1;
            }
            (Err(_), None) => {
                rejected += 1;
            }
            (actual, expected) => failures.push(format!(
                "{name}: [outcome] Rust {actual:?}; reference {expected:?}"
            )),
        }
    }
    eprintln!(
        "Editable drawing comparisons: {accepted} accepted ({exact} byte-identical), {rejected} rejected; {invalid_inputs} invalid documents rejected"
    );
    assert!(accepted >= 400 && rejected >= 800);
    if !failures.is_empty() {
        let mut categories = std::collections::BTreeMap::<String, usize>::new();
        for failure in &failures {
            for line in failure.lines() {
                if let Some(suffix) = line.strip_prefix('[')
                    && let Some((category, _)) = suffix.split_once(']')
                {
                    *categories.entry(category.to_owned()).or_default() += 1;
                }
            }
        }
        std::fs::create_dir_all("artifacts")?;
        let diagnostic = format!(
            "{} failed comparisons; categories={categories:?}\n{}",
            failures.len(),
            failures.join("\n")
        );
        std::fs::write("artifacts/drawing-exchange-failures.txt", &diagnostic)?;
        anyhow::bail!("drawing-exchange: {diagnostic}");
    }
    Ok(())
}

#[test]
fn invalid_export_geometry_and_text_return_errors() -> anyhow::Result<()> {
    use reshiki::graphics::{Graphic, GraphicKind, PathCommand};
    let mut doc = Document::default();
    doc.graphics.push(Graphic::dragged(
        1,
        GraphicKind::Line,
        Point::default(),
        Point::new(42., 28.),
        Default::default(),
        Default::default(),
        false,
    ));
    assert!(drawing::write(&doc, drawing::Options::default()).is_err());
    for commands in [
        vec![PathCommand::Line(Point::new(42., 28.))],
        vec![PathCommand::Move(Point::default())],
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Line(Point::new(f32::NAN, 0.)),
        ],
    ] {
        let paths = std::collections::HashMap::from([(1, commands)]);
        assert!(
            drawing::write(
                &doc,
                drawing::Options {
                    graphic_paths: Some(&paths),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    let mut text = Document::default();
    text.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "invalid\0text".into(),
        format: Default::default(),
    });
    assert!(drawing::write(&text, drawing::Options::default()).is_err());
    text.annotations
        .first_mut()
        .context("Missing caption")?
        .text = "valid".into();
    let metrics = std::collections::HashMap::from([(
        1,
        reshiki::engine::TextMetrics {
            width: f32::NAN,
            height: 10.,
            baseline: 9.,
        },
    )]);
    assert!(
        drawing::write(
            &text,
            drawing::Options {
                text_layout: Some(&metrics),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(drawing::write(&text, drawing::Options::default())?.contains("valid"));
    Ok(())
}
