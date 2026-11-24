use super::eval::Value;
use super::geometry::{add, len, mul, sub};
use super::plan::{Ink, Pass};
use super::solve::{JointRef, Port, adjust};
use super::tube_bend::{Bend, Draft, Route, Segment};
use super::{builtin, parts_list, placed, solve};
use crate::document::{Document, History, Point};
use crate::graphics::Graphic;
use std::f64::consts::PI;

const STRAIGHT: &str = "reshiki/compression-tube-120-1-4";
const PRESET: &str = "reshiki/compression-tube-bend-90-1-4";
const CAP: &str = "reshiki/compression-cap-1-4";

fn add_piece(doc: &mut Document, definition: &str) -> u64 {
    let id = doc.next_id();
    let mm = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic(
        id,
        builtin::find(definition).unwrap(),
        Point::new(200., 200.),
        mm,
    ));
    id
}

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

fn port(doc: &Document, id: u64, name: &str) -> Port {
    solve::ports(graphic(doc, id))
        .into_iter()
        .find(|p| p.at.joint == name)
        .unwrap()
}

fn world(g: &Graphic, [x, y]: [f64; 2]) -> Point {
    g.origin.offset(
        g.axis_x.x * x as f32 + g.axis_y.x * y as f32,
        g.axis_x.y * x as f32 + g.axis_y.y * y as f32,
    )
}

fn cap(doc: &mut Document, tube: u64, end: &str) -> u64 {
    let cap = add_piece(doc, CAP);
    solve::connect(
        doc,
        &[cap],
        &port(doc, tube, end).at,
        &port(doc, cap, "tube").at,
    )
    .unwrap();
    cap
}

fn replace(doc: &mut Document, candidate: Graphic) {
    let i = doc
        .graphics
        .iter()
        .position(|g| g.id == candidate.id)
        .unwrap();
    doc.graphics[i] = candidate;
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-3, "{a} != {b}");
}

#[test]
fn first_bend_uses_the_clicked_leg_and_analytic_cut_length() {
    let mut doc = Document::default();
    let tube = add_piece(&mut doc, STRAIGHT);
    let original = doc.clone();
    let g = graphic(&doc, tube);
    let draft = Draft::new(&doc, tube, world(g, [47., 1.])).unwrap();
    assert!(draft.pivot().distance(world(g, [47., 0.])) < 1e-4);
    let drop = world(g, [47., 29.]);
    near(draft.angle_to(drop).unwrap(), 90.);
    let bent = draft.preview(90., 12.).unwrap();
    assert_eq!(doc, original);
    let route = bent.piece.as_ref().unwrap().tube_route.as_ref().unwrap();
    near(route.end[0], 47.);
    near(route.end[1], 73.);
    let line = route.centerline().unwrap();
    near(line.length_mm, 120. - 24. + 6. * PI);
    let [
        Segment::Straight { to, .. },
        Segment::Arc {
            center,
            from,
            sweep_rad,
        },
        Segment::Straight { from: exit, .. },
    ] = line.segments.as_slice()
    else {
        panic!("expected a straight, a circular bend and a straight");
    };
    assert_eq!(to, from);
    near(len(sub(*from, *center)), 12.);
    near(len(sub(*exit, *center)), 12.);
    near(*sweep_rad, PI / 2.);
    assert!(graphic(&doc, tube).hit(world(g, [32., 0.]), 0.1));
    assert!(bent.hit(world(g, [47., 55.]), 0.1));
    assert!(!bent.hit(world(g, [91., 0.]), 0.1));
    replace(&mut doc, bent);
    let ports = solve::ports(graphic(&doc, tube));
    near(
        ports.iter().find(|p| p.at.joint == "span").unwrap().length / port(&doc, tube, "a").mm,
        35.,
    );
    assert!(ports.iter().any(|p| p.at.joint == "route-span-1"));
    let report = parts_list::parts_list(&doc, &[tube]).unwrap();
    assert_eq!(report[0].variant["geometry"], "1 custom bend");
    assert!(!report[0].variant.contains_key("length"));
    near(
        report[0].tube.as_ref().unwrap().cut_length_mm,
        line.length_mm,
    );
}

