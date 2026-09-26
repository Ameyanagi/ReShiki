//! Export the style-editor sample in both canvas modes for desktop QA.
use anyhow::Context;
use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    document::Point,
    document_styles::Preset,
    editing, exchange, export, rings,
};
use std::{fs, path::PathBuf};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("canvas_colors OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&root)?;
    let style = Preset::Jacs.style();
    let mut doc = rings::Preset::Regular.document(style.bond_length_world, false);
    let atom = doc.atoms.first().context("ring atom")?;
    let (id, p) = (atom.id, atom.position);
    let oxygen = doc.add_atom("O", p.offset(0., -style.bond_length_world));
    doc.add_bond(id, oxygen, 2, "plain");
    let ids = doc.all_ids();
    editing::transform_about(&mut doc, &ids, Point::default(), 1., 90.);
    doc.drawing_style = style;
    doc.atoms[2].element = "N".into();
    for palette in ColorTheme::ALL {
        palette.apply(&mut doc);
        for theme in CanvasTheme::ALL {
            doc.canvas_theme = theme;
            let dir = root
                .join(palette.to_string().to_lowercase())
                .join(theme.to_string().to_lowercase());
            fs::create_dir_all(&dir)?;
            fs::write(dir.join("sample.rsk"), serde_json::to_vec_pretty(&doc)?)?;
            let xml = exchange::drawing::write(&doc, Default::default())?;
            fs::write(
                dir.join("sample.cdx"),
                exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?,
            )?;
            fs::write(dir.join("sample.cdxml"), xml)?;
            for format in ["png", "svg", "pdf"] {
                fs::write(
                    dir.join(format!("reshiki.{format}")),
                    export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
                )?;
            }
            for format in ["png", "pdf", "svg"] {
                fs::write(
                    dir.join(format!("clipboard.{format}")),
                    export::clipboard_drawing(&doc, format).map_err(anyhow::Error::msg)?,
                )?;
            }
            let mut paged = doc.clone();
            paged.page_layout = Some(reshiki::pages::Layout::around(&paged));
            fs::write(
                dir.join("pages.pdf"),
                export::pages_pdf(&paged).map_err(anyhow::Error::msg)?,
            )?;
        }
    }
    if std::env::args().any(|arg| arg == "--copy") {
        let result = reshiki::clipboard::copy(Default::default(), doc, false)
            .await
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            result.external_editable,
            "Editable clipboard is unavailable: {:?}",
            result.notices
        );
        println!("Copied dark canvas with editable structure and image formats");
    }
    Ok(())
}
