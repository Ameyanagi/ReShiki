//! Nominal pipe thread identity is distinct from a tube's outside diameter.
use super::builtin;
use super::eval::{Value, evaluate, resolve};
use super::format::Gender::{F, M};
use super::format::{self, Gender, PlugFamily};
use super::placed;
use super::solve::{self, JointRef, Kind, Port};
use crate::document::{Document, Point};
use crate::graphics::Graphic;

fn port(family: PlugFamily, size: f64, gender: Gender) -> Port {
    Port {
        at: JointRef {
            piece: 1,
            joint: "process".into(),
        },
        kind: Kind::Plug(family, size),
        gender,
        point: [0., 0.],
        dir: [-1., 0.],
        mm: 1.,
        length: 0.,
        contact_length: 0.,
        insertion: 0.,
    }
}

#[test]
fn npt_requires_the_same_thread_family_nominal_size_and_opposite_gender() {
    let socket = port(PlugFamily::Npt, 6.35, Gender::F);
    assert!(solve::mates(
        &socket,
        &port(PlugFamily::Npt, 6.35, Gender::M)
    ));
    for other in [
        port(PlugFamily::Npt, 6.35, Gender::F),
        port(PlugFamily::Npt, 6., Gender::M),
        port(PlugFamily::Npt, 3.175, Gender::M),
        port(PlugFamily::Compression, 6.35, Gender::M),
        port(PlugFamily::Hose, 6.35, Gender::M),
    ] {
        assert!(!solve::mates(&socket, &other));
    }
    assert_eq!(socket.size(), "1/4 in NPT");
    assert_eq!(socket.diameter(), 13.716);
    assert_eq!(
        port(PlugFamily::Compression, 6.35, Gender::F).diameter(),
        6.35
    );
}

#[test]
fn npt_definition_serializes_its_type_and_rejects_unimplemented_nominal_sizes() {
    let text = r#"{
        "format":"reshiki-piece","format_version":0,"id":"test/thread",
        "revision":1,"family":"thread","variant":{},"names":{"en":"Thread"},
        "category":"other","units":"mm",
        "shapes":[{"id":"body","layer":"body","material":"metal","role":"solid",
                   "path":"M-10 -8H10V8H-10Z"}],
        "joints":[{"id":"process","kind":"plug","family":"npt","gender":"F",
                   "size_mm":6.35,"at":[-10,0],"dir":[-1,0]}]
    }"#;
    let definition = format::parse(text).unwrap();
    assert!(
        serde_json::to_string(&definition)
            .unwrap()
            .contains("\"family\":\"npt\"")
    );
    let error = format::parse(&text.replace("\"size_mm\":6.35", "\"size_mm\":3.175")).unwrap_err();
    assert!(error.contains("only 1/4 in NPT"), "{error}");
    let with_depth = text.replace("\"size_mm\":6.35", "\"size_mm\":6.35,\"insertion_mm\":5");
    assert!(format::parse(&with_depth).is_ok());
    let error =
        format::parse(&with_depth.replace("\"gender\":\"F\"", "\"gender\":\"M\"")).unwrap_err();
    assert!(error.contains("only on a female compression"), "{error}");
}

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

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

fn connect(doc: &mut Document, fixed: u64, socket: &str, moving: u64, plug: &str) {
    let snap = solve::connect(doc, &[moving], &joint(fixed, socket), &joint(moving, plug)).unwrap();
    assert!(matches!(snap.offer, solve::Offer::Seat));
    assert_eq!(snap.target.at, joint(fixed, socket));
    assert_eq!(snap.moving.at, joint(moving, plug));
}

#[test]
fn sample_cylinders_keep_their_one_or_two_npt_seats_across_views() {
    for (variant, sockets) in [
        ("150", &["left", "right"][..]),
        ("300", &["left", "right"][..]),
        ("150-single-end", &["left"][..]),
        ("300-single-end", &["left"][..]),
    ] {
        let mut doc = Document::default();
        let cylinder = add(
            &mut doc,
            &format!("reshiki/gas-sample-cylinder-{variant}-1-4-npt"),
        );
        assert_eq!(solve::ports(graphic(&doc, cylinder)).len(), sockets.len());
        for &port in sockets {
            let adapter = add(&mut doc, "reshiki/gas-compression-npt-adapter-1-4");
            connect(&mut doc, cylinder, port, adapter, "thread");
            let socket = solve::ports(graphic(&doc, cylinder))
                .into_iter()
                .find(|p| p.at.joint == port)
                .unwrap();
            let tip = solve::ports(graphic(&doc, adapter))
                .into_iter()
                .find(|p| p.at.joint == "thread")
                .unwrap();
            let engaged_mm = (socket.point[0] - tip.point[0]) * socket.dir[0] / socket.mm;
            assert!((engaged_mm - 8.).abs() < solve::TOLERANCE_MM);
        }
        let seats = doc.seats.clone();
        let ports = all_ports(&doc);
        set(&mut doc, cylinder, "view", "section");
        assert_connected(&mut doc, &seats, &ports);
    }
}

