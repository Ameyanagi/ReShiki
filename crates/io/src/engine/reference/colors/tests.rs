use super::*;
use serde_json::json;

#[test]
fn bridge_preserves_explicit_colors_and_does_not_rewrite_text_or_changed_bytes() {
    let original = json!({"document": {"bonds": [{"color": "#B43237"}], "annotations": [{"text": "#B43237"}]}});
    let mut message = original.clone();
    let colors = prepare(&mut message).unwrap();
    assert_eq!(
        message["document"]["bonds"][0]["color"],
        json!([180, 50, 55])
    );
    assert_eq!(message["document"]["annotations"][0]["text"], "#B43237");
    colors.restore(&mut message).unwrap();
    assert_eq!(message, original);

    let mut changed = original.clone();
    let colors = prepare(&mut changed).unwrap();
    changed["document"]["bonds"][0]["color"] = json!([180, 50, 56]);
    colors.restore(&mut changed).unwrap();
    assert_eq!(
        changed["document"]["bonds"][0]["color"],
        json!([180, 50, 56])
    );
    assert_ne!(changed, original);
}

#[test]
fn imported_swatch_bytes_remain_explicit_and_palette_requests_are_rejected() {
    let mut request = json!({"operation": "import", "format": "cdxml"});
    let colors = prepare(&mut request).unwrap();
    let mut response = json!({"document": {"bonds": [{"color": [180, 50, 55]}]}});
    colors.restore(&mut response).unwrap();
    assert_eq!(response["document"]["bonds"][0]["color"], "#B43237");

    let mut palette = json!({"document": {"bonds": [{"color": "red.strong"}]}});
    assert!(prepare(&mut palette).is_err());
    let mut black = json!({"document": {"bonds": [{"color": "#000000"}]}});
    let colors = prepare(&mut black).unwrap();
    colors.restore(&mut black).unwrap();
    assert_eq!(black["document"]["bonds"][0]["color"], "#000000");
}

#[test]
fn auxiliary_export_payloads_translate_without_rewriting_labels() {
    let mut request = json!({
        "atom_indicators": [{"text": "#B43237", "style": {"color": "#B43237"}}],
        "graphic_parts": {"71": [{"style": {"stroke": "#117E6C", "fill": "#000000"}}]}
    });
    prepare(&mut request).unwrap();
    assert_eq!(request["atom_indicators"][0]["text"], "#B43237");
    assert_eq!(
        request["atom_indicators"][0]["style"]["color"],
        json!([180, 50, 55])
    );
    assert_eq!(
        request["graphic_parts"]["71"][0]["style"]["stroke"],
        json!([17, 126, 108])
    );
    assert_eq!(
        request["graphic_parts"]["71"][0]["style"]["fill"],
        json!([0, 0, 0])
    );
}

#[test]
fn removed_and_reordered_objects_restore_by_identity_and_preserve_changed_rgb() {
    let mut request = json!({"document": {
        "atoms": [
            {"id": 1, "text_style": {"color": "#B43237"}},
            {"id": 2, "text_style": {"color": "#000000"}},
            {"id": 3, "text_style": {"color": "#B43237"}}
        ],
        "bonds": [{"a": 1, "b": 2, "color": "#117E6C"}, {"a": 2, "b": 3, "color": "#B43237"}],
        "annotations": [{"id": 10, "format": {"style": {"color": "#000000"}}}, {"id": 11, "text": "label"}]
    }});
    let colors = prepare(&mut request).unwrap();
    let mut response = json!({"document": {
        "atoms": [
            {"id": 3, "text_style": {"color": [180, 50, 56]}},
            {"id": 2, "text_style": {"color": [0, 0, 0]}},
            {"id": 4, "text_style": {"color": [0, 0, 0]}}
        ],
        "bonds": [{"a": 3, "b": 2, "color": [180, 50, 55]}, {"a": 1, "b": 2, "color": [17, 126, 108]}],
        "annotations": [{"id": 11, "text": "label"}, {"id": 10, "format": {"style": {"color": [0, 0, 0]}}}]
    }});
    colors.restore(&mut response).unwrap();
    assert_eq!(
        response["document"]["atoms"][0]["text_style"]["color"],
        json!([180, 50, 56])
    );
    assert_eq!(
        response["document"]["atoms"][1]["text_style"]["color"],
        "#000000"
    );
    assert_eq!(
        response["document"]["atoms"][2]["text_style"]["color"],
        json!([0, 0, 0])
    );
    assert_eq!(response["document"]["bonds"][0]["color"], "#B43237");
    assert_eq!(response["document"]["bonds"][1]["color"], "#117E6C");
    assert_eq!(
        response["document"]["annotations"][1]["format"]["style"]["color"],
        "#000000"
    );
}
