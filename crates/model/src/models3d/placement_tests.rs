use super::*;
use crate::{
    document::{Annotation, Arrow},
    reactions::{Role, assign},
    scene,
};

struct Scheme {
    doc: Document,
    species: Vec<Vec<u64>>,
    arrow: u64,
    plus: Option<u64>,
}

fn molecule(
    doc: &mut Document,
    atoms: &[(&str, f32, f32)],
    bonds: &[(usize, usize, u8)],
) -> Vec<u64> {
    let ids: Vec<_> = atoms
        .iter()
        .map(|&(symbol, x, y)| doc.add_atom(symbol, Point::new(x, y)))
        .collect();
    for &(a, b, order) in bonds {
        doc.add_bond(ids[a], ids[b], order, "plain");
    }
    ids
}

fn arrow(doc: &mut Document, start: Point, end: Point) -> u64 {
    let id = doc.next_id();
    doc.arrows.push(Arrow::new(
        id,
        start,
        end,
        Default::default(),
        Default::default(),
    ));
    id
}

fn model(doc: &mut Document, atoms: &[u64]) -> usize {
    let geometry = atoms
        .iter()
        .map(|&id| {
            let p = doc.atom(id).unwrap().position;
            Placed {
                id,
                xyz: [p.x / 30., p.y / 30., 0.],
            }
        })
        .collect();
    let mut model = Model3d::new(doc, geometry, vec![], ForceField::default());
    model.size = 100.;
    let index = doc.models.len();
    doc.models.push(model);
    index
}

fn claisen() -> Scheme {
    let mut doc = Document::default();
    let reactant = molecule(
        &mut doc,
        &[
            ("C", 0., 0.),
            ("C", 30., -18.),
            ("O", 60., 0.),
            ("C", 90., -18.),
            ("C", 120., 0.),
            ("C", 150., -18.),
        ],
        &[(0, 1, 2), (1, 2, 1), (2, 3, 1), (3, 4, 1), (4, 5, 2)],
    );
    let product = molecule(
        &mut doc,
        &[
            ("O", 255., 0.),
            ("C", 285., -18.),
            ("C", 315., 0.),
            ("C", 345., -18.),
            ("C", 375., 0.),
            ("C", 405., -18.),
        ],
        &[(0, 1, 2), (1, 2, 1), (2, 3, 1), (3, 4, 1), (4, 5, 2)],
    );
    let arrow = arrow(&mut doc, Point::new(175., 0.), Point::new(230., 0.));
    assign(&mut doc, arrow, &reactant, Role::Reactant).unwrap();
    assign(&mut doc, arrow, &product, Role::Product).unwrap();
    Scheme {
        doc,
        species: vec![reactant, product],
        arrow,
        plus: None,
    }
}

fn diels_alder() -> Scheme {
    let mut doc = Document::default();
    let diene = molecule(
        &mut doc,
        &[
            ("C", 0., 0.),
            ("C", 24., -18.),
            ("C", 48., 0.),
            ("C", 72., -18.),
        ],
        &[(0, 1, 2), (1, 2, 1), (2, 3, 2)],
    );
    let alkene = molecule(&mut doc, &[("C", 110., 8.), ("C", 146., 30.)], &[(0, 1, 2)]);
    let product = molecule(
        &mut doc,
        &[
            ("C", 330., 0.),
            ("C", 330., 36.),
            ("C", 361., 54.),
            ("C", 392., 36.),
            ("C", 392., 0.),
            ("C", 361., -18.),
        ],
        &[
            (0, 1, 2),
            (1, 2, 1),
            (2, 3, 1),
            (3, 4, 1),
            (4, 5, 1),
            (5, 0, 1),
        ],
    );
    let agent = molecule(
        &mut doc,
        &[
            ("B", 230., -120.),
            ("Cl", 200., -138.),
            ("Cl", 260., -138.),
            ("Cl", 230., -84.),
        ],
        &[(0, 1, 1), (0, 2, 1), (0, 3, 1)],
    );
    let arrow = arrow(&mut doc, Point::new(190., 0.), Point::new(270., 0.));
    let plus = doc.next_id();
    doc.annotations.push(Annotation {
        id: plus,
        position: Point::new(86., -5.),
        text: "+".into(),
        format: Default::default(),
        angle: 0.,
    });
    assign(&mut doc, arrow, &diene, Role::Reactant).unwrap();
    assign(&mut doc, arrow, &alkene, Role::Reactant).unwrap();
    assign(&mut doc, arrow, &product, Role::Product).unwrap();
    assign(&mut doc, arrow, &agent, Role::Agent).unwrap();
    Scheme {
        doc,
        species: vec![diene, alkene, product, agent],
        arrow,
        plus: Some(plus),
    }
}

