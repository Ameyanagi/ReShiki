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
