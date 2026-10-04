//! Capture the unchanged renderer once, then compare candidate pixels without
//! retaining a second implementation of the production renderer as an oracle.
use super::*;
use iced::advanced::Renderer as _;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::image::Renderer as _;
use iced::advanced::renderer::Headless;
use iced::widget::canvas::Program;
use reshiki::{
    canvas_theme::CanvasTheme,
    document::{Annotation, Arrow},
    graphics::LinePattern,
    pictures::Picture,
    typography::TextStyle,
};

fn picture() -> Picture {
    let pixels = image::RgbaImage::from_fn(12, 8, |x, y| {
        image::Rgba(match (x < 6, y < 4) {
            (true, true) => [255, 0, 0, 255],
            (false, true) => [0, 255, 0, 255],
            (true, false) => [0, 0, 255, 255],
            (false, false) => [0, 0, 0, 0],
        })
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    Picture::import(bytes.get_ref()).unwrap()
}

fn mixed_primitives() -> Vec<Primitive> {
    let picture = picture();
    let mut upright = picture.graphic(1, World::default());
    upright.origin = World::new(50., 45.);
    upright.axis_x = World::new(190., 20.);
    upright.axis_y = World::new(-10., 110.);
    let mut flipped = picture.graphic(2, World::default());
    flipped.origin = World::new(370., 170.);
    flipped.axis_x = World::new(175., -15.);
    flipped.axis_y = World::new(10., -110.);
    let background = Primitive::Path {
        commands: vec![
            PathCommand::Move(World::new(35., 30.)),
            PathCommand::Line(World::new(560., 30.)),
            PathCommand::Line(World::new(560., 190.)),
            PathCommand::Line(World::new(35., 190.)),
            PathCommand::Close,
        ],
        style: GraphicStyle {
            fill: Some(reshiki::palette::Color::Custom([240, 220, 180])),
            ..Default::default()
        },
        filled: true,
    };
    let dashed = Primitive::Path {
        commands: vec![
            PathCommand::Move(World::new(25., 120.)),
            PathCommand::Cubic(
                World::new(170., -40.),
                World::new(410., 245.),
                World::new(595., 90.),
            ),
        ],
        style: GraphicStyle {
            stroke: reshiki::palette::Color::Custom([100, 40, 160]),
            width_pt: 1.2,
            pattern: LinePattern::Dashed,
            ..Default::default()
        },
        filled: false,
    };
    vec![
        background,
        Primitive::Picture(upright),
        Primitive::Line(World::new(30., 80.), World::new(590., 140.), 0.2),
        Primitive::Polygon(vec![
            World::new(230., 40.),
            World::new(280., 170.),
            World::new(330., 70.),
        ]),
        dashed,
        Primitive::Picture(flipped),
        Primitive::Text {
            position: World::new(70., 240.),
            text: "Underlined NH2 · café".into(),
            size: 28.,
            color: [20, 90, 160],
            style: TextStyle {
                underline: true,
                bold: true,
                ..Default::default()
            },
        },
    ]
}

fn routing_document() -> Document {
    let mut doc = Document::default();
    let a = doc.add_atom("N", World::new(170., 130.));
    let b = doc.add_atom("O", World::new(212., 130.));
    doc.add_bond(a, b, 1, "bold");
    doc.bonds[0].highlight = Some(reshiki::palette::Color::Custom([190, 230, 240]));
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: World::new(125., 240.),
        text: "Editable underlined caption".into(),
        format: Default::default(),
    });
    doc.annotations[0].format.style.underline = true;
    doc.arrows.push(Arrow {
        id: doc.next_id(),
        start: World::new(260., 215.),
        end: World::new(365., 215.),
        kind: "forward".into(),
        control: Some(World::new(305., 160.)),
        style: None,
    });
    for (layer, origin, axis_y) in [
        (-1, World::new(140., 70.), World::new(0., 115.)),
        (1, World::new(445., 180.), World::new(0., -100.)),
    ] {
        let mut g = picture().graphic(doc.next_id(), World::default());
        g.origin = origin;
        g.axis_x = World::new(150., 10.);
        g.axis_y = axis_y;
        g.layer = layer;
        doc.graphics.push(g);
    }
    doc
}

fn check_pixels(directory: &std::path::Path, name: &str, pixels: &[u8], capture: bool) {
    let path = directory.join(format!("{name}.rgba"));
    if capture {
        std::fs::write(&path, pixels).unwrap();
    } else {
        let expected = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("Read baseline {}: {error}", path.display()));
        assert_same_pixels(name, pixels, &expected);
    }
    println!(
        "PIXEL,{name},{} bytes,{}",
        pixels.len(),
        if capture { "captured" } else { "matched" }
    );
}

fn assert_same_pixels(name: &str, pixels: &[u8], expected: &[u8]) {
    assert_eq!(
        pixels.len(),
        expected.len(),
        "{name}: image dimensions changed"
    );
    let different = pixels.iter().zip(expected).filter(|(a, b)| a != b).count();
    let first = pixels
        .iter()
        .zip(expected)
        .position(|(a, b)| a != b)
        .map(|i| {
            let pixel = i / 4;
            format!(
                "({}, {}), actual {:?}, expected {:?}",
                pixel % 640,
                pixel / 640,
                &pixels[pixel * 4..pixel * 4 + 4],
                &expected[pixel * 4..pixel * 4 + 4]
            )
        });
    assert_eq!(
        different, 0,
        "{name}: {different} different channels; first pixel {first:?}"
    );
}

