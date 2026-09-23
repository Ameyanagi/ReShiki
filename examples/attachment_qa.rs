//! Generate isolated editable files for ChemDraw/ReShiki GUI interoperability QA.
use reshiki::{
    attachments::{self, Kind},
    document::{Document, Point},
    editing, exchange,
};
use std::{fs, path::PathBuf};

fn ring(doc: &mut Document, n: usize, center: Point, radius: f32, cp: bool) -> Vec<u64> {
    let mut ids = Vec::new();
    for i in 0..n {
        let angle = i as f32 * std::f32::consts::TAU / n as f32 - std::f32::consts::FRAC_PI_2;
        let id = doc.add_atom(
            "C",
            center.offset(radius * angle.cos(), radius * angle.sin()),
        );
        if cp
            && i == 0
            && let Some(a) = doc.atom_mut(id)
        {
            a.charge = -1;
            a.explicit_h = 1;
            a.no_implicit = true;
        }
        ids.push(id);
    }
    for (i, (&a, &b)) in ids.iter().zip(ids.iter().cycle().skip(1)).enumerate() {
        doc.add_bond(
            a,
            b,
            if if cp { i == 1 || i == 3 } else { i % 2 == 0 } {
                2
            } else {
                1
            },
            "plain",
        );
    }
    ids
}

fn save(dir: &std::path::Path, name: &str, doc: &Document) -> anyhow::Result<()> {
    doc.validate().map_err(anyhow::Error::msg)?;
    fs::write(
        dir.join(format!("{name}.rsk")),
        serde_json::to_string_pretty(doc)?,
    )?;
    let xml = exchange::drawing::write(doc, Default::default())?;
    fs::write(dir.join(format!("{name}.cdxml")), &xml)?;
    fs::write(
        dir.join(format!("{name}.cdx")),
        exchange::to_cdx(&xml).map_err(anyhow::Error::msg)?,
    )?;
    if let Ok(mol) = reshiki::chemistry::molfile::write_document(doc) {
        fs::write(dir.join(format!("{name}.mol")), mol)?;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("Usage: attachment_qa OUTPUT_DIRECTORY"))?,
    );
    fs::create_dir_all(&dir)?;
    let mut arene = Document::default();
    let ids = ring(&mut arene, 6, Point::new(100., 80.), 42., false);
    let point =
        attachments::add(&mut arene, &ids, Kind::MultiCenter).map_err(anyhow::Error::msg)?;
    let cr = arene.add_atom("Cr", Point::new(145., 160.));
    arene.add_bond(point, cr, 1, "plain");
    save(&dir, "eta6-arene", &arene)?;
    let mut variable = arene.clone();
    variable
        .atom_mut(point)
        .ok_or_else(|| anyhow::anyhow!("point"))?
        .attachment = Some(Kind::Variable);
    variable
        .atom_mut(cr)
        .ok_or_else(|| anyhow::anyhow!("substituent"))?
        .element = "O".into();
    let me = variable.add_atom("C", Point::new(181., 181.));
    variable.add_bond(cr, me, 1, "plain");
    save(&dir, "variable-arene", &variable)?;
    let mut allyl = Document::default();
    let a = allyl.add_atom("C", Point::new(58., 80.));
    let b = allyl.add_atom("C", Point::new(100., 56.));
    let c = allyl.add_atom("C", Point::new(142., 80.));
    allyl.add_bond(a, b, 2, "plain");
    allyl.add_bond(b, c, 1, "plain");
    allyl
        .atom_mut(c)
        .ok_or_else(|| anyhow::anyhow!("allyl"))?
        .charge = -1;
    let point =
        attachments::add(&mut allyl, &[a, b, c], Kind::MultiCenter).map_err(anyhow::Error::msg)?;
    let pd = allyl.add_atom("Pd", Point::new(100., 142.));
    allyl
        .atom_mut(pd)
        .ok_or_else(|| anyhow::anyhow!("Pd"))?
        .charge = 2;
    allyl.add_bond(point, pd, 1, "plain");
    save(&dir, "eta3-allyl", &allyl)?;
    let mut sandwich = Document::default();
    let top = ring(&mut sandwich, 5, Point::new(100., 60.), 42., true);
    let bottom = ring(&mut sandwich, 5, Point::new(100., 210.), 42., true);
    let top =
        attachments::add(&mut sandwich, &top, Kind::MultiCenter).map_err(anyhow::Error::msg)?;
    let bottom =
        attachments::add(&mut sandwich, &bottom, Kind::MultiCenter).map_err(anyhow::Error::msg)?;
    let fe = sandwich.add_atom("Fe", Point::new(100., 135.));
    sandwich
        .atom_mut(fe)
        .ok_or_else(|| anyhow::anyhow!("Fe"))?
        .charge = 2;
    sandwich.add_bond(top, fe, 1, "plain");
    sandwich.add_bond(bottom, fe, 1, "plain");
    save(&dir, "ferrocene", &sandwich)?;
    let mut groups = Document::default();
    for (i, label) in [
        "Boc", "Cbz", "Fmoc", "Ac", "Ts", "Ms", "Me", "Et", "tBu", "Ph", "Bn", "OMe",
    ]
    .into_iter()
    .enumerate()
    {
        let mut doc = Document::default();
        let c = doc.add_atom("C", Point::new(0., 0.));
        let n = doc.add_atom("N", Point::new(36., 21.));
        let target = doc.add_atom("C", Point::new(72., 0.));
        doc.add_bond(c, n, 1, "plain");
        doc.add_bond(n, target, 1, "plain");
        let doc = reshiki::atom_text::apply(&doc, target, label, reshiki::atom_text::Mode::Group)
            .map_err(anyhow::Error::msg)?;
        save(&dir, &format!("group-{label}"), &doc)?;
        editing::append(
            &mut groups,
            &doc,
            Point::new((i % 3) as f32 * 380., (i / 3) as f32 * 220.),
        );
    }
    save(&dir, "common-groups", &groups)?;
    for label in reshiki::ligands::LABELS {
        let mut doc = Document::default();
        let fe = doc.add_atom("Fe", Point::new(200., 160.));
        doc.atom_mut(fe)
            .ok_or_else(|| anyhow::anyhow!("Fe"))?
            .charge = 2;
        for position in [Point::new(100., 60.), Point::new(300., 260.)] {
            let end = doc.add_atom("C", position);
            doc.add_bond(fe, end, 1, "plain");
            doc = reshiki::atom_text::apply(&doc, end, label, reshiki::atom_text::Mode::Auto)
                .map_err(anyhow::Error::msg)?;
        }
        save(&dir, if *label == "Cp" { "Cp" } else { "Cp-star" }, &doc)?;
    }
    let mut dummy = Document::default();
    let a = dummy.add_atom("C", Point::new(80., 80.));
    let point = dummy.add_atom("*", Point::new(120., 100.));
    let b = dummy.add_atom("C", Point::new(160., 80.));
    dummy.add_bond(a, point, 1, "plain");
    dummy.add_bond(point, b, 1, "plain");
    save(&dir, "hidden-dummy", &dummy)?;
    println!(
        "Wrote 4 attachment cases and 12 common groups to {}",
        dir.display()
    );
    Ok(())
}
