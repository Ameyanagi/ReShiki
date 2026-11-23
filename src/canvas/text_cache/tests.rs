use super::*;

#[test]
fn cold_offscreen_ascii_does_not_create_outlines_and_warm_unicode_uses_exact_bounds() {
    let mut cache = TextCache::default();
    let style = TextStyle::default();
    assert!(
        cache
            .get_visible("OFFSCREEN", 14., 1., [0, 0, 0], &style, |_| false)
            .is_none()
    );
    assert!(
        cache.entries.is_empty(),
        "The early-out must precede outlining"
    );
    assert!(cache.extents.values().any(Option::is_some));
    let first = cache.get("日本語 🧪", 14., 1., [0, 0, 0], &style);
    assert!(
        cache
            .get_visible("日本語 🧪", 14., 1., [0, 0, 0], &style, |_| false)
            .is_none()
    );
    assert!(Rc::ptr_eq(
        &first,
        &cache.get("日本語 🧪", 14., 1., [0, 0, 0], &style)
    ));
}

#[test]
fn conservative_ascii_bounds_cover_shaped_ink_at_supported_sizes_and_fonts() {
    for family in ["Arial", "Times New Roman", "Courier New"] {
        for bold in [false, true] {
            let style = TextStyle {
                family: family.into(),
                bold,
                ..Default::default()
            };
            for size in [6., 14., 80.] {
                for zoom in [0.25, 1., 4.] {
                    for text in ["AV office fi", "(NH2) + COOH", "gypjQ_ / [] {}"] {
                        let Some(bound) = bounds::conservative(text, size, zoom, &style) else {
                            continue;
                        };
                        let paths = outline(text, size, zoom, [0, 0, 0], &style);
                        if let Some(ink) = paths.bounds {
                            assert!(
                                bound.x <= ink.x
                                    && bound.y <= ink.y
                                    && bound.x + bound.width >= ink.x + ink.width
                                    && bound.y + bound.height >= ink.y + ink.height,
                                "{family} bold={bold} {size} {zoom} {text}: {bound:?} must cover {ink:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(bounds::conservative("🧪", 14., 1., &Default::default()).is_none());
    assert!(
        bounds::conservative(
            "italic",
            14.,
            1.,
            &TextStyle {
                italic: true,
                ..Default::default()
            }
        )
        .is_none()
    );
}

#[test]
fn offscreen_text_extents_have_a_separate_bounded_budget() {
    let mut cache = TextCache::default();
    for i in 0..2100 {
        cache.get_visible(
            &i.to_string(),
            14.,
            1.,
            [0, 0, 0],
            &Default::default(),
            |_| false,
        );
        assert!(cache.entries.is_empty());
        assert!(cache.extents.len() <= LIMIT);
        assert!(cache.extent_cost <= 1024 * 1024);
    }
}

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
                    assert_eq!(
                        cached.pictures.iter().map(|(r, _)| *r).collect::<Vec<_>>(),
                        fresh.pictures.iter().map(|(r, _)| *r).collect::<Vec<_>>()
                    );
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

/// Apple Color Emoji's outlines are empty markers, so an emoji drawn from its
/// outline was blank. It must come out as a sharp colour picture beside the text.
#[cfg(target_os = "macos")]
#[test]
fn colour_emoji_draw_as_pictures_beside_the_text() {
    let style = TextStyle::default();
    let plain = outline("H", 14., 2., [0, 0, 0], &style);
    let text = outline("H💡", 14., 2., [0, 0, 0], &style);
    assert!(plain.pictures.is_empty());
    let [
        (
            rect,
            Handle::Rgba {
                width,
                height,
                pixels,
                ..
            },
        ),
    ] = &text.pictures[..]
    else {
        panic!("one picture expected, got {}", text.pictures.len());
    };
    let h = plain.bounds.unwrap();
    // Right of the H, about one em (28 px at zoom 2) tall, rasterized at twice that.
    assert!(rect.x >= h.x + h.width - 4., "{rect:?} {h:?}");
    assert!((20. ..=36.).contains(&rect.height), "{rect:?}");
    assert_eq!(*width as f32, rect.width * 2.);
    assert_eq!(*height as f32, rect.height * 2.);
    // In colour: some opaque pixel that is not grey.
    assert!(
        pixels
            .chunks(4)
            .any(|p| p[3] == 255 && (p[0] != p[1] || p[1] != p[2]))
    );
    assert!(text.bounds.unwrap().width > h.width + rect.width - 4.);
    // On the text's baseline: the H's ink ends 2 px above its bounds' bottom.
    let baseline = h.y + h.height - 2.;
    assert!(
        (rect.y + rect.height - baseline).abs() < 8.,
        "{rect:?} {h:?}"
    );
    // A repeated emoji shares one image.
    let twice = outline("💡💡", 14., 2., [0, 0, 0], &style);
    let [(_, a), (_, b)] = &twice.pictures[..] else {
        panic!("two pictures expected");
    };
    assert_eq!(a.id(), b.id());
}
