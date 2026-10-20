use super::*;
use reshiki::document::Document;
use reshiki::models3d::{Display, ForceField, Model3d, Placed, Style, set_display};

/// Ethanol shown only as a 3D model, in `style`.
fn model(style: Style) -> Document {
    let mut doc = Document::default();
    let c1 = doc.add_atom("C", World::new(0., 0.));
    let c2 = doc.add_atom("C", World::new(36., -21.));
    let o = doc.add_atom("O", World::new(72., 0.));
    doc.add_bond(c1, c2, 1, "plain");
    doc.add_bond(c2, o, 1, "plain");
    let atoms = [
        (c1, [-1.2, -0.2, 0.1]),
        (c2, [0.1, 0.5, -0.1]),
        (o, [1.2, -0.3, 0.2]),
    ]
    .map(|(id, xyz)| Placed { id, xyz })
    .to_vec();
    doc.models
        .push(Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s));
    doc.models[0].style = style;
    set_display(&mut doc, 0, Display::ThreeD);
    doc
}

/// The canvas image of a model is the figure export's rendering of it, and
/// smooth at 400%: no banding rings inside the balls.
#[test]
fn model_images_match_figure_export_and_are_smooth() {
    for style in Style::ALL {
        let doc = model(style);
        let run = scene::primitives(&doc);
        assert!(run.iter().all(is_model), "{style:?}");
        let (lo, hi) = scene::bounds(&run);
        let scale = 4. * OVERSAMPLE;
        let (width, height) = (
            ((hi.x - lo.x) * scale).ceil() as u32,
            ((hi.y - lo.y) * scale).ceil() as u32,
        );
        let canvas = pixels(&run, lo, scale, width, height).unwrap();

        let tree = resvg::usvg::Tree::from_str(&scene::svg(&doc), &Default::default()).unwrap();
        let mut export = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
        // The export SVG is sized in points over the same view box.
        let points = tree.size().width() / (hi.x - lo.x);
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale / points, scale / points),
            &mut export.as_mut(),
        );
        let worst = canvas
            .chunks(4)
            .zip(export.pixels())
            .flat_map(|(a, b)| {
                let b = b.demultiply();
                a.iter()
                    .zip([b.red(), b.green(), b.blue(), b.alpha()])
                    .map(|(a, b)| a.abs_diff(b))
                    .collect::<Vec<_>>()
            })
            .max()
            .unwrap();
        // Anti-aliased edges may round differently under the two transforms.
        assert!(
            worst <= 6,
            "{style:?}: canvas differs from export by {worst}"
        );

        // Across the front ball's middle row, neighbouring pixels differ by
        // at most a few levels: the gradient is continuous.
        let front = run.iter().rev().find_map(|p| match p {
            Primitive::Sphere { center, radius, .. } => Some((center, radius)),
            _ => None,
        });
        if let Some((center, radius)) = front {
            let y = ((center.y - lo.y) * scale) as u32;
            let (from, to) = (
                ((center.x - radius * 0.85 - lo.x) * scale) as u32,
                ((center.x + radius * 0.85 - lo.x) * scale) as u32,
            );
            let row = |x: u32| &canvas[((y * width + x) * 4) as usize..][..3];
            for x in from..to {
                let step = row(x)
                    .iter()
                    .zip(row(x + 1))
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(step <= 4, "{style:?}: step {step} at {x}");
            }
        }
    }
}

/// A model zoomed in far enough for a multi-megabyte image appears in the
/// first frame, not after iced's background upload, and is drawn whole.
#[tokio::test]
#[ignore = "Opt-in real renderer 3D model image check"]
async fn a_zoomed_model_appears_in_its_first_frame() {
    use iced::advanced::Renderer as _;
    use iced::advanced::graphics::geometry::Renderer as _;
    use iced::advanced::renderer::Headless;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless renderer");
    let doc = model(Style::SpaceFilling);
    let (lo, hi) = reshiki::models3d::frame(&doc, &doc.models[0]).unwrap();
    let bounds = Rectangle::with_size(Size::new(640., 400.));
    let camera = Camera {
        center: World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
        zoom: 6.,
    };
    let mut render = || {
        renderer.reset(bounds);
        let mut frame = layered::Frame::new(&renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), iced::Color::WHITE);
        super::super::render::draw_primitives(
            &mut frame,
            &scene::primitives(&doc),
            camera,
            bounds,
            0.,
        );
        for geometry in frame.finish() {
            renderer.draw_geometry(geometry);
        }
        Headless::screenshot(&mut renderer, Size::new(1280, 800), 2., iced::Color::WHITE)
    };
    let cold = render();
    assert!(
        cold.chunks(4).any(|p| p[0] < 200),
        "the model is drawn in the first frame"
    );
    assert_eq!(cold, render(), "cold and warm frames match");
}
