use super::*;
use crate::generic::drawing_test_support::{
    contained, formula, legible, neighbors, preview, same_except_positions,
    union_actual_ink_contained,
};
use crate::generic::{
    budget::Presentation,
    ink::{atom_bounds, measured_bounds, normalize_arrows, saved_and_refreshed_boxes},
    probe::{self, Stage},
    references::protect_references,
};
use crate::{
    atom_text::{self, Mode},
    generic::{self, Limits},
};
use std::collections::HashSet;

#[test]
fn native_variable_arene_unique_drawing_fits_default_work_and_measured_ink() {
    // Exact ROOT native save of the failed source, not a manufactured CDXML
    // roundtrip: a129c109a464b9a1414cce4d09aded6ac6290848e0cfec84c92f67dd520f7be2.
    let source = Document::from_native_file(include_bytes!(
        "../../../../../tests/fixtures/generic-structures/variable-arene-native-refused-v26.rsk"
    ))
    .unwrap();
    assert_eq!((source.atoms.len(), source.bonds.len()), (9, 8));
    let variable = source.atom(7).unwrap();
    assert_eq!(
        variable.attachment,
        Some(crate::attachments::Kind::Variable)
    );
    assert_eq!(variable.centroid, vec![1, 2, 3, 4, 5, 6]);
    assert!(
        source
            .atoms
            .iter()
            .all(|a| a.display.stereo.show == Some(false))
    );
    let before = source.clone();
    let solver_calls = probe::count(Stage::Solver);
    let input = generic::prepare_expansion_input(&source, &[], Limits::default()).unwrap();
    let prepared = generic::expand_prepared(input).unwrap();
    assert_eq!(probe::count(Stage::Solver), solver_calls);
    // Six equivalent raw sites yield one stable first chemical representative.
    // Direct pose coverage below still exercises every one of the six sites.
    assert_eq!(prepared.count(), 1);
    let bounds = prepared.bounds();
    let output = prepared.take().unwrap();
    assert_eq!((output.atoms.len(), output.bonds.len()), (8, 8));
    assert_eq!(output.generic, Default::default());
    assert!(
        output
            .atoms
            .iter()
            .all(|a| a.attachment.is_none() && a.centroid.is_empty())
    );
    let mut tile_bounds = Vec::new();
    for (choice, atoms) in output.atoms.as_chunks::<8>().0.iter().enumerate() {
        let ids: HashSet<_> = atoms.iter().map(|a| a.id).collect();
        let part = Document {
            atoms: atoms.to_vec(),
            bonds: output
                .bonds
                .iter()
                .filter(|b| ids.contains(&b.a) && ids.contains(&b.b))
                .cloned()
                .collect(),
            ..Default::default()
        };
        assert_eq!(part.bonds.len(), 8);
        assert_eq!(formula(&part), "C7H8O");
        assert!(part.atoms.iter().all(|a| a.isotope == 0 && a.charge == 0));
        let oxygen = part.atoms.get(6).unwrap();
        let methyl = part.atoms.get(7).unwrap();
        let selected = part
            .atoms
            .iter()
            .take(6)
            .find(|a| neighbors(&part, oxygen.id).contains(&a.id))
            .unwrap();
        assert_eq!(selected.id, part.atoms[0].id);
        assert_eq!(oxygen.element, "O");
        let attached: HashSet<_> = neighbors(&part, oxygen.id).into_iter().collect();
        assert_eq!(attached, HashSet::from([selected.id, methyl.id]));
        let shift = Point::new(
            part.atoms[0].position.x - source.atoms[0].position.x,
            part.atoms[0].position.y - source.atoms[0].position.y,
        );
        for (actual, original) in part.atoms.iter().take(6).zip(&source.atoms) {
            assert!(
                actual
                    .position
                    .distance(original.position.offset(shift.x, shift.y))
                    < 0.001
            );
        }
        assert_methoxy_angle(selected.position, oxygen.position, methyl.position);
        let connecting = selected.position.distance(oxygen.position);
        assert!((connecting - source.drawing_style.bond_length_world).abs() < 0.001);
        assert!(
            (oxygen.position.distance(methyl.position)
                - source
                    .atom(8)
                    .unwrap()
                    .position
                    .distance(source.atom(9).unwrap().position))
            .abs()
                < 0.001
        );
        let ring_center = source.atom(7).unwrap().position.offset(shift.x, shift.y);
        let outward = Point::new(
            selected.position.x - ring_center.x,
            selected.position.y - ring_center.y,
        );
        for branch in [oxygen, methyl] {
            assert!(
                (branch.position.x - selected.position.x) * outward.x
                    + (branch.position.y - selected.position.y) * outward.y
                    > 0.
            );
        }
        for (i, a) in part.atoms.iter().take(6).enumerate() {
            let b = part.atoms.get((i + 1) % 6).unwrap();
            let bond = part
                .bonds
                .iter()
                .find(|bond| {
                    (bond.a == a.id && bond.b == b.id) || (bond.a == b.id && bond.b == a.id)
                })
                .unwrap();
            assert_eq!(bond.order, if i % 2 == 0 { 2 } else { 1 });
            if a.id != selected.id && b.id != selected.id {
                assert!(
                    !proper_crossing(selected.position, oxygen.position, a.position, b.position),
                    "choice {choice} cuts across nonincident ring edge {}-{}",
                    a.id,
                    b.id
                );
            }
            assert!(!proper_crossing(
                oxygen.position,
                methyl.position,
                a.position,
                b.position
            ));
        }
        let shown = preview(&part);
        let boxes = saved_and_refreshed_boxes(&part, &shown).unwrap();
        let tile = measured_bounds(&part, &boxes).unwrap();
        contained(bounds, tile);
        tile_bounds.push(tile);
        legible(&part);
    }
    for (i, (lo, hi)) in tile_bounds.iter().enumerate() {
        for (zl, zh) in tile_bounds.iter().skip(i + 1) {
            assert!(hi.x <= zl.x || zh.x <= lo.x || hi.y <= zl.y || zh.y <= lo.y);
        }
    }
    union_actual_ink_contained(&output, bounds);
    let reopened = Document::from_native_file(&output.file_json().unwrap()).unwrap();
    assert_eq!(reopened, output.current());
    union_actual_ink_contained(&reopened, bounds);
    assert_eq!(source, before);
}

