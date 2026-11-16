//! Opt-in release-mode measurements of the actual canvas Program path.
use super::*;
use iced::advanced::renderer::Headless;
use iced::widget::canvas::Program;
use std::{hint::black_box, time::Instant};

fn measure(name: &str, count: usize, mut run: impl FnMut(usize)) {
    for i in 0..3 {
        run(i);
    }
    let mut samples = Vec::new();
    for i in 0..count {
        let start = Instant::now();
        run(i);
        samples.push(start.elapsed().as_secs_f64() * 1_000.);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "PERF,{name},{count},{:.4},{:.4},{:.4}",
        samples[count / 2],
        samples[(count * 95 / 100).min(count - 1)],
        samples.iter().sum::<f64>() / count as f64
    );
    // Opt-in, process-wide requested Rust bytes. Run this test alone with one
    // test thread. Fixture creation, CPU samples and printing precede reset;
    // renderer-internal allocations are included, native/GPU/RSS bytes are not.
    // This extra pass reuses the outlines prepared by the timed pass above.
    if std::env::var("RESHIKI_PERF_ALLOCATIONS").as_deref() == Ok("1") {
        let baseline = crate::allocation_metrics::reset();
        for i in 0..count {
            run(i);
        }
        let snapshot = crate::allocation_metrics::snapshot();
        println!(
            "ALLOC,{name},{count},{},{},{},{}",
            snapshot.allocated_bytes,
            snapshot.allocation_count,
            snapshot.peak_bytes.saturating_sub(baseline),
            snapshot.live_bytes as i128 - baseline as i128,
        );
    }
}

#[tokio::test]
#[ignore = "Canvas grid profiling; requires a headless renderer"]
async fn manual_dense_grid_cache() {
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    println!("Renderer: {}", renderer.name());
    let doc = Document::default();
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.grid = true;
    canvas.camera.zoom = 10. / doc.drawing_style.bond_length_world;
    let bounds = Rectangle::with_size(iced::Size::new(3840., 2160.));
    let state = State::default();
    println!("PERF,workload,iterations,median_ms,p95_ms,mean_ms");
    measure("4k_grid_rebuilt", 30, |_| {
        state.grid.borrow().geometry.clear();
        black_box(canvas.draw(
            &state,
            &renderer,
            &Theme::Light,
            bounds,
            mouse::Cursor::Unavailable,
        ));
    });
    measure("4k_grid_cached", 30, |_| {
        black_box(canvas.draw(
            &state,
            &renderer,
            &Theme::Light,
            bounds,
            mouse::Cursor::Unavailable,
        ));
    });
}

