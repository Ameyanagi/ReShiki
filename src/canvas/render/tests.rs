use super::*;

#[test]
fn rotated_picture_and_text_bounds_keep_corners_and_underlines_visible() {
    let bounds = Rectangle::with_size(iced::Size::new(100., 100.));
    let picture = Rectangle {
        x: 105.,
        y: 0.,
        width: 10.,
        height: 100.,
    };
    assert!(!picture_visible(picture, 0., bounds));
    assert!(picture_visible(
        picture,
        std::f32::consts::FRAC_PI_4,
        bounds
    ));
    let ink = Some(Rectangle {
        x: 0.,
        y: 0.,
        width: 30.,
        height: 20.,
    });
    let at = Point::new(110., 10.);
    assert!(!text_visible(ink, at, 1., 0., None, None, bounds));
    assert!(text_visible(
        ink,
        at,
        1.,
        0.,
        Some((Point::new(100., 50.), 180.)),
        None,
        bounds
    ));
    assert!(
        text_visible(
            None,
            at,
            1.,
            0.,
            None,
            Some(Rectangle {
                x: 1.,
                y: 10.,
                width: 30.,
                height: 1.
            }),
            bounds
        ),
        "An underline can be the run's only visible ink"
    );
    assert!(text_visible(
        ink,
        Point::new(f32::NAN, 0.),
        1.,
        0.,
        None,
        None,
        bounds
    ));
}

