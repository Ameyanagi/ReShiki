//! One independent work tube through a furnace, terminating seals and gas lines.
use super::builtin;
use super::eval::{Value, Values, evaluate, resolve};
use super::format::{Gender, HoldKind, Joint, Material, PlugFamily};
use super::geometry::{flatten, inside};
use super::placed;
use super::solve::{self, JointRef, Kind, Port, ports, residual};
use crate::document::{Document, Point};

const FURNACE: &str = "reshiki/tube-furnace-32";
const TUBE: &str = "reshiki/quartz-work-tube-600-32";
const SEAL: &str = "reshiki/work-tube-end-seal-32";
const ADAPTER: &str = "reshiki/work-tube-gas-port-adapter-1-4";

fn add(doc: &mut Document, name: &str) -> u64 {
    let id = doc.next_id();
    let definition = builtin::find(name).unwrap();
    let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics
        .push(placed::graphic(id, definition, Point::default(), scale));
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

/// All eight pieces are separately countable; the seven seats include one
/// mechanical through-bore relation, two work-tube seals and four gas seats.
fn complete() -> (Document, [u64; 8]) {
    let mut doc = Document::default();
    let furnace = add(&mut doc, FURNACE);
    let tube = add(&mut doc, TUBE);
    let furnace_origin = doc
        .graphics
        .iter()
        .find(|g| g.id == furnace)
        .unwrap()
        .origin;
    doc.graphics
        .iter_mut()
        .find(|g| g.id == tube)
        .unwrap()
        .origin = furnace_origin;
    solve::connect(
        &mut doc,
        &[tube],
        &joint(furnace, "bore"),
        &joint(tube, "span"),
    )
    .unwrap();
    let mut ids = [furnace, tube, 0, 0, 0, 0, 0, 0];
    for (k, end) in ["left", "right"].into_iter().enumerate() {
        let seal = add(&mut doc, SEAL);
        solve::connect(
            &mut doc,
            &[seal],
            &joint(tube, end),
            &joint(seal, "work-tube"),
        )
        .unwrap();
        let adapter = add(&mut doc, ADAPTER);
        solve::connect(
            &mut doc,
            &[adapter],
            &joint(seal, "gas-nozzle"),
            &joint(adapter, "nozzle"),
        )
        .unwrap();
        let line = add(&mut doc, "reshiki/compression-tube-120-1-4");
        solve::connect(
            &mut doc,
            &[line],
            &joint(adapter, "gas-line"),
            &joint(line, "a"),
        )
        .unwrap();
        ids[2 + k * 3..5 + k * 3].copy_from_slice(&[seal, adapter, line]);
    }
    (doc, ids)
}

#[test]
fn tube_furnace_work_tube_interfaces_are_exact_and_distinct_from_gas_seats() {
    let (doc, ids) = complete();
    let end = port(&doc, ids[1], "right");
    let socket = port(&doc, ids[5], "work-tube");
    assert_eq!(end.kind, Kind::Plug(PlugFamily::WorkTube, 32.));
    assert_eq!(socket.kind, end.kind);
    assert_eq!((end.gender, socket.gender), (Gender::M, Gender::F));
    assert!(solve::mates(&end, &socket));
    assert_eq!(socket.size(), "Ø32 mm work tube");
    for kind in [
        Kind::Plug(PlugFamily::WorkTube, 31.99),
        Kind::Plug(PlugFamily::Compression, 32.),
        Kind::Plug(PlugFamily::Hose, 32.),
    ] {
        assert!(!solve::mates(
            &end,
            &Port {
                kind,
                ..socket.clone()
            }
        ));
    }
    let nozzle = port(&doc, ids[5], "gas-nozzle");
    let adapter = port(&doc, ids[6], "nozzle");
    assert_eq!(nozzle.kind, Kind::Plug(PlugFamily::Compression, 6.35));
    assert!(solve::mates(&nozzle, &adapter));
    assert!(!solve::mates(
        &nozzle,
        &Port {
            kind: Kind::Plug(PlugFamily::Compression, 6.),
            ..adapter
        }
    ));
    let depth =
        super::geometry::dot(super::geometry::sub(socket.point, end.point), socket.dir) / socket.mm;
    assert!((depth - 18.).abs() < 1e-4);

    let mut invalid = (*builtin::find(SEAL).unwrap()).clone();
    if let Joint::Plug { gender, .. } = &mut invalid.joints[0] {
        *gender = Gender::M;
    }
    assert!(
        super::validate::validate(&invalid)
            .unwrap_err()
            .contains("female compression or work-tube")
    );
}

#[test]
fn tube_furnace_keeps_one_tube_and_all_seven_seats_across_views_and_reopen() {
    let fixture = Document::from_json(include_bytes!(
        "../../../../docs/fixtures/tube-furnace/connected-exterior.rsk"
    ))
    .unwrap();
    fixture.validate().unwrap();
    assert_eq!((fixture.graphics.len(), fixture.seats.len()), (8, 7));
    for definition in &fixture.piece_definitions {
        assert_eq!(
            definition.as_ref(),
            builtin::find(&definition.id).unwrap().as_ref(),
            "the native example must use the current catalogue artwork"
        );
    }
    let (mut doc, ids) = complete();
    doc.validate().unwrap();
    assert_eq!(doc.graphics.len(), 8);
    assert_eq!(doc.seats.len(), 7);
    assert_eq!(solve::assembly(&doc, ids[0]).len(), 8);
    let frames: Vec<_> = doc
        .graphics
        .iter()
        .map(|g| (g.id, g.origin, g.axis_x, g.axis_y))
        .collect();
    let seats = doc.seats.clone();
    let before_ports: Vec<_> = doc
        .graphics
        .iter()
        .flat_map(ports)
        .map(|p| (p.at, p.kind, p.point, p.dir, p.length))
        .collect();
    for view in ["section", "exterior"] {
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
            before_ports
        );
        for seat in &doc.seats {
            let a = port(&doc, seat.a.piece, &seat.a.joint);
            let b = port(&doc, seat.b.piece, &seat.b.joint);
            let (off, angle) = residual(&a, &b);
            assert!(off < 1e-3 && angle < 1e-3);
        }
        let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(back.seats, seats);
        assert_eq!(back.graphics.len(), 8);
        write_evidence(&doc, view);
    }
}