// Iced decodes Bytes handles asynchronously during ordinary image preparation.
// Explicit loading synchronously decodes/uploads these tiny fixtures, so the
// cold frame contains its pictures and can be compared exactly with warm frames.
fn load_pictures(
    renderer: &Renderer,
    graphics: impl IntoIterator<Item = Graphic>,
) -> Vec<iced::advanced::image::Allocation> {
    graphics
        .into_iter()
        .filter_map(|g| {
            let picture = g.picture.as_ref()?;
            let flip = g.axis_x.x * g.axis_y.y - g.axis_x.y * g.axis_y.x < 0.;
            Some(renderer.load_image(&picture.handle(flip).unwrap()).unwrap())
        })
        .collect()
}

#[tokio::test]
#[ignore = "Baseline/candidate pixel comparison; requires a headless renderer and artifact directory"]
async fn renderer_matches_captured_baseline() {
    let directory = std::path::PathBuf::from(
        std::env::var("RESHIKI_CANVAS_PIXELS")
            .expect("Set RESHIKI_CANVAS_PIXELS to the baseline artifact directory"),
    );
    let capture = std::env::var("RESHIKI_CANVAS_CAPTURE_BASELINE").as_deref() == Ok("1");
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let bounds = Rectangle::with_size(iced::Size::new(640., 400.));
    let camera = Camera {
        center: World::new(320., 200.),
        zoom: 1.,
    };
    let primitives = mixed_primitives();
    let _allocations = load_pictures(
        &renderer,
        primitives.iter().filter_map(|primitive| {
            if let Primitive::Picture(g) = primitive {
                Some(g.clone())
            } else {
                None
            }
        }),
    );
    let metadata = serde_json::json!({
        "renderer": renderer.name(), "width": 640, "height": 400,
        "font": reshiki::style::ui_font_family(),
        "fixture": format!("{primitives:?}"),
        "routing_document": routing_document(),
    });
    if capture {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("metadata.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
    } else {
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("metadata.json")).unwrap())
                .unwrap();
        assert_eq!(
            metadata, expected,
            "Use the same renderer, fonts and fixtures as the baseline"
        );
    }
    for theme in CanvasTheme::ALL {
        for minimum in [0., 0.6] {
            let mut render = || {
                renderer.reset(bounds);
                let mut frame = layered::Frame::new(&renderer, bounds.size()).with_canvas(theme);
                frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
                draw_primitives(&mut frame, &primitives, camera, bounds, minimum);
                for geometry in frame.finish() {
                    renderer.draw_geometry(geometry);
                }
                Headless::screenshot(&mut renderer, iced::Size::new(640, 400), 1., Color::WHITE)
            };
            let pixels = render();
            assert_same_pixels("Primitive cold/warm rendering", &pixels, &render());
            for (x, y) in [(86, 71), (421, 138)] {
                let offset = (y * 640 + x) * 4;
                assert_eq!(
                    &pixels[offset..offset + 4],
                    &[255, 0, 0, 255],
                    "Picture quadrant missing at ({x}, {y}); covers upright and flipped pictures"
                );
            }
            check_pixels(
                &directory,
                &format!("primitives_{theme:?}_{minimum}"),
                &pixels,
                capture,
            );
        }
        let mut doc = routing_document();
        doc.canvas_theme = theme;
        let _allocations = load_pictures(&renderer, doc.graphics.iter().cloned());
        let all = doc.all_ids();
        let partial = &all[..1];
        let point = Point::new(185., 130.);
        for mode in [
            "idle",
            "selected",
            "hidden",
            "whole_drag",
            "hidden_drag",
            "partial_drag",
            "copy_drag",
            "resize",
        ] {
            let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
            canvas.tool = Tool::Select;
            canvas.camera = camera;
            canvas.selected = if mode == "idle" {
                &[]
            } else if mode == "partial_drag" {
                partial
            } else {
                &all
            };
            canvas.hidden_annotation =
                matches!(mode, "hidden" | "hidden_drag").then_some(doc.annotations[0].id);
            let mut state = State::default();
            if mode.ends_with("drag") {
                state.cursor = Some(point + Vector::new(28., 19.));
                let ids = canvas.selected.to_vec();
                state.gesture = Some(Gesture::Move {
                    start: camera.world(point, bounds),
                    clicked: vec![ids[0]],
                    ids,
                });
                if mode == "partial_drag" {
                    state.modifiers = iced::keyboard::Modifiers::ALT;
                }
                if mode == "copy_drag" {
                    state.modifiers =
                        iced::keyboard::Modifiers::CTRL | iced::keyboard::Modifiers::LOGO;
                }
            } else if mode == "resize" {
                let selection = SelectionBox::new(&doc, &all, camera, bounds).unwrap();
                let start = World::new(600., 300.);
                state.gesture = Some(Gesture::Transform(Box::new(TransformDrag::new(
                    selection,
                    Handle::Resize(2),
                    start,
                    &all,
                ))));
                state.cursor = Some(camera.screen(start.offset(35., 25.), bounds));
            }
            let mut render = || {
                renderer.reset(bounds);
                for geometry in canvas.draw(
                    &state,
                    &renderer,
                    &Theme::Light,
                    bounds,
                    mouse::Cursor::Unavailable,
                ) {
                    renderer.draw_geometry(geometry);
                }
                Headless::screenshot(&mut renderer, iced::Size::new(640, 400), 1., Color::WHITE)
            };
            let pixels = render();
            assert_same_pixels(&format!("{mode}: cold/warm routing"), &pixels, &render());
            check_pixels(
                &directory,
                &format!("routing_{theme:?}_{mode}"),
                &pixels,
                capture,
            );
        }
    }
}