fn proper_crossing(a: Point, b: Point, c: Point, d: Point) -> bool {
    let delta = |p: Point, q: Point| {
        (
            f64::from(p.x) - f64::from(q.x),
            f64::from(p.y) - f64::from(q.y),
        )
    };
    let cross = |u: (f64, f64), v: (f64, f64)| u.0 * v.1 - u.1 * v.0;
    let u = delta(b, a);
    let v = delta(d, c);
    let denominator = cross(u, v);
    if denominator.abs() < 0.000001 {
        return false;
    }
    let t = cross(delta(c, a), v) / denominator;
    let z = cross(delta(c, a), u) / denominator;
    t > 0.000001 && t < 0.999999 && z > 0.000001 && z < 0.999999
}

fn variable_arene_source() -> Document {
    Document::from_native_file(include_bytes!(
        "../../../../../tests/fixtures/generic-structures/variable-arene-native-refused-v26.rsk"
    ))
    .unwrap()
}

#[test]
fn variable_pose_is_local_rigid_and_preserves_unrelated_native_associations() {
    use crate::arrow_anchors::{Anchor, Target};
    let mut source = variable_arene_source();
    source.atom_mut(8).unwrap().text_style = Some(crate::typography::TextStyle {
        italic: true,
        ..Default::default()
    });
    source.atom_mut(9).unwrap().depth = 5.;
    let tip = source.add_atom("C", source.atom(9).unwrap().position.offset(35., -22.));
    source.add_bond(9, tip, 1, "plain");
    let fixed = source.add_atom("O", Point::new(-250., 80.));
    let annotation = source.next_id();
    source.annotations.push(crate::document::Annotation {
        id: annotation,
        position: Point::new(-300., 180.),
        text: "oxidane".into(),
        format: Default::default(),
    });
    source
        .molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation,
            atoms: vec![fixed],
            smiles: "O".into(),
            text: "oxidane".into(),
        });
    source.arrows.push(crate::document::Arrow {
        id: source.next_id(),
        start: Point::new(-270., 80.),
        end: Point::new(-400., 80.),
        start_anchor: Some(Anchor {
            target: Target::Atom { atom: fixed },
            offset: Point::new(-20., 0.),
            direction: Point::new(-1., 0.),
            gap: 4.,
        }),
        end_anchor: None,
        kind: "forward".into(),
        control: None,
        cubic: Some([Point::new(-300., 10.), Point::new(-350., 150.)]),
        style: None,
    });
    source.validate().unwrap();
    let original = source.clone();
    let mut work = Work::new(usize::MAX);
    protect_references(&source, &mut work).unwrap();
    pose_variable(&mut source, 7, 1, 8, &mut work).unwrap();
    for atom in &source.atoms {
        if ![8, 9, tip].contains(&atom.id) {
            assert_eq!(atom, original.atom(atom.id).unwrap());
        }
    }
    assert!(
        (source
            .atom(1)
            .unwrap()
            .position
            .distance(source.atom(8).unwrap().position)
            - 42.)
            .abs()
            < 0.001
    );
    assert!(
        (source
            .atom(8)
            .unwrap()
            .position
            .distance(source.atom(9).unwrap().position)
            - original
                .atom(8)
                .unwrap()
                .position
                .distance(original.atom(9).unwrap().position))
        .abs()
            < 0.001
    );
    for a in [8, 9] {
        assert!(
            (source
                .atom(a)
                .unwrap()
                .position
                .distance(source.atom(tip).unwrap().position)
                - original
                    .atom(a)
                    .unwrap()
                    .position
                    .distance(original.atom(tip).unwrap().position))
            .abs()
                < 0.001
        );
    }
    let area = |doc: &Document| {
        let a = doc.atom(8).unwrap().position;
        let b = doc.atom(9).unwrap().position;
        let c = doc.atom(tip).unwrap().position;
        (f64::from(b.x) - f64::from(a.x)) * (f64::from(c.y) - f64::from(a.y))
            - (f64::from(b.y) - f64::from(a.y)) * (f64::from(c.x) - f64::from(a.x))
    };
    assert!(area(&original).abs() > 1.);
    assert!((area(&source) - area(&original)).abs() < 0.01);
    same_except_positions(source.clone(), &original);
    source.validate().unwrap();
    let mut changed = original.clone();
    changed.arrows[0].start_anchor.as_mut().unwrap().target = Target::Atom { atom: 8 };
    assert!(
        protect_references(&changed, &mut work)
            .unwrap_err()
            .contains("attached arrows")
    );
    assert_eq!(
        original.atom(8).unwrap().position,
        Point::new(168.87506, 209.50421)
    );
}

