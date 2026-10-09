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
fn windows_print_keeps_group_alpha_and_emf_reports_partial_alpha_loss() {
    let mut doc = Document::default();
    let ids: Vec<_> = [(0., 0., -20.), (80., 0., -20.), (120., 80., 20.)]
        .into_iter()
        .map(|(x, y, z)| {
            let id = doc.add_atom("C", crate::document::Point::new(x, y));
            doc.atom_mut(id).unwrap().depth = z;
            id
        })
        .collect();
    for pair in ids.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    crate::depth_appearance::set_rear_opacity(&mut doc, &ids, 0.5).unwrap();
    doc.page_layout = Some(crate::pages::Layout::around(&doc));
    let snapshot: Value = serde_json::from_slice(&print_snapshot(&doc).unwrap()).unwrap();
    assert!(
        snapshot["primitives"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["fill"][3] == 128 || p["stroke"]["color"][3] == 128)
    );
    let error = office_metafile(&doc).unwrap_err();
    assert!(error.contains("partial transparency"));
    assert!(error.contains("SVG"));
}
