use super::*;
use crate::document::Point;

fn scope() -> Scope {
    Scope {
        atoms: vec![11, 22, 33],
        automatic: false,
        strength: 0.6,
        rear_opacity: 0.4,
        rear_overrides: BTreeMap::from([(11, 0.1), (22, 0.2), (33, 0.3)]),
        rear_weights: BTreeMap::from([(11, 0.4), (22, 0.5), (33, 0.6)]),
        weights: BTreeMap::from([(11, 0.7), (22, 0.8), (33, 0.9)]),
        overrides: BTreeMap::from([(11, 0.), (22, 0.5), (33, 1.)]),
    }
}

#[test]
fn retain_members_filters_atoms_and_every_map() {
    let mut scope = scope();
    let expected = Scope {
        atoms: vec![11, 33],
        rear_overrides: BTreeMap::from([(11, 0.1), (33, 0.3)]),
        rear_weights: BTreeMap::from([(11, 0.4), (33, 0.6)]),
        weights: BTreeMap::from([(11, 0.7), (33, 0.9)]),
        overrides: BTreeMap::from([(11, 0.), (33, 1.)]),
        ..scope.clone()
    };
    scope.retain_members(|id| id != 22);
    assert_eq!(scope, expected);
}

#[test]
fn remapped_maps_every_map_and_drops_empty_scopes() {
    let scope = scope();
    let mapping = HashMap::from([(11, 220), (33, 110)]);
    let unmapped = Scope {
        atoms: vec![44],
        ..scope.clone()
    };
    assert_eq!(
        remap(&[scope.clone(), unmapped], &mapping),
        vec![Scope {
            atoms: vec![220, 110],
            rear_overrides: BTreeMap::from([(220, 0.1), (110, 0.3)]),
            rear_weights: BTreeMap::from([(220, 0.4), (110, 0.6)]),
            weights: BTreeMap::from([(220, 0.7), (110, 0.9)]),
            overrides: BTreeMap::from([(220, 0.), (110, 1.)]),
            ..scope
        }]
    );
}

#[test]
fn clear_rear_opacity_resets_alpha_fields_only() {
    let mut scope = scope();
    let expected = Scope {
        rear_opacity: 1.,
        rear_overrides: BTreeMap::new(),
        rear_weights: BTreeMap::new(),
        ..scope.clone()
    };
    scope.clear_rear_opacity();
    assert_eq!(scope, expected);
}

#[test]
fn alpha_only_scope_validates_without_rgb_fade() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("O", Point::new(40., 0.));
    doc.add_bond(a, b, 1, "plain");
    let scope = Scope::alpha_only(&doc, vec![a, b], 0.4);
    assert!(!scope.has_rgb_fade());
    doc.depth_appearance.push(scope);
    assert_eq!(validate(&doc), Ok(()));
}

#[test]
fn covalent_component_ids_returns_sorted_whole_molecules() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(0., 40.));
    let c = doc.add_atom("O", Point::new(40., 0.));
    let d = doc.add_atom("O", Point::new(40., 40.));
    let attached = doc.add_atom("Fe", Point::new(80., 0.));
    doc.add_bond(a, c, 1, "plain");
    doc.add_bond(b, d, 1, "plain");
    doc.add_bond(c, attached, 5, "plain");
    assert!(
        doc.bonds
            .iter()
            .any(|bond| bond.connects(c, attached) && !bond.is_covalent())
    );
    assert_eq!(covalent_component_ids(&doc, &[d, c]), vec![a, b, c, d]);
}

fn general_component_ids(doc: &Document, seeds: &[u64]) -> Vec<u64> {
    let mut ids: Vec<_> = covalent_groups(doc, seeds).into_iter().flatten().collect();
    ids.sort_unstable();
    ids
}

