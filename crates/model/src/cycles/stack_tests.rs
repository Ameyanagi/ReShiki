use super::{Cycle, Role, drawn};
use crate::{
    arrows::Head,
    document::{Arrow, Document, Point},
    graphics::Graphic,
};

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

fn cycle(doc: &Document, id: u64) -> &Cycle {
    graphic(doc, id).cycle.as_deref().unwrap()
}

fn center(doc: &Document, ids: &[u64]) -> Point {
    let (lo, hi) = crate::scene::selection_bounds(doc, ids).unwrap();
    lo.lerp(hi, 0.5)
}

fn side_parts(cycle: &Cycle) -> Vec<(usize, bool, &[u64])> {
    cycle
        .steps
        .iter()
        .enumerate()
        .flat_map(|(step, value)| {
            value
                .reagents
                .iter()
                .map(move |part| (step, true, part.as_slice()))
                .chain(
                    value
                        .products
                        .iter()
                        .map(move |part| (step, false, part.as_slice())),
                )
        })
        .collect()
}

fn touch(side: &Arrow, incoming: bool) -> Point {
    if incoming { side.end } else { side.start }
}

fn fraction(lane: &Arrow, point: Point) -> f32 {
    let (center, _, first) = lane.arc_circle().unwrap();
    let sweep = lane.arc.unwrap();
    let angle = (point.y - center.y).atan2(point.x - center.x).to_degrees();
    let delta = (angle - (first + sweep * 0.5) + 180.).rem_euclid(360.) - 180.;
    0.5 + delta / sweep
}

fn unit(point: Point) -> Point {
    let length = point.x.hypot(point.y);
    assert!(length > 0. && length.is_finite());
    Point::new(point.x / length, point.y / length)
}

// Match final public drawn() connectors back to their roles, then compare
// their contacts and direction with the actual painted reversible arrows.
fn check_reversible_routes(doc: &Document, id: u64, step: usize) -> usize {
    let drawing = drawn(doc, graphic(doc, id));
    let lanes: Vec<_> = drawing
        .arrows
        .iter()
        .zip(&drawing.arcs)
        .filter_map(|(a, s)| (*s == Some(step)).then_some(a))
        .collect();
    assert_eq!(lanes.len(), 2, "fixture step {step} must paint both lanes");
    let sides: Vec<_> = drawing
        .arrows
        .iter()
        .zip(&drawing.arcs)
        .filter_map(|(a, s)| s.is_none().then_some(a))
        .collect();
    let parts = side_parts(cycle(doc, id));
    assert_eq!(
        sides.len(),
        parts.len(),
        "every fixture side part must draw"
    );
    let mut contacts = vec![vec![], vec![]];
    for ((s, incoming, ids), connector) in parts.into_iter().zip(sides) {
        if s != step {
            continue;
        }
        let body = center(doc, ids);
        let (lane_index, lane) = lanes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let distance = |a: &Arrow| {
                    let (c, radius, _) = a.arc_circle().unwrap();
                    (body.distance(c) - radius).abs()
                };
                distance(a).total_cmp(&distance(b))
            })
            .unwrap();
        let point = touch(connector, incoming);
        let (arc_center, radius, _) = lane.arc_circle().unwrap();
        assert!(
            (point.distance(arc_center) - radius).abs() < 0.1,
            "step {step}, part {ids:?} misses its nearest painted lane"
        );
        let [a, b] = connector.cubic.unwrap();
        let derivative = unit(if incoming {
            Point::new(point.x - b.x, point.y - b.y)
        } else {
            Point::new(a.x - point.x, a.y - point.y)
        });
        let radial = unit(Point::new(point.x - arc_center.x, point.y - arc_center.y));
        let sign = lane.arc.unwrap().signum();
        let tangent = Point::new(-sign * radial.y, sign * radial.x);
        assert!(derivative.x * tangent.x + derivative.y * tangent.y > 0.9999);
        assert_eq!(
            connector.appearance().head,
            if incoming { Head::None } else { Head::Full }
        );
        contacts[lane_index].push((fraction(lane, body), fraction(lane, point)));
    }
    let bond = doc.drawing_style.bond_length_world;
    let mut pairs = 0;
    for (lane, positions) in lanes.iter().zip(&mut contacts) {
        positions.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (_, radius, _) = lane.arc_circle().unwrap();
        let length = radius * lane.arc.unwrap().to_radians().abs();
        for pair in positions.windows(2) {
            pairs += 1;
            assert!(
                pair[1].1 >= pair[0].1,
                "projected part order reversed: {pair:?}"
            );
            if length * 0.7 >= 0.25 * bond * (positions.len() - 1) as f32 {
                assert!(
                    (pair[1].1 - pair[0].1) * length >= 0.25 * bond - 0.05,
                    "step {step} crowded lane joins: {pair:?}"
                );
            }
        }
    }
    pairs
}

