//! Make editable Haworth examples for native UI, figure, and ChemDraw checks.
use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    editing,
    templates::LIBRARY,
};
use std::{fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: haworth_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let mut gallery = Document::default();
    for (index, template) in LIBRARY
        .iter()
        .filter(|t| {
            t.group == "Carbohydrates"
                && (t.name.contains("glucopyranose") || t.name.contains("ribofuranose"))
        })
        .enumerate()
    {
        let name = template.name.replace(" · Haworth", "");
        let offset = Point::new(
            150. + (index % 2) as f32 * 330.,
            160. + (index / 2) as f32 * 300.,
        );
        editing::append(&mut gallery, &template.document, offset);
        gallery.annotations.push(Annotation {
            id: gallery.next_id(),
            position: offset.offset(-105., 125.),
            text: name.clone(),
            format: Default::default(),
        });
    }
    for template in LIBRARY.iter().filter(|t| t.group == "Carbohydrates") {
        let name = template.name.replace(" · Haworth", "");
        fs::write(
            directory.join(format!("{name}.rsk")),
            serde_json::to_vec_pretty(&template.document)?,
        )?;
        fs::write(
            directory.join(format!("{name}.mol")),
            reshiki::chemistry::molfile::write_document(&template.document)?,
        )?;
        for format in ["svg", "png"] {
            fs::write(
                directory.join(format!("{name}.{format}")),
                reshiki::export::drawing(&template.document, format).map_err(anyhow::Error::msg)?,
            )?;
        }
    }
    fs::write(
        directory.join("Haworth projections.rsk"),
        serde_json::to_vec_pretty(&gallery)?,
    )?;
    for format in ["svg", "png", "pdf"] {
        fs::write(
            directory.join(format!("haworth-projections.{format}")),
            reshiki::export::drawing(&gallery, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    Ok(())
}
