use super::*;
use crate::canvas::cache::SceneCache;
use reshiki::document::Annotation;

#[test]
fn retained_caption_hits_preserve_rotation_wrapping_overlap_and_boundaries() {
    let mut doc = Document::default();
    for (id, text, position, angle) in [
        (10, "First\ncaption", World::new(-30., -20.), 0.),
        (20, "H₂O and 日本語", World::new(100., 30.), 37.),
        (30, "Upper caption", World::new(-30., -20.), 315.),
    ] {
        let mut annotation = Annotation {
            id,
            text: text.into(),
            position,
            angle,
            format: Default::default(),
        };
        annotation.format.style.bold = id == 20;
        annotation.format.style.italic = id == 30;
        annotation.format.width_pt = (id == 20).then_some(35.);
        doc.annotations.push(annotation);
    }
    let mut cache = SceneCache::default();
    let sizes = cache.caption_sizes(&doc).unwrap();
    assert!(sizes.iter().all(|size| size.get().is_none()));
    for annotation in &doc.annotations {
        let (width, height) = annotation.size();
        let center = annotation.position.offset(width / 2., height / 2.);
        for x in [-0.001, 0., width / 2., width - 0.001, width, width + 0.001] {
            for y in [
                -0.001,
                0.,
                height / 2.,
                height - 0.001,
                height,
                height + 0.001,
            ] {
                let point = annotation
                    .position
                    .offset(x, y)
                    .turned(center, annotation.angle);
                assert_eq!(
                    hit_selection_cached(&doc, point, 12., &mut cache),
                    hit_selection(&doc, point, 12.),
                    "caption {}, local ({x}, {y})",
                    annotation.id,
                );
            }
        }
    }
    let point = World::new(10_000., 10_000.);
    assert_eq!(
        hit_selection_cached(&doc, point, 12., &mut cache),
        Vec::<u64>::new()
    );
    assert!(sizes.iter().all(|size| size.get().is_some()));
    // Malformed imported geometry retains the uncached query's result.
    for angle in [f32::NAN, f32::INFINITY] {
        doc.annotations[0].angle = angle;
        assert_eq!(
            hit_selection_cached(&doc, point, 12., &mut cache),
            hit_selection(&doc, point, 12.)
        );
    }
}

#[test]
fn retained_caption_targets_keep_caption_priority_and_refusals() {
    let mut doc = Document::default();
    let source = doc.add_atom("C", World::default());
    for (id, text, position) in [
        (10, "Pd2+", World::new(200., 0.)),
        (
            20,
            "A caption that cannot become an atom",
            World::new(200., 80.),
        ),
    ] {
        doc.annotations.push(Annotation {
            id,
            text: text.into(),
            position,
            angle: 0.,
            format: Default::default(),
        });
    }
    let mut cache = SceneCache::default();
    for annotation in &doc.annotations {
        let point = reshiki::atom_text::text_atom_position(&doc, annotation.id)
            .or_else(|| reshiki::atom_text::text_first_glyph_center(annotation))
            .unwrap();
        for tool in [
            Tool::Bond(1),
            Tool::Atom,
            Tool::StyledBond(reshiki::bonds::BondPreset::Dotted),
        ] {
            let expected = bond_target_with_text(
                &doc,
                World::default(),
                point,
                (Some(source), None),
                12.,
                BondDrawing::default(),
                tool,
            );
            for _ in 0..2 {
                let actual = cache
                    .bond_target(
                        &doc,
                        (World::default(), point),
                        (Some(source), None),
                        12.,
                        BondDrawing::default(),
                        tool,
                    )
                    .map(|target| (target.end, target.atom, target.text));
                assert_eq!(actual, expected);
            }
        }
    }
}

#[test]
#[ignore = "Release-mode isolated caption hit profiling"]
fn manual_caption_hit_workloads() {
    use std::hint::black_box;
    use std::time::Instant;
    fn measure(name: &str, mut query: impl FnMut(usize)) {
        let count = std::env::var("RESHIKI_PERF_ITERATIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(30)
            .max(1);
        for i in 0..3 {
            query(i);
        }
        let mut samples: Vec<_> = (0..count)
            .map(|i| {
                let start = Instant::now();
                query(i);
                start.elapsed().as_secs_f64() * 1000.
            })
            .collect();
        samples.sort_by(f64::total_cmp);
        println!(
            "PERF,{name},{count},{:.4},{:.4},{:.4}",
            samples[count / 2],
            samples[((count as f64 * 0.95).ceil() as usize - 1).min(count - 1)],
            samples.iter().sum::<f64>() / count as f64
        );
    }
    let original: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let (lo, hi) = original.bounds();
    let mut doc = original.clone();
    for copy in 1..16 {
        reshiki::editing::append(
            &mut doc,
            &original,
            World::new((hi.x - lo.x + 100.) * copy as f32, 0.),
        );
    }
    let point = World::new(hi.x, hi.y + 4. * doc.drawing_style.bond_length_world);
    let clips = CellClips::new(&doc);
    let mut cache = SceneCache::default();
    let captions = cache.caption_sizes(&doc).unwrap();
    println!("PERF,workload,iterations,median_ms,p95_ms,mean_ms");
    for retained in [false, true] {
        let label = if retained { "retained" } else { "uncached" };
        let sizes = retained.then_some(captions.as_ref());
        measure(&format!("gallery_16x_caption_hover_{label}"), |i| {
            black_box(hit_object_in_cells(
                &doc,
                point.offset((i % 8) as f32, 0.),
                24.,
                false,
                &clips,
                sizes,
            ));
        });
        measure(&format!("gallery_16x_caption_bond_target_{label}"), |i| {
            black_box(bond_target_with_plane(
                (&doc, sizes),
                (World::default(), point.offset((i % 8) as f32, 0.)),
                (None, None),
                24.,
                BondDrawing::default(),
                Tool::Bond(1),
                None,
            ))
            .unwrap();
        });
    }
}