#[test]
fn tube_furnace_reserves_both_ends_and_keeps_seals_clear_at_each_slide_limit() {
    let (doc, ids) = complete();
    let span = port(&doc, ids[1], "span");
    let bore = port(&doc, ids[0], "bore");
    assert_eq!(span.kind, Kind::Hold(HoldKind::TubeSpan, [32.; 2]));
    assert_eq!(bore.kind, Kind::Hold(HoldKind::ThroughBore, [32.; 2]));
    assert!((bore.contact_length / bore.mm - 300.).abs() < 1e-4);
    assert!((span.length / span.mm - 540.).abs() < 1e-4);
    let tube = builtin::find(TUBE).unwrap();
    assert_eq!(tube.holds[0].from, [-270., 0.]);
    assert_eq!(tube.holds[0].to, [270., 0.]);
    assert_eq!(tube.joints[0].at(), [-300., 0.]);
    assert_eq!(tube.joints[1].at(), [300., 0.]);
    assert!(builtin::find(FURNACE).unwrap().joints.is_empty());
    let mut short = span.clone();
    short.length = 299. * short.mm;
    assert!(!solve::mates(&short, &bore));
    let moving = solve::carried_through(&doc, &ids, &[ids[1]], &[]).unwrap();
    assert_eq!(moving.len(), 7);
    assert!(!moving.contains(&ids[0]));
    let furnace_origin = doc.graphics.iter().find(|g| g.id == ids[0]).unwrap().origin;
    for view in ["exterior", "section"] {
        // Only material at the seal's height constrains this assembly:
        // the wider controller base is below it. Derive the boundaries
        // from the artwork so an aperture or seal edit cannot hide a clash.
        let furnace_bounds = material_x_bounds_at_seal_height(FURNACE, view);
        assert_eq!(furnace_bounds, [-155., 155.]);
        let seal_bounds = material_x_bounds_at_seal_height(SEAL, view);
        assert_eq!(seal_bounds[0], 0.);
        let mut viewed = doc.clone();
        for id in ids {
            assert!(
                solve::adjust::set(&mut viewed, id, "view", Value::State(view.into()))
                    .unwrap()
                    .is_empty()
            );
        }
        for direction in [-1., 1.] {
            let delta = solve::constrain(&viewed, &moving, [direction * 1e6, 100.]).unwrap();
            assert!((delta[0] / span.mm - direction * 120.).abs() < 1e-3);
            assert!(delta[1].abs() < 1e-6);
            let mut slid = viewed.clone();
            slid.translate(&moving, delta[0] as f32, delta[1] as f32);
            solve::check(&mut slid, None);
            assert_eq!(slid.seats, doc.seats);
            assert_eq!(
                slid.graphics
                    .iter()
                    .find(|g| g.id == ids[0])
                    .unwrap()
                    .origin,
                furnace_origin
            );
            let nearest_seal = if direction > 0. { ids[2] } else { ids[5] };
            let lip = port(&slid, nearest_seal, "work-tube");
            let lip_distance = (lip.point[0] - f64::from(furnace_origin.x)).abs() / span.mm;
            assert!((lip_distance - 162.).abs() < 1e-3);
            let case_extent = if direction > 0. {
                -furnace_bounds[0]
            } else {
                furnace_bounds[1]
            };
            let gap_mm = lip_distance - case_extent;
            assert!((gap_mm - 7.).abs() < 1e-3);
            // Two 0.6 pt outlines leave a visible gap at the default scale.
            assert!(gap_mm * f64::from(placed::DEFAULT_PT_PER_MM) - 0.6 > 2.);
            let reopened = Document::from_json(&slid.file_json().unwrap()).unwrap();
            assert_eq!(reopened.seats, doc.seats);
        }
    }
}