#[test]
fn one_seated_end_keeps_its_position_and_tangent_in_rotated_and_mirrored_drawings() {
    for (end, mirrored) in [("a", false), ("b", true)] {
        let mut doc = Document::default();
        let tube = add_piece(&mut doc, STRAIGHT);
        let g = doc.graphics.first_mut().unwrap();
        let scale = g.axis_x.x;
        g.axis_x = Point::new(0., scale);
        g.axis_y = Point::new(if mirrored { scale } else { -scale }, 0.);
        let _cap = cap(&mut doc, tube, end);
        let before = doc.clone();
        let fixed = port(&doc, tube, end);
        let g = graphic(&doc, tube);
        let click = world(g, [50., 0.]);
        let draft = Draft::new(&doc, tube, click).unwrap();
        let cursor = click.offset(-20., 0.);
        let signed = draft.angle_to(cursor).unwrap();
        near(signed, if mirrored { -90. } else { 90. });
        let candidate = draft.preview(signed, 14.).unwrap();
        replace(&mut doc, candidate);
        solve::check(&mut doc, None);
        assert_eq!(doc.seats, before.seats);
        assert_eq!(doc.graphics[1], before.graphics[1]);
        let after = port(&doc, tube, end);
        near(len(sub(after.point, fixed.point)), 0.);
        near(len(sub(after.dir, fixed.dir)), 0.);
    }
}

#[test]
fn invalid_edits_report_the_reason_and_leave_the_original_assembly() {
    let mut doc = Document::default();
    let tube = add_piece(&mut doc, STRAIGHT);
    let _cap = cap(&mut doc, tube, "a");
    let g = graphic(&doc, tube);
    let draft = Draft::new(&doc, tube, world(g, [19., 0.])).unwrap();
    assert!(
        draft
            .preview(90., 10.)
            .unwrap_err()
            .contains("seated fitting")
    );
    assert!(
        draft
            .preview(174., 20.)
            .unwrap_err()
            .contains("does not fit")
    );
    assert!(
        draft
            .preview(90., 3.175)
            .unwrap_err()
            .contains("tube diameter")
    );
    for angle in [0., -180., 181., f64::NAN] {
        assert!(draft.preview(angle, 10.).is_err());
    }
    assert!(draft.preview(90., f64::INFINITY).is_err());
    let _cap = cap(&mut doc, tube, "b");
    let before = doc.clone();
    let g = graphic(&doc, tube);
    let both = Draft::new(&doc, tube, world(g, [50., 0.])).unwrap();
    assert!(
        both.preview(30., 18.)
            .unwrap_err()
            .contains("Both tube ends")
    );
    assert_eq!(doc, before);
}

#[test]
fn preset_radius_can_change_between_two_seated_fittings() {
    let mut doc = Document::default();
    let tube = add_piece(&mut doc, PRESET);
    let _cap = cap(&mut doc, tube, "a");
    let _cap = cap(&mut doc, tube, "b");
    let before = doc.clone();
    let g = graphic(&doc, tube);
    let mid_arc = 20. / 2_f64.sqrt();
    let draft = Draft::new(&doc, tube, world(g, [60. + mid_arc, 20. - mid_arc])).unwrap();
    assert!(draft.is_existing_bend());
    near(draft.default_angle_deg(), 90.);
    near(draft.default_radius_mm(), 20.);
    assert_eq!(draft.preview(90., 20.).unwrap(), *g);
    assert!(
        draft
            .preview(80., 16.)
            .unwrap_err()
            .contains("radius can change")
    );
    let candidate = draft.preview(90., 16.).unwrap();
    replace(&mut doc, candidate);
    solve::check(&mut doc, None);
    assert_eq!(doc.seats, before.seats);
    for name in ["a", "b"] {
        let (new, old) = (port(&doc, tube, name), port(&before, tube, name));
        near(len(sub(new.dir, old.dir)), 0.);
        near(len(sub(new.point, old.point)), 0.);
    }
}

#[test]
fn adjacent_bends_reserve_a_straight_leg_between_tangencies() {
    let mut r = Route {
        start: [0., 0.],
        bends: vec![
            Bend {
                corner: [35., 0.],
                radius_mm: 10.,
            },
            Bend {
                corner: [35., 21.],
                radius_mm: 10.,
            },
        ],
        end: [70., 21.],
    };
    let curve = r.centerline().unwrap();
    near(curve.length_mm, 91. - 40. + 10. * PI);
    r.bends[1].corner[1] = 20.;
    r.end[1] = 20.;
    assert!(r.centerline().unwrap_err().contains("does not fit"));
}

