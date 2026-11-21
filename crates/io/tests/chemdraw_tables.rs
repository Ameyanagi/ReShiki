//! Normal-entry and editable interchange proofs for native cell ownership.
use reshiki_io::{chemistry::cdxml, engine::LocalEngine, exchange, formats};
use reshiki_model::{
    document::{Document, Point},
    graphics::{
        Graphic, GraphicKind, LinePattern,
        table::{Membership, Side},
    },
};

const XML: &[u8] = include_bytes!("fixtures/tables/native.chemdraw.cdxml");
const CDX: &[u8] = include_bytes!("fixtures/tables/native.chemdraw.cdx");

fn table(doc: &Document) -> &Graphic {
    let tables = doc
        .graphics
        .iter()
        .filter(|g| g.kind == GraphicKind::Table)
        .collect::<Vec<_>>();
    assert_eq!(tables.len(), 1);
    tables[0]
}

fn cell_texts(doc: &Document, index: usize) -> Vec<&str> {
    let cell = &table(doc).table.as_ref().unwrap().cells[index];
    doc.annotations
        .iter()
        .filter(|a| cell.members.contains(&a.id))
        .map(|a| a.text.as_str())
        .collect()
}

fn check_native(doc: &Document) {
    doc.validate().unwrap();
    assert_eq!(doc.atoms.len(), 6);
    assert_eq!(doc.bonds.len(), 6);
    assert_eq!(
        doc.annotations.len(),
        8,
        "Cell captions must be read exactly once"
    );
    let graphic = table(doc);
    let cells = &graphic.table.as_ref().unwrap().cells;
    assert_eq!(cells.len(), 6);
    assert_eq!(cell_texts(doc, 0), ["Entry"]);
    assert_eq!(cell_texts(doc, 1), ["Structure"]);
    assert_eq!(cell_texts(doc, 2), ["Yield"]);
    assert_eq!(
        cell_texts(doc, 3),
        ["1", "Crosses into the next cell", "Below the table"]
    );
    assert_eq!(cell_texts(doc, 5), ["Example", "Past the table edge"]);
    assert!(
        doc.annotations.iter().all(|a| !a.text.contains('\n')),
        "Generated native LineStarts must not become authored newlines"
    );
    assert_eq!(cells[4].members.len(), 6);
    assert!(doc.atoms.iter().all(|a| cells[4].members.contains(&a.id)));
    assert!(cells.iter().all(|c| c.borders.len() == 4));
    assert_eq!(
        cells[0].borders.iter().map(|b| b.side).collect::<Vec<_>>(),
        [Side::Left, Side::Top, Side::Bottom, Side::Right]
    );
    assert_eq!(cells[0].borders[3].width_pt, 5.);
    assert_eq!(cells[1].borders[0].width_pt, 1.);
    assert_ne!(cells[0].borders[3].color, cells[1].borders[0].color);
    assert_eq!(cells[3].borders[3].width_pt, 1.);
    assert_eq!(cells[4].borders[0].width_pt, 5.);
    let ownership = Membership::new(doc).unwrap();
    assert_eq!(ownership.owned_ids().count(), 14);
    // Native chemistry extends beyond its owning cell. Its real atom positions
    // survive; clipping is not a chemical coordinate edit or a membership test.
    let frame = cells[4].frame(graphic).unwrap();
    assert!(
        doc.atoms
            .iter()
            .any(|a| frame.local(a.position).unwrap().x > 1.)
    );
    let below = doc
        .annotations
        .iter()
        .find(|a| a.text == "Below the table")
        .unwrap();
    assert!(below.position.y > cells[3].frame(graphic).unwrap().point(Point::new(0., 1.)).y);
    assert_eq!(ownership.cell_of(below.id).unwrap().cell, cells[3].id);
}

async fn read(bytes: &[u8], format: &str) -> Document {
    let (doc, warnings) =
        formats::read_with_warnings(bytes.to_vec(), format, &LocalEngine::in_process())
            .await
            .unwrap();
    assert!(
        warnings
            .iter()
            .all(|w| !w.contains("table") && !w.contains("nested page")),
        "{warnings:?}"
    );
    doc
}

#[tokio::test]
async fn native_table_normal_entry_keeps_grid_chemistry_full_text_and_explicit_cell_ownership() {
    for (bytes, format) in [(XML, "cdxml"), (CDX, "cdx")] {
        check_native(&read(bytes, format).await);
    }
    check_native(
        &cdxml::import_cdxml(std::str::from_utf8(XML).unwrap())
            .unwrap()
            .document,
    );
    let loss = formats::chemdraw_loss("cdxml", std::str::from_utf8(XML).unwrap());
    assert_eq!(loss.unsupported, 0);
}

