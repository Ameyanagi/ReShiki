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
