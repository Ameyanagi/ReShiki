//! Matched renderer evidence for regular-ring attachment and overlap rejection.
//! Run this unchanged on the base and head revisions with separate output dirs.
use anyhow::Context;
use reshiki::{
    document::{Document, Point},
    editing,
    rings::Preset,
};
use std::{fs, path::PathBuf};

fn saturated() -> Document {
    let mut doc = Document::default();
    let center = doc.add_atom("C", Point::default());
    for p in [
        Point::new(42., 0.),
        Point::new(-42., 0.),
        Point::new(0., 42.),
        Point::new(0., -42.),
    ] {
        let other = doc.add_atom("C", p);
        doc.add_bond(center, other, 1, "plain");
    }
    doc
}
fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(std::env::args_os().nth(1).context("Output directory")?);
    fs::create_dir_all(&out)?;
    let mut methane = Document::default();
    let carbon = methane.add_atom("C", Point::default());
    methane.atom_mut(carbon).unwrap().explicit_h = 4;
    let ring = Preset::Regular.document(42., false);
    let a = ring.atom(ring.bonds[0].a).unwrap().position;
    let b = ring.atom(ring.bonds[0].b).unwrap().position;
    let midpoint = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
    let valid_attachments = std::env::args().nth(2).as_deref() == Some("--valid-attachments");
    let cases = if valid_attachments {
        let mut phosphorus = Document::default();
        let p = phosphorus.add_atom("P", Point::default());
        let c = phosphorus.add_atom("C", Point::new(-42., 0.));
        phosphorus.add_bond(p, c, 1, "plain");
        let mut styled = Document::default();
        let a = styled.add_atom("C", Point::new(-21., 0.));
        let b = styled.add_atom("C", Point::new(21., 0.));
        styled.add_bond(a, b, 2, "bold");
        styled.bonds[0].secondary_display = Some("plain".into());
        vec![
            (
                "phosphorus",
                "Phosphorus attachment",
                phosphorus,
                Point::default(),
                Some(Point::new(80., 0.)),
            ),
            (
                "styled",
                "Bold double-bond fusion",
                styled,
                Point::default(),
                Some(Point::new(0., 80.)),
            ),
        ]
    } else {
        vec![
            (
                "saturated",
                "Four-bond carbon",
                saturated(),
                Point::default(),
                None,
            ),
            (
                "explicit-h",
                "Explicit CH4",
                methane,
                Point::default(),
                None,
            ),
            (
                "overlay",
                "Drag ring over itself",
                ring,
                midpoint,
                Some(Point::default()),
            ),
        ]
    };
    let width = cases.len() * 320;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"350\" viewBox=\"0 0 {width} 350\"><rect width=\"{width}\" height=\"350\" fill=\"white\"/>",
    );
    for (column, (name, title, source, point, direction)) in cases.into_iter().enumerate() {
        fs::write(
            out.join(format!("{name}-input.rsk")),
            serde_json::to_vec_pretty(&source)?,
        )?;
        let mut result = source.clone();
        let accepted = editing::ring_oriented(&mut result, point, 6, false, 5., direction).is_ok();
        fs::write(
            out.join(format!("{name}-result.rsk")),
            serde_json::to_vec_pretty(&result)?,
        )?;
        let coincident = result
            .atoms
            .iter()
            .enumerate()
            .map(|(i, a)| {
                result
                    .atoms
                    .iter()
                    .skip(i + 1)
                    .filter(|b| a.position.distance(b.position) < 0.01)
                    .count()
            })
            .sum::<usize>();
        let valence = result
            .bonds
            .iter()
            .filter(|b| b.a == carbon || b.b == carbon)
            .map(|b| u32::from(b.order))
            .sum::<u32>()
            + result.atom(carbon).unwrap().explicit_h;
        println!(
            "{name}: accepted={accepted}, atoms={}, bonds={}, coincident={coincident}, first-atom valence={valence}, unchanged={}",
            result.atoms.len(),
            result.bonds.len(),
            result == source
        );
        let drawing = reshiki::scene::svg(&result);
        let (_, body) = drawing.split_once('>').context("SVG opening tag")?;
        let body = body
            .strip_suffix("</svg>\n")
            .or_else(|| body.strip_suffix("</svg>"))
            .context("SVG closing tag")?;
        let x = column * 320;
        let detail = match name {
            "overlay" => format!("{coincident} coincident atom pairs"),
            "phosphorus" => format!("Phosphorus bond valence: {valence}"),
            "styled" => format!(
                "Original bond retained: {}",
                result.bonds[0] == source.bonds[0]
            ),
            _ => format!("Carbon valence: {valence}"),
        };
        svg.push_str(&format!("<text x=\"{}\" y=\"28\" font-family=\"Arial\" font-size=\"19\">{title}</text><svg x=\"{x}\" y=\"40\" width=\"320\" height=\"245\" viewBox=\"-110 -100 260 210\">{body}</svg><text x=\"{}\" y=\"306\" font-family=\"Arial\" font-size=\"16\">{} atoms / {} bonds · {}</text><text x=\"{}\" y=\"331\" font-family=\"Arial\" font-size=\"15\">{detail}</text>", x+16, x+16, result.atoms.len(), result.bonds.len(), if accepted { "accepted" } else { "rejected" }, x+16));
    }
    svg.push_str("</svg>");
    fs::write(out.join("regular-ring-safety.svg"), &svg)?;
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width as u32, 350).context("Pixmap")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(out.join("regular-ring-safety.png"))?;
    Ok(())
}