#[test]
fn variable_pose_rejects_bridges_shared_anchors_and_bad_geometry_atomically() {
    let source = variable_arene_source();
    let mut guarded = Vec::new();
    let mut bridge = source.clone();
    bridge.add_bond(9, 4, 1, "plain");
    guarded.push(bridge);
    let mut shared = source.clone();
    let second =
        crate::attachments::add(&mut shared, &[1, 2], crate::attachments::Kind::Variable).unwrap();
    shared.add_bond(second, 9, 1, "plain");
    guarded.push(shared);
    let mut derived = source.clone();
    crate::projection::add_centroid(&mut derived, &[8, 9]).unwrap();
    guarded.push(derived);
    let mut degenerate = source.clone();
    degenerate.atom_mut(8).unwrap().position = degenerate.atom(7).unwrap().position;
    guarded.push(degenerate);
    let mut nonfinite = source.clone();
    nonfinite.atom_mut(9).unwrap().position.x = f32::INFINITY;
    guarded.push(nonfinite);
    let mut curling = source.clone();
    let tip = curling.add_atom("C", Point::new(-191.12494, -0.49579));
    curling.add_bond(9, tip, 1, "plain");
    guarded.push(curling);
    let mut nonconvex = source.clone();
    let tip = nonconvex.add_atom("C", Point::new(123.87084, 0.));
    nonconvex.add_bond(4, tip, 1, "plain");
    guarded.push(nonconvex);
    let mut stereo = source.clone();
    let center = stereo.add_atom("C", Point::new(246., 230.));
    stereo.add_bond(9, center, 1, "plain");
    let mut neighbors = vec![9];
    for (symbol, position) in [
        ("F", Point::new(260., 195.)),
        ("Cl", Point::new(280., 245.)),
        ("Br", Point::new(230., 265.)),
    ] {
        let id = stereo.add_atom(symbol, position);
        stereo.add_bond(center, id, 1, "plain");
        neighbors.push(id);
    }
    stereo.atom_mut(center).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors,
    });
    stereo.validate().unwrap();
    guarded.push(stereo);
    let mut mixed = source.clone();
    let tip = mixed.add_atom("C", Point::new(240., 250.));
    mixed.add_bond(9, tip, 1, "plain");
    mixed = atom_text::apply(&mixed, 9, "[CH2]1-2", Mode::Auto).unwrap();
    mixed.validate().unwrap();
    guarded.push(mixed);
    for mut candidate in guarded {
        let before = candidate.clone();
        assert!(pose_variable(&mut candidate, 7, 1, 8, &mut Work::new(usize::MAX)).is_err());
        assert_eq!(candidate, before);
    }
    let mut candidate = source.clone();
    let mut work = Work::new(1471);
    assert!(
        pose_variable(&mut candidate, 7, 1, 8, &mut work)
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(candidate, source);
    let mut exact = Work::new(1472);
    pose_variable(&mut candidate, 7, 1, 8, &mut exact).unwrap();
    assert_eq!(exact.remaining, 0);
    // Graph-only enumeration retains its established coordinate contract.
    let products = generic::expand(&source, Limits::default()).unwrap();
    assert_eq!(products.len(), 1);
    for product in products {
        assert_eq!(
            product.atom(8).unwrap().position,
            source.atom(8).unwrap().position
        );
        assert_eq!(
            product.atom(9).unwrap().position,
            source.atom(9).unwrap().position
        );
    }
    assert_eq!(source, variable_arene_source());
}