fn overlaps(a: InkBox, b: InkBox) -> bool {
    a.0.x < b.1.x && a.1.x > b.0.x && a.0.y < b.1.y && a.1.y > b.0.y
}

fn scheme_ink(scheme: &Scheme) -> Vec<InkBox> {
    let mut selections = scheme.species.clone();
    selections.push(vec![scheme.arrow]);
    if let Some(plus) = scheme.plus {
        selections.push(vec![plus]);
    }
    scene::selections_ink_bounds(&scheme.doc, &selections)
        .into_iter()
        .flatten()
        .collect()
}

fn assert_row(scheme: &Scheme, expected: &[InkBox], top: f32) {
    let frames: Vec<_> = scheme
        .doc
        .models
        .iter()
        .map(|model| {
            assert_eq!(model.display, Display::Both);
            frame(&scheme.doc, model).unwrap()
        })
        .collect();
    assert_eq!(frames.len(), expected.len());
    for (i, (&(lo, hi), &(a, b))) in frames.iter().zip(expected).enumerate() {
        assert!(
            ((lo.x + hi.x - a.x - b.x) / 2.).abs() < 1e-3,
            "{lo:?} {hi:?} {a:?} {b:?}"
        );
        assert!((lo.y - top).abs() < 1e-3, "{lo:?} expected top {top}");
        for bounds in &frames[..i] {
            assert!(!overlaps((lo, hi), *bounds));
        }
    }
}

fn annotation(doc: &mut Document, text: &str, x: f32, y: f32) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: Point::new(x, y),
        text: text.into(),
        format: Default::default(),
        angle: 0.,
    });
    id
}

fn check_reaction_row(mut scheme: Scheme, reverse: bool) {
    if reverse {
        let r = &mut scheme.doc.reactions[0];
        std::mem::swap(&mut r.reactants, &mut r.products);
        let arrow = &mut scheme.doc.arrows[0];
        std::mem::swap(&mut arrow.start, &mut arrow.end);
    }
    let reagent = annotation(&mut scheme.doc, "Catalyst", 204., -50.);
    let condition = annotation(&mut scheme.doc, "Conditions", 199., 84.);
    for (annotation, side) in [
        (reagent, crate::arrow_labels::Side::Above),
        (condition, crate::arrow_labels::Side::Below),
    ] {
        scheme
            .doc
            .arrow_labels
            .push(crate::arrow_labels::ArrowLabel {
                arrow: scheme.arrow,
                annotation,
                side,
                gap: None,
            });
    }
    let ink = scheme_ink(&scheme);
    let arrow = scheme.doc.arrows[0].bounds();
    let bottom = ink
        .iter()
        .map(|(_, hi)| hi.y)
        .chain(scheme.doc.annotations.iter().map(|a| a.bounds().1.y))
        .reduce(f32::max)
        .unwrap();
    // Keep the reagent drawing in the scheme and give the path a separate model.
    let count = scheme.species.len().min(3);
    for atoms in &scheme.species[..count] {
        let index = model(&mut scheme.doc, atoms);
        scheme.doc.models[index].display = Display::ThreeD;
        scheme.doc.models[index].offset = Point::new(300., 50.);
    }
    scheme.doc.models[0].size = 1000.;
    let source = if count == 2 { 0 } else { 2 };
    let path = model(&mut scheme.doc, &scheme.species[source]);
    let expected: Vec<_> = ink[..count].iter().copied().chain([arrow]).collect();
    let bond = scheme.doc.drawing_style.bond_length_world;

    place_reaction_row(&mut scheme.doc, scheme.arrow, Some(path));
    assert_row(&scheme, &expected, bottom + bond);
    assert!(scheme.doc.models[0].size < 1000.);
    let placed = scheme.doc.clone();
    place_reaction_row(&mut scheme.doc, scheme.arrow, Some(path));
    assert_eq!(scheme.doc, placed);
}

#[test]
fn claisen_models_and_ts_share_one_row() {
    check_reaction_row(claisen(), false);
}

