use super::*;
use crate::{
    cycles::{self, Shape, reverse},
    document::{Document, Point},
};

fn cycle(n: usize) -> Cycle {
    Cycle {
        shape: Shape::Ellipse,
        intermediates: (0..n).map(|i| vec![i as u64 + 1]).collect(),
        steps: (0..n)
            .map(|i| Step {
                name: format!("Step {i}"),
                lead_pt: 7. + i as f32,
                trail_pt: 11. + i as f32,
                bow_pt: 12. + i as f32,
                reversible: true,
                reagents: vec![vec![100 + i as u64]],
                products: vec![vec![200 + i as u64]],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn station(u: f64) -> SplitStation {
    SplitStation {
        u,
        start: 0.,
        span: 0.5,
    }
}

#[test]
fn frozen_source_capture_survives_repeated_splits_and_reverse_wraps_the_baseline_once() {
    for (start, span, reversed_start) in [
        (0.875, 0.5, 0.375),
        (0.125, -0.5, 0.625),
        (0., -0.5, 0.5),
        (0.5, 0.5, 0.),
    ] {
        let mut c = cycle(4);
        assert!(c.insert_intermediate(
            0,
            vec![90],
            Some(SplitStation {
                u: 0.25,
                start,
                span
            })
        ));
        assert!(
            c.insert_intermediate(
                1,
                vec![91],
                Some(SplitStation {
                    u: 0.75,
                    start: f64::NAN,
                    span: f64::INFINITY,
                })
            ),
            "existing runs use only u, retaining their original capture"
        );
        assert_eq!(
            (c.split_runs[0].source_start, c.split_runs[0].source_span),
            (start, span)
        );
        let original = c.clone();
        reverse(&mut c);
        assert_eq!(
            (c.split_runs[0].source_start, c.split_runs[0].source_span),
            (reversed_start, -span)
        );
        assert_eq!(c.split_runs[0].cuts, [0.25, 0.75]);
        assert!(!c.split_runs[0].source_start.is_sign_negative());
        c.validate().unwrap();
        reverse(&mut c);
        assert_eq!(c, original);
    }

    // rem_euclid can round a tiny negative input to exactly one. Stored starts
    // still canonicalize that seam to positive zero.
    let mut c = cycle(4);
    assert!(c.insert_intermediate(
        1,
        vec![90],
        Some(SplitStation {
            u: 0.5,
            start: 0.,
            span: -1e-30,
        })
    ));
    reverse(&mut c);
    assert_eq!(c.split_runs[0].source_start.to_bits(), 0_f64.to_bits());
    assert_eq!(c.split_runs[0].source_span, 1e-30);
}

#[test]
fn source_baseline_is_required_finite_in_domain_and_rejected_atomically() {
    for (start, span) in [
        (-f64::EPSILON, 0.5),
        (1., 0.5),
        (f64::NAN, 0.5),
        (f64::INFINITY, 0.5),
        (0., -1.),
        (0., 0.),
        (0., -0.),
        (0., 1.),
        (0., f64::NAN),
        (0., f64::INFINITY),
    ] {
        let mut c = cycle(4);
        let before = c.clone();
        assert!(!c.insert_intermediate(
            1,
            vec![90],
            Some(SplitStation {
                u: 0.5,
                start,
                span
            })
        ));
        assert_eq!(c, before);
        let mut invalid = before;
        invalid.split_runs = vec![SplitRun {
            first_step: 0,
            cuts: vec![0.5],
            source_start: start,
            source_span: span,
            source_bow_pt: 12.,
            source_reversible: true,
        }];
        assert!(invalid.validate().is_err());
    }
    for span in [358. / 360., -358. / 360.] {
        let mut c = cycle(4);
        assert!(c.insert_intermediate(
            1,
            vec![90],
            Some(SplitStation {
                u: 0.5,
                start: 1. - f64::EPSILON,
                span,
            })
        ));
        c.validate().unwrap();
        let value = serde_json::to_value(&c.split_runs[0]).unwrap();
        assert_eq!(
            serde_json::from_value::<SplitRun>(value.clone()).unwrap(),
            c.split_runs[0]
        );
        for field in ["source_start", "source_span"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<SplitRun>(missing).is_err(),
                "{field} must be explicit"
            );
        }
    }
    let mut circle = cycle(4);
    circle.shape = Shape::Circle;
    let before = circle.clone();
    assert!(!circle.insert_intermediate(1, vec![90], Some(station(0.5))));
    assert_eq!(circle, before);
}

#[test]
fn repeated_wrapping_insertions_use_global_stations_and_only_exterior_trims() {
    let original = cycle(4);
    let mut c = original.clone();
    assert!(c.insert_intermediate(0, vec![90], Some(station(0.25))));
    assert_eq!(c.split_runs[0].first_step, 4);
    assert_eq!(c.split_runs[0].cuts, [0.25]);
    assert_eq!(c.steps[4].lead_pt, original.steps[3].lead_pt);
    assert_eq!(c.steps[4].trail_pt, 0.);
    assert_eq!(c.steps[0].lead_pt, 0.);
    assert_eq!(c.steps[0].trail_pt, original.steps[3].trail_pt);
    assert_eq!(c.steps[4].name, original.steps[3].name);
    assert_eq!(c.steps[4].reagents, original.steps[3].reagents);
    assert!(c.insert_intermediate(1, vec![91], Some(station(0.75))));
    let run = &c.split_runs[0];
    assert_eq!(
        (run.first_step, run.last_step(6), run.end_slot(6)),
        (5, 1, 2)
    );
    assert_eq!(run.cuts, [0.25, 0.75]);
    for (step, ordinal) in [(5, 0), (0, 1), (1, 2)] {
        assert_eq!(c.split_run(step).unwrap().1, ordinal);
    }
    assert!(c.split_run(2).is_none());
    assert_eq!(c.steps[0].trail_pt, 0.);
    assert_eq!(c.steps[1].lead_pt, 0.);
    assert_eq!(c.steps[1].trail_pt, original.steps[3].trail_pt);
    c.validate().unwrap();
    assert!(c.remove_intermediate(0).is_empty());
    assert_eq!(c.split_runs[0].cuts, [0.75]);
    assert!(c.remove_intermediate(0).is_empty());
    assert_eq!(c, original);
}

#[test]
fn removing_the_last_cut_after_reverse_restores_source_geometry_and_roles() {
    for n in [1, 4] {
        let original = cycle(n);
        let mut c = original.clone();
        assert!(c.insert_intermediate(1, vec![90], Some(station(0.25))));
        if n == 1 {
            assert!(c.insert_intermediate(2, vec![91], Some(station(0.75))));
            assert_eq!(c.split_runs[0].end_slot(3), c.split_runs[0].first_step);
        }
        reverse(&mut c);
        while let Some(run) = c.split_runs.first() {
            let cut = (run.first_step + 1) % c.steps.len();
            assert!(c.remove_intermediate(cut).is_empty());
            c.validate().unwrap();
        }
        let mut expected = original;
        reverse(&mut expected);
        assert_eq!(c, expected, "source Bow, reversible, trims and chemistry");
    }

    let mut expected = cycle(4);
    let mut wrapping = expected.clone();
    assert!(wrapping.insert_intermediate(0, vec![90], Some(station(0.25))));
    reverse(&mut wrapping);
    assert!(wrapping.remove_intermediate(0).is_empty());
    reverse(&mut expected);
    // Removing slot zero retains the ordinary cyclic anchor change.
    expected.intermediates.rotate_left(1);
    expected.steps.rotate_left(1);
    assert_eq!(wrapping, expected);
}

#[test]
fn outer_endpoint_retirement_returns_ordered_surviving_cut_stations() {
    let mut c = cycle(4);
    assert!(c.insert_intermediate(1, vec![90], Some(station(0.25))));
    assert!(c.insert_intermediate(2, vec![91], Some(station(0.75))));
    let retired = c.remove_intermediate(0);
    assert_eq!(
        retired,
        [
            RetiredCut { index: 0, u: 0.25 },
            RetiredCut { index: 1, u: 0.75 }
        ]
    );
    assert!(c.split_runs.is_empty());
    c.validate().unwrap();

    let mut c = cycle(1);
    assert!(c.insert_intermediate(1, vec![90], Some(station(0.25))));
    assert!(c.insert_intermediate(2, vec![91], Some(station(0.75))));
    let retired = c.remove_intermediate(0);
    assert_eq!(
        retired,
        [
            RetiredCut { index: 0, u: 0.25 },
            RetiredCut { index: 1, u: 0.75 }
        ]
    );
    assert!(c.split_runs.is_empty());
    c.validate().unwrap();
}

#[test]
fn unrelated_changes_reindex_disjoint_runs_and_legacy_circle_insertions_stay_plain() {
    let mut c = cycle(6);
    assert!(c.insert_intermediate(1, vec![90], Some(station(0.5))));
    assert!(c.insert_intermediate(5, vec![91], Some(station(0.5))));
    assert_eq!(
        c.split_runs
            .iter()
            .map(|r| r.first_step)
            .collect::<Vec<_>>(),
        [0, 4]
    );
    assert!(c.insert_intermediate(3, vec![92], None));
    assert_eq!(
        c.split_runs
            .iter()
            .map(|r| r.first_step)
            .collect::<Vec<_>>(),
        [0, 5]
    );
    assert!(c.remove_intermediate(3).is_empty());
    assert_eq!(
        c.split_runs
            .iter()
            .map(|r| r.first_step)
            .collect::<Vec<_>>(),
        [0, 4]
    );
    reverse(&mut c);
    assert_eq!(
        c.split_runs
            .iter()
            .map(|r| r.first_step)
            .collect::<Vec<_>>(),
        [6, 2]
    );
    c.validate().unwrap();

    let mut circle = cycle(4);
    circle.shape = Shape::Circle;
    let mut expected = circle.clone();
    expected.intermediates.insert(2, vec![90]);
    expected.steps.insert(2, Step::default());
    assert!(circle.insert_intermediate(2, vec![90], None));
    assert_eq!(circle, expected);
    assert!(
        serde_json::to_value(&circle)
            .unwrap()
            .get("split_runs")
            .is_none()
    );
}

#[test]
fn invalid_overlap_and_unaddressable_stations_are_rejected_before_mutation() {
    let mut valid = cycle(4);
    let run = SplitRun {
        first_step: 0,
        cuts: vec![0.5],
        source_start: 0.,
        source_span: 0.5,
        source_bow_pt: 12.,
        source_reversible: true,
    };
    valid.split_runs = vec![
        run.clone(),
        SplitRun {
            first_step: 2,
            ..run.clone()
        },
    ];
    valid.validate().unwrap();
    let mut overlap = valid.clone();
    overlap.split_runs[1].first_step = 1;
    assert!(overlap.validate().is_err());
    for bad in [
        SplitRun {
            first_step: 4,
            ..run.clone()
        },
        SplitRun {
            source_bow_pt: f32::NAN,
            ..run.clone()
        },
        SplitRun {
            source_bow_pt: 721.,
            ..run.clone()
        },
        SplitRun {
            cuts: vec![0.125, 0.25, 0.5, 0.75],
            ..run.clone()
        },
    ] {
        let mut c = cycle(4);
        c.split_runs = vec![bad];
        assert!(c.validate().is_err());
    }
    for cuts in [
        vec![],
        vec![0.],
        vec![1.],
        vec![f64::NAN],
        vec![f64::INFINITY],
        vec![0.5, 0.5],
        vec![1e-30],
        vec![1e-16, 1.1e-16],
    ] {
        let mut c = cycle(4);
        c.split_runs = vec![SplitRun {
            cuts,
            ..run.clone()
        }];
        assert!(c.validate().is_err());
    }
    let mut c = cycle(4);
    let original = c.clone();
    for u in [0., 1., f64::NAN, 1e-30] {
        assert!(!c.insert_intermediate(1, vec![90], Some(station(u))));
        assert_eq!(c, original);
    }
    assert!(c.insert_intermediate(1, vec![90], Some(station(0.5))));
    let original = c.clone();
    assert!(
        !c.insert_intermediate(1, vec![91], Some(station(0.75))),
        "u must belong to the donor child"
    );
    assert!(
        !c.insert_intermediate(2, vec![91], Some(station(0.5))),
        "existing cuts are not duplicated"
    );
    assert_eq!(c, original);
}

#[test]
fn old_json_omits_runs_and_native_copy_remaps_only_parts_not_relative_metadata() {
    let old: Cycle = serde_json::from_str("{}").unwrap();
    assert!(old.split_runs.is_empty());
    assert!(
        serde_json::to_value(old)
            .unwrap()
            .get("split_runs")
            .is_none()
    );
    let source = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/cycles/suzuki-rectangle-bow180-target.rsk"
    ))
    .unwrap();
    let original_graphic = source.graphics.iter().find(|g| g.cycle.is_some()).unwrap();
    let id = original_graphic.id;
    let original = original_graphic.cycle.as_deref().unwrap().clone();
    let inserted: Vec<_> = (137..=143).collect();
    let proposal = cycles::propose_drop(&source, &inserted)
        .expect("the actual bowed Suzuki insertion must produce valid fitted geometry");
    assert_eq!(proposal.target.role, cycles::Role::Intermediate(2));
    let doc = proposal.doc;
    doc.validate().unwrap();
    let graphic = doc.graphics.iter().find(|g| g.id == id).unwrap();
    let c = graphic.cycle.as_deref().unwrap();
    assert_eq!(c.split_runs.len(), 1);
    assert_eq!(c.intermediates[2], inserted);
    assert_eq!(c.split_runs[0].first_step, 1);
    assert_eq!(c.split_runs[0].source_bow_pt, 180.);
    assert!(c.split_runs[0].source_reversible);
    assert!(source_baseline(
        c.split_runs[0].source_start,
        c.split_runs[0].source_span
    ));
    let bytes = doc.file_json().unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"],
        40
    );
    let back = Document::from_json(&bytes).unwrap();
    let read = back.graphics.iter().find(|g| g.id == id).unwrap();
    assert_eq!(read.cycle.as_deref().unwrap(), c);
    assert_eq!(
        cycles::drawn(&back, read).arrows,
        cycles::drawn(&doc, graphic).arrows
    );
    assert_eq!(back.file_json().unwrap(), bytes);
    let mut copied = Document::default();
    // Force a real ID remap even though the fixture's atom IDs start at one.
    copied.add_atom("He", Point::new(-2000., -2000.));
    assert!(!crate::editing::append(&mut copied, &doc, Point::new(50., 20.)).is_empty());
    copied.validate().unwrap();
    assert_eq!(
        copied.graphics[0].cycle.as_ref().unwrap().split_runs,
        c.split_runs
    );
    assert_ne!(
        copied.graphics[0].cycle.as_ref().unwrap().intermediates,
        c.intermediates
    );
    let selected: Vec<_> = doc
        .all_ids()
        .into_iter()
        .filter(|id| !inserted.contains(id))
        .collect();
    let partial = crate::editing::selection(&doc, &selected);
    partial.validate().unwrap();
    let partial_graphic = partial.graphics.iter().find(|g| g.id == id).unwrap();
    assert_eq!(
        *partial_graphic.cycle.as_deref().unwrap(),
        original,
        "omitting the cut molecule coalesces exactly back to the original source cycle"
    );
}
