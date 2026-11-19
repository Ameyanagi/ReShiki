use super::*;
use crate::{
    arrows::{ArrowStyle, Preset},
    document::{Annotation, Arrow},
    typography::TextFormat,
};

/// A zigzag chain of `atoms` carbons starting at `at`, with bonds `bond` long.
fn chain(doc: &mut Document, atoms: usize, at: Point, bond: f32) -> Vec<u64> {
    let mut ids = vec![];
    for i in 0..atoms {
        let p = at.offset(
            i as f32 * bond * 0.866,
            if i % 2 == 0 { 0. } else { -bond * 0.5 },
        );
        ids.push(doc.add_atom("C", p));
        if i > 0 {
            doc.add_bond(ids[i - 1], ids[i], 1, "plain");
        }
    }
    ids
}
fn arrow(doc: &mut Document, start: Point, end: Point) -> u64 {
    let id = doc.next_id();
    doc.arrows.push(Arrow::new(
        id,
        start,
        end,
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    id
}
fn text(doc: &mut Document, text: &str, centre: Point) -> u64 {
    let id = doc.next_id();
    let mut caption = Annotation {
        id,
        position: Point::default(),
        text: text.into(),
        format: TextFormat::default(),
    };
    let (w, h) = caption.size();
    caption.position = centre.offset(-w / 2., -h / 2.);
    doc.annotations.push(caption);
    id
}
fn bounds(doc: &Document, ids: &[u64]) -> (Point, Point) {
    crate::scene::selection_bounds(doc, ids).unwrap()
}
fn centre(doc: &Document, ids: &[u64]) -> Point {
    let (lo, hi) = bounds(doc, ids);
    lo.lerp(hi, 0.5)
}
fn the_arrow(doc: &Document, id: u64) -> &Arrow {
    doc.arrows.iter().find(|a| a.id == id).unwrap()
}
fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

/// A + B → C drawn untidily: off the axis, uneven gaps, a short arrow.
fn untidy() -> (Document, [Vec<u64>; 4], u64) {
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let a = chain(&mut doc, 3, Point::new(0., 13.), bond);
    let plus = vec![text(&mut doc, "+", Point::new(110., -9.))];
    let b = chain(&mut doc, 2, Point::new(150., 20.), bond);
    let arrow = arrow(&mut doc, Point::new(260., 0.), Point::new(320., 0.));
    let c = chain(&mut doc, 4, Point::new(420., -25.), bond);
    (doc, [a, plus, b, c], arrow)
}

#[test]
fn a_plus_b_to_c_lines_up_on_the_arrow_with_even_gaps() {
    let (doc, units, arrow) = untidy();
    let bond = doc.drawing_style.bond_length_world;
    let cleaned = clean(&doc, &[], false).unwrap();
    let a = the_arrow(&cleaned, arrow);
    // The arrow keeps its middle and takes the standard length.
    assert_eq!(a.start.lerp(a.end, 0.5), Point::new(290., 0.));
    assert!(close(a.start.distance(a.end), bond * 3.2));
    for unit in &units {
        assert!(
            close(centre(&cleaned, unit).y, 0.),
            "{unit:?} is off the axis"
        );
    }
    let [a0, plus, b, c] = &units;
    let edges = [
        (bounds(&cleaned, a0).1.x, bounds(&cleaned, plus).0.x),
        (bounds(&cleaned, plus).1.x, bounds(&cleaned, b).0.x),
        (bounds(&cleaned, b).1.x, a.start.x),
        (a.end.x, bounds(&cleaned, c).0.x),
    ];
    for (left, right) in edges {
        assert!(close(right - left, bond * GAP), "gap {}", right - left);
    }
    // Cleaning a clean scheme changes nothing.
    assert_eq!(clean(&cleaned, &[], false).unwrap(), cleaned);
}

#[test]
fn text_beside_the_arrow_moves_with_it_and_is_centred() {
    let (mut doc, _, arrow) = untidy();
    let above = text(&mut doc, "Pd(PPh3)4, K2CO3", Point::new(280., -20.));
    let below = text(&mut doc, "THF, 60 °C", Point::new(300., 22.));
    let far = text(&mut doc, "Scheme 1", Point::new(150., 200.));
    let cleaned = clean(&doc, &[], false).unwrap();
    let a = the_arrow(&cleaned, arrow);
    let mid = a.start.lerp(a.end, 0.5);
    for id in [above, below] {
        let c = centre(&cleaned, &[id]);
        assert!(close(c.x, mid.x), "{c:?} vs {mid:?}");
    }
    assert!(centre(&cleaned, &[above]).y < 0. && centre(&cleaned, &[below]).y > 0.);
    // The arrow fits the wider text.
    let width = bounds(&cleaned, &[above]);
    assert!(a.start.distance(a.end) >= width.1.x - width.0.x);
    assert_eq!(
        cleaned.annotations.iter().find(|t| t.id == far),
        doc.annotations.iter().find(|t| t.id == far)
    );
}

#[test]
fn a_downward_arrow_centres_its_molecules_on_its_line() {
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let top = chain(&mut doc, 3, Point::new(-20., -120.), bond);
    let arrow = arrow(&mut doc, Point::new(0., -40.), Point::new(0., 40.));
    let bottom = chain(&mut doc, 3, Point::new(10., 110.), bond);
    let cleaned = clean(&doc, &[], false).unwrap();
    let a = the_arrow(&cleaned, arrow);
    assert!(close(a.start.x, 0.) && close(a.end.x, 0.) && a.end.y > a.start.y);
    for unit in [&top, &bottom] {
        assert!(close(centre(&cleaned, unit).x, 0.));
    }
    assert!(close(a.start.y - bounds(&cleaned, &top).1.y, bond * GAP));
    assert!(close(bounds(&cleaned, &bottom).0.y - a.end.y, bond * GAP));
}

#[test]
fn a_two_step_scheme_gets_equal_arrows_and_gaps() {
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let a = chain(&mut doc, 2, Point::new(0., 5.), bond);
    let first = arrow(&mut doc, Point::new(80., 0.), Point::new(130., 0.));
    let b = chain(&mut doc, 3, Point::new(170., -12.), bond);
    let second = arrow(&mut doc, Point::new(300., 2.), Point::new(420., 2.));
    let c = chain(&mut doc, 2, Point::new(470., 30.), bond);
    let cleaned = clean(&doc, &[], false).unwrap();
    let (one, two) = (the_arrow(&cleaned, first), the_arrow(&cleaned, second));
    assert!(close(
        one.start.distance(one.end),
        two.start.distance(two.end)
    ));
    assert!(close(one.start.y, 0.) && close(two.start.y, 0.));
    for (left, right) in [
        (bounds(&cleaned, &a).1.x, one.start.x),
        (one.end.x, bounds(&cleaned, &b).0.x),
        (bounds(&cleaned, &b).1.x, two.start.x),
        (two.end.x, bounds(&cleaned, &c).0.x),
    ] {
        assert!(close(right - left, bond * GAP), "gap {}", right - left);
    }
}

#[test]
fn style_bond_length_redraws_molecules_at_the_style_length() {
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let a = chain(&mut doc, 4, Point::new(0., 0.), bond * 2.);
    arrow(&mut doc, Point::new(200., 0.), Point::new(260., 0.));
    let length = |doc: &Document| {
        let p = |id| doc.atom(id).unwrap().position;
        p(a[0]).distance(p(a[1]))
    };
    assert!(close(length(&clean(&doc, &[], false).unwrap()), bond * 2.));
    assert!(close(length(&clean(&doc, &[], true).unwrap()), bond));
}

#[test]
fn a_selection_cleans_only_its_own_reaction() {
    let (mut doc, _, first) = untidy();
    let selected = doc.all_ids();
    let bond = doc.drawing_style.bond_length_world;
    let other = chain(&mut doc, 2, Point::new(0., 300.), bond);
    let second = arrow(&mut doc, Point::new(100., 310.), Point::new(150., 310.));
    let cleaned = clean(&doc, &selected, false).unwrap();
    assert_ne!(the_arrow(&cleaned, first), the_arrow(&doc, first));
    assert_eq!(the_arrow(&cleaned, second), the_arrow(&doc, second));
    assert_eq!(centre(&cleaned, &other), centre(&doc, &other));
}

#[test]
fn a_drawing_without_a_straight_arrow_is_refused() {
    let mut doc = Document::default();
    chain(&mut doc, 3, Point::default(), 42.);
    assert!(clean(&doc, &[], false).is_err());
}