fn set(doc: &mut Document, id: u64, param: &str, state: &str) {
    assert!(
        solve::adjust::set(doc, id, param, Value::State(state.into()))
            .unwrap()
            .is_empty()
    );
}

fn has_shape(doc: &Document, id: u64, name: &str) -> bool {
    let piece = graphic(doc, id).piece.as_ref().unwrap();
    let definition = piece.linked.as_ref().unwrap();
    evaluate(definition, &resolve(definition, &piece.params).unwrap())
        .unwrap()
        .shapes
        .iter()
        .any(|s| s.shape.id == name)
}

fn all_ports(doc: &Document) -> Vec<Port> {
    doc.graphics.iter().flat_map(solve::ports).collect()
}

fn assert_connected(doc: &mut Document, seats: &[solve::Seat], before: &[Port]) {
    solve::check(doc, None);
    assert_eq!(doc.seats, seats);
    let after = all_ports(doc);
    assert_eq!(after.len(), before.len());
    for (a, b) in after.iter().zip(before) {
        assert_eq!(
            (&a.at, &a.kind, a.gender, a.point, a.dir, a.mm, a.insertion),
            (&b.at, &b.kind, b.gender, b.point, b.dir, b.mm, b.insertion)
        );
    }
    for seat in seats {
        let find = |j: &JointRef| after.iter().find(|p| &p.at == j).unwrap();
        let (a, b) = (find(&seat.a), find(&seat.b));
        assert!(solve::mates(a, b));
        let (offset, angle) = solve::residual(a, b);
        assert!(offset < 1e-3 && angle < 1e-3, "{seat:?}: {offset}, {angle}");
    }
    assert_eq!(
        solve::assembly(doc, doc.graphics[0].id).len(),
        doc.graphics.len()
    );
    doc.validate().unwrap();
}

const NPT: Kind = Kind::Plug(PlugFamily::Npt, 6.35);
const COMPRESSION: Kind = Kind::Plug(PlugFamily::Compression, 6.35);

fn assert_plug(
    doc: &Document,
    id: u64,
    name: &str,
    kind: &Kind,
    gender: Gender,
    pose: ([f64; 2], [f64; 2]),
) {
    let p = solve::ports(graphic(doc, id))
        .into_iter()
        .find(|p| p.at.joint == name)
        .unwrap();
    assert_eq!(&p.kind, kind);
    assert_eq!(p.gender, gender);
    let piece = graphic(doc, id).piece.as_ref().unwrap();
    let definition = piece.linked.as_ref().unwrap();
    let evaluated = evaluate(definition, &resolve(definition, &piece.params).unwrap()).unwrap();
    let joint = evaluated
        .joints
        .iter()
        .find(|j| j.joint.id() == name)
        .unwrap();
    assert_eq!((joint.at, joint.dir), pose);
}

#[test]
fn builtin_gauge_seats_on_the_mixed_tee_and_keeps_both_tube_connections_across_views() {
    let mut doc = Document::default();
    let tee = add(&mut doc, "reshiki/gas-gauge-tee-1-4");
    let gauge = add(&mut doc, "reshiki/gas-pressure-gauge-1-4-npt");
    let mut ids = vec![tee, gauge];
    assert_plug(&doc, tee, "instrument", &NPT, F, ([0., -28.], [0., -1.]));
    assert_plug(&doc, gauge, "process", &NPT, M, ([0., 0.], [0., 1.]));
    connect(&mut doc, tee, "instrument", gauge, "process");
    for (side, x, direction) in [("left", -32., -1.), ("right", 32., 1.)] {
        assert_plug(&doc, tee, side, &COMPRESSION, F, ([x, 0.], [direction, 0.]));
        let tube = add(&mut doc, "reshiki/compression-tube-60-1-4");
        ids.push(tube);
        assert_plug(&doc, tube, "a", &COMPRESSION, M, ([0., 0.], [-1., 0.]));
        connect(&mut doc, tee, side, tube, "a");
    }
    let seats = doc.seats.clone();
    assert_eq!(seats.len(), 3);
    let ports = all_ports(&doc);
    let exterior = placed::plan(graphic(&doc, gauge));
    for view in ["section", "exterior"] {
        for &id in &ids {
            set(&mut doc, id, "view", view);
        }
        assert_eq!(has_shape(&doc, gauge, "dial-face"), view == "exterior");
        assert_eq!(has_shape(&doc, gauge, "bourdon-tube"), view == "section");
        if view == "section" {
            assert_ne!(placed::plan(graphic(&doc, gauge)), exterior);
        }
        assert_connected(&mut doc, &seats, &ports);
        let mut reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_connected(&mut reopened, &seats, &ports);
    }
}

