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
}

#[tokio::test]
#[ignore = "Release-mode canvas workload profiling; requires a headless renderer"]
async fn loaded_canvas_workloads() {
    let renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless renderer");
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
    doc.annotations[0].format.style.color = [180, 60, 40];
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
        .chunks_exact(4)
        .zip(committed.chunks_exact(4))
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
