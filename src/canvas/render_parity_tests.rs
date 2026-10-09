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
    templates::{Anchor, LIBRARY},
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

/// A ring, an N–O bond, an arrow, a caption and a rectangle, spread over the
/// 640x400 view. Returns the ring ids and `[n, o, arrow, rectangle]`.
fn paper_document() -> (Document, Vec<u64>, [u64; 4]) {
    let (mut doc, ring) = reshiki::editing::ring_placement(
        &Document::default(),
        World::new(200., 200.),
        6,
        false,
        10.,
        None,
    )
    .unwrap();
    let n = doc.add_atom("N", World::new(380., 120.));
    let o = doc.add_atom("O", World::new(422., 120.));
    doc.add_bond(n, o, 1, "plain");
    let arrow = doc.next_id();
    doc.arrows.push(Arrow {
        id: arrow,
        start: World::new(260., 330.),
        end: World::new(365., 330.),
        kind: "forward".into(),
        control: None,
        style: None,
    });
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: World::new(430., 250.),
        text: "Caption".into(),
        format: Default::default(),
    });
    let rectangle = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        rectangle,
        GraphicKind::Rectangle,
        World::new(470., 290.),
        World::new(560., 360.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    (doc, ring, [n, o, arrow, rectangle])
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

#[tokio::test]
#[ignore = "Text-placement baseline/candidate pixels; requires a renderer and artifact directory"]
async fn text_placement_matches_captured_baseline() {
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
    let mut primitives = mixed_primitives();
    for (index, family) in ["Arial", "Times New Roman", "Courier New"]
        .into_iter()
        .enumerate()
    {
        primitives.insert(
            index * 2 + 1,
            Primitive::Text {
                position: World::new(
                    79.123 + index as f32 * 91.317,
                    61.731 + index as f32 * 66.193,
                ),
                text: "AV office fi α β 日本語 العربية 🧪\nSecond line".into(),
                size: 17.317,
                color: [127, 45, 190],
                style: TextStyle {
                    family: family.into(),
                    bold: index == 0,
                    italic: index == 1,
                    underline: true,
                    ..Default::default()
                },
            },
        );
    }
    let _allocations = load_pictures(
        &renderer,
        primitives.iter().filter_map(|primitive| match primitive {
            Primitive::Picture(graphic) => Some(graphic.clone()),
            _ => None,
        }),
    );
    let views = [
        Camera {
            center: World::new(320., 200.),
            zoom: 1.,
        },
        Camera {
            center: World::new(318.371, 198.619),
            zoom: 0.455,
        },
        Camera {
            center: World::new(306.127, 184.913),
            zoom: 1.25,
        },
    ];
    let metadata = serde_json::json!({
        "renderer": renderer.name(), "width": 640, "height": 400,
        "font": reshiki::style::ui_font_family(), "fixture": format!("{primitives:?}"),
        "views": views.map(|camera| [camera.center.x, camera.center.y, camera.zoom]),
    });
    let metadata_path = directory.join("text_placement_metadata.json");
    if capture {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
    } else {
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
        assert_eq!(
            metadata, expected,
            "Use the same renderer, fonts and fixtures"
        );
    }
    for theme in CanvasTheme::ALL {
        for rulers in [false, true] {
            let paper = guides::Guides {
                rulers,
                ..Default::default()
            }
            .paper(bounds);
            let offset = Vector::new(paper.x, paper.y);
            for (index, camera) in views.into_iter().enumerate() {
                let cache = std::cell::RefCell::new(text_cache::TextCache::default());
                let mut render = |cached| {
                    renderer.reset(bounds);
                    let mut frame =
                        layered::Frame::clipped(&renderer, paper, offset).with_canvas(theme);
                    if cached {
                        frame = frame.with_text_cache(&cache);
                    }
                    frame.fill_rectangle(Point::ORIGIN, paper.size(), Color::WHITE);
                    draw_primitives(&mut frame, &primitives, camera, paper, 0.6);
                    for geometry in frame.finish() {
                        renderer.draw_geometry(geometry);
                    }
                    Headless::screenshot(&mut renderer, iced::Size::new(640, 400), 1., Color::WHITE)
                };
                let pixels = render(true);
                assert_same_pixels("Text placement cold/warm", &pixels, &render(true));
                assert_same_pixels("Text placement cached/fresh", &pixels, &render(false));
                check_pixels(
                    &directory,
                    &format!("text_placement_{theme:?}_{rulers}_{index}"),
                    &pixels,
                    capture,
                );
            }
        }
    }
}

#[tokio::test]
#[ignore = "Preview cache theme/viewport pixels; requires a headless renderer"]
async fn drawing_preview_matches_fresh_theme_and_size() {
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let mut doc = routing_document();
    let _allocations = load_pictures(&renderer, doc.graphics.iter().cloned());
    let state = PreviewState::default();
    for theme in CanvasTheme::ALL {
        doc.canvas_theme = theme;
        for size in [iced::Size::new(640., 400.), iced::Size::new(596.5, 361.25)] {
            let bounds = Rectangle::with_size(size);
            let mut render = |state| {
                renderer.reset(bounds);
                for geometry in DrawingPreview(&doc).draw(
                    state,
                    &renderer,
                    &Theme::Light,
                    bounds,
                    mouse::Cursor::Unavailable,
                ) {
                    renderer.draw_geometry(geometry);
                }
                Headless::screenshot(&mut renderer, iced::Size::new(640, 400), 1., Color::WHITE)
            };
            let pixels = render(&state);
            assert_same_pixels("Preview cold/warm", &pixels, &render(&state));
            assert_same_pixels(
                "Preview warmed/fresh theme and viewport",
                &pixels,
                &render(&PreviewState::default()),
            );
        }
    }
}

#[tokio::test]
#[ignore = "Paper layer baseline/candidate pixels; requires a headless renderer and artifact directory"]
async fn paper_layers_match_captured_baseline() {
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
    let (paper, ring, [n, o, arrow, rectangle]) = paper_document();
    let metadata = serde_json::json!({
        "renderer": renderer.name(), "width": 640, "height": 400,
        "font": reshiki::style::ui_font_family(), "paper_document": paper,
    });
    let metadata_path = directory.join("paper_metadata.json");
    if capture {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            &metadata_path,
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
    } else {
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
        assert_eq!(
            metadata, expected,
            "Use the same renderer, fonts and fixtures"
        );
    }
    let all = paper.all_ids();
    let arrow_only = [arrow];
    let edit_points = [rectangle, arrow];
    let ring_atom = *ring.iter().find(|id| paper.atom(**id).is_some()).unwrap();
    let position = |id| paper.atom(id).unwrap().position;
    let screen = |p| camera.screen(p, bounds);
    let at = |x, y| screen(World::new(x, y));
    let offset = |p, x, y| screen(p) + Vector::new(x, y);
    // The ring is centered on (200, 200): a chain from its atom to the opposite
    // vertex crosses the ring.
    let across = World::new(400. - position(ring_atom).x, 400. - position(ring_atom).y);
    let command = iced::keyboard::Modifiers::CTRL | iced::keyboard::Modifiers::LOGO;
    let template = LIBRARY.iter().find(|t| t.name == "Cyclohexane").unwrap();
    let chain = |start| Gesture::Chain {
        start,
        pressed: start,
        source: None,
        points: vec![start],
        snaking: false,
        dragged: true,
    };
    let transform = |handle| {
        let selection = SelectionBox::new(&paper, &all, camera, bounds).unwrap();
        Gesture::Transform(Box::new(TransformDrag::new(
            selection,
            handle,
            World::new(300., 200.),
            &all,
        )))
    };
    let mut idle = None;
    let mut line_short = None;
    for mode in [
        "idle",
        "selected",
        "hover",
        "select_rect",
        "lasso",
        "move",
        "copy_move",
        "rotate",
        "edge",
        "tilt",
        "chain",
        "chain_overlap",
        "ring_drag",
        "ring_hover",
        "ring_preset",
        "ring_delocalized",
        "template",
        "bond",
        "dotted",
        "atom",
        "arrow_draw",
        "arrow_selected",
        "arrow_handle",
        "graphic",
        "edit_points",
        "erase",
        "line_short",
        "line_long",
        "keyboard_target",
        "pages_grid",
        "dark_grid_move",
    ] {
        let mut doc = paper.clone();
        match mode {
            "pages_grid" => doc.page_layout = Some(Default::default()),
            "dark_grid_move" => doc.canvas_theme = CanvasTheme::Dark,
            _ => {}
        }
        let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        canvas.camera = camera;
        let mut state = State::default();
        state.cursor = match mode {
            "idle" => None,
            "selected" => {
                canvas.selected = &all;
                None
            }
            "hover" => Some(screen(position(ring_atom))),
            "select_rect" => {
                state.gesture = Some(Gesture::Select {
                    start: World::new(150., 150.),
                });
                Some(at(260., 260.))
            }
            "lasso" => {
                state.gesture = Some(Gesture::Lasso {
                    points: vec![
                        World::new(150., 150.),
                        World::new(270., 150.),
                        World::new(270., 270.),
                    ],
                });
                Some(at(150., 270.))
            }
            "move" | "copy_move" | "dark_grid_move" => {
                canvas.selected = &ring;
                canvas.grid = mode == "dark_grid_move";
                state.gesture = Some(Gesture::Move {
                    start: position(ring_atom),
                    ids: ring.clone(),
                    clicked: vec![ring_atom],
                });
                if mode == "copy_move" {
                    state.modifiers = command;
                }
                Some(offset(position(ring_atom), 30., 20.))
            }
            "rotate" | "edge" => {
                canvas.selected = &all;
                state.gesture = Some(transform(if mode == "rotate" {
                    Handle::Rotate
                } else {
                    Handle::Edge(1)
                }));
                Some(at(325., 235.))
            }
            "tilt" => {
                canvas.tool = Tool::Tilt;
                canvas.selected = &ring;
                let start = at(200., 200.);
                state.gesture = Some(Gesture::Tilt(tilt::TiltDrag {
                    ids: ring.clone(),
                    start,
                }));
                Some(start + Vector::new(30., -20.))
            }
            "chain" => {
                canvas.tool = Tool::Chain(ChainMode::Straight);
                state.gesture = Some(chain(World::new(300., 380.)));
                Some(at(400., 380.))
            }
            "chain_overlap" => {
                canvas.tool = Tool::Chain(ChainMode::Straight);
                state.gesture = Some(chain(position(ring_atom)));
                Some(screen(across))
            }
            "ring_drag" => {
                canvas.tool = Tool::Ring;
                state.gesture = Some(Gesture::Ring {
                    start: position(n),
                    attached: true,
                });
                Some(offset(position(n), 0., -40.))
            }
            // O already has its bond to N, so a spiro ring there is rejected:
            // the hover draws the rejection outline, atom marker and badge.
            "ring_hover" => {
                canvas.tool = Tool::Ring;
                let rejection = reshiki::editing::ring_placement(
                    &doc,
                    position(o),
                    canvas.ring_size,
                    canvas.aromatic_ring,
                    10.,
                    None,
                )
                .expect_err("ring_hover must hover a rejected ring attachment");
                assert!(
                    !rejection.outline.is_empty() && rejection.atom == Some(o),
                    "ring_hover rejection must draw its outline and atom marker"
                );
                Some(screen(position(o)))
            }
            "ring_preset" | "ring_delocalized" => {
                canvas.tool = Tool::RingPreset(reshiki::rings::Preset::Benzene);
                if mode == "ring_delocalized" {
                    state.modifiers = command;
                }
                Some(at(560., 120.))
            }
            "template" => {
                canvas.tool = Tool::Template;
                canvas.template = Some((template, Anchor::Auto));
                Some(screen(position(o)))
            }
            "bond" => {
                canvas.tool = Tool::Bond(1);
                state.gesture = Some(Gesture::Draw {
                    start: position(o),
                    id: Some(o),
                });
                Some(offset(position(o), 40., -25.))
            }
            "dotted" => {
                canvas.tool = Tool::StyledBond(reshiki::bonds::BondPreset::Dotted);
                let start = World::new(560., 200.);
                state.gesture = Some(Gesture::Draw { start, id: None });
                Some(offset(start, 40., 0.))
            }
            "atom" => {
                canvas.tool = Tool::Atom;
                state.gesture = Some(Gesture::Draw {
                    start: position(n),
                    id: Some(n),
                });
                Some(offset(position(n), 0., 45.))
            }
            "arrow_draw" => {
                canvas.tool = Tool::Arrow;
                let start = World::new(300., 40.);
                state.gesture = Some(Gesture::Draw { start, id: None });
                Some(offset(start, 80., 10.))
            }
            "arrow_selected" => {
                canvas.selected = &arrow_only;
                None
            }
            "arrow_handle" => {
                canvas.selected = &arrow_only;
                state.gesture = Some(Gesture::ArrowHandle {
                    id: arrow,
                    index: 1,
                });
                Some(at(430., 320.))
            }
            "graphic" => {
                canvas.tool = Tool::Graphic(GraphicKind::Ellipse);
                state.gesture = Some(Gesture::Graphic {
                    start: World::new(80., 320.),
                });
                Some(at(150., 380.))
            }
            "edit_points" => {
                canvas.tool = Tool::EditPoints;
                canvas.selected = &edit_points;
                None
            }
            "erase" => {
                canvas.tool = Tool::Erase;
                Some(at(300., 200.))
            }
            // Tool::Select passes the gesture overlay guard, and the selection box
            // drawn after its early return distinguishes a short drag from a long one.
            "line_short" | "line_long" => {
                canvas.selected = &all;
                let start = World::new(560., 250.);
                state.gesture = Some(Gesture::Draw { start, id: None });
                Some(offset(
                    start,
                    if mode == "line_short" { 1. } else { 40. },
                    0.,
                ))
            }
            "keyboard_target" => {
                canvas.keyboard_target = Some(World::new(300., 200.));
                None
            }
            "pages_grid" => {
                canvas.grid = true;
                None
            }
            _ => unreachable!("unknown paper mode {mode}"),
        };
        let pointer = match state.cursor {
            Some(point)
                if matches!(
                    mode,
                    "hover"
                        | "ring_hover"
                        | "ring_preset"
                        | "ring_delocalized"
                        | "template"
                        | "erase"
                ) =>
            {
                mouse::Cursor::Available(point)
            }
            _ => mouse::Cursor::Unavailable,
        };
        let mut render = || {
            renderer.reset(bounds);
            for geometry in canvas.draw(&state, &renderer, &Theme::Light, bounds, pointer) {
                renderer.draw_geometry(geometry);
            }
            Headless::screenshot(&mut renderer, iced::Size::new(640, 400), 1., Color::WHITE)
        };
        let pixels = render();
        assert_same_pixels(&format!("{mode}: cold/warm paper"), &pixels, &render());
        check_pixels(&directory, &format!("paper_{mode}"), &pixels, capture);
        if let Some(idle) = &idle {
            assert!(
                pixels != *idle,
                "mode {mode} does not reach its draw branch"
            );
        }
        match mode {
            "idle" => idle = Some(pixels),
            "line_short" => line_short = Some(pixels),
            "line_long" => assert!(
                line_short.as_ref() != Some(&pixels),
                "line_long does not draw past line_short's early return"
            ),
            _ => {}
        }
    }
}

#[tokio::test]
#[ignore = "Actual native renderer alpha pixels; requires a headless renderer"]
async fn rear_opacity_canvas_matches_half_ink_and_keeps_front_and_filled_marks_clean() {
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
        center: World::new(100., 40.),
        zoom: 2.,
    };
    for theme in CanvasTheme::ALL {
        for alpha in [0., 0.5, 1.] {
            let mut doc = Document::default();
            doc.canvas_theme = theme;
            let ids: Vec<_> = [
                (0., 0., -20.),
                (80., 0., -20.),
                (120., 80., 20.),
                (200., 80., 20.),
            ]
            .into_iter()
            .map(|(x, y, z)| {
                let id = doc.add_atom("C", World::new(x, y));
                doc.atom_mut(id).unwrap().depth = z;
                id
            })
            .collect();
            for pair in ids.windows(2) {
                doc.add_bond(pair[0], pair[1], 1, "plain");
            }
            reshiki::scientific::attach(
                doc.atom_mut(ids[0]).unwrap(),
                reshiki::scientific::SymbolKind::LonePair,
                World::new(0., -18.),
            )
            .unwrap();
            reshiki::depth_appearance::set_rear_opacity(&mut doc, &ids, alpha).unwrap();
            renderer.reset(bounds);
            let mut frame = layered::Frame::new(&renderer, bounds.size()).with_canvas(theme);
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
            draw_primitives(
                &mut frame,
                &reshiki::scene::primitives(&doc),
                camera,
                bounds,
                0.,
            );
            for geometry in frame.finish() {
                renderer.draw_geometry(geometry);
            }
            let pixels =
                Headless::screenshot(&mut renderer, iced::Size::new(640, 400), 1., Color::WHITE);
            let darkness = |point: World| {
                let screen = camera.screen(point, bounds);
                let x = screen.x.round() as usize;
                let y = screen.y.round() as usize;
                (x.saturating_sub(2)..=(x + 2).min(639))
                    .flat_map(|x| {
                        (y.saturating_sub(2)..=(y + 2).min(399)).map(move |y| (y * 640 + x) * 4)
                    })
                    .map(|offset| {
                        let channel = pixels[offset];
                        if theme.is_light() {
                            255 - channel
                        } else {
                            channel
                        }
                    })
                    .max()
                    .unwrap()
            };
            let expected = (alpha * 255.) as i16;
            assert!(
                (i16::from(darkness(World::new(40., 0.))) - expected).abs() <= 2,
                "Rear ink alpha {alpha} {theme:?}"
            );
            assert_eq!(darkness(World::new(160., 80.)), 255, "Front ink changed");
            assert!(
                (i16::from(darkness(World::new(-2.1, -18.))) - expected).abs() <= 3,
                "Filled lone-pair alpha blended twice"
            );
        }
    }
}