#[test]
fn diels_alder_models_and_ts_share_one_row() {
    let mut scheme = diels_alder();
    // A tracked participant can contain multiple disconnected species.
    let reactants = &mut scheme.doc.reactions[0].reactants;
    let alkene = reactants.pop().unwrap();
    reactants[0].atoms.extend(alkene.atoms);
    check_reaction_row(scheme, false);
}

#[test]
fn one_to_two_models_and_ts_share_one_row() {
    check_reaction_row(diels_alder(), true);
}

#[test]
fn untracked_plus_and_other_drawing_objects_move_the_whole_row_down() {
    let mut scheme = diels_alder();
    // The untracked plus is lower than the reactants, products, and arrow.
    let plus = scheme
        .doc
        .annotations
        .iter_mut()
        .find(|a| Some(a.id) == scheme.plus)
        .unwrap();
    plus.position.y = 59.;
    let bottoms = scheme_ink(&scheme);
    let bottom = bottoms.last().unwrap().1.y;
    assert!(
        bottom
            > bottoms[..bottoms.len() - 1]
                .iter()
                .map(|(_, hi)| hi.y)
                .reduce(f32::max)
                .unwrap()
    );
    let bond = scheme.doc.drawing_style.bond_length_world;
    for atoms in &scheme.species[..3] {
        model(&mut scheme.doc, atoms);
    }
    place_reaction_row(&mut scheme.doc, scheme.arrow, None);
    assert_row(&scheme, &bottoms[..3], bottom + bond);

    let block = annotation(
        &mut scheme.doc,
        "Outside the scheme",
        0.,
        bottom + bond + 12.,
    );
    let lo = scene::selection_bounds(&scheme.doc, &[block]).unwrap().1;
    place_reaction_row(&mut scheme.doc, scheme.arrow, None);
    assert_row(&scheme, &bottoms[..3], lo.y + bond);
}

#[test]
fn unlinked_condition_below_the_arrow_counts_without_a_path_model() {
    let mut scheme = claisen();
    let condition = annotation(&mut scheme.doc, "Conditions", 180., 36.);
    let top = scene::selection_bounds(&scheme.doc, &[condition])
        .unwrap()
        .1
        .y
        + scheme.doc.drawing_style.bond_length_world;
    let expected = scheme_ink(&scheme);
    for atoms in &scheme.species {
        model(&mut scheme.doc, atoms);
    }
    place_reaction_row(&mut scheme.doc, scheme.arrow, None);
    assert_row(&scheme, &expected[..2], top);
}

#[test]
fn centered_catalyst_model_stays_put_and_does_not_shrink_the_ts() {
    let mut scheme = diels_alder();
    for atoms in &scheme.species[..3] {
        model(&mut scheme.doc, atoms);
    }
    let catalyst = model(&mut scheme.doc, &scheme.species[3]);
    scheme.doc.models[catalyst].display = Display::Both;
    let (lo, hi) = frame(&scheme.doc, &scheme.doc.models[catalyst]).unwrap();
    let (arrow_lo, arrow_hi) = scheme.doc.arrows[0].bounds();
    let center = (arrow_lo.x + arrow_hi.x) / 2.;
    scheme.doc.models[catalyst].offset = Point::new(center - (lo.x + hi.x) / 2., 60. - lo.y);
    let saved = scheme.doc.models[catalyst].clone();
    let catalyst_frame = frame_with_panels(&scheme.doc, &saved).unwrap();
    let path = model(&mut scheme.doc, &scheme.species[2]);

    place_reaction_row(&mut scheme.doc, scheme.arrow, Some(path));

    assert_eq!(scheme.doc.models[catalyst], saved);
    assert!(scheme.doc.models[path].size > 10.);
    let path_frame = frame(&scheme.doc, &scheme.doc.models[path]).unwrap();
    assert!(
        ((path_frame.0.x + path_frame.1.x) / 2. - center).abs() < 1e-3,
        "path frame {path_frame:?} should be centered at {center}"
    );
    for index in [0, 1, 2, path] {
        let bounds = frame(&scheme.doc, &scheme.doc.models[index]).unwrap();
        assert!((bounds.0.y - path_frame.0.y).abs() < 1e-3);
        assert!(!overlaps(bounds, catalyst_frame));
    }
}