fn compare(source: &Document, restored: &Document) {
    let (a, b) = (table(source), table(restored));
    assert!(a.axis_x.distance(b.axis_x) < 0.003);
    assert!(a.axis_y.distance(b.axis_y) < 0.003);
    for (index, (left, right)) in a
        .table
        .as_ref()
        .unwrap()
        .cells
        .iter()
        .zip(&b.table.as_ref().unwrap().cells)
        .enumerate()
    {
        assert!(left.lo.distance(right.lo) < 0.0001);
        assert!(left.hi.distance(right.hi) < 0.0001);
        assert_eq!(left.borders.len(), right.borders.len());
        for (left, right) in left.borders.iter().zip(&right.borders) {
            assert_eq!(left.side, right.side);
            assert_eq!(left.color, right.color);
            assert_eq!(left.pattern, right.pattern);
            // CDX stores points as 16.16 fixed-point values.
            assert!((left.width_pt - right.width_pt).abs() <= 1. / 65536.);
        }
        assert_eq!(cell_texts(source, index), cell_texts(restored, index));
    }
    for (left, right) in source.atoms.iter().zip(&restored.atoms) {
        assert_eq!(left.element, right.element);
        assert!(
            left.position
                .offset(-a.origin.x, -a.origin.y)
                .distance(right.position.offset(-b.origin.x, -b.origin.y))
                < 0.003
        );
    }
    for left in &source.annotations {
        let right = restored
            .annotations
            .iter()
            .find(|r| r.text == left.text)
            .unwrap();
        assert!(
            left.position
                .offset(-a.origin.x, -a.origin.y)
                .distance(right.position.offset(-b.origin.x, -b.origin.y))
                < 0.003,
            "{}: {:?} {:?}",
            left.text,
            left.position,
            right.position
        );
    }
}

#[tokio::test]
async fn native_table_roundtrips_as_native_editable_cdxml_cdx_and_cds_twice() {
    for (bytes, input) in [(XML, "cdxml"), (CDX, "cdx")] {
        let source = read(bytes, input).await;
        for output in ["cdxml", "cdx", "cds"] {
            let mut doc = source.clone();
            for _ in 0..2 {
                let xml = String::from_utf8(
                    formats::write_with_engine(doc.clone(), "cdxml", &LocalEngine::in_process())
                        .await
                        .unwrap(),
                )
                .unwrap();
                let tree = roxmltree::Document::parse(&xml).unwrap();
                let native = tree
                    .descendants()
                    .find(|n| n.has_tag_name("table"))
                    .unwrap();
                assert_eq!(
                    native.children().filter(|n| n.has_tag_name("page")).count(),
                    6
                );
                assert_eq!(
                    native
                        .descendants()
                        .filter(|n| n.has_tag_name("border"))
                        .count(),
                    24
                );
                assert_eq!(
                    native.descendants().filter(|n| n.has_tag_name("n")).count(),
                    6
                );
                assert_eq!(
                    native.descendants().filter(|n| n.has_tag_name("b")).count(),
                    6
                );
                assert_eq!(
                    native.descendants().filter(|n| n.has_tag_name("t")).count(),
                    8
                );
                // Cell page content really is local: its off-cell molecule is
                // still about 67–93 paper points, not 155–181 world points.
                let x = native
                    .descendants()
                    .find(|n| n.has_tag_name("n"))
                    .unwrap()
                    .attribute("p")
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap();
                assert!((60. ..100.).contains(&x), "{x}");
                let data = match output {
                    "cdx" => exchange::to_cdx(&xml).unwrap(),
                    "cds" => exchange::to_cds(&xml).unwrap(),
                    _ => xml.into_bytes(),
                };
                doc = read(&data, if output == "cdxml" { "cdxml" } else { "cdx" }).await;
                check_native(&doc);
                compare(&source, &doc);
            }
        }
    }
}

const SIMPLE: &str = r#"<CDXML BondLength="30"><page id="1"><table id="2" BoundingBox="18 28 122 72"><page id="3" BoundsInParent="20 30 70 70"><t id="4" p="8 15" LineStarts="2 5"><s>one two&#10;three</s></t><border id="5" Side="right" LineWidth="0"/></page><page id="6" BoundsInParent="70 30 120 70"><fragment id="7"><n id="8" p="8 20" Element="8"/></fragment><border id="9" Side="left" LineWidth="2"/></page></table><t id="10" p="150 50"><s>Outside</s></t></page></CDXML>"#;

