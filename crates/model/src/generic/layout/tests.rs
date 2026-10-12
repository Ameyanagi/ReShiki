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
    probe::reset();
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
    assert_eq!(probe::counts(), [0; 5]);
    let anchors = HashSet::from([2]);
    let candidate = raw(&source, 1);
    let mut probe = Work::new(usize::MAX);
    Plan::new(&candidate, &anchors, &mut probe).unwrap();
    let reserved = usize::MAX - probe.remaining;
    let mut short = Work::new(reserved - 1);
    assert!(Plan::new(&candidate, &anchors, &mut short).is_err());
    assert_eq!(probe::counts(), [0; 5]);
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
    probe::reset();
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
    let stages = probe::counts();
    assert_eq!(
        probe::count(Stage::Prepare),
        2,
        "the aggregate reservation must prevent the third product's preparation: {stages:?}"
    );
    assert_eq!(
        probe::count(Stage::Measure),
        2,
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
    probe::reset();
    assert!(
        generic::expand_drawings(&source, Limits::default())
            .unwrap_err()
            .contains("work limit")
    );
    assert_eq!(
        probe::counts(),
        [0; 5],
        "reject before seed cloning, materialization, preparation, depiction or ink measurement"
    );
    assert_eq!(source, before);
    // Drawing presentation costs do not change graph enumeration's limits.
    // The 32 binary words on five alternating path sites have only reversal
    // symmetry: eight palindromes, hence (32+8)/2 = 20 distinct structures.
    // Derive the expected representatives independently of the deduplicator.
    let canonical = |word: String| {
        let reverse: String = word.chars().rev().collect();
        word.min(reverse)
    };
    let expected: HashSet<_> = (0..32)
        .map(|bits| {
            canonical(
                (0..5)
                    .map(|i| if bits & (1 << i) == 0 { 'C' } else { 'N' })
                    .collect(),
            )
        })
        .collect();
    assert_eq!(expected.len(), (32 + 8) / 2);
    let products = generic::expand(&source, Limits::default()).unwrap();
    let actual: HashSet<_> = products
        .iter()
        .map(|p| {
            canonical(
                [1, 3, 5, 7, 9]
                    .iter()
                    .map(|id| p.atom(*id).unwrap().element.as_str())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(products.len(), expected.len());
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
    probe::reset();
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
    assert_eq!(probe::counts(), [0; 5]);
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
    assert_eq!(probe::counts(), [0; 5]);
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
        probe::reset();
        assert!(
            generic::prepare_expansion(&source, Limits::default())
                .unwrap_err()
                .contains(expected)
        );
        assert_eq!(probe::counts(), [0; 5]);
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
    probe::reset();
    assert!(
        generic::prepare_expansion_input(&oversized, &[], Limits::default())
            .err()
            .unwrap()
            .contains("work limit")
    );
    assert_eq!(probe::counts(), [0; 5]);
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
    let output = prepared.take().unwrap().document;
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

#[test]
fn clearance_certificate_refuses_touch_collinearity_and_review_crossing() {
    for x in [0., 0.00021] {
        assert!(
            !Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 0.).separated(
                Footprint::segment(Point::new(x, -1.), Point::new(x, 1.), 0.)
            )
        );
    }
    assert!(
        !Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 0.).separated(
            Footprint::segment(Point::new(21., 0.), Point::new(63., 0.), 0.)
        )
    );
    assert!(
        Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 1.).separated(
            Footprint::segment(Point::new(0., 3.), Point::new(42., 3.), 1.)
        )
    );
    assert!(
        !Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 1.).separated(
            Footprint::segment(Point::new(0., 2.), Point::new(42., 2.), 1.)
        )
    );
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
