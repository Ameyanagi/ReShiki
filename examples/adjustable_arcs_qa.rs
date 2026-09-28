//! Render the adjustable-arc examples through the application's export pipeline.
use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    graphics::{ArcGeometry, BracketSides, Graphic, GraphicKind, GraphicStyle},
};
use std::{fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: adjustable_arcs_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let mut doc = Document::default();
    for (index, (label, start, sweep)) in [
        ("90° preset", 180., 90.),
        ("120° preset", 180., 120.),
        ("180° preset", 180., 180.),
        ("270° preset", 180., 270.),
        ("234.5° sweep\n32° start", 32., 234.5),
        ("360° full circle", 180., 360.),
        ("Affine\ntransform", 180., 270.),
        ("Legacy\nhalf ellipse", 180., 180.),
    ]
    .into_iter()
    .enumerate()
    {
        let origin = Point::new(
            (index % 4) as f32 * 210. + 25.,
            (index / 4) as f32 * 170. + 30.,
        );
        let mut graphic = Graphic::dragged(
            doc.next_id(),
            GraphicKind::Arc,
            origin,
            origin.offset(145., 100.),
            GraphicStyle {
                stroke: [32, 80, 145],
                width_pt: 1.2,
                ..Default::default()
            },
            BracketSides::Both,
            false,
        )
        .with_arc(ArcGeometry {
            start_degrees: start,
            sweep_degrees: sweep,
        });
        if index == 5 {
            graphic.origin.x += 22.5;
            graphic.axis_x = Point::new(100., 0.);
        }
        if index == 6 {
            graphic.map_positions(|p| {
                let x = p.x - origin.x;
                let y = p.y - origin.y;
                origin.offset(140. - 0.8 * x - 0.3 * y, 0.22 * x + 0.75 * y)
            });
        }
        if index == 7 {
            graphic.arc = None;
        }
        doc.graphics.push(graphic);
        doc.annotations.push(Annotation {
            id: doc.next_id(),
            position: origin.offset(-8., 130.),
            text: label.into(),
            format: Default::default(),
        });
    }
    doc.validate().map_err(anyhow::Error::msg)?;
    fs::write(
        directory.join("adjustable-arcs.rsk"),
        serde_json::to_vec_pretty(&doc)?,
    )?;
    for format in ["png", "svg", "pdf"] {
        fs::write(
            directory.join(format!("adjustable-arcs.{format}")),
            reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    println!(
        "Wrote adjustable-arc native fixture and PNG/SVG/PDF exports to {}",
        directory.display()
    );
    Ok(())
}