#[tokio::test]
#[ignore = "Release-mode canvas workload profiling; requires a headless renderer"]
async fn manual_loaded_canvas_workloads() {
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    println!("Renderer: {}", renderer.name());
    let original: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    let iterations = std::env::var("RESHIKI_PERF_ITERATIONS")
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let bounds = Rectangle::with_size(iced::Size::new(1066., 570.));
    println!("PERF,workload,iterations,median_ms,p95_ms,mean_ms");
    for copies in [1, 4] {
        let mut doc = original.clone();
        let (lo, hi) = original.bounds();
        for copy in 1..copies {
            reshiki::editing::append(
                &mut doc,
                &original,
                World::new((hi.x - lo.x + 100.) * copy as f32, 0.),
            );
        }
        let ids = doc.all_ids();
        let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        canvas.camera = Camera {
            center: World::new(378., 294.),
            zoom: 0.5,
        };
        let point = Point::new(350., 250.);
        let cursor = mouse::Cursor::Available(point);
        let mut state = State {
            cursor: Some(point),
            ..Default::default()
        };
        measure(
            &format!("gallery_{copies}x_unselected_draw"),
            iterations,
            |_| {
                black_box(canvas.draw(&state, &renderer, &Theme::Light, bounds, cursor));
            },
        );
        canvas.selected = &ids;
        measure(
            &format!("gallery_{copies}x_selected_draw"),
            iterations,
            |_| {
                black_box(canvas.draw(&state, &renderer, &Theme::Light, bounds, cursor));
            },
        );
        measure(
            &format!("gallery_{copies}x_pointer_hit"),
            iterations,
            |_| {
                black_box(canvas.mouse_interaction(&state, bounds, cursor));
            },
        );
        state.gesture = Some(Gesture::Move {
            start: canvas.camera.world(point, bounds),
            ids: ids.clone(),
            clicked: vec![ids[0]],
        });
        measure(&format!("gallery_{copies}x_drag_all"), iterations, |i| {
            state.cursor = Some(point + Vector::new(20. + i as f32, 15.));
            black_box(canvas.draw(&state, &renderer, &Theme::Light, bounds, cursor));
            black_box(canvas.mouse_interaction(&state, bounds, cursor));
        });
        let partial = &ids[..2];
        canvas.selected = partial;
        state.gesture = Some(Gesture::Move {
            start: canvas.camera.world(point, bounds),
            ids: partial.to_vec(),
            clicked: partial.to_vec(),
        });
        measure(
            &format!("gallery_{copies}x_drag_partial"),
            iterations,
            |i| {
                state.cursor = Some(point + Vector::new(20. + i as f32, 15.));
                black_box(canvas.draw(&state, &renderer, &Theme::Light, bounds, cursor));
            },
        );
        canvas.selected = &ids;
        state.gesture = None;
        measure(&format!("gallery_{copies}x_pan"), iterations, |i| {
            canvas.camera.center.x = 378. + i as f32;
            black_box(canvas.draw(&state, &renderer, &Theme::Light, bounds, cursor));
        });
        measure(&format!("gallery_{copies}x_zoom"), iterations, |i| {
            canvas.camera.zoom = 0.45 + (i % 20) as f32 * 0.005;
            black_box(canvas.draw(&state, &renderer, &Theme::Light, bounds, cursor));
        });
        assert_eq!(doc.atoms.len(), original.atoms.len() * copies);
    }
}

/// An unchanged drawing redrawn with the Select tool and nothing selected,
/// in view and with the whole drawing off-screen.
#[tokio::test]
#[ignore = "Release-mode idle redraw profiling; requires a headless renderer"]
async fn manual_idle_redraw_workloads() {
    let renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        std::env::var("RESHIKI_PERF_RENDERER").ok().as_deref(),
    )
    .await
    .expect("Headless renderer");
    println!("Renderer: {}", renderer.name());
    let gallery: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    let bounds = Rectangle::with_size(iced::Size::new(1066., 570.));
    let point = Point::new(350., 250.);
    let cursor = mouse::Cursor::Available(point);
    println!("PERF,workload,iterations,median_ms,p95_ms,mean_ms");
    for copies in [1, 4, 16] {
        let mut doc = Document::default();
        for copy in 0..copies {
            let offset = World::new((copy % 4) as f32 * 1800., (copy / 4) as f32 * 1800.);
            reshiki::editing::append(&mut doc, &gallery, offset);
        }
        let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        let state = State {
            cursor: Some(point),
            ..Default::default()
        };
        for (place, center) in [
            ("onscreen", World::new(378., 294.)),
            ("offscreen", World::new(-50_000., -50_000.)),
        ] {
            canvas.camera = Camera { center, zoom: 0.5 };
            measure(&format!("gallery_{copies}x_{place}_idle_draw"), 30, |_| {
                black_box(canvas.draw(&state, &renderer, &Theme::Light, bounds, cursor));
            });
        }
    }
}

