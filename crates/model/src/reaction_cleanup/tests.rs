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
fn a_caption_under_a_molecule_moves_with_it() {
    let (mut doc, [a, ..], _) = untidy();
    let below = centre(&doc, &a).offset(0., 40.);
    let caption = text(&mut doc, "1", below);
    let offset = |doc: &Document| {
        let (m, c) = (centre(doc, &a), centre(doc, &[caption]));
        (c.x - m.x, c.y - m.y)
    };
    let cleaned = clean(&doc, &[], false).unwrap();
    let (before, after) = (offset(&doc), offset(&cleaned));
    assert!(close(before.0, after.0) && close(before.1, after.1));
    assert_ne!(centre(&cleaned, &a), centre(&doc, &a));
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
fn style_bond_length_moves_a_3d_only_molecule_without_scaling_its_drawing() {
    use crate::models3d::{Display, ForceField, Model3d, Placed};

    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let atoms = chain(&mut doc, 2, Point::new(0., 30.), bond * 2.);
    let mut model = Model3d::new(
        &doc,
        atoms
            .iter()
            .enumerate()
            .map(|(i, &id)| Placed {
                id,
                xyz: [i as f32, 0., 0.],
            })
            .collect(),
        vec![],
        ForceField::Mmff94s,
    );
    model.display = Display::ThreeD;
    doc.models.push(model);
    arrow(&mut doc, Point::new(200., 0.), Point::new(260., 0.));

    let placed = clean(&doc, &[], false).unwrap();
    let cleaned = clean(&doc, &[], true).unwrap();
    assert_ne!(centre(&cleaned, &atoms), centre(&doc, &atoms));
    assert!(close(centre(&cleaned, &atoms).y, 0.));
    for id in atoms {
        assert_eq!(cleaned.atom(id), placed.atom(id));
    }
}

#[test]
fn style_bond_length_scales_grouped_molecules_about_their_own_centres() {
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let a = chain(&mut doc, 2, Point::new(0., 0.), bond);
    let b = chain(&mut doc, 2, Point::new(80., 22.), bond * 2.);
    doc.group_selection(&[a[0], b[0]]).unwrap();
    arrow(&mut doc, Point::new(240., 0.), Point::new(300., 0.));

    let cleaned = clean(&doc, &[], true).unwrap();
    for molecule in [&a, &b] {
        let p = |id| cleaned.atom(id).unwrap().position;
        assert!(close(p(molecule[0]).distance(p(molecule[1])), bond));
    }
    let offset = |doc: &Document| {
        let (a, b) = (
            crate::editing::center(doc, &a),
            crate::editing::center(doc, &b),
        );
        b.offset(-a.x, -a.y)
    };
    assert!(close(offset(&doc).distance(offset(&cleaned)), 0.));
}

#[test]
fn style_bond_length_scales_a_collapsed_ligand_with_its_metal() {
    // The ligand's atoms come before the metal, and only the abbreviation
    // joins the ring to the metal's bond.
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let ligand = chain(&mut doc, 2, Point::new(0., 30.), bond * 2.);
    let attachment =
        crate::attachments::add(&mut doc, &ligand, crate::attachments::Kind::MultiCenter).unwrap();
    let at = doc.atom(attachment).unwrap().position;
    let metal = doc.add_atom("Fe", at.offset(bond * 2., 0.));
    doc.add_bond(attachment, metal, 1, "plain");
    doc.contract(&[ligand[0], ligand[1], attachment], "L", "L")
        .unwrap();
    arrow(&mut doc, Point::new(200., 0.), Point::new(260., 0.));

    let cleaned = clean(&doc, &[], true).unwrap();
    for (a, b) in [(ligand[0], ligand[1]), (attachment, metal)] {
        let p = |id| cleaned.atom(id).unwrap().position;
        assert!(close(p(a).distance(p(b)), bond));
    }
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
fn grouped_arrows_stay_put_and_arrows_with_attached_labels_are_cleaned() {
    let (mut doc, _, labelled) = untidy();
    let grouped = arrow(&mut doc, Point::new(550., 0.), Point::new(610., 0.));
    // Grouped with only its own label, so the group alone keeps it out.
    let caption = text(&mut doc, "Step 2", Point::new(580., -35.));
    let label = text(&mut doc, "heat", Point::new(290., -35.));
    for (arrow, annotation) in [(grouped, caption), (labelled, label)] {
        doc.arrow_labels.push(crate::arrow_labels::ArrowLabel {
            arrow,
            annotation,
            side: crate::arrow_labels::Side::Above,
            gap: None,
        });
    }
    doc.group_selection(&[grouped, caption]).unwrap();
    let before = doc.clone();
    crate::arrow_labels::reconcile(&mut doc, &before);

    let cleaned = clean(&doc, &[], false).unwrap();
    assert_eq!(the_arrow(&cleaned, grouped), the_arrow(&doc, grouped));
    assert_eq!(centre(&cleaned, &[caption]), centre(&doc, &[caption]));
    let a = the_arrow(&cleaned, labelled);
    assert!(close(
        a.start.distance(a.end),
        doc.drawing_style.bond_length_world * 3.2
    ));
    assert_eq!(cleaned.arrow_labels, doc.arrow_labels);
    assert!(close(
        centre(&cleaned, &[label]).x,
        a.start.lerp(a.end, 0.5).x
    ));
}

#[test]
fn a_drawing_without_a_straight_arrow_is_refused() {
    let mut doc = Document::default();
    chain(&mut doc, 3, Point::default(), 42.);
    assert!(clean(&doc, &[], false).is_err());
}

#[test]
fn arrows_facing_each_other_share_one_row() {
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let a = chain(&mut doc, 2, Point::new(0., 8.), bond);
    let forward = arrow(&mut doc, Point::new(90., 0.), Point::new(130., 0.));
    let b = chain(&mut doc, 3, Point::new(180., -10.), bond);
    let back = arrow(&mut doc, Point::new(380., 3.), Point::new(320., 3.));
    let c = chain(&mut doc, 2, Point::new(420., 12.), bond);
    let cleaned = clean(&doc, &[], false).unwrap();
    let (one, two) = (the_arrow(&cleaned, forward), the_arrow(&cleaned, back));
    assert!(one.end.x > one.start.x && two.end.x < two.start.x);
    for (left, right) in [
        (bounds(&cleaned, &a).1.x, one.start.x),
        (one.end.x, bounds(&cleaned, &b).0.x),
        (bounds(&cleaned, &b).1.x, two.end.x),
        (two.start.x, bounds(&cleaned, &c).0.x),
    ] {
        assert!(close(right - left, bond * GAP), "gap {}", right - left);
    }
}

#[test]
fn reagents_side_by_side_stay_apart_and_widen_the_arrow() {
    let (mut doc, _, arrow) = untidy();
    let left = text(&mut doc, "NaOH", Point::new(235., -18.));
    let right = text(&mut doc, "NaOH", Point::new(335., -18.));
    let cleaned = clean(&doc, &[], false).unwrap();
    let (l, r) = (bounds(&cleaned, &[left]), bounds(&cleaned, &[right]));
    assert!(l.1.x <= r.0.x + 0.05, "{l:?} overlaps {r:?}");
    let a = the_arrow(&cleaned, arrow);
    let mid = a.start.lerp(a.end, 0.5).x;
    assert!(close((l.0.x + r.1.x) / 2., mid));
    assert!(a.start.distance(a.end) >= r.1.x - l.0.x);
}

#[test]
fn style_bond_length_leaves_molecules_outside_reactions_alone() {
    let (mut doc, _, _) = untidy();
    let bond = doc.drawing_style.bond_length_world;
    let lone = chain(&mut doc, 3, Point::new(0., 400.), bond * 2.);
    let cleaned = clean(&doc, &[], true).unwrap();
    for id in lone {
        assert_eq!(cleaned.atom(id), doc.atom(id));
    }
}

#[test]
fn a_selected_arrow_brings_its_recorded_reaction() {
    let (mut doc, [a, _, _, c], arrow) = untidy();
    let mut reaction = crate::reactions::Reaction::new(arrow);
    for (atoms, role) in [
        (&a, crate::reactions::Role::Reactant),
        (&c, crate::reactions::Role::Product),
    ] {
        reaction
            .participants_mut(role)
            .push(crate::reactions::Participant {
                atoms: atoms.clone(),
                coefficient: 1,
            });
    }
    doc.reactions.push(reaction);
    let cleaned = clean(&doc, &[arrow], false).unwrap();
    for unit in [&a, &c] {
        assert!(
            close(centre(&cleaned, unit).y, 0.),
            "{unit:?} is off the axis"
        );
    }
}

#[test]
fn a_plus_symbol_counts_as_a_sign() {
    let mut doc = Document::default();
    let bond = doc.drawing_style.bond_length_world;
    let a = chain(&mut doc, 2, Point::new(0., 0.), bond);
    let plus = doc.next_id();
    doc.graphics.push(crate::graphics::Graphic::dragged(
        plus,
        crate::graphics::GraphicKind::Symbol(crate::scientific::SymbolKind::Plus),
        Point::new(100., 14.),
        Point::new(100., 14.),
        Default::default(),
        Default::default(),
        false,
    ));
    let b = chain(&mut doc, 2, Point::new(140., 0.), bond);
    arrow(&mut doc, Point::new(260., 0.), Point::new(300., 0.));
    let cleaned = clean(&doc, &[], false).unwrap();
    let gaps = [
        bounds(&cleaned, &[plus]).0.x - bounds(&cleaned, &a).1.x,
        bounds(&cleaned, &b).0.x - bounds(&cleaned, &[plus]).1.x,
    ];
    assert!(gaps.iter().all(|gap| close(*gap, bond * GAP)), "{gaps:?}");
}

#[test]
fn a_molecule_far_along_the_line_is_not_part_of_the_reaction() {
    let (mut doc, _, _) = untidy();
    let bond = doc.drawing_style.bond_length_world;
    let far = chain(&mut doc, 2, Point::new(1200., 5.), bond * 1.5);
    let cleaned = clean(&doc, &[], true).unwrap();
    for id in far {
        assert_eq!(cleaned.atom(id), doc.atom(id));
    }
}

#[test]
fn an_arrow_selected_alone_without_roles_is_refused() {
    let (doc, _, arrow) = untidy();
    assert!(clean(&doc, &[arrow], false).is_err());
}

#[test]
fn a_molecule_on_the_line_near_the_arrow_stays_on_the_axis() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(35., 0.));
    let b = doc.add_atom("C", Point::new(77., 0.));
    doc.add_bond(a, b, 1, "plain");
    let arrow = arrow(&mut doc, Point::new(100., 0.), Point::new(140., 0.));
    let cleaned = clean(&doc, &[], false).unwrap();
    let ink = bounds(&cleaned, &[a, b]);
    assert!(ink.1.x < the_arrow(&cleaned, arrow).start.x);
}
