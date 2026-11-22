//! Upright furnace artwork and direct standard compression end-seal seats.
use super::builtin;
use super::eval::{Value, Values, evaluate, resolve};
use super::format::{Gender, HoldKind, Joint, PlugFamily};
use super::geometry::{flatten, inside};
use super::placed;
use super::solve::{self, JointRef, Kind, Port, ports, residual};
use crate::document::{Document, Point};

const FURNACE: &str = "reshiki/tube-furnace-vertical-32";
const TUBE: &str = "reshiki/quartz-work-tube-600-32";

fn add(doc: &mut Document, name: &str) -> u64 {
    let id = doc.next_id();
    let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic(
        id,
        builtin::find(name).unwrap(),
        Point::default(),
        scale,
    ));
    id
}
fn joint(piece: u64, name: &str) -> JointRef {
    JointRef {
        piece,
        joint: name.into(),
    }
}
fn port(doc: &Document, piece: u64, name: &str) -> Port {
    ports(doc.graphics.iter().find(|g| g.id == piece).unwrap())
        .into_iter()
        .find(|p| p.at.joint == name)
        .unwrap()
}
fn complete(size: &str) -> (Document, [u64; 6]) {
    let mut doc = Document::default();
    let furnace = add(&mut doc, FURNACE);
    let origin = doc
        .graphics
        .iter()
        .find(|g| g.id == furnace)
        .unwrap()
        .origin;
    let tube = add(&mut doc, TUBE);
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    let g = doc.graphics.iter_mut().find(|g| g.id == tube).unwrap();
    g.origin = origin;
    g.axis_x = Point::new(0., scale);
    g.axis_y = Point::new(-scale, 0.);
    solve::connect(
        &mut doc,
        &[tube],
        &joint(furnace, "bore"),
        &joint(tube, "span"),
    )
    .unwrap();
    let mut ids = [furnace, tube, 0, 0, 0, 0];
    for (k, end) in ["left", "right"].into_iter().enumerate() {
        let seal = add(
            &mut doc,
            &format!("reshiki/work-tube-end-seal-compression-32-{size}"),
        );
        solve::connect(
            &mut doc,
            &[seal],
            &joint(tube, end),
            &joint(seal, "work-tube"),
        )
        .unwrap();
        let line = add(&mut doc, &format!("reshiki/compression-tube-120-{size}"));
        solve::connect(
            &mut doc,
            &[line],
            &joint(seal, "gas-line"),
            &joint(line, "a"),
        )
        .unwrap();
        ids[2 + 2 * k..4 + 2 * k].copy_from_slice(&[seal, line]);
    }
    (doc, ids)
}

#[test]
fn vertical_tube_furnace_and_direct_seals_keep_six_identities_and_five_seats() {
    for (size, fixture) in [
        (
            "1-4",
            include_bytes!("../../../../tests/fixtures/vertical-tube-furnace/connected-1-4.rsk")
                .as_slice(),
        ),
        (
            "1-8",
            include_bytes!("../../../../tests/fixtures/vertical-tube-furnace/connected-1-8.rsk")
                .as_slice(),
        ),
    ] {
        let saved = Document::from_json(fixture).unwrap();
        saved.validate().unwrap();
        assert_eq!((saved.graphics.len(), saved.seats.len()), (6, 5));
        for definition in &saved.piece_definitions {
            assert_eq!(
                definition.as_ref(),
                builtin::find(&definition.id).unwrap().as_ref()
            );
        }
        let (mut doc, ids) = complete(size);
        doc.validate().unwrap();
        assert_eq!((doc.graphics.len(), doc.seats.len()), (6, 5));
        assert_eq!(solve::assembly(&doc, ids[0]).len(), 6);
        let bore = port(&doc, ids[0], "bore");
        assert_eq!(bore.kind, Kind::Hold(HoldKind::ThroughBore, [32.; 2]));
        assert!((bore.contact_length / bore.mm - 300.).abs() < 1e-4);
        assert!(bore.dir[0].abs() < 1e-6 && (bore.dir[1] - 1.).abs() < 1e-6);
        let frames: Vec<_> = doc
            .graphics
            .iter()
            .map(|g| (g.id, g.origin, g.axis_x, g.axis_y))
            .collect();
        let seats = doc.seats.clone();
        let before: Vec<_> = doc
            .graphics
            .iter()
            .flat_map(ports)
            .map(|p| (p.at, p.kind, p.point, p.dir, p.length))
            .collect();
        for view in ["exterior", "section"] {
            for id in ids {
                assert!(
                    solve::adjust::set(&mut doc, id, "view", Value::State(view.into()))
                        .unwrap()
                        .is_empty()
                );
            }
            assert_eq!(doc.seats, seats);
            assert_eq!(
                doc.graphics
                    .iter()
                    .map(|g| (g.id, g.origin, g.axis_x, g.axis_y))
                    .collect::<Vec<_>>(),
                frames
            );
            assert_eq!(
                doc.graphics
                    .iter()
                    .flat_map(ports)
                    .map(|p| (p.at, p.kind, p.point, p.dir, p.length))
                    .collect::<Vec<_>>(),
                before
            );
            let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
            assert_eq!((back.graphics.len(), back.seats.len()), (6, 5));
            assert_eq!(back.seats, seats);
            write_evidence(&doc, size, view);
        }
    }
}

