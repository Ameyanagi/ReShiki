//! Bellows shutoff artwork keeps its tube seats fixed while the stem opens.
use super::eval::{Evaluated, Value, evaluate, resolve};
use super::format::{self, Definition, Gender, Joint, PlugFamily};
use super::geometry::{Seg, distance_to_outline, flatten, inside};

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
        assert_eq!(definition.joints.len(), 2);
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
        let baseline = pose(&definition, "exterior", "closed");
        for view in ["exterior", "section"] {
            for valve in ["closed", "open"] {
                let current = pose(&definition, view, valve);
                for (before, after) in baseline.joints.iter().zip(&current.joints) {
                    assert_eq!(before.at, after.at);
                    assert_eq!(before.dir, after.dir);
                    assert_eq!(before.tube_stop, after.tube_stop);
                    for sign in [-1., 1.] {
                        let rim = [after.at[0], after.at[1] + sign * diameter / 2.];
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
fn bellows_shutoff_tip_contacts_the_closed_seat_and_clears_it_when_open() {
    for (text, _, travel, seat) in CASES {
        let definition = format::parse(text).unwrap();
        let closed = pose(&definition, "section", "closed");
        let open = pose(&definition, "section", "open");
        let tip_id = "spherical-shutoff-tip-section";
        let closed_tip = flatten(shape(&closed, tip_id));
        let open_tip = flatten(shape(&open, tip_id));
        for x in [-seat, seat] {
            assert!(distance_to_outline([x, 0.], &closed_tip) < 1e-8);
            assert!(distance_to_outline([x, 0.], &open_tip) > 2.5);
        }
        assert!(inside([0., 0.], &closed_tip));
        assert!(!inside([0., 0.], &open_tip));
        for id in ["actuator-section", tip_id, "bellows-top-cap"] {
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
    }
}
