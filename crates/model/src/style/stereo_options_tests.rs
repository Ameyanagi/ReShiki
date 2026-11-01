use super::DrawingStyle;
use crate::document::Document;

#[test]
fn old_styles_keep_their_bytes_and_default_identity() {
    let source = include_str!("../../../../assets/drawing_style.json");
    let style: DrawingStyle = serde_json::from_str(source).unwrap();
    assert!(!style.two_wedge_stereocentres);
    assert!(style.ring_fusion_hydrogens);
    assert!(!style.wavy_unknown_alkenes);
    assert!(style.is_default());
    assert_eq!(
        serde_json::to_string_pretty(&style).unwrap(),
        source.trim_end()
    );
    let mut explicit: serde_json::Value = serde_json::from_str(source).unwrap();
    explicit["two_wedge_stereocentres"] = false.into();
    explicit["ring_fusion_hydrogens"] = true.into();
    explicit["wavy_unknown_alkenes"] = false.into();
    let explicit: DrawingStyle = serde_json::from_value(explicit).unwrap();
    assert!(explicit.is_default());
    assert_eq!(explicit, style);

    let original = Document::default().file_json().unwrap();
    assert!(!String::from_utf8_lossy(&original).contains("drawing_style"));
    let reopened = Document::from_json(&original).unwrap();
    assert!(reopened.drawing_style.is_default());
    assert_eq!(reopened.file_json().unwrap(), original);
}

#[test]
fn stereochemistry_options_survive_native_documents_and_style_json() {
    for two_wedge in [false, true] {
        for fusion_h in [false, true] {
            for wavy in [false, true] {
                let style = DrawingStyle {
                    two_wedge_stereocentres: two_wedge,
                    ring_fusion_hydrogens: fusion_h,
                    wavy_unknown_alkenes: wavy,
                    ..Default::default()
                };
                style.validate().unwrap();
                let encoded = serde_json::to_vec(&style).unwrap();
                assert_eq!(
                    serde_json::from_slice::<DrawingStyle>(&encoded).unwrap(),
                    style
                );
                let fields: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
                assert_eq!(fields.get("two_wedge_stereocentres").is_some(), two_wedge);
                assert_eq!(fields.get("ring_fusion_hydrogens").is_some(), !fusion_h);
                assert_eq!(fields.get("wavy_unknown_alkenes").is_some(), wavy);
                let doc = Document {
                    drawing_style: style.clone(),
                    ..Default::default()
                };
                assert_eq!(
                    Document::from_json(&doc.file_json().unwrap())
                        .unwrap()
                        .drawing_style,
                    style
                );
            }
        }
    }
}
