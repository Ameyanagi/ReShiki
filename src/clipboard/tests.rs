use super::*;

#[tokio::test]
async fn embedded_chemdraw_payload_imports_editable_chemistry_and_rejects_corruption() {
    let cdx = include_bytes!("../../tests/fixtures/native-ethyl-clipboard.cdx");
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let preview = Representation::new("public.png", png.get_ref());
    let picture = paste_packet(
        LocalEngine::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![preview.clone()],
        },
    )
    .await
    .unwrap();
    assert!(picture.atoms.is_empty());
    assert!(picture.bonds.is_empty());
    assert_eq!(
        picture
            .graphics
            .iter()
            .filter(|graphic| graphic.picture.is_some())
            .count(),
        1
    );
    let packet = |data: &[u8]| Packet {
        generation: Some("test".into()),
        representations: vec![
            Representation::new("com.revvity.chemdraw.cdx-clipboard", data),
            preview.clone(),
        ],
    };
    let drawing = paste_packet(LocalEngine::default(), packet(cdx))
        .await
        .unwrap();
    assert_eq!(drawing.atoms.len(), 11);
    assert_eq!(drawing.bonds.len(), 11);
    assert_eq!(
        drawing
            .atoms
            .iter()
            .filter(|atom| atom.element == "C")
            .count(),
        9
    );
    assert_eq!(
        drawing
            .atoms
            .iter()
            .filter(|atom| atom.element == "O")
            .count(),
        2
    );
    assert!(
        drawing
            .graphics
            .iter()
            .all(|graphic| graphic.picture.is_none())
    );
    drawing.validate().unwrap();

    let fallback = paste_packet(LocalEngine::default(), packet(&cdx[..32]))
        .await
        .unwrap();
    assert!(fallback.atoms.is_empty());
    assert_eq!(fallback.graphics.len(), 1);
}

#[derive(Serialize, Deserialize)]
struct LegacyRepresentation {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}
#[derive(Serialize)]
struct LegacyRequest<'a> {
    operation: &'static str,
    representations: &'a [LegacyRepresentation],
}

#[test]
fn cdx_aliases_share_payload_without_changing_serialized_protocol() {
    let bytes = b"CDX\0exact binary\xffpayload";
    let encoded: Arc<String> = STANDARD.encode(bytes).into();
    let representations: Vec<_> = Representation::aliases(&CDX_TYPES, encoded).collect();
    assert_eq!(
        representations
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>(),
        CDX_TYPES
    );
    for item in &representations {
        assert!(Arc::ptr_eq(&representations[0].data, &item.data));
        assert_eq!(item.bytes().unwrap(), bytes);
    }
    let legacy: Vec<_> = CDX_TYPES
        .iter()
        .map(|kind| LegacyRepresentation {
            kind: (*kind).into(),
            data: STANDARD.encode(bytes),
        })
        .collect();
    let packet = encode_request("write", &representations).unwrap();
    assert_eq!(
        packet,
        serde_json::to_vec(&LegacyRequest {
            operation: "write",
            representations: &legacy,
        })
        .unwrap()
    );
    let decoded: Packet = serde_json::from_slice(&packet).unwrap();
    for (before, after) in representations.iter().zip(decoded.representations) {
        assert_eq!(before.kind, after.kind);
        assert_eq!(before.bytes().unwrap(), after.bytes().unwrap());
    }
}

#[test]
#[ignore = "process-wide allocation counters: run alone with --test-threads=1 --nocapture"]
fn cdx_alias_allocation_metrics() {
    use crate::allocation_metrics;
    use std::time::Instant;
    fn measure<T>(create: impl FnOnce() -> T) -> (T, [usize; 4], std::time::Duration) {
        let baseline = allocation_metrics::reset();
        let started = Instant::now();
        let output = create();
        let elapsed = started.elapsed();
        let measured = allocation_metrics::snapshot();
        (
            output,
            [
                measured.live_bytes - baseline,
                measured.peak_bytes - baseline,
                measured.allocated_bytes,
                measured.allocation_count,
            ],
            elapsed,
        )
    }
    let bytes = vec![0xab; 1024 * 1024];
    let encoded = STANDARD.encode(&bytes);
    let (old_editable, old_editable_cost, old_editable_time) = measure(|| {
        let data = encoded.clone();
        CDX_TYPES
            .iter()
            .map(|kind| LegacyRepresentation {
                kind: (*kind).into(),
                data: data.clone(),
            })
            .collect::<Vec<_>>()
    });
    let (shared_editable, shared_editable_cost, shared_editable_time) =
        measure(|| Representation::aliases(&CDX_TYPES, encoded.clone().into()).collect::<Vec<_>>());
    let (old_image, old_image_cost, old_image_time) = measure(|| {
        CDX_TYPES
            .iter()
            .map(|kind| LegacyRepresentation {
                kind: (*kind).into(),
                data: STANDARD.encode(&bytes),
            })
            .collect::<Vec<_>>()
    });
    let (shared_image, shared_image_cost, shared_image_time) = measure(|| {
        Representation::aliases(&CDX_TYPES, STANDARD.encode(&bytes).into()).collect::<Vec<_>>()
    });
    let single_wire = serde_json::to_vec(&LegacyRepresentation {
        kind: CDX_TYPES[0].into(),
        data: encoded.clone(),
    })
    .unwrap();
    let (old_read, old_read_cost, old_read_time) =
        measure(|| serde_json::from_slice::<LegacyRepresentation>(&single_wire).unwrap());
    let (shared_read, shared_read_cost, shared_read_time) =
        measure(|| serde_json::from_slice::<Representation>(&single_wire).unwrap());
    assert_eq!(old_read.data, shared_read.data.as_str());
    assert!(shared_read_cost[2].saturating_sub(old_read_cost[2]) < 1024);
    let packet = encode_request("write", &shared_editable).unwrap();
    assert_eq!(packet, encode_request("write", &shared_image).unwrap());
    for legacy in [&old_editable, &old_image] {
        assert_eq!(
            packet,
            serde_json::to_vec(&LegacyRequest {
                operation: "write",
                representations: legacy,
            })
            .unwrap()
        );
    }
    for (old, shared) in [
        (old_editable_cost, shared_editable_cost),
        (old_image_cost, shared_image_cost),
    ] {
        assert!(shared[0] < old[0], "retained bytes: {shared:?} vs {old:?}");
        assert!(shared[1] < old[1], "peak bytes: {shared:?} vs {old:?}");
        assert!(shared[2] < old[2], "allocated bytes: {shared:?} vs {old:?}");
    }
    println!(
        "raw={} encoded={} alias_payloads={} wire={}",
        bytes.len(),
        encoded.len(),
        3 * encoded.len(),
        packet.len()
    );
    for (name, cost, elapsed, encodes) in [
        ("editable-old", old_editable_cost, old_editable_time, 0),
        (
            "editable-shared",
            shared_editable_cost,
            shared_editable_time,
            0,
        ),
        ("image-old", old_image_cost, old_image_time, 3),
        ("image-shared", shared_image_cost, shared_image_time, 1),
        ("single-read-old", old_read_cost, old_read_time, 0),
        ("single-read-shared", shared_read_cost, shared_read_time, 0),
    ] {
        println!(
            "{name}: retained={} peak={} allocated={} allocations={} encodes={encodes} generation={elapsed:?}",
            cost[0], cost[1], cost[2], cost[3]
        );
    }
}

