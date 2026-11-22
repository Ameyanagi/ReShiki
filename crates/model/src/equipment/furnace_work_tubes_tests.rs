//! Interchangeable furnace apertures use the ordinary straight tube span.
use super::builtin;
use super::eval::Value;
use super::format::{HoldKind, TubeMaterial};
use super::parts_list::tube_row;
use super::placed;
use super::solve::{self, JointRef, Kind, Port, ports};
use crate::document::{Document, Point};

const FURNACES: [&str; 3] = [
    "reshiki/tube-furnace-32",
    "reshiki/tube-furnace-vertical-32",
    "reshiki/tube-furnace-body-32",
];
const SUS: [(&str, f64); 5] = [
    ("1-16-in", 1.5875),
    ("1-8-in", 3.175),
    ("1-4-in", 6.35),
    ("3-8-in", 9.525),
    ("6-mm", 6.),
];

fn add(doc: &mut Document, name: &str) -> u64 {
    let id = doc.next_id();
    let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    let definition = builtin::find(name).unwrap();
    doc.graphics
        .push(placed::graphic(id, definition, Point::default(), scale));
    id
}

fn joint(piece: u64, port: &str) -> JointRef {
    JointRef {
        piece,
        joint: port.into(),
    }
}

fn port_of(doc: &Document, endpoint: &JointRef) -> Port {
    doc.graphics
        .iter()
        .filter(|g| g.id == endpoint.piece)
        .flat_map(ports)
        .find(|p| p.at == *endpoint)
        .unwrap()
}

fn bore_of(doc: &Document, id: u64) -> Port {
    let mut available = doc.graphics.iter().filter(|g| g.id == id).flat_map(ports);
    let bore = available.next().unwrap();
    assert_eq!(bore.hold(), Some(HoldKind::ThroughBore));
    assert!(available.next().is_none());
    bore
}

fn change(doc: &mut Document, id: u64, param: &str, value: &str) -> Vec<solve::Seat> {
    solve::adjust::set(doc, id, param, Value::State(value.into())).unwrap()
}

fn length(doc: &mut Document, id: u64, mm: f64) {
    let _ = solve::adjust::set(doc, id, "length", Value::Number(mm)).unwrap();
}

fn link(doc: &mut Document, fixed: &JointRef, moving: &JointRef) {
    let result = solve::connect(doc, &[moving.piece], fixed, moving).unwrap();
    assert!(matches!(result.offer, solve::Offer::Seat));
}

fn link_through_centre(doc: &mut Document, bore: &Port, span: &JointRef) {
    let tube = port_of(doc, span);
    let centre = |p: &Port| {
        [
            p.point[0] + p.dir[0] * p.length / 2.,
            p.point[1] + p.dir[1] * p.length / 2.,
        ]
    };
    let [fx, fy] = centre(bore);
    let [tx, ty] = centre(&tube);
    doc.translate(&[span.piece], (fx - tx) as f32, (fy - ty) as f32);
    link(doc, &bore.at, span);
}

fn check_slide(
    doc: &Document,
    furnace: u64,
    tube: u64,
    ends: &[JointRef; 2],
    travel: f64,
    min_gap: f64,
) {
    let bore = bore_of(doc, furnace);
    let axis = bore.dir;
    let centre = [
        bore.point[0] + axis[0] * bore.length / 2.,
        bore.point[1] + axis[1] * bore.length / 2.,
    ];
    let ids: Vec<_> = solve::assembly(doc, furnace).into_iter().collect();
    let moving = solve::carried_through(doc, &ids, &[tube], &[]).unwrap();
    assert_eq!(moving.len() + 1, ids.len());
    assert!(!moving.contains(&furnace));
    for direction in [-1., 1.] {
        let delta = solve::constrain(
            doc,
            &moving,
            [axis[0] * direction * 1e5, axis[1] * direction * 1e5],
        )
        .unwrap();
        let mm = (delta[0] * axis[0] + delta[1] * axis[1]) / bore.mm;
        assert!((mm - direction * travel).abs() < 0.001);
        let mut moved = doc.clone();
        moved.translate(&moving, delta[0] as f32, delta[1] as f32);
        solve::check(&mut moved, None);
        assert_eq!(moved.seats, doc.seats);
        for end in ends {
            let point = port_of(&moved, end).point;
            let distance = ((point[0] - centre[0]) * axis[0] + (point[1] - centre[1]) * axis[1])
                .abs()
                / bore.mm;
            assert!(distance - 155. + 0.001 >= min_gap);
        }
        let reopened = Document::from_json(&moved.file_json().unwrap()).unwrap();
        assert_eq!(reopened.seats, moved.seats);
    }
}

#[test]
fn aperture_selection_has_one_exact_bore_in_each_furnace_and_view() {
    for name in FURNACES {
        let mut doc = Document::default();
        let furnace = add(&mut doc, name);
        assert_eq!(bore_of(&doc, furnace).at.joint, "bore");
        for (state, od, span) in [
            ("32-mm", 32., 300.),
            ("15-mm", 15., 300.),
            ("20-mm", 20., 300.),
        ]
        .into_iter()
        .chain(SUS.map(|(state, od)| (state, od, 360.)))
        {
            assert!(change(&mut doc, furnace, "tube-od", state).is_empty());
            let exterior = bore_of(&doc, furnace);
            assert_eq!(exterior.kind, Kind::Hold(HoldKind::ThroughBore, [od, od]));
            assert!((exterior.length / exterior.mm - span).abs() < 0.001);
            assert!(change(&mut doc, furnace, "view", "section").is_empty());
            let section = bore_of(&doc, furnace);
            assert_eq!(
                (
                    section.at,
                    section.point,
                    section.dir,
                    section.length,
                    section.kind
                ),
                (
                    exterior.at,
                    exterior.point,
                    exterior.dir,
                    exterior.length,
                    exterior.kind,
                )
            );
            assert!(change(&mut doc, furnace, "view", "exterior").is_empty());
        }
    }
}