#[test]
fn actual_tidy_fixture_keeps_roles_is_idempotent_and_joins_painted_lanes() {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/cycles/tidy-before.rsk"
    ))
    .expect("captured native fixture must open without fallback");
    let original = cycle(&doc, 191).clone();
    assert_eq!(original.intermediates.len(), 3);
    assert_eq!(original.steps.len(), 3);
    assert!(original.steps[1].reversible);
    let before = doc.clone();
    assert!(
        super::tidy(&mut doc, 191),
        "actual walkthrough fixture must change"
    );
    crate::atom_labels::refresh::refresh_edited(&before, &mut doc);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(
        cycle(&doc, 191),
        &original,
        "all roles/settings must stay exact"
    );
    doc.validate().unwrap();
    assert!(
        check_reversible_routes(&doc, 191, 1) > 0,
        "exercise actual crowded pair"
    );
    assert!(
        !super::tidy(&mut doc, 191),
        "tidy after reconciliation must be idempotent"
    );
    if let Some(dir) = std::env::var_os("RESHIKI_CYCLE_STACK_QA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tidy.svg"), crate::scene::svg(&doc)).unwrap();
        std::fs::write(dir.join("tidy-applied.rsk"), doc.file_json().unwrap()).unwrap();
    }
}

#[test]
fn actual_bowed_drop_repairs_final_reversible_routes_without_foreign_arc_crossings() {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/cycles/bowed-insertion.rsk"
    ))
    .expect("captured bowed fixture must open without fallback");
    doc.graphics
        .iter_mut()
        .find(|g| g.id == 191)
        .unwrap()
        .cycle
        .as_deref_mut()
        .unwrap()
        .steps[0]
        .reversible = true;
    let dragged: Vec<_> = (40..=85).collect();
    doc.translate(&dragged, -234.75783, 388.1741);
    let target = super::drop_target(&doc, &dragged).expect("actual captured drag must hit an arc");
    assert_eq!((target.cycle, target.role), (191, Role::Intermediate(1)));
    let before = doc.clone();
    let old_cycle = cycle(&before, 191).clone();
    let inherited = old_cycle.steps[0].clone();
    super::assign(&mut doc, &target);
    let mut expected = old_cycle;
    expected.intermediates.insert(1, target.part.clone());
    expected.steps.insert(1, Default::default());
    assert_eq!(
        cycle(&doc, 191),
        &expected,
        "inherited roles/settings stay exact"
    );
    let assigned = doc.clone();
    crate::atom_labels::refresh::refresh_edited(&before, &mut doc);
    let _ = crate::transaction::reconcile(&mut doc, before.clone()).unwrap();
    assert_eq!(cycle(&doc, 191), &expected);
    doc.validate().unwrap();
    check_reversible_routes(&doc, 191, 0);

    let drawing = drawn(&doc, graphic(&doc, 191));
    let connectors: Vec<_> = drawing
        .arrows
        .iter()
        .zip(&drawing.arcs)
        .filter_map(|(a, step)| step.is_none().then_some(a))
        .collect();
    let inherited_parts: Vec<_> = inherited
        .reagents
        .iter()
        .chain(&inherited.products)
        .collect();
    let mut moved = 0;
    for (part, connector) in inherited_parts.into_iter().zip(connectors) {
        if center(&before, part).distance(center(&assigned, part)) <= 0.1 {
            continue;
        }
        moved += 1;
        for (foreign, step) in drawing
            .arrows
            .iter()
            .zip(&drawing.arcs)
            .filter(|(_, step)| step.is_some() && **step != Some(0))
        {
            for k in 0..=2048 {
                let p = connector.point(k as f32 / 2048.);
                assert!(
                    !foreign.hit(p, 1.),
                    "moved inherited connector crosses step {step:?} at {p:?}"
                );
            }
        }
    }
    assert!(
        moved > 0,
        "actual insertion must repair stranded inherited parts"
    );
}