#[test]
fn stale_catalyst_fragment_models_remain_unchanged() {
    let mut scheme = diels_alder();
    for atoms in &scheme.species[..3] {
        model(&mut scheme.doc, atoms);
    }
    // These are the old source links left by joining two modeled fragments.
    let a = model(&mut scheme.doc, &scheme.species[3][..2]);
    let b = model(&mut scheme.doc, &scheme.species[3][2..]);
    scheme.doc.models[a].display = Display::Both;
    scheme.doc.models[a].offset = Point::new(-500., 300.);
    scheme.doc.models[b].display = Display::ThreeD;
    scheme.doc.models[b].offset = Point::new(500., 600.);
    let saved = [scheme.doc.models[a].clone(), scheme.doc.models[b].clone()];

    place_reaction_row(&mut scheme.doc, scheme.arrow, None);

    assert_eq!(scheme.doc.models[a], saved[0]);
    assert_eq!(scheme.doc.models[b], saved[1]);
    let top = frame(&scheme.doc, &scheme.doc.models[0]).unwrap().0.y;
    for index in 0..3 {
        assert_eq!(scheme.doc.models[index].display, Display::Both);
        assert!((frame(&scheme.doc, &scheme.doc.models[index]).unwrap().0.y - top).abs() < 1e-3);
    }
}

#[test]
fn a_stale_endpoint_fragment_cannot_replace_the_complete_species_model() {
    let mut scheme = claisen();
    let ink = scheme_ink(&scheme);
    let bottom = ink.iter().map(|(_, hi)| hi.y).reduce(f32::max).unwrap();
    let stale = model(&mut scheme.doc, &scheme.species[0][..3]);
    scheme.doc.models[stale].display = Display::ThreeD;
    scheme.doc.models[stale].offset = Point::new(-500., 500.);
    let saved = scheme.doc.models[stale].clone();
    let reactant = model(&mut scheme.doc, &scheme.species[0]);
    let product = model(&mut scheme.doc, &scheme.species[1]);

    place_reaction_row(&mut scheme.doc, scheme.arrow, None);

    assert_eq!(scheme.doc.models[stale], saved);
    for (index, &(lo, hi)) in [reactant, product].into_iter().zip(&ink) {
        let placed = &scheme.doc.models[index];
        assert_eq!(placed.display, Display::Both);
        let bounds = frame(&scheme.doc, placed).unwrap();
        assert!(((bounds.0.x + bounds.1.x - lo.x - hi.x) / 2.).abs() < 1e-3);
        assert!((bounds.0.y - bottom - scheme.doc.drawing_style.bond_length_world).abs() < 1e-3);
    }
}

fn check_connected_reaction_rows(count: usize, inferred: bool) {
    let mut doc = Document::default();
    let species: Vec<_> = (0..count)
        .map(|index| {
            let x = 200. * index as f32;
            molecule(&mut doc, &[("C", x, 0.), ("O", x + 30., 0.)], &[(0, 1, 1)])
        })
        .collect();
    let arrows: Vec<_> = (0..count - 1)
        .map(|index| {
            let x = 200. * index as f32;
            let arrow = arrow(&mut doc, Point::new(x + 80., 0.), Point::new(x + 150., 0.));
            if !inferred {
                assign(&mut doc, arrow, &species[index], Role::Reactant).unwrap();
                assign(&mut doc, arrow, &species[index + 1], Role::Product).unwrap();
            }
            arrow
        })
        .collect();
    let condition = annotation(
        &mut doc,
        "Lowest condition",
        200. * (count - 2) as f32 + 80.,
        150.,
    );
    doc.arrow_labels.push(crate::arrow_labels::ArrowLabel {
        arrow: *arrows.last().unwrap(),
        annotation: condition,
        side: crate::arrow_labels::Side::Below,
        gap: None,
    });
    let top = scene::selection_bounds(&doc, &[condition]).unwrap().1.y
        + doc.drawing_style.bond_length_world;
    let expected: Vec<_> = scene::selections_ink_bounds(&doc, &species)
        .into_iter()
        .flatten()
        .collect();
    for atoms in &species {
        model(&mut doc, atoms);
    }
    let mut scheme = Scheme {
        doc,
        species,
        arrow: arrows[0],
        plus: None,
    };

    place_reaction_row(&mut scheme.doc, arrows[0], None);
    assert_row(&scheme, &expected, top);
    let placed = scheme.doc.clone();
    for arrow in arrows.into_iter().rev() {
        place_reaction_row(&mut scheme.doc, arrow, None);
        assert_eq!(scheme.doc, placed);
    }
}

