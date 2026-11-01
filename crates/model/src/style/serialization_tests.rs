use super::DrawingStyle;
use crate::document::Document;

#[test]
fn radical_order_defaults_to_iupac_and_only_serializes_the_ms_override() {
    let legacy = include_str!("../../../../assets/drawing_style.json");
    let mut style: DrawingStyle = serde_json::from_str(legacy).unwrap();
    assert!(!style.charge_before_radical);
    assert_eq!(style, DrawingStyle::default());
    assert!(
        serde_json::to_value(&style)
            .unwrap()
            .get("charge_before_radical")
            .is_none()
    );

    style.charge_before_radical = true;
    let encoded = serde_json::to_value(&style).unwrap();
    assert_eq!(encoded["charge_before_radical"], true);
    assert_eq!(
        serde_json::from_value::<DrawingStyle>(encoded).unwrap(),
        style
    );
    let doc = Document {
        drawing_style: style,
        ..Default::default()
    };
    let native = serde_json::to_vec(&doc).unwrap();
    assert_eq!(serde_json::from_slice::<Document>(&native).unwrap(), doc);
}