#[test]
fn variable_pose_refuses_positive_strip_touch_and_actual_f32_precision_loss() {
    fn attachment_branch(
        doc: &mut Document,
        candidates: &[u64],
        handle_position: Point,
        root_position: Point,
        tail_position: Point,
    ) -> (u64, u64) {
        let handle =
            crate::attachments::add(doc, candidates, crate::attachments::Kind::Variable).unwrap();
        doc.atom_mut(handle).unwrap().position = handle_position;
        let root = doc.add_atom("O", root_position);
        let tail = doc.add_atom("C", tail_position);
        doc.add_bond(handle, root, 1, "plain");
        doc.add_bond(root, tail, 1, "plain");
        doc.validate().unwrap();
        (handle, root)
    }
    // Exact review geometry: the v1 +0.00042 strip admitted an edge crossing
    // (0,0)->(42,0) at x=0.00021. x=0 touches the connector at its target.
    for x in [0.00021, 0.] {
        let mut doc = Document::default();
        let target = doc.add_atom("C", Point::new(0., 0.));
        let a = doc.add_atom("C", Point::new(-1., 1.));
        let b = doc.add_atom("C", Point::new(-1., -1.));
        let c = doc.add_atom("C", Point::new(x, 1.));
        let d = doc.add_atom("C", Point::new(x, -1.));
        for (i, j) in [(target, a), (a, c), (c, d), (d, b), (b, target)] {
            doc.add_bond(i, j, 1, "plain");
        }
        let (handle, root) = attachment_branch(
            &mut doc,
            &[target, a, b],
            Point::new(-42., 0.),
            Point::new(0., 0.),
            Point::new(42., 0.),
        );
        let before = doc.clone();
        let error =
            pose_variable(&mut doc, handle, target, root, &mut Work::new(usize::MAX)).unwrap_err();
        assert!(error.contains("exterior"));
        assert_eq!(doc, before);
    }
    // Exact review f32 counterexample: requested +42 rounds to +64 at x=1e9.
    let far = 1e9_f32;
    let mut doc = Document::default();
    let target = doc.add_atom("C", Point::new(far, 0.));
    let a = doc.add_atom("C", Point::new(far - 128., 128.));
    let b = doc.add_atom("C", Point::new(far - 128., -128.));
    doc.add_bond(target, a, 1, "plain");
    doc.add_bond(target, b, 1, "plain");
    let (handle, root) = attachment_branch(
        &mut doc,
        &[target, a, b],
        Point::new(far - 64., 0.),
        Point::new(far + 128., 0.),
        Point::new(far + 192., 0.),
    );
    let before = doc.clone();
    assert!(
        pose_variable(&mut doc, handle, target, root, &mut Work::new(usize::MAX))
            .unwrap_err()
            .contains("precision")
    );
    assert_eq!(doc, before);
    // The root's vertical +42 is exact, but rotating the 64-unit tail by45deg
    // rounds its x displacement45.2548 to64: every moved vertex needs checking.
    let mut doc = Document::default();
    let target = doc.add_atom("C", Point::new(far, 0.));
    let a = doc.add_atom("C", Point::new(far - 128., -128.));
    let b = doc.add_atom("C", Point::new(far + 128., -128.));
    doc.add_bond(target, a, 1, "plain");
    doc.add_bond(target, b, 1, "plain");
    let (handle, root) = attachment_branch(
        &mut doc,
        &[target, a, b],
        Point::new(far - 64., -192.),
        Point::new(far + 128., 0.),
        Point::new(far + 192., 0.),
    );
    let before = doc.clone();
    assert!(
        pose_variable(&mut doc, handle, target, root, &mut Work::new(usize::MAX))
            .unwrap_err()
            .contains("precision")
    );
    assert_eq!(doc, before);
}