#[test]
fn shared_intermediate_keeps_both_arrows_below_their_combined_annotations() {
    check_connected_reaction_rows(3, false);
}

#[test]
fn shared_species_connect_the_reaction_row_transitively() {
    check_connected_reaction_rows(4, false);
}

#[test]
fn inferred_arrows_share_the_connected_reaction_row() {
    check_connected_reaction_rows(4, true);
}

#[test]
fn standalone_shared_placement_stays_right_when_clear_and_below_when_blocked() {
    let mut doc = Document::default();
    let atoms = molecule(&mut doc, &[("C", 0., 0.), ("O", 30., 0.)], &[(0, 1, 1)]);
    let index = model(&mut doc, &atoms);
    let (_, hi) = scene::selection_bounds(&doc, &atoms).unwrap();
    let bond = doc.drawing_style.bond_length_world;
    assert!(set_display(&mut doc, index, Display::Both));
    let (lo, _) = frame(&doc, &doc.models[index]).unwrap();
    assert!((lo.x - hi.x - bond).abs() < 1e-3);
    annotation(&mut doc, "Obstacle", lo.x + 3., 0.);
    assert!(set_display(&mut doc, index, Display::Both));
    let (lo, _) = frame(&doc, &doc.models[index]).unwrap();
    assert!((lo.y - hi.y - bond).abs() < 1e-3);
}

#[test]
fn active_layout_keeps_the_scheme_in_place_and_the_row_shares_undo() {
    let mut scheme = claisen();
    let ids = scheme.doc.all_ids();
    crate::layouts::arrange(&mut scheme.doc, &ids, crate::layouts::Flow::Row, 12.).unwrap();
    let before = scheme.doc.clone();
    let ink = scheme_ink(&scheme);
    for atoms in &scheme.species {
        model(&mut scheme.doc, atoms);
    }
    scheme.doc.models[0].size = 60.;
    let edit = crate::transaction::reconcile_with_reaction_row(
        &mut scheme.doc,
        before.clone(),
        scheme.arrow,
    )
    .unwrap();
    assert_eq!(scheme_ink(&scheme), ink);
    let bottom = ink.iter().map(|(_, hi)| hi.y).reduce(f32::max).unwrap();
    assert_row(
        &scheme,
        &ink[..2],
        bottom + scheme.doc.drawing_style.bond_length_world,
    );

    let mut history = crate::document::History::default();
    assert!(edit.commit(&mut scheme.doc, &mut history, false).recorded);
    assert!(history.undo(&mut scheme.doc));
    assert_eq!(scheme.doc, before);
    assert!(!history.can_undo());
}

#[test]
fn reaction_row_clears_a_zero_order_bond_between_distant_molecules() {
    let mut scheme = claisen();
    for atoms in &scheme.species {
        model(&mut scheme.doc, atoms);
    }
    place_reaction_row(&mut scheme.doc, scheme.arrow, None);
    let (lo, hi) = frame(&scheme.doc, &scheme.doc.models[0]).unwrap();
    let y = (lo.y + hi.y) / 2.;
    let a = scheme.doc.add_atom("O", Point::new(-300., y));
    let b = scheme.doc.add_atom("N", Point::new(700., y));
    scheme.doc.add_bond(a, b, 0, "dotted");
    let endpoints = scene::selections_ink_bounds(&scheme.doc, &[vec![a], vec![b]]);
    let pad = scheme.doc.drawing_style.line_width() / 2.;
    let ink = (Point::new(-300., y - pad), Point::new(700., y + pad));
    for model in &scheme.doc.models {
        let bounds = frame(&scheme.doc, model).unwrap();
        assert!(overlaps(bounds, ink));
        assert!(
            endpoints
                .iter()
                .flatten()
                .all(|endpoint| !overlaps(bounds, *endpoint))
        );
    }

    place_reaction_row(&mut scheme.doc, scheme.arrow, None);

    let expected_top = ink.1.y + scheme.doc.drawing_style.bond_length_world;
    for model in &scheme.doc.models {
        let bounds = frame(&scheme.doc, model).unwrap();
        assert!((bounds.0.y - expected_top).abs() < 1e-3);
        assert!(!overlaps(bounds, ink));
    }
}

