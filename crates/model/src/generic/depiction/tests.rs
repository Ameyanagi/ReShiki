use super::*;
use crate::generic::drawing_test_support::{
    chain, formula, legible, neighbors, preview, raw, same_except_positions,
};
use crate::generic::references::protect_references;
use crate::{
    atom_text::{self, Mode},
    generic::{self, Crossing, Limits, RepeatCount, RepeatPattern, RepeatUnit},
};

fn repeat_source(branched: bool) -> Document {
    let mut doc = chain(&["O", "C", "C", "N"]);
    let mut members = vec![2, 3];
    if branched {
        let id = doc.add_atom("C", Point::new(42., 42.));
        doc.add_bond(2, id, 1, "plain");
        members.push(id);
    }
    let bracket = doc.next_id();
    doc.graphics.push(crate::graphics::Graphic::dragged(
        bracket,
        crate::graphics::GraphicKind::Brackets,
        Point::new(30., -30.),
        Point::new(100., 70.),
        Default::default(),
        crate::graphics::BracketSides::Both,
        false,
    ));
    generic::set_repeat_unit(
        &mut doc,
        RepeatUnit {
            bracket: Some(bracket),
            bracket_objects: vec![],
            members,
            crossings: vec![
                Crossing {
                    inside: 2,
                    outside: 1,
                },
                Crossing {
                    inside: 3,
                    outside: 4,
                },
            ],
            crossing_brackets: vec![],
            head_count: Some(1),
            usage: "SRU".into(),
            pattern: RepeatPattern::HeadToTail,
            count: RepeatCount::Finite { min: 1, max: 3 },
            label: "1-3".into(),
            flip: "NoFlip".into(),
        },
    )
    .unwrap();
    doc
}

fn drawings(source: &Document) -> Vec<Document> {
    generic::expand_drawings(source, Limits::default())
        .unwrap()
        .into_iter()
        .map(|p| p.document)
        .collect()
}

#[test]
fn generated_links_keep_propane_butane_pentane_and_visible_clone_bonds() {
    let source = atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-3", Mode::Auto).unwrap();
    let before = source.clone();
    let products = drawings(&source);
    for (i, (product, expected)) in products.iter().zip(["C3H8", "C4H10", "C5H12"]).enumerate() {
        assert_eq!(product.atoms.len(), 3 + i);
        assert_eq!(product.bonds.len(), 2 + i);
        assert_eq!(formula(product), expected);
        assert!(product.generic.is_empty());
        assert!(product.bonds.iter().all(|b| b.order == 1));
        assert_eq!(
            product
                .atoms
                .iter()
                .filter(|a| neighbors(product, a.id).len() == 1)
                .count(),
            2
        );
        same_except_positions(product.clone(), &raw(&source, i as u32 + 1));
        legible(product);
        assert!(product.bonds.iter().all(|b| {
            product
                .atom(b.a)
                .unwrap()
                .position
                .distance(product.atom(b.b).unwrap().position)
                >= 41.9
        }));
    }
    assert_eq!(source, before);
}

#[test]
fn generated_repeats_keep_endcaps_and_expose_skeletal_carbon_vertices() {
    let source = repeat_source(false);
    let before = source.clone();
    let products = drawings(&source);
    for (i, (product, expected)) in products
        .iter()
        .zip(["C2H7NO", "C4H11NO", "C6H15NO"])
        .enumerate()
    {
        assert_eq!(product.atoms.len(), 4 + i * 2);
        assert_eq!(product.bonds.len(), 3 + i * 2);
        assert_eq!(formula(product), expected);
        assert_eq!(neighbors(product, 1).len(), 1);
        assert_eq!(neighbors(product, 4).len(), 1);
        assert_eq!(product.atom(1).unwrap().element, "O");
        assert_eq!(product.atom(4).unwrap().element, "N");
        let shown = preview(product);
        assert_eq!(shown.atom(1).unwrap().label_h, 1);
        assert_eq!(shown.atom(4).unwrap().label_h, 2);
        for a in product.atoms.iter().filter(|a| a.element == "C") {
            let ends = neighbors(product, a.id);
            assert_eq!(ends.len(), 2);
            let p = product.atom(ends[0]).unwrap().position;
            let q = product.atom(ends[1]).unwrap().position;
            let ux = p.x - a.position.x;
            let uy = p.y - a.position.y;
            let vx = q.x - a.position.x;
            let vy = q.y - a.position.y;
            assert!(
                (ux * vy - uy * vx).abs() / (ux.hypot(uy) * vx.hypot(vy)) > 0.4,
                "hidden carbon {} has no visible corner",
                a.id
            );
        }
        same_except_positions(product.clone(), &raw(&source, i as u32 + 1));
        legible(product);
    }
    assert_eq!(source, before);
}

