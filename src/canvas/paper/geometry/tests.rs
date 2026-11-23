use super::*;
use iced::advanced::{Renderer as _, graphics::geometry::Renderer as _, renderer::Headless};
use iced::{Color, Point, Renderer, Size};
use reshiki::{canvas_theme::CanvasTheme, document::Document};

#[tokio::test]
#[ignore = "Mixed retained/live scene parity; requires a headless renderer"]
async fn mixed_scene_retains_vectors_around_live_tiles_in_paint_order() {
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let bounds = Rectangle::with_size(Size::new(320., 240.));
    let mut doc = Document::default();
    let id = doc.add_atom("N", World::new(100., 100.));
    let markers = Rc::new(Markers::new(&doc, &[id]));
    let mut model = reshiki::models3d::Model3d::new(
        &doc,
        vec![reshiki::models3d::Placed {
            id,
            xyz: [0., 0., 0.],
        }],
        vec![],
        reshiki::models3d::ForceField::Mmff94s,
    );
    model.display = reshiki::models3d::Display::ThreeD;
    doc.models.push(model);
    let ball = |x| Primitive::Sphere {
        center: World::new(x, 110.),
        radius: 35.,
        focal: World::new(x - 10., 100.),
        stops: vec![(0., [250, 200, 70]), (1., [90, 30, 10])],
        outline: None,
        contact: None,
    };
    let scene: Rc<[Primitive]> = vec![
        Primitive::Line(World::new(20., 110.), World::new(300., 110.), 8.),
        ball(110.),
        Primitive::Line(World::new(20., 95.), World::new(300., 125.), 4.),
        Primitive::Opacity {
            alpha: 0.65,
            primitive: Box::new(ball(205.)),
        },
        Primitive::Line(World::new(20., 130.), World::new(300., 90.), 2.),
    ]
    .into();
    let text = std::cell::RefCell::default();
    for theme in CanvasTheme::ALL {
        for zoom in [0.7, 1., 1.25] {
            let mut retained = SceneGeometryCache::default();
            let mut camera = Camera {
                center: World::new(160., 120.),
                zoom,
            };
            for step in 0..5 {
                if step > 0 {
                    camera.center = camera.center.offset(7., -3.);
                }
                // The same scene must keep its vector layers when live raster
                // sampling switches from turning quality back to resting.
                doc.models[0].turning = step < 3;
                models3d::begin(&doc);
                let mut render = |cache: Option<&mut SceneGeometryCache>| {
                    renderer.reset(bounds);
                    let mut frame = layered::Frame::new(&renderer, bounds.size())
                        .with_canvas(theme)
                        .with_text_cache(&text);
                    frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
                    crate::canvas::render::DRAWN_PRIMITIVES.with(|n| n.set(0));
                    if let Some(cache) = cache {
                        cache.draw(
                            &mut frame,
                            (markers.clone(), scene.clone()),
                            camera,
                            bounds,
                            &[],
                            false,
                        );
                    } else {
                        markers.draw_with_nmr(&mut frame, camera, bounds, true, &[]);
                        draw_primitives(&mut frame, &scene, camera, bounds, 0.);
                    }
                    let drawn = crate::canvas::render::DRAWN_PRIMITIVES.with(std::cell::Cell::get);
                    let (layers, shifts) = frame.finish_shifted();
                    for (i, layer) in layers.into_iter().enumerate() {
                        let shift = shifts
                            .iter()
                            .find(|(range, _)| range.contains(&i))
                            .map_or(Vector::ZERO, |(_, shift)| *shift);
                        renderer.with_layer(bounds, |renderer| {
                            renderer
                                .with_translation(shift, |renderer| renderer.draw_geometry(layer))
                        });
                    }
                    (
                        Headless::screenshot(&mut renderer, Size::new(320, 240), 1., Color::WHITE),
                        drawn,
                    )
                };
                let (live, _) = render(None);
                let (cold, _) = render(Some(&mut retained));
                let (warm, drawn) = render(Some(&mut retained));
                if theme == CanvasTheme::Light
                    && zoom == 1.
                    && step == 4
                    && let Some(directory) = std::env::var_os("RESHIKI_RENDER_EVIDENCE")
                {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    for (name, pixels) in [("live.png", &live), ("retained.png", &warm)] {
                        image::RgbaImage::from_raw(320, 240, pixels.clone())
                            .unwrap()
                            .save(directory.join(name))
                            .unwrap();
                    }
                }
                let gold = warm
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[0] > p[1].saturating_add(15) && p[1] > p[2].saturating_add(15))
                    .count();
                assert!(
                    gold > 200,
                    "The raster balls must actually appear: {gold} gold pixels"
                );
                assert_eq!(drawn, 0, "No vector primitives are tessellated again");
                assert_eq!(warm, cold, "A warm frame must preserve every layer");
                let error: usize = live
                    .iter()
                    .zip(&warm)
                    .map(|(a, b)| a.abs_diff(*b) as usize)
                    .sum();
                let changed = live
                    .chunks(4)
                    .zip(warm.chunks(4))
                    .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 8))
                    .count();
                assert!(
                    changed < 320 * 240 / 1000 && (error as f64 / live.len() as f64) < 0.1,
                    "Mixed scene/pan parity: theme={theme:?}, zoom={zoom}, step={step}, changed={changed}, error={error}"
                );
            }
        }
    }
    models3d::end();
}
