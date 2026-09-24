//! Reproducible visual cases; output must be an explicitly supplied directory.
use anyhow::Context;
use reshiki::{
    abbreviations::LabelAlignment,
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
            .context("Usage: label_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let engine = LocalEngine::default();
    let mut cases = Vec::new();
    let mut ammonia = Document::default();
    let pt = ammonia.add_atom("Pt", Point::default());
    for (element, point) in [
        ("N", Point::new(-70., 0.)),
        ("N", Point::new(0., -70.)),
        ("Cl", Point::new(70., 0.)),
        ("Cl", Point::new(0., 70.)),
    ] {
        let id = ammonia.add_atom(element, point);
        ammonia.add_bond(pt, id, 1, "plain");
        if element == "N" {
            ammonia =
                atom_text::apply(&ammonia, id, "NH3", Mode::Auto).map_err(anyhow::Error::msg)?;
        }
    }
    cases.push(("Typed NH3 · two ammines".to_string(), ammonia));
    for label in ["C2H5", "OCH3", "Boc"] {
        let mut source = Document::default();
        let n = source.add_atom("N", Point::new(-70., 0.));
        let a = source.add_atom("C", Point::default());
        source.add_bond(n, a, 1, "plain");
        source = atom_text::apply(&source, a, label, Mode::Auto).map_err(anyhow::Error::msg)?;
        for degrees in (0..360).step_by(45) {
            let mut doc = source.clone();
            let ids = doc.all_ids();
            editing::transform_about(&mut doc, &ids, Point::default(), 1., degrees as f32);
            cases.push((format!("{label} · {degrees}°"), doc));
        }
        for alignment in LabelAlignment::ALL {
            let mut doc = source.clone();
            doc.abbreviations.first_mut().context("Group")?.alignment = alignment;
            cases.push((format!("{label} · {alignment}"), doc));
        }
    }
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Label review</title><style>body{font:16px system-ui;max-width:1200px;margin:40px auto;background:#f4f6f5;color:#24302c}main{display:grid;grid-template-columns:repeat(3,1fr);gap:20px}article{padding:20px;background:white;border:1px solid #cdd7d2;border-radius:12px}img{width:100%;height:200px;object-fit:contain}a{color:#347c68}</style><h1>Atom and group labels</h1><p>Local visual checks. Each figure comes from an editable document and the application renderer.</p><main>",
    );
    let mut sheets = Vec::<Document>::new();
    for (index, (title, doc)) in cases.iter().enumerate() {
        let checked = engine
            .request(Request::molecule("analyze", doc.clone()))
            .await
            .map_err(anyhow::Error::msg)?;
        let formula = checked.analysis.context("Properties")?.formula;
        let mut displayed = doc.clone();
        reshiki::atom_labels::refresh_computed(
            &mut displayed,
            &checked.document.context("Checked drawing")?,
        );
        let doc = displayed;
        let stem = format!("case-{index:02}");
        fs::write(
            directory.join(format!("{stem}.rsk")),
            serde_json::to_vec_pretty(&doc)?,
        )?;
        for format in ["svg", "pdf", "png"] {
            fs::write(
                directory.join(format!("{stem}.{format}")),
                reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        html.push_str(&format!("<article><h3>{title}</h3><img src='{stem}.svg'><p>{formula}</p><a href='{stem}.rsk'>Editable drawing</a> · <a href='{stem}.pdf'>PDF</a></article>"));
        if index % 9 == 0 {
            sheets.push(Document::default());
        }
        let sheet = sheets.last_mut().context("Sheet")?;
        let dest = Point::new(
            (index % 3) as f32 * 280. + 130.,
            (index % 9 / 3) as f32 * 240. + 105.,
        );
        let (lo, hi) = doc.bounds();
        editing::append(
            sheet,
            &doc,
            Point::new(dest.x - (lo.x + hi.x) / 2., dest.y - (lo.y + hi.y) / 2.),
        );
        sheet.annotations.push(Annotation {
            id: sheet.next_id(),
            position: dest.offset(-100., 85.),
            text: title.clone(),
            format: reshiki::typography::TextFormat {
                style: reshiki::typography::TextStyle {
                    size_pt: 6.,
                    ..Default::default()
                },
                ..Default::default()
            },
        });
    }
    html.push_str("</main>");
    fs::write(directory.join("index.html"), html)?;
    for (index, doc) in sheets.iter().enumerate() {
        fs::write(
            directory.join(format!("sheet-{index}.png")),
            reshiki::export::drawing(doc, "png").map_err(anyhow::Error::msg)?,
        )?;
    }
    println!("Wrote {} checked label cases", cases.len());
    Ok(())
}
