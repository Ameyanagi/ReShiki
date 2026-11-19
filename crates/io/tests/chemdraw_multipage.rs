use reshiki_io::{chemistry::cdxml, engine::LocalEngine, exchange, formats};
use reshiki_model::{arrows::Head, document::Document, scene};
use std::path::Path;

#[tokio::test]
async fn all_generated_pages_import_with_their_molecules_captions_and_stamps() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/chemdraw-import-corpus");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let engine = LocalEngine::in_process();
    for case in manifest["cases"].as_array().unwrap() {
        let Some(pages) = case["page_count"].as_u64() else {
            continue;
        };
        if pages < 2 {
            continue;
        }
        let source =
            std::fs::read_to_string(root.join(case["files"]["cdxml"].as_str().unwrap())).unwrap();
        let tree = roxmltree::Document::parse_with_options(
            &source,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let atom_count = tree.descendants().filter(|n| n.has_tag_name("n")).count();
        let bond_count = tree.descendants().filter(|n| n.has_tag_name("b")).count();
        let captions: Vec<_> = tree
            .descendants()
            .filter(|n| {
                n.has_tag_name("t") && !n.parent_element().is_some_and(|p| p.has_tag_name("n"))
            })
            .map(|n| {
                n.children()
                    .filter(|s| s.has_tag_name("s"))
                    .map(|s| s.text().unwrap_or_default())
                    .collect::<String>()
            })
            .collect();
        let direct = cdxml::import_cdxml(&source).unwrap();
        assert_eq!(direct.document.atoms.len(), atom_count);
        assert_eq!(direct.document.bonds.len(), bond_count);
        assert!(direct.notices.iter().any(|n| n.contains("ChemDraw pages")));
        for (variant, format) in [("cdxml", "cdxml"), ("cdx", "cdx"), ("cds", "cdx")] {
            let path = root.join(case["files"][variant].as_str().unwrap());
            let (document, warnings) =
                formats::read_with_warnings(std::fs::read(&path).unwrap(), format, &engine)
                    .await
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert_eq!(document.atoms.len(), atom_count, "{}", path.display());
            assert_eq!(document.bonds.len(), bond_count, "{}", path.display());
            assert_eq!(
                document.annotations.len(),
                captions.len(),
                "{}",
                path.display()
            );
            for text in &captions {
                assert!(
                    document
                        .annotations
                        .iter()
                        .any(|caption| &caption.text == text)
                );
            }
            assert_eq!(
                warnings
                    .iter()
                    .filter(|n| n.contains("ChemDraw pages"))
                    .count(),
                1
            );
            assert!(document.page_layout.is_none());
            for target in ["cdxml", "cdx", "cds"] {
                let xml = formats::write_with_engine(document.clone(), "cdxml", &engine)
                    .await
                    .unwrap();
                let exported =
                    roxmltree::Document::parse(std::str::from_utf8(&xml).unwrap()).unwrap();
                let page = exported
                    .descendants()
                    .find(|node| node.has_tag_name("page"))
                    .unwrap();
                let bounds: Vec<f64> = page
                    .attribute("BoundingBox")
                    .unwrap()
                    .split_whitespace()
                    .map(|value| value.parse().unwrap())
                    .collect();
                for atom in exported.descendants().filter(|node| node.has_tag_name("n")) {
                    let position: Vec<f64> = atom
                        .attribute("p")
                        .unwrap()
                        .split_whitespace()
                        .map(|value| value.parse().unwrap())
                        .collect();
                    assert!(position[0] >= bounds[0] && position[0] <= bounds[2]);
                    assert!(
                        position[1] >= bounds[1] && position[1] <= bounds[3],
                        "{}: exported page bounds clip atom at {}",
                        case["id"],
                        position[1]
                    );
                }
                let bytes = match target {
                    "cdx" => exchange::to_cdx(std::str::from_utf8(&xml).unwrap()).unwrap(),
                    "cds" => exchange::to_cds(std::str::from_utf8(&xml).unwrap()).unwrap(),
                    _ => xml,
                };
                let restored = formats::read_with_engine(
                    bytes,
                    if target == "cdxml" { "cdxml" } else { "cdx" },
                    &engine,
                )
                .await
                .unwrap();
                assert_eq!(restored.atoms.len(), atom_count);
                assert_eq!(restored.bonds.len(), bond_count);
                assert_eq!(restored.annotations.len(), captions.len());
                assert_eq!(restored.graphics.len(), document.graphics.len());
            }
        }
    }
}