#[test]
fn generated_cyclic_links_keep_six_seven_eight_member_ring_closure() {
    let mut source = Document::default();
    for i in 0..6 {
        let angle = i as f32 * std::f32::consts::TAU / 6.;
        source.add_atom("C", Point::new(angle.cos() * 42., angle.sin() * 42.));
    }
    for i in 1..=6 {
        source.add_bond(i, i % 6 + 1, 1, "plain");
    }
    let source = atom_text::apply(&source, 2, "[CH2]1-3", Mode::Auto).unwrap();
    let before = source.clone();
    let products = drawings(&source);
    for (i, product) in products.iter().enumerate() {
        assert_eq!(product.atoms.len(), 6 + i);
        assert_eq!(product.bonds.len(), 6 + i);
        assert!(
            product
                .atoms
                .iter()
                .all(|a| neighbors(product, a.id).len() == 2)
        );
        assert_eq!(formula(product), format!("C{}H{}", 6 + i, 12 + i * 2));
        let mut pending = vec![1];
        let mut reached = HashSet::new();
        while let Some(id) = pending.pop() {
            if reached.insert(id) {
                pending.extend(neighbors(product, id));
            }
        }
        assert_eq!(reached.len(), product.atoms.len());
        assert!(
            product
                .atoms
                .iter()
                .any(|a| (a.position.y - product.atoms[0].position.y).abs() > 20.)
        );
        same_except_positions(product.clone(), &raw(&source, i as u32 + 1));
        legible(product);
    }
    assert_eq!(source, before);
}

