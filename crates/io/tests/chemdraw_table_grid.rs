//! Shared native grid cuts must survive both decimal XML and binary ticks.
use reshiki_io::{engine::LocalEngine, exchange, formats};
use reshiki_model::{document::Document, graphics::GraphicKind};
use std::{collections::BTreeSet, path::PathBuf};

const XML: &[u8] = include_bytes!("fixtures/tables/native.chemdraw.cdxml");
const CDX: &[u8] = include_bytes!("fixtures/tables/native.chemdraw.cdx");

async fn read(bytes: &[u8], format: &str) -> Document {
    formats::read_with_engine(bytes.to_vec(), format, &LocalEngine::in_process())
        .await
        .unwrap()
}
fn bounds(text: &str) -> Vec<f64> {
    text.split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}
fn grid(xml: &str) -> Vec<Vec<f64>> {
    let tree = roxmltree::Document::parse(xml).unwrap();
    let table = tree
        .descendants()
        .find(|n| n.has_tag_name("table"))
        .unwrap();
    let mut x = BTreeSet::new();
    let mut y = BTreeSet::new();
    let cells = table
        .children()
        .filter(|n| n.has_tag_name("page"))
        .map(|cell| {
            let text = cell.attribute("BoundsInParent").unwrap();
            assert_eq!(cell.attribute("BoundingBox"), Some(text));
            let rect = bounds(text);
            assert_eq!(rect.len(), 4);
            for &v in &rect {
                assert_eq!(
                    (v * 65536.).fract(),
                    0.,
                    "A shared cut must be an exact native tick: {v}"
                );
            }
            x.extend([rect[0].to_bits(), rect[2].to_bits()]);
            y.extend([rect[1].to_bits(), rect[3].to_bits()]);
            rect
        })
        .collect::<Vec<_>>();
    assert_eq!(cells.len(), 6);
    assert_eq!(
        x.len(),
        4,
        "A repeated column edge must not create a narrow extra column"
    );
    assert_eq!(y.len(), 3);
    assert_eq!(cells[0][2], cells[1][0]);
    assert_eq!(cells[1][2], cells[2][0]);
    assert_eq!(cells[0][3], cells[3][1]);
    assert_eq!(cells[3][2], cells[4][0]);
    assert_eq!(cells[4][2], cells[5][0]);
    let outer = bounds(table.attribute("BoundingBox").unwrap());
    assert_eq!(outer, [cells[0][0], cells[0][1], cells[5][2], cells[5][3]]);
    cells
}

#[tokio::test]
async fn shared_table_edges_are_identical_native_ticks_in_both_encodings() {
    for (bytes, format) in [(XML, "cdxml"), (CDX, "cdx")] {
        let doc = read(bytes, format).await;
        let before = doc.clone();
        let xml = String::from_utf8(
            formats::write_with_engine(doc.clone(), "cdxml", &LocalEngine::in_process())
                .await
                .unwrap(),
        )
        .unwrap();
        let expected = grid(&xml);
        let decoded = exchange::from_cdx(&exchange::to_cdx(&xml).unwrap()).unwrap();
        assert_eq!(grid(&decoded), expected);
        let reopened = read(decoded.as_bytes(), "cdxml").await;
        let table = reopened
            .graphics
            .iter()
            .find(|g| g.kind == GraphicKind::Table)
            .unwrap()
            .table
            .as_ref()
            .unwrap();
        let texts = |cell: usize| {
            reopened
                .annotations
                .iter()
                .filter(|a| table.cells[cell].members.contains(&a.id))
                .map(|a| a.text.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(0), ["Entry"]);
        assert_eq!(texts(2), ["Yield"]);
        assert_eq!(
            texts(3),
            ["1", "Crosses into the next cell", "Below the table"]
        );
        assert!(
            reopened
                .atoms
                .iter()
                .all(|a| table.cells[4].members.contains(&a.id))
        );
        assert_eq!(doc, before);
    }
}

#[tokio::test]
async fn distinct_cuts_that_collapse_at_native_precision_are_refused_not_merged() {
    let mut doc = read(XML, "cdxml").await;
    let g = doc
        .graphics
        .iter_mut()
        .find(|g| g.kind == GraphicKind::Table)
        .unwrap();
    let cells = &mut g.table.as_mut().unwrap().cells;
    cells[1].lo.x = f32::from_bits(cells[0].hi.x.to_bits() + 1);
    doc.validate().unwrap();
    let before = doc.clone();
    let error = formats::write_with_engine(doc.clone(), "cdxml", &LocalEngine::in_process())
        .await
        .unwrap_err();
    assert!(error.contains("coordinate precision"), "{error}");
    assert_eq!(doc, before);
}

/// Produce exact IO outputs for an independent local ChemDraw SDK. This does
/// not build or launch the GUI, and never overwrites earlier evidence.
#[tokio::test]
#[ignore = "manual external ChemDraw SDK probe receipts"]
async fn emit_external_sdk_probe_receipts() {
    use sha2::{Digest, Sha256};
    let input = PathBuf::from(
        std::env::var_os("RESHIKI_TABLE_SDK_INPUTS")
            .expect("Set the authored probe input directory"),
    );
    let output = PathBuf::from(
        std::env::var_os("RESHIKI_TABLE_SDK_OUTPUTS").expect("Set a new output directory"),
    );
    std::fs::create_dir(&output).unwrap();
    let mut manifest = vec![];
    for name in [
        "reaction-table-source.cdxml",
        "reaction-table-native.cdx",
        "overflow-probe-source.cdxml",
        "overflow-probe-native.cdx",
        "overflow-probe-native.cdxml",
    ] {
        let source = std::fs::read(input.join(name)).unwrap();
        let format = if name.ends_with(".cdx") {
            "cdx"
        } else {
            "cdxml"
        };
        let doc = read(&source, format).await;
        let leaf = output.join(name.replace('.', "-"));
        std::fs::create_dir(&leaf).unwrap();
        for format in ["cdxml", "cdx", "svg", "png", "pdf", "rsk"] {
            let bytes = formats::write_with_engine(doc.clone(), format, &LocalEngine::in_process())
                .await
                .unwrap();
            let file = leaf.join(format!("drawing.{format}"));
            std::fs::write(&file, &bytes).unwrap();
            manifest.push(serde_json::json!({"input": name, "input_sha256": format!("{:x}", Sha256::digest(&source)), "format": format, "file": file.strip_prefix(&output).unwrap(), "bytes": bytes.len(), "sha256": format!("{:x}", Sha256::digest(&bytes))}));
        }
    }
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
