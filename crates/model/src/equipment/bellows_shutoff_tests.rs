//! Pneumatic bellows shutoff artwork keeps process and pilot seats fixed.
use super::builtin;
use super::eval::{Evaluated, Value, evaluate, resolve};
use super::format::{self, Definition, Gender, Joint, PlugFamily};
use super::geometry::{Seg, distance_to_outline, flatten, inside};
use super::{placed, solve};
use crate::document::{Document, Point};
use std::sync::Arc;

const CASES: [(&str, f64, f64, f64); 3] = [
    (
        include_str!("../../../../assets/pieces/gas-bellows-shutoff-valve-1-4.reshiki-piece"),
        6.35,
        3.6,
        1.7,
    ),
    (
        include_str!("../../../../assets/pieces/gas-bellows-shutoff-valve-3-8.reshiki-piece"),
        9.525,
        4.,
        2.6,
    ),
    (
        include_str!("../../../../assets/pieces/gas-bellows-shutoff-valve-1-2.reshiki-piece"),
        12.7,
        4.,
        2.6,
    ),
];

fn pose<'a>(definition: &'a Definition, view: &str, valve: &str) -> Evaluated<'a> {
    let values = [
        ("view".into(), Value::State(view.into())),
        ("valve".into(), Value::State(valve.into())),
    ]
    .into();
    evaluate(definition, &resolve(definition, &values).unwrap()).unwrap()
}

fn shape<'a>(pose: &'a Evaluated<'_>, id: &str) -> &'a [Seg] {
    &pose.shapes.iter().find(|s| s.shape.id == id).unwrap().path
}

fn y_extent(path: &[Seg]) -> [f64; 2] {
    flatten(path)
        .iter()
        .flat_map(|(points, _)| points)
        .fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], p| {
            [lo.min(p[1]), hi.max(p[1])]
        })
}

#[test]
fn bellows_shutoff_views_and_motion_keep_both_compression_mouths_and_stops_fixed() {
    for (text, diameter, _, _) in CASES {
        let definition = format::parse(text).unwrap();
        assert_eq!(definition.joints.len(), 3);
        for (joint, (id, dir)) in definition
            .joints
            .iter()
            .zip([("inlet", [-1., 0.]), ("outlet", [1., 0.])])
        {
            assert!(matches!(joint, Joint::Plug {
                family: PlugFamily::Compression,
                gender: Gender::F,
                size_mm,
                insertion_mm: Some(depth),
                ..
            } if *size_mm == diameter && *depth > 0.));
            assert_eq!(joint.id(), id);
            assert_eq!(joint.dir(), dir);
        }
        assert!(matches!(&definition.joints[2], Joint::Plug {
            id,
            family: PlugFamily::Npt,
            gender: Gender::F,
            size_mm: 3.175,
            insertion_mm: Some(6.),
            ..
        } if id == "pilot-air"));
        let baseline = pose(&definition, "exterior", "closed");
        for view in ["exterior", "section"] {
            for valve in ["closed", "open"] {
                let current = pose(&definition, view, valve);
                for (before, after) in baseline.joints.iter().zip(&current.joints) {
                    assert_eq!(before.at, after.at);
                    assert_eq!(before.dir, after.dir);
                    assert_eq!(before.tube_stop, after.tube_stop);
                    let Joint::Plug {
                        family, size_mm, ..
                    } = after.joint
                    else {
                        panic!("all valve ports are plug joints");
                    };
                    for sign in [-1., 1.] {
                        let rim = [
                            after.at[0],
                            after.at[1] + sign * family.opening_mm(*size_mm) / 2.,
                        ];
                        let gap = current
                            .shapes
                            .iter()
                            .map(|s| distance_to_outline(rim, &flatten(&s.path)))
                            .fold(f64::INFINITY, f64::min);
                        assert!(gap < 1e-8, "{} {view} {valve}: {rim:?}", definition.id);
                    }
                }
                assert_eq!(
                    current
                        .shapes
                        .iter()
                        .any(|s| s.shape.id == "bellows-section-left"),
                    view == "section"
                );
            }
        }
    }
}