#[test]
fn typed_text_formats_follow_cheap_markers() {
    for (text, format) in [
        ("  InChI=1S/C2H6O/c1-2-3/h3H,2H2,1H3", "inchi"),
        ("$RXN\n\n  ReShiki\n\n  1  1\n$MOL\nM  END", "rxn"),
        (
            "ethanol\n\n\n  3  2  0  0  0  0  0  0  0  0999 V2000\nM  END",
            "mol",
        ),
        ("\n\n\n  0  0  0     0  0            999 V3000\n", "mol"),
        ("<?xml version=\"1.0\"?><CDXML><page/></CDXML>", "cdxml"),
        ("CCO>>CC=O", "rsmi"),
        ("CCO>O=O>CC=O", "rsmi"),
        ("C->C", "smiles"),
        ("c1ccccc1", "smiles"),
    ] {
        assert_eq!(text_format(text), format, "{text}");
        assert_eq!(text_request(text).format.as_deref(), Some(format));
    }
}

#[tokio::test]
async fn both_canvas_modes_copy_visible_ink_without_background_objects() {
    use crate::{canvas_theme::CanvasTheme, document::Point};
    for theme in CanvasTheme::ALL {
        let mut doc = Document {
            canvas_theme: theme,
            ..Default::default()
        };
        let c = doc.add_atom("C", Point::default());
        let o = doc.add_atom("O", Point::new(42., 0.));
        doc.add_bond(c, o, 2, "plain");
        let (outcome, representations) = prepare_copy(Default::default(), doc.clone(), false)
            .await
            .unwrap();
        assert!(outcome.external_editable, "{:?}", outcome.notices);
        assert!(representations.iter().all(|item| !matches!(
            item.kind.as_str(),
            "public.utf8-plain-text" | "org.opensmiles.smiles"
        )));
        let native = representations.iter().find(|r| r.kind == NATIVE).unwrap();
        let original: Document = serde_json::from_slice(&native.bytes().unwrap()).unwrap();
        assert_eq!(original.canvas_theme, theme);
        assert_eq!(original.drawing_style, doc.drawing_style);
        assert!(original.graphics.is_empty());
        let binary = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .unwrap()
            .clone();
        let back = paste_packet(
            Default::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![binary],
            },
        )
        .await
        .unwrap();
        assert_eq!(back.atoms.len(), 2);
        assert_eq!(back.bonds.len(), 1);
        assert!(
            back.graphics.is_empty(),
            "Editable copies must not add a canvas rectangle"
        );
        assert_eq!(
            back.bonds[0].color,
            crate::palette::Color::imported(theme.color([0; 3]))
        );
        for (_, image) in copy_images(&doc, true, false) {
            let image = image.unwrap();
            if image.kind == "public.png" {
                let raster = image::load_from_memory(&image.bytes().unwrap())
                    .unwrap()
                    .into_rgba8();
                assert_eq!(raster.get_pixel(0, 0)[3], 0);
                let ink = theme.color([0; 3]);
                assert!(
                    raster
                        .pixels()
                        .any(|p| p.0 == [ink[0], ink[1], ink[2], 255])
                );
            } else if image.kind == "public.svg-image" {
                let bytes = image.bytes().unwrap();
                let tree =
                    roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
                assert!(!tree.descendants().any(|n| n.has_tag_name("rect")));
            }
        }
    }
}