#[test]
fn native_variable_arene_all_six_candidate_poses_preserve_ring_and_clear_crossings() {
    let source = variable_arene_source();
    let before = source.clone();
    let mut work = Work::new(Limits::default().max_work);
    for target in 1..=6 {
        let mut posed = source.clone();
        pose_variable(&mut posed, 7, target, 8, &mut work).unwrap();
        same_except_positions(posed.clone(), &source);
        for id in 1..=7 {
            assert_eq!(posed.atom(id), source.atom(id));
        }
        let attached = posed.atom(target).unwrap().position;
        let oxygen = posed.atom(8).unwrap().position;
        let methyl = posed.atom(9).unwrap().position;
        assert_methoxy_angle(attached, oxygen, methyl);
        assert!((attached.distance(oxygen) - source.drawing_style.bond_length_world).abs() < 0.001);
        assert!(
            (oxygen.distance(methyl)
                - source
                    .atom(8)
                    .unwrap()
                    .position
                    .distance(source.atom(9).unwrap().position))
            .abs()
                < 0.001
        );
        for edge in source.bonds.iter().filter(|b| b.a <= 6 && b.b <= 6) {
            let a = posed.atom(edge.a).unwrap().position;
            let b = posed.atom(edge.b).unwrap().position;
            if edge.a != target && edge.b != target {
                assert!(
                    !proper_crossing(attached, oxygen, a, b),
                    "target {target} crosses ring edge {}-{}",
                    edge.a,
                    edge.b
                );
            }
            assert!(!proper_crossing(oxygen, methyl, a, b));
        }
    }
    assert_eq!(Limits::default().max_work - work.remaining, 8832);
    assert_eq!(source, before);
}

#[test]
fn variable_pose_cached_boxes_contain_saved_and_refreshed_charged_label_ink() {
    let mut source = variable_arene_source();
    let oxygen = source.atom_mut(8).unwrap();
    oxygen.charge = 1;
    oxygen.explicit_h = 0;
    oxygen.no_implicit = false;
    oxygen.display.hydrogens = Some(true);
    oxygen.display.hydrogen_position = crate::atom_labels::HydrogenPosition::Left;
    oxygen.display.highlight = Some(crate::palette::Color::Custom([240, 210, 30]));
    oxygen.text_style = Some(crate::typography::TextStyle {
        size_pt: 10.,
        italic: true,
        ..Default::default()
    });
    source.validate().unwrap();
    let before = source.clone();
    for target in 1..=6 {
        let mut product = source.clone();
        pose_variable(&mut product, 7, target, 8, &mut Work::new(usize::MAX)).unwrap();
        // Isolate the geometry/ink seam from the independent symmetry policy.
        // The end-to-end prepared-expansion test verifies the real materializer.
        product.atoms.retain(|a| a.id != 7);
        for bond in &mut product.bonds {
            if bond.a == 7 {
                bond.a = target;
            }
            if bond.b == 7 {
                bond.b = target;
            }
        }
        generic::clear_after_expansion(&mut product);
        product.validate().unwrap();
        let shown = preview(&product);
        assert_eq!(product.atom(8).unwrap().label_h, 0);
        assert_eq!(shown.atom(8).unwrap().label_h, 1);
        assert_eq!(shown.atom(8).unwrap().charge, 1);
        assert_ne!(
            atom_bounds(product.atom(8).unwrap(), &product).unwrap(),
            atom_bounds(shown.atom(8).unwrap(), &shown).unwrap()
        );
        let boxes = saved_and_refreshed_boxes(&product, &shown).unwrap();
        let bounds = measured_bounds(&product, &boxes).unwrap();
        for state in [&product, &shown] {
            for (atom, cached) in state.atoms.iter().zip(&boxes) {
                for ink in crate::scene::atom_label_ink_boxes(atom, state) {
                    contained(cached.unwrap(), ink);
                }
                if let Some(highlight) = crate::highlights::atom_bounds(state, atom) {
                    contained(bounds, highlight);
                }
            }
        }
        union_actual_ink_contained(&product, bounds);
        let reopened = Document::from_native_file(&product.file_json().unwrap()).unwrap();
        assert_eq!(reopened, product.current());
        union_actual_ink_contained(&reopened, bounds);
    }
    assert_eq!(source, before);
}