#[tokio::test]
async fn zero_width_authored_breaks_and_outside_objects_keep_their_ownership() {
    let source = read(SIMPLE.as_bytes(), "cdxml").await;
    assert_eq!(cell_texts(&source, 0), ["one two\nthree"]);
    assert_eq!(
        table(&source).table.as_ref().unwrap().cells[0].borders[0].width_pt,
        0.
    );
    let outside = source
        .annotations
        .iter()
        .find(|a| a.text == "Outside")
        .unwrap();
    assert!(
        Membership::new(&source)
            .unwrap()
            .cell_of(outside.id)
            .is_none()
    );
    for output in ["cdxml", "cdx"] {
        let bytes = formats::write_with_engine(source.clone(), output, &LocalEngine::in_process())
            .await
            .unwrap();
        let restored = read(&bytes, output).await;
        assert_eq!(restored.annotations.len(), 2);
        compare(&source, &restored);
    }
}

#[tokio::test]
async fn a_parent_group_keeps_the_table_with_its_outside_caption() {
    let xml = SIMPLE
        .replace("<table id=\"2\"", "<group id=\"11\"><table id=\"2\"")
        .replace("</page></CDXML>", "</group></page></CDXML>");
    let source = read(xml.as_bytes(), "cdxml").await;
    assert_eq!(source.groups.len(), 1);
    let outside = source
        .annotations
        .iter()
        .find(|a| a.text == "Outside")
        .unwrap();
    let mut members = source.groups[0].members.clone();
    members.sort_unstable();
    let mut expected = vec![table(&source).id, outside.id];
    expected.sort_unstable();
    assert_eq!(members, expected);
    for format in ["cdxml", "cdx"] {
        let bytes = formats::write_with_engine(source.clone(), format, &LocalEngine::in_process())
            .await
            .unwrap();
        let restored = read(&bytes, format).await;
        assert_eq!(restored.groups.len(), 1);
        assert!(restored.groups[0].members.contains(&table(&restored).id));
        compare(&source, &restored);
    }
}

#[tokio::test]
async fn top_level_page_arrangement_moves_the_cell_frame_and_contents_together() {
    let xml = SIMPLE.replace("</CDXML>","<page id=\"100\"><table id=\"101\"><page id=\"102\" BoundsInParent=\"20 30 120 70\"><t id=\"103\" p=\"8 15\"><s>Second page</s></t><border id=\"104\" Side=\"bottom\" LineWidth=\"1\"/></page></table></page></CDXML>");
    for (bytes, format) in [
        (xml.as_bytes().to_vec(), "cdxml"),
        (exchange::to_cdx(&xml).unwrap(), "cdx"),
    ] {
        let (doc, warnings) =
            formats::read_with_warnings(bytes, format, &LocalEngine::in_process())
                .await
                .unwrap();
        assert_eq!(
            warnings
                .iter()
                .filter(|w| w.contains("ChemDraw pages"))
                .count(),
            1
        );
        let tables: Vec<_> = doc.graphics.iter().filter(|g| g.table.is_some()).collect();
        assert_eq!(tables.len(), 2);
        let second = doc
            .annotations
            .iter()
            .find(|a| a.text == "Second page")
            .unwrap();
        let owner = Membership::new(&doc).unwrap().cell_of(second.id).unwrap();
        let graphic = doc.graphics.iter().find(|g| g.id == owner.table).unwrap();
        let p = second.position.offset(-graphic.origin.x, -graphic.origin.y);
        assert!(p.distance(Point::new(8. * 42. / 14.4, 15. * 42. / 14.4)) < 0.003);
        assert!(tables[0].origin.distance(tables[1].origin) > 100.);
        let bytes = formats::write_with_engine(doc.clone(), "cdx", &LocalEngine::in_process())
            .await
            .unwrap();
        let restored = read(&bytes, "cdx").await;
        assert_eq!(
            restored
                .graphics
                .iter()
                .filter(|g| g.table.is_some())
                .count(),
            2
        );
        assert_eq!(restored.atoms.len(), doc.atoms.len());
        assert_eq!(restored.annotations.len(), doc.annotations.len());
    }
}

#[tokio::test]
async fn empty_native_cells_remain_an_editable_table() {
    let xml = r#"<CDXML><page id="1"><table id="2"><page id="3" BoundsInParent="0 0 40 30"><border id="4" Side="top" LineWidth="1"/></page></table></page></CDXML>"#;
    for (bytes, format) in [
        (xml.as_bytes().to_vec(), "cdxml"),
        (exchange::to_cdx(xml).unwrap(), "cdx"),
    ] {
        let doc = read(&bytes, format).await;
        assert!(
            table(&doc).table.as_ref().unwrap().cells[0]
                .members
                .is_empty()
        );
        let bytes = formats::write_with_engine(doc, "cdx", &LocalEngine::in_process())
            .await
            .unwrap();
        let restored = read(&bytes, "cdx").await;
        assert_eq!(table(&restored).table.as_ref().unwrap().cells.len(), 1);
    }
}