fn seed_drawing() -> (Document, [u64; 6], u64, u64) {
    use crate::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle};
    let mut doc = Document::default();
    let real = ["C", "O", "N", "Fe", "Ne", "*"]
        .map(|element| doc.add_atom(element, Point::new(doc.atoms.len() as f32 * 42., 0.)));
    let [a, b, c, metal, _, _] = real;
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 5, "plain");
    doc.add_bond(c, metal, 1, "plain");
    let centroid = doc.add_atom("*", Point::new(21., 0.));
    doc.atom_mut(centroid).unwrap().centroid = vec![a, b];
    doc.add_bond(centroid, metal, 1, "plain");
    let graphic = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        graphic,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(42., 42.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    doc.atoms.rotate_left(3);
    (doc, real, centroid, graphic)
}

#[test]
fn complete_and_partial_seeds_match_general_components_with_nonatom_extras() {
    let empty = Document::default();
    for seeds in [vec![], vec![u64::MAX, u64::MAX]] {
        assert_eq!(covalent_component_ids(&empty, &seeds), Vec::<u64>::new());
    }
    let (mut doc, real, centroid, graphic) = seed_drawing();
    for mask in 0..1 << real.len() {
        let mut seeds: Vec<_> = real
            .iter()
            .enumerate()
            .filter_map(|(i, id)| (mask & (1 << i) != 0).then_some(*id))
            .collect();
        // Length alone cannot establish completeness: extras and repeated
        // seeds can outnumber the real atoms even for a partial selection.
        seeds.extend([centroid, graphic, u64::MAX, graphic, centroid]);
        if let Some(first) = seeds.first().copied() {
            seeds.extend([first; 8]);
        }
        let actual = covalent_component_ids(&doc, &seeds);
        assert_eq!(actual, general_component_ids(&doc, &seeds), "subset {mask}");
        assert!(actual.windows(2).all(|ids| ids[0] < ids[1]));
    }
    let [a, b, c, metal, isolated, dummy] = real;
    assert_eq!(covalent_component_ids(&doc, &[a]), [a, b]);
    assert_eq!(covalent_component_ids(&doc, &[c]), [c, metal]);
    assert!(covalent_component_ids(&doc, &[centroid, graphic]).is_empty());

    doc.contract(&[a, b], "Et", "Et").unwrap();
    let anchor = doc.abbreviations[0].anchor;
    let seeds =
        doc.expand_abbreviation_selection(&[anchor, c, metal, isolated, dummy, centroid, graphic]);
    assert_eq!(covalent_component_ids(&doc, &seeds), real);
    assert_eq!(
        covalent_component_ids(&doc, &seeds),
        general_component_ids(&doc, &seeds)
    );
    // The general path also deduplicates intermediate duplicate atom records.
    doc.atoms.push(doc.atom(a).unwrap().clone());
    assert_eq!(covalent_component_ids(&doc, &seeds), real);
    assert_eq!(
        covalent_component_ids(&doc, &seeds),
        general_component_ids(&doc, &seeds)
    );
}

#[test]
fn complete_seed_detection_tracks_consecutive_atom_bond_and_centroid_edits() {
    let (mut doc, real, centroid, graphic) = seed_drawing();
    let mut seeds = real.to_vec();
    seeds.extend([centroid, graphic, u64::MAX]);
    let check = |doc: &Document, seeds: &[u64]| {
        let actual = covalent_component_ids(doc, seeds);
        assert_eq!(actual, general_component_ids(doc, seeds));
        actual
    };
    assert_eq!(check(&doc, &seeds), real);
    let added = doc.add_atom("S", Point::new(400., 0.));
    assert_eq!(check(&doc, &seeds), real);
    doc.add_bond(real[3], added, 1, "plain");
    assert!(check(&doc, &seeds).contains(&added));
    doc.bonds
        .iter_mut()
        .find(|bond| bond.connects(real[3], added))
        .unwrap()
        .order = 5;
    assert_eq!(check(&doc, &seeds), real);
    seeds.push(added);
    assert!(check(&doc, &seeds).contains(&added));
    doc.atom_mut(added).unwrap().centroid = vec![real[0], real[1]];
    assert_eq!(check(&doc, &seeds), real);
    doc.atom_mut(added).unwrap().centroid.clear();
    assert!(check(&doc, &seeds).contains(&added));
    doc.atoms.retain(|atom| atom.id != added);
    doc.bonds.retain(|bond| bond.a != added && bond.b != added);
    assert_eq!(check(&doc, &seeds), real);
}
