use super::*;
use crate::generic::drawing_test_support::{chain, raw, union_actual_ink_contained};
use crate::generic::{
    certify::preserve,
    depiction::Plan,
    ink::atom_bounds,
    probe::{self, Stage},
    references::protect_references,
};
use crate::{
    atom_text::{self, Mode},
    document::Point,
    generic::{self, Limits},
};
use std::collections::HashSet;

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
    assert!(probe::counts().iter().take(5).all(|count| *count == 0));
    let anchors = HashSet::from([2]);
    let candidate = raw(&source, 1);
    let mut probe = Work::new(usize::MAX);
    Plan::new(&candidate, &anchors, &mut probe).unwrap();
    let reserved = usize::MAX - probe.remaining;
    let mut short = Work::new(reserved - 1);
    assert!(Plan::new(&candidate, &anchors, &mut short).is_err());
    assert!(probe::counts().iter().take(5).all(|count| *count == 0));
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
    charge_validation(&source, &mut upfront).unwrap();
    protect_references(&source, &mut upfront).unwrap();
    upfront
        .charge(drawing_size(&source) * 11 + 9 * 3 * 20)
        .unwrap();
    presentation
        .reserve(&source, 5, 9, 4 * 2 + 3 * 8, &mut upfront)
        .unwrap();
    // All candidates validate during enumeration, before finishing any tile.
    for count in 1..=3 {
        charge_validation(&raw(&source, count), &mut upfront).unwrap();
    }
    let upfront = usize::MAX - upfront.remaining;
    let mut completed = Work::new(Limits::default().max_work);
    for count in 1..=2 {
        let mut product = raw(&source, count);
        charge_validation(&product, &mut completed).unwrap();
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
    assert!(
        probe::counts().iter().take(5).all(|count| *count == 0),
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
fn generated_union_picture_preflight_counts_emf_and_shared_png_once() {
    // Existing producer-owned fixture is read byte-identically. Do not rewrite
    // its old native version or invent a second EMF inventory.
    let imported = Document::from_json(include_bytes!(
        "../../../../../tests/fixtures/emf-import/retained-source.rsk"
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
    assert!(probe::counts().iter().take(5).all(|count| *count == 0));
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
    assert!(probe::counts().iter().take(5).all(|count| *count == 0));
    assert_eq!(source, before);
    for g in &source.graphics {
        assert_eq!(g.picture.as_ref().unwrap().png(), picture.png());
        assert_eq!(g.picture.as_ref().unwrap().emf(), picture.emf());
    }
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
    let output = prepared.take().unwrap();
    union_actual_ink_contained(&output, bounds);
    assert_eq!(source, before);
}