#[test]
fn a_second_bend_and_section_reopen_with_their_exact_native_outline() {
    let mut doc = Document::default();
    let tube = add_piece(&mut doc, STRAIGHT);
    let _cap = cap(&mut doc, tube, "a");
    let g = graphic(&doc, tube);
    let first = Draft::new(&doc, tube, world(g, [45., 0.]))
        .unwrap()
        .preview(90., 11.)
        .unwrap();
    replace(&mut doc, first);
    let mut history = History::default();
    let before = doc.clone();
    let g = graphic(&doc, tube);
    let second = Draft::new(&doc, tube, world(g, [45., 42.]))
        .unwrap()
        .preview(-65., 13.)
        .unwrap();
    replace(&mut doc, second);
    adjust::set(&mut doc, tube, "view", Value::State("section".into())).unwrap();
    let change =
        crate::transaction::reconcile_with_layout_anchor(&mut doc, before.clone(), tube).unwrap();
    assert_eq!(doc.seats, before.seats);
    assert!(change.commit(&mut doc, &mut history, false).recorded);
    let saved = doc.file_json().unwrap();
    let reopened = Document::from_json(&saved).unwrap();
    assert_eq!(reopened.version, crate::document::TUBE_BENDS);
    assert_eq!(doc.current(), reopened);
    let g = graphic(&doc, tube);
    let plan = placed::plan(g);
    assert!(
        plan.iter()
            .any(|p| p.material == super::format::Material::Paper && p.ink == Ink::Fill)
    );
    assert!(plan.iter().any(|p| p.pass == Pass::Body));
    assert!(plan.iter().any(|p| {
        p.path
            .iter()
            .any(|s| matches!(s, super::geometry::Seg::Cubic(..)))
    }));
    assert_eq!(
        plan.as_ref(),
        placed::plan(graphic(&reopened, tube)).as_ref()
    );
    let after = doc.clone();
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    assert!(history.redo(&mut doc));
    assert_eq!(doc, after);
}

#[test]
fn a_through_fitting_keeps_its_span_when_an_invalid_bend_is_rejected() {
    let mut doc = Document::default();
    let tube = add_piece(&mut doc, STRAIGHT);
    let bore = add_piece(
        &mut doc,
        "reshiki/compression-bulkhead-union-bored-through-1-4",
    );
    let span = port(&doc, tube, "span");
    let socket = port(&doc, bore, "bore");
    let offset = sub(add(span.point, mul(span.dir, 60. * span.mm)), socket.point);
    doc.translate(&[bore], offset[0] as f32, offset[1] as f32);
    solve::connect(&mut doc, &[bore], &span.at, &socket.at).unwrap();
    let before = doc.clone();
    let g = graphic(&doc, tube);
    let draft = Draft::new(&doc, tube, world(g, [48., 0.])).unwrap();
    assert!(
        draft
            .preview(45., 8.)
            .unwrap_err()
            .contains("through fitting")
    );
    assert_eq!(doc, before);
    assert_eq!(doc.seats[0].a.piece, tube);
    assert_eq!(
        solve::partner(
            &doc,
            &JointRef {
                piece: tube,
                joint: "span".into()
            }
        )
        .unwrap()
        .piece,
        bore
    );
}

#[test]
fn both_free_cut_ends_extend_along_their_own_tangents() {
    let mut doc = Document::default();
    let tube = add_piece(&mut doc, STRAIGHT);
    let g = doc.graphics.first_mut().unwrap();
    let s = g.axis_x.x;
    g.axis_x = Point::new(0., s);
    g.axis_y = Point::new(s, 0.);
    let corner = world(g, [54., 0.]);
    let replacement = Draft::new(&doc, tube, corner)
        .unwrap()
        .preview(68., 14.)
        .unwrap();
    replace(&mut doc, replacement);
    for (end, other) in [("a", "b"), ("b", "a")] {
        let moving = port(&doc, tube, end);
        let fixed = port(&doc, tube, other);
        let g = graphic(&doc, tube);
        let old = g
            .piece
            .as_ref()
            .unwrap()
            .tube_route
            .as_ref()
            .unwrap()
            .bends
            .clone();
        let before = parts_list::graphic_cut_length_mm(g).unwrap().unwrap();
        let handle = adjust::handles(g)
            .into_iter()
            .find(|h| h.tube_end.as_deref() == Some(end))
            .unwrap();
        let [dx, dy] = moving.dir;
        let drag = add(
            moving.point,
            mul([dx * 19. - dy * 6., dy * 19. + dx * 6.], moving.mm),
        );
        let value = adjust::dragged(g, &handle, drag).unwrap();
        assert!(
            adjust::set_end(&mut doc, tube, &handle.param, end, value)
                .unwrap()
                .is_empty()
        );
        let g = graphic(&doc, tube);
        near(len(sub(port(&doc, tube, other).point, fixed.point)), 0.);
        near(len(sub(port(&doc, tube, other).dir, fixed.dir)), 0.);
        near(
            parts_list::graphic_cut_length_mm(g).unwrap().unwrap(),
            before + 19.,
        );
        assert_eq!(
            g.piece.as_ref().unwrap().tube_route.as_ref().unwrap().bends,
            old
        );
    }
    let _cap = cap(&mut doc, tube, "b");
    let before = doc.clone();
    assert!(
        adjust::set_end(
            &mut doc,
            tube,
            adjust::ROUTE_END_LENGTH_PARAM,
            "b",
            Value::Number(60.)
        )
        .unwrap_err()
        .contains("Detach the fitting")
    );
    assert_eq!(doc, before);
}

