use super::*;
use reshiki::{
    cycles::{self, Shape, Step},
    document::{Annotation, History, Point},
};

fn named_cycle(count: usize) -> Document {
    let mut doc = Document::default();
    let radius = if count > 4 { 2000. } else { 200. };
    cycles::insert(&mut doc, Point::default(), radius);
    let mut parts = Vec::new();
    for i in 0..count {
        let angle = std::f32::consts::TAU * i as f32 / count as f32;
        let id = doc.next_id();
        let mut text = Annotation {
            id,
            angle: 0.,
            position: Point::default(),
            text: "Pd".into(),
            format: Default::default(),
        };
        let (w, h) = text.size();
        text.position = Point::new(
            radius * angle.cos() - w * 0.5,
            radius * angle.sin() - h * 0.5,
        );
        doc.annotations.push(text);
        parts.push(vec![id]);
    }
    let cycle = doc.graphics[0].cycle.as_mut().unwrap();
    cycle.intermediates = parts;
    cycle.steps = (0..count)
        .map(|i| Step {
            name: format!("Step {}", i + 1),
            ..Default::default()
        })
        .collect();
    doc.validate().unwrap();
    doc
}

fn label_box(doc: &Document) -> (Point, Point) {
    let drawn = cycles::drawn(doc, &doc.graphics[0]);
    let (at, text, format) = &drawn.labels[0];
    let layout = reshiki::typography::layout(text, format);
    (*at, at.offset(layout.width, layout.height))
}

fn agrees(cache: &mut SceneCache, doc: &Document) {
    let g = &doc.graphics[0];
    let drawn = cycles::drawn(doc, g);
    let mut probes = vec![Point::new(10000., 10000.)];
    probes.extend(drawn.arrows.iter().map(|arrow| arrow.point(0.5)));
    for (at, text, format) in drawn.labels {
        let size = reshiki::typography::layout(&text, &format);
        probes.extend([
            at.offset(0.5 * size.width, 0.5 * size.height),
            at.offset(size.width + 0.5, 0.5 * size.height),
            at.offset(size.width + 2., 0.5 * size.height),
        ]);
    }
    for point in probes {
        assert_eq!(
            cache.cycle_hit(doc, g, point, 1.),
            cycles::hit(doc, g, point, 1.),
            "{point:?}"
        );
    }
}

#[test]
fn distant_cycle_hover_reuses_layout_and_keeps_exact_label_hits() {
    let mut doc = Document::default();
    let id = cycles::insert(&mut doc, Point::default(), 50.);
    let cache = RefCell::new(SceneCache::default());
    let far = Point::new(10000., 10000.);
    // The unnamed fast reject must not allocate even one hit layout.
    assert!(
        !cache
            .borrow_mut()
            .cycle_hit(&doc, &doc.graphics[0], far, 1.)
    );
    assert_eq!(cache.borrow().cycle_hit_layouts, 0);
    doc.graphics[0].cycle.as_mut().unwrap().label = "M".repeat(200);
    let (lo, hi) = label_box(&doc);
    let label = Point::new(lo.x + (hi.x - lo.x) * 0.9, (lo.y + hi.y) * 0.5);
    assert!(label.distance(Point::default()) > 50. + 8. * doc.drawing_style.bond_length_world);
    for _ in 0..40 {
        let _frame = Frame::begin(&cache, &doc);
        for (point, expected) in [
            (far, vec![]),
            (label, vec![id]),
            (Point::new(hi.x + 0.5, label.y), vec![id]),
            (Point::new(hi.x + 2., label.y), vec![]),
        ] {
            assert_eq!(
                super::super::hit::hit_selection_cached(&doc, point, 1., &mut cache.borrow_mut()),
                expected
            );
        }
    }
    assert_eq!(cache.borrow().cycle_hit_layouts, 1);
    // A layout warmed near an unnamed cycle must retain its outer rejection;
    // whitespace is unnamed just as it is in the uncached preflight.
    doc.graphics[0].cycle.as_mut().unwrap().label = " ".repeat(200);
    cache
        .borrow_mut()
        .cycle_hit(&doc, &doc.graphics[0], Point::default(), 1.);
    assert!(
        !cache
            .borrow_mut()
            .cycle_hit(&doc, &doc.graphics[0], label, 1.)
    );
    assert!(!cycles::hit(&doc, &doc.graphics[0], label, 1.));
    assert_eq!(cache.borrow().cycle_hit_layouts, 2);
}