fn material_x_bounds_at_seal_height(id: &str, view: &str) -> [f64; 2] {
    let definition = builtin::find(id).unwrap();
    let values = Values::from([("view".into(), Value::State(view.into()))]);
    let evaluated = evaluate(&definition, &resolve(&definition, &values).unwrap()).unwrap();
    let mut bounds = [f64::INFINITY, f64::NEG_INFINITY];
    for drawn in &evaluated.shapes {
        if !drawn.shape.role.filled() {
            continue;
        }
        let points: Vec<_> = flatten(&drawn.path).into_iter().flatten().collect();
        let low = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let high = points
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        if low <= 33. && high >= 16. {
            for point in points {
                bounds[0] = bounds[0].min(point[0]);
                bounds[1] = bounds[1].max(point[0]);
            }
        }
    }
    assert!(bounds.iter().all(|v| v.is_finite()));
    bounds
}

#[test]
fn tube_furnace_section_removes_shell_material_from_the_continuous_gas_passage() {
    for (id, points) in [
        (FURNACE, vec![[-100., 0.], [0., 0.], [100., 0.]]),
        (SEAL, vec![[0., 0.], [18., 0.], [27., 0.], [40., 0.]]),
        (
            ADAPTER,
            vec![[0., 0.], [12., 0.], [18., 0.], [24., 0.], [36., 0.]],
        ),
    ] {
        let definition = builtin::find(id).unwrap();
        let values = Values::from([("view".into(), Value::State("section".into()))]);
        let evaluated = evaluate(&definition, &resolve(&definition, &values).unwrap()).unwrap();
        for point in points {
            for drawn in &evaluated.shapes {
                if drawn.shape.role.filled()
                    && matches!(
                        drawn.shape.material,
                        Material::Metal | Material::Porcelain | Material::Rubber
                    )
                {
                    assert!(
                        !inside(point, &flatten(&drawn.path)),
                        "{id} {} blocks {point:?}",
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

/// Opt in to fresh exact-code review artifacts with
/// RESHIKI_TUBE_FURNACE_EVIDENCE=/absolute/path cargo test -p reshiki-model tube_furnace
fn write_evidence(source: &Document, view: &str) {
    let Ok(directory) = std::env::var("RESHIKI_TUBE_FURNACE_EVIDENCE") else {
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
            directory.join(format!("tube-furnace-{view}-{name}.rsk")),
            doc.file_json().unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join(format!("tube-furnace-{view}-{name}.svg")),
            crate::scene::svg_with_background(&doc),
        )
        .unwrap();
    }
}