/// Previews over the gallery at 1×, 4× and 16×: hovering a ring or template,
/// drawing a bond, moving part of a molecule, and panning. "Onscreen" keeps
/// the gallery in view; "offscreen" moves the camera away from all of it.
#[tokio::test]
#[ignore = "Release-mode canvas preview profiling; requires a headless renderer"]
async fn manual_preview_workloads() {
    let backend = std::env::var("RESHIKI_PERF_RENDERER").ok();
    let renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        backend.as_deref(),
    )
    .await
    .expect("Headless renderer");
    println!("Renderer: {}", renderer.name());
    let original: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    let iterations = std::env::var("RESHIKI_PERF_ITERATIONS")
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let template = reshiki::templates::LIBRARY
        .iter()
        .find(|t| t.name == "Cyclohexane")
        .unwrap();
    let bounds = Rectangle::with_size(iced::Size::new(1066., 570.));
    let length = original.drawing_style.bond_length_world;
    println!("PERF,workload,iterations,median_ms,p95_ms,mean_ms");
    for copies in [1, 4, 16] {
        let mut doc = original.clone();
        let (lo, hi) = original.bounds();
        for copy in 1..copies {
            reshiki::editing::append(
                &mut doc,
                &original,
                World::new((hi.x - lo.x + 100.) * copy as f32, 0.),
            );
        }
        // The lowest end carbon of the first copy: bonds start there, and it
        // moves with its neighbor.
        let degree = |id| doc.bonds.iter().filter(|b| b.a == id || b.b == id).count();
        let end = doc
            .atoms
            .iter()
            .filter(|a| a.element == "C" && degree(a.id) == 1)
            .max_by(|a, b| a.position.y.total_cmp(&b.position.y))
            .unwrap();
        let (end, end_position) = (end.id, end.position);
        let neighbor = doc
            .bonds
            .iter()
            .find_map(|b| match (b.a == end, b.b == end) {
                (true, _) => Some(b.b),
                (_, true) => Some(b.a),
                _ => None,
            })
            .unwrap();
        // Empty paper below the gallery, with the gallery in the top half.
        let free = World::new(end_position.x, hi.y + 4. * length);
        let onscreen = Camera {
            center: World::new(end_position.x, hi.y),
            zoom: 0.5,
        };
        let offscreen = Camera {
            center: World::new(-100_000., -100_000.),
            zoom: 0.5,
        };
        for (view, camera) in [("onscreen", onscreen), ("offscreen", offscreen)] {
            let name = |workload: &str| format!("g{copies}x_{view}_{workload}");
            let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
            canvas.camera = camera;
            // Offscreen, the preview stays in view over empty paper.
            let hover = if view == "onscreen" {
                camera.screen(free, bounds)
            } else {
                Point::new(500., 300.)
            };
            let start = camera.screen(end_position, bounds);
            let mut state = State {
                cursor: Some(hover),
                ..Default::default()
            };
            let draw = |canvas: &MoleculeCanvas<'_>, state: &State| {
                let cursor = mouse::Cursor::Available(state.cursor.unwrap());
                black_box(canvas.draw(state, &renderer, &Theme::Light, bounds, cursor));
            };
            measure(&name("idle"), iterations, |_| draw(&canvas, &state));
            canvas.tool = Tool::RingPreset(reshiki::rings::Preset::Benzene);
            measure(&name("ring_preset_hover"), iterations, |i| {
                state.cursor = Some(hover + Vector::new((i % 8) as f32, 0.));
                draw(&canvas, &state);
            });
            state.cursor = Some(hover);
            measure(&name("ring_preset_hover_still"), iterations, |_| {
                draw(&canvas, &state)
            });
            canvas.tool = Tool::Template;
            canvas.template = Some((template, reshiki::templates::Anchor::Auto));
            measure(&name("template_hover"), iterations, |i| {
                state.cursor = Some(hover + Vector::new((i % 8) as f32, 0.));
                draw(&canvas, &state);
            });
            canvas.template = None;
            canvas.tool = Tool::Bond(1);
            state.gesture = Some(Gesture::Draw {
                start: end_position,
                pressed: end_position,
                id: Some(end),
                text: None,
            });
            measure(&name("bond_drag"), iterations, |i| {
                state.cursor = Some(start + Vector::new(30. + (i % 8) as f32, -10.));
                draw(&canvas, &state);
            });
            canvas.tool = Tool::Select;
            let moving = [end, neighbor];
            canvas.selected = &moving;
            state.gesture = Some(Gesture::Move {
                start: end_position,
                ids: moving.to_vec(),
                clicked: moving.to_vec(),
            });
            measure(&name("partial_move"), iterations, |i| {
                state.cursor = Some(start + Vector::new(10. + (i % 8) as f32, 5.));
                draw(&canvas, &state);
            });
        }
        let (lo, hi) = doc.bounds();
        let fit = (bounds.width / (hi.x - lo.x)).min(bounds.height / (hi.y - lo.y)) * 0.9;
        let center = World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
        for (view, zoom) in [("fit", fit), ("half", 0.5)] {
            let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
            canvas.tool = Tool::Select;
            let state = State::default();
            measure(&format!("g{copies}x_{view}_pan"), iterations, |i| {
                canvas.camera = Camera {
                    center: center.offset((i % 16) as f32 / zoom, 0.),
                    zoom,
                };
                black_box(canvas.draw(
                    &state,
                    &renderer,
                    &Theme::Light,
                    bounds,
                    mouse::Cursor::Unavailable,
                ));
            });
        }
        assert_eq!(doc.atoms.len(), original.atoms.len() * copies);
    }
}