#[test]
fn vertical_tube_furnace_direct_seals_use_exact_standard_od_and_true_tube_stops() {
    for (size, od, insertion) in [("1-4", 6.35, 13.5), ("1-8", 3.175, 10.5)] {
        let seal =
            builtin::find(&format!("reshiki/work-tube-end-seal-compression-32-{size}")).unwrap();
        let union = builtin::find(&format!("reshiki/compression-union-{size}")).unwrap();
        let socket = seal.joints.iter().find(|p| p.id() == "gas-line").unwrap();
        let standard = union.joints.iter().find(|p| p.id() == "right").unwrap();
        let stop = |j: &Joint| match j {
            Joint::Plug { insertion_mm, .. } => *insertion_mm,
            _ => None,
        };
        assert_eq!(stop(socket), Some(insertion));
        assert_eq!(stop(socket), stop(standard));
        assert_eq!(socket.at(), [54., 0.]);
        assert_eq!(socket.gender(), Gender::F);
        let (doc, ids) = complete(size);
        for (seal, line) in [(ids[2], ids[3]), (ids[4], ids[5])] {
            let gas = port(&doc, seal, "gas-line");
            let end = port(&doc, line, "a");
            assert_eq!(gas.kind, Kind::Plug(PlugFamily::Compression, od));
            assert!(solve::mates(&gas, &end));
            let depth =
                super::geometry::dot(super::geometry::sub(gas.point, end.point), gas.dir) / gas.mm;
            assert!((depth - insertion).abs() < 1e-4);
            for wrong in [
                Kind::Plug(PlugFamily::Compression, if od == 6.35 { 6. } else { 6.35 }),
                Kind::Plug(PlugFamily::Hose, od),
                Kind::Plug(PlugFamily::WorkTube, od),
            ] {
                assert!(!solve::mates(
                    &gas,
                    &Port {
                        kind: wrong,
                        ..end.clone()
                    }
                ));
            }
            let work = port(&doc, seal, "work-tube");
            assert_eq!(work.kind, Kind::Plug(PlugFamily::WorkTube, 32.));
            assert_eq!(work.gender, Gender::F);
            let tip = if seal == ids[2] {
                port(&doc, ids[1], "left")
            } else {
                port(&doc, ids[1], "right")
            };
            let depth = super::geometry::dot(super::geometry::sub(work.point, tip.point), work.dir)
                / work.mm;
            assert!((depth - 18.).abs() < 1e-4);
        }
    }
}

