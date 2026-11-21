use super::super::names::arrow_hits;
use super::*;
use crate::cycles::split_runs::SplitStation;
use crate::{
    cycles::{self, Cycle, Role, Shape, Step},
    document::Annotation,
    graphics::{GraphicKind, GraphicStyle},
    typography::TextFormat,
};

fn fixture() -> (Document, Vec<u64>, u64) {
    let doc = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/cycles/suzuki-rectangle-bow180-target.rsk"
    ))
    .unwrap();
    let id = doc.graphics.iter().find(|g| g.cycle.is_some()).unwrap().id;
    (doc, (137..=143).collect(), id)
}

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}
fn center(doc: &Document, part: &[u64]) -> Point {
    center_of(crate::scene::selection_bounds(doc, part).unwrap())
}
fn main_arrows(doc: &Document, id: u64, steps: &[usize]) -> Vec<Arrow> {
    let drawn = cycles::drawn(doc, graphic(doc, id));
    drawn
        .arrows
        .into_iter()
        .zip(drawn.arcs)
        .filter_map(|(a, s)| s.filter(|s| steps.contains(s)).map(|_| a))
        .collect()
}
fn applied() -> (Document, Document, Vec<u64>, u64) {
    let (before, part, id) = fixture();
    let proposal = cycles::propose_drop(&before, &part)
        .expect("captured bowed insertion must produce a clear candidate");
    assert_eq!(proposal.target.role, Role::Intermediate(2));
    assert_eq!(
        proposal
            .doc
            .graphics
            .iter()
            .find(|g| g.id == id)
            .unwrap()
            .cycle
            .as_ref()
            .unwrap()
            .split_runs
            .len(),
        1
    );
    (before, proposal.doc, part, id)
}

fn text_at(doc: &mut Document, at: Point, text: &str) -> u64 {
    let id = doc.next_id();
    let mut text = Annotation {
        id,
        position: Point::default(),
        text: text.into(),
        angle: 0.,
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    };
    let (w, h) = text.size();
    text.position = at.offset(-w * 0.5, -h * 0.5);
    doc.annotations.push(text);
    id
}

fn capture_station(doc: &Document, id: u64, step: usize, u: f64) -> SplitStation {
    let g = graphic(doc, id);
    let cycle = g.cycle.as_ref().unwrap();
    let outline = cycles::outline(g).unwrap();
    let bounds = crate::scene::selections_bounds(doc, &cycle.intermediates);
    let end = |slot: usize| {
        let ink = bounds[slot];
        (outline.project(center_of(ink.unwrap())).unwrap().s, ink)
    };
    let p = super::parameters(
        doc,
        g,
        &outline,
        &cycle.steps[step],
        [end(step), end((step + 1) % cycle.intermediates.len())],
        cycle.intermediates.len() == 1,
    )
    .unwrap();
    SplitStation {
        u,
        start: p.start.rem_euclid(outline.length()) / outline.length(),
        span: p.length / outline.length(),
    }
}

// A source consisting of one horizontal native edge makes the lane-only
// miss independent of Bow curvature, text metrics and projection rounding.
fn straight_split() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let id = 1;
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Ellipse,
        Point::new(-600., -200.),
        Point::new(600., 200.),
        GraphicStyle::default(),
        Default::default(),
        false,
    );
    g.cycle = Some(Box::new(Cycle {
        shape: Shape::Rectangle,
        ..Default::default()
    }));
    doc.graphics.push(g);
    let a = text_at(&mut doc, Point::new(-400., -200.), "Pd");
    let b = text_at(&mut doc, Point::new(400., -200.), "Pd");
    let cut = text_at(&mut doc, Point::new(0., -200.), "X");
    {
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        cycle.intermediates = vec![vec![a], vec![b]];
        cycle.steps = vec![
            Step {
                reversible: true,
                ..Default::default()
            },
            Step::default(),
        ];
    }
    let station = capture_station(&doc, id, 0, 0.5);
    assert!(doc.graphics[0].cycle.as_mut().unwrap().insert_intermediate(
        1,
        vec![cut],
        Some(station)
    ));
    doc.group_selection(&[id, a, b, cut]).unwrap();
    assert!(valid(&doc, id));
    doc.validate().unwrap();
    (doc, cut, id)
}

fn forward_only_delta(doc: &Document, id: u64, cut: u64) -> Point {
    let g = graphic(doc, id);
    let cycle = g.cycle.as_ref().unwrap();
    let layout = Layout::graphic(doc, g).unwrap();
    let source = layout.sources[0].as_ref().unwrap();
    let at = source.profiles.center.point(source.cuts[0]).unwrap();
    let forward = source
        .profiles
        .forward
        .as_ref()
        .unwrap()
        .point(source.cuts[0])
        .unwrap();
    let reverse = source
        .profiles
        .reverse
        .as_ref()
        .unwrap()
        .point(source.cuts[0])
        .unwrap();
    assert!(forward.y < at.y && at.y < reverse.y);
    let ink = crate::scene::selection_bounds(doc, &[cut]).unwrap();
    // Put the grown ink's bottom edge strictly between the two source lanes.
    Point::new(0., at.y - ink.1.y - DEFAULT.world(cycle.gap_pt))
}

fn far_off_split() -> (Document, u64, u64) {
    let (mut doc, cut, id) = straight_split();
    doc.translate(&[cut], 80., 2000.);
    // This remains valid stored topology, but the actual member box misses
    // the frozen lane regardless of its stored logical station.
    doc.graphics[0].cycle.as_mut().unwrap().split_runs[0].cuts[0] = 0.25;
    graphic(&doc, id)
        .cycle
        .as_ref()
        .unwrap()
        .validate()
        .unwrap();
    assert!(!valid(&doc, id));
    (doc, cut, id)
}

