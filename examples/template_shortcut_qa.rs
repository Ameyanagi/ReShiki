//! Render new library entries and contextual group/ligand edits for visual review.
use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    editing,
    engine::{LocalEngine, Request},
    hotkeys,
    templates::LIBRARY,
};
use std::{fs, path::PathBuf};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: template_shortcut_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&out)?;
    let mut cases: Vec<_> = LIBRARY
        .iter()
        .filter(|t| matches!(t.group.as_str(), "Macrocycles" | "Ligands"))
        .map(|t| (t.name.clone(), t.document.clone()))
        .collect();
    let mut chain = Document::default();
    let a = chain.add_atom("C", Point::new(-72.74613, 0.));
    let b = chain.add_atom("C", Point::new(-36.373066, 21.));
    let c = chain.add_atom("C", Point::default());
    chain.add_bond(a, b, 1, "plain");
    chain.add_bond(b, c, 1, "plain");
    for (key, name) in [("M", "MgBr group"), ("Z", "Azide group")] {
        let (mut doc, _) = hotkeys::atom_edit(&chain, c, key, 42.)
            .context("Missing shortcut")?
            .map_err(anyhow::Error::msg)?;
        let group = doc.abbreviations.first().context("Group")?.members.clone();
        doc.expand_abbreviations(&group);
        cases.push((name.into(), doc));
    }
    for (key, element, count, charge, name) in [
        ("j", "Fe", 2, 2, "Two Cp ligands"),
        ("J", "Ru", 1, 0, "Arene attachment"),
    ] {
        let mut doc = Document::default();
        let metal = doc.add_atom(element, Point::default());
        doc.atom_mut(metal).context("Metal")?.charge = charge;
        for _ in 0..count {
            doc = hotkeys::atom_edit(&doc, metal, key, 42.)
                .context("Ligand shortcut")?
                .map_err(anyhow::Error::msg)?
                .0;
        }
        cases.push((name.into(), doc));
    }
    let engine = LocalEngine::default();
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Templates and shortcuts</title><style>body{font:16px system-ui;padding:30px;background:#f4f6f5}main{display:grid;grid-template-columns:repeat(4,1fr);gap:16px}article{background:white;padding:16px}img{width:100%;height:230px;object-fit:contain}</style><h1>Templates and shortcuts</h1><p>New Macrocycles and Ligands collections. At an atom: M inserts MgBr; Z inserts N₃; j adds Cp and J adds an arene attachment. Existing text inputs keep normal typing behavior.</p><main>",
    );
    let mut sheets = Vec::<Document>::new();
    for (i, (name, doc)) in cases.iter().enumerate() {
        doc.validate().map_err(anyhow::Error::msg)?;
        fs::write(
            out.join(format!("case-{i}.rsk")),
            serde_json::to_vec_pretty(doc)?,
        )?;
        for format in ["svg", "png", "pdf"] {
            fs::write(
                out.join(format!("case-{i}.{format}")),
                reshiki::export::drawing(doc, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        for format in ["cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(format.into());
            let response = match engine.request(request).await {
                Ok(response) => response,
                Err(error)
                    if doc.bonds.iter().any(|b| b.projection)
                        && (error.contains("front-bond emphasis")
                            || error.contains("hidden charge labels")) =>
                {
                    // Perspective examples retain their XYZ model in native
                    // files. Report the editable-format limit beside figures.
                    fs::write(
                        out.join(format!("case-{i}.{format}-unavailable.txt")),
                        error,
                    )?;
                    continue;
                }
                Err(error) => return Err(anyhow::Error::msg(error)),
            };
            let output = response.output.context("Export")?;
            let bytes = if format == "cdx" {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(output)?
            } else {
                output.into_bytes()
            };
            fs::write(out.join(format!("case-{i}.{format}")), bytes)?;
        }
        if i % 4 == 0 {
            sheets.push(Document::default());
        }
        let sheet = sheets.last_mut().context("Sheet")?;
        let (lo, hi) = doc.bounds();
        let scale = (260. / (hi.x - lo.x).max(hi.y - lo.y)).min(1.);
        let mut drawing = doc.clone();
        let ids = drawing.all_ids();
        editing::transform_about(&mut drawing, &ids, Point::default(), scale, 0.);
        let (lo, hi) = drawing.bounds();
        let dest = Point::new((i % 4) as f32 * 340. + 170., 160.);
        editing::append(
            sheet,
            &drawing,
            dest.offset(-(lo.x + hi.x) / 2., -(lo.y + hi.y) / 2.),
        );
        sheet.annotations.push(Annotation {
            id: sheet.next_id(),
            position: dest.offset(-120., 170.),
            text: if name == "1,2-Bis(diphenylphosphino)ethane" {
                "dppe".into()
            } else {
                name.clone()
            },
            format: Default::default(),
        });
        html.push_str(&format!("<article><h3>{name}</h3><img src='case-{i}.svg'><a href='case-{i}.rsk'>Editable</a> · <a href='case-{i}.pdf'>PDF</a> · <a href='case-{i}.cdxml'>CDXML</a></article>"));
    }
    for (i, sheet) in sheets.iter().enumerate() {
        fs::write(
            out.join(format!("sheet-{i}.png")),
            reshiki::export::drawing(sheet, "png").map_err(anyhow::Error::msg)?,
        )?;
    }
    html.push_str("</main>");
    fs::write(out.join("index.html"), html)?;
    println!("Wrote {} cases", cases.len());
    Ok(())
}
