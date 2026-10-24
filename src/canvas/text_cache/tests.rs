use super::*;

#[test]
fn text_cache_keys_cover_shaping_color_and_zoom() {
    let mut cache = TextCache::default();
    let style = TextStyle::default();
    let first = cache.get("NH2 α", 14., 1., [0, 0, 0], &style);
    assert!(Rc::ptr_eq(
        &first,
        &cache.get("NH2 α", 14., 1., [0, 0, 0], &style)
    ));
    for (text, size, zoom, color, changed) in [
        ("NH3 α", 14., 1., [0, 0, 0], style.clone()),
        ("NH2 α", 16., 1., [0, 0, 0], style.clone()),
        ("NH2 α", 14., 2., [0, 0, 0], style.clone()),
        ("NH2 α", 14., 1., [128, 0, 0], style.clone()),
        (
            "NH2 α",
            14.,
            1.,
            [0, 0, 0],
            TextStyle {
                bold: true,
                ..style.clone()
            },
        ),
        (
            "NH2 α",
            14.,
            1.,
            [0, 0, 0],
            TextStyle {
                italic: true,
                ..style.clone()
            },
        ),
        (
            "NH2 α",
            14.,
            1.,
            [0, 0, 0],
            TextStyle {
                family: "Times New Roman".into(),
                ..style.clone()
            },
        ),
    ] {
        let cached = cache.get(text, size, zoom, color, &changed);
        assert!(!Rc::ptr_eq(&first, &cached));
        let fresh = outline(text, size, zoom, color, &changed);
        assert_eq!(cached.bounds, fresh.bounds);
        assert_eq!(cached.paths.len(), fresh.paths.len());
        for ((a, ac), (b, bc)) in cached.paths.iter().zip(&fresh.paths) {
            assert_eq!(ac, bc);
            assert_eq!(
                a.raw().iter().collect::<Vec<_>>(),
                b.raw().iter().collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn shared_glyphs_exactly_match_fresh_outlines_at_each_zoom_and_font() {
    let mut cache = TextCache::default();
    for family in ["Arial", "Times New Roman", "Courier New"] {
        for zoom in [0.08, 0.45, 0.455, 0.5, 0.7, 1., 1.25, 2.5, 8.] {
            for (bold, italic) in [(false, false), (true, false), (false, true)] {
                let style = TextStyle {
                    family: family.into(),
                    bold,
                    italic,
                    ..Default::default()
                };
                for text in [
                    "NH2 OH α β →",
                    "AV office fi",
                    "日本語 العربية 🧪",
                    "First\nSecond",
                ] {
                    let cached = cache.get(text, 14., zoom, [25, 90, 140], &style);
                    let fresh = outline(text, 14., zoom, [25, 90, 140], &style);
                    assert_eq!(cached.bounds, fresh.bounds, "{family} {zoom} {text}");
                    assert_eq!(cached.paths.len(), fresh.paths.len());
                    for ((a, ac), (b, bc)) in cached.paths.iter().zip(&fresh.paths) {
                        assert_eq!(ac, bc);
                        assert_eq!(
                            a.raw().iter().collect::<Vec<_>>(),
                            b.raw().iter().collect::<Vec<_>>(),
                            "{family} {zoom} {text}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn changing_text_does_not_grow_the_cache_without_limit() {
    let mut cache = TextCache::default();
    for i in 0..2100 {
        cache.get(&i.to_string(), 14., 1., [0, 0, 0], &TextStyle::default());
        assert!(cache.entries.len() <= 2048);
        assert!(cache.cost <= 8 * 1024 * 1024);
        assert!(cache.glyphs.within_budget());
    }
}

#[test]
fn zoom_steps_are_quarter_octaves_within_an_eighth_octave_of_the_zoom() {
    for zoom in [0.25, 0.5, 1., 2., 8.] {
        assert_eq!(zoom_step(zoom), zoom);
    }
    let mut zoom = 0.05;
    while zoom < 20. {
        let step = zoom_step(zoom);
        assert!((zoom / step).log2().abs() <= 0.125 + 1e-5, "{zoom} {step}");
        assert_eq!(zoom_step(step), step);
        zoom *= 1.013;
    }
}

#[test]
fn scaled_step_outlines_land_where_exact_outlines_do() {
    let style = TextStyle::default();
    for zoom in [0.3, 0.455, 0.7, 1.25, 1.9, 3.1] {
        let step = zoom_step(zoom);
        let scale = zoom / step;
        let scaled = outline("NH2 OH", 14., step, [0, 0, 0], &style)
            .bounds
            .unwrap();
        let exact = outline("NH2 OH", 14., zoom, [0, 0, 0], &style)
            .bounds
            .unwrap();
        // Vertical hinting snaps ink to whole pixels at each size: about a pixel apart.
        for (a, b) in [
            (scaled.x * scale, exact.x),
            (scaled.y * scale, exact.y),
            (scaled.width * scale, exact.width),
            (scaled.height * scale, exact.height),
        ] {
            assert!((a - b).abs() <= 1. + 0.06 * b.abs(), "{zoom}: {a} vs {b}");
        }
    }
}

#[test]
fn a_full_cache_evicts_the_least_recently_used_outlines() {
    let mut cache = TextCache::default();
    let style = TextStyle::default();
    let kept = cache.get("kept", 14., 1., [0, 0, 0], &style);
    let stale = cache.get("stale", 14., 1., [0, 0, 0], &style);
    for i in 0..LIMIT {
        cache.get(&i.to_string(), 14., 1., [0, 0, 0], &style);
        // Text drawn every frame stays cached while new text fills the cache.
        assert!(Rc::ptr_eq(
            &kept,
            &cache.get("kept", 14., 1., [0, 0, 0], &style)
        ));
    }
    assert!(cache.entries.len() > LIMIT / 2 && cache.entries.len() < LIMIT);
    assert!(!Rc::ptr_eq(
        &stale,
        &cache.get("stale", 14., 1., [0, 0, 0], &style)
    ));
}
