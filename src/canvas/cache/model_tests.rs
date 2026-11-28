use super::*;
use reshiki::{document::Point, models3d::Display};

fn drawing(copies: usize) -> Document {
    let mut doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/models3d/controls/caffeine.rsk"
    ))
    .unwrap();
    let gallery = Document::from_json(include_bytes!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    for copy in 0..copies {
        reshiki::editing::append(
            &mut doc,
            &gallery,
            Point::new(1000. + (copy % 4) as f32 * 1800., (copy / 4) as f32 * 1800.),
        );
    }
    doc
}

#[test]
fn coordinate_only_models_keep_static_scene_and_rebuild_selection_geometry() {
    for display in [Display::ThreeD, Display::Both] {
        let mut doc = drawing(0);
        doc.models[0].display = display;
        let selected = doc.models[0].linked_atoms.clone();
        let mut cache = SceneCache::default();
        let initial = cache.render(&doc, &selected);
        let opacity = cache.opacity(&doc);
        assert!(cache.model_scene.is_some());
        for frame in 1..=3 {
            doc.models[0].atoms[0].xyz[2] += frame as f32 * 0.05;
            let (markers, scene) = cache.render(&doc, &selected);
            assert_eq!(cache.model_scene_updates, frame);
            assert_eq!(
                format!("{scene:?}"),
                format!("{:?}", scene::primitives(&doc))
            );
            assert_eq!(
                *markers,
                super::super::markers::Markers::new(&doc, &selected)
            );
            assert!(!Rc::ptr_eq(&initial.0, &markers));
            assert!(Rc::ptr_eq(&opacity, &cache.opacity(&doc)));
        }
        doc.annotations[0].text.push('!');
        let (_, scene) = cache.render(&doc, &selected);
        assert_eq!(cache.model_scene_updates, 3);
        assert_eq!(
            format!("{scene:?}"),
            format!("{:?}", scene::primitives(&doc))
        );
        assert!(!Rc::ptr_eq(&opacity, &cache.opacity(&doc)));
        let updates = cache.model_scene_updates;
        cache.meshes = cache.meshes.wrapping_sub(1);
        cache.render(&doc, &selected);
        assert_eq!(
            cache.model_scene_updates, updates,
            "new orbital meshes rebuild the complete scene"
        );
    }
}

#[test]
#[ignore = "Release-mode changed-model scene cache benchmark; no renderer needed"]
fn manual_model_coordinate_scene_workloads() {
    use std::{hint::black_box, time::Instant};
    let iterations = std::env::var("RESHIKI_PERF_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    println!("PERF,workload,iterations,median_ms,p95_ms,mean_ms");
    for copies in [0, 1, 4, 16] {
        let base = drawing(copies);
        let frames: Vec<_> = (0..iterations + 3)
            .map(|frame| {
                let mut doc = base.clone();
                doc.models[0].atoms[0].xyz[2] += (frame + 1) as f32 * 0.002;
                doc
            })
            .collect();
        for retained in [false, true] {
            let mut cache = SceneCache::default();
            cache.render(&base, &[]);
            let mut samples = Vec::with_capacity(iterations);
            for (index, frame) in frames.iter().enumerate() {
                if !retained {
                    cache = SceneCache::default();
                }
                let started = Instant::now();
                black_box(cache.render(frame, &[]));
                let elapsed = started.elapsed().as_secs_f64() * 1000.;
                if index >= 3 {
                    samples.push(elapsed);
                }
            }
            if retained {
                assert_eq!(cache.model_scene_updates, frames.len());
            }
            samples.sort_by(f64::total_cmp);
            let mode = if retained { "retained" } else { "fresh" };
            println!(
                "PERF,caffeine_gallery_{copies}x_model_scene_{mode},{iterations},{:.4},{:.4},{:.4}",
                samples[iterations / 2],
                samples[(iterations * 95 / 100).min(iterations - 1)],
                samples.iter().sum::<f64>() / iterations as f64,
            );
        }
    }
}