#[test]
fn a_routed_end_needs_room_for_a_fittings_insertion_depth() {
    let mut doc = Document::default();
    let tube = add_piece(&mut doc, STRAIGHT);
    let g = graphic(&doc, tube);
    let close = Draft::new(&doc, tube, world(g, [8., 0.])).unwrap();
    replace(&mut doc, close.preview(90., 5.).unwrap());
    let fitting = add_piece(&mut doc, CAP);
    let socket = port(&doc, fitting, "tube");
    let short = port(&doc, tube, "a");
    assert!(!solve::mates(&short, &socket));
    assert!(solve::connect(&mut doc, &[fitting], &short.at, &socket.at).is_none());
    assert!(doc.seats.is_empty());
    let far = port(&doc, tube, "b");
    assert!(solve::connect(&mut doc, &[fitting], &far.at, &socket.at).is_some());
    assert_eq!(doc.seats.len(), 1);
}

#[test]
fn routed_terminal_drags_keep_their_native_edit_without_an_unusable_reducer_offer() {
    for (end, other) in [("a", "b"), ("b", "a")] {
        let mut doc = Document::default();
        let tube = add_piece(&mut doc, STRAIGHT);
        let g = graphic(&doc, tube);
        let bent = Draft::new(&doc, tube, world(g, [54., 0.]))
            .unwrap()
            .preview(68., 14.)
            .unwrap();
        replace(&mut doc, bent);
        cap(&mut doc, tube, other);
        let handle = adjust::handles(graphic(&doc, tube))
            .into_iter()
            .find(|h| h.tube_end.as_deref() == Some(end))
            .unwrap();
        let moving = port(&doc, tube, end);
        let value = adjust::dragged(
            graphic(&doc, tube),
            &handle,
            add(moving.point, mul(moving.dir, 12. * moving.mm)),
        )
        .unwrap();
        let mut desired = doc.clone();
        adjust::set_end(&mut desired, tube, &handle.param, end, value.clone()).unwrap();
        let moving = port(&desired, tube, end);
        let socket = add_piece(&mut doc, "reshiki/compression-cap-1-8");
        let old = port(&doc, socket, "tube");
        let turn = (-moving.dir[1]).atan2(-moving.dir[0]).to_degrees()
            - old.dir[1].atan2(old.dir[0]).to_degrees();
        let pivot = graphic(&doc, socket).origin;
        crate::editing::transform_about(&mut doc, &[socket], pivot, 1., turn as f32);
        let target = port(&doc, socket, "tube");
        doc.translate(
            &[socket],
            (moving.point[0] - target.point[0]) as f32,
            (moving.point[1] - target.point[1]) as f32,
        );
        assert!(solve::compression_reducer(&port(&doc, socket, "tube"), &moving).is_some());
        let mut expected = doc.clone();
        adjust::set_end(&mut expected, tube, &handle.param, end, value.clone()).unwrap();
        let (released, offer) =
            adjust::snap_end(&mut doc, tube, &handle.param, end, value, 14.).unwrap();
        assert!(released.is_empty());
        assert!(offer.is_none());
        assert_eq!(doc, expected);
        let seated = doc.clone();
        solve::check(&mut doc, None);
        assert_eq!(doc.seats, seated.seats);
    }
}

#[test]
fn separate_runs_of_a_custom_tube_cannot_cross_each_other() {
    let mut route = Route {
        start: [0., 0.],
        bends: vec![
            Bend {
                corner: [60., 0.],
                radius_mm: 5.,
            },
            Bend {
                corner: [60., 60.],
                radius_mm: 5.,
            },
            Bend {
                corner: [20., 60.],
                radius_mm: 5.,
            },
        ],
        end: [20., -20.],
    };
    assert!(route.centerline().is_ok());
    assert!(
        route
            .validate(6.35)
            .unwrap_err()
            .contains("overlaps another part")
    );
    // The centre lines now miss, but the cut rim still enters the tube wall.
    route.end[1] = 2.;
    assert!(
        route
            .validate(6.35)
            .unwrap_err()
            .contains("overlaps another part")
    );
    route.end[1] = 10.;
    assert!(route.validate(6.35).is_ok());
}
