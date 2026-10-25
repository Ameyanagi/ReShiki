//! Capture #513 with a fixed tetrahedral methane through the application renderer.
use anyhow::Context;
use reshiki_model::{
    canvas_theme::CanvasTheme,
    document::{Document, Point},
    models3d::{self, Background, Display, ForceField, Hydrogen, Look, Model3d, Placed, Style},
    scene,
};
use std::{fs, path::PathBuf};

fn methane() -> Document {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let length = 1.09_f32;
    let hydrogens = [
        [0., 0., length],
        [8_f32.sqrt() * length / 3., 0., -length / 3.],
        [
            -2_f32.sqrt() * length / 3.,
            6_f32.sqrt() * length / 3.,
            -length / 3.,
        ],
        [
            -2_f32.sqrt() * length / 3.,
            -6_f32.sqrt() * length / 3.,
            -length / 3.,
        ],
    ]
    .map(|xyz| Hydrogen {
        parent: carbon,
        xyz,
    })
    .to_vec();
    let mut model = Model3d::new(
        &doc,
        vec![Placed {
            id: carbon,
            xyz: [0.; 3],
        }],
        hydrogens,
        ForceField::Mmff94s,
    );
    model.rotation = [1., 0., 0., 0.];
    model.style = Style::SpaceFilling;
    model.look = Look::Glossy;
    model.perspective = 0.;
    model.depth_fade = 0.;
    doc.models.push(model);
    models3d::set_display(&mut doc, 0, Display::ThreeD);
    doc
}

fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: space_filling_cap_qa OUTPUT_DIRECTORY")?,
    );
    fs::create_dir_all(&directory)?;
    let input = methane();
    input.validate().map_err(anyhow::Error::msg)?;
    fs::write(
        directory.join("methane.rsk"),
        input.file_json().map_err(anyhow::Error::msg)?,
    )?;
    for (name, theme, background) in [
        ("white", CanvasTheme::Light, Background::None),
        ("card", CanvasTheme::Light, Background::Card),
        ("dark", CanvasTheme::Dark, Background::None),
    ] {
        let mut doc = input.clone();
        doc.canvas_theme = theme;
        if let Some(model) = doc.models.first_mut() {
            model.background = background;
        }
        let svg = scene::svg_with_background(&doc);
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;
        let scale = 480. / tree.size().width().max(tree.size().height());
        let width = (tree.size().width() * scale).round() as u32;
        let height = (tree.size().height() * scale).round() as u32;
        let mut pixmap =
            resvg::tiny_skia::Pixmap::new(width, height).context("allocate capture")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        fs::write(directory.join(format!("{name}.svg")), svg)?;
        pixmap.save_png(directory.join(format!("{name}.png")))?;
    }
    Ok(())
}
