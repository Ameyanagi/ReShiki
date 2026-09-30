//! The published palette is a reusable data contract, not a second color engine.
use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    color_contrast::{Oklch, Rgb},
    document::Document,
    editing::ELEMENTS,
    palette::{Palette, Row},
    ring_fills,
    theme_files::{self, ThemeFile},
};
use serde_json::Value;

fn check_color(value: &Value, expected: Rgb) {
    let rgb: Rgb = serde_json::from_value(value["rgb"].clone()).unwrap();
    assert_eq!(rgb, expected);
    let [l, c, h]: [f64; 3] = serde_json::from_value(value["oklch"].clone()).unwrap();
    let actual = Oklch::from_rgb(rgb);
    assert!((l - actual.l).abs() <= 0.00000051);
    assert!((c - actual.c).abs() <= 0.00000051);
    assert!((h - actual.h.to_degrees().rem_euclid(360.)).abs() <= 0.00000051);
    assert_eq!(
        Oklch {
            l,
            c,
            h: h.to_radians()
        }
        .to_rgb(),
        rgb
    );
}

#[test]
fn public_color_reference_matches_the_application_and_portable_themes() {
    let catalog: Value =
        serde_json::from_str(include_str!("../website/src/data/theme-colors.json")).unwrap();
    assert_eq!(catalog["themes"].as_array().unwrap().len(), 3);
    for (index, theme) in [
        ColorTheme::Presentation,
        ColorTheme::Pastel,
        ColorTheme::Jmol,
    ]
    .into_iter()
    .enumerate()
    {
        let exported = &catalog["themes"][index];
        assert_eq!(exported["name"], theme.to_string());
        assert_eq!(exported["id"], theme.to_string().to_lowercase());
        for mode in CanvasTheme::ALL {
            let palette = &exported["modes"][mode.to_string().to_lowercase()];
            check_color(&palette["paper"], mode.background());
            check_color(&palette["ink"], mode.color([0; 3]));
            let elements = palette["elements"].as_array().unwrap();
            assert_eq!(elements.len(), 118);
            for (i, &symbol) in ELEMENTS.iter().enumerate() {
                let element = &elements[i];
                assert_eq!(element["symbol"], symbol);
                assert_eq!(element["number"], i + 1);
                check_color(&element["label"], theme.element_color(symbol, mode));
            }
            // Ring interiors use the theme's Tint row under the earlier slot names.
            let tints = Palette::new(theme.tones(mode), Default::default(), mode);
            assert_eq!(palette["ring_fills"].as_object().unwrap().len(), 5);
            for (name, _, hue) in ring_fills::PALETTE {
                check_color(&palette["ring_fills"][name], tints.swatch(hue, Row::Tint));
            }
        }
        let file = ThemeFile::capture(&Document {
            color_theme: theme,
            ..Default::default()
        });
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("website/public/colors/{}.reshiki-theme", file.id));
        assert_eq!(theme_files::load(&path).unwrap(), file);
    }
}