#[tokio::test]
async fn identical_local_atom_coordinates_on_separate_pages_do_not_collide() {
    let xml = r#"<CDXML BondLength="24" LabelFace="3" LabelSize="18">
      <page id="1" BoundingBox="0 0 612 792"><fragment id="2">
        <n id="3" Element="8" p="20 30"/></fragment></page>
      <page id="4" BoundingBox="0 0 792 612"><fragment id="5">
        <n id="6" Element="8" p="20 30"/></fragment></page></CDXML>"#;
    for bytes in [
        xml.as_bytes().to_vec(),
        exchange::from_cdx(&exchange::to_cds(xml).unwrap())
            .unwrap()
            .into_bytes(),
    ] {
        let document = formats::read_with_engine(bytes, "cdxml", &LocalEngine::in_process())
            .await
            .unwrap();
        assert_eq!(document.atoms.len(), 2);
        let atoms = &document.atoms;
        assert!((atoms[0].position.x - atoms[1].position.x).abs() < 0.001);
        assert!(atoms[1].position.y - atoms[0].position.y > 2000.);
        for atom in atoms {
            let style = atom.text_style.as_ref().unwrap();
            assert!(style.bold && style.italic);
        }
    }
}

#[tokio::test]
async fn a_page_library_opens_as_a_grid_of_pages_in_page_order() {
    // 48 letter pages in one column would run past the signed 16.16 point
    // range of CDX coordinates; ChemDraw's template libraries are like this.
    let pages: String = (0..48)
        .map(|page| {
            format!(
                "<page id='{}'><graphic id='{}' GraphicType='Rectangle' \
                 BoundingBox='0 0 540 720'/></page>",
                2 * page + 1,
                2 * page + 2
            )
        })
        .collect();
    let xml = format!("<CDXML>{pages}</CDXML>");
    let engine = LocalEngine::in_process();
    for (format, bytes) in [
        ("cdxml", xml.clone().into_bytes()),
        ("cdx", exchange::to_cdx(&xml).unwrap()),
    ] {
        let (document, warnings) = formats::read_with_warnings(bytes, format, &engine)
            .await
            .unwrap_or_else(|error| panic!("{format}: {error}"));
        assert!(warnings.iter().any(|w| w.contains("ChemDraw pages")));
        let boxes: Vec<_> = document
            .graphics
            .iter()
            .map(|graphic| {
                scene::bounds(&scene::primitives(&Document {
                    graphics: vec![graphic.clone()],
                    ..Document::default()
                }))
            })
            .collect();
        assert_eq!(boxes.len(), 48);
        // Nine pages to a row, about as wide as a window: Fit shows them all.
        for (page, (lo, _)) in boxes.iter().enumerate() {
            let (first, _) = boxes[page / 9 * 9];
            assert!((lo.y - first.y).abs() < 0.01, "page {page} leaves its row");
            if page % 9 > 0 {
                let (_, left) = boxes[page - 1];
                assert!(lo.x > left.x, "page {page} overlaps the one before");
            } else if page > 0 {
                let (_, above) = boxes[page - 9];
                assert!(lo.y > above.y, "row of page {page} overlaps the one above");
            }
        }
    }
}

#[tokio::test]
async fn text_only_pages_without_boxes_are_spaced_using_their_text_size() {
    let xml = r#"<CDXML CaptionSize="72"><page id="1"><t id="2" p="0 0"><s>One</s></t></page>
        <page id="3"><t id="4" p="0 0"><s>Two</s></t></page></CDXML>"#;
    let document =
        formats::read_with_engine(xml.as_bytes().to_vec(), "cdxml", &LocalEngine::in_process())
            .await
            .unwrap();
    assert_eq!(document.annotations.len(), 2);
    let bounds: Vec<_> = document
        .annotations
        .iter()
        .map(|caption| {
            assert_eq!(caption.format.style.size_pt, 72.);
            let single = Document {
                annotations: vec![caption.clone()],
                drawing_style: document.drawing_style.clone(),
                ..Document::default()
            };
            scene::bounds(&scene::primitives(&single))
        })
        .collect();
    let [(_, first_bottom), (second_top, _)] = bounds.as_slice() else {
        panic!("Expected bounds for both page captions");
    };
    assert!(second_top.y > first_bottom.y, "Page captions overlap");
}

#[tokio::test]
async fn malformed_or_nested_page_geometry_is_rejected_without_a_partial_drawing() {
    for xml in [
        "<CDXML><page><page/></page></CDXML>",
        "<CDXML><page><t p='NaN 10'><s>bad</s></t></page><page/></CDXML>",
        "<CDXML><page BoundingBox='0 0 10'/><page/></CDXML>",
    ] {
        assert!(
            formats::read_with_engine(xml.as_bytes().to_vec(), "cdxml", &LocalEngine::in_process())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn published_legacy_curve_heads_keep_their_direction_and_half_head() {
    for (flags, head, tail) in [
        (8, Head::Full, Head::None),
        (16, Head::None, Head::Full),
        (32, Head::Left, Head::None),
        (64, Head::None, Head::Left),
    ] {
        let xml = format!(
            "<CDXML><page id='1'><curve id='2' CurveType='{flags}' \
             CurvePoints='0 0 0 0 20 -10 40 10 60 0 60 0'/></page></CDXML>"
        );
        let document: Document =
            formats::read_with_engine(xml.into_bytes(), "cdxml", &LocalEngine::in_process())
                .await
                .unwrap();
        assert_eq!(document.arrows.len(), 1);
        let style = document.arrows[0].style.as_ref().unwrap();
        assert_eq!(style.head, head);
        assert_eq!(style.tail, tail);
    }
}
