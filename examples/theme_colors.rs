//! Regenerate the public color reference from the application's palette code.
//! cargo run --locked --example theme_colors
use anyhow::Context;
use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    color_contrast::{Oklch, Rgb},
    document::Document,
    editing::ELEMENTS,
    ring_fills,
    theme_files::{self, ThemeFile},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn color(rgb: Rgb) -> Value {
    let lch = Oklch::from_rgb(rgb);
    let rounded = |n: f64| (n * 1_000_000.).round() / 1_000_000.;
    json!({
        "rgb": rgb,
        "oklch": [rounded(lch.l), rounded(lch.c), rounded(lch.h.to_degrees().rem_euclid(360.))]
    })
}

fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let download = root.join("website/public/colors");
    std::fs::create_dir_all(&download)?;
    let mut themes = Vec::new();
    for theme in [
        ColorTheme::Presentation,
        ColorTheme::Pastel,
        ColorTheme::Jmol,
    ] {
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
                let fills: BTreeMap<_, _> = ring_fills::PALETTE
                    .iter()
                    .map(|&(name, key)| (name, color(ring_fills::palette_color(key, mode))))
                    .collect();
                (
                    mode.to_string().to_lowercase(),
                    json!({
                        "paper": color(mode.background()),
                        "ink": color(mode.color([0; 3])),
                        "elements": elements,
                        "ring_fills": fills
                    }),
                )
            })
            .collect();
        themes.push(json!({ "id": file.id, "name": file.name, "modes": modes }));
    }
    let catalog = json!({
        "version": 1,
        "reshiki_version": env!("CARGO_PKG_VERSION"),
        "rgb_space": "sRGB, 8-bit channels (0–255)",
        "oklch_units": "L: 0–1; C: chroma; h: degrees (0–360). Rounded to six decimals from final RGB.",
        "label_context": "Automatic labels on plain paper, with no ring fills or explicit object overrides.",
        "sources": [
            "https://jmol.sourceforge.net/jscolors/",
            "https://bottosson.github.io/posts/oklab/",
            "https://github.com/Ameyanagi/ReShiki/blob/main/src/canvas_theme.rs",
            "https://github.com/Ameyanagi/ReShiki/blob/main/src/color_contrast.rs"
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
        "Exported 3 themes × 2 modes × 118 elements to {}",
        path.display()
    );
    Ok(())
}
