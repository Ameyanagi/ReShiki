use super::*;
#[test]
fn legacy_ring_fills_migrate_by_key_and_fixed_colors_stay_exact() {
    let read = |json: serde_json::Value| serde_json::from_value::<RingFill>(json);
    for (key, hue) in PALETTE.map(|(_, key, hue)| (key, hue)) {
        let fill = read(serde_json::json!({"atoms": [1, 2, 3], "color": key})).unwrap();
        assert_eq!(fill.color, Color::Palette(hue, Row::Tint));
        let fixed = serde_json::json!({"atoms": [1, 2, 3], "color": key, "fixed_color": true});
        assert_eq!(read(fixed).unwrap().color, Color::Custom(key));
    }
    let other = read(serde_json::json!({"atoms": [1, 2, 3], "color": [245, 221, 165]}));
    assert_eq!(other.unwrap().color, Color::Custom([245, 221, 165]));
    let current = RingFill {
        atoms: vec![1, 2, 3],
        color: Color::Palette(Hue::Indigo, Row::Tint),
    };
    let json = serde_json::to_value(&current).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"atoms": [1, 2, 3], "color": "indigo.tint"})
    );
    assert_eq!(read(json).unwrap(), current);
    // A custom color equal to an old key stays exact after saving.
    let exact = RingFill {
        atoms: vec![1, 2, 3],
        color: Color::Custom([201, 224, 248]),
    };
    let json = serde_json::to_value(&exact).unwrap();
    assert_eq!(json["fixed_color"], true);
    assert_eq!(read(json).unwrap(), exact);
    let ink = RingFill {
        atoms: vec![1, 2, 3],
        color: Color::Ink,
    };
    assert_eq!(read(serde_json::to_value(&ink).unwrap()).unwrap(), ink);
    let black = RingFill {
        color: Color::Custom([0; 3]),
        ..ink
    };
    assert_eq!(read(serde_json::to_value(&black).unwrap()).unwrap(), black);
    assert!(read(serde_json::json!({"atoms": [1], "color": "sky"})).is_err());
}