#[test]
fn generated_branched_repeat_keeps_branches_and_fixed_neighboring_components() {
    let mut source = repeat_source(true);
    let fixed = source.add_atom("C", Point::new(-250., 70.));
    let mut ends = Vec::new();
    for (i, symbol) in ["F", "Cl", "Br", "I"].into_iter().enumerate() {
        let id = source.add_atom(symbol, Point::new(-300. + i as f32 * 35., 130.));
        source.add_bond(fixed, id, 1, "plain");
        ends.push(id);
    }
    source.atom_mut(fixed).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: ends.clone(),
    });
    source.groups.push(crate::grouping::Group {
        id: source.next_id(),
        members: std::iter::once(fixed).chain(ends).collect(),
        integral: false,
    });
    source.validate().unwrap();
    let before = source.clone();
    let products = drawings(&source);
    for (i, product) in products.iter().enumerate() {
        assert_eq!(product.atoms.len(), 10 + i * 3);
        assert_eq!(
            product
                .atoms
                .iter()
                .filter(|a| a.element == "C" && neighbors(product, a.id).len() == 3)
                .count(),
            i + 1
        );
        assert_eq!(
            product
                .atoms
                .iter()
                .filter(|a| a.element == "C" && neighbors(product, a.id).len() == 1)
                .count(),
            i + 1
        );
        for a in before.atoms.iter().filter(|a| a.id >= fixed) {
            assert_eq!(product.atom(a.id).unwrap(), a);
        }
        assert_eq!(product.groups, before.groups);
        same_except_positions(product.clone(), &raw(&source, i as u32 + 1));
        // Only the generated component is checked for atom-label clearance; the
        // intentionally arbitrary fixed stereocenter layout is retained exactly.
        let mut pending = vec![1];
        let mut ids = HashSet::new();
        while let Some(id) = pending.pop() {
            if ids.insert(id) {
                pending.extend(neighbors(product, id));
            }
        }
        let generated =
            crate::editing::analysis_document(product, &ids.into_iter().collect::<Vec<_>>());
        assert_eq!(
            formula(&generated),
            format!("C{}H{}NO", (i + 1) * 3, (i + 1) * 6 + 3)
        );
        assert!(generated.atoms.iter().any(|a| a.position.y.abs() > 0.1));
        legible(&generated);
    }
    assert_eq!(source, before);
    // Remote affected-component stereo is guarded even away from the two ports.
    let mut guarded = repeat_source(false);
    let c = guarded.add_atom("C", Point::new(180., 0.));
    guarded.add_bond(4, c, 1, "plain");
    let d = guarded.add_atom("C", Point::new(220., 0.));
    guarded.add_bond(c, d, 1, "plain");
    let f = guarded.add_atom("F", Point::new(220., 42.));
    guarded.add_bond(d, f, 1, "plain");
    let cl = guarded.add_atom("Cl", Point::new(260., 0.));
    guarded.add_bond(d, cl, 1, "plain");
    let br = guarded.add_atom("Br", Point::new(220., -42.));
    guarded.add_bond(d, br, 1, "plain");
    guarded.atom_mut(d).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors: vec![c, f, cl, br],
    });
    let before_guard = guarded.clone();
    assert!(
        generic::expand_drawings(&guarded, Limits::default())
            .unwrap_err()
            .contains("component stereochemistry")
    );
    assert_eq!(guarded, before_guard);
    let mut wedge = guarded.clone();
    wedge.atom_mut(d).unwrap().stereo = None;
    wedge
        .bonds
        .iter_mut()
        .find(|b| generic::pair(b.a, b.b) == generic::pair(d, f))
        .unwrap()
        .display = "wedge".into();
    let before_wedge = wedge.clone();
    assert!(
        generic::expand_drawings(&wedge, Limits::default())
            .unwrap_err()
            .contains("component stereochemistry")
    );
    assert_eq!(wedge, before_wedge);
    let mut alkene = repeat_source(false);
    let mut last = 4;
    let mut remote = Vec::new();
    for i in 0..4 {
        let id = alkene.add_atom(
            "C",
            Point::new(180. + i as f32 * 42., if i % 2 == 0 { 20. } else { 0. }),
        );
        alkene.add_bond(last, id, 1, "plain");
        remote.push(id);
        last = id;
    }
    let specified = alkene
        .bonds
        .iter_mut()
        .find(|b| generic::pair(b.a, b.b) == generic::pair(remote[1], remote[2]))
        .unwrap();
    specified.order = 2;
    specified.stereo_authoritative = true;
    specified.stereo = Some("z".into());
    specified.stereo_atoms = vec![remote[0], remote[3]];
    let before_alkene = alkene.clone();
    assert!(
        generic::expand_drawings(&alkene, Limits::default())
            .unwrap_err()
            .contains("component stereochemistry")
    );
    assert_eq!(alkene, before_alkene);
}

#[test]
fn anchored_components_keep_atom_scan_and_stack_pop_traversal_order() {
    let mut doc = Document::default();
    let ids: Vec<_> = (0..12)
        .map(|i| doc.add_atom("C", Point::new((i % 4) as f32 * 42., (i / 4) as f32 * 42.)))
        .collect();
    // Interleave the three components in atom order and deliberately vary
    // branch insertion order: the last neighbor pushed is the first popped.
    for (a, b) in [
        (0, 2),
        (0, 5),
        (5, 8),
        (1, 7),
        (1, 4),
        (7, 10),
        (3, 6),
        (3, 9),
        (9, 11),
    ] {
        doc.add_bond(ids[a], ids[b], 1, "plain");
    }
    let expected = vec![vec![0, 5, 8, 2], vec![1, 4, 7, 10], vec![3, 9, 11, 6]];
    for (anchors, components) in [
        (vec![ids[11], ids[8], ids[10]], expected.clone()),
        (vec![ids[11], ids[10]], expected[1..].to_vec()),
    ] {
        let plan = Plan::new(
            &doc,
            &anchors.into_iter().collect(),
            &mut Work::new(usize::MAX),
        )
        .unwrap();
        assert_eq!(plan.components, components);
    }

    // protect_references keeps its component labels private; pin its observable
    // result on this graph, including a caption spanning only part of a branch.
    assert_eq!(protect_references(&doc, &mut Work::new(usize::MAX)), Ok(()));
    doc.molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: doc.next_id(),
            atoms: vec![ids[0], ids[2]],
            smiles: "CC".into(),
            text: "ethane".into(),
        });
    const INCOMPLETE_NAME: &str = "Generated drawings require complete inherited named components; the source drawing is retained";
    assert_eq!(
        protect_references(&doc, &mut Work::new(usize::MAX)).unwrap_err(),
        INCOMPLETE_NAME
    );
}
