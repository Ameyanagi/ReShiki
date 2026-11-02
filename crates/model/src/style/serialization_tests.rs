use super::{DrawingStyle, HydrogenIsotopes};
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

#[test]
fn hydrogen_isotopes_default_to_auto_and_roundtrip_without_changing_atoms() {
    let source = include_str!("../../../../assets/drawing_style.json");
    let legacy: DrawingStyle = serde_json::from_str(source).unwrap();
    assert_eq!(legacy.hydrogen_isotopes, HydrogenIsotopes::Auto);
    assert_eq!(
        serde_json::to_string_pretty(&legacy).unwrap(),
        source.trim_end()
    );

    for (mode, value) in [
        (HydrogenIsotopes::Auto, "auto"),
        (HydrogenIsotopes::Letters, "letters"),
        (HydrogenIsotopes::MassNumbers, "mass_numbers"),
    ] {
        let style = DrawingStyle {
            hydrogen_isotopes: mode,
            ..legacy.clone()
        };
        let encoded = serde_json::to_value(&style).unwrap();
        assert_eq!(encoded.get("hydrogen_isotopes").is_none(), mode.is_auto());
        if !mode.is_auto() {
            assert_eq!(encoded["hydrogen_isotopes"], value);
        }
        assert_eq!(
            serde_json::from_value::<DrawingStyle>(encoded).unwrap(),
            style
        );
        let mut doc = Document {
            drawing_style: style,
            ..Default::default()
        };
        let id = doc.add_atom("H", crate::document::Point::new(0., 0.));
        doc.atom_mut(id).unwrap().isotope = 2;
        let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reopened.drawing_style, doc.drawing_style);
        assert_eq!(reopened.atoms, doc.atoms);
        assert_eq!(reopened.bonds, doc.bonds);
        assert_eq!(reopened.atoms[0].element, "H");
        assert_eq!(reopened.atoms[0].isotope, 2);
    }

    let mut explicit = serde_json::to_value(&legacy).unwrap();
    explicit["hydrogen_isotopes"] = "auto".into();
    assert_eq!(
        serde_json::from_value::<DrawingStyle>(explicit.clone()).unwrap(),
        legacy
    );
    for invalid in [
        serde_json::json!("D/T"),
        serde_json::json!(true),
        serde_json::json!(3),
    ] {
        explicit["hydrogen_isotopes"] = invalid;
        assert!(serde_json::from_value::<DrawingStyle>(explicit.clone()).is_err());
    }
}
