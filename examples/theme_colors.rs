//! Regenerate the public color reference from the application's palette code.
//! cargo run --locked --example theme_colors
use anyhow::Context;
use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    color_contrast::{Oklch, Rgb},
    document::Document,
    editing::ELEMENTS,
    palette::{Hue, Palette, Row},
    theme_files::{self, ThemeFile},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

/// The reference describes Nightly builds until the palette ships in a stable
/// release; set this to false when regenerating for that release.
const NIGHTLY: bool = false;

fn color(rgb: Rgb) -> Value {
    let lch = Oklch::from_rgb(rgb);
    let rounded = |n: f64| (n * 1_000_000.).round() / 1_000_000.;
    json!({
        "rgb": rgb,
        "oklch": [rounded(lch.l), rounded(lch.c), rounded(lch.h.to_degrees().rem_euclid(360.))]
    })
}

/// One palette row: its shared OKLCH tone and the eight hues in slot order.
fn row(palette: &Palette, row: Row, [lightness, target_chroma]: [f64; 2]) -> Value {
    let colors: Vec<_> = Hue::ALL
        .into_iter()
        .map(|hue| {
            let mut value = color(palette.swatch(hue, row));
            value["hue"] = json!(hue.name());
            value["degrees"] = json!(hue.default_degrees());
            value
        })
        .collect();
    json!({ "lightness": lightness, "target_chroma": target_chroma, "colors": colors })
}

fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let download = root.join("website/public/colors");
    std::fs::create_dir_all(&download)?;
    let mut themes = Vec::new();
    for theme in ColorTheme::ALL {
        let file = ThemeFile::capture(&Document {
            color_theme: theme,
            ..Default::default()
        });
        theme_files::save(&download.join(format!("{}.reshiki-theme", file.id)), &file)
            .map_err(anyhow::Error::msg)?;
        let modes: BTreeMap<_, _> = CanvasTheme::ALL
            .into_iter()
            .map(|mode| {
                let elements: Vec<_> = ELEMENTS
                    .iter()
                    .enumerate()
                    .map(|(i, symbol)| {
                        json!({
                            "number": i + 1,
                            "symbol": symbol,
                            "label": color(theme.element_color(symbol, mode))
                        })
                    })
                    .collect();
                let tones = file.tones(mode);
                let palette = Palette::new(tones, file.hues, mode);
                (
                    mode.to_string().to_lowercase(),
                    json!({
                        "paper": color(mode.background()),
                        "ink": color(mode.color([0; 3])),
                        "strong": row(&palette, Row::Strong, tones.strong),
                        "tint": row(&palette, Row::Tint, tones.tint),
                        "elements": elements
                    }),
                )
            })
            .collect();
        themes.push(json!({ "id": file.id, "name": file.name, "modes": modes }));
    }
    let version = env!("CARGO_PKG_VERSION");
    let catalog = json!({
        "version": 2,
        "reshiki_version": if NIGHTLY { format!("{version}-nightly") } else { version.into() },
        "rgb_space": "sRGB, 8-bit channels (0–255)",
        "oklch_units": "L: 0–1; C: chroma; h: degrees (0–360). Rounded to six decimals from final RGB.",
        "label_context": "Automatic labels on plain paper, with no ring fills or explicit object overrides.",
        "palette_context": "Strong colors bonds, text and strokes; Tint colors fills, ring interiors and highlight boxes. Each row shares one OKLCH lightness and target chroma per theme and canvas. Default hue angles.",
        "sources": [
            "https://jmol.sourceforge.net/jscolors/",
            "https://bottosson.github.io/posts/oklab/",
            "https://github.com/Ameyanagi/ReShiki/blob/main/crates/model/src/canvas_theme.rs",
            "https://github.com/Ameyanagi/ReShiki/blob/main/crates/model/src/palette.rs",
            "https://github.com/Ameyanagi/ReShiki/blob/main/crates/model/src/color_contrast.rs"
        ],
        "themes": themes
    });
    let path = root.join("website/src/data/theme-colors.json");
    std::fs::create_dir_all(path.parent().context("Missing data directory")?)?;
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&catalog)?),
    )?;
    println!(
        "Exported {} themes × 2 modes × 118 elements and 17 palette colors to {}",
        ColorTheme::ALL.len(),
        path.display()
    );
    Ok(())
}