#[test]
fn vertical_tube_furnace_keeps_both_ends_and_frame_clear_at_each_travel_limit() {
    for size in ["1-4", "1-8"] {
        let (doc, ids) = complete(size);
        let origin = doc.graphics.iter().find(|g| g.id == ids[0]).unwrap().origin;
        let mm = port(&doc, ids[1], "span").mm;
        let moving = solve::carried_through(&doc, &ids, &[ids[1]], &[]).unwrap();
        assert_eq!(moving.len(), 5);
        assert!(!moving.contains(&ids[0]));
        for view in ["exterior", "section"] {
            let definition = builtin::find(FURNACE).unwrap();
            let values = Values::from([("view".into(), Value::State(view.into()))]);
            let evaluated = evaluate(&definition, &resolve(&definition, &values).unwrap()).unwrap();
            let mut case_extent = 0_f64;
            // Each contour is separate: side feet must not become a fictitious
            // crossbar in the open centre of the upright frame.
            for drawn in &evaluated.shapes {
                if !drawn.shape.role.filled() {
                    continue;
                }
                for (points, _) in flatten(&drawn.path) {
                    let lo = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                    let hi = points
                        .iter()
                        .map(|p| p[0])
                        .fold(f64::NEG_INFINITY, f64::max);
                    if lo <= 33. && hi >= -33. {
                        for p in points {
                            case_extent = case_extent.max(p[1].abs());
                        }
                    }
                }
            }
            assert_eq!(
                case_extent, 155.,
                "the frame must leave both tube ends open"
            );
            let mut viewed = doc.clone();
            for id in ids {
                solve::adjust::set(&mut viewed, id, "view", Value::State(view.into())).unwrap();
            }
            for direction in [-1., 1.] {
                let delta = solve::constrain(&viewed, &moving, [100., direction * 1e6]).unwrap();
                assert!(delta[0].abs() < 1e-6 && (delta[1] / mm - direction * 120.).abs() < 1e-3);
                let mut slid = viewed.clone();
                slid.translate(&moving, delta[0] as f32, delta[1] as f32);
                solve::check(&mut slid, None);
                assert_eq!(slid.seats, doc.seats);
                let near = if direction > 0. { ids[2] } else { ids[4] };
                let lip = port(&slid, near, "work-tube");
                let distance = (lip.point[1] - f64::from(origin.y)).abs() / mm;
                assert!((distance - case_extent - 7.).abs() < 1e-3);
                assert_eq!(
                    slid.graphics
                        .iter()
                        .find(|g| g.id == ids[0])
                        .unwrap()
                        .origin,
                    origin
                );
                for seat in &slid.seats {
                    let (off, angle) = residual(
                        &port(&slid, seat.a.piece, &seat.a.joint),
                        &port(&slid, seat.b.piece, &seat.b.joint),
                    );
                    assert!(off < 1e-3 && angle < 1e-3);
                }
                assert_eq!(
                    Document::from_json(&slid.file_json().unwrap())
                        .unwrap()
                        .seats,
                    doc.seats
                );
            }
        }
    }
}

#[test]
fn vertical_tube_furnace_direct_seal_sections_have_one_open_gas_passage() {
    for size in ["1-4", "1-8"] {
        let definition =
            builtin::find(&format!("reshiki/work-tube-end-seal-compression-32-{size}")).unwrap();
        let values = Values::from([("view".into(), Value::State("section".into()))]);
        let evaluated = evaluate(&definition, &resolve(&definition, &values).unwrap()).unwrap();
        for x in [0., 18., 24., 28., 34., 40.5, 43.5, 50., 54.] {
            for drawn in &evaluated.shapes {
                if drawn.shape.role.filled() {
                    assert!(
                        !inside([x, 0.], &flatten(&drawn.path)),
                        "{} {} blocks x={x}",
                        definition.id,
                        drawn.shape.id
                    );
                }
            }
        }
        for view in ["exterior", "section"] {
            let mut check = (*definition).clone();
            if let super::format::Param::States { default, .. } = &mut check.params[0] {
                *default = view.into();
            }
            super::validate::validate(&check).unwrap();
        }
    }
}

fn write_evidence(source: &Document, size: &str, view: &str) {
    let Ok(directory) = std::env::var("RESHIKI_VERTICAL_FURNACE_EVIDENCE") else {
        return;
    };
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    for (name, theme) in [
        ("light", crate::canvas_theme::CanvasTheme::Light),
        ("dark", crate::canvas_theme::CanvasTheme::Dark),
    ] {
        let mut doc = source.clone();
        doc.canvas_theme = theme;
        std::fs::write(
            directory.join(format!("vertical-furnace-{size}-{view}-{name}.rsk")),
            doc.file_json().unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join(format!("vertical-furnace-{size}-{view}-{name}.svg")),
            crate::scene::svg_with_background(&doc),
        )
        .unwrap();
    }
}
