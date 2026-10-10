use super::*;

#[test]
fn windows_print_keeps_vector_text_page_offsets_and_physical_size() {
    let mut doc: Document = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    doc.page_layout = Some(crate::pages::Layout {
        columns: 2,
        ..crate::pages::Layout::around(&doc)
    });
    let result: Value = serde_json::from_slice(&print_snapshot(&doc).unwrap()).unwrap();
    assert_eq!(result["version"], 1);
    assert_eq!(result["pages"].as_array().unwrap().len(), 2);
    assert!(result["width_pt"].as_f64().unwrap() > 500.);
    assert!(
        result["primitives"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["fill"].is_array())
    );
    assert!(
        result["primitives"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["kind"] == "path")
    );
    let first = result["pages"][0][0].as_f64().unwrap();
    let second = result["pages"][1][0].as_f64().unwrap();
    assert!(first - second > 500.);
}

#[test]
fn retained_metafile_is_used_for_vector_export_and_preview_for_printing() {
    let mut emf = vec![0; 108];
    for (offset, value) in [
        (0, 1),
        (4, 88),
        (32, 2540),
        (36, 1270),
        (40, 0x464d4520),
        (44, 0x10000),
        (48, 108),
        (52, 2),
        (56, 1),
        (88, 14),
        (92, 20),
        (104, 20),
    ] {
        emf[offset..offset + 4].copy_from_slice(&u32::to_le_bytes(value));
    }
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, 8, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[255; 8 * 4 * 4])
        .unwrap();
    let picture = crate::pictures::Picture::from_emf(&emf, &png).unwrap();
    let mut doc = picture.document();
    let tree = crate::export::parse_svg(scene::svg(&doc)).unwrap();
    let mut primitives = Vec::new();
    collect(
        tree.root(),
        usvg::Transform::identity(),
        &mut primitives,
        &metafile_sources(&doc),
    )
    .unwrap();
    let source = primitives
        .iter()
        .find(|item| item["kind"] == "metafile")
        .unwrap();
    assert_eq!(
        STANDARD.decode(source["data"].as_str().unwrap()).unwrap(),
        emf
    );
    assert!(primitives.iter().all(|item| item["kind"] != "image"));
    doc.page_layout = Some(crate::pages::Layout::around(&doc));
    let print: Value = serde_json::from_slice(&print_snapshot(&doc).unwrap()).unwrap();
    let images: Vec<_> = print["primitives"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == "image")
        .collect();
    assert_eq!(images.len(), 1);
    assert_eq!(
        STANDARD
            .decode(images[0]["data"].as_str().unwrap())
            .unwrap(),
        picture.png()
    );
}
