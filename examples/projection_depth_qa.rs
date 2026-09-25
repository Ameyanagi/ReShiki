//! Reproducible native/figure examples for depth ordering and retilting.
use anyhow::{Context, Result};
use reshiki::{
    assistant::{
        review::{self, Edit},
        sketch::Sketch,
    },
    attachments,
    document::{Annotation, Document, Point},
    editing, export, hotkeys, projection,
};
use std::{fs, path::PathBuf};

fn ligand(key: &str, moved: bool) -> Result<Document> {
    let mut doc = Document::default();
    let metal = doc.add_atom(if key == "j" { "Fe" } else { "Ru" }, Point::default());
    doc = hotkeys::atom_edit(&doc, metal, key, 42.)
        .context("Shortcut")?
        .map_err(anyhow::Error::msg)?
        .0;
    if moved {
        let point = doc
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .context("Point")?
            .clone();
        let ids = attachments::movement_selection(&doc, &[point.id]);
        doc.translate(&ids, -2. * point.position.x, -2. * point.position.y);
    }
    Ok(doc)
}

fn main() -> Result<()> {
    let out = PathBuf::from(std::env::args_os().nth(1).context("Output directory")?);
    fs::create_dir_all(&out)?;
    let near = ligand("J", false)?;
    let far = ligand("J", true)?;
    let mut retilted = far.clone();
    let anchor = retilted
        .atoms
        .iter()
        .find(|a| a.attachment.is_some())
        .context("Point")?
        .id;
    let ids = attachments::movement_selection(&retilted, &[anchor]);
    projection::tilt(&mut retilted, &ids, 62.5, false);
    let mut cp = ligand("j", true)?;
    let ids = cp.all_ids();
    editing::transform_about(&mut cp, &ids, Point::default(), 1., 30.);
    let mut partial = far.clone();
    let ring = partial
        .atoms
        .iter()
        .find(|a| a.attachment.is_some())
        .context("Ring")?
        .centroid
        .clone();
    reshiki::ring_arcs::toggle(&mut partial, ring.get(..4).context("Four ring atoms")?)
        .map_err(anyhow::Error::msg)?;
    let mut sketch: Sketch = serde_json::from_str(include_str!(
        "../tests/fixtures/assistant-cp-star-dimer.json"
    ))?;
    for ligand in &mut sketch.ligands {
        ligand.show_charge = false;
    }
    let dimer = sketch
        .render(&Default::default())
        .map_err(anyhow::Error::msg)?;
    let target = review::targets(&dimer)
        .into_iter()
        .find(|t| t.kind == "ligand")
        .context("Cp* target")?;
    let changed_dimer = review::apply(
        &dimer,
        &[Edit::TiltLigand {
            target: target.name,
            x_degrees: 17.,
            y_degrees: -8.,
            rotation_degrees: 12.,
            depth_bonds: true,
            show_charge: false,
        }],
        true,
    )
    .map_err(anyhow::Error::msg)?;
    let sugar = reshiki::haworth::sugar_document(
        reshiki::haworth::Sugar::Glucose,
        reshiki::haworth::Anomer::Alpha,
        42.,
    )
    .map_err(anyhow::Error::msg)?;
    let cases = [
        ("Arene: contact behind near side", near),
        ("Arene: dragged to the far side", far),
        ("Arene: tilted again", retilted),
        ("Cp: contact crosses a ring vertex", cp),
        ("Partial inner curve", partial),
        ("Cp* dimer: explicit contact layers", dimer),
        ("Cp* dimer: one ligand retilted", changed_dimer),
        ("Haworth: stereo retained", sugar),
    ];
    let mut gallery = Document::default();
    for (i, (label, doc)) in cases.iter().enumerate() {
        doc.validate().map_err(anyhow::Error::msg)?;
        fs::write(
            out.join(format!("case-{i}.rsk")),
            serde_json::to_vec_pretty(doc)?,
        )?;
        for format in ["svg", "png", "pdf"] {
            fs::write(
                out.join(format!("case-{i}.{format}")),
                export::drawing(doc, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        let drawing = doc.clone();
        let (lo, hi) = drawing.bounds();
        anyhow::ensure!(
            hi.x - lo.x < 560. && hi.y - lo.y < 300.,
            "Example exceeds review cell: {label}"
        );
        let x = (i % 2) as f32 * 600.;
        let y = (i / 2) as f32 * 380.;
        editing::append(
            &mut gallery,
            &drawing,
            Point::new(x + 280. - (lo.x + hi.x) / 2., y + 150. - (lo.y + hi.y) / 2.),
        );
        let mut format = reshiki::typography::TextFormat::default();
        format.style.size_pt = 7.5;
        format.style.bold = true;
        format.style.color = [41, 62, 56];
        format.width_pt = Some(190.);
        gallery.annotations.push(Annotation {
            id: gallery.next_id(),
            position: Point::new(x, y + 330.),
            text: label.to_string(),
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
            export::drawing(&gallery, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    println!(
        "Exported {} editable cases and SVG/PNG/PDF figures",
        cases.len()
    );
    Ok(())
}
