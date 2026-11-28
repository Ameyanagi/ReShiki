use super::*;

fn linear_apart(rest: &[Bounds], fragment: &[Bounds]) -> bool {
    enclosing(fragment).is_none_or(|all| {
        rest.iter()
            .filter(|rest| overlap(rest, &all))
            .all(|rest| !fragment.iter().any(|f| overlap(rest, f)))
    })
}

fn indexed_apart(rest: &mut [Bounds], fragment: &[Bounds]) -> bool {
    let index = BoundsTree::new(rest, 0);
    enclosing(fragment).is_none_or(|all| fragment.iter().all(|f| !index.overlaps(rest, f, &all)))
}

#[test]
fn bounds_index_matches_linear_for_touching_and_nonfinite_rectangles() {
    let values = [
        f32::NEG_INFINITY,
        -f32::MAX,
        -1.,
        -0.,
        0.,
        f32::from_bits(1),
        1.,
        f32::MAX,
        f32::INFINITY,
        f32::NAN,
    ];
    let mut rest = Vec::new();
    for &x in &values {
        for &y in &values {
            rest.push((World::new(x, y), World::new(y, x)));
            rest.push((World::new(x, x), World::new(y, y)));
        }
    }
    let original = rest.clone();
    for &x in &values {
        for &y in &values {
            for fragment in [
                vec![],
                vec![(World::new(x, y), World::new(y, x))],
                vec![
                    (World::new(x, x), World::new(y, y)),
                    (World::new(-1., -1.), World::new(0., 0.)),
                ],
            ] {
                assert_eq!(
                    indexed_apart(&mut rest, &fragment),
                    linear_apart(&original, &fragment),
                    "{fragment:?}"
                );
            }
        }
    }
    let touching = (World::new(0., 0.), World::new(1., 1.));
    for corner in [World::new(0., 0.), World::new(1., 1.)] {
        assert!(!indexed_apart(&mut [touching], &[(corner, corner)]));
    }
}

#[test]
fn bounds_index_matches_linear_across_sparse_and_overlapping_layouts() {
    let mut seed = 0xabcde_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) % 2000) as f32 - 1000.
    };
    for count in [0, 1, 7, 8, 9, 63, 64, 65, 128, 513] {
        let original: Vec<_> = (0..count)
            .map(|_| {
                let lo = World::new(next(), next());
                (lo, lo.offset(next().abs() / 50., next().abs() / 50.))
            })
            .collect();
        let mut rest = original.clone();
        let index = BoundsTree::new(&mut rest, 0);
        for _ in 0..100 {
            let fragment: Vec<_> = (0..17)
                .map(|_| {
                    let lo = World::new(next(), next());
                    (lo, lo.offset(next().abs() / 25., next().abs() / 25.))
                })
                .collect();
            let actual = enclosing(&fragment)
                .is_none_or(|all| fragment.iter().all(|f| !index.overlaps(&rest, f, &all)));
            assert_eq!(actual, linear_apart(&original, &fragment), "{count}");
        }
    }
}

#[test]
fn retained_bounds_follow_translucent_foreground_edits_and_undo() {
    use reshiki::graphics::{GraphicKind, GraphicStyle};
    let mut doc = Document::default();
    let a = doc.add_atom("C", World::new(-400., 0.));
    let b = doc.add_atom("N", World::new(-360., 0.));
    doc.add_bond(a, b, 1, "plain");
    // Invisible, distant atoms force the retained drawing to use its index.
    for i in 0..80 {
        doc.add_atom("*", World::new(2000. + i as f32 * 40., 2000.));
    }
    let mut foreground = Graphic::dragged(
        doc.next_id(),
        GraphicKind::Rectangle,
        World::new(-20., -40.),
        World::new(80., 40.),
        GraphicStyle::default(),
        Default::default(),
        false,
    );
    foreground.layer = 1;
    foreground.style.fill = Some(reshiki::palette::Color::Ink);
    foreground.style.effects.opacity = Some(0.4);
    doc.graphics.push(foreground);
    let saved = doc.clone();
    let camera = Camera {
        zoom: 1.,
        ..Default::default()
    };
    let mut cache = SceneCache::default();
    let mut moved = |doc: &Document, dy| {
        let mut preview = doc.clone();
        preview.translate(&[a, b], 400., dy);
        Fragment::new(
            &mut cache,
            doc,
            &preview,
            camera,
            Some(World::new(400., dy)),
        )
    };
    assert!(moved(&doc, 0.).is_none(), "foreground ink meets the move");
    let first = moved(&doc, 200.).unwrap().remainder;
    assert!(first.index.is_some());
    assert!(Rc::ptr_eq(&first, &moved(&doc, 201.).unwrap().remainder));
    doc.graphics[0].origin = doc.graphics[0].origin.offset(0., 200.);
    assert!(
        moved(&doc, 200.).is_none(),
        "an edit moved foreground ink into the preview"
    );
    doc = saved;
    let restored = moved(&doc, 200.).unwrap().remainder;
    assert!(
        !Rc::ptr_eq(&first, &restored),
        "undo rebuilt the retained index"
    );
    assert!(Rc::ptr_eq(&restored, &moved(&doc, 201.).unwrap().remainder));
}