#[test]
fn cycle_hit_cache_invalidates_for_edits_history_load_and_shapes() {
    let mut doc = named_cycle(3);
    let mut cache = SceneCache::default();
    agrees(&mut cache, &doc);
    let before = doc.clone();
    doc.annotations[0].position.x += 90.;
    doc.annotations[0].text = "Ar–Pd(II)–X".into();
    agrees(&mut cache, &doc);
    assert_eq!(cache.cycle_hit_layouts, 2);
    let mut history = History::default();
    assert!(history.commit(before, &doc));
    assert!(history.undo(&mut doc));
    agrees(&mut cache, &doc);
    assert!(history.redo(&mut doc));
    agrees(&mut cache, &doc);
    assert_eq!(cache.cycle_hit_layouts, 4);
    doc.drawing_style.font_size_pt += 5.;
    agrees(&mut cache, &doc);
    assert_eq!(cache.cycle_hit_layouts, 5);
    for shape in [
        Shape::Circle,
        Shape::Ellipse,
        Shape::RoundedRectangle,
        Shape::Rectangle,
    ] {
        for reflected in [false, true] {
            let g = &mut doc.graphics[0];
            g.origin = Point::new(-240., if reflected { 120. } else { -120. });
            g.axis_x = Point::new(480., 0.);
            g.axis_y = Point::new(0., if reflected { -240. } else { 240. });
            let cycle = g.cycle.as_mut().unwrap();
            cycle.shape = shape;
            cycle.counterclockwise = reflected;
            cycle.steps[0].name = "Oxidative addition".into();
            cycle.steps[0].reversible = true;
            cycle.steps[0].bow_pt = 18.;
            agrees(&mut cache, &doc);
        }
    }
    let mut loaded = Document::from_json(&doc.file_json().unwrap()).unwrap();
    loaded.graphics[0].cycle.as_mut().unwrap().steps[0].name = "Changed after load".into();
    let builds = cache.cycle_hit_layouts;
    agrees(&mut cache, &loaded);
    assert_eq!(cache.cycle_hit_layouts, builds + 1);
    cache.foreground_orbital(&Document::default());
    assert!(cache.cycle_hits.is_empty());
}

#[test]
fn detached_preview_hits_cannot_replace_a_base_frame_cycle_layout() {
    let mut doc = Document::default();
    let id = cycles::insert(&mut doc, Point::default(), 50.);
    doc.graphics[0].cycle.as_mut().unwrap().label = "M".repeat(200);
    let (lo, hi) = label_box(&doc);
    let base_point = Point::new(lo.x + (hi.x - lo.x) * 0.9, (lo.y + hi.y) * 0.5);
    let cache = RefCell::new(SceneCache::default());
    let mut preview = doc.clone();
    preview.translate(&[id], 10000., 5000.);
    let preview_point = base_point.offset(10000., 5000.);
    {
        let _frame = Frame::begin(&cache, &doc);
        assert!(
            cache
                .borrow_mut()
                .cycle_hit(&doc, &doc.graphics[0], base_point, 1.)
        );
        assert!(cycles::hit(
            &preview,
            &preview.graphics[0],
            preview_point,
            1.
        ));
        assert!(!cycles::hit(&preview, &preview.graphics[0], base_point, 1.));
        assert!(
            cache
                .borrow_mut()
                .cycle_hit(&doc, &doc.graphics[0], base_point, 1.)
        );
        assert_eq!(cache.borrow().cycle_hit_layouts, 1);
    }
    // Outside a base frame, a preview supplied to the cache is another complete
    // snapshot; returning to the base must invalidate it again.
    assert!(
        cache
            .borrow_mut()
            .cycle_hit(&preview, &preview.graphics[0], preview_point, 1.)
    );
    assert!(
        cache
            .borrow_mut()
            .cycle_hit(&doc, &doc.graphics[0], base_point, 1.)
    );
    assert_eq!(cache.borrow().cycle_hit_layouts, 3);
    assert_eq!(cache.borrow().document.as_ref(), Some(&doc));
}

#[test]
fn cached_cycle_names_do_not_expand_distant_arrow_hit_scope() {
    let mut doc = named_cycle(3);
    let reagent = doc.next_id();
    doc.annotations.push(Annotation {
        id: reagent,
        position: Point::new(5000., 5000.),
        text: "Ar–X".into(),
        angle: 0.,
        format: Default::default(),
    });
    doc.graphics[0].cycle.as_mut().unwrap().steps[0]
        .reagents
        .push(vec![reagent]);
    let g = &doc.graphics[0];
    let drawn = cycles::drawn(&doc, g);
    let side = drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .find(|(_, step)| step.is_none())
        .unwrap()
        .0;
    let point = side.point(0.5);
    assert!(side.hit(point, 1.));
    assert!(!cycles::hit(&doc, g, point, 1.));
    assert!(!SceneCache::default().cycle_hit(&doc, g, point, 1.));
}

#[test]
#[ignore = "local retained named-cycle hover timing"]
fn benchmark_retained_named_cycle_hover() {
    use std::{hint::black_box, time::Instant};
    for count in [4, 64] {
        let doc = named_cycle(count);
        let cache = RefCell::new(SceneCache::default());
        let point = Point::new(10000., 10000.);
        let start = Instant::now();
        for _ in 0..3 {
            black_box(cycles::hit(&doc, &doc.graphics[0], point, 1.));
        }
        let uncached_ms = start.elapsed().as_secs_f64() * 1000. / 3.;
        cache
            .borrow_mut()
            .cycle_hit(&doc, &doc.graphics[0], point, 1.);
        let start = Instant::now();
        for _ in 0..1000 {
            let _frame = Frame::begin(&cache, &doc);
            black_box(super::super::hit::hit_selection_cached(
                &doc,
                point,
                1.,
                &mut cache.borrow_mut(),
            ));
        }
        let retained_ms = start.elapsed().as_secs_f64() * 1000. / 1000.;
        assert_eq!(cache.borrow().cycle_hit_layouts, 1);
        println!(
            "{count} named steps: uncached cycle {uncached_ms:.4} ms, retained full hover {retained_ms:.4} ms"
        );
    }
}