fn cached_outer_straight_split() -> (Document, u64, u64, u64) {
    let (mut doc, cut, id) = straight_split();
    let old_outer = {
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        let _ = cycle.remove_intermediate(1);
        cycle.intermediates[0].clone()
    };
    let molecule = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap();
    let outer = crate::editing::append_preserving_layout(&mut doc, &molecule, Point::default());
    let centre = molecule
        .all_ids()
        .into_iter()
        .zip(outer.iter().copied())
        .find(|(id, _)| *id == 2)
        .unwrap()
        .1;
    doc.graphics[0].cycle.as_mut().unwrap().intermediates[0] = outer.clone();
    doc.annotations.retain(|a| !old_outer.contains(&a.id));
    doc.groups.clear();
    doc.atom_labels.stereo = true;
    doc.atom_mut(centre).unwrap().display.stereo.show = Some(true);
    doc.atom_mut(centre).unwrap().display.stereo.offset = Some(Point::new(120., 0.));
    let from = center_of(
        crate::scene::primary_selections_bounds(
            &nominal_document(&doc),
            std::slice::from_ref(&outer),
        )
        .pop()
        .flatten()
        .unwrap(),
    );
    doc.translate(&outer, -400. - from.x, -200. - from.y);
    crate::atom_labels::refresh::Refresh::calculate(&doc, &Default::default())
        .unwrap()
        .apply(&mut doc);
    assert_eq!(doc.atom(centre).unwrap().cip_label.as_deref(), Some("R"));
    let station = capture_station(&doc, id, 0, 0.5);
    let target = {
        let g = graphic(&doc, id);
        let outline = cycles::outline(g).unwrap();
        deform::profiles(
            id,
            &outline,
            station.start * outline.length(),
            station.span * outline.length(),
            0.,
            true,
            &arrow_style(g),
        )
        .unwrap()
        .center
        .point(station.u)
        .unwrap()
    };
    let from = center(&doc, &[cut]);
    doc.translate(&[cut], target.x - from.x, target.y - from.y);
    assert!(doc.graphics[0].cycle.as_mut().unwrap().insert_intermediate(
        1,
        vec![cut],
        Some(station)
    ));
    let members: Vec<_> = std::iter::once(id)
        .chain(
            graphic(&doc, id)
                .cycle
                .as_ref()
                .unwrap()
                .intermediates
                .iter()
                .flatten()
                .copied(),
        )
        .collect();
    doc.group_selection(&members).unwrap();
    doc.validate().unwrap();
    (doc, cut, id, centre)
}

