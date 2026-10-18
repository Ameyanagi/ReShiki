use super::{Number, Owner, indicators, number_style, segment_hits_rect};
use crate::document::{Document, Point};

const CAPTION: &str = "attached caption text";

/// An sp3 carbon whose two bonds leave the open side pointing along `open_x`.
fn branch(open_x: f32, text: &str) -> (Document, u64) {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    for y in [-15., 15.] {
        let b = doc.add_atom("C", Point::new(-26. * open_x, y));
        doc.add_bond(a, b, 1, "plain");
    }
    doc.atom_mut(a).unwrap().display.number = Some(Number {
        text: text.into(),
        offset: None,
        style: number_style(),
    });
    (doc, a)
}

fn bounds(doc: &Document) -> (Point, Point) {
    let [label] = indicators(doc).try_into().unwrap();
    assert!(matches!(label.owner, Owner::Number(_)));
    (label.origin, label.origin.offset(label.width, label.height))
}

fn clear_of_bonds(doc: &Document, (lo, hi): (Point, Point)) -> bool {
    doc.bonds.iter().all(|b| {
        let a = doc.atom(b.a).unwrap().position;
        let z = doc.atom(b.b).unwrap().position;
        !segment_hits_rect(a, z, lo, hi)
    })
}

#[test]
fn long_attached_text_grows_away_from_the_structure() {
    for open_x in [1., -1.] {
        let (doc, _) = branch(open_x, CAPTION);
        let (lo, hi) = bounds(&doc);
        assert!(clear_of_bonds(&doc, (lo, hi)), "{open_x}: {lo:?}..{hi:?}");
        // The text starts beside the atom on the open side and runs outward,
        // instead of being centred over the atom and its bonds.
        if open_x > 0. {
            assert!(lo.x > 0. && lo.x < 20., "{lo:?}");
        } else {
            assert!(hi.x < 0. && hi.x > -20., "{hi:?}");
        }
        assert!(lo.y < 0. && hi.y > 0., "{lo:?}..{hi:?}");
    }
}

#[test]
fn short_numbers_keep_their_centred_candidates() {
    let (doc, _) = branch(1., "7");
    let [label] = indicators(&doc).try_into().unwrap();
    let radii = super::indicator_radii(label.height);
    let distance = label.center.distance(Point::new(0., 0.));
    assert!(
        radii.iter().any(|r| (r - distance).abs() < 0.01),
        "{distance} {radii:?}"
    );
}

#[test]
fn manual_positions_stay_exact_for_long_text() {
    let (mut doc, a) = branch(1., CAPTION);
    Owner::Number(a).set_offset(&mut doc, Some(Point::new(-5., -40.)));
    let [label] = indicators(&doc).try_into().unwrap();
    assert_eq!(label.center, Point::new(-5., -40.));
}

#[test]
fn long_attached_text_avoids_free_captions() {
    let (mut doc, _) = branch(1., CAPTION);
    let open = bounds(&doc);
    let id = doc.next_id();
    doc.annotations.push(crate::document::Annotation {
        id,
        position: open.0,
        text: "caption".into(),
        format: Default::default(),
    });
    let caption = crate::atom_text::text_ink_bounds(&doc.annotations[0]).unwrap();
    let (lo, hi) = bounds(&doc);
    let overlaps =
        lo.x < caption.1.x && caption.0.x < hi.x && lo.y < caption.1.y && caption.0.y < hi.y;
    assert!(!overlaps, "{lo:?}..{hi:?} vs {caption:?}");
    assert!(clear_of_bonds(&doc, (lo, hi)));
}