#[tokio::test]
async fn cell_graphics_keep_their_local_stacking_plane() {
    let xml=SIMPLE.replace("id=\"8\" p=\"8 20\"","id=\"8\" Z=\"1\" p=\"8 20\"")
        .replace("</fragment><border id=\"9\"","</fragment><graphic id=\"12\" Z=\"2\" GraphicType=\"Rectangle\" BoundingBox=\"2 2 12 12\"/><border id=\"9\"")
        .replace("id=\"10\" p=\"150 50\"","id=\"10\" Z=\"100\" p=\"150 50\"");
    let source = read(xml.as_bytes(), "cdxml").await;
    let bytes = formats::write_with_engine(source.clone(), "cdx", &LocalEngine::in_process())
        .await
        .unwrap();
    let restored = read(&bytes, "cdx").await;
    let mut shape_bounds = Vec::new();
    for doc in [&source, &restored] {
        let cell = &table(doc).table.as_ref().unwrap().cells[1];
        let shapes = doc
            .graphics
            .iter()
            .filter(|g| cell.members.contains(&g.id))
            .collect::<Vec<_>>();
        assert_eq!(shapes.len(), 1);
        let rectangle = shapes[0];
        // Plain rectangles export as equivalent editable paths.
        assert!(matches!(
            rectangle.kind,
            GraphicKind::Rectangle | GraphicKind::Path
        ));
        assert!(
            rectangle.layer > 0,
            "Outside Z must not change a cell-local shape's layer"
        );
        assert_eq!(
            Membership::new(doc)
                .unwrap()
                .cell_of(rectangle.id)
                .unwrap()
                .cell,
            cell.id
        );
        let (lo, hi) = rectangle.bounds();
        let origin = table(doc).origin;
        shape_bounds.push((
            lo.offset(-origin.x, -origin.y),
            hi.offset(-origin.x, -origin.y),
        ));
    }
    assert!(shape_bounds[0].0.distance(shape_bounds[1].0) < 0.003);
    assert!(shape_bounds[0].1.distance(shape_bounds[1].1) < 0.003);
}

#[tokio::test]
async fn malformed_native_tables_are_refused_without_flattening_their_contents() {
    for bad in [
        SIMPLE.replace("70 30 120 70","60 30 120 70"),
        SIMPLE.replace("Side=\"right\"","Side=\"unknown\""),
        SIMPLE.replace("LineWidth=\"0\"","LineWidth=\"13\""),
        SIMPLE.replace("20 30 70 70","NaN 30 70 70"),
        SIMPLE.replace("<fragment id=\"7\">","<table id=\"77\"><page id=\"78\" BoundsInParent=\"0 0 10 10\"/></table><fragment id=\"7\">"),
    ] {
        let error=formats::read_with_engine(bad.into_bytes(),"cdxml",&LocalEngine::in_process()).await.unwrap_err();
        assert!(!error.is_empty());
    }
}

#[tokio::test]
async fn editable_export_refuses_unrepresentable_table_frames_borders_and_child_groups() {
    let source = read(SIMPLE.as_bytes(), "cdxml").await;
    let table_id = table(&source).id;
    let mut rotated = source.clone();
    rotated
        .graphics
        .iter_mut()
        .find(|g| g.id == table_id)
        .unwrap()
        .axis_x
        .y = 20.;
    let error = formats::write_with_engine(rotated, "cdxml", &LocalEngine::in_process())
        .await
        .unwrap_err();
    assert!(error.contains("sheared table"), "{error}");
    let mut dotted = source.clone();
    dotted
        .graphics
        .iter_mut()
        .find(|g| g.id == table_id)
        .unwrap()
        .table
        .as_mut()
        .unwrap()
        .cells[0]
        .borders[0]
        .pattern = LinePattern::Dotted;
    let error = formats::write_with_engine(dotted, "cdxml", &LocalEngine::in_process())
        .await
        .unwrap_err();
    assert!(error.contains("dotted"), "{error}");
    let mut grouped = source.clone();
    let members = table(&source)
        .table
        .as_ref()
        .unwrap()
        .cells
        .iter()
        .flat_map(|c| c.members.iter().copied())
        .collect();
    grouped.groups.push(reshiki_model::grouping::Group {
        id: grouped.next_id(),
        members,
        integral: false,
        layout: None,
    });
    let error = formats::write_with_engine(grouped, "cdxml", &LocalEngine::in_process())
        .await
        .unwrap_err();
    assert!(error.contains("crosses table cells"), "{error}");
}
