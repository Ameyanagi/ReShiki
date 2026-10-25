use super::*;
use reshiki::document::Document;
use reshiki::models3d::{Display, ForceField, Model3d, Placed, Style, set_display};
use std::collections::HashMap;

/// Ethanol at `x`, shown only as a 3D model.
fn add_ethanol(doc: &mut Document, x: f32) {
    let c1 = doc.add_atom("C", World::new(x, 0.));
    let c2 = doc.add_atom("C", World::new(x + 36., -21.));
    let o = doc.add_atom("O", World::new(x + 72., 0.));
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
        .push(Model3d::new(doc, atoms, vec![], ForceField::Mmff94s));
    set_display(doc, doc.models.len() - 1, Display::ThreeD);
}

/// Ethanol shown only as a 3D model, in `style`.
fn model(style: Style) -> Document {
    let mut doc = Document::default();
    add_ethanol(&mut doc, 0.);
    doc.models[0].style = style;
    doc
}

/// Keep the actual exported viewBox and paint body, while registering its
/// physical viewport in world units for pixel comparison. Independent f32 pt
/// conversions and xMidYMid meet can otherwise move an antialias sample.
fn registered_svg(svg: &str, (lo, hi): (World, World)) -> String {
    let xml = roxmltree::Document::parse(svg).unwrap();
    let root = xml.root_element();
    let view_box: Vec<_> = root
        .attribute("viewBox")
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(view_box.len(), 4);
    assert_eq!(
        view_box
            .iter()
            .map(|value| value.parse::<f32>().unwrap())
            .collect::<Vec<_>>(),
        [lo.x, lo.y, hi.x - lo.x, hi.y - lo.y],
        "The export viewBox must match the canvas bounds"
    );
    let mut replacements = vec![];
    for (attribute, extent) in [("width", view_box[2]), ("height", view_box[3])] {
        let attribute = root.attribute_node(attribute).unwrap();
        let physical: f32 = attribute
            .value()
            .strip_suffix("pt")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            physical,
            extent.parse::<f32>().unwrap() * reshiki::style::DEFAULT.points_per_world(),
            "The original export must retain both physical dimensions"
        );
        replacements.push((attribute.range_value(), extent));
    }
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut normalized = svg.to_string();
    for (range, extent) in replacements {
        normalized.replace_range(range, extent);
    }
    normalized
}

fn render_export(
    svg: &str,
    bounds: (World, World),
    scale: f32,
    width: u32,
    height: u32,
) -> resvg::tiny_skia::Pixmap {
    let tree =
        resvg::usvg::Tree::from_str(&registered_svg(svg, bounds), &Default::default()).unwrap();
    let mut image = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut image.as_mut(),
    );
    image
}

fn worst_difference(canvas: &[u8], export: &resvg::tiny_skia::Pixmap) -> u8 {
    canvas
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
        .unwrap()
}

#[test]
fn pixel_registration_preserves_export_paint_and_detects_a_real_displacement() {
    let doc = model(Style::Stick);
    let svg = scene::svg(&doc);
    let run = scene::primitives(&doc);
    let (lo, hi) = scene::bounds(&run);
    let start = svg.find('>').unwrap() + 1;
    let end = svg.rfind("</svg>").unwrap();
    let normalized = registered_svg(&svg, (lo, hi));
    assert_eq!(
        &normalized[normalized.find('>').unwrap() + 1..],
        &svg[start..]
    );
    let displaced = format!(
        "{}<g transform=\"translate(1 0)\">{}</g>{}",
        &svg[..start],
        &svg[start..end],
        &svg[end..]
    );
    let scale = 4. * OVERSAMPLE;
    let (width, height) = (
        ((hi.x - lo.x) * scale).ceil() as u32,
        ((hi.y - lo.y) * scale).ceil() as u32,
    );
    let canvas = pixels(&run, lo, scale, width, height).unwrap();
    assert!(
        worst_difference(
            &canvas,
            &render_export(&displaced, (lo, hi), scale, width, height)
        ) > 6,
        "Viewport registration must not hide a visible paint/position error"
    );
}

