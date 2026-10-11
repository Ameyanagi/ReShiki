use super::*;
use crate::{
    atom_text::{self, Mode},
    generic::{self, Crossing, Limits, RepeatCount, RepeatPattern, RepeatUnit},
};

fn chain(elements: &[&str]) -> Document {
    let mut doc = Document::default();
    for (i, element) in elements.iter().enumerate() {
        let id = doc.add_atom(element, Point::new(i as f32 * 42., 0.));
        if i > 0 {
            doc.add_bond(id - 1, id, 1, "plain");
        }
    }
    doc
}
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
fn formula(doc: &Document) -> String {
    let molecule = crate::chemistry::document::prepare(doc).unwrap();
    let facts: Vec<_> = molecule
        .state
        .graph
        .atoms
        .iter()
        .zip(&molecule.state.valences)
        .map(|(a, v)| crate::chemistry::AtomFacts {
            atomic_number: a.atomic_number,
            isotope: a.isotope,
            charge: a.charge,
            hydrogens: (u32::from(a.explicit_hydrogens) + v.implicit_hydrogens) as u8,
            radical_electrons: a.radical_electrons,
        })
        .collect();
    crate::chemistry::properties(&facts).unwrap().formula
}
fn neighbors(doc: &Document, id: u64) -> Vec<u64> {
    doc.bonds
        .iter()
        .filter_map(|b| {
            if b.a == id {
                Some(b.b)
            } else if b.b == id {
                Some(b.a)
            } else {
                None
            }
        })
        .collect()
}
fn preview(doc: &Document) -> Document {
    let molecule = crate::chemistry::document::prepare(doc).unwrap();
    let mut result = doc.clone();
    for (i, a) in result.atoms.iter_mut().enumerate() {
        a.label_h = u32::from(molecule.state.graph.atoms[i].explicit_hydrogens)
            + molecule.state.valences[i].implicit_hydrogens;
    }
    result
}
fn legible(doc: &Document) {
    let shown = preview(doc);
    let boxes: Vec<_> = shown
        .atoms
        .iter()
        .map(|a| atom_bounds(a, &shown).unwrap())
        .collect();
    for (i, a) in shown.atoms.iter().enumerate() {
        for (j, z) in shown.atoms.iter().enumerate().skip(i + 1) {
            if let (Some((lo, hi)), Some((zl, zh))) = (boxes[i], boxes[j]) {
                assert!(
                    hi.x <= zl.x || zh.x <= lo.x || hi.y <= zl.y || zh.y <= lo.y,
                    "labels {} and {} overlap",
                    a.id,
                    z.id
                );
            }
        }
    }
    for b in &shown.bonds {
        let i = shown.atoms.iter().position(|a| a.id == b.a).unwrap();
        let j = shown.atoms.iter().position(|a| a.id == b.b).unwrap();
        let a = &shown.atoms[i];
        let z = &shown.atoms[j];
        let distance = a.position.distance(z.position);
        let ux = (z.position.x - a.position.x) / distance;
        let uy = (z.position.y - a.position.y) / distance;
        let available =
            distance - reach(a.position, boxes[i], ux, uy) - reach(z.position, boxes[j], -ux, -uy);
        assert!(
            available >= doc.drawing_style.bond_length_world * 0.39,
            "bond {}-{} has only {available} world units of clear ink",
            b.a,
            b.b
        );
    }
}
fn same_except_positions(mut actual: Document, expected: &Document) {
    for (a, b) in actual.atoms.iter_mut().zip(&expected.atoms) {
        assert_eq!(a.id, b.id);
        a.position = b.position;
    }
    assert_eq!(&actual, expected);
}
fn raw(source: &Document, count: u32) -> Document {
    let mut seed = source.clone();
    seed.generic = Default::default();
    let mut result = if let Some(link) = source.generic.links.first() {
        generic::link(&seed, link, count).unwrap()
    } else {
        generic::repeat(&seed, &source.generic.repeats[0], count).unwrap()
    };
    crate::atom_labels::clear_computed(&mut result);
    result
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
fn generated_layout_shares_one_budget_and_rejects_atomically_before_expensive_work() {
    let source = atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-3", Mode::Auto).unwrap();
    let before = source.clone();
    STAGES.with(|s| s.set([0; 5]));
    assert!(
        generic::expand_drawings(
            &source,
            Limits {
                max_work: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    assert_eq!(STAGES.with(std::cell::Cell::get), [0; 5]);
    let anchors = HashSet::from([2]);
    let candidate = raw(&source, 1);
    let mut probe = Work::new(usize::MAX);
    Plan::new(&candidate, &anchors, &mut probe).unwrap();
    let reserved = usize::MAX - probe.remaining;
    let mut short = Work::new(reserved - 1);
    assert!(Plan::new(&candidate, &anchors, &mut short).is_err());
    assert_eq!(STAGES.with(std::cell::Cell::get), [0; 5]);
    // A consumed allowance is not restored for the next product. This budget
    // funds one complete product but cannot even reserve the next product.
    let molecule = crate::chemistry::document::prepare(&candidate).unwrap();
    let mut first = candidate.clone();
    let mut work = Work::new(Limits::default().max_work);
    let plan = Plan::new(&first, &anchors, &mut work).unwrap();
    plan.finish(&mut first, &molecule, &mut work).unwrap();
    let spent = Limits::default().max_work - work.remaining;
    let mut shared = Work::new(spent);
    let mut first = candidate.clone();
    Plan::new(&first, &anchors, &mut shared)
        .unwrap()
        .finish(&mut first, &molecule, &mut shared)
        .unwrap();
    assert!(Plan::new(&candidate, &anchors, &mut shared).is_err());
    // For this one three-way link, reserve its drawing presentation and graph
    // construction, fund two complete products, and stop one unit short of the
    // third product's preparation reservation. Earlier completions stay private.
    let mut upfront = Work::new(usize::MAX);
    let presentation = Presentation::inspect(&source, &mut upfront).unwrap();
    protect_references(&source, &mut upfront).unwrap();
    upfront
        .charge(drawing_size(&source) * 11 + 9 * 3 * 20)
        .unwrap();
    presentation
        .reserve(&source, 5, 9, 4 * 2 + 3 * 8, &mut upfront)
        .unwrap();
    let upfront = usize::MAX - upfront.remaining;
    let mut completed = Work::new(Limits::default().max_work);
    for count in 1..=2 {
        let mut product = raw(&source, count);
        let molecule = crate::chemistry::document::prepare(&product).unwrap();
        completed
            .charge(product.atoms.len() * std::mem::size_of::<u32>())
            .unwrap();
        Plan::new(&product, &anchors, &mut completed)
            .unwrap()
            .finish(&mut product, &molecule, &mut completed)
            .unwrap();
    }
    let completed = Limits::default().max_work - completed.remaining;
    let mut third = Work::new(usize::MAX);
    Plan::new(&raw(&source, 3), &anchors, &mut third).unwrap();
    let third = usize::MAX - third.remaining;
    STAGES.with(|s| s.set([0; 5]));
    assert!(
        generic::expand_drawings(
            &source,
            Limits {
                max_work: upfront + completed + third - 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let stages = STAGES.with(std::cell::Cell::get);
    assert_eq!(
        stages[0], 2,
        "the aggregate reservation must prevent the third product's preparation: {stages:?}"
    );
    assert_eq!(
        stages[2], 2,
        "completed earlier products must not escape on later exhaustion: {stages:?}"
    );
    // Solver/measurement errors reject detached data; no partially completed
    // enumeration escapes, and the source remains byte-for-byte equivalent.
    let mut insufficient = candidate.clone();
    let mut work = Work::new(reserved);
    let plan = Plan::new(&insufficient, &anchors, &mut work).unwrap();
    assert!(
        plan.finish(&mut insufficient, &molecule, &mut work)
            .is_err()
    );
    let mut corrupted = candidate.clone();
    corrupted.atoms[0].explicit_h = 1;
    assert!(preserve(&candidate, &corrupted, &[true; 3]).is_err());
    let mut bad_geometry = molecule.clone();
    bad_geometry.state.graph.atoms.pop();
    let mut detached = candidate.clone();
    let mut work = Work::new(Limits::default().max_work);
    assert!(
        Plan::new(&detached, &anchors, &mut work)
            .unwrap()
            .finish(&mut detached, &bad_geometry, &mut work)
            .is_err()
    );
    assert_eq!(detached, candidate);
    assert_eq!(source, before);
}

#[test]
fn generated_presentation_preflight_rejects_large_cartesian_annotations_before_materialization() {
    let mut source = chain(&["*", "C", "*", "C", "*", "C", "*", "C", "*"]);
    for id in [1, 3, 5, 7, 9] {
        source = atom_text::apply(&source, id, "[C,N]", Mode::Auto).unwrap();
    }
    source.annotations.push(crate::document::Annotation {
        id: source.next_id(),
        position: Point::new(0., 140.),
        text: "a".repeat(200_000),
        format: Default::default(),
    });
    source.validate().unwrap();
    let before = source.clone();
    STAGES.with(|s| s.set([0; 5]));
    assert!(
        generic::expand_drawings(&source, Limits::default())
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(
        STAGES.with(std::cell::Cell::get),
        [0; 5],
        "reject before seed cloning, materialization, preparation, depiction or ink measurement"
    );
    assert_eq!(source, before);
    // Chemistry-only enumeration keeps its established limits/API behavior;
    // drawing presentation costs do not change this public materializer.
    let products = generic::expand(&source, Limits::default()).unwrap();
    assert_eq!(products.len(), 32);
    assert!(
        products
            .iter()
            .all(|p| p.annotations == source.annotations && p.generic.is_empty())
    );
    assert_eq!(source, before);
}

#[test]
fn generated_paint_bounds_contain_extreme_highlights_and_joined_bonds() {
    let paint = crate::palette::Color::Custom([240, 210, 30]);
    for (length, spacing, bold) in [(100., 0.4, 2.), (5., 0.05, 12.)] {
        let mut source =
            atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-3", Mode::Auto).unwrap();
        source.drawing_style.set_bond_length(length);
        source.drawing_style.bond_spacing_ratio = spacing;
        source.drawing_style.line_width_pt = 0.1;
        source.drawing_style.bold_width_pt = bold;
        source.atom_mut(2).unwrap().display.carbons = Some(crate::atom_labels::Carbons::All);
        source.atom_mut(2).unwrap().text_style = Some(crate::typography::TextStyle {
            size_pt: 144.,
            ..Default::default()
        });
        // A fixed, valid tert-butyl graph with acute projected bold junctions
        // exercises the corner clamp without moving arbitrary manual geometry.
        let center = source.add_atom("C", Point::new(-900., 0.));
        for (i, p) in [
            Point::new(-610., 0.),
            Point::new(-615., 60.),
            Point::new(-615., -60.),
        ]
        .into_iter()
        .enumerate()
        {
            let leaf = source.add_atom("C", p);
            source.add_bond(center, leaf, 1, "plain");
            let b = source.bonds.last_mut().unwrap();
            b.display = "bold".into();
            b.projection = true;
            b.color = crate::palette::Color::Custom([60 + i as u8 * 60, 30, 90]);
        }
        for a in &mut source.atoms {
            a.display.highlight = Some(paint);
        }
        for b in &mut source.bonds {
            b.highlight = Some(paint);
        }
        source.validate().unwrap();
        let before = source.clone();
        for (i, product) in generic::expand_drawings(&source, Limits::default())
            .unwrap()
            .iter()
            .enumerate()
        {
            let shown = preview(&product.document);
            let joins = crate::bond_joins::Joins::new(&shown);
            let (lo, hi) = product.bounds;
            let contains = |p: Point| {
                assert!(
                    p.x >= lo.x - 0.01
                        && p.y >= lo.y - 0.01
                        && p.x <= hi.x + 0.01
                        && p.y <= hi.y + 0.01,
                    "paint {p:?} lies outside returned bounds {lo:?}..{hi:?} for {length}/{spacing}/{bold}"
                );
            };
            for a in &shown.atoms {
                if let Some((a, z)) = crate::highlights::atom_bounds(&shown, a) {
                    contains(a);
                    contains(z);
                }
            }
            for b in &shown.bonds {
                if let Some((a, z)) = crate::highlights::bond_bounds(&shown, b, &joins) {
                    contains(a);
                    contains(z);
                }
                if joins.needed(b) {
                    for p in joins.polygon(
                        b,
                        shown.atom(b.a).unwrap().position,
                        shown.atom(b.b).unwrap().position,
                    ) {
                        contains(p);
                    }
                }
            }
            let junctions = joins.junctions();
            assert!(!junctions.is_empty());
            for p in junctions
                .iter()
                .flat_map(|j| &j.parts)
                .flat_map(|(_, points)| points)
            {
                contains(*p);
            }
            same_except_positions(product.document.clone(), &raw(&source, i as u32 + 1));
            for a in before.atoms.iter().filter(|a| a.id >= center) {
                assert_eq!(product.document.atom(a.id).unwrap(), a);
            }
        }
        assert_eq!(source, before);
    }
}

// Compatibility proposals below require the composed import/drawing/names/maps
// model. These are private authored tests; no execution is claimed.
fn union_source() -> Document {
    use crate::{
        arrow_anchors::{Anchor, Target},
        atom_labels::{MappingDisplay, Number},
        scientific::{AtomMark, MarkKind},
    };
    let mut source = atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-3", Mode::Auto).unwrap();
    for (id, map) in [(1, 17), (3, 23)] {
        let atom = source.atom_mut(id).unwrap();
        atom.map_num = map;
        atom.display.mapping = MappingDisplay {
            show: Some(true),
            offset: (id == 1).then_some(Point::new(-110., -120.)),
            style: crate::typography::TextStyle {
                size_pt: 32.,
                italic: true,
                underline: true,
                family: "Times New Roman".into(),
                ..Default::default()
            },
        };
    }
    source.atom_mut(2).unwrap().mark_serial = 11;
    source.atom_mut(3).unwrap().display.number = Some(Number {
        text: "tail".into(),
        offset: Some(Point::new(0., 130.)),
        style: crate::typography::TextStyle {
            size_pt: 24.,
            italic: true,
            ..Default::default()
        },
    });
    let fixed = source.add_atom("O", Point::new(-250., 80.));
    let atom = source.atom_mut(fixed).unwrap();
    atom.mark_serial = 11;
    atom.marks.push(AtomMark {
        id: Some(7),
        kind: MarkKind::LonePair,
        offset: Point::new(-16., -16.),
        angle: 0.,
        size_pt: Some(10.),
    });
    source
        .depth_appearance
        .push(crate::depth_appearance::Scope {
            atoms: vec![fixed],
            automatic: false,
            strength: 0.62,
            rear_opacity: 0.3,
            weights: [(fixed, 0.7)].into(),
            overrides: [(fixed, 0.2)].into(),
            rear_weights: [(fixed, 1.)].into(),
            rear_overrides: [(fixed, 0.4)].into(),
        });
    let annotation = source.next_id();
    source.annotations.push(crate::document::Annotation {
        id: annotation,
        position: Point::new(-380., 180.),
        text: "oxidane".into(),
        format: crate::typography::TextFormat {
            width_pt: Some(55.),
            style: crate::typography::TextStyle {
                size_pt: 30.,
                italic: true,
                underline: true,
                family: "Times New Roman".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    });
    source
        .molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation,
            atoms: vec![fixed],
            smiles: "O".into(),
            text: "oxidane".into(),
        });
    let target = Target::LonePair {
        atom: fixed,
        mark: 7,
    };
    let center = target.center(&source).unwrap();
    source.arrows.push(crate::document::Arrow {
        id: source.next_id(),
        start: center.offset(-100., 0.),
        end: Point::new(-600., 80.),
        start_anchor: Some(Anchor {
            target,
            offset: Point::new(-100., 0.),
            direction: Point::new(-1., 0.),
            gap: 4.,
        }),
        end_anchor: None,
        kind: "equilibrium".into(),
        control: None,
        cubic: Some([Point::new(-500., -190.), Point::new(-500., 300.)]),
        style: None,
    });
    source.validate().unwrap();
    source
}
fn contained((lo, hi): (Point, Point), (a, z): (Point, Point)) {
    assert!(
        a.x >= lo.x - 0.01 && a.y >= lo.y - 0.01 && z.x <= hi.x + 0.01 && z.y <= hi.y + 0.01,
        "actual ink {a:?}..{z:?} lies outside {lo:?}..{hi:?}"
    );
}
fn union_actual_ink_contained(doc: &Document, bounds: (Point, Point)) {
    let shown = preview(doc);
    for state in [doc, &shown] {
        for atom in &state.atoms {
            for ink in crate::scene::atom_label_ink_boxes(atom, state) {
                contained(bounds, ink);
            }
        }
    }
    for indicator in crate::atom_labels::indicators(&shown) {
        for (lo, hi) in
            crate::style::text_ink_boxes(&indicator.text, indicator.height, &indicator.style)
        {
            contained(
                bounds,
                (
                    indicator.origin.offset(lo.x, lo.y),
                    indicator.origin.offset(hi.x, hi.y),
                ),
            );
        }
    }
    for a in &doc.annotations {
        for f in crate::typography::layout(&a.text, &a.format).fragments {
            let origin = a.position.offset(f.position.x, f.position.y);
            for (lo, hi) in crate::style::text_ink_boxes(&f.text, f.style.size(), &f.style) {
                contained(
                    bounds,
                    (origin.offset(lo.x, lo.y), origin.offset(hi.x, hi.y)),
                );
            }
        }
    }
    for arrow in &doc.arrows {
        for part in arrow.paths() {
            let half = part.style.width() / 2.;
            for point in part
                .commands
                .iter()
                .flat_map(crate::graphics::PathCommand::iter_points)
            {
                contained(
                    bounds,
                    (point.offset(-half, -half), point.offset(half, half)),
                );
            }
        }
    }
}

#[test]
fn generated_union_picture_preflight_counts_emf_and_shared_png_once() {
    // Existing producer-owned fixture is read byte-identically. Do not rewrite
    // its old native version or invent a second EMF inventory.
    let imported = Document::from_json(include_bytes!(
        "../../../../../docs/changes/fixtures/emf-import/retained-source.rsk"
    ))
    .unwrap();
    let mut source = atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-3", Mode::Auto).unwrap();
    let mut graphic = imported
        .graphics
        .iter()
        .find(|g| g.picture.is_some())
        .unwrap()
        .clone();
    graphic.id = source.next_id();
    source.graphics.push(graphic);
    let picture = source.graphics[0].picture.as_ref().unwrap().clone();
    assert!(picture.emf().is_some_and(|bytes| !bytes.is_empty()));
    let mut one_work = Work::new(usize::MAX);
    let one = Presentation::inspect(&source, &mut one_work).unwrap();
    let one_spent = usize::MAX - one_work.remaining;
    let mut duplicate = source.graphics[0].clone();
    duplicate.id = source.next_id();
    let record = std::mem::size_of::<crate::graphics::Graphic>()
        + duplicate.path.len() * std::mem::size_of::<crate::graphics::PathCommand>();
    source.graphics.push(duplicate);
    let before = source.clone();
    let mut two_work = Work::new(usize::MAX);
    let two = Presentation::inspect(&source, &mut two_work).unwrap();
    assert_eq!(usize::MAX - two_work.remaining - one_spent, record + 1);
    assert_eq!(two.copy - one.copy, record);
    let mut copies = Work::new(usize::MAX);
    two.reserve(
        &source,
        source.atoms.len(),
        drawing_size(&source),
        128,
        &mut copies,
    )
    .unwrap();
    assert_eq!(usize::MAX - copies.remaining, two.copy * 128);
    STAGES.with(|s| s.set([0; 5]));
    assert!(
        generic::expand_drawings(
            &source,
            Limits {
                max_work: picture.stored_bytes() - 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .contains("work limit")
    );
    assert_eq!(STAGES.with(std::cell::Cell::get), [0; 5]);
    assert!(
        generic::prepare_expansion_input(
            &source,
            &[],
            Limits {
                max_work: picture.stored_bytes() - 1,
                ..Limits::default()
            }
        )
        .err()
        .unwrap()
        .contains("work limit")
    );
    assert_eq!(STAGES.with(std::cell::Cell::get), [0; 5]);
    assert_eq!(source, before);
    for g in &source.graphics {
        assert_eq!(g.picture.as_ref().unwrap().png(), picture.png());
        assert_eq!(g.picture.as_ref().unwrap().emf(), picture.emf());
    }
}

#[test]
fn generated_union_keeps_fixed_maps_captions_cubic_arrows_and_rear_metadata() {
    let source = union_source();
    let before = source.clone();
    let products = generic::expand_drawings(&source, Limits::default()).unwrap();
    assert_eq!(products.len(), 3);
    for (i, product) in products.iter().enumerate() {
        same_except_positions(product.document.clone(), &raw(&source, i as u32 + 1));
        assert_eq!(product.document.atom(4), source.atom(4));
        assert_eq!(product.document.depth_appearance, source.depth_appearance);
        assert_eq!(product.document.molecule_names, source.molecule_names);
        assert_eq!(product.document.annotations, source.annotations);
        assert_eq!(product.document.arrows, source.arrows);
        union_actual_ink_contained(&product.document, product.bounds);
        legible(&crate::editing::analysis_document(
            &product.document,
            &product
                .document
                .atoms
                .iter()
                .filter(|a| a.id != 4)
                .map(|a| a.id)
                .collect::<Vec<_>>(),
        ));
    }
    assert_eq!(source, before);
}

#[test]
fn generated_union_rejects_changed_component_dependencies_atomically() {
    let mut cases = Vec::new();
    let mut named = union_source();
    let caption = named.next_id();
    named.annotations.push(crate::document::Annotation {
        id: caption,
        position: Point::new(0., 100.),
        text: "propane".into(),
        format: Default::default(),
    });
    named
        .molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: caption,
            atoms: vec![1, 2, 3],
            smiles: "CCC".into(),
            text: "propane".into(),
        });
    cases.push((named, "generated name captions"));
    let mut rear = union_source();
    rear.depth_appearance[0].atoms.push(1);
    rear.depth_appearance[0].weights.insert(1, 0.5);
    rear.depth_appearance[0].rear_weights.insert(1, 1.);
    cases.push((rear, "depth appearance scopes"));
    for target in [
        crate::arrow_anchors::Target::Atom { atom: 1 },
        crate::arrow_anchors::Target::Bond {
            a: 1,
            b: 2,
            fraction: 0.3,
        },
        crate::arrow_anchors::Target::LonePair { atom: 1, mark: 7 },
    ] {
        let mut linked = union_source();
        linked
            .atom_mut(1)
            .unwrap()
            .marks
            .push(crate::scientific::AtomMark {
                id: Some(7),
                kind: crate::scientific::MarkKind::LonePair,
                offset: Point::new(0., -20.),
                angle: 0.,
                size_pt: None,
            });
        linked.arrows[0].start_anchor.as_mut().unwrap().target = target;
        cases.push((linked, "attached arrows"));
    }
    // Alternatives change the whole component even when there is no depiction.
    let mut alternative = Document::default();
    let a = alternative.add_atom("C", Point::default());
    let b = alternative.add_atom("*", Point::new(42., 0.));
    alternative.add_bond(a, b, 1, "plain");
    alternative = atom_text::apply(&alternative, b, "[C,N]", Mode::Auto).unwrap();
    let caption = alternative.next_id();
    alternative.annotations.push(crate::document::Annotation {
        id: caption,
        position: Point::new(0., 70.),
        text: "inherited claim".into(),
        format: Default::default(),
    });
    alternative
        .molecule_names
        .push(crate::molecule_names::MoleculeName {
            annotation: caption,
            atoms: vec![a, b],
            smiles: "CC".into(),
            text: "inherited claim".into(),
        });
    cases.push((alternative, "generated name captions"));
    for (source, expected) in cases {
        let before = source.clone();
        source.validate().unwrap();
        STAGES.with(|s| s.set([0; 5]));
        assert!(
            generic::prepare_expansion(&source, Limits::default())
                .unwrap_err()
                .contains(expected)
        );
        assert_eq!(STAGES.with(std::cell::Cell::get), [0; 5]);
        assert_eq!(source, before);
    }
}

#[test]
fn prepared_union_publication_inherits_claims_without_identity_and_rejects_tampering() {
    let mut source = union_source();
    // Intentionally uninterpreted native claim: the contract transfers it, and
    // must not pretend to certify it by silently calling identifier generation.
    source.molecule_names[0].smiles = "inherited-native-claim".into();
    let before = source.clone();
    crate::molecule_names::IDENTITY_ENTRIES.with(|n| n.set(0));
    let input = generic::prepare_expansion_input(&source, &[], Limits::default()).unwrap();
    let prepared = generic::expand_prepared(input).unwrap();
    // Strict component selection preserves complete input exactly and avoids
    // ordinary delete/arrow-reconcile/name/automatic-weight paths.
    let selected =
        generic::prepare_expansion_input(&source, &[1, 2, 3], Limits::default()).unwrap();
    assert!(selected.source.molecule_names.is_empty());
    assert!(selected.source.arrows.is_empty());
    assert!(selected.source.depth_appearance.is_empty());
    assert_eq!(selected.source.generic, source.generic);
    assert_eq!(selected.source.atoms, source.atoms[..3]);
    assert!(
        generic::prepare_expansion_input(&source, &[2], Limits::default())
            .err()
            .unwrap()
            .contains("complete selected")
    );
    let mut oversized = source.clone();
    oversized.annotations.push(crate::document::Annotation {
        id: oversized.next_id(),
        position: Point::default(),
        text: "x".repeat(3_000_000),
        format: Default::default(),
    });
    STAGES.with(|s| s.set([0; 5]));
    assert!(
        generic::prepare_expansion_input(&oversized, &[], Limits::default())
            .err()
            .unwrap()
            .contains("work limit")
    );
    assert_eq!(STAGES.with(std::cell::Cell::get), [0; 5]);
    let replay = prepared.clone();
    let bounds = prepared.bounds();
    assert!(!format!("{prepared:?}").contains("oxidane"));
    let insertion = crate::transaction::take_expansion(prepared).unwrap();
    let mut doc = Document::default();
    let empty = doc.clone();
    let mut history = crate::document::History::default();
    crate::transaction::install_expansion(&mut doc, empty, insertion)
        .unwrap()
        .commit(&mut doc, &mut history, false);
    assert_eq!(
        crate::molecule_names::IDENTITY_ENTRIES.with(std::cell::Cell::get),
        0
    );
    assert!(crate::transaction::take_expansion(replay).is_err());
    assert_eq!(doc.molecule_names.len(), 3);
    assert!(
        doc.molecule_names
            .iter()
            .all(|n| n.smiles == "inherited-native-claim" && n.text == "oxidane")
    );
    union_actual_ink_contained(&doc, bounds);
    assert_eq!(source, before);
    let tile = generic::expand_drawings(&source, Limits::default())
        .unwrap()
        .remove(0)
        .document;
    let mut target = tile.clone();
    target.atoms.clear();
    target.bonds.clear();
    target.annotations.clear();
    target.graphics.clear();
    target.arrows.clear();
    target.groups.clear();
    target.depth_appearance.clear();
    target.molecule_names.clear();
    let shift = Point::new(120., 80.);
    let (part, map, _) = crate::editing::preserved_append_part(&target, &tile, shift).unwrap();
    certify_translation(&tile, &part, &map, shift).unwrap();
    crate::molecule_names::certify_inherited(&tile, &part, &map, shift).unwrap();
    for case in 0..4 {
        let mut tampered = part.clone();
        match case {
            0 => {
                tampered
                    .atoms
                    .iter_mut()
                    .find(|a| a.id == map[&4])
                    .unwrap()
                    .charge = 1
            }
            1 => tampered.annotations[0].format.style.italic = false,
            2 => tampered.arrows[0].cubic.as_mut().unwrap()[0].x += 1.,
            _ => {
                let id = map[&4];
                tampered.depth_appearance[0].rear_weights.insert(id, 0.2);
            }
        }
        assert!(certify_translation(&tile, &tampered, &map, shift).is_err());
        if case <= 1 {
            assert!(
                crate::molecule_names::certify_inherited(&tile, &tampered, &map, shift).is_err()
            );
        }
    }
    // Independent negative construction: old+shift equality alone succeeds
    // at 1e30, while every 42-unit bond collapses. Local-vector proof rejects.
    let huge = Point::new(1e30, 0.);
    let (collapsed, map, _) = crate::editing::preserved_append_part(&target, &tile, huge).unwrap();
    assert!(
        certify_translation(&tile, &collapsed, &map, huge)
            .unwrap_err()
            .contains("local geometry")
    );
    // Real native loader performs endpoint repair. Demonstrate formerly unsafe
    // Atom AND Bond under-clearance links, then prove corrected packed geometry
    // survives that same loader and a refreshed-label reconciliation unchanged.
    for bond_target in [false, true] {
        let mut unsettled = union_source();
        let target = if bond_target {
            let tail = unsettled.add_atom("C", Point::new(-208., 80.));
            unsettled.add_bond(4, tail, 1, "plain");
            unsettled.molecule_names[0].atoms.push(tail);
            crate::arrow_anchors::Target::Bond {
                a: 4,
                b: tail,
                fraction: 0.3,
            }
        } else {
            crate::arrow_anchors::Target::Atom { atom: 4 }
        };
        let center = target.center(&unsettled).unwrap();
        let direction = if bond_target {
            Point::new(0., 1.)
        } else {
            Point::new(1., 0.)
        };
        let anchor = unsettled.arrows[0].start_anchor.as_mut().unwrap();
        anchor.target = target;
        anchor.direction = direction;
        anchor.offset = Point::new(0.1, 0.1);
        anchor.gap = 1000.;
        unsettled.arrows[0].start = center.offset(0.1, 0.1);
        unsettled.validate().unwrap();
        assert!(crate::arrow_anchors::certify_resolved(&unsettled).is_err());
        let before = unsettled.clone();
        let generated = generic::prepare_expansion(&unsettled, Limits::default()).unwrap();
        let bounds = generated.bounds();
        let mut doc = generated.take().unwrap().document;
        assert!(doc.arrows.iter().all(|a| {
            a.start_anchor
                .as_ref()
                .unwrap()
                .offset
                .distance(Point::default())
                > 999.
        }));
        crate::arrow_anchors::certify_resolved(&doc).unwrap();
        union_actual_ink_contained(&doc, bounds);
        let reloaded = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        assert_eq!(reloaded, doc.current());
        union_actual_ink_contained(&reloaded, bounds);
        let labels =
            crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default()).unwrap();
        labels.apply(&mut doc);
        let before_reconcile = doc.clone();
        crate::arrow_anchors::reconcile(&mut doc);
        assert_eq!(doc, before_reconcile);
        let reloaded = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
        let mut expected = doc.current();
        crate::atom_labels::clear_computed(&mut expected);
        assert_eq!(reloaded, expected);
        union_actual_ink_contained(&reloaded, bounds);
        assert_eq!(unsettled, before);
    }
    // Saved H caches are drawing state, not chemical authority. They may be
    // larger than fresh valence H, and both immediate/reopened ink must fit.
    // A one-copy link retains the cache; a two-copy link changes chemistry and
    // correctly clears it. Element alternatives would clear both saved caches.
    let mut stale_cache =
        atom_text::apply(&chain(&["C", "C", "C"]), 2, "[CH2]1-2", Mode::Auto).unwrap();
    let oxygen = stale_cache.add_atom("O", Point::new(-200., 80.));
    let a = stale_cache.atom_mut(oxygen).unwrap();
    a.label_h = 123_456_789;
    a.display.hydrogens = Some(true);
    a.text_style = Some(crate::typography::TextStyle {
        size_pt: 144.,
        ..Default::default()
    });
    stale_cache.validate().unwrap();
    let original = stale_cache.clone();
    let prepared = generic::prepare_expansion(&stale_cache, Limits::default()).unwrap();
    assert_eq!(prepared.count(), 2);
    let bounds = prepared.bounds();
    let output = prepared.take().unwrap().document;
    assert_eq!(
        output
            .atoms
            .iter()
            .filter(|a| a.element == "O")
            .map(|a| a.label_h)
            .collect::<Vec<_>>(),
        vec![123_456_789, 0]
    );
    union_actual_ink_contained(&output, bounds);
    let reopened = Document::from_native_file(&output.file_json().unwrap()).unwrap();
    assert_eq!(reopened, output.current());
    union_actual_ink_contained(&reopened, bounds);
    assert_eq!(stale_cache, original);
    // A genuine post-install refresh can restore displayed CIP on an unrelated
    // specified component. Potential glyph envelopes must be prepaid and fit
    // even when the installed/native cache itself has no descriptor yet.
    let mut future_cip = chain(&["C", "*"]);
    future_cip = atom_text::apply(&future_cip, 2, "[Cl,Br]", Mode::Auto).unwrap();
    let center = future_cip.add_atom("C", Point::new(-240., 80.));
    let mut neighbors = Vec::new();
    for (element, x, y) in [
        ("F", -198., 80.),
        ("Cl", -240., 122.),
        ("Br", -282., 80.),
        ("I", -240., 38.),
    ] {
        let id = future_cip.add_atom(element, Point::new(x, y));
        future_cip.add_bond(center, id, 1, "plain");
        neighbors.push(id);
    }
    let center_atom = future_cip.atom_mut(center).unwrap();
    center_atom.stereo = Some(crate::document::AtomStereo {
        winding: "cw".into(),
        neighbors,
    });
    center_atom.display.stereo.show = Some(true);
    center_atom.display.stereo.offset = Some(Point::new(1000., 1000.));
    center_atom.display.stereo.style = crate::typography::TextStyle {
        family: "Times New Roman".into(),
        size_pt: 144.,
        italic: true,
        underline: true,
        ..Default::default()
    };
    // Cl and N outrank the implicit H at their respective alkene ends.
    // Both carriers lie on the same side of C=C: this is independently Z.
    let mut alkene = Vec::new();
    for (element, x, y) in [
        ("Cl", -360., -118.),
        ("C", -318., -160.),
        ("C", -276., -160.),
        ("N", -234., -118.),
    ] {
        alkene.push(future_cip.add_atom(element, Point::new(x, y)));
    }
    future_cip.add_bond(alkene[0], alkene[1], 1, "plain");
    future_cip.add_bond(alkene[1], alkene[2], 2, "plain");
    future_cip.add_bond(alkene[2], alkene[3], 1, "plain");
    let double = future_cip
        .bonds
        .iter_mut()
        .find(|b| b.a == alkene[1] && b.b == alkene[2])
        .unwrap();
    // Native stereo tags are lowercase; uppercase Z is a derived CIP label.
    double.stereo = Some("z".into());
    double.stereo_authoritative = true;
    double.stereo_atoms = vec![alkene[0], alkene[3]];
    double.indicator.show = Some(true);
    double.indicator.offset = Some(Point::new(-1000., -1000.));
    double.indicator.style = crate::typography::TextStyle {
        size_pt: 144.,
        italic: true,
        underline: true,
        ..Default::default()
    };
    future_cip.validate().unwrap();
    let original = future_cip.clone();
    let prepared = generic::prepare_expansion(&future_cip, Limits::default()).unwrap();
    let bounds = prepared.bounds();
    let mut output = prepared.take().unwrap().document;
    assert!(output.atoms.iter().all(|a| a.cip_label.is_none()));
    assert!(output.bonds.iter().all(|b| b.cip_label.is_none()));
    let fresh =
        crate::atom_labels::refresh::Refresh::calculate(&output, &Default::default()).unwrap();
    assert!(fresh.notice.is_none(), "{:?}", fresh.notice);
    fresh.apply(&mut output);
    assert!(output.atoms.iter().any(|a| a.cip_label.is_some()));
    assert_eq!(
        output
            .bonds
            .iter()
            .filter(|b| b.order == 2)
            .map(|b| b.cip_label.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Z"), Some("Z")]
    );
    union_actual_ink_contained(&output, bounds);
    let reopened = Document::from_native_file(&output.file_json().unwrap()).unwrap();
    union_actual_ink_contained(&reopened, bounds);
    assert_eq!(future_cip, original);
    let poisoned = generic::prepare_expansion(&source, Limits::default()).unwrap();
    let handle = poisoned.clone();
    assert!(
        std::panic::catch_unwind(move || {
            let _held = handle.data.lock().unwrap();
            panic!("intentional transport poison");
        })
        .is_err()
    );
    assert!(crate::transaction::take_expansion(poisoned).is_err());
    assert_eq!(doc.molecule_names.len(), 3);
    assert_eq!(source, before);
}

#[test]
fn native_variable_arene_six_drawings_fit_default_work_and_measured_ink() {
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
    let input = generic::prepare_expansion_input(&source, &[], Limits::default()).unwrap();
    let prepared = generic::expand_prepared(input).unwrap();
    assert_eq!(prepared.count(), 6);
    let bounds = prepared.bounds();
    let output = prepared.take().unwrap().document;
    assert_eq!((output.atoms.len(), output.bonds.len()), (48, 48));
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
        let selected = part.atoms.get(choice).unwrap();
        assert_eq!(oxygen.element, "O");
        let attached: HashSet<_> = neighbors(&part, oxygen.id).into_iter().collect();
        assert_eq!(attached, HashSet::from([selected.id, methyl.id]));
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

#[test]
fn ink_precharge_skips_disabled_glyphs_but_pays_owned_styles_and_enabled_paths() {
    let mut source = chain(&["C", "O"]);
    source.atom_mut(1).unwrap().text_style = Some(Default::default());
    let spent = |doc: &Document| {
        let mut work = Work::new(usize::MAX);
        charge_ink_work(doc, &mut work).unwrap();
        usize::MAX - work.remaining
    };
    let baseline = spent(&source);
    let mut hidden = source.clone();
    let carbon = hidden.atom_mut(1).unwrap();
    carbon.text_style.as_mut().unwrap().family = "x".repeat(64);
    carbon.display.mapping.style.family = "x".repeat(64);
    carbon.display.stereo.style.family = "x".repeat(64);
    hidden.bonds.first_mut().unwrap().indicator.style.family = "x".repeat(64);
    hidden.validate().unwrap();
    assert_eq!(atom_bounds(hidden.atom(1).unwrap(), &hidden).unwrap(), None);
    assert_eq!(spent(&hidden), baseline);
    // Hidden text is still owned native state. Presentation inspection pays its
    // storage/copies even when the glyph path never reads that font family.
    let owned = |doc: &Document| {
        let mut work = Work::new(usize::MAX);
        Presentation::inspect(doc, &mut work).unwrap().copy
    };
    assert!(owned(&hidden) > owned(&source));
    let mut shown_carbon = hidden.clone();
    shown_carbon.atom_mut(1).unwrap().display.carbons = Some(crate::atom_labels::Carbons::All);
    let mut shown_map = hidden.clone();
    shown_map.atom_mut(1).unwrap().map_num = 17;
    shown_map.atom_mut(1).unwrap().display.mapping.show = Some(true);
    let mut shown_atom_cip = hidden.clone();
    shown_atom_cip.atom_mut(1).unwrap().display.stereo.show = Some(true);
    let mut shown_bond_cip = hidden.clone();
    shown_bond_cip.bonds.first_mut().unwrap().indicator.show = Some(true);
    let mut shown_global_cip = hidden.clone();
    shown_global_cip.atom_labels.stereo = true;
    for shown in [
        shown_carbon,
        shown_map,
        shown_atom_cip,
        shown_bond_cip,
        shown_global_cip,
    ] {
        let before = shown.clone();
        assert!(spent(&shown) > baseline);
        let mut work = Work::new(baseline);
        assert!(
            charge_ink_work(&shown, &mut work)
                .unwrap_err()
                .contains("work limit")
        );
        assert_eq!(shown, before);
    }
}

#[test]
fn materialized_visible_element_is_charged_before_label_measurement() {
    let source = atom_text::apply(&chain(&["C", "C"]), 2, "[C,N]", Mode::Auto).unwrap();
    let before = source.clone();
    let products = generic::expand(&source, Limits::default()).unwrap();
    let carbon = products
        .iter()
        .find(|d| d.atom(2).unwrap().element == "C")
        .unwrap();
    let nitrogen = products
        .iter()
        .find(|d| d.atom(2).unwrap().element == "N")
        .unwrap();
    assert_eq!(atom_bounds(carbon.atom(2).unwrap(), carbon).unwrap(), None);
    assert!(
        atom_bounds(nitrogen.atom(2).unwrap(), nitrogen)
            .unwrap()
            .is_some()
    );
    let mut work = Work::new(usize::MAX);
    charge_ink_work(carbon, &mut work).unwrap();
    let hidden_cost = usize::MAX - work.remaining;
    let mut limited = Work::new(hidden_cost);
    assert!(
        charge_ink_work(nitrogen, &mut limited)
            .unwrap_err()
            .contains("work limit")
    );
    let prepared = generic::prepare_expansion(&source, Limits::default()).unwrap();
    let bounds = prepared.bounds();
    let output = prepared.take().unwrap().document;
    union_actual_ink_contained(&output, bounds);
    assert_eq!(source, before);
}