#[test]
fn ordinary_stainless_steel_seats_pass_and_report_dimensions_with_fittings_attached() {
    for ((state, od), suffix) in SUS.into_iter().zip(["1-16", "1-8", "1-4", "3-8", "6-mm"]) {
        for stock in [60, 120] {
            // Alternate axes, so stock parts pass through every housing orientation.
            for furnace_name in FURNACES {
                let mut doc = Document::default();
                let furnace = add(&mut doc, furnace_name);
                assert!(change(&mut doc, furnace, "tube-od", state).is_empty());
                let key = format!("reshiki/compression-tube-{stock}-{suffix}");
                let tube = add(&mut doc, &key);
                let span = joint(tube, "span");
                let bore = bore_of(&doc, furnace);
                assert!(!solve::mates(&bore, &port_of(&doc, &span)));
                length(&mut doc, tube, 359.99);
                assert!(!solve::mates(&bore, &port_of(&doc, &span)));
                length(&mut doc, tube, 360.);
                assert!(solve::mates(&bore, &port_of(&doc, &span)));
                length(&mut doc, tube, 500.25);
                link_through_centre(&mut doc, &bore, &span);
                let ends = ["a", "b"].map(|end| {
                    let union = add(&mut doc, &format!("reshiki/compression-union-{suffix}"));
                    let face = joint(union, "left");
                    link(&mut doc, &joint(tube, end), &face);
                    face
                });
                assert_eq!(doc.seats.len(), 3);
                let row = tube_row(&doc, tube).unwrap().unwrap();
                assert_eq!(row.material, TubeMaterial::StainlessSteel);
                assert_eq!((row.od_mm, row.cut_length_mm), (od, 500.25));
                assert_eq!(row.through.len(), 1);
                assert_eq!(row.through[0].fitting.id, furnace);
                check_slide(&doc, furnace, tube, &ends, 70.125, 10.);
            }
        }
    }
}

#[test]
fn smaller_quartz_uses_exact_end_seals_and_keeps_its_passage_when_rotated() {
    for size in [15, 20] {
        for gas in ["1-8", "1-4"] {
            for furnace_name in FURNACES {
                let mut doc = Document::default();
                let furnace = add(&mut doc, furnace_name);
                assert!(change(&mut doc, furnace, "tube-od", &format!("{size}-mm")).is_empty());
                let tube = add(&mut doc, &format!("reshiki/quartz-work-tube-500-{size}"));
                let bore = bore_of(&doc, furnace);
                link_through_centre(&mut doc, &bore, &joint(tube, "span"));
                let ends = ["left", "right"].map(|end| {
                    let seal = add(
                        &mut doc,
                        &format!("reshiki/work-tube-end-seal-compression-{size}-{gas}"),
                    );
                    let female = joint(seal, "work-tube");
                    link(&mut doc, &joint(tube, end), &female);
                    let line = add(&mut doc, &format!("reshiki/compression-tube-60-{gas}"));
                    link(&mut doc, &joint(seal, "gas-line"), &joint(line, "a"));
                    female
                });
                assert_eq!(doc.seats.len(), 5);
                let row = tube_row(&doc, tube).unwrap().unwrap();
                assert_eq!(row.material, TubeMaterial::Quartz);
                assert_eq!(row.od_mm, f64::from(size));
                assert_eq!(row.cut_length_mm, 500.);
                assert_eq!(row.through[0].fitting.id, furnace);
                check_slide(&doc, furnace, tube, &ends, 70., 7.);
            }
        }
    }
}

#[test]
fn changing_an_occupied_bore_releases_only_its_furnace_seat() {
    let mut doc = Document::default();
    let furnace = add(&mut doc, FURNACES[0]);
    let tube = add(&mut doc, "reshiki/compression-tube-120-6-mm");
    let union = add(&mut doc, "reshiki/compression-union-6-mm");
    length(&mut doc, tube, 500.);
    assert!(change(&mut doc, furnace, "tube-od", "6-mm").is_empty());
    let bore = bore_of(&doc, furnace);
    link_through_centre(&mut doc, &bore, &joint(tube, "span"));
    link(&mut doc, &joint(tube, "b"), &joint(union, "left"));
    let endpoint_seat = doc
        .seats
        .iter()
        .find(|s| s.a.piece == union || s.b.piece == union)
        .unwrap()
        .clone();
    let old = doc.graphics.clone();
    let released = change(&mut doc, furnace, "tube-od", "1-4-in");
    assert_eq!(released.len(), 1);
    assert!(released[0].a.piece == furnace || released[0].b.piece == furnace);
    assert_eq!(doc.seats, [endpoint_seat]);
    assert_eq!(doc.graphics[1..], old[1..]);
    assert!(!solve::mates(
        &bore_of(&doc, furnace),
        &port_of(&doc, &joint(tube, "span"))
    ));
}