#[test]
#[should_panic(expected = "The export viewBox must match the canvas bounds")]
fn pixel_registration_rejects_coupled_viewbox_and_physical_width_changes() {
    let doc = model(Style::Stick);
    let svg = scene::svg(&doc);
    let (lo, hi) = scene::bounds(&scene::primitives(&doc));
    let xml = roxmltree::Document::parse(&svg).unwrap();
    let root = xml.root_element();
    let width = hi.x - lo.x + 1.;
    let mut replacements = [
        (
            root.attribute_node("viewBox").unwrap().range_value(),
            format!("{} {} {width} {}", lo.x, lo.y, hi.y - lo.y),
        ),
        (
            root.attribute_node("width").unwrap().range_value(),
            format!("{}pt", width * reshiki::style::DEFAULT.points_per_world()),
        ),
    ];
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut inflated = svg.clone();
    for (range, value) in replacements {
        inflated.replace_range(range, &value);
    }
    let _ = registered_svg(&inflated, (lo, hi));
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

        let export = render_export(&scene::svg(&doc), (lo, hi), scale, width, height);
        let worst = worst_difference(&canvas, &export);
        // Anti-aliased edges may round differently under the two transforms.
        assert!(
            worst <= 6,
            "{style:?}: canvas differs from export by {worst}"
        );

        // Across the front ball's middle row, neighbouring pixels differ by
        // at most a few levels: the gradient is continuous. A ball meeting
        // others has its seams across it instead.
        let front = run.iter().rev().find_map(|p| match p {
            Primitive::Sphere {
                center,
                radius,
                contact: None,
                ..
            } => Some((center, radius)),
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
        crate::canvas::render::draw_primitives(
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

/// Tiles sit on a grid fixed to the drawing: a pan reuses the tiles still in
/// view, the gap between two models has none, and editing one model leaves
/// the other's tiles cached.
#[test]
fn tiles_survive_a_pan_and_another_models_edit() {
    let mut doc = Document::default();
    add_ethanol(&mut doc, 0.);
    add_ethanol(&mut doc, 600.);
    let bounds = Rectangle::with_size(Size::new(1600., 900.));
    let camera = Camera {
        center: World::new(300., 0.),
        zoom: 1.5,
    };
    let keys = |doc: &Document, camera| -> HashMap<_, _> {
        plan(&scene::primitives(doc), camera, bounds)
            .into_iter()
            .map(|tile| (tile.rect, tile.key))
            .collect()
    };
    let before = keys(&doc, camera);
    let panned = Camera {
        center: World::new(313.7, 5.3),
        ..camera
    };
    assert_eq!(keys(&doc, panned), before);
    let middle = (300. * camera.zoom * OVERSAMPLE) as i64;
    assert!(
        before.keys().all(|r| r[2] < middle || r[0] > middle),
        "no tile spans the gap: {before:?}"
    );

    doc.models[1].rotation = [0., 0., 1., 0.];
    let edited = keys(&doc, camera);
    let left = |keys: &HashMap<[i64; 4], u64>| -> Vec<_> {
        let mut left: Vec<_> = keys.iter().filter(|(r, _)| r[2] < middle).collect();
        left.sort();
        left.into_iter().map(|(r, k)| (*r, *k)).collect()
    };
    assert!(!left(&before).is_empty());
    assert_eq!(left(&edited), left(&before));
    assert_ne!(edited, before);
}

#[test]
fn the_tile_cache_keeps_to_its_budget() {
    let mut cache = Cache::default();
    let bytes = (TILE * TILE * 4) as usize;
    let count = (BUDGET / bytes + 20) as u64;
    for key in 0..count {
        let tile = cache.get(key, [TILE as u32; 2], || Some(vec![0; bytes]));
        assert!(tile.is_some() && cache.bytes <= BUDGET);
        // The first tile stays in use, so it outlives the ones after it.
        let _ = cache.get(0, [TILE as u32; 2], || None);
    }
    assert!(cache.tiles.contains_key(&0) && cache.tiles.contains_key(&(count - 1)));
    assert!(!cache.tiles.contains_key(&1));
}

/// Tiles render only the primitives reaching them, yet each matches the
/// whole model rendered over it, and no ink lies outside them.
#[test]
fn tiles_together_match_the_whole_model() {
    for style in Style::ALL {
        let doc = model(style);
        let run = scene::primitives(&doc);
        let (lo, hi) = scene::bounds(&run);
        let camera = Camera {
            center: World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
            zoom: 6.,
        };
        let tiles = plan(&run, camera, Rectangle::with_size(Size::new(2000., 2000.)));
        assert!(tiles.len() > 4, "{style:?}");
        let scale = camera.zoom * OVERSAMPLE;
        let world = |x: i64, y: i64| World::new(x as f32 / scale, y as f32 / scale);
        for tile in &tiles {
            let [x0, y0, x1, y1] = tile.rect;
            let part = primitives(&run, &tile.parts);
            let size = ((x1 - x0) as u32, (y1 - y0) as u32);
            let image = pixels(&part, world(x0, y0), scale, size.0, size.1).unwrap();
            let whole = pixels(&run, world(x0, y0), scale, size.0, size.1).unwrap();
            assert_eq!(image, whole, "{style:?} {:?}", tile.rect);
        }
        // Every inked pixel of the model lies in some tile.
        let (x, y) = (
            (lo.x * scale).floor() as i64 - 4,
            (lo.y * scale).floor() as i64 - 4,
        );
        let (w, h) = (
            ((hi.x - lo.x) * scale) as u32 + 8,
            ((hi.y - lo.y) * scale) as u32 + 8,
        );
        let whole = pixels(&run, world(x, y), scale, w, h).unwrap();
        for (i, pixel) in whole.chunks(4).enumerate() {
            let (px, py) = (x + (i as u32 % w) as i64, y + (i as u32 / w) as i64);
            assert!(
                pixel[3] == 0
                    || tiles.iter().any(|t| {
                        let [x0, y0, x1, y1] = t.rect;
                        (x0..x1).contains(&px) && (y0..y1).contains(&py)
                    }),
                "{style:?}: ink outside the tiles at {px},{py}"
            );
        }
    }
}

/// A tile's contour holds exactly the bond outlines reaching it, as one
/// stroke: flat sticks draw every outline in one contour, and no tile copies
/// them all.
#[test]
fn a_tile_contour_holds_only_the_outlines_reaching_it() {
    let doc = model(Style::Stick);
    let run = scene::primitives(&doc);
    let (index, all) = run
        .iter()
        .enumerate()
        .find_map(|(i, p)| contour(p).map(|polygons| (i, polygons.to_vec())))
        .expect("flat sticks draw a contour");
    let (lo, hi) = scene::bounds(&run);
    let camera = Camera {
        center: World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
        zoom: 6.,
    };
    let scale = camera.zoom * OVERSAMPLE;
    let tiles = plan(&run, camera, Rectangle::with_size(Size::new(2000., 2000.)));
    let mut fewer = false;
    for tile in &tiles {
        let [x0, y0, x1, y1] = tile.rect.map(|v| v as f32);
        let contours: Vec<_> = primitives(&run, &tile.parts)
            .into_iter()
            .filter(|p| contour(p).is_some())
            .collect();
        assert!(contours.len() <= 1, "one stroke per tile: {:?}", tile.rect);
        let kept = contours.first().and_then(contour).unwrap_or_default();
        for (j, polygon) in all.iter().enumerate() {
            let (lo, hi) = scene::bounds(&[keep_polygons(&run[index], &[j])]);
            let reaches = ((lo.x - MARGIN) * scale).floor() < x1
                && ((hi.x + MARGIN) * scale).ceil() + 1. > x0
                && ((lo.y - MARGIN) * scale).floor() < y1
                && ((hi.y + MARGIN) * scale).ceil() + 1. > y0;
            assert_eq!(
                kept.contains(polygon),
                reaches,
                "{:?} polygon {j}",
                tile.rect
            );
        }
        fewer |= !kept.is_empty() && kept.len() < all.len();
    }
    assert!(fewer, "some tile holds fewer outlines than the model");
}

/// During the 2D → 3D morph the fading hydrogens stay in the model's run of
/// tiles in every style, and render.
#[test]
fn a_morphing_model_draws_as_tiles_in_every_style() {
    for style in Style::ALL {
        let mut doc = model(style);
        let o = doc.models[0].atoms[2].id;
        doc.models[0].hydrogens.push(reshiki::models3d::Hydrogen {
            parent: o,
            xyz: [1.8, 0.4, 0.3],
        });
        // Far enough out that a space-filling hydrogen is no longer hidden
        // inside its atom.
        doc.models[0].morph = Some(0.8);
        let run = scene::primitives(&doc);
        assert!(run.iter().all(is_model), "{style:?}");
        assert!(
            run.iter().any(|p| matches!(p, Primitive::Opacity { .. })),
            "{style:?}: the hydrogen fades in"
        );
        let (lo, hi) = scene::bounds(&run);
        let image = pixels(
            &run,
            lo,
            OVERSAMPLE,
            ((hi.x - lo.x) * OVERSAMPLE).ceil() as u32,
            ((hi.y - lo.y) * OVERSAMPLE).ceil() as u32,
        )
        .unwrap();
        assert!(image.chunks(4).any(|p| p[3] > 0), "{style:?}");
    }
}

/// The two halves of a bond meet without a pale anti-aliasing seam: at
/// 1200 dpi every pixel along the C–C stick across its middle is opaque, in
/// each style with sticks, whatever the view.
#[test]
fn a_stick_has_no_seam_where_its_halves_meet() {
    for style in [Style::BallStick, Style::Stick] {
        let mut doc = model(style);
        for rotation in [
            doc.models[0].rotation,
            reshiki::models3d::turn(doc.models[0].rotation, 37., 11., false),
            reshiki::models3d::turn(doc.models[0].rotation, -23., 52., true),
        ] {
            doc.models[0].rotation = rotation;
            let (balls, sticks) =
                reshiki::models3d::balls_and_sticks(&doc, &doc.models[0]).unwrap();
            // The half from C1 ends at the C1–C2 middle.
            let c1 = balls[0].0;
            let &(_, middle, width) = sticks
                .iter()
                .find(|(a, e, _)| {
                    a.distance(c1) < 1e-3 && e.distance(balls[1].0) < c1.distance(balls[1].0)
                })
                .unwrap();
            let d = c1.distance(middle);
            let along = ((middle.x - c1.x) / d, (middle.y - c1.y) / d);
            let across = (-along.1, along.0);
            let run = scene::primitives(&doc);
            let scale = 1200. / 72.;
            let lo = middle.offset(-4. * width, -4. * width);
            let side = (8. * width * scale).ceil() as u32;
            let image = pixels(&run, lo, scale, side, side).unwrap();
            let alpha = |p: World| {
                let (x, y) = (((p.x - lo.x) * scale) as u32, ((p.y - lo.y) * scale) as u32);
                image[((y * side + x) * 4 + 3) as usize]
            };
            for i in -20..=20 {
                for j in -5..=5 {
                    let (u, v) = (i as f32 * 0.02 * width, j as f32 * 0.1 * width);
                    let p = middle.offset(along.0 * u + across.0 * v, along.1 * u + across.1 * v);
                    assert_eq!(alpha(p), 255, "{style:?} {rotation:?} at {i}, {j}");
                }
            }
        }
    }
}

/// Neighbouring sticks meet at a joint without a pale seam along their
/// shared border: at 1200 dpi every pixel around the middle carbon is
/// opaque, whatever the view.
#[test]
fn a_stick_joint_has_no_seam_between_its_sticks() {
    let mut doc = model(Style::Stick);
    for rotation in [
        doc.models[0].rotation,
        reshiki::models3d::turn(doc.models[0].rotation, 37., 11., false),
        reshiki::models3d::turn(doc.models[0].rotation, -23., 52., true),
    ] {
        doc.models[0].rotation = rotation;
        let (balls, sticks) = reshiki::models3d::balls_and_sticks(&doc, &doc.models[0]).unwrap();
        let joint = balls[1].0;
        let width = sticks
            .iter()
            .find(|(a, ..)| a.distance(joint) < 1e-3)
            .unwrap()
            .2;
        let run = scene::primitives(&doc);
        let scale = 1200. / 72.;
        let lo = joint.offset(-2. * width, -2. * width);
        let side = (4. * width * scale).ceil() as u32;
        let image = pixels(&run, lo, scale, side, side).unwrap();
        for i in -9..=9 {
            for j in -9..=9 {
                let p = joint.offset(i as f32 * 0.1 * width, j as f32 * 0.1 * width);
                if p.distance(joint) > 0.9 * width {
                    continue;
                }
                let (x, y) = (((p.x - lo.x) * scale) as u32, ((p.y - lo.y) * scale) as u32);
                let alpha = image[((y * side + x) * 4 + 3) as usize];
                assert_eq!(alpha, 255, "{rotation:?} at {i}, {j}");
            }
        }
    }
}