fn methoxytoluene_source() -> Document {
    Document::from_native_file(include_bytes!(
        "../../../../../tests/fixtures/generic-structures/variable-methoxytoluene-v26.rsk"
    ))
    .unwrap()
}

#[test]
fn substituted_ring_drawings_keep_clean_ortho_meta_para_with_one_shared_budget() {
    // ROOT's authored native input, derived from the unchanged failed arene:
    // 6eaa850521b62f9a0538bd3ea107b45acbdb2314b39d274166f90685a90ead82.
    let source = methoxytoluene_source();
    source.validate().unwrap();
    let before = source.clone();
    let fixed: Vec<_> = source
        .atoms
        .iter()
        .filter(|a| a.id != 7 && a.id != 8 && a.id != 9)
        .cloned()
        .collect();
    let handle = source.atom(7).unwrap();
    assert_eq!(handle.centroid, vec![2, 3, 4, 5, 6]);
    // Exercise all raw positions including both adjacent sites. The prepared
    // chemical deduplicator must not mask a geometry refusal at the other ortho.
    let mut pose_work = Work::new(Limits::default().max_work);
    for target in &handle.centroid {
        let mut posed = source.clone();
        pose_variable(&mut posed, 7, *target, 8, &mut pose_work).unwrap();
        for atom in &fixed {
            assert_eq!(posed.atom(atom.id), Some(atom));
        }
        same_except_positions(posed, &source);
    }
    assert!(pose_work.remaining > 0);
    let products = generic::expand_drawings(&source, Limits::default()).unwrap();
    assert_eq!(products.len(), 3);
    for (drawing, target) in products.iter().zip([2, 3, 4]) {
        let doc = &drawing.document;
        assert_eq!((doc.atoms.len(), doc.bonds.len()), (9, 9));
        assert_eq!(formula(doc), "C8H10O");
        let mut attached = neighbors(doc, 8);
        attached.sort_unstable();
        assert_eq!(attached, vec![target, 9]);
        assert_methoxy_angle(
            doc.atom(target).unwrap().position,
            doc.atom(8).unwrap().position,
            doc.atom(9).unwrap().position,
        );
        let connecting = doc
            .atom(target)
            .unwrap()
            .position
            .distance(doc.atom(8).unwrap().position);
        assert!((connecting - source.drawing_style.bond_length_world).abs() < 0.001);
        assert!(
            (doc.atom(8)
                .unwrap()
                .position
                .distance(doc.atom(9).unwrap().position)
                - source
                    .atom(8)
                    .unwrap()
                    .position
                    .distance(source.atom(9).unwrap().position))
            .abs()
                < 0.001
        );
        for atom in &fixed {
            assert_eq!(doc.atom(atom.id).unwrap().position, atom.position);
        }
        let mut proof_work = Work::new(Limits::default().max_work);
        let presentation = Presentation::inspect(&source, &mut proof_work).unwrap();
        // Both saved and exact refreshed labels participate in the real helper.
        let shown = preview(doc);
        let hydrogens: Vec<_> = shown.atoms.iter().map(|a| a.label_h).collect();
        normalize_arrows(
            &mut doc.clone(),
            &hydrogens,
            &source,
            &presentation,
            &mut proof_work,
        )
        .unwrap();
        for branch in doc
            .bonds
            .iter()
            .filter(|b| b.a == 8 || b.b == 8 || b.a == 9 || b.b == 9)
        {
            for host in doc
                .bonds
                .iter()
                .filter(|b| b.a != 8 && b.b != 8 && b.a != 9 && b.b != 9)
            {
                if [branch.a, branch.b]
                    .iter()
                    .any(|id| *id == host.a || *id == host.b)
                {
                    continue;
                }
                assert!(!proper_crossing(
                    doc.atom(branch.a).unwrap().position,
                    doc.atom(branch.b).unwrap().position,
                    doc.atom(host.a).unwrap().position,
                    doc.atom(host.b).unwrap().position
                ));
            }
        }
        legible(doc);
        union_actual_ink_contained(doc, drawing.bounds);
        let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reopened, doc.current());
    }
    let prepared = generic::prepare_expansion(&source, Limits::default()).unwrap();
    assert_eq!(prepared.count(), 3);
    assert_eq!(source, before);
}