#[test]
fn pneumatic_piston_lifts_the_soft_tip_and_compresses_the_return_spring() {
    for (text, _, travel, seat) in CASES {
        let definition = format::parse(text).unwrap();
        let closed = pose(&definition, "section", "closed");
        let open = pose(&definition, "section", "open");
        let tip_id = "conical-soft-shutoff-tip-section";
        let closed_tip = flatten(shape(&closed, tip_id));
        let open_tip = flatten(shape(&open, tip_id));
        for x in [-seat, seat] {
            assert!(distance_to_outline([x, 0.], &closed_tip) < 1e-8);
            assert!(distance_to_outline([x, 0.], &open_tip) > 1.);
        }
        assert!(inside([0., 0.], &closed_tip));
        assert!(!inside([0., 0.], &open_tip));
        for id in [
            "piston-section",
            "piston-rod-section",
            tip_id,
            "bellows-top-cap",
        ] {
            let a = y_extent(shape(&closed, id));
            let b = y_extent(shape(&open, id));
            assert!((a[0] - b[0] - travel).abs() < 1e-8, "{id}");
            assert!((a[1] - b[1] - travel).abs() < 1e-8, "{id}");
        }
        for id in ["bellows-section-left", "bellows-section-right"] {
            let a = y_extent(shape(&closed, id));
            let b = y_extent(shape(&open, id));
            assert!((a[0] - b[0] - travel).abs() < 1e-8, "{id}");
            assert!((a[1] - b[1]).abs() < 1e-8, "bellows base moved: {id}");
        }
        let a = y_extent(shape(&closed, "return-spring-section"));
        let b = y_extent(shape(&open, "return-spring-section"));
        assert!((a[0] - b[0]).abs() < 1e-8, "spring cap moved");
        assert!(
            (a[1] - b[1] - travel).abs() < 1e-8,
            "spring did not compress"
        );
        for current in [&closed, &open] {
            for side in ["left", "right"] {
                let wall = flatten(shape(current, &format!("bellows-bonnet-section-{side}")));
                for id in [
                    "bellows-top-cap",
                    "bellows-section-left",
                    "bellows-section-right",
                ] {
                    for point in flatten(shape(current, id))
                        .iter()
                        .flat_map(|(points, _)| points)
                    {
                        assert!(!inside(*point, &wall), "{id} intersects the fixed bonnet");
                    }
                }
            }
        }
    }
}

#[test]
fn pilot_air_passage_is_separate_from_the_process_chamber() {
    for (text, _, _, _) in CASES {
        let definition = format::parse(text).unwrap();
        let pilot = definition
            .joints
            .iter()
            .find(|j| j.id() == "pilot-air")
            .unwrap();
        for state in ["closed", "open"] {
            let current = pose(&definition, "section", state);
            let air = flatten(shape(&current, &format!("pilot-air-chamber-{state}")));
            assert!(inside([pilot.at()[0] + 2., pilot.at()[1]], &air));
            assert!(inside([-10., pilot.at()[1]], &air));
            assert!(!inside([0., 0.], &air));
            assert!(
                !inside([0., pilot.at()[1]], &air),
                "air region includes solid rod"
            );
            for (x, side) in [(-6., "left"), (6., "right")] {
                let partition =
                    flatten(shape(&current, &format!("pilot-process-partition-{side}")));
                assert!(inside([x, pilot.at()[1] + 5.], &partition));
            }
            let air_bottom = y_extent(shape(&current, &format!("pilot-air-chamber-{state}")))[1];
            let process_top = y_extent(shape(&current, "bellows-section-left"))[0];
            assert!(
                air_bottom < process_top - 3.,
                "pilot chamber reaches process bellows"
            );
        }
    }
}

#[test]
fn pilot_adapter_stays_seated_through_all_valve_states_and_views() {
    let adapter = builtin::find("reshiki/gas-compression-npt-adapter-1-4-to-1-8").unwrap();
    for (text, _, _, _) in CASES {
        let mut doc = Document::default();
        let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
        for (id, definition) in [
            (1, Arc::new(format::parse(text).unwrap())),
            (2, adapter.clone()),
        ] {
            doc.graphics
                .push(placed::graphic(id, definition, Point::default(), scale));
        }
        let joint = |piece, name: &str| solve::JointRef {
            piece,
            joint: name.into(),
        };
        let seated =
            solve::connect(&mut doc, &[2], &joint(1, "pilot-air"), &joint(2, "thread")).unwrap();
        assert!(matches!(seated.offer, solve::Offer::Seat));
        let before: Vec<_> = solve::ports(&doc.graphics[0])
            .into_iter()
            .map(|p| (p.at, p.point, p.dir, p.insertion))
            .collect();
        let seats = doc.seats.clone();
        for view in ["exterior", "section"] {
            for valve in ["closed", "open"] {
                for (parameter, state) in [("view", view), ("valve", valve)] {
                    assert!(
                        solve::adjust::set(&mut doc, 1, parameter, Value::State(state.into()))
                            .unwrap()
                            .is_empty()
                    );
                }
                solve::check(&mut doc, None);
                assert_eq!(doc.seats, seats);
                let after: Vec<_> = solve::ports(&doc.graphics[0])
                    .into_iter()
                    .map(|p| (p.at, p.point, p.dir, p.insertion))
                    .collect();
                assert_eq!(before, after);
                let pilot = solve::ports(&doc.graphics[0])
                    .into_iter()
                    .find(|p| p.at.joint == "pilot-air")
                    .unwrap();
                let thread = solve::ports(&doc.graphics[1])
                    .into_iter()
                    .find(|p| p.at.joint == "thread")
                    .unwrap();
                assert!(solve::mates(&pilot, &thread));
                let (gap, angle) = solve::residual(&pilot, &thread);
                assert!(gap < 1e-3 && angle < 1e-3);
                doc.validate().unwrap();
            }
        }
    }
}
