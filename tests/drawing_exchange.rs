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
            .arg(root.join("tests/drawing_exchange_reference.py"))
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
    let a = roxmltree::Document::parse(actual)?;
    let e = roxmltree::Document::parse(expected)?;
    let a: Vec<_> = a.descendants().filter(|n| n.is_element()).collect();
    let e: Vec<_> = e.descendants().filter(|n| n.is_element()).collect();
    assert_eq!(a.len(), e.len(), "{name}: object count");
    for (a, e) in a.iter().zip(e.iter()) {
        assert_eq!(a.tag_name().name(), e.tag_name().name(), "{name}: tag");
        assert_eq!(
            a.attributes().len(),
            e.attributes().len(),
            "{name}: {:?} attributes",
            e.tag_name()
        );
        for attr in e.attributes() {
            let value = a
                .attribute(attr.name())
                .context(format!("{name}: missing {}", attr.name()))?;
            if value == attr.value() {
                continue;
            }
            if attr.name() == "PNG" {
                let pixels = |s: &str| -> anyhow::Result<image::RgbaImage> {
                    let bytes = s
                        .as_bytes()
                        .chunks_exact(2)
                        .map(|p| {
                            u8::from_str_radix(std::str::from_utf8(p)?, 16)
                                .map_err(anyhow::Error::from)
                        })
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(image::load_from_memory(&bytes)?.to_rgba8())
                };
                assert_eq!(
                    pixels(value)?,
                    pixels(attr.value())?,
                    "{name}: picture pixels"
                );
                continue;
            }
            // Geometry has six decimal places. Other numeric attributes must
            // be exactly equal after parsing; references and labels are text.
            let geometry = matches!(
                attr.name(),
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
                    attr.name(),
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
                        | "r"
                        | "g"
                        | "b"
                );
            if !numeric {
                assert_eq!(value, attr.value(), "{name}: {}", attr.name());
                continue;
            }
            let values = |s: &str| {
                s.split_whitespace()
                    .map(str::parse::<f64>)
                    .collect::<Result<Vec<_>, _>>()
            };
            if let (Ok(av), Ok(ev)) = (values(value), values(attr.value())) {
                assert_eq!(av.len(), ev.len(), "{name}: {}", attr.name());
                for (a, e) in av.into_iter().zip(ev) {
                    assert!(
                        (a - e).abs() <= if geometry { 0.0000011 } else { 0. },
                        "{name}: {}: {a} != {e}",
                        attr.name()
                    );
                }
            } else {
                assert_eq!(value, attr.value(), "{name}: {}", attr.name());
            }
        }
        let text = |n: &roxmltree::Node<'_, '_>| {
            n.children()
                .filter(|c| c.is_text())
                .filter_map(|c| c.text())
                .collect::<String>()
        };
        assert_eq!(text(a), text(e), "{name}: text");
        assert_eq!(
            a.children().filter(|c| c.is_element()).count(),
            e.children().filter(|c| c.is_element()).count(),
            "{name}: children"
        );
    }
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
                for fill in [None, Some([19, 170, 82])] {
                    let mut doc = Document::default();
                    let mut graphic = Graphic::dragged(
                        1,
                        kind,
                        Point::new(-73.12, 92.3),
                        Point::new(117.8, 215.),
                        GraphicStyle {
                            pattern,
                            fill,
                            stroke: [74, 112, 239],
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
                style: TextStyle {
                    family: "Hiragino Sans".into(),
                    color: [145, 60, 220],
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
            b.color = [120, 25, 90];
        }
        doc.atom_labels.carbons = reshiki::atom_labels::Carbons::All;
        doc.atom_labels.stereo = true;
        docs.push((format!("transformed/{name}"), doc));
    }
    docs.extend(cases::documents()?);
    let (mut accepted, mut rejected, mut exact, mut invalid_inputs) = (0, 0, 0, 0);
    for (name, doc) in docs {
        let request = Request::molecule("export", doc.clone());
        // Both engine backends validate the Rust document before dispatch.
        // The direct Python function alone accepts some forbidden displays.
        if doc.validate().is_err() {
            assert!(
                drawing::write(&doc, drawing::Options::from(&request)).is_err(),
                "{name}"
            );
            invalid_inputs += 1;
            continue;
        }
        let expected = oracle.request(&request)?;
        let actual = drawing::write(&doc, drawing::Options::from(&request));
        match (actual, expected.get("output").and_then(Value::as_str)) {
            (Ok(actual), Some(expected)) => {
                compare_xml(&actual, expected, &name)?;
                exact += usize::from(actual == expected);
                accepted += 1;
            }
            (Err(_), None) => {
                rejected += 1;
            }
            (actual, expected) => anyhow::bail!(
                "{name}: Rust {actual:?}; expected {expected:?}; reference {expected:?}"
            ),
        }
    }
    eprintln!(
        "Editable drawing comparisons: {accepted} accepted ({exact} byte-identical), {rejected} rejected; {invalid_inputs} invalid documents rejected"
    );
    assert!(accepted >= 400 && rejected >= 800);
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