#[tokio::test]
#[ignore = "Rotated/offscreen culling parity; requires a headless renderer"]
async fn early_culling_matches_unculled_text_pictures_and_halos() {
    use iced::advanced::{Renderer as _, graphics::geometry::Renderer as _, renderer::Headless};
    use iced::{Renderer, Size};
    use reshiki::{canvas_theme::CanvasTheme, pictures::Picture, typography::TextStyle};
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    let pixels = image::RgbaImage::from_pixel(4, 4, image::Rgba([20, 100, 220, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let picture = Picture::import(png.get_ref()).unwrap();
    let bounds = Rectangle::with_size(Size::new(240., 160.));
    let mut offscreen = picture.graphic(900, World::new(10000., 10000.));
    offscreen.axis_y.y *= -1.;
    PICTURE_HANDLES.set(0);
    let mut frame = layered::Frame::new(&renderer, bounds.size());
    draw_primitives(
        &mut frame,
        &[Primitive::Picture(Box::new(offscreen))],
        Camera::default(),
        bounds,
        0.,
    );
    assert_eq!(
        PICTURE_HANDLES.get(),
        0,
        "An offscreen mirrored image must not resolve/decode its handle"
    );
    for theme in CanvasTheme::ALL {
        for zoom in [0.45, 1.09, 2.5] {
            for angle in [0., 45., 180., -70.] {
                for family in ["Arial", "Times New Roman"] {
                    let case = format!("{theme:?} {zoom} {angle} {family}");
                    if std::env::var("RESHIKI_CULL_CASE").is_ok_and(|wanted| wanted != case) {
                        continue;
                    }
                    let style = TextStyle {
                        family: family.into(),
                        italic: family == "Times New Roman",
                        underline: true,
                        ..Default::default()
                    };
                    let mut scene = Vec::new();
                    for (i, (x, y, text)) in [
                        (-30., 12., "AV office fi"),
                        (215., 70., "日本語 🧪"),
                        (80., -18., "gypj"),
                        (90., 150., "     "),
                        (1500., -2000., "OFFSCREEN"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let text = Primitive::Text {
                            position: World::new(x, y),
                            text: text.into(),
                            size: 22.,
                            color: [20, 50, 90],
                            style: style.clone(),
                        };
                        scene.push(Primitive::Halo {
                            radius: 2.,
                            primitives: vec![text.clone()],
                        });
                        scene.push(Primitive::Rotated {
                            pivot: World::new(x, y),
                            angle,
                            primitives: vec![text],
                        });
                        let mut g = picture.graphic(i as u64 + 1, World::new(x, y));
                        let (sin, cos) = angle.to_radians().sin_cos();
                        g.axis_x = World::new(cos * 40., sin * 40.);
                        g.axis_y = World::new(sin * 20., -cos * 20.);
                        scene.push(Primitive::Picture(Box::new(g)));
                    }
                    let camera = Camera {
                        center: World::new(120., 80.),
                        zoom,
                    };
                    let mut capture = |enabled, primitives: &[Primitive]| {
                        CULL.set(enabled);
                        let cache = std::cell::RefCell::default();
                        renderer.reset(bounds);
                        let mut frame = layered::Frame::new(&renderer, bounds.size())
                            .with_canvas(theme)
                            .with_text_cache(&cache);
                        draw_primitives(&mut frame, primitives, camera, bounds, 0.);
                        for layer in frame.finish() {
                            renderer.draw_geometry(layer);
                        }
                        Headless::screenshot(&mut renderer, Size::new(240, 160), 1., Color::WHITE)
                    };
                    let unculled = capture(false, &scene);
                    let culled = capture(true, &scene);
                    if culled != unculled {
                        let difference = |a: &[u8], b: &[u8]| {
                            let mut count = 0;
                            let mut bounds = [240, 160, 0, 0];
                            let mut worst = 0;
                            for (i, (a, b)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate()
                            {
                                let delta =
                                    a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                                if delta > 0 {
                                    count += 1;
                                    let (x, y) = (i % 240, i / 240);
                                    bounds = [
                                        bounds[0].min(x),
                                        bounds[1].min(y),
                                        bounds[2].max(x),
                                        bounds[3].max(y),
                                    ];
                                    worst = worst.max(delta);
                                }
                            }
                            (count, bounds, worst)
                        };
                        let repeated_reference = capture(false, &scene);
                        let repeated_culled = capture(true, &scene);
                        eprintln!(
                            "CULL {case}: changed/count-bounds-max={:?}, reference repeat={:?}, culled repeat={:?}",
                            difference(&unculled, &culled),
                            difference(&unculled, &repeated_reference),
                            difference(&culled, &repeated_culled)
                        );
                        if let Ok(path) = std::env::var("RESHIKI_CULL_PIXELS") {
                            let path = std::path::PathBuf::from(path);
                            std::fs::create_dir_all(&path).unwrap();
                            for (name, pixels) in [
                                ("reference", &unculled),
                                ("culled", &culled),
                                ("reference-repeat", &repeated_reference),
                                ("culled-repeat", &repeated_culled),
                            ] {
                                image::save_buffer(
                                    path.join(format!("{name}.png")),
                                    pixels,
                                    240,
                                    160,
                                    image::ColorType::Rgba8,
                                )
                                .unwrap();
                            }
                        }
                        for (i, primitive) in scene.iter().enumerate() {
                            let reference = capture(false, std::slice::from_ref(primitive));
                            let culled = capture(true, std::slice::from_ref(primitive));
                            if reference != culled {
                                eprintln!(
                                    "CULL primitive {i} {:?}: {:?}",
                                    std::mem::discriminant(primitive),
                                    difference(&reference, &culled)
                                );
                            }
                        }
                        CULL.set(true);
                        panic!(
                            "Culling changed pixels for {case}: {:?}",
                            difference(&unculled, &culled)
                        );
                    }
                }
            }
        }
    }
    CULL.set(true);
}

/// Text outlined at a zoom step and scaled up to a zoom between steps is
/// kept while any of it reaches the viewport, as is its halo.
#[test]
fn zoomed_text_partly_on_screen_between_zoom_steps_is_not_culled() {
    let zoom = 1.09;
    let step = text_cache::zoom_step(zoom);
    assert!(step < zoom);
    let paths = text_cache::outline("OH", 40., step, [0, 0, 0], &Default::default());
    let text = paths.bounds.unwrap();
    let scale = zoom / step;
    // Only the scaled text's last pixels reach the left edge.
    let right = (text.x + text.width) * scale;
    let at = Point::new(1. - right, 10.);
    let viewport = Rectangle::with_size(iced::Size::new(200., 100.));
    assert!(on_screen(&paths, at, scale, 0., viewport));
    assert!(on_screen(&paths, at, scale, 3., viewport));
    // Wholly off screen, it is culled.
    let at = Point::new(-right - 1., 10.);
    assert!(!on_screen(&paths, at, scale, 0., viewport));
}
