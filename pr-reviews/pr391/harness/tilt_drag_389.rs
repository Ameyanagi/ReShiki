//! Issue #389 evidence harness. Compiles the checkout's own canvas drag math
//! (src/canvas/tilt.rs: TiltDrag::angles + apply, which calls
//! reshiki::projection::tilt) and applies a drag to a substituted benzene.
//!
//! usage: tilt_drag_389 OUT_DIR LABEL DX,DY [DX,DY ...]
//! Writes OUT_DIR/LABEL-<dx>px.rsk and prints one line per drag.
#![allow(dead_code)]
#[path = "../src/canvas/tilt.rs"]
mod tilt;

use reshiki::document::{Document, Point};

fn molecule() -> Document {
    let mut doc = reshiki::rings::Preset::Benzene.document(42., false);
    let ring: Vec<_> = doc.atoms.iter().map(|a| (a.id, a.position)).collect();
    // Cl on the upper-right vertex and O on the bottom vertex make the
    // drawing chiral in the plane, so a 180° turn is visible as a mirror.
    for (index, element) in [(1, "Cl"), (3, "O")] {
        let (id, p) = ring[index];
        let length = (p.x * p.x + p.y * p.y).sqrt();
        let outward = Point::new(p.x + p.x / length * 42., p.y + p.y / length * 42.);
        let substituent = doc.add_atom(element, outward);
        doc.add_bond(id, substituent, 1, "plain");
    }
    let ring_ids: Vec<_> = ring.iter().map(|(id, _)| *id).collect();
    // Perspective outlines: near ring edges bold, so depth order is visible.
    reshiki::projection::depth_bonds(&mut doc, &ring_ids);
    doc
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().expect("OUT_DIR"));
    let label = args.next().expect("LABEL");
    std::fs::create_dir_all(&out)?;
    for spec in args {
        let (dx, dy) = spec.split_once(',').expect("DX,DY");
        let (dx, dy): (f32, f32) = (dx.parse()?, dy.parse()?);
        let mut doc = molecule();
        let ids = doc.all_ids();
        let drag = tilt::TiltDrag {
            ids: ids.clone(),
            start: iced::Point::new(0., 0.),
        };
        let (x, y) = drag.angles(iced::Point::new(dx, dy), false);
        let before: Vec<_> = doc.atoms.iter().map(|a| (a.position, a.depth)).collect();
        tilt::apply(&mut doc, &ids, x, y);
        let moved = doc
            .atoms
            .iter()
            .zip(&before)
            .any(|(a, (p, z))| a.position.distance(*p) > 1e-3 || (a.depth - z).abs() > 1e-3);
        // Measured Y rotation of the Cl atom about the ring centre. It starts
        // flat (depth 0, right of centre); projection::tilt maps
        // x' = x cos θ, z' = −x sin θ, so θ = atan2(−z', x').
        let cl = &doc.atoms[6];
        let cx = doc.atoms[..6].iter().map(|a| a.position.x).sum::<f32>() / 6.;
        let cz = doc.atoms[..6].iter().map(|a| a.depth).sum::<f32>() / 6.;
        let theta = (-(cl.depth - cz)).atan2(cl.position.x - cx).to_degrees();
        let path = out.join(format!("{label}-{dx:.0}px.rsk"));
        std::fs::write(&path, serde_json::to_vec_pretty(&doc)?)?;
        println!(
            "{label}: drag ({dx:+.0}, {dy:+.0}) px -> angles() X {x:+.1}° Y {y:+.1}°; changed={moved}; measured Y rotation of Cl {theta:+.1}°; {}",
            path.display()
        );
    }
    Ok(())
}
