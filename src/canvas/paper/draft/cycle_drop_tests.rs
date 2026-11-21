use super::*;
use reshiki::document::{Annotation, Document};

fn text(doc: &mut Document, at: World, text: &str) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: at,
        text: text.into(),
        format: reshiki::typography::TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
        angle: 0.,
    });
    id
}

fn middle(doc: &Document, id: u64) -> World {
    let (lo, hi) = reshiki::scene::selection_bounds(doc, &[id]).unwrap();
    lo.lerp(hi, 0.5)
}

#[test]
fn dragging_onto_an_arc_lights_it_and_tags_below_the_piece() {
    let mut doc = Document::default();
    let parts = [
        (0., -200., "Pd(0)L2"),
        (170., 90., "ArPdX"),
        (-230., 90., "ArPdAr"),
    ]
    .map(|(x, y, t)| text(&mut doc, World::new(x, y), t));
    let (cycle, _) = reshiki::cycles::make(&mut doc, &parts).unwrap();
    let free = text(&mut doc, World::new(0., 0.), "Ar–X");
    let (center, radius) = doc
        .graphics
        .iter()
        .find(|g| g.id == cycle)
        .unwrap()
        .cycle_circle();
    // Halfway round the arc from the first intermediate to the second.
    let angle = |id| {
        let p = middle(&doc, id);
        (p.y - center.y).atan2(p.x - center.x)
    };
    let mid = (angle(parts[0]) + angle(parts[1])) * 0.5;
    let to = center.offset(radius * mid.cos(), radius * mid.sin());
    let from = middle(&doc, free);
    let mut canvas = crate::canvas::tests::chain_canvas(&doc, crate::canvas::ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.smart_guides = false;
    let bounds = Rectangle::new(Point::new(0., 0.), iced::Size::new(1200., 900.));
    let state = State {
        gesture: Some(Gesture::Move {
            start: from,
            ids: vec![free],
            clicked: vec![free],
        }),
        cursor: Some(canvas.camera.screen(to, bounds)),
        ..Default::default()
    };
    let mut draft = Draft::new(&doc);
    canvas.preview_move(&mut draft, &state, bounds);
    assert_eq!(
        reshiki::cycles::role_of(&draft.preview, free),
        Some((cycle, reshiki::cycles::Role::Intermediate(1))),
        "the preview drawing already contains the proposed intermediate"
    );
    let target = draft.cycle_drop.expect("a drop target");
    assert_eq!(target.tag, "Insert between 1 and 2");
    assert_eq!(target.arc.len(), 1, "the arc from 1 to 2 lights up");
    let (at, tag, valid) = draft.chain_badge.expect("a tag");
    assert_eq!((tag.as_str(), valid), ("Insert between 1 and 2", true));
    // The tag hangs below the dragged text, not over it.
    let (_, hi) = reshiki::scene::selection_bounds(&draft.preview, &[free]).unwrap();
    assert!(
        at.y >= hi.y,
        "tag at {} above the ink's bottom {}",
        at.y,
        hi.y
    );
}