/// Read back rendered pixels: cache reuse must not change a drawing, and a
/// rigid drag preview must agree with committing that translation.
#[tokio::test]
#[ignore = "Pixel regression check; requires a headless renderer"]
async fn cached_canvas_matches_fresh_edits_and_committed_drag() {
    use iced::advanced::Renderer as _;
    use iced::advanced::graphics::geometry::Renderer as _;
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless renderer");
    let bounds = Rectangle::with_size(iced::Size::new(800., 500.));
    let mut doc: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    let ids = doc.all_ids();
    let render = |renderer: &mut Renderer, doc: &Document, state: &State, camera: Camera| {
        let mut canvas = tests::chain_canvas(doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        canvas.selected = &ids;
        canvas.camera = camera;
        renderer.reset(bounds);
        for layer in canvas.draw(
            state,
            renderer,
            &Theme::Light,
            bounds,
            mouse::Cursor::Unavailable,
        ) {
            renderer.draw_geometry(layer);
        }
        Headless::screenshot(renderer, iced::Size::new(800, 500), 1., Color::WHITE)
    };
    let camera = Camera {
        center: World::new(378., 294.),
        zoom: 0.5,
    };
    let state = State::default();
    let original = render(&mut renderer, &doc, &state, camera);
    assert_eq!(original, render(&mut renderer, &doc, &state, camera));
    let before = doc.clone();
    doc.annotations[0].text = "Edited NH2 caption".into();
    doc.annotations[0].format.style.bold = true;
    doc.annotations[0].format.style.color = reshiki::palette::Color::Custom([180, 60, 40]);
    doc.translate(&ids[..2], 30., 10.);
    let mut history = reshiki::document::History::default();
    history.commit(before, &doc);
    for view in [
        camera,
        Camera {
            center: World::new(300., 250.),
            zoom: 0.7,
        },
    ] {
        assert_eq!(
            render(&mut renderer, &doc, &state, view),
            render(&mut renderer, &doc, &State::default(), view)
        );
    }
    assert!(history.undo(&mut doc));
    assert_eq!(original, render(&mut renderer, &doc, &state, camera));
    assert!(history.redo(&mut doc));
    assert_eq!(
        render(&mut renderer, &doc, &state, camera),
        render(&mut renderer, &doc, &State::default(), camera)
    );

    let point = Point::new(300., 200.);
    let delta = World::new(40., 30.);
    let drag = State {
        gesture: Some(Gesture::Move {
            start: camera.world(point, bounds),
            ids: ids.clone(),
            clicked: vec![ids[0]],
        }),
        cursor: Some(point + Vector::new(delta.x * camera.zoom, delta.y * camera.zoom)),
        ..Default::default()
    };
    let preview = render(&mut renderer, &doc, &drag, camera);
    doc.translate(&ids, delta.x, delta.y);
    let committed = render(&mut renderer, &doc, &state, camera);
    // Algebraically equivalent camera/point translations can differ by a few
    // floating-point ulps at antialiased edges; bound both area and mean error.
    let changed = preview
        .as_chunks::<4>()
        .0
        .iter()
        .zip(committed.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 8))
        .count();
    let error: u64 = preview
        .iter()
        .zip(&committed)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    assert!(
        changed < 800 * 500 / 1000,
        "{changed} significantly different pixels"
    );
    assert!(
        (error as f64 / preview.len() as f64) < 0.1,
        "Mean channel error {error}"
    );
}