#[test]
fn plus_signs_follow_arrow_ownership_and_species_gaps() {
    let mut doc = Document::default();
    let left = molecule(&mut doc, &[("C", 0., 0.), ("O", 30., 0.)], &[(0, 1, 1)]);
    let right = molecule(&mut doc, &[("C", 300., 0.), ("O", 330., 0.)], &[(0, 1, 1)]);
    let product = molecule(&mut doc, &[("C", 600., 0.), ("O", 630., 0.)], &[(0, 1, 1)]);
    let target = arrow(&mut doc, Point::new(400., 0.), Point::new(500., 0.));
    assign(&mut doc, target, &left, Role::Reactant).unwrap();
    assign(&mut doc, target, &right, Role::Reactant).unwrap();
    assign(&mut doc, target, &product, Role::Product).unwrap();
    let other_left = molecule(&mut doc, &[("C", 900., 0.), ("O", 930., 0.)], &[(0, 1, 1)]);
    let other_right = molecule(
        &mut doc,
        &[("C", 1200., 0.), ("O", 1230., 0.)],
        &[(0, 1, 1)],
    );
    let other = arrow(&mut doc, Point::new(1000., 0.), Point::new(1100., 0.));
    assign(&mut doc, other, &other_left, Role::Reactant).unwrap();
    assign(&mut doc, other, &other_right, Role::Product).unwrap();
    for atoms in [&left, &right, &product] {
        model(&mut doc, atoms);
    }
    place_reaction_row(&mut doc, target, None);
    let baseline = doc.models.clone();
    let plus = annotation(&mut doc, "+", 150., 24.);
    let plus_bounds = scene::selection_bounds(&doc, &[plus]).unwrap();
    assert!(
        doc.models
            .iter()
            .all(|model| !overlaps(frame(&doc, model).unwrap(), plus_bounds))
    );
    doc.arrow_labels.push(crate::arrow_labels::ArrowLabel {
        arrow: other,
        annotation: plus,
        side: crate::arrow_labels::Side::Below,
        gap: None,
    });

    place_reaction_row(&mut doc, target, None);
    assert_eq!(doc.models, baseline);

    let reaction = doc
        .reactions
        .iter_mut()
        .find(|r| r.arrow == target)
        .unwrap();
    reaction.annotations.push(plus);
    place_reaction_row(&mut doc, target, None);
    assert_eq!(doc.models, baseline);
    doc.reactions
        .iter_mut()
        .find(|r| r.arrow == target)
        .unwrap()
        .annotations
        .clear();

    doc.arrow_labels[0].arrow = target;
    place_reaction_row(&mut doc, target, None);
    let expected_top = plus_bounds.1.y + doc.drawing_style.bond_length_world;
    for model in &doc.models {
        assert!((frame(&doc, model).unwrap().0.y - expected_top).abs() < 1e-3);
    }

    doc.arrow_labels.clear();
    place_reaction_row(&mut doc, target, None);
    for model in &doc.models {
        assert!((frame(&doc, model).unwrap().0.y - expected_top).abs() < 1e-3);
    }

    // A loose plus below one species is outside the gap even though it is
    // inside the reactants' combined bounds. Keep it clear of the model row.
    let bond = doc.drawing_style.bond_length_world;
    let baseline_top = frame(&doc, &baseline[0]).unwrap().0.y;
    let dx = 15. - (plus_bounds.0.x + plus_bounds.1.x) / 2.;
    let dy = baseline_top - bond / 2. - plus_bounds.1.y;
    let text = doc.annotations.iter_mut().find(|a| a.id == plus).unwrap();
    text.position = text.position.offset(dx, dy);
    let below_species = scene::selection_bounds(&doc, &[plus]).unwrap();
    assert!(below_species.1.y > baseline_top - bond && below_species.1.y < baseline_top);
    place_reaction_row(&mut doc, target, None);
    assert_eq!(doc.models, baseline);

    let text = doc.annotations.iter_mut().find(|a| a.id == plus).unwrap();
    text.position.x += 150.;
    place_reaction_row(&mut doc, target, None);
    let expected_top = below_species.1.y + bond;
    for model in &doc.models {
        assert!((frame(&doc, model).unwrap().0.y - expected_top).abs() < 1e-3);
    }
}
