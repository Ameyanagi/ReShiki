//! Generate editable ring-color checks and publication export comparisons.
use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    editing,
    engine::{LocalEngine, Request},
    ring_fills,
};
use std::{fs, path::PathBuf};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: ring_fill_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&out)?;
    let mut aromatic = Document::default();
    let ids = editing::ring(&mut aromatic, Point::default(), 6, true, 0.);
    let mut tilted = aromatic.clone();
    reshiki::projection::tilt(&mut tilted, &ids, 55., true);
    let mut wide = aromatic.clone();
    editing::scale_axes_about(&mut wide, &ids, Point::default(), 1.6, 0.7);
    let chair = reshiki::rings::Preset::ChairUp.document(42., false);
    let sugar = reshiki::haworth::sugar_document(
        reshiki::haworth::Sugar::Glucose,
        reshiki::haworth::Anomer::Alpha,
        42.,
    )
    .map_err(anyhow::Error::msg)?;
    let engine = LocalEngine::default();
    let fused = engine
        .request(Request::import("smiles", "c1ccc2ccccc2c1"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Fused rings")?;
    let mut sheet = Document::default();
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Ring interior colors</title><style>body{font:16px system-ui;padding:32px;background:#f4f6f5}main{display:grid;grid-template-columns:repeat(3,1fr);gap:16px}article{background:white;padding:16px}img{width:100%;height:200px;object-fit:contain}</style><h1>Ring interior colors</h1><p>Select a ring, then choose Color → Ring interiors. Use a swatch or enter a hex color; Clear fill removes it. Bond colors and chemistry stay unchanged.</p><main>",
    );
    for (index, (title, mut doc, color)) in [
        ("Aromatic", aromatic, [255, 241, 174]),
        ("Tilted", tilted, [201, 224, 248]),
        ("Independent resize", wide, [198, 233, 220]),
        ("Chair", chair, [249, 207, 209]),
        ("Glucose", sugar, [226, 211, 245]),
        ("Fused rings", fused, [255, 241, 174]),
    ]
    .into_iter()
    .enumerate()
    {
        let ids = doc.all_ids();
        ring_fills::apply(&mut doc, &ids, Some(color));
        if index == 5
            && let Some(ring) = doc.ring_fills.first().cloned()
        {
            ring_fills::apply(&mut doc, &ring.atoms, Some([201, 224, 248]));
        }
        doc.validate().map_err(anyhow::Error::msg)?;
        fs::write(
            out.join(format!("case-{index}.rsk")),
            serde_json::to_vec_pretty(&doc)?,
        )?;
        for format in ["svg", "png", "pdf"] {
            fs::write(
                out.join(format!("case-{index}.{format}")),
                reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        for format in ["cdxml", "cdx"] {
            let mut req = Request::molecule("export", doc.clone());
            req.format = Some(format.into());
            let output = engine
                .request(req)
                .await
                .map_err(anyhow::Error::msg)?
                .output
                .context("Export")?;
            let bytes = if format == "cdx" {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.decode(output)?
            } else {
                output.into_bytes()
            };
            fs::write(out.join(format!("case-{index}.{format}")), bytes)?;
        }
        let (lo, hi) = doc.bounds();
        let dest = Point::new(
            (index % 3) as f32 * 360. + 180.,
            (index / 3) as f32 * 330. + 140.,
        );
        editing::append(
            &mut sheet,
            &doc,
            dest.offset(-(lo.x + hi.x) / 2., -(lo.y + hi.y) / 2.),
        );
        sheet.annotations.push(Annotation {
            id: sheet.next_id(),
            position: dest.offset(-100., 125.),
            text: title.into(),
            format: Default::default(),
        });
        html.push_str(&format!("<article><h3>{title}</h3><img src='case-{index}.svg'><a href='case-{index}.rsk'>Editable drawing</a> · <a href='case-{index}.pdf'>PDF</a> · <a href='case-{index}.cdxml'>CDXML</a> · <a href='case-{index}.cdx'>CDX</a></article>"));
    }
    fs::write(
        out.join("sheet.png"),
        reshiki::export::drawing(&sheet, "png").map_err(anyhow::Error::msg)?,
    )?;
    fs::write(out.join("sheet.rsk"), serde_json::to_vec_pretty(&sheet)?)?;
    html.push_str("</main>");
    fs::write(out.join("index.html"), html)?;
    println!("Wrote 6 ring-fill cases in 6 formats");
    Ok(())
}
