//! Matched caption-formatting figures and an editable review drawing.
use anyhow::{Context, Result};
use reshiki::{
    document::{Annotation, Document, Point},
    typography::{self, TextFormat},
};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let out = PathBuf::from(std::env::args_os().nth(1).context("Output directory")?);
    fs::create_dir_all(&out)?;
    for automatic in [false, true] {
        let mut doc = Document::default();
        for (i, text) in ["C2H2", "C2H5OH", "Ca(OH)2", "Figure 2"]
            .into_iter()
            .enumerate()
        {
            let mut format = TextFormat::default();
            format.style.formula = automatic && typography::is_formula(text);
            doc.annotations.push(Annotation {
                id: doc.next_id(),
                position: Point::new(0., i as f32 * 60.),
                text: text.into(),
                format,
            });
        }
        let name = if automatic {
            "formula-after"
        } else {
            "formula-before"
        };
        fs::write(
            out.join(format!("{name}.rsk")),
            serde_json::to_vec_pretty(&doc)?,
        )?;
        for format in ["svg", "png", "pdf"] {
            fs::write(
                out.join(format!("{name}.{format}")),
                reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        if automatic {
            let mut review: Document =
                serde_json::from_str(include_str!("../docs/changes/fixtures/arene-bold-join.rsk"))?;
            reshiki::editing::append(&mut review, &doc, Point::new(130., -30.));
            fs::write(
                out.join("Drawing and text review.rsk"),
                serde_json::to_vec_pretty(&review)?,
            )?;
        }
    }
    Ok(())
}
