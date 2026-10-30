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
    let extent = |id| {
        crate::scene::participant_label_ink_boxes(&aligned, &[id])
            .into_iter()
            .fold((x(id), x(id)), |(lo, hi), (a, b)| {
                (lo.min(a.x), hi.max(b.x))
            })
    };
    let left = extent(p1_id).1;
    let right = extent(p2_id).0;
    assert!(centre.distance(Point::new((left + right) / 2., 0.)) < 0.01);
}

fn label_edit_fixture(role: Role) -> (Document, [u64; 2], u64, u64) {
    use crate::{
        atom_labels::LabelAttachment,
        document::{Annotation, Arrow},
        reactions::Participant,
    };
    let mut doc = Document::default();
    let a = doc.add_atom("*", Point::default());
    let b = doc.add_atom("*", Point::new(100., 0.));
    for (id, text) in [(a, "O"), (b, "N")] {
        let atom = doc.atom_mut(id).unwrap();
        atom.display.variable = Some(text.into());
        atom.display.label_attachment = LabelAttachment::Left;
    }
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(150., 0.),
        Point::new(250., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut sign = |center: Point| {
        let id = doc.next_id();
        let mut annotation = Annotation {
            id,
            position: Point::default(),
            text: "+".into(),
            format: Default::default(),
        };
        let origin = glyph_box(&annotation).0;
        annotation.position = Point::new(center.x - origin.x, center.y - origin.y);
        doc.annotations.push(annotation);
        id
    };
    let plus = sign(Point::new(55., 0.));
    // This tracked conditions sign projects onto the same gap but is outside
    // the participant row. It must never be claimed as a separator.
    let conditions_plus = sign(Point::new(55., -100.));
    let mut reaction = Reaction::new(arrow);
    *reaction.participants_mut(role) = [a, b]
        .map(|id| Participant {
            atoms: vec![id],
            coefficient: 1,
        })
        .to_vec();
    reaction.annotations = vec![conditions_plus, plus];
    doc.reactions.push(reaction);
    (doc, [a, b], plus, conditions_plus)
}

fn assert_label_separator_gap(doc: &Document, atoms: [u64; 2], plus: u64) {
    let extent = |id| {
        let x = doc.atom(id).unwrap().position.x;
        crate::scene::participant_label_ink_boxes(doc, &[id])
            .into_iter()
            .fold((x, x), |(lo, hi), (a, b)| (lo.min(a.x), hi.max(b.x)))
    };
    let annotation = doc.annotations.iter().find(|a| a.id == plus).unwrap();
    let (center, (width, _)) = glyph_box(annotation);
    let clearance = doc.drawing_style.world(doc.drawing_style.margin_width_pt);
    assert!(center.x - width / 2. - extent(atoms[0]).1 >= clearance - 0.01);
    assert!(extent(atoms[1]).0 - center.x - width / 2. >= clearance - 0.01);
}

#[test]
fn participant_label_growth_reflows_all_roles_in_one_undo_frame() {
    use crate::{document::History, transaction};
    for role in Role::ALL {
        let (mut doc, [a, b], plus, conditions) = label_edit_fixture(role);
        let before = doc.clone();
        let mut history = History::default();
        doc.atom_mut(a)
            .unwrap()
            .display
            .set_variable(Some("CO2CH2CH2CH2CH3".into()));
        transaction::apply(&mut doc, &mut history, before.clone(), false).unwrap();
        assert_label_separator_gap(&doc, [a, b], plus);
        // Room opens away from the arrow: reactants move back, others forward.
        let x = |d: &Document, id| d.atom(id).unwrap().position.x;
        if role == Role::Reactant {
            assert!(x(&doc, a) < x(&before, a) && x(&doc, b) == x(&before, b));
        } else {
            assert!(x(&doc, b) > x(&before, b) && x(&doc, a) == x(&before, a));
        }
        assert_eq!(
            doc.annotations.iter().find(|a| a.id == conditions),
            before.annotations.iter().find(|a| a.id == conditions),
        );
        let edited = doc.clone();
        assert!(history.undo(&mut doc));
        assert_eq!(doc, before);
        assert!(!history.can_undo());
        assert!(history.redo(&mut doc));
        assert_eq!(doc, edited);
        // A second reconcile cannot drift the sign or participants.
        let _ = transaction::reconcile(&mut doc, edited.clone()).unwrap();
        assert_eq!(doc, edited);
    }
}

#[test]
fn hydrogen_counts_and_charges_reflow_measured_participant_ink() {
    use crate::{atom_labels::HydrogenPosition, transaction};
    for hydrogens in [true, false] {
        let (mut doc, [a, b], plus, _) = label_edit_fixture(Role::Reactant);
        // Crowd the sign so the longer label reaches it.
        doc.atom_mut(b).unwrap().position.x = 70.;
        let atom = doc.atom_mut(a).unwrap();
        atom.position.x = 30.;
        atom.element = "N".into();
        atom.display.variable = None;
        atom.display.hydrogen_position = HydrogenPosition::Right;
        atom.label_h = 1;
        let before = doc.clone();
        if hydrogens {
            doc.atom_mut(a).unwrap().label_h = 99;
        } else {
            doc.atom_mut(a).unwrap().charge = 12;
        }
        let _ = transaction::reconcile(&mut doc, before.clone()).unwrap();
        assert_label_separator_gap(&doc, [a, b], plus);
        assert_ne!(
            doc.annotations.iter().find(|n| n.id == plus),
            before.annotations.iter().find(|n| n.id == plus),
        );
    }
}

#[test]
fn computed_label_repair_is_stable_when_ink_already_landed() {
    let (mut doc, [a, b], plus, _) = label_edit_fixture(Role::Agent);
    doc.atom_mut(a)
        .unwrap()
        .display
        .set_variable(Some("CO2CH2CH2CH2CH3".into()));
    let before = doc.clone();
    reflow_separators(&before, &mut doc);
    assert_label_separator_gap(&doc, [a, b], plus);
    let repaired = doc.clone();
    reflow_separators(&repaired, &mut doc);
    assert_eq!(doc, repaired);
}

#[test]
fn unrelated_edits_and_explicit_separator_drags_keep_their_placement() {
    use crate::transaction;
    let (mut doc, [a, _], plus, _) = label_edit_fixture(Role::Product);
    let before = doc.clone();
    doc.annotations
        .iter_mut()
        .find(|a| a.id == plus)
        .unwrap()
        .position
        .x += 10.;
    let expected = doc.clone();
    let _ = transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(doc, expected);
    let before = doc.clone();
    doc.atom_mut(a).unwrap().position.x -= 20.;
    let expected = doc.clone();
    let _ = transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(doc, expected);
}