#[test]
fn substituted_ring_refuses_crossings_labels_and_exhaustion_without_installing() {
    let source = methoxytoluene_source();
    let mut unlimited = Work::new(usize::MAX);
    pose_variable(&mut source.clone(), 7, 2, 8, &mut unlimited).unwrap();
    let spent = usize::MAX - unlimited.remaining;
    assert!(spent > 1472, "exterior-branch comparisons must be paid");
    let mut short = source.clone();
    assert!(
        pose_variable(&mut short, 7, 2, 8, &mut Work::new(spent - 1))
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(short, source);
    let mut exact = Work::new(spent);
    pose_variable(&mut source.clone(), 7, 2, 8, &mut exact).unwrap();
    assert_eq!(exact.remaining, 0);
    let mut crossed = source.clone();
    // Fixed methyl bond crosses the new adjacent connector. It is a leaf,
    // so peeling alone cannot establish clearance and must never admit it.
    let tip = crossed.atoms.iter().find(|a| a.id > 9).unwrap().id;
    crossed.atom_mut(tip).unwrap().position = Point::new(210., 100.);
    let before = crossed.clone();
    assert!(pose_variable(&mut crossed, 7, 2, 8, &mut Work::new(usize::MAX)).is_err());
    assert_eq!(crossed, before);
    let mut collision = source.clone();
    // Deliberately offset an existing fixed atom number into the new O ink.
    // Its native metadata is retained and the complete enumeration refuses.
    let center = collision.atom(tip).unwrap().position;
    collision.atom_mut(tip).unwrap().display.number = Some(crate::atom_labels::Number {
        text: "fixed mark".into(),
        offset: Some(Point::new(196.616, 87.5).offset(-center.x, -center.y)),
        style: Default::default(),
    });
    collision.validate().unwrap();
    let before = collision.clone();
    assert!(
        generic::expand_drawings(&collision, Limits::default())
            .unwrap_err()
            .contains("preserved bonds and atom labels")
    );
    assert_eq!(collision, before);
    // A refreshed H label that overlaps the host is equally unsupported even
    // when the saved label_h was empty. The low-level guard uses both states.
    let mut product = source.clone();
    pose_variable(&mut product, 7, 2, 8, &mut Work::new(usize::MAX)).unwrap();
    product.atoms.retain(|a| a.id != 7);
    for b in &mut product.bonds {
        if b.a == 7 {
            b.a = 2;
        }
        if b.b == 7 {
            b.b = 2;
        }
    }
    generic::clear_after_expansion(&mut product);
    let oxygen = product.atom_mut(8).unwrap();
    oxygen.charge = 1;
    oxygen.explicit_h = 0;
    oxygen.no_implicit = false;
    oxygen.display.hydrogens = Some(true);
    oxygen.display.hydrogen_position = crate::atom_labels::HydrogenPosition::Left;
    let shown = preview(&product);
    assert_eq!(shown.atom(8).unwrap().label_h, 1);
    let saved = atom_bounds(product.atom(8).unwrap(), &product)
        .unwrap()
        .unwrap();
    let refreshed_h = crate::scene::atom_label_ink_boxes(shown.atom(8).unwrap(), &shown)
        .into_iter()
        .find(|(lo, hi)| {
            let center = Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
            center.x < saved.0.x
                || center.x > saved.1.x
                || center.y < saved.0.y
                || center.y > saved.1.y
        })
        .expect("real refreshed implicit H must add ink beyond the saved O+ label");
    let center = Point::new(
        (refreshed_h.0.x + refreshed_h.1.x) * 0.5,
        (refreshed_h.0.y + refreshed_h.1.y) * 0.5,
    );
    let fixed_center = product.atom(tip).unwrap().position;
    product.atom_mut(tip).unwrap().display.number = Some(crate::atom_labels::Number {
        text: "X".into(),
        offset: Some(center.offset(-fixed_center.x, -fixed_center.y)),
        style: Default::default(),
    });
    let shown = preview(&product);
    let boxes = saved_and_refreshed_boxes(&product, &shown).unwrap();
    let fixed = atom_bounds(product.atom(tip).unwrap(), &product)
        .unwrap()
        .unwrap();
    assert!(
        fixed.0.x < refreshed_h.1.x
            && refreshed_h.0.x < fixed.1.x
            && fixed.0.y < refreshed_h.1.y
            && refreshed_h.0.y < fixed.1.y,
        "the fixed connected host label must actually overlap refreshed H ink"
    );
    assert!(variable_ink_clearance(&source, &product, &boxes).is_err());
}

fn assert_methoxy_angle(target: Point, root: Point, neighbor: Point) {
    let a = (
        f64::from(target.x) - f64::from(root.x),
        f64::from(target.y) - f64::from(root.y),
    );
    let b = (
        f64::from(neighbor.x) - f64::from(root.x),
        f64::from(neighbor.y) - f64::from(root.y),
    );
    let cosine = (a.0 * b.0 + a.1 * b.1) / (a.0.hypot(a.1) * b.0.hypot(b.1));
    assert!(
        (cosine + 0.5).abs() < 0.00001,
        "ordinary methoxy angle: {cosine}"
    );
}

#[test]
fn variable_connector_refuses_folded_moving_bonds_and_atoms_before_publication() {
    for folded_y in [-10., 0.] {
        let mut source = Document::default();
        let target = source.add_atom("C", Point::new(0., 0.));
        let first = source.add_atom("C", Point::new(-21., 36.373066));
        let second = source.add_atom("C", Point::new(-21., -36.373066));
        for (a, b) in [(target, first), (first, second), (second, target)] {
            source.add_bond(a, b, 1, "plain");
        }
        let handle = crate::attachments::add(
            &mut source,
            &[target, first, second],
            crate::attachments::Kind::Variable,
        )
        .unwrap();
        source.atom_mut(handle).unwrap().position = Point::new(120.301_51, -29.698485);
        let root = source.add_atom("C", Point::new(150., 0.));
        let a = source.add_atom("C", Point::new(171., 36.373066));
        let b = source.add_atom("C", Point::new(129., folded_y));
        for (a, b) in [(handle, root), (root, a), (a, b)] {
            source.add_bond(a, b, 1, "plain");
        }
        source.validate().unwrap();
        let before = source.clone();
        assert!(!proper_crossing(
            source.atom(handle).unwrap().position,
            source.atom(root).unwrap().position,
            source.atom(a).unwrap().position,
            source.atom(b).unwrap().position
        ));
        let mut posed = source.clone();
        pose_variable(&mut posed, handle, target, root, &mut Work::new(usize::MAX)).unwrap();
        assert!(
            posed
                .atom(root)
                .unwrap()
                .position
                .distance(Point::new(42., 0.))
                < 0.001
        );
        assert!(
            posed
                .atom(a)
                .unwrap()
                .position
                .distance(Point::new(63., 36.373066))
                < 0.001
        );
        assert!(
            posed
                .atom(b)
                .unwrap()
                .position
                .distance(Point::new(21., folded_y))
                < 0.001
        );
        if folded_y < 0. {
            assert!(proper_crossing(
                posed.atom(target).unwrap().position,
                posed.atom(root).unwrap().position,
                posed.atom(a).unwrap().position,
                posed.atom(b).unwrap().position
            ));
        } else {
            // The nonroot atom touches the new connector exactly, even though
            // its source atom/bond did not touch the old handle connection.
            assert!(posed.atom(b).unwrap().position.y.abs() < 0.001);
        }
        // The detached pose is only a candidate. Full saved/refreshed ink and
        // internal-bond certification rejects it before any product escapes.
        assert!(
            generic::expand_drawings(&source, Limits::default())
                .unwrap_err()
                .contains("preserved bonds and atom labels")
        );
        assert_eq!(source, before);
    }
}
