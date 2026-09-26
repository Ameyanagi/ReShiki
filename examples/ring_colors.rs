//! Render actual scene output for a ring-highlight review outside the repository.
use anyhow::Context;
use reshiki::{
    canvas_theme::{CanvasTheme, ColorTheme},
    document::Document,
    export, ring_fills,
};
use std::{fs, path::PathBuf};
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let root = PathBuf::from(
        args.first()
            .context("ring_colors OUTPUT_DIRECTORY INPUT.rsk")?,
    );
    let input = args.get(1).context("missing INPUT.rsk")?;
    let original: Document = serde_json::from_slice(&fs::read(input)?)?;
    fs::create_dir_all(&root)?;
    let mut cards = String::new();
    for canvas in CanvasTheme::ALL {
        for (name, color) in ring_fills::PALETTE {
            let mut doc = original.clone();
            doc.canvas_theme = canvas;
            ColorTheme::Presentation.apply(&mut doc);
            let ids = doc.all_ids();
            ring_fills::apply(&mut doc, &ids, Some(color));
            let file = format!(
                "{}-{}",
                canvas.to_string().to_lowercase(),
                name.to_lowercase()
            );
            fs::write(
                root.join(format!("{file}.rsk")),
                serde_json::to_vec_pretty(&doc)?,
            )?;
            fs::write(
                root.join(format!("{file}.svg")),
                export::drawing(&doc, "svg").map_err(anyhow::Error::msg)?,
            )?;
            fs::write(
                root.join(format!("{file}.png")),
                export::drawing(&doc, "png").map_err(anyhow::Error::msg)?,
            )?;
            cards.push_str(&format!("<figure class='{canvas}'><img src='{file}.svg'><figcaption>{name} · {canvas}</figcaption></figure>"));
        }
    }
    fs::write(
        root.join("index.html"),
        format!(
            "<!doctype html><meta charset='utf-8'><title>Ring highlight review</title><style>body{{font:14px system-ui;background:#20262c;color:#e4e9ed;padding:22px}}main{{display:grid;grid-template-columns:repeat(5,1fr);gap:14px}}figure{{margin:0;border-radius:8px;overflow:hidden;padding:18px}}.Light{{background:white;color:#222}}.Dark{{background:black;color:white}}img{{width:100%;height:230px;object-fit:contain}}figcaption{{padding-top:14px}}</style><h2>Ring highlights · Actual ReShiki renderer</h2><main>{cards}</main>"
        ),
    )?;
    println!("{}", root.join("index.html").display());
    Ok(())
}
