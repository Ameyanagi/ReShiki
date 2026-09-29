//! Render real editable TBDPS / OTBDPS examples for issue #46.
use anyhow::Context;
use reshiki::{
    atom_text::{self, Mode},
    document::{Annotation, Document, Point},
    editing,
    engine::{LocalEngine, Request},
};
use std::{fs, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: tbdps_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let engine = LocalEngine::default();
    let source = engine
        .request(Request::import_smiles("CCC"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Propane")?;
    fs::write(
        directory.join("source.rsk"),
        serde_json::to_vec_pretty(&source)?,
    )?;
    let end = source.atoms.last().context("Endpoint")?.id;
    let mut sheet = Document::default();
    let mut properties = String::new();
    for (row, label) in ["TBDPS", "OTBDPS"].iter().enumerate() {
        let doc = atom_text::apply(&source, end, label, Mode::Auto).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            doc.abbreviation(end).is_some(),
            "Missing {label} chemical definition"
        );
        let checked = engine
            .request(Request::molecule("analyze", doc))
            .await
            .map_err(anyhow::Error::msg)?;
        let analysis = checked.analysis.context("Properties")?;
        anyhow::ensure!(
            analysis.formula
                == if *label == "OTBDPS" {
                    "C18H24OSi"
                } else {
                    "C18H24Si"
                },
            "Unexpected {label} composition"
        );
        properties.push_str(&format!(
            "{label}: {} · {}\n",
            analysis.formula, analysis.inchikey
        ));
        let doc = checked.document.context("Checked drawing")?;
        for (col, caption) in ["Left bond", "Right bond", "Expanded"].iter().enumerate() {
            let mut case = doc.clone();
            if col == 1 {
                let ids = case.all_ids();
                editing::transform(&mut case, &ids, editing::Transform::FlipHorizontal);
            } else if col == 2 {
                case.expand_abbreviations(&[end]);
            }
            let name = format!("{}-{col}", label.to_lowercase());
            fs::write(
                directory.join(format!("{name}.rsk")),
                serde_json::to_vec_pretty(&case)?,
            )?;
            fs::write(
                directory.join(format!("{name}.svg")),
                reshiki::export::drawing(&case, "svg").map_err(anyhow::Error::msg)?,
            )?;
            let (lo, hi) = reshiki::scene::selection_bounds(&case, &case.all_ids())
                .context("Drawing bounds")?;
            let center = Point::new(190. + col as f32 * 370., 140. + row as f32 * 350.);
            editing::append(
                &mut sheet,
                &case,
                Point::new(center.x - (lo.x + hi.x) / 2., center.y - (lo.y + hi.y) / 2.),
            );
            for (offset, text, size) in [
                (-145., format!("{label} · {caption}"), 10.),
                (135., analysis.formula.clone(), 9.),
            ] {
                sheet.annotations.push(Annotation {
                    id: sheet.next_id(),
                    position: center.offset(-140., offset),
                    text,
                    format: reshiki::typography::TextFormat {
                        style: reshiki::typography::TextStyle {
                            size_pt: size,
                            formula: offset > 0.,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                });
            }
        }
    }
    fs::write(
        directory.join("gallery.rsk"),
        serde_json::to_vec_pretty(&sheet)?,
    )?;
    for format in ["png", "svg"] {
        fs::write(
            directory.join(format!("gallery.{format}")),
            reshiki::export::drawing(&sheet, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    fs::write(directory.join("properties.txt"), &properties)?;
    print!("{properties}");
    Ok(())
}
