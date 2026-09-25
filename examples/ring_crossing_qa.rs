//! Native drawings and transparent figure exports for ring crossing review.
use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    editing,
};
use std::{fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(std::env::args_os().nth(1).context("Output directory")?);
    fs::create_dir_all(&out)?;
    let mut sheet = Document::default();
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Ring crossing clearance</title><style>body{font:16px system-ui;background:#f3f5f4;padding:24px}main{display:grid;grid-template-columns:repeat(4,1fr);gap:16px}article{padding:20px;background:#fff}img{width:100%;height:300px;object-fit:contain}</style><h1>Ring crossing clearance</h1><p>Circles and partial curves share the bond crossing margin. Front/back order determines which stroke has a gap.</p><main>",
    );
    for index in 0..8 {
        let (tilted, partial, front) = (index & 1 != 0, index & 2 != 0, index & 4 == 0);
        let name = format!(
            "{} · {} · contact {}",
            if tilted { "Tilted" } else { "Flat" },
            if partial { "Partial" } else { "Circle" },
            if front { "in front" } else { "behind" }
        );
        let mut doc = Document::default();
        let metal = doc.add_atom("Fe", Point::default());
        doc.atom_mut(metal).context("Metal")?.charge = 2;
        doc = reshiki::hotkeys::atom_edit(&doc, metal, "J", 42.)
            .context("Arene shortcut")?
            .map_err(anyhow::Error::msg)?
            .0;
        let ring = doc
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .context("Attachment")?
            .centroid
            .clone();
        for bond in &mut doc.bonds {
            if !ring.contains(&bond.a) || !ring.contains(&bond.b) {
                bond.z_order = if front { 10 } else { -10 };
            }
        }
        if partial {
            let selected = ring
                .iter()
                .copied()
                .filter(|id| doc.atom(*id).is_some_and(|a| a.position.y >= -84.1))
                .collect::<Vec<_>>();
            reshiki::ring_arcs::toggle(&mut doc, &selected).map_err(anyhow::Error::msg)?;
        }
        if tilted {
            reshiki::projection::tilt(&mut doc, &ring, 45., true);
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
        let (lo, hi) = doc.bounds();
        let dest = Point::new((index % 4) as f32 * 185., (index / 4) as f32 * 230.);
        editing::append(&mut sheet, &doc, dest.offset(-(lo.x + hi.x) / 2., -lo.y));
        sheet.annotations.push(Annotation {
            id: sheet.next_id(),
            position: dest.offset(-65., 170.),
            text: name.replacen(" · ", " ", 1).replace(" · ", "\n"),
            format: reshiki::typography::TextFormat {
                style: reshiki::typography::TextStyle {
                    size_pt: 5.5,
                    ..Default::default()
                },
                ..Default::default()
            },
        });
        html.push_str(&format!("<article><h3>{name}</h3><img src='case-{index}.svg'><a href='case-{index}.rsk'>Editable drawing</a> · <a href='case-{index}.pdf'>PDF</a></article>"));
    }
    fs::write(
        out.join("sheet.png"),
        reshiki::export::drawing(&sheet, "png").map_err(anyhow::Error::msg)?,
    )?;
    fs::write(out.join("index.html"), format!("{html}</main>"))?;
    Ok(())
}