#[test]
fn builtin_regulators_keep_both_thread_adapters_seated_when_the_plan_view_is_sectioned() {
    for name in [
        "reshiki/gas-pressure-reducing-regulator-1-4-npt",
        "reshiki/gas-back-pressure-regulator-1-4-npt",
    ] {
        let mut doc = Document::default();
        let regulator = add(&mut doc, name);
        let mut ids = vec![regulator];
        for (side, x, direction) in [("inlet", -44., -1.), ("outlet", 44., 1.)] {
            assert_plug(&doc, regulator, side, &NPT, F, ([x, 0.], [direction, 0.]));
            let adapter = add(&mut doc, "reshiki/gas-compression-npt-adapter-1-4");
            ids.push(adapter);
            assert_plug(&doc, adapter, "thread", &NPT, M, ([28., 0.], [1., 0.]));
            assert_plug(
                &doc,
                adapter,
                "compression",
                &COMPRESSION,
                F,
                ([-35., 0.], [-1., 0.]),
            );
            connect(&mut doc, regulator, side, adapter, "thread");
        }
        let seats = doc.seats.clone();
        assert_eq!(seats.len(), 2);
        let ports = all_ports(&doc);
        let exterior = placed::plan(graphic(&doc, regulator));
        for view in ["section", "exterior"] {
            for &id in &ids {
                set(&mut doc, id, "view", view);
            }
            assert_eq!(
                has_shape(&doc, regulator, "top-adjustment-knob"),
                view == "exterior"
            );
            assert_eq!(
                has_shape(&doc, regulator, "control-spring"),
                view == "section"
            );
            if view == "section" {
                assert_ne!(placed::plan(graphic(&doc, regulator)), exterior);
            }
            assert_connected(&mut doc, &seats, &ports);
        }
    }
}

#[test]
fn builtin_gas_valve_states_change_the_section_anatomy_without_releasing_fittings() {
    for (name, param, states, mate, plug, kind) in [
        (
            "reshiki/gas-ball-valve-1-4",
            "valve",
            &["closed", "open"][..],
            "reshiki/compression-tube-60-1-4",
            "a",
            COMPRESSION,
        ),
        (
            "reshiki/gas-needle-valve-1-4",
            "valve",
            &["closed", "part-open", "open"][..],
            "reshiki/compression-tube-60-1-4",
            "a",
            COMPRESSION,
        ),
        (
            "reshiki/gas-safety-relief-1-4-npt",
            "condition",
            &["seated", "relieving"][..],
            "reshiki/gas-compression-npt-adapter-1-4",
            "thread",
            NPT,
        ),
    ] {
        let mut doc = Document::default();
        let valve = add(&mut doc, name);
        let mut ids = vec![valve];
        let sockets = if kind == NPT {
            [
                ("inlet", [0., 24.], [0., 1.]),
                ("discharge", [40., -12.], [1., 0.]),
            ]
        } else {
            [
                ("inlet", [-24.5, 0.], [-1., 0.]),
                ("outlet", [24.5, 0.], [1., 0.]),
            ]
        };
        for (socket, at, dir) in sockets {
            assert_plug(&doc, valve, socket, &kind, F, (at, dir));
            let fitting = add(&mut doc, mate);
            ids.push(fitting);
            let pose = if kind == NPT {
                ([28., 0.], [1., 0.])
            } else {
                ([0., 0.], [-1., 0.])
            };
            assert_plug(&doc, fitting, plug, &kind, M, pose);
            connect(&mut doc, valve, socket, fitting, plug);
        }
        let seats = doc.seats.clone();
        assert_eq!(seats.len(), 2);
        let ports = all_ports(&doc);
        let mut exterior = None;
        for view in ["exterior", "section"] {
            for &id in &ids {
                set(&mut doc, id, "view", view);
            }
            let mut previous = None;
            for state in states {
                set(&mut doc, valve, param, state);
                let plan = placed::plan(graphic(&doc, valve));
                assert!(!plan.is_empty());
                if view == "section" {
                    let mechanism = if param == "condition" {
                        format!("poppet-{state}")
                    } else if name.contains("ball-valve") {
                        format!(
                            "ball-{state}-{}-bore",
                            if *state == "open" {
                                "horizontal"
                            } else {
                                "perpendicular"
                            }
                        )
                    } else {
                        "needle-section".into()
                    };
                    assert!(has_shape(&doc, valve, &mechanism), "{name}: {state}");
                    assert_ne!(Some(&plan), exterior.as_ref());
                    if let Some(before) = previous {
                        assert_ne!(plan, before, "{name}: {state}");
                    }
                } else {
                    exterior = Some(plan.clone());
                }
                previous = Some(plan);
                assert_connected(&mut doc, &seats, &ports);
            }
        }
    }
}
