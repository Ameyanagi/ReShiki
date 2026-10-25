use super::*;

fn labelled_document() -> Document {
    let mut doc = Document::default();
    let first = doc.add_atom("C", Point::new(0., 0.));
    let second = doc.add_atom("O", Point::new(42., 0.));
    let atom = doc.atom_mut(first).unwrap();
    atom.map_num = 27;
    atom.display.mapping.show = Some(true);
    atom.display.mapping.offset = Some(Point::new(0., -38.));
    let atom = doc.atom_mut(second).unwrap();
    atom.map_num = 28;
    atom.display.mapping.show = Some(false);
    atom.display.mapping.offset = Some(Point::new(14., 20.));
    doc
}

#[test]
fn map_range_error_keeps_the_exact_text_and_is_used_by_set_map() {
    let expected = "Enter an atom map from 1 to 2147483647, or 0 to clear";
    assert_eq!(map_range_error(), expected);
    assert_eq!(
        set_map(&labelled_document(), 1, MAX_MAP + 1).unwrap_err(),
        expected,
    );
}

#[test]
fn set_maps_visible_clears_all_overrides_and_preserves_the_other_fields() {
    let source = labelled_document();
    for show in [false, true] {
        let mut expected = source.clone();
        expected.atom_labels.maps = show;
        for atom in &mut expected.atoms {
            atom.display.mapping.show = None;
        }
        assert_eq!(set_maps_visible(&source, show), expected);
    }
    assert_eq!(source, labelled_document());
}

#[test]
fn reset_label_offsets_clears_all_offsets_and_preserves_the_other_fields() {
    let source = labelled_document();
    let mut expected = source.clone();
    for atom in &mut expected.atoms {
        atom.display.mapping.offset = None;
    }
    assert_eq!(reset_label_offsets(&source), expected);
    assert_eq!(source, labelled_document());
}

#[test]
fn align_moves_only_the_product_plus_not_a_reagent_plus() {
    use crate::{
        document::{Annotation, Arrow},
        reactions::Participant,
    };
    let mut doc = Document::default();
    let mut part = |x: f32, map| {
        let id = doc.add_atom("C", Point::new(x, 0.));
        doc.atom_mut(id).unwrap().map_num = map;
        Participant {
            atoms: vec![id],
            coefficient: 1,
        }
    };
    let (mut r1, r2, p1, p2) = (part(0., 1), part(300., 2), part(500., 1), part(700., 2));
    // An unmapped reactant atom shifts the lane, so the products move right.
    r1.atoms.push(doc.add_atom("C", Point::new(-100., 0.)));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(300., 0.),
        Point::new(400., 0.),
        Default::default(),
        Default::default(),
    ));
    let plus = |doc: &mut Document, centre: Point, width_pt| {
        let id = doc.next_id();
        let mut sign = Annotation {
            id,
            position: Point::default(),
            text: "+".into(),
            format: crate::typography::TextFormat {
                width_pt,
                ..Default::default()
            },
        };
        let glyph = super::glyph_box(&sign).0;
        sign.position = Point::new(centre.x - glyph.x, centre.y - glyph.y);
        doc.annotations.push(sign);
        id
    };
    // The product `+` sits off-centre at the left of a frame wide enough to
    // put the frame's centre past both products; the reagent `+` projects
    // onto the gap.
    let product = plus(&mut doc, Point::new(560., 0.), Some(400.));
    let frame = doc.annotations.last().unwrap();
    assert!(frame.position.x + frame.size().0 / 2. > 700.);
    let reagent = plus(&mut doc, Point::new(600., -100.), None);
    let (p1_id, p2_id) = (p1.atoms[0], p2.atoms[0]);
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = vec![r1, r2];
    reaction.products = vec![p1, p2];
    reaction.annotations = vec![reagent, product];
    doc.reactions.push(reaction);
    let aligned = align(&doc, arrow).unwrap();
    let centre = |id| super::glyph_box(aligned.annotations.iter().find(|a| a.id == id).unwrap()).0;
    // Lane 600 - 200/3: products land at 1600/3 and 2500/3, gap moves 250/3.
    let x = |id| aligned.atom(id).unwrap().position.x;
    assert!((x(p1_id) - 1600. / 3.).abs() < 0.01);
    assert!((x(p2_id) - 2500. / 3.).abs() < 0.01);
    assert!(centre(product).distance(Point::new(560. + 250. / 3., 0.)) < 0.01);
    assert!(centre(reagent).distance(Point::new(600., -100.)) < 0.01);
}

#[test]
fn align_keeps_the_product_plus_inside_a_narrowed_gap() {
    use crate::{
        document::{Annotation, Arrow},
        reactions::Participant,
    };
    let mut doc = Document::default();
    let mut part = |x: f32, map| {
        let id = doc.add_atom("C", Point::new(x, 0.));
        doc.atom_mut(id).unwrap().map_num = map;
        Participant {
            atoms: vec![id],
            coefficient: 1,
        }
    };
    // The products sit 400 apart; their mapped reactants only 100 apart.
    let (mut r1, r2, p1, p2) = (part(0., 1), part(100., 2), part(1000., 1), part(1400., 2));
    r1.atoms.push(doc.add_atom("C", Point::new(350., 0.)));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(500., 0.),
        Point::new(800., 0.),
        Default::default(),
        Default::default(),
    ));
    let plus = doc.next_id();
    let mut sign = Annotation {
        id: plus,
        position: Point::default(),
        // Trailing whitespace must not shift the visible sign's centre.
        text: format!("+{}", " ".repeat(80)),
        format: Default::default(),
    };
    let glyph = super::glyph_box(&sign).0;
    sign.position = Point::new(1120. - glyph.x, -glyph.y);
    doc.annotations.push(sign);
    let (p1_id, p2_id) = (p1.atoms[0], p2.atoms[0]);
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = vec![r1, r2];
    reaction.products = vec![p1, p2];
    reaction.annotations = vec![plus];
    doc.reactions.push(reaction);
    let aligned = align(&doc, arrow).unwrap();
    let (centre, (w, _)) =
        super::glyph_box(aligned.annotations.iter().find(|a| a.id == plus).unwrap());
    let x = |id| aligned.atom(id).unwrap().position.x;
    // Keeping its offset would put the sign at 1020, before both products;
    // it is centred in the narrowed gap instead.
    assert!((x(p2_id) - x(p1_id) - 100.).abs() < 0.01 && w < 50.);
    let left = crate::scene::atom_label_ink_bounds(aligned.atom(p1_id).unwrap(), &aligned)
        .unwrap()
        .1
        .x;
    let right = crate::scene::atom_label_ink_bounds(aligned.atom(p2_id).unwrap(), &aligned)
        .unwrap()
        .0
        .x;
    assert!(centre.distance(Point::new((left + right) / 2., 0.)) < 0.01);
}
