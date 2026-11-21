use super::*;
use reshiki::{
    document::Annotation,
    graphics::{
        Graphic, GraphicKind,
        table::{Cell, Table},
    },
};

fn drawing() -> (Document, u64, u64, u64) {
    let mut doc = Document::default();
    let visible = doc.add_atom("C", World::new(30., 185.));
    let hidden = doc.add_atom("C", World::new(130., 185.));
    doc.add_bond(visible, hidden, 1, "plain");
    let text = doc.next_id();
    doc.annotations.push(Annotation {
        id: text,
        position: World::new(5., 5.),
        text: "alpha beta gamma delta".into(),
        angle: 0.,
        format: Default::default(),
    });
    let mut table = Graphic::dragged(
        100,
        GraphicKind::Rectangle,
        World::default(),
        World::new(200., 220.),
        Default::default(),
        Default::default(),
        false,
    );
    table.kind = GraphicKind::Table;
    table.layer = -1;
    table.table = Some(Box::new(Table {
        cells: vec![
            Cell {
                id: 1,
                lo: World::default(),
                hi: World::new(0.5, 1.),
                members: vec![visible, hidden, text],
                borders: vec![],
            },
            Cell {
                id: 2,
                lo: World::new(0.5, 0.),
                hi: World::new(1., 1.),
                members: vec![],
                borders: vec![],
            },
        ],
    }));
    doc.graphics.push(table);
    doc.validate().unwrap();
    (doc, visible, hidden, text)
}

#[test]
fn cell_clipping_is_shared_by_pointer_selection_atom_snapping_and_bond_hits() {
    let (doc, visible, hidden, _) = drawing();
    let before = doc.file_json().unwrap();
    let outside = doc.atom(hidden).unwrap().position;
    assert_eq!(doc.nearest(outside, 8.), None);
    assert_ne!(hit::hit_object(&doc, outside, 8.), Some(hidden));
    assert!(!hit::hit_selection(&doc, outside, 8.).contains(&hidden));
    assert_eq!(
        reshiki::editing::nearest_bond(&doc, World::new(120., 185.), 8.),
        None
    );
    assert_eq!(
        reshiki::editing::nearest_bond(&doc, World::new(70., 185.), 8.),
        Some(0)
    );
    assert_eq!(
        hit::bond_target(
            &doc,
            doc.atom(visible).unwrap().position,
            outside,
            Some(visible),
            8.
        )
        .1,
        None
    );
    let canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    assert_eq!(canvas.doc.nearest(outside, 8.), None);
    assert_eq!(
        doc.file_json().unwrap(),
        before,
        "picking never clips stored chemistry"
    );
}

#[test]
fn mechanism_target_picking_respects_the_same_cell_clip_for_atoms_marks_and_bonds() {
    use reshiki::arrow_anchors::{self, Pick};
    let (mut doc, visible, hidden, _) = drawing();
    doc.atom_mut(hidden).unwrap().element = "O".into();
    reshiki::scientific::attach(
        doc.atom_mut(visible).unwrap(),
        reshiki::scientific::SymbolKind::LonePair,
        World::new(80., 0.),
    )
    .unwrap();
    doc.validate().unwrap();
    let before = doc.clone();
    let paint = std::rc::Rc::new(reshiki::rear_opacity::Paint::new(&doc));
    for p in [
        doc.atom(hidden).unwrap().position,
        World::new(120., 185.),
        World::new(110., 185.),
    ] {
        assert_eq!(arrow_anchors::pick(&doc, p, 2.), None);
        assert_eq!(
            arrow_anchors::pick_with_paint(&doc, p, 2., paint.clone()),
            None
        );
    }
    assert_eq!(
        arrow_anchors::pick(&doc, doc.atom(visible).unwrap().position, 2.),
        Some(Pick::Atom(visible))
    );
    assert!(matches!(
        arrow_anchors::pick(&doc, World::new(70., 185.), 2.),
        Some(Pick::Bond { a, b, .. }) if a == visible && b == hidden
    ));
    assert_eq!(
        doc, before,
        "picking keeps the clipped chemistry and mark intact"
    );
}

#[test]
fn wrapped_lower_caption_lines_use_the_painted_layout_for_selection_and_bond_targeting() {
    let (doc, _, _, text) = drawing();
    let original = doc.annotations.iter().find(|a| a.id == text).unwrap();
    let cells = reshiki::scene::table::Layout::new(&doc);
    let effective = cells.annotation(original);
    let layout = reshiki::typography::layout(&effective.text, &effective.format);
    let fragment = layout
        .fragments
        .iter()
        .find(|f| f.position.y > original.size().1)
        .unwrap();
    let (lo, hi) =
        reshiki::style::text_ink_boxes(&fragment.text, fragment.style.size(), &fragment.style)[0];
    let p = effective.position.offset(
        fragment.position.x + (lo.x + hi.x) * 0.5,
        fragment.position.y + (lo.y + hi.y) * 0.5,
    );
    assert!(!original.contains(p));
    assert_eq!(hit::hit_object(&doc, p, 4.), Some(text));
    assert_eq!(hit::bond_text_at(&doc, p, 4.), Some(text));
    assert_eq!(hit::hit_selection(&doc, p, 4.), vec![text]);
    let region = [
        World::new(0., 0.),
        World::new(100., 0.),
        World::new(100., 170.),
        World::new(0., 170.),
    ];
    assert!(reshiki::selection_region::objects(&doc, &region).contains(&text));
    assert!(original.format.width_pt.is_none());
    let raw_tail = original.position.offset(original.size().0 - 2., 5.);
    assert_ne!(hit::hit_object(&doc, raw_tail, 4.), Some(text));
    assert_eq!(hit::bond_text_at(&doc, raw_tail, 4.), None);
}
