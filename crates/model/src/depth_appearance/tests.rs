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