#[tokio::test]
async fn internal_condensed_labels_keep_editable_text_and_formula_formatting() -> anyhow::Result<()>
{
    use anyhow::{Context, ensure};
    let source: Document = serde_json::from_str(include_str!(
        "../../tests/fixtures/internal-labels/internal-labels.rsk"
    ))?;
    let (outcome, representations) = prepare_copy(Default::default(), source, false)
        .await
        .map_err(anyhow::Error::msg)?;
    ensure!(outcome.external_editable && !outcome.image_only);
    let binary = representations
        .iter()
        .find(|r| r.kind == CDX_TYPES[0])
        .context("CDX")?;
    let xml = crate::exchange::from_cdx(&binary.bytes().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let tree = roxmltree::Document::parse(&xml)?;
    ensure!(!tree.descendants().any(|n| n.has_tag_name("embeddedobject")));
    for label in ["CCl2", "CF2", "NMe"] {
        let runs: Vec<_> = tree
            .descendants()
            .filter(|n| n.has_tag_name("s") && n.text() == Some(label))
            .collect();
        ensure!(runs.len() == 4, "Missing {label} orientations");
        ensure!(
            runs.iter().all(|n| n
                .attribute("face")
                .and_then(|s| s.parse::<u8>().ok())
                .is_some_and(|f| f & 96 == 96)),
            "{label} lost formula typography"
        );
    }
    for (format, data) in [("cdx", binary.data.as_str()), ("cdxml", xml.as_str())] {
        let back = LocalEngine::default()
            .request(Request::import(format, data))
            .await
            .map_err(anyhow::Error::msg)?
            .document
            .context("Imported drawing")?;
        ensure!(back.atoms.len() == 60 && back.bonds.len() == 40 && back.graphics.is_empty());
        for label in ["CCl2", "CF2", "NMe"] {
            ensure!(
                back.atoms
                    .iter()
                    .filter(|a| a.display.variable.as_deref() == Some(label))
                    .count()
                    == 4,
                "{format} lost {label}: {:?}",
                back.atoms
                    .iter()
                    .filter(|a| a.display.variable.is_some() || a.element == "*")
                    .map(|a| (&a.element, &a.display.variable))
                    .collect::<Vec<_>>()
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn projected_arene_with_bold_edge_has_editable_clipboard_and_native_depth()
-> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    let source: Document = serde_json::from_str(include_str!(
        "../../tests/fixtures/bond-joins/arene-bold-join.rsk"
    ))?;
    let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
        .await
        .map_err(anyhow::Error::msg)?;
    ensure!(
        outcome.external_editable && outcome.notices.is_empty(),
        "{:?}",
        outcome.notices
    );
    let native = representations
        .iter()
        .find(|r| r.kind == NATIVE)
        .context("Native drawing")?;
    let native = Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    for (a, b) in source.atoms.iter().zip(&native.atoms) {
        assert_eq!((a.position, a.depth), (b.position, b.depth));
    }
    assert_eq!(source.bonds, native.bonds);
    let binary = representations
        .iter()
        .find(|r| r.kind == CDX_TYPES[0])
        .context("Binary drawing")?;
    let back = paste_packet(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![binary.clone()],
        },
    )
    .await
    .map_err(anyhow::Error::msg)?;
    ensure!(back.atoms.len() == 7 && back.bonds.len() == 7 && back.graphics.is_empty());
    ensure!(back.atoms.iter().all(|a| a.stereo.is_none()));
    Ok(())
}

#[tokio::test]
async fn projected_double_bonds_keep_bold_rails_in_editable_clipboard() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    let mut source: Document = serde_json::from_str(include_str!(
        "../../tests/fixtures/tilted-fused-double-bonds.rsk"
    ))?;
    let ids = source.all_ids();
    crate::projection::depth_bonds(&mut source, &ids);
    let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
        .await
        .map_err(anyhow::Error::msg)?;
    ensure!(outcome.external_editable && !outcome.image_only);
    let native = representations
        .iter()
        .find(|r| r.kind == NATIVE)
        .context("Native drawing")?;
    assert_eq!(
        Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
            .map_err(anyhow::Error::msg)?,
        source.current()
    );
    let binary = representations
        .iter()
        .find(|r| r.kind == CDX_TYPES[0])
        .context("CDX drawing")?;
    let back = paste_packet(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![binary.clone()],
        },
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert_eq!(
        (back.atoms.len(), back.bonds.len()),
        (source.atoms.len(), source.bonds.len())
    );
    for (before, after) in source
        .bonds
        .iter()
        .zip(&back.bonds)
        .filter(|(b, _)| b.order == 2)
    {
        assert_eq!(
            (
                before.order,
                &before.display,
                before
                    .secondary_display
                    .as_deref()
                    .unwrap_or(&before.display)
            ),
            (
                after.order,
                &after.display,
                after.secondary_display.as_deref().unwrap_or(&after.display)
            )
        );
    }
    ensure!(back.atoms.iter().all(|a| a.stereo.is_none()));
    Ok(())
}

#[tokio::test]
async fn copying_unvalidated_rings_keeps_editable_exchange_and_paste_warning() -> anyhow::Result<()>
{
    use anyhow::{Context, ensure};
    let source = Representation::new(
        CDX_TYPES[0],
        include_bytes!("../../tests/fixtures/aromatic-five-attachment.cdx"),
    );
    let pasted = paste_packet_with_warnings(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![source],
        },
        false,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    ensure!(!pasted.warnings.is_empty());
    let (outcome, representations) =
        prepare_copy(Default::default(), pasted.document.clone(), false)
            .await
            .map_err(anyhow::Error::msg)?;
    ensure!(outcome.external_editable && !outcome.image_only);
    let binary = representations
        .iter()
        .find(|r| r.kind == CDX_TYPES[0])
        .context("Missing editable drawing")?;
    let back = paste_packet_with_warnings(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![binary.clone()],
        },
        false,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    ensure!(back.document.atoms.len() == pasted.document.atoms.len());
    ensure!(!back.warnings.is_empty());
    Ok(())
}

#[tokio::test]
async fn simplified_appearance_keeps_editable_atoms_and_the_native_original() -> anyhow::Result<()>
{
    use anyhow::{Context, ensure};
    let mut doc = Document::default();
    let id = doc.add_atom("*", crate::document::Point::default());
    doc.atom_mut(id).context("Missing label")?.display.variable = Some("M".into());
    let mut cases = vec![doc];
    for (key, element) in [("j", "Fe"), ("J", "Ru")] {
        let mut source = Document::default();
        let id = source.add_atom(element, crate::document::Point::default());
        cases.push(
            crate::hotkeys::atom_edit(&source, id, key, 42.)
                .context("Ligand shortcut")?
                .map_err(anyhow::Error::msg)?
                .0,
        );
    }
    for doc in cases {
        let (outcome, representations) = prepare_copy(Default::default(), doc.clone(), false)
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(outcome.external_editable, "{:?}", outcome.notices);
        let native = representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .context("Missing native drawing")?;
        ensure!(
            Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?
                == doc.current()
        );
        let binary = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .context("Missing editable copy")?;
        let back = paste_packet(
            Default::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![binary.clone()],
            },
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(back.atoms.len() == doc.atoms.len() && back.bonds.len() == doc.bonds.len());
        ensure!(back.graphics.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn complete_shortcut_gallery_has_editable_cdx_without_altering_the_native_copy()
-> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    let source: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk"))?;
    let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
        .await
        .map_err(anyhow::Error::msg)?;
    ensure!(outcome.external_editable, "{:?}", outcome.notices);
    ensure!(
        outcome.notices.iter().any(|n| n.contains("variable label")),
        "Missing variable-label notice"
    );
    let native = representations
        .iter()
        .find(|r| r.kind == NATIVE)
        .context("Native drawing")?;
    ensure!(
        Document::from_json(&native.bytes().map_err(anyhow::Error::msg)?)
            .map_err(anyhow::Error::msg)?
            == source.current()
    );
    let cdx = representations
        .iter()
        .find(|r| r.kind == CDX_TYPES[0])
        .context("Editable CDX")?;
    let xml = crate::exchange::from_cdx(&cdx.bytes().map_err(anyhow::Error::msg)?)
        .map_err(anyhow::Error::msg)?;
    let tree = roxmltree::Document::parse(&xml)?;
    ensure!(
        !tree.descendants().any(|n| n.has_tag_name("embeddedobject")),
        "Gallery must not be flattened into a picture"
    );
    for label in ["R", "X"] {
        ensure!(tree.descendants().any(|n| {
            n.has_tag_name("n")
                && n.attribute("Element") == Some("0")
                && n.descendants()
                    .any(|s| s.has_tag_name("s") && s.text() == Some(label))
        }));
    }
    let back = paste_packet(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![cdx.clone()],
        },
    )
    .await
    .map_err(anyhow::Error::msg)?;
    ensure!(
        back.atoms.len() == source.atoms.len(),
        "Atom count: {} / {}",
        back.atoms.len(),
        source.atoms.len()
    );
    ensure!(back.bonds.len() == source.bonds.len());
    ensure!(back.annotations.len() == source.annotations.len());
    ensure!(
        back.atoms.iter().map(|a| a.charge).sum::<i32>()
            == source.atoms.iter().map(|a| a.charge).sum::<i32>()
    );
    // This native CDX self-round-trip keeps the complete current gallery,
    // including captions and pi ligands together. Genuine external
    // captures are covered separately in pi_ligand_exchange/chemdraw_captions.
    ensure!(back.atoms.iter().filter(|a| a.attachment.is_some()).count() == 5);
    ensure!(back.bonds.iter().filter(|b| b.order == 4).count() == 33);
    ensure!(
        back.atoms
            .iter()
            .filter(|a| a.charge == -1 && a.display.hide_charge)
            .count()
            == 3
    );
    ensure!(
        back.atoms
            .iter()
            .any(|a| a.element == "Fe" && a.charge == 2)
    );
    let mut expected_text: Vec<_> = source.annotations.iter().map(|a| &a.text).collect();
    let mut returned_text: Vec<_> = back.annotations.iter().map(|a| &a.text).collect();
    expected_text.sort();
    returned_text.sort();
    ensure!(returned_text == expected_text);
    for caption in &back.annotations {
        ensure!(
            source.annotations.iter().any(|original| {
                original.text == caption.text
                        && original.format.style == caption.format.style
                        // The binary format rounds to the nearest 0.05pt.
                        && (original.format.style.size_pt * original.format.line_spacing
                            - caption.format.style.size_pt * caption.format.line_spacing)
                            .abs()
                            <= 0.0251
            }),
            "Caption style or spacing changed: {}",
            caption.text
        );
    }
    Ok(())
}

#[tokio::test]
async fn previous_native_and_text_clipboards_remain_editable() -> anyhow::Result<()> {
    let json = include_str!("../../tests/fixtures/legacy-drawing.moruno");
    let expected: Document = serde_json::from_str(json)?;
    expected.validate().map_err(anyhow::Error::msg)?;
    for (kind, contents) in [
        ("dev.moruno.drawing", json.to_owned()),
        (NATIVE, json.to_owned()),
        (
            "public.utf8-plain-text",
            format!("MORUNO_DRAWING_V1\n{json}"),
        ),
        (
            "public.utf8-plain-text",
            format!("{}{json}", editing::CLIPBOARD_PREFIX),
        ),
    ] {
        let restored = paste_packet_with_warnings(
            LocalEngine::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![Representation::new(kind, contents.as_bytes())],
            },
            false,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        assert!(restored.native, "{kind}");
        assert_eq!(restored.document, expected);
    }
    Ok(())
}

#[tokio::test]
async fn old_dark_reshiki_clipboards_convert_colors_once() -> anyhow::Result<()> {
    use crate::{canvas_theme::CanvasTheme, document::Point, palette::Color};
    let mut doc = Document {
        canvas_theme: CanvasTheme::Dark,
        ..Default::default()
    };
    let c = doc.add_atom("C", Point::default());
    let o = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(c, o, 1, "plain");
    doc.bonds[0].color = Color::Custom([10, 120, 200]);
    assert!(doc.version < crate::document::VERSION);
    let (_, representations) = prepare_copy(Default::default(), doc, false)
        .await
        .map_err(anyhow::Error::msg)?;
    let copied = representations
        .iter()
        .find(|r| r.kind == NATIVE)
        .ok_or_else(|| anyhow::anyhow!("no native data"))?
        .bytes()
        .map_err(anyhow::Error::msg)?;
    // The previous release's dark drawing showed [10, 120, 200] flipped.
    let old = include_bytes!("../../tests/fixtures/palette/legacy-dark.rsk");
    for (data, bond, color) in [
        (copied.as_slice(), 0, [10, 120, 200]),
        (old.as_slice(), 1, [55, 165, 245]),
    ] {
        let pasted = paste_packet_with_warnings(
            LocalEngine::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![Representation::new(NATIVE, data)],
            },
            false,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        assert_eq!(pasted.document.bonds[bond].color, Color::Custom(color));
    }
    Ok(())
}

#[tokio::test]
async fn dark_depth_copy_materializes_external_paint_before_switching_paper() -> anyhow::Result<()>
{
    use crate::{canvas_theme::CanvasTheme, document::Point, palette::Color};
    use anyhow::{Context, ensure};
    for frozen in [false, true] {
        let mut source = Document {
            canvas_theme: CanvasTheme::Dark,
            ..Document::default()
        };
        for (i, element) in ["O", "C", "O"].into_iter().enumerate() {
            let id = source.add_atom(element, Point::new(i as f32 * 42., 0.));
            source.atom_mut(id).context("atom")?.depth = i as f32 * 20. - 20.;
            if i > 0 {
                source.add_bond(id - 1, id, 1, "plain");
            }
        }
        let ids = source.all_ids();
        crate::depth_appearance::enable(&mut source, &ids, 0.6).map_err(anyhow::Error::msg)?;
        if frozen {
            crate::depth_appearance::freeze(&mut source, &ids);
        }
        let before = source.clone();
        let (outcome, representations) = prepare_copy(Default::default(), source.clone(), false)
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(outcome.external_editable);
        let native_representation = representations
            .iter()
            .find(|r| r.kind == NATIVE)
            .context("native drawing")?;
        let native =
            Document::from_json(&native_representation.bytes().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?;
        assert_eq!(native.depth_appearance, source.depth_appearance);
        assert_eq!(native.canvas_theme, CanvasTheme::Dark);
        assert_eq!(
            native.bonds[0].color,
            Color::Ink,
            "native retains the editable base color"
        );
        let pasted = paste_packet_with_warnings(
            Default::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![native_representation.clone()],
            },
            false,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(pasted.native);
        assert_eq!(pasted.document.depth_appearance, source.depth_appearance);

        let cdx = representations
            .iter()
            .find(|r| r.kind == CDX_TYPES[0])
            .context("external CDX")?
            .bytes()
            .map_err(anyhow::Error::msg)?;
        let xml = crate::exchange::from_cdx(&cdx).map_err(anyhow::Error::msg)?;
        let external = crate::chemistry::cdxml::import_cdxml(&xml)?.document;
        let rear = external
            .atoms
            .iter()
            .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .context("rear label")?;
        assert_eq!(
            rear.text_style.as_ref().context("rear ink")?.color.rgb(),
            [102; 3],
            "{frozen}"
        );
        let rear_bond = external
            .bonds
            .iter()
            .find(|b| b.a == rear.id || b.b == rear.id)
            .context("rear bond")?;
        assert_eq!(rear_bond.color.rgb(), [140; 3], "{frozen}");
        assert!(external.depth_appearance.is_empty());
        assert_eq!(
            source, before,
            "copy keeps the source projection and scopes"
        );
    }
    Ok(())
}

#[tokio::test]
async fn only_reshiki_clipboards_keep_palette_references() -> anyhow::Result<()> {
    use crate::palette::{Color, Hue, Palette, Row};
    let mut doc = Document::default();
    let c = doc.add_atom("C", crate::document::Point::default());
    let n = doc.add_atom("N", crate::document::Point::new(42., 0.));
    let o = doc.add_atom("O", crate::document::Point::new(84., 0.));
    doc.add_bond(c, n, 1, "plain");
    doc.add_bond(n, o, 1, "plain");
    doc.bonds[0].color = Color::Palette(Hue::Red, Row::Strong);
    let red = Palette::of(&doc).rgb(doc.bonds[0].color);
    let native = serde_json::to_vec(&doc)?;
    let xml = crate::exchange::drawing::write(&doc, Default::default())?;
    for (kind, data, native) in [
        (NATIVE, native.as_slice(), true),
        ("com.perkinelmer.chemdraw.cdxml", xml.as_bytes(), false),
    ] {
        let pasted = paste_packet_with_warnings(
            LocalEngine::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![Representation::new(kind, data)],
            },
            false,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        assert_eq!(pasted.native, native, "{kind}");
        let colors: Vec<_> = pasted.document.bonds.iter().map(|b| b.color).collect();
        if native {
            assert_eq!(colors, [Color::Palette(Hue::Red, Row::Strong), Color::Ink]);
        } else {
            // ChemDraw data keeps exact colors; its black becomes Ink.
            assert!(colors.contains(&Color::Custom(red)), "{colors:?}");
            assert!(colors.contains(&Color::Ink), "{colors:?}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn raster_paste_and_copy_image_preserve_pixels_and_physical_size_without_a_clipboard_write() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let images = copy_images(&doc, true, false);
    #[cfg(windows)]
    {
        let svg = images
            .iter()
            .find(|(format, _)| *format == "svg")
            .unwrap()
            .1
            .as_ref()
            .unwrap()
            .bytes()
            .unwrap();
        let text = std::str::from_utf8(&svg).unwrap();
        assert!(!text.contains("<text"), "Office needs outlined labels");
        assert!(text.contains("<path"));
    }
    let native = images
        .iter()
        .find(|(format, _)| *format == "native picture")
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    let png = images
        .iter()
        .find(|(format, _)| *format == "png")
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    let restored = paste_packet(
        LocalEngine::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![native],
        },
    )
    .await
    .unwrap();
    let raster = paste_packet(
        LocalEngine::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![png],
        },
    )
    .await
    .unwrap();
    assert!(restored.atoms.is_empty());
    assert_eq!(restored.graphics.len(), 1);
    assert_eq!(raster.graphics[0].picture, restored.graphics[0].picture);
    assert!((raster.graphics[0].axis_x.x - restored.graphics[0].axis_x.x).abs() < 0.01);
    assert!((raster.graphics[0].axis_y.y - restored.graphics[0].axis_y.y).abs() < 0.01);
    let invalid = Representation::new("public.png", b"not a PNG");
    assert!(
        paste_packet(
            LocalEngine::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![invalid]
            }
        )
        .await
        .is_err()
    );
    for (kind, format) in [
        ("public.jpeg", image::ImageFormat::Jpeg),
        ("public.tiff", image::ImageFormat::Tiff),
        ("org.webmproject.webp", image::ImageFormat::WebP),
    ] {
        let mut data = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(12, 8)
            .write_to(&mut data, format)
            .unwrap();
        let result = paste_packet(
            LocalEngine::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![Representation::new(kind, &data.into_inner())],
            },
        )
        .await
        .unwrap();
        let picture = result.graphics[0].picture.as_ref().unwrap();
        assert_eq!((picture.width(), picture.height()), (12, 8));
    }
}

#[test]
fn mol_line_framing_preserves_empty_title_and_refuses_truncation() {
    let text = "\n  example\n\n  0  0\nM  END\n";
    let data: Vec<u8> = text
        .lines()
        .flat_map(|line| std::iter::once(line.len() as u8).chain(line.bytes()))
        .collect();
    assert_eq!(mol_text(&data).unwrap(), text);
    assert_eq!(mol_text(text.as_bytes()).unwrap(), text);
    assert!(mol_text(&[10, b'x']).is_err());
    assert!(mol_text(&[1, 255]).is_err());
}

#[test]
fn raster_clipboard_object_preserves_bytes_and_publication_size() {
    let doc: Document = serde_json::from_str(include_str!(
        "../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let png = export::drawing(&doc, "png").unwrap();
    let drawing = embedded_png(&png).unwrap();
    // Independent inspection of the tagged stream: image-only object,
    // physical bounding box, original PNG bytes, and balanced terminators.
    assert_eq!(
        &drawing[..22],
        b"VjCD0100\x04\x03\x02\x01\0\0\0\0\0\0\0\0\0\0"
    );
    assert_eq!(
        u16::from_le_bytes(drawing[54..56].try_into().unwrap()),
        0x8009
    );
    assert_eq!(
        u16::from_le_bytes(drawing[60..62].try_into().unwrap()),
        0x0204
    );
    let right = i32::from_le_bytes(drawing[76..80].try_into().unwrap()) as f64 / 65536.0;
    let reader = png::Decoder::new(std::io::Cursor::new(&png))
        .read_info()
        .unwrap();
    let expected_width = f64::from(reader.info().width) * 72.0
        / (f64::from(reader.info().pixel_dims.unwrap().xppu) * 0.0254);
    assert!((right - 30.0 - expected_width).abs() < 0.0001);
    assert_eq!(
        u16::from_le_bytes(drawing[80..82].try_into().unwrap()),
        0x0a70
    );
    let start = if png.len() < 65535 { 84 } else { 88 };
    assert_eq!(&drawing[start..drawing.len() - 8], &png);
    assert_eq!(&drawing[drawing.len() - 8..], &[0; 8]);
    assert!(embedded_png(b"invalid image").is_err());
    assert!(embedded_png(&png[..20]).is_err());
    assert!(embedded_png(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
}

#[test]
fn adaptive_raster_wrappers_keep_publication_size_from_actual_resolution() {
    for dpi in [1200, 600, 300, 150, 96, 72] {
        // The same one-inch figure at each of the preview resolutions.
        let (width, height) = (dpi, dpi / 2);
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: (f64::from(dpi) / 0.0254).round() as u32,
            yppu: (f64::from(dpi) / 0.0254).round() as u32,
            unit: png::Unit::Meter,
        }));
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![0; width as usize * height as usize * 4])
            .unwrap();
        let drawing = embedded_png(&bytes).unwrap();
        let coordinate = |offset| {
            f64::from(i32::from_le_bytes(
                drawing[offset..offset + 4].try_into().unwrap(),
            )) / 65536.
        };
        let (cdx_width, cdx_height) = (
            coordinate(76) - coordinate(68),
            coordinate(72) - coordinate(64),
        );
        let native = crate::pictures::clipboard_document(&bytes).unwrap();
        let graphic = &native.graphics[0];
        let style = &*crate::style::DEFAULT;
        // The PNG pHYs field is integer pixels/meter, hence a small physical
        // quantization at the lowest resolutions. All wrappers must agree.
        assert!((cdx_width - 72.).abs() < 0.02, "{dpi} DPI width");
        assert!((cdx_height - 36.).abs() < 0.02, "{dpi} DPI height");
        assert!((f64::from(graphic.axis_x.x * style.points_per_world()) - cdx_width).abs() < 0.001);
        assert!(
            (f64::from(graphic.axis_y.y * style.points_per_world()) - cdx_height).abs() < 0.001
        );
    }
}

fn paste_test_picture(kind: &str, format: image::ImageFormat) -> Representation {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut bytes, format)
        .unwrap();
    Representation::new(kind, bytes.get_ref())
}

#[tokio::test]
async fn platform_packets_prefer_chemical_text_over_pictures_and_skip_other_text() {
    let source = Document::from_json(include_bytes!(
        "../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let mol = prepare_as(Default::default(), source.clone(), CopyFormat::Mol)
        .await
        .unwrap();
    let mol_text = mol.text().unwrap();
    let png = paste_test_picture("public.png", image::ImageFormat::Png);
    let tiff = paste_test_picture("public.tiff", image::ImageFormat::Tiff);
    let smiles = Representation::new("public.utf8-plain-text", b"CCO");
    let caption = Representation::new("public.utf8-plain-text", b"hello");
    let mol_text = Representation::new("public.utf8-plain-text", mol_text.as_bytes());
    let native = Representation::new(NATIVE, &serde_json::to_vec(&source).unwrap());
    let cdx_bytes = include_bytes!("../../tests/fixtures/native-ethyl-clipboard.cdx");
    // Every platform sends portable kinds. Exercise each platform's CDX alias
    // here on every host, rather than cfg-eliding two-thirds of the cases.
    for (platform, cdx_kind) in [
        ("macOS", "com.perkinelmer.chemdraw.cdx-clipboard"),
        ("Windows", "com.revvity.chemdraw.cdx-clipboard"),
        ("Linux", "chemical/x-cdx"),
    ] {
        let cdx = Representation::new(cdx_kind, cdx_bytes);
        for (representations, atoms, picture, is_native) in [
            (vec![png.clone(), smiles.clone()], 3, false, false),
            (vec![caption.clone(), png.clone()], 0, true, false),
            (
                vec![png.clone(), smiles.clone(), cdx.clone()],
                11,
                false,
                false,
            ),
            (
                vec![
                    png.clone(),
                    smiles.clone(),
                    cdx.clone(),
                    mol_text.clone(),
                    native.clone(),
                ],
                source.atoms.len(),
                false,
                true,
            ),
            (vec![tiff.clone(), mol_text.clone()], 3, false, false),
        ] {
            let pasted = paste_packet_with_warnings(
                Default::default(),
                Packet {
                    generation: Some("test".into()),
                    representations,
                },
                false,
            )
            .await
            .unwrap();
            assert_eq!(pasted.document.atoms.len(), atoms, "{platform}");
            assert_eq!(pasted.native, is_native, "{platform}");
            assert_eq!(
                pasted.document.graphics.iter().any(|g| g.picture.is_some()),
                picture,
                "{platform}"
            );
            assert!(
                pasted.warnings.is_empty(),
                "{platform}: {:?}",
                pasted.warnings
            );
        }
        let error = paste_packet(
            Default::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![Representation::new("com.adobe.pdf", b"%PDF-1.7")],
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            "Paste supports PNG, JPEG, TIFF and WebP pictures. Export this PDF or SVG to PNG first",
            "{platform}"
        );
    }
}

#[tokio::test]
async fn failed_native_cdx_and_cdxml_fall_through_and_newer_drawing_error_is_retained() {
    let newer = Representation::new(
        NATIVE,
        &serde_json::to_vec(&serde_json::json!({
            "version": u64::from(crate::document::VERSION) + 1,
            "future_field": ["not readable by this version"],
        }))
        .unwrap(),
    );
    let cdx = Representation::new(
        "com.revvity.chemdraw.cdx-clipboard",
        include_bytes!("../../tests/fixtures/native-ethyl-clipboard.cdx"),
    );
    let fallback = paste_packet_with_warnings(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![cdx.clone(), newer.clone()],
        },
        false,
    )
    .await
    .unwrap();
    assert_eq!(fallback.document.atoms.len(), 11);
    assert!(!fallback.native);
    assert!(fallback.warnings.is_empty());

    let mut source = Document::from_json(include_bytes!(
        "../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let mol = prepare_as(Default::default(), source.clone(), CopyFormat::Mol)
        .await
        .unwrap();
    let mol = Representation::new("com.mdli.molfile", mol.text().unwrap().as_bytes());
    source.bonds[0].a = u64::MAX;
    let invalid_native = Representation::new(NATIVE, &serde_json::to_vec(&source).unwrap());
    let damaged_cdx =
        Representation::new("com.revvity.chemdraw.cdx-clipboard", b"VjCD0100 damaged");
    let damaged_cdxml = Representation::new("chemical/x-cdxml", b"<CDXML><bad");
    for failed in [invalid_native, damaged_cdx.clone(), damaged_cdxml] {
        let result = paste_packet_with_warnings(
            Default::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![mol.clone(), failed],
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(result.document.atoms.len(), 3);
        assert!(result.warnings.is_empty());
    }
    let error = paste_packet(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![
                newer,
                damaged_cdx,
                Representation::new("public.utf8-plain-text", b"hello"),
                Representation::new("public.png", b"invalid picture"),
            ],
        },
    )
    .await
    .unwrap_err();
    assert!(error.contains("newer version of ReShiki"), "{error}");
    assert!(error.contains("Update ReShiki"), "{error}");
}

#[tokio::test]
async fn text_only_clipboard_reports_no_picture_for_picture_paste() {
    for (kind, image_only, expected) in [
        ("public.text", true, "No picture on the clipboard"),
        (
            "public.utf8-plain-text",
            true,
            "No picture on the clipboard",
        ),
        (
            "public.text",
            false,
            "No supported drawing on the clipboard",
        ),
    ] {
        let packet = Packet {
            generation: Some("test".into()),
            representations: vec![Representation::new(kind, b"plain text")],
        };
        let error = paste_packet_with_warnings(Default::default(), packet, image_only)
            .await
            .unwrap_err();
        assert_eq!(error, expected);
    }
}

#[tokio::test]
async fn lazy_paste_reads_only_until_success_and_explicit_picture_paste_ignores_text() {
    let png = paste_test_picture("public.png", image::ImageFormat::Png);
    for (text, picture_only, expected_reads, atoms) in [
        ("CCO", false, vec!["public.utf8-plain-text"], 3),
        (
            "hello",
            false,
            vec!["public.utf8-plain-text", "public.png"],
            0,
        ),
        ("CCO", true, vec!["public.png"], 0),
    ] {
        let offered = [
            png.clone(),
            Representation::new("public.utf8-plain-text", text.as_bytes()),
        ];
        let mut reads = Vec::new();
        let result = read_candidates(
            picture_only,
            |formats, _, _, _| {
                let item = formats
                    .iter()
                    .find_map(|kind| offered.iter().find(|item| &item.kind == kind))
                    .cloned();
                if let Some(item) = &item {
                    reads.push(item.kind.clone());
                }
                std::future::ready(Ok(Packet {
                    generation: Some("test".into()),
                    representations: item.into_iter().collect(),
                }))
            },
            |item| paste_representation(Default::default(), item),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(reads, expected_reads);
        assert_eq!(result.document.atoms.len(), atoms);
        assert!(result.warnings.is_empty());
    }
    let mut broken = Representation::new(NATIVE, b"");
    broken.read_error = Some("Clipboard data exceeds 64 MB".into());
    let result = paste_packet(
        Default::default(),
        Packet {
            generation: Some("test".into()),
            representations: vec![broken, png],
        },
    )
    .await
    .unwrap();
    assert_eq!(result.graphics.len(), 1);
}

#[tokio::test]
async fn failed_office_drawing_retries_standalone_native_without_repeating_office() {
    let source = Document::from_json(include_bytes!(
        "../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    let standalone = Representation::new(NATIVE, &serde_json::to_vec(&source).unwrap());
    let mut office = Representation::new(NATIVE, b"invalid Office drawing");
    office.embedded = Some(true);
    let mut skips = Vec::new();
    let result = read_candidates(
        false,
        |formats, _, skip_embedded, _| {
            assert!(formats.contains(&NATIVE.to_owned()));
            skips.push(skip_embedded);
            std::future::ready(Ok(Packet {
                generation: Some("test".into()),
                representations: vec![if skip_embedded {
                    standalone.clone()
                } else {
                    office.clone()
                }],
            }))
        },
        |item| paste_representation(Default::default(), item),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(skips, [false, true]);
    assert_eq!(result.document, source);
    assert!(result.native);
}

#[tokio::test]
async fn failed_transport_alias_is_consumed_once_before_the_next_alias() {
    let mut primary = Representation::new("com.revvity.chemdraw.cdx-clipboard", b"damaged CDX");
    primary.source = Some("chemical/x-cdx".into());
    let mut alternate = Representation::new(
        "com.revvity.chemdraw.cdx-clipboard",
        include_bytes!("../../tests/fixtures/native-ethyl-clipboard.cdx"),
    );
    alternate.source = Some("application/x-chemdraw".into());
    let mut reads = Vec::new();
    let result = read_candidates(
        false,
        |formats, skip_sources, _, _| {
            assert!(formats.contains(&primary.kind));
            let item = [&primary, &alternate]
                .into_iter()
                .find(|item| !skip_sources.contains(item.source.as_ref().unwrap()))
                .cloned();
            if let Some(item) = &item {
                reads.push(item.source.clone().unwrap());
            }
            std::future::ready(Ok(Packet {
                generation: Some("test".into()),
                representations: item.into_iter().collect(),
            }))
        },
        |item| paste_representation(Default::default(), item),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(reads, ["chemical/x-cdx", "application/x-chemdraw"]);
    assert_eq!(result.document.atoms.len(), 11);
    assert!(result.warnings.is_empty());
}

#[test]
fn ordered_clipboard_requests_keep_format_and_source_lists_bounded() {
    assert!(encode_ordered_request("read", &[], &["unknown".into()], &[], false, None).is_err());
    assert!(
        encode_ordered_request(
            "read",
            &[],
            &[],
            &vec!["image/png".into(); 129],
            false,
            None
        )
        .is_err()
    );
    assert!(
        encode_ordered_request("read", &[], &[], &["bad\0source".into()], false, None).is_err()
    );
    let bytes = encode_ordered_request(
        "read",
        &[],
        &["public.png".into()],
        &["PNG".into()],
        true,
        Some("42"),
    )
    .unwrap();
    let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(request["formats"], serde_json::json!(["public.png"]));
    assert_eq!(request["skip_sources"], serde_json::json!(["PNG"]));
    assert_eq!(request["skip_embedded"], true);
    assert_eq!(request["generation"], "42");
    for invalid in ["", "two tokens", "\0", &"x".repeat(129)] {
        assert!(encode_ordered_request("read", &[], &[], &[], false, Some(invalid)).is_err());
    }
}

#[tokio::test]
async fn later_clipboard_owner_failure_keeps_the_newer_drawing_diagnostic() {
    let newer = Representation::new(
        NATIVE,
        &serde_json::to_vec(&serde_json::json!({
            "version": u64::from(crate::document::VERSION) + 1,
        }))
        .unwrap(),
    );
    let mut calls = 0;
    let result = read_candidates(
        false,
        |_, _, _, _| {
            calls += 1;
            std::future::ready(if calls == 1 {
                Ok(Packet {
                    generation: Some("test".into()),
                    representations: vec![newer.clone()],
                })
            } else {
                Err("The clipboard owner is unavailable".into())
            })
        },
        |item| paste_representation(Default::default(), item),
    )
    .await
    .unwrap_err();
    assert_eq!(calls, 2);
    assert!(result.contains("Update ReShiki"), "{result}");
}

#[tokio::test]
async fn empty_clipboard_drawing_falls_through_to_picture() {
    let empty_mol = b"\n  example\n\n  0  0\nM  END\n";
    let png = paste_test_picture("public.png", image::ImageFormat::Png);
    for empty in [
        Representation::new("public.utf8-plain-text", empty_mol),
        Representation::new("com.mdli.molfile", empty_mol),
        Representation::new(NATIVE, &serde_json::to_vec(&Document::default()).unwrap()),
    ] {
        let pasted = paste_packet_with_warnings(
            Default::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![empty.clone(), png.clone()],
            },
            false,
        )
        .await
        .unwrap();
        assert!(pasted.document.atoms.is_empty());
        assert_eq!(pasted.document.graphics.len(), 1);
        assert!(pasted.document.graphics[0].picture.is_some());
        assert!(pasted.warnings.is_empty());
        let error = paste_packet(
            Default::default(),
            Packet {
                generation: Some("test".into()),
                representations: vec![empty],
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "The clipboard drawing is empty");
    }
}

#[tokio::test]
async fn changed_generation_aborts_before_importing_the_replacement_candidate() {
    for (first, replacement) in [
        ("7", "8"),                       // macOS changeCount
        ("windows:42:3", "windows:43:3"), // Windows owner and format count
        ("x11:123:7", "x11:456:8"),
        ("wayland:1:7", "wayland:1:8"),
    ] {
        let mut calls = 0;
        let mut imports = 0;
        let result = read_candidates(
            false,
            |_, sources, embedded, generation| {
                calls += 1;
                if calls == 1 {
                    assert!(generation.is_none());
                } else {
                    assert_eq!(generation.as_deref(), Some(first));
                    assert_eq!(sources, ["CDX"]);
                    assert!(embedded);
                }
                let mut item = Representation::new("com.revvity.chemdraw.cdx-clipboard", b"cdx");
                item.source = Some("CDX".into());
                std::future::ready(Ok(Packet {
                    generation: Some(if calls == 1 { first } else { replacement }.into()),
                    representations: vec![item],
                }))
            },
            |_| {
                imports += 1;
                std::future::ready(Err::<(), _>("damaged CDX".into()))
            },
        )
        .await
        .unwrap_err();
        assert_eq!(result, priority::CLIPBOARD_CHANGED);
        assert_eq!(calls, 2);
        assert_eq!(imports, 1);
    }
}

#[tokio::test]
async fn native_generation_change_error_overrides_an_earlier_update_diagnostic() {
    let mut calls = 0;
    let result = read_candidates(
        false,
        |_, _, _, _| {
            calls += 1;
            std::future::ready(if calls == 1 {
                Ok(Packet {
                    generation: Some("7".into()),
                    representations: vec![Representation::new(NATIVE, b"future drawing")],
                })
            } else {
                Err(priority::CLIPBOARD_CHANGED.into())
            })
        },
        |_| {
            std::future::ready(Err::<(), _>(
                "A newer version of ReShiki is needed; Update ReShiki".into(),
            ))
        },
    )
    .await
    .unwrap_err();
    assert_eq!(result, priority::CLIPBOARD_CHANGED);
    assert_eq!(calls, 2);
}
