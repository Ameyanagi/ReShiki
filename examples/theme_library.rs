//! Export portable presets, or validate a contributed theme without opening the UI.
use anyhow::Context;
use reshiki::{
    canvas_theme::{self, CanvasTheme, ColorTheme},
    color_contrast::contrast,
    document::{Document, Point},
    document_styles::{self, Preset},
    ring_fills::{self, RingFill},
    theme_files::{self, ThemeFile},
};
use std::path::PathBuf;
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args
        .next()
        .context("theme_library OUTPUT_DIRECTORY | --check FILE")?;
    if first == "--check" {
        let path = PathBuf::from(args.next().context("--check needs a theme file")?);
        let theme = theme_files::load(&path).map_err(anyhow::Error::msg)?;
        for mode in CanvasTheme::ALL {
            let mut doc = Document {
                canvas_theme: mode,
                ..Default::default()
            };
            theme.clone().apply(&mut doc).map_err(anyhow::Error::msg)?;
            let id = doc.add_atom("O", Point::default());
            doc.ring_fills = ring_fills::PALETTE
                .iter()
                .map(|&(_, color)| RingFill {
                    atoms: vec![id],
                    color,
                    fixed_color: false,
                })
                .collect();
            let mut minimum = 21_f64;
            for &element in reshiki::editing::ELEMENTS {
                if let Some(atom) = doc.atoms.first_mut() {
                    atom.element = element.into();
                }
                let atom = doc.atoms.first().context("missing audit atom")?;
                let ink = mode.color(canvas_theme::atom_color(&doc, atom));
                for bg in std::iter::once(mode.background()).chain(
                    doc.ring_fills
                        .iter()
                        .map(|f| canvas_theme::fill_color(&doc, f)),
                ) {
                    minimum = minimum.min(contrast(ink, bg));
                }
            }
            anyhow::ensure!(
                minimum >= 5.,
                "{} / {mode}: {minimum:.4}:1 is below 5:1",
                theme.name
            );
            println!(
                "{} / {mode}: all 118 elements on paper and overlapping ring fills >= {minimum:.4}:1",
                theme.name
            );
        }
        return Ok(());
    }
    let root = PathBuf::from(first);
    std::fs::create_dir_all(&root)?;
    for color_theme in ColorTheme::ALL {
        let theme = ThemeFile::capture(&Document {
            color_theme,
            ..Default::default()
        });
        theme_files::save(&root.join(format!("{}.reshiki-theme", theme.id)), &theme)
            .map_err(anyhow::Error::msg)?;
    }
    for theme in theme_files::bundled().map_err(anyhow::Error::msg)? {
        theme_files::save(&root.join(format!("{}.reshiki-theme", theme.id)), &theme)
            .map_err(anyhow::Error::msg)?;
    }
    for preset in Preset::ALL {
        for extension in ["reshiki-style", "cds"] {
            document_styles::save(
                &root.join(format!("{}.{}", preset.id(), extension)),
                &preset.style(),
            )
            .map_err(anyhow::Error::msg)?;
        }
    }
    println!("Exported themes and drawing styles to {}", root.display());
    Ok(())
}
