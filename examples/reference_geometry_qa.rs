//! Reproducible inputs and actual renderer outputs for the reference controls.
use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    editing::{
        self,
        reference::{self, Edge, Stretch},
    },
    engine::{ChemistryEngine, LocalEngine, Request},
    rings::Preset,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn write(directory: &Path, name: &str, doc: &Document) -> anyhow::Result<()> {
    doc.validate().map_err(anyhow::Error::msg)?;
    fs::write(
        directory.join(format!("{name}.rsk")),
        doc.file_json().map_err(anyhow::Error::msg)?,
    )?;
    for format in ["svg", "png", "pdf"] {
        fs::write(
            directory.join(format!("{name}.{format}")),
            reshiki::export::drawing(doc, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    Ok(())
}
fn sheet(cases: &[(&str, Document)]) -> Document {
    let mut doc = Document::default();
    for (i, (label, source)) in cases.iter().enumerate() {
        let (lo, hi) = source.bounds();
        let center = Point::new(180. + (i % 3) as f32 * 370., 180. + (i / 3) as f32 * 430.);
        editing::append(
            &mut doc,
            source,
            Point::new(
                center.x - (lo.x + hi.x) * 0.5,
                center.y - (lo.y + hi.y) * 0.5,
            ),
        );
        let format = reshiki::typography::TextFormat {
            style: reshiki::typography::TextStyle {
                size_pt: 6.,
                ..Default::default()
            },
            width_pt: Some(110.),
            ..Default::default()
        };
        doc.annotations.push(Annotation {
            id: doc.next_id(),
            position: center.offset(-140., 200.),
            text: (*label).into(),
            format,
        });
    }
    doc
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: reference_geometry_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let ring = Preset::Benzene.document(42., false);
    let ids = ring.all_ids();
    let edge = reference::edges(&ring, &ids)
        .first()
        .copied()
        .context("Ring edge")?;
    let turn = reference::alignment_degrees(&ring, edge, 17.3, true).map_err(anyhow::Error::msg)?;
    let (ring, _) = reference::rotate(&ring, &ids, Point::default(), turn, false)
        .map_err(anyhow::Error::msg)?;
    let turn = reference::alignment_degrees(&ring, edge, 0., false).map_err(anyhow::Error::msg)?;
    let (horizontal, _) = reference::rotate(&ring, &ids, Point::default(), turn, false)
        .map_err(anyhow::Error::msg)?;
    let turn = reference::alignment_degrees(&ring, edge, 90., false).map_err(anyhow::Error::msg)?;
    let (vertical, _) = reference::rotate(&ring, &ids, Point::default(), turn, false)
        .map_err(anyhow::Error::msg)?;
    let mut branch = Document::default();
    let (s, c) = 17.3_f32.to_radians().sin_cos();
    let (s2, c2) = (-42.7_f32).to_radians().sin_cos();
    branch.add_atom("C", Point::default());
    branch.add_atom("C", Point::new(42. * c, 42. * s));
    branch.add_atom("C", Point::new(42. * (c + c2), 42. * (s + s2)));
    branch.add_atom("O", Point::new(42. * (2. * c + c2), 42. * (2. * s + s2)));
    branch.add_bond(1, 2, 1, "plain");
    branch.add_bond(2, 3, 1, "plain");
    branch.add_bond(3, 4, 1, "plain");
    branch.atom_mut(4).context("Terminal oxygen")?.label_h = 1;
    let plan = Stretch::new(&branch, 1, 2).map_err(anyhow::Error::msg)?;
    let stretched = plan.apply(&branch, 63.).map_err(anyhow::Error::msg)?;
    let mut orbit = Preset::Cyclopentadiene.document(42., false);
    let ids = orbit.all_ids();
    orbit.translate(&ids, 0., -140.);
    for angle in [90., 180., 270.] {
        let (copied, _) = reference::rotate(&orbit, &ids, Point::default(), angle, true)
            .map_err(anyhow::Error::msg)?;
        orbit = copied;
    }
    let engine = LocalEngine::default();
    let stereo = engine
        .execute(Request::import_smiles("C[C@H](F)Cl"))
        .await
        .map_err(anyhow::Error::msg)?
        .document
        .context("Chiral example")?;
    let bond = stereo
        .bonds
        .iter()
        .find(|b| matches!(b.display.as_str(), "wedge" | "hash"))
        .context("Chiral example's stereo bond")?;
    let stereo_edge = Edge::Bond(bond.a, bond.b);
    let stereo_ids = stereo.all_ids();
    let turn = reference::alignment_degrees(&stereo, stereo_edge, 17.3, true)
        .map_err(anyhow::Error::msg)?;
    let (stereo, _) = reference::rotate(&stereo, &stereo_ids, Point::default(), turn, false)
        .map_err(anyhow::Error::msg)?;
    let turn = reference::alignment_degrees(&stereo, stereo_edge, 0., false)
        .map_err(anyhow::Error::msg)?;
    let (stereo_aligned, _) =
        reference::rotate(&stereo, &stereo_ids, Point::default(), turn, false)
            .map_err(anyhow::Error::msg)?;
    let cases = vec![
        ("Reference edge: 17.3° input", ring),
        ("Align edge horizontally", horizontal),
        ("Align edge vertically", vertical),
        ("Branch: 14.4 pt at 17.3°", branch),
        ("Stretch to 21.6 pt; same direction", stretched),
        ("Quarter-turn copies; pinned center", orbit),
        ("Stereo bond: 17.3° input", stereo),
        ("Align stereo bond horizontally", stereo_aligned),
    ];
    for (i, (_, doc)) in cases.iter().enumerate() {
        write(&directory, &format!("case-{i}"), doc)?;
    }
    write(&directory, "reference-geometry", &sheet(&cases))?;
    fs::write(
        directory.join("actions.txt"),
        "Application renderer, JACS/ACS style, default PNG1200dpi; these are input/output figures, not desktop evidence.\nUI: select the chosen bond's endpoints (Shift-click), Properties > Transform > Reference: align / stretch. Choose Horizontal/Vertical, or To angle for a directed absolute angle.\nFor copies: Pin selected center, enter pinned X/Y in points or retain the pin, set Turn, Copy + rotate. The same pin survives subsequent selections.\nStretch case3: reference first bond, Connected fragment scope, Drag to stretch. Move its second endpoint along17.3deg, optionally with a perpendicular pointer offset. The endpoint and downstream branch move rigidly; Set length21.6pt produces case4.\ncase5 is a construction geometry demonstration of four separate carbocycles, not a validated porphyrin.\ncases6/7 use C[C@H](F)Cl, retaining the stereo bond's stored endpoint order and full connected fragment.\nRing bonds deliberately do not offer Stretch; source fixed and wedge directions are retained.\n",
    )?;
    println!(
        "Wrote eight editable reference-geometry cases and renderer SVG/PNG/PDF outputs to {}",
        directory.display()
    );
    Ok(())
}