fn add_plain_opposite_run(doc: &mut Document, id: u64) {
    let station = capture_station(doc, id, 2, 0.5);
    let target = {
        let g = graphic(doc, id);
        let outline = cycles::outline(g).unwrap();
        deform::profiles(
            id,
            &outline,
            station.start * outline.length(),
            station.span * outline.length(),
            0.,
            false,
            &arrow_style(g),
        )
        .unwrap()
        .center
        .point(station.u)
        .unwrap()
    };
    let cut = text_at(doc, target, "Y");
    assert!(doc.graphics[0].cycle.as_mut().unwrap().insert_intermediate(
        3,
        vec![cut],
        Some(station)
    ));
    for group in doc.groups.iter_mut().filter(|g| g.members.contains(&id)) {
        group.members.push(cut);
    }
    assert_eq!(
        graphic(doc, id)
            .cycle
            .as_ref()
            .unwrap()
            .split_runs
            .iter()
            .map(|r| r.first_step)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    doc.validate().unwrap();
}

fn outside_openings(doc: &Document, id: u64, p: Point) -> bool {
    let c = graphic(doc, id).cycle.as_ref().unwrap();
    c.intermediates.iter().all(|ids| {
        crate::scene::selection_bounds(doc, ids).is_none_or(|ink| {
            !crate::cycles::contains(grown(ink, DEFAULT.world(c.gap_pt + 12.)), p)
        })
    })
}

fn same_locus(original: &[Arrow], children: &[Arrow], doc: &Document, id: u64) {
    let shafts: Vec<_> = children
        .iter()
        .filter_map(crate::cycles::shapes::Shaft::from_arrow)
        .collect();
    let mut checked = 0;
    for old in original
        .iter()
        .filter_map(crate::cycles::shapes::Shaft::from_arrow)
    {
        for i in 1..80 {
            let p = old.sample(old.length() * i as f64 / 80., false).unwrap().0;
            if !outside_openings(doc, id, p) {
                continue;
            }
            let distance = shafts
                .iter()
                .filter_map(|s| s.project(p))
                .map(|p| p.distance)
                .fold(f64::INFINITY, f64::min);
            assert!(
                distance < 0.05,
                "parent point{p:?} left child ink by{distance}"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 10,
        "native curve comparison must exercise painted ink"
    );
}

#[test]
fn fit_moved_without_fitted_sources_keeps_noops_and_ordinary_moves() {
    for before in [Document::default(), fixture().0] {
        assert!(!cycles::has_fits(&before));
        let ids = before.all_ids();
        let mut after = before.clone();
        assert!(!cycles::fit_moved(&before, &mut after, &ids));
        assert_eq!(after, before);
        if ids.is_empty() {
            continue;
        }
        after.translate(&ids, 37., -21.);
        let expected = after.clone();
        assert_ne!(expected, before, "the ordinary move must actually run");
        assert!(cycles::fit_moved(&before, &mut after, &ids));
        assert_eq!(after, expected, "an unfitted move needs no layout repair");
        after.validate().unwrap();
    }
}

#[test]
fn fit_moved_keeps_fitted_noops_and_source_presence_transitions() {
    let (fitted, _, id) = straight_split();
    let mut same = fitted.clone();
    assert!(!cycles::fit_moved(&fitted, &mut same, &[]));
    assert_eq!(same, fitted);

    let mut plain = fitted.clone();
    plain.graphics[0].cycle.as_mut().unwrap().split_runs.clear();
    assert!(!cycles::has_fits(&plain));
    for (before, mut after) in [(plain.clone(), fitted.clone()), (fitted, plain)] {
        let expected = after.clone();
        assert!(cycles::fit_moved(&before, &mut after, &[id]));
        assert_eq!(after, expected, "keep an added or retired source intact");
        after.validate().unwrap();
    }
}

#[test]
fn captured_bowed_forward_lane_is_cut_without_rebowing_and_clears_actual_member_ink() {
    let (before, after, part, id) = applied();
    let parent = main_arrows(&before, id, &[1]);
    let children = main_arrows(&after, id, &[1, 2]);
    same_locus(&parent[..1], &children, &after, id);
    let body = crate::scene::selection_bounds(&after, &part).unwrap();
    let gap = DEFAULT.world(graphic(&after, id).cycle.as_ref().unwrap().gap_pt);
    let ink = grown(body, gap);
    let first = main_arrows(&after, id, &[1]);
    let next = main_arrows(&after, id, &[2]);
    assert_eq!(first.len(), 2);
    assert_eq!(next.len(), 1);
    assert!(
        first
            .iter()
            .all(|a| matches!(a.appearance().head, Head::Left | Head::Right))
    );
    assert!(
        children.iter().any(|a| a.paths().iter().any(|p| p.filled)),
        "actual arrowhead ink must be checked"
    );
    for arrow in &children {
        assert!(
            !arrow_hits(arrow, body, (gap - 0.05).max(0.)),
            "painted shaft or head wing enters the requested member gap"
        );
    }
    for (p, arrow) in [(first[0].end, &first[0]), (next[0].start, &next[0])] {
        let style = arrow.appearance();
        let head_envelope = DEFAULT
            .world(style.head_length_pt)
            .hypot(DEFAULT.world(style.head_width_pt))
            + DEFAULT.world(style.width_pt);
        assert!(
            crate::cycles::contains(grown(ink, head_envelope + 0.05), p),
            "stroke-safe child crop moved beyond its local head envelope"
        );
        assert!(
            !crate::cycles::contains(grown(ink, -0.05), p),
            "child centerline enters the requested gap"
        );
    }
    assert_eq!(after.atoms.len(), before.atoms.len());
    assert_eq!(after.bonds, before.bonds);
    assert_eq!(center(&after, &part), center(&before, &part));
}

#[test]
fn cut_holds_requires_every_lane_on_both_adjacent_children() {
    let (source, cut, id) = straight_split();
    let delta = forward_only_delta(&source, id, cut);
    for incoming_reversible in [true, false] {
        let mut doc = source.clone();
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        cycle.steps[0].reversible = incoming_reversible;
        cycle.steps[1].reversible = !incoming_reversible;
        doc.translate(&[cut], delta.x, delta.y);
        let layout = Layout::graphic(&doc, graphic(&doc, id)).unwrap();
        let opening = &layout.sources[0].as_ref().unwrap().openings[0];
        assert!(
            opening[1].is_some(),
            "the displayed forward lane still meets the cut"
        );
        assert!(
            opening[2].is_none(),
            "the required reverse lane must actually miss"
        );
        assert_eq!(
            layout.cut_intersects(1),
            Some(false),
            "either adjacent reversible child requires both openings"
        );
        assert!(!cycles::holds(&doc, id, &[cut]));
    }
}

#[test]
fn a_sideways_drag_that_only_meets_one_lane_takes_out_and_coalesces() {
    let (before, cut, id) = straight_split();
    let delta = forward_only_delta(&before, id, cut);
    let mut after = before.clone();
    after.translate(&[cut], delta.x, delta.y);
    cycles::fit_moved(&before, &mut after, &[cut]);
    assert!(
        after
            .take_out(&[cut], delta)
            .contains(&crate::grouping::Container::Cycle)
    );
    assert!(cycles::role_of(&after, cut).is_none());
    let cycle = graphic(&after, id).cycle.as_ref().unwrap();
    assert!(cycle.split_runs.is_empty());
    assert_eq!(cycle.steps.len(), 2);
    assert!(cycle.steps[0].reversible);
    assert_eq!(main_arrows(&after, id, &[0]).len(), 2);
    assert_eq!(main_arrows(&after, id, &[1]).len(), 1);
    for arrow in main_arrows(&after, id, &[0, 1]) {
        assert!(arrow.validate().is_ok());
        assert!(
            crate::cycles::shapes::Shaft::from_arrow(&arrow)
                .unwrap()
                .length()
                > 1.
        );
    }
    let moved = center(&after, &[cut]);
    let from = center(&before, &[cut]);
    assert!(
        moved.distance(from.offset(delta.x, delta.y)) < 0.01,
        "a detached member keeps the user's drag position"
    );
    after.validate().unwrap();
}

#[test]
fn intersecting_openings_do_not_hold_a_cut_after_either_child_loses_positive_length() {
    for incoming in [true, false] {
        let (mut doc, _, id) = straight_split();
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        if incoming {
            cycle.steps[0].trail_pt = 720.;
        } else {
            cycle.steps[1].lead_pt = 720.;
        }
        let layout = Layout::graphic(&doc, graphic(&doc, id)).unwrap();
        let opening = &layout.sources[0].as_ref().unwrap().openings[0];
        assert!(opening[1].is_some() && opening[2].is_some());
        assert!(
            layout.steps[usize::from(!incoming)].is_none(),
            "the chosen adjacent child must be exhausted"
        );
        assert_eq!(
            layout.cut_intersects(1),
            Some(false),
            "a cut needs positive native arrows on both sides"
        );
        assert!(!valid(&doc, id));
    }
}

#[test]
fn schema_valid_far_off_cut_rejects_load_and_reconcile_retires_and_reflows_it() {
    let (mut doc, cut, id) = far_off_split();
    let anchors: Vec<_> = doc
        .annotations
        .iter()
        .filter(|a| a.id != cut)
        .cloned()
        .collect();
    let error = Document::from_json(&doc.file_json().unwrap())
        .unwrap_err()
        .to_ascii_lowercase();
    assert!(
        error.contains("cycle") && error.contains("fitted") && error.contains("route"),
        "load must explain the undrawable fitted route: {error}"
    );
    cycles::reconcile(&mut doc);
    let cycle = graphic(&doc, id).cycle.as_ref().unwrap();
    assert!(cycle.split_runs.is_empty());
    assert_eq!(cycle.intermediates.len(), 3);
    assert_eq!(
        cycles::role_of(&doc, cut),
        Some((id, Role::Intermediate(1)))
    );
    assert!(
        cycles::outline(graphic(&doc, id))
            .unwrap()
            .project(center(&doc, &[cut]))
            .unwrap()
            .distance
            < 0.05
    );
    for anchor in anchors {
        assert_eq!(
            doc.annotations.iter().find(|a| a.id == anchor.id),
            Some(&anchor)
        );
    }
    for step in 0..3 {
        let arrows = main_arrows(&doc, id, &[step]);
        assert_eq!(
            arrows.len(),
            if step == 0 { 2 } else { 1 },
            "retirement cannot leave an invisible child step"
        );
        for arrow in arrows {
            assert!(
                crate::cycles::shapes::Shaft::from_arrow(&arrow)
                    .unwrap()
                    .length()
                    > 1.
            );
        }
    }
    doc.validate().unwrap();
    assert!(Document::from_json(&doc.file_json().unwrap()).is_ok());
}

#[test]
fn an_exhausted_child_edit_declines_without_partial_member_or_station_updates() {
    let (_, before, _, id) = applied();
    let mut doc = before.clone();
    doc.graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_mut()
        .unwrap()
        .steps[2]
        .lead_pt = 720.;
    assert!(cycles::fit_edited(&before, &mut doc, id, None).is_err());
    assert_eq!(
        doc, before,
        "an impossible edit must not partially move members or rewrite stored source stations"
    );
}

#[test]
fn no_op_child_bow_or_lane_setting_keeps_the_fitted_source_exact() {
    let (_, before, _, id) = applied();
    for bow in [None, Some(1), Some(2)] {
        let mut after = before.clone();
        assert!(!cycles::fit_edited(&before, &mut after, id, bow).unwrap());
        assert_eq!(after, before);
    }
}

#[test]
fn display_opening_joins_only_a_nearby_indicator_interval_on_its_native_lane() {
    let (doc, _, id) = straight_split();
    let layout = Layout::graphic(&doc, graphic(&doc, id)).unwrap();
    let source = layout.sources[0].as_ref().unwrap();
    let profile = &source.profiles.center;
    let station = source.cuts[0];
    let center = profile.point(station).unwrap();
    let box_at = |lo, hi| (center.offset(lo, -5.), center.offset(hi, 5.));
    let boxes = [box_at(-5., 5.), box_at(20., 30.), box_at(70., 80.)];
    let plain = connected_opening(profile, station, (0., 1.), &boxes, 0.).unwrap();
    let local = connected_opening(profile, station, (0., 1.), &boxes, 20.).unwrap();
    assert!((profile.point(plain.1).unwrap().x - (center.x + 5.)).abs() < 0.001);
    assert!((profile.point(local.1).unwrap().x - (center.x + 30.)).abs() < 0.001);
    assert!(
        profile.point(local.1).unwrap().x < center.x + 70.,
        "the distant interval cannot widen the local opening"
    );
}

#[test]
fn distant_mapping_ink_does_not_fill_the_clear_corridor_to_its_molecule() {
    let mut doc = Document::default();
    let atom = doc.add_atom("N", Point::default());
    doc.atom_labels.maps = true;
    let a = doc.atom_mut(atom).unwrap();
    a.map_num = 11;
    a.display.mapping.offset = Some(Point::new(0., -500.));
    let union = crate::scene::selection_bounds(&doc, &[atom]).unwrap();
    let middle = center_of(union);
    let arrow = Arrow::new(
        doc.next_id(),
        middle.offset(-100., 0.),
        middle.offset(100., 0.),
        Default::default(),
        Default::default(),
    );
    assert!(super::super::names::arrow_hits(&arrow, union, 0.));
    let ink = part_ink(&doc, &[atom]);
    assert!(ink.len() >= 2, "body and map indicator retain separate ink");
    assert!(
        ink.iter()
            .all(|ink| !super::super::names::arrow_hits(&arrow, *ink, 0.)),
        "a route through the empty corridor must not hit an artificial union box"
    );
}

#[test]
fn reverse_fit_keeps_frozen_sources_with_changed_outer_cache_and_swapped_run_slots() {
    for two_runs in [false, true] {
        let (mut before, _, id, centre) = cached_outer_straight_split();
        if two_runs {
            add_plain_opposite_run(&mut before, id);
        }
        let captured_ink = crate::scene::selection_bounds(
            &before,
            &graphic(&before, id).cycle.as_ref().unwrap().intermediates[0],
        );
        crate::atom_labels::clear_computed(&mut before);
        assert!(before.atom(centre).unwrap().cip_label.is_none());
        assert_ne!(
            captured_ink,
            crate::scene::selection_bounds(
                &before,
                &graphic(&before, id).cycle.as_ref().unwrap().intermediates[0]
            )
        );
        let cycle = graphic(&before, id).cycle.as_ref().unwrap().clone();
        let samples = |doc: &Document| {
            let layout = Layout::graphic(doc, graphic(doc, id)).unwrap();
            layout
                .sources
                .iter()
                .flat_map(|source| {
                    let source = source.as_ref().unwrap();
                    [
                        Some(&source.profiles.center),
                        source.profiles.forward.as_ref(),
                        source.profiles.reverse.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                    .flat_map(|profile| {
                        [0., 0.125, 0.35, 0.5, 0.7, 0.875, 1.].map(|u| profile.point(u).unwrap())
                    })
                    .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let original_samples = samples(&before);
        let mut expected = cycle.split_runs.clone();
        let n = cycle.steps.len();
        for run in &mut expected {
            run.first_step = (n - (run.first_step + run.cuts.len() + 1) % n) % n;
            let start = (run.source_start + run.source_span).rem_euclid(1.);
            run.source_start = if start >= 1. || start == 0. {
                0.
            } else {
                start
            };
            run.source_span = -run.source_span;
            run.cuts = run.cuts.iter().rev().map(|u| 1. - u).collect();
        }
        if two_runs {
            assert_eq!(
                expected.iter().map(|r| r.first_step).collect::<Vec<_>>(),
                vec![2, 0]
            );
        }
        let mut after = before.clone();
        cycles::reverse(after.graphics[0].cycle.as_mut().unwrap());
        assert!(!cycles::fit_edited(&before, &mut after, id, None).unwrap());
        assert_eq!(
            graphic(&after, id).cycle.as_ref().unwrap().split_runs,
            expected,
            "reverse must use each captured family, even when first_step keys exchange"
        );
        assert_eq!(after.atoms, before.atoms);
        assert_eq!(after.annotations, before.annotations);
        after.validate().unwrap();
        let reversed = after.clone();
        cycles::reverse(after.graphics[0].cycle.as_mut().unwrap());
        assert!(!cycles::fit_edited(&reversed, &mut after, id, None).unwrap());
        let restored = graphic(&after, id).cycle.as_ref().unwrap();
        let mut original = cycle;
        for (wanted, actual) in original.split_runs.iter_mut().zip(&restored.split_runs) {
            // The two inverse modular additions can differ by one f64 ulp.
            assert!((wanted.source_start - actual.source_start).abs() <= 16. * f64::EPSILON);
            wanted.source_start = actual.source_start;
        }
        assert_eq!(restored, &original);
        let restored_samples = samples(&after);
        assert_eq!(restored_samples.len(), original_samples.len());
        for (a, b) in restored_samples.into_iter().zip(original_samples) {
            assert!(
                a.distance(b) < 0.001,
                "double reverse moved native f32 geometry: {a:?} versus {b:?}"
            );
        }
        assert_eq!(after.atoms, before.atoms);
        assert_eq!(after.annotations, before.annotations);
        after.validate().unwrap();
    }
}

#[test]
fn selected_two_cut_batch_slides_atomically_without_taking_either_member_out() {
    let (mut before, first, id) = straight_split();
    let (second_at, station) = {
        let g = graphic(&before, id);
        let layout = Layout::graphic(&before, g).unwrap();
        let source = layout.sources[0].as_ref().unwrap();
        let run = &g.cycle.as_ref().unwrap().split_runs[0];
        (
            source.profiles.center.point(0.75).unwrap(),
            SplitStation {
                u: 0.75,
                start: run.source_start,
                span: run.source_span,
            },
        )
    };
    let second = text_at(&mut before, second_at, "Y");
    assert!(
        before.graphics[0]
            .cycle
            .as_mut()
            .unwrap()
            .insert_intermediate(2, vec![second], Some(station))
    );
    for group in before.groups.iter_mut().filter(|g| g.members.contains(&id)) {
        group.members.push(second);
    }
    before.validate().unwrap();
    let run = graphic(&before, id).cycle.as_ref().unwrap().split_runs[0].clone();
    let delta = Point::new(60., 0.);
    let selected = [first, second];
    let expected = {
        let layout = Layout::graphic(&before, graphic(&before, id)).unwrap();
        let profile = &layout.sources[0].as_ref().unwrap().profiles.center;
        let spacing =
            profile.distance_at(run.cuts[1]).unwrap() - profile.distance_at(run.cuts[0]).unwrap();
        assert!(f64::from(delta.x) < spacing);
        run.cuts
            .iter()
            .map(|u| profile.shift(*u, f64::from(delta.x)).unwrap())
            .collect::<Vec<_>>()
    };
    for cut in selected {
        let ink = crate::scene::selection_bounds(&before, &[cut]).unwrap();
        assert!(
            delta.x
                > (ink.1.x - ink.0.x) * 0.5
                    + DEFAULT.world(graphic(&before, id).cycle.as_ref().unwrap().gap_pt)
        );
    }
    let mut after = before.clone();
    after.translate(&selected, delta.x, delta.y);
    assert!(cycles::fit_moved(&before, &mut after, &selected));
    let changed = &graphic(&after, id).cycle.as_ref().unwrap().split_runs[0];
    assert_eq!(
        (changed.source_start, changed.source_span),
        (run.source_start, run.source_span)
    );
    for ((u, old), expected) in changed.cuts.iter().zip(&run.cuts).zip(expected) {
        assert!(
            *u > *old + 0.01,
            "both selected cuts need their new logical station"
        );
        assert!((*u - expected).abs() < 1e-6);
    }
    let layout = Layout::graphic(&after, graphic(&after, id)).unwrap();
    assert_eq!(layout.cut_intersects(1), Some(true));
    assert_eq!(layout.cut_intersects(2), Some(true));
    for resolved in &layout.steps {
        for arrow in &resolved
            .as_ref()
            .expect("every child remains positive after the batch")
            .arrows
        {
            assert!(
                crate::cycles::shapes::Shaft::from_arrow(arrow)
                    .unwrap()
                    .length()
                    > 1.
            );
        }
    }
    assert!(after.take_out(&selected, delta).is_empty());
    assert_eq!(
        cycles::role_of(&after, first),
        Some((id, Role::Intermediate(1)))
    );
    assert_eq!(
        cycles::role_of(&after, second),
        Some((id, Role::Intermediate(2)))
    );
    after.validate().unwrap();
}

#[test]
fn whole_selected_thirteen_degree_rotation_preserves_capture_and_reflection_maps_it_once() {
    use crate::editing::{self, Transform};
    for reflected in [false, true] {
        let (mut doc, _, id, centre) = cached_outer_straight_split();
        let captured = graphic(&doc, id).cycle.as_ref().unwrap().split_runs.clone();
        assert_eq!(doc.atom(centre).unwrap().cip_label.as_deref(), Some("R"));
        crate::atom_labels::clear_computed(&mut doc);
        let ids = doc.all_ids();
        let old_axis = graphic(&doc, id).axis_x;
        let expected: Vec<_> = captured
            .iter()
            .map(|run| {
                if reflected {
                    ((-run.source_start).rem_euclid(1.), -run.source_span)
                } else {
                    (run.source_start, run.source_span)
                }
            })
            .collect();
        if reflected {
            editing::transform(&mut doc, &ids, Transform::FlipHorizontal);
            let runs = &graphic(&doc, id).cycle.as_ref().unwrap().split_runs;
            assert_eq!(
                runs.iter()
                    .map(|r| (r.source_start, r.source_span))
                    .collect::<Vec<_>>(),
                expected
            );
            editing::transform_about(&mut doc, &ids, Point::new(-80., 50.), 1., 13.);
        } else {
            editing::transform(&mut doc, &ids, Transform::Rotate(13.));
        }
        assert_ne!(
            graphic(&doc, id).axis_x,
            old_axis,
            "the ordinary transform must actually run"
        );
        let runs = &graphic(&doc, id).cycle.as_ref().unwrap().split_runs;
        assert_eq!(
            runs.iter()
                .map(|r| (r.source_start, r.source_span))
                .collect::<Vec<_>>(),
            expected,
            "rigid 13-degree rotation must not remeasure transient exterior ink or flip a capture twice"
        );
        assert_eq!(
            runs.iter().map(|r| r.cuts.clone()).collect::<Vec<_>>(),
            captured.iter().map(|r| r.cuts.clone()).collect::<Vec<_>>()
        );
        let layout = Layout::graphic(&doc, graphic(&doc, id)).unwrap();
        assert!(
            layout
                .steps
                .iter()
                .all(|r| r.as_ref().is_some_and(|r| !r.arrows.is_empty()))
        );
        doc.validate().unwrap();
    }
}

#[test]
fn whole_selected_projection_tilt_maps_fitted_sources_before_reconciliation() {
    use crate::projection;
    let (_, before, _, id) = applied();
    let ids = before.all_ids();
    let captured = graphic(&before, id)
        .cycle
        .as_ref()
        .unwrap()
        .split_runs
        .clone();
    for around_x in [true, false] {
        let mut doc = before.clone();
        projection::tilt(&mut doc, &ids, 180., around_x);
        assert_ne!(doc, before, "a half turn must actually run");
        let mut expected = captured.clone();
        for run in &mut expected {
            let start = (-run.source_start).rem_euclid(1.);
            run.source_start = if start >= 1. || start == 0. {
                0.
            } else {
                start
            };
            run.source_span = -run.source_span;
        }
        assert_eq!(
            graphic(&doc, id).cycle.as_ref().unwrap().split_runs,
            expected,
            "tilt reflects the saved source once, like a planar flip"
        );
        assert!(valid(&doc, id));
        doc.validate().unwrap();
        let tilted = doc.clone();
        cycles::reconcile(&mut doc);
        assert_eq!(
            doc, tilted,
            "ordinary reconciliation must not retire the raw fitted family or move its cut"
        );
        assert_eq!(
            Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
            doc
        );
    }
}

#[test]
fn fitted_projection_tilt_keeps_valid_families_on_both_sides_of_edge_on() {
    let (_, before, _, id) = applied();
    let ids = before.all_ids();
    let captured = graphic(&before, id)
        .cycle
        .as_ref()
        .unwrap()
        .split_runs
        .clone();
    for around_x in [true, false] {
        for degrees in [15., 89.99, 90., 90.01, 120.] {
            let mut doc = before.clone();
            crate::projection::tilt(&mut doc, &ids, degrees, around_x);
            let runs = &graphic(&doc, id).cycle.as_ref().unwrap().split_runs;
            assert_eq!(runs.len(), captured.len(), "{around_x}, {degrees}");
            assert_eq!(
                runs.iter().map(|r| &r.cuts).collect::<Vec<_>>(),
                captured.iter().map(|r| &r.cuts).collect::<Vec<_>>(),
                "a whole tilt rebases or declines without retiring or sliding a cut"
            );
            assert!(valid(&doc, id), "{around_x}, {degrees}");
            doc.validate().unwrap();
        }
    }
}

#[test]
fn fitted_projection_tilt_that_collapses_its_host_rolls_back_the_complete_document() {
    let (mut doc, _, id) = straight_split();
    let ids = [id];
    let center = crate::editing::center(&doc, &ids);
    let cos = 90_f32.to_radians().cos();
    let g = doc.graphics.iter_mut().find(|g| g.id == id).unwrap();
    // This retained Z slope makes the X quarter turn exactly edge-on,
    // including the f32 approximation of cos(90°).
    g.depth = [
        (g.origin.y - center.y) * cos,
        g.axis_x.y * cos,
        g.axis_y.y * cos,
    ];
    doc.validate().unwrap();
    let before = doc.clone();
    crate::projection::tilt(&mut doc, &ids, 90., true);
    assert_eq!(
        doc, before,
        "no partial frame, depth or source edit survives"
    );
    assert!(valid(&doc, id));
}

#[test]
fn computed_label_lifecycle_keeps_frozen_source_and_nominal_children_valid() {
    use crate::atom_labels::{Number, Owner, clear_computed, refresh::Refresh};

    let molecule = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/chemical-naming-rust/r-lactic-rust-native.rsk"
    ))
    .unwrap();
    let mut doc = Document::default();
    let id = 1;
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Ellipse,
        Point::new(-1200., -700.),
        Point::new(1200., 700.),
        GraphicStyle::default(),
        Default::default(),
        false,
    );
    g.cycle = Some(Box::new(Cycle {
        shape: Shape::Rectangle,
        ..Default::default()
    }));
    doc.graphics.push(g);
    let outer = crate::editing::append_preserving_layout(&mut doc, &molecule, Point::default());
    let cut = crate::editing::append_preserving_layout(&mut doc, &molecule, Point::new(0., 1400.));
    assert_eq!(outer.len(), molecule.atoms.len());
    assert_eq!(cut.len(), molecule.atoms.len());
    let remap = |ids: &[u64], original| {
        molecule
            .all_ids()
            .into_iter()
            .zip(ids.iter().copied())
            .find(|(id, _)| *id == original)
            .unwrap()
            .1
    };
    let centres = [remap(&outer, 2), remap(&cut, 2)];
    let explicit_h = [remap(&outer, 3), remap(&cut, 3)];
    doc.atom_labels.stereo = true;
    doc.atom_labels.mark_undefined_stereocentres = true;
    doc.atom_labels.undefined_xi = true;
    for (i, &centre) in centres.iter().enumerate() {
        let atom = doc.atom_mut(centre).unwrap();
        atom.display.stereo.show = Some(true);
        atom.display.stereo.offset =
            Some(Point::new(150. - 30. * i as f32, -100. - 60. * i as f32));
        atom.display.number = Some(Number {
            text: format!("{}", i + 7),
            offset: Some(Point::new(-15., 0.)),
            style: crate::atom_labels::number_style(),
        });
    }
    for atom in explicit_h {
        doc.atom_mut(atom).unwrap().explicit_h = 1;
    }
    let from = center_of(crate::scene::selection_bounds(&nominal_document(&doc), &outer).unwrap());
    doc.translate(&outer, -600. - from.x, -700. - from.y);
    let last = text_at(&mut doc, Point::new(600., -700.), "Pd");
    {
        let cycle = doc.graphics[0].cycle.as_mut().unwrap();
        cycle.intermediates = vec![outer.clone(), vec![last]];
        cycle.steps = vec![
            Step {
                reversible: true,
                bow_pt: 180.,
                ..Default::default()
            },
            Step::default(),
        ];
    }
    // Both initial R labels come from real chemistry, not a fabricated cache.
    Refresh::calculate(&doc, &Refresh::default())
        .unwrap()
        .apply(&mut doc);
    for centre in centres {
        assert_eq!(doc.atom(centre).unwrap().cip_label.as_deref(), Some("R"));
        assert!(
            crate::atom_labels::indicators(&doc)
                .iter()
                .any(|i| i.owner == Owner::AtomStereo(centre))
        );
    }
    assert_ne!(
        crate::scene::selection_bounds(&doc, &outer),
        crate::scene::selection_bounds(&nominal_document(&doc), &outer),
        "the regression must exercise real computed ink changing an outer box"
    );
    // Capture the currently displayed outer trim before the cut is inserted.
    let station = capture_station(&doc, id, 0, 0.5);
    let target = {
        let g = graphic(&doc, id);
        let outline = cycles::outline(g).unwrap();
        deform::profiles(
            id,
            &outline,
            station.start * outline.length(),
            station.span * outline.length(),
            f64::from(DEFAULT.world(180.)),
            true,
            &arrow_style(g),
        )
        .unwrap()
        .center
        .point(station.u)
        .unwrap()
    };
    let from = center_of(crate::scene::selection_bounds(&nominal_document(&doc), &cut).unwrap());
    doc.translate(&cut, target.x - from.x, target.y - from.y);
    assert!(doc.graphics[0].cycle.as_mut().unwrap().insert_intermediate(
        1,
        cut.clone(),
        Some(station)
    ));
    let mut held = vec![id, last];
    held.extend(&outer);
    held.extend(&cut);
    doc.group_selection(&held).unwrap();

    let runs = graphic(&doc, id).cycle.as_ref().unwrap().split_runs.clone();
    let source_sample = |doc: &Document| {
        let layout = Layout::graphic(doc, graphic(doc, id)).unwrap();
        let source = layout.sources[0].as_ref().unwrap();
        let points: Vec<_> = (0..3)
            .flat_map(|lane| {
                [0., 0.125, 0.35, 0.5, 0.7, 0.875, 1.]
                    .map(|u| source.lane(lane).unwrap().point(u).unwrap())
            })
            .collect();
        ((source.first, source.span), source.cuts.clone(), points)
    };
    let frozen = source_sample(&doc);
    let authored_atoms = doc.atoms.clone();
    let authored_bonds = doc.bonds.clone();
    let check = |doc: &Document| {
        assert_eq!(graphic(doc, id).cycle.as_ref().unwrap().split_runs, runs);
        assert_eq!(
            source_sample(doc),
            frozen,
            "passive labels changed the saved source or logical cuts"
        );
        let layout = Layout::graphic(doc, graphic(doc, id)).unwrap();
        for step in 0..3 {
            let arrows = &layout.steps[step]
                .as_ref()
                .expect("every nominal child remains visible")
                .arrows;
            assert_eq!(arrows.len(), if step == 0 { 2 } else { 1 });
            assert_eq!(
                main_arrows(doc, id, &[step]).len(),
                arrows.len(),
                "computed display clearance cannot hide a nominal child"
            );
            for arrow in arrows {
                assert!(
                    crate::cycles::shapes::Shaft::from_arrow(arrow)
                        .unwrap()
                        .length()
                        > 1.
                );
            }
        }
        doc.validate().unwrap();
        for old in &authored_atoms {
            let mut now = doc.atom(old.id).unwrap().clone();
            now.label_h = old.label_h;
            now.cip_label = old.cip_label.clone();
            now.steroid_label = old.steroid_label.clone();
            now.undefined_stereo = old.undefined_stereo;
            now.undefined_pseudoasymmetric = old.undefined_pseudoasymmetric;
            assert_eq!(
                &now, old,
                "authored explicit H, number, stereo display and geometry must survive passive refresh"
            );
        }
        for old in &authored_bonds {
            let mut now = doc
                .bonds
                .iter()
                .find(|b| b.a == old.a && b.b == old.b)
                .unwrap()
                .clone();
            now.cip_label = old.cip_label.clone();
            now.undefined_stereo = old.undefined_stereo;
            assert_eq!(&now, old);
        }
    };
    check(&doc);
    Refresh::calculate(&doc, &Refresh::default())
        .unwrap()
        .apply(&mut doc);
    check(&doc);
    clear_computed(&mut doc);
    check(&doc);
    cycles::reconcile(&mut doc);
    check(&doc);
    for atom in &mut doc.atoms {
        atom.label_h += 2;
    }
    for (i, centre) in centres.into_iter().enumerate() {
        let atom = doc.atom_mut(centre).unwrap();
        atom.cip_label = (i == 1).then(|| "S".into());
        atom.steroid_label = Some("7α".into());
        atom.undefined_stereo = true;
        atom.undefined_pseudoasymmetric = true;
    }
    doc.bonds[0].cip_label = Some("E".into());
    doc.bonds[0].undefined_stereo = true;
    assert!(
        crate::atom_labels::indicators(&doc)
            .iter()
            .any(|i| i.text.contains('ξ'))
    );
    check(&doc);
    cycles::reconcile(&mut doc);
    check(&doc);
    let bytes = doc.file_json().unwrap();
    check(&Document::from_json(&bytes).unwrap());
    let mut reopened = Document::from_native_file(&bytes).unwrap();
    assert!(reopened.atoms.iter().all(|a| a.cip_label.is_none()));
    check(&reopened);
    Refresh::calculate(&reopened, &Refresh::default())
        .unwrap()
        .apply(&mut reopened);
    check(&reopened);
    check(&Document::from_native_file(&reopened.file_json().unwrap()).unwrap());
}

#[test]
fn fitted_routes_roundtrip_reverse_and_full_copy_as_native_geometry() {
    let (_, mut after, _, id) = applied();
    let saved = after.file_json().unwrap();
    let reopened = Document::from_json(&saved).unwrap();
    let rendered = cycles::drawn(&after, graphic(&after, id));
    let read = cycles::drawn(&reopened, graphic(&reopened, id));
    assert_eq!(rendered.arrows, read.arrows);
    assert_eq!(rendered.labels, read.labels);
    let all = main_arrows(&after, id, &[1, 2]);
    cycles::reverse(
        after
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .unwrap()
            .cycle
            .as_mut()
            .unwrap(),
    );
    let run = graphic(&after, id).cycle.as_ref().unwrap().split_runs[0].clone();
    let n = graphic(&after, id).cycle.as_ref().unwrap().steps.len();
    let steps: Vec<_> = (0..=run.cuts.len())
        .map(|i| (run.first_step + i) % n)
        .collect();
    same_locus(&all, &main_arrows(&after, id, &steps), &after, id);
    let mut copied = Document::default();
    crate::editing::append(&mut copied, &reopened, Point::new(50., -20.));
    let g = copied.graphics.iter().find(|g| g.cycle.is_some()).unwrap();
    assert_eq!(
        g.cycle.as_ref().unwrap().split_runs,
        graphic(&reopened, id).cycle.as_ref().unwrap().split_runs
    );
    assert!(valid(&copied, g.id));
    let (visible, written) = cycles::materialized(&reopened);
    assert!(visible.graphics.iter().all(|g| g.cycle.is_none()));
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].graphic.cycle, graphic(&reopened, id).cycle);
    for (_, arrow_id) in &written[0].arcs {
        assert!(
            visible
                .arrows
                .iter()
                .find(|a| a.id == *arrow_id)
                .unwrap()
                .validate()
                .is_ok()
        );
    }
}

#[test]
fn an_away_drag_coalesces_the_source_instead_of_leaving_a_detached_member() {
    let (_, mut after, part, id) = applied();
    let before = after.clone();
    let from = center(&after, &part);
    let target = Point::new(0., 0.);
    let delta = Point::new(target.x - from.x, target.y - from.y);
    after.translate(&part, delta.x, delta.y);
    cycles::fit_moved(&before, &mut after, &part);
    assert!(
        after
            .take_out(&part, delta)
            .contains(&crate::grouping::Container::Cycle)
    );
    assert!(cycles::role_of(&after, part[0]).is_none());
    let c = graphic(&after, id).cycle.as_ref().unwrap();
    assert!(c.split_runs.is_empty());
    assert_eq!(c.steps.len(), 3);
    assert_eq!(c.steps[1].bow_pt, 180.);
    assert!(c.steps[1].reversible);
    after.validate().unwrap();
}

#[test]
fn resize_and_circle_switch_rebase_or_retire_runs_and_tidy_retires_only_on_candidate() {
    let (_, mut after, part, id) = applied();
    let original = after.clone();
    let g = graphic(&after, id);
    let (w, h) = (
        g.axis_x.distance(Point::default()),
        g.axis_y.distance(Point::default()),
    );
    assert!(cycles::set_dimensions(&mut after, id, w * 1.15, h * 0.9));
    assert!(valid(&after, id));
    let resized = after.clone();
    after
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_mut()
        .unwrap()
        .shape = crate::cycles::Shape::Circle;
    assert!(cycles::fit_edited(&resized, &mut after, id, None).unwrap());
    assert!(
        graphic(&after, id)
            .cycle
            .as_ref()
            .unwrap()
            .split_runs
            .is_empty()
    );
    let outline = cycles::outline(graphic(&after, id)).unwrap();
    assert!(outline.project(center(&after, &part)).unwrap().distance < 0.05);
    let mut tidy = original.clone();
    assert!(cycles::tidy(&mut tidy, id));
    assert!(
        graphic(&tidy, id)
            .cycle
            .as_ref()
            .unwrap()
            .split_runs
            .is_empty()
    );
    assert!(
        !graphic(&original, id)
            .cycle
            .as_ref()
            .unwrap()
            .split_runs
            .is_empty()
    );
}

#[test]
fn independent_reversible_edit_retires_plain_source_then_new_plain_child_keeps_displayed_outer_lane()
 {
    let (mut before, part, id) = fixture();
    before
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_mut()
        .unwrap()
        .steps[1]
        .reversible = false;
    let mut fitted = cycles::propose_drop(&before, &part).unwrap().doc;
    assert!(!graphic(&fitted, id).cycle.as_ref().unwrap().split_runs[0].source_reversible);
    let old = fitted.clone();
    fitted
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .unwrap()
        .cycle
        .as_mut()
        .unwrap()
        .steps[2]
        .reversible = true;
    assert!(cycles::fit_edited(&old, &mut fitted, id, None).unwrap());
    assert!(
        graphic(&fitted, id)
            .cycle
            .as_ref()
            .unwrap()
            .split_runs
            .is_empty()
    );
    let parent = main_arrows(&fitted, id, &[2]);
    let at = parent[0].point(0.5);
    let next = fitted.next_id();
    let mut text = Annotation {
        id: next,
        position: Point::default(),
        text: "Pd".into(),
        angle: 0.,
        format: TextFormat::default(),
    };
    let (w, h) = text.size();
    text.position = at.offset(-w * 0.5, -h * 0.5);
    fitted.annotations.push(text);
    let after = cycles::propose_drop(&fitted, &[next]).unwrap().doc;
    let run = graphic(&after, id)
        .cycle
        .as_ref()
        .unwrap()
        .split_runs
        .last()
        .unwrap();
    assert!(run.source_reversible);
    same_locus(&parent[..1], &main_arrows(&after, id, &[2, 3]), &after, id);
}
