//! Reproducible application-renderer fixtures for QA issues #494/#495/#497/#506.
use anyhow::{Context, anyhow};
use reshiki::{
    arrow_anchors::{self, Pick},
    arrows::{ArrowStyle, Preset},
    atom_text::{self, Mode},
    document::{Document, Point},
    editing,
    engine::{LocalEngine, Request},
    graphics::{GraphicKind, GraphicStyle},
    scientific::{self, Drawing, Phase, SymbolKind},
};
use std::{fs, path::PathBuf};

fn symbol(doc: &mut Document, atom: u64, kind: SymbolKind) -> anyhow::Result<()> {
    let at = doc.atom(atom).context("Attachment atom")?.position;
    Drawing {
        kind: GraphicKind::Symbol(kind),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: true,
        snap_orbitals: true,
    }
    .place(doc, at, at, false, 10.)
    .map_err(anyhow::Error::msg)?;
    Ok(())
}
fn orient(doc: &mut Document, atom: u64, neighbour: u64) -> anyhow::Result<()> {
    let center = doc.atom(atom).context("Center")?.position;
    let other = doc.atom(neighbour).context("Neighbour")?.position;
    let degrees = -90. - (other.y - center.y).atan2(other.x - center.x).to_degrees();
    let ids = doc.all_ids();
    editing::transform_about(doc, &ids, center, 1., degrees);
    Ok(())
}
async fn import(engine: &LocalEngine, text: &str) -> anyhow::Result<Document> {
    let format = if text.contains('>') { "rsmi" } else { "smiles" };
    engine
        .request(Request::import(format, text))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Imported drawing")
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: drawing_geometry_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let engine = LocalEngine::default();
    let mut cases = Vec::new();
    let mut nitro = import(&engine, "C[N+](=O)[O-]").await?;
    let n = nitro
        .atoms
        .iter()
        .find(|a| a.element == "N")
        .context("N")?
        .id;
    let o = nitro.bonds.iter().find(|b| b.order == 2).context("N=O")?.b;
    orient(&mut nitro, n, o)?;
    nitro.atom_mut(n).context("N")?.charge = 0;
    cases.push(("494-nitro-input".into(), nitro.clone()));
    symbol(&mut nitro, n, SymbolKind::CirclePlus)?;
    cases.push(("494-nitro".into(), nitro));
    let mut methylamine = import(&engine, "CN").await?;
    let n = methylamine
        .atoms
        .iter()
        .find(|a| a.element == "N")
        .context("N")?
        .id;
    symbol(&mut methylamine, n, SymbolKind::CirclePlus)?;
    cases.push(("494-methylamine".into(), methylamine));
    for from_left in [true, false] {
        for text in ["MOrph", "M", "NC"] {
            let mut source = Document::default();
            let id = source.add_atom("C", Point::default());
            let other = source.add_atom("C", Point::new(if from_left { -42. } else { 42. }, 0.));
            source.add_bond(id, other, 1, "plain");
            let doc =
                atom_text::apply(&source, id, text, Mode::Text).map_err(anyhow::Error::msg)?;
            cases.push((
                format!("497-{text}-{}", if from_left { "left" } else { "right" }),
                doc,
            ));
        }
    }
    for (name, text) in [
        (
            "495-agents",
            "[CH3:1][CH:2]=[O:3]>[BH4-].[Na+]>[CH3:1][CH2:2][OH:3]",
        ),
        ("495-ions", "[BH4-].[Na+]>>[AlH4-].[Li+]"),
    ] {
        cases.push((name.into(), import(&engine, text).await?));
    }
    let mut acetone = import(&engine, "CC(C)=O").await?;
    let bond = acetone.bonds.iter().find(|b| b.order == 2).context("C=O")?;
    let (c, o) = if acetone.atom(bond.a).context("C")?.element == "O" {
        (bond.b, bond.a)
    } else {
        (bond.a, bond.b)
    };
    orient(&mut acetone, c, o)?;
    let methoxide = import(&engine, "C[O-]").await?;
    let oxygen = methoxide
        .atoms
        .iter()
        .find(|a| a.element == "O")
        .context("O")?
        .position;
    let center = acetone.atom(c).context("Carbonyl")?.position;
    let source_ids = editing::append(
        &mut acetone,
        &methoxide,
        Point::new(center.x - 135. - oxygen.x, center.y - 35. - oxygen.y),
    );
    let source = acetone
        .atoms
        .iter()
        .find(|a| source_ids.contains(&a.id) && a.element == "O")
        .context("Methoxide O")?
        .id;
    // Position a chemically sensible lone pair toward the acceptor.
    scientific::attach(
        acetone.atom_mut(source).context("Source")?,
        SymbolKind::LonePair,
        Point::new(25., 0.),
    )
    .map_err(anyhow::Error::msg)?;
    let mark_at = acetone
        .atom(source)
        .context("Source")?
        .position
        .offset(25., 0.);
    let pick = arrow_anchors::pick(&acetone, mark_at, 2.).context("Lone pair pick")?;
    let arrow = arrow_anchors::create(
        &mut acetone,
        &pick,
        &Pick::Atom(c),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    )
    .map_err(anyhow::Error::msg)?;
    cases.push(("506-default".into(), acetone.clone()));
    let end = acetone
        .arrows
        .iter()
        .find(|a| a.id == arrow)
        .context("Arrow")?
        .end;
    arrow_anchors::edit_handle(&mut acetone, arrow, 4, end.offset(0., -20.));
    cases.push(("506-aimed".into(), acetone));
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Drawing geometry QA</title><style>body{font:16px system-ui;margin:32px;background:#f4f5f6}main{display:grid;grid-template-columns:repeat(2,minmax(300px,1fr));gap:24px}article{padding:20px;background:white}img{width:100%;height:280px;object-fit:contain}</style><h1>Drawing geometry QA · application renderer</h1><main>",
    );
    for (name, doc) in cases {
        doc.validate().map_err(|e| anyhow!("{name}: {e}"))?;
        fs::write(
            directory.join(format!("{name}.rsk")),
            doc.file_json().map_err(anyhow::Error::msg)?,
        )?;
        for format in ["svg", "png"] {
            fs::write(
                directory.join(format!("{name}.{format}")),
                reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        html.push_str(&format!("<article><h2>{name}</h2><img src='{name}.svg'><a href='{name}.rsk'>Editable input</a></article>"));
    }
    html.push_str("</main>");
    fs::write(directory.join("index.html"), html)?;
    Ok(())
}
