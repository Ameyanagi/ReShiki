//! Copyable tilted-ring bond-growth examples at the document's normal scale.
use anyhow::{Context, Result};
use reshiki::{
    bonds::BondPreset,
    document::{Annotation, Document, Point},
    projection::{self, growth},
};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let out = PathBuf::from(std::env::args_os().nth(1).context("Output directory")?);
    fs::create_dir_all(&out)?;
    let mut cp = Document::default();
    let anchor = cp.add_atom("C", Point::default());
    cp = reshiki::ligands::replace(&cp, anchor, "Cp").map_err(anyhow::Error::msg)?;
    let ring = cp.atom(anchor).context("Ring")?.centroid.clone();
    cp.expand_abbreviations(&[anchor]);
    for atom in &mut cp.atoms {
        if ring.contains(&atom.id) {
            atom.aromatic = true;
            atom.display.hide_charge = true;
        }
    }
    for bond in &mut cp.bonds {
        bond.order = 4;
    }
    let mut arene = reshiki::rings::Preset::Benzene.document(42., false);
    let mut gallery = Document::default();
    for (i, doc) in [&mut cp, &mut arene].into_iter().enumerate() {
        let ids = doc.all_ids();
        projection::tilt(doc, &ids, 25., true);
        projection::tilt(doc, &ids, -55., false);
        reshiki::editing::transform_about(doc, &ids, Point::default(), 1., 20.);
        projection::depth_bonds(doc, &ids);
        fs::write(
            out.join(format!("ring-{i}.rsk")),
            serde_json::to_vec_pretty(doc)?,
        )?;
        let atom = doc
            .atoms
            .iter()
            .filter(|a| a.element == "C")
            .nth(1)
            .context("Growth atom")?
            .id;
        let end = growth::Plane::at(doc, atom)
            .context("Plane")?
            .outward(42.)
            .context("End")?;
        let (mut grown, added) = growth::place(
            doc,
            atom,
            end,
            if i == 0 { "C" } else { "F" },
            BondPreset::Single,
        )
        .map_err(anyhow::Error::msg)?;
        if let Some(bond) = grown
            .bonds
            .iter_mut()
            .find(|b| b.a == added || b.b == added)
        {
            bond.color = [43, 112, 97];
        }
        fs::write(
            out.join(format!("case-{i}.rsk")),
            serde_json::to_vec_pretty(&grown)?,
        )?;
        for format in ["svg", "png", "pdf"] {
            fs::write(
                out.join(format!("case-{i}.{format}")),
                reshiki::export::drawing(&grown, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        let (lo, hi) = grown.bounds();
        let x = i as f32 * 330.;
        reshiki::editing::append(
            &mut gallery,
            &grown,
            Point::new(x + 135. - (lo.x + hi.x) / 2., 130. - (lo.y + hi.y) / 2.),
        );
        let mut format = reshiki::typography::TextFormat::default();
        format.style.size_pt = 8.;
        format.style.bold = true;
        format.style.color = [41, 62, 56];
        gallery.annotations.push(Annotation {
            id: gallery.next_id(),
            position: Point::new(x, 255.),
            text: if i == 0 {
                "Cp: methyl added after tilt"
            } else {
                "Arene: fluorine added after tilt"
            }
            .into(),
            format,
        });
    }
    fs::write(
        out.join("gallery.rsk"),
        serde_json::to_vec_pretty(&gallery)?,
    )?;
    for format in ["svg", "png", "pdf"] {
        fs::write(
            out.join(format!("gallery.{format}")),
            reshiki::export::drawing(&gallery, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    Ok(())
}
