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