/// Compare optimized selection decorations with the pre-cache drawing path,
/// including clipping at viewport edges and the page-fit marker policy.
#[tokio::test]
#[ignore = "Pixel regression check; requires a headless renderer"]
async fn selection_markers_match_unculled_reference() {
    use iced::advanced::Renderer as _;
    use iced::advanced::graphics::geometry::Renderer as _;
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless renderer");
    let doc: Document =
        serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk")).unwrap();
    let bounds = Rectangle::with_size(iced::Size::new(800., 500.));
    let all = doc.all_ids();
    for ids in [&all[..], &all[..2]] {
        let markers = markers::Markers::new(&doc, ids);
        for zoom in [0.08, 0.12, 0.5, 1.25, 2.5] {
            for center in [World::new(378., 294.), World::new(750., 580.)] {
                let camera = Camera { center, zoom };
                let mut render = |reference| {
                    renderer.reset(bounds);
                    let mut frame = layered::Frame::new(&renderer, bounds.size());
                    if reference {
                        draw_reference_atom_markers(&mut frame, &doc, ids, camera, bounds, true);
                        for annotation in doc.annotations.iter().filter(|a| ids.contains(&a.id)) {
                            let (width, height) = annotation.size();
                            frame.stroke(
                                &Path::rectangle(
                                    camera.screen(annotation.position, bounds),
                                    iced::Size::new(width * zoom, height * zoom),
                                ),
                                Stroke::default().with_color(Color::from_rgb8(20, 130, 112)),
                            );
                        }
                    } else {
                        markers.draw(&mut frame, camera, bounds, true);
                    }
                    for geometry in frame.finish() {
                        renderer.draw_geometry(geometry);
                    }
                    Headless::screenshot(&mut renderer, iced::Size::new(800, 500), 1., Color::WHITE)
                };
                assert_eq!(
                    render(false),
                    render(true),
                    "zoom {zoom}, center {center:?}"
                );
            }
        }
    }
}

fn draw_reference_atom_markers(
    frame: &mut layered::Frame<'_>,
    doc: &Document,
    ids: &[u64],
    camera: Camera,
    bounds: Rectangle,
    selected: bool,
) {
    // At page-fit zoom the selection box is more useful than overlapping atom rings.
    if selected && ids.len() > 1 && camera.zoom < 0.12 {
        return;
    }
    let marker_scale = camera.zoom.clamp(0.15, 1.);
    for bond in doc.bonds.iter().filter(|b| doc.bond_visible(b.a, b.b)) {
        if ids.contains(&bond.a)
            && ids.contains(&bond.b)
            && let Some((a, b)) = doc.atom(bond.a).zip(doc.atom(bond.b))
        {
            frame.stroke(
                &Path::line(
                    camera.screen(a.position, bounds),
                    camera.screen(b.position, bounds),
                ),
                Stroke::default()
                    .with_width(7.0 * marker_scale)
                    .with_color(Color::from_rgba8(19, 135, 116, 0.16)),
            );
        }
    }
    for id in ids {
        if let Some(atom) = doc.atom(*id).filter(|a| doc.atom_visible(a.id)) {
            let circle = Path::circle(camera.screen(atom.position, bounds), 8.0 * marker_scale);
            if selected {
                frame.fill(&circle, Color::from_rgba8(19, 135, 116, 0.12));
            }
            frame.stroke(
                &circle,
                Stroke::default()
                    .with_width((if selected { 1.8 } else { 1.4 }) * marker_scale.sqrt())
                    .with_color(rgb([19, 135, 116])),
            );
        }
    }
}
