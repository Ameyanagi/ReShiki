//! Generate editable before/after drawings for independent-axis resizing.
use anyhow::Context;
use reshiki::{
    atom_text::{self, Mode},
    document::{Annotation, Document, Point},
    editing,
    haworth::{Anomer, Sugar, sugar_document},
};
use std::{fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: resize_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let mut ring = Document::default();
    let ids = editing::ring(&mut ring, Point::default(), 6, true, 0.);
    let mut tilted = ring.clone();
    reshiki::projection::tilt(&mut tilted, &ids, 45., true);
    let sugar = sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
    let mut group = Document::default();
    let n = group.add_atom("N", Point::new(-60., -30.));
    let c = group.add_atom("C", Point::default());
    group.add_bond(n, c, 1, "plain");
    group = atom_text::apply(&group, c, "Boc", Mode::Auto).map_err(anyhow::Error::msg)?;
    group = atom_text::apply(&group, n, "NH2", Mode::Auto).map_err(anyhow::Error::msg)?;
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Independent resizing</title><style>body{font:16px system-ui;margin:32px;background:#f4f6f5;color:#24302c}main{display:grid;grid-template-columns:repeat(4,1fr);gap:16px}article{background:white;padding:16px}img{width:100%;height:220px;object-fit:contain}a{color:#347c68}</style><h1>Independent width and height</h1><p>Text and line width stay fixed. Geometry changes on the requested axis.</p><main>",
    );
    for (row, (title, source)) in [
        ("Aromatic ring", ring),
        ("Tilted ring", tilted),
        ("Glucose", sugar),
        ("Group label", group),
    ]
    .into_iter()
    .enumerate()
    {
        let mut sheet = Document::default();
        for (col, (name, x, y)) in [
            ("Original", 1., 1.),
            ("Width 160%", 1.6, 1.),
            ("Height 60%", 1., 0.6),
            ("Height 150%", 1., 1.5),
        ]
        .into_iter()
        .enumerate()
        {
            let mut doc = source.clone();
            let ids = doc.all_ids();
            editing::scale_axes_about(&mut doc, &ids, Point::default(), x, y);
            doc.validate().map_err(anyhow::Error::msg)?;
            let stem = format!("case-{row}-{col}");
            fs::write(
                directory.join(format!("{stem}.rsk")),
                serde_json::to_vec_pretty(&doc)?,
            )?;
            for format in ["svg", "png", "pdf"] {
                fs::write(
                    directory.join(format!("{stem}.{format}")),
                    reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
                )?;
            }
            html.push_str(&format!("<article><h3>{title} · {name}</h3><img src='{stem}.svg'><a href='{stem}.rsk'>Editable drawing</a> · <a href='{stem}.pdf'>PDF</a></article>"));
            let (lo, hi) = doc.bounds();
            let dest = Point::new(col as f32 * 320. + 160., 160.);
            editing::append(
                &mut sheet,
                &doc,
                Point::new(dest.x - (lo.x + hi.x) / 2., dest.y - (lo.y + hi.y) / 2.),
            );
            sheet.annotations.push(Annotation {
                id: sheet.next_id(),
                position: dest.offset(-100., 150.),
                text: name.into(),
                format: Default::default(),
            });
        }
        fs::write(
            directory.join(format!("sheet-{row}.png")),
            reshiki::export::drawing(&sheet, "png").map_err(anyhow::Error::msg)?,
        )?;
    }
    html.push_str("</main>");
    fs::write(directory.join("index.html"), html)?;
    println!("Wrote 16 resize cases");
    Ok(())
}
