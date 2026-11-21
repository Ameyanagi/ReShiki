use super::*;
use crate::{
    cycles::{Cycle, Shape},
    graphics::{Graphic, GraphicKind},
};

fn rectangle() -> Outline {
    let mut g = Graphic::dragged(
        1,
        GraphicKind::Ellipse,
        Point::new(-300., -150.),
        Point::new(300., 150.),
        Default::default(),
        Default::default(),
        false,
    );
    g.cycle = Some(Box::new(Cycle {
        shape: Shape::Rectangle,
        ..Default::default()
    }));
    crate::cycles::outline(&g).unwrap()
}

fn near(a: V, b: V) {
    assert!(sub(a, b)[0].hypot(sub(a, b)[1]) < 1e-7, "{a:?} != {b:?}");
}

fn near_point(a: Point, b: V) {
    near([f64::from(a.x), f64::from(a.y)], b);
}

#[test]
fn bow_uses_original_arclength_midpoint_and_preserves_endpoint_control_pairs() {
    let outline = rectangle();
    // Top midpoint to right midpoint: 300 across and 150 down. The Bow
    // peak lies inside the top edge, before the retained sharp corner.
    let baseline = Path(outline.cubic_sections(300., 450.).unwrap());
    assert_eq!(baseline.0.len(), 3);
    assert_eq!(baseline.0[0].end_t, 0.5);
    assert!(!baseline.0[0].corner_after);
    assert!(baseline.0[1].corner_after);
    for bow in [40., -40.] {
        let bowed = baseline.bowed(bow, 1.).unwrap();
        near(bowed.0[0].points[0], [0., -150.]);
        near(bowed.0[2].points[3], [300., 0.]);
        near(bowed.0[0].points[3], [225., -150. - bow]);
        near(
            bowed.0[1].points[3],
            [300. + 0.75 * bow, -150. - 0.75 * bow],
        );
        for (source, target) in baseline.0.iter().zip(&bowed.0) {
            near(
                sub(target.points[0], source.points[0]),
                sub(target.points[1], source.points[1]),
            );
            near(
                sub(target.points[2], source.points[2]),
                sub(target.points[3], source.points[3]),
            );
            near(
                tangent(source.points, true).unwrap(),
                tangent(target.points, true).unwrap(),
            );
            near(
                tangent(source.points, false).unwrap(),
                tangent(target.points, false).unwrap(),
            );
        }
    }
}

#[test]
fn lane_miters_offset_both_supporting_lines_and_both_shafts_are_painted() {
    let outline = rectangle();
    let baseline = Path(outline.cubic_sections(300., 450.).unwrap());
    let bowed = baseline.bowed(40., 1.).unwrap();
    let outside = bowed.offset(5., 1.).unwrap();
    let inside = bowed.offset(-5., 1.).unwrap().reversed();
    near(outside.0[1].points[3], [335., -185.]);
    near(inside.0[0].points[0], [295., 0.]);
    near(inside.0[0].points[3], [325., -175.]);
    let style = ArrowStyle::preset(Preset::Forward);
    let arrows = arrows(1, &outline, 300., 450., 40., true, &style).unwrap();
    assert_eq!(arrows.len(), 2);
    for arrow in &arrows {
        assert_eq!(arrow.appearance().head, Head::Left);
        assert_eq!(arrow.appearance().tail, Head::None);
        assert!(arrow.arc.is_none());
        let corner = arrow
            .nodes
            .iter()
            .find(|n| n.kind == PathNodeType::Corner)
            .unwrap()
            .point;
        let paths = arrow.paths();
        let shaft = paths.iter().find(|p| !p.filled).unwrap();
        assert!(
            shaft
                .commands
                .iter()
                .flat_map(|c| c.points())
                .any(|p| p == corner)
        );
        assert!(
            !shaft
                .commands
                .iter()
                .any(|c| matches!(c, crate::graphics::PathCommand::Line(_))),
            "miter must not become a painted bevel"
        );
        assert!(paths.iter().any(|p| p.filled), "half head must be painted");
    }
    assert_ne!(arrows[0].paths()[0].commands, arrows[1].paths()[0].commands);
}

#[test]
fn reversing_the_same_span_preserves_shape_outward_bow_and_lane_geometry() {
    let outline = rectangle();
    let forward = Path(outline.cubic_sections(300., 450.).unwrap())
        .bowed(40., 1.)
        .unwrap();
    let reverse = Path(outline.cubic_sections(750., -450.).unwrap())
        .bowed(40., -1.)
        .unwrap()
        .reversed();
    assert_eq!(forward.0.len(), reverse.0.len());
    for (a, b) in forward.0.iter().zip(&reverse.0) {
        for (a, b) in a.points.iter().zip(b.points) {
            near(*a, b);
        }
        assert_eq!(a.corner_after, b.corner_after);
    }
    let style = ArrowStyle::preset(Preset::Forward);
    let forward = arrows(1, &outline, 300., 450., 40., true, &style).unwrap();
    let reverse = arrows(1, &outline, 750., -450., 40., true, &style).unwrap();
    assert!(reverse.iter().all(|a| a.appearance().head == Head::Right));
    near_point(
        forward[0].start,
        [f64::from(reverse[0].end.x), f64::from(reverse[0].end.y)],
    );
    near_point(
        forward[1].start,
        [f64::from(reverse[1].end.x), f64::from(reverse[1].end.y)],
    );
}

#[test]
fn zero_bow_preserves_raw_section_geometry_and_exact_corner_end_normals() {
    let outline = rectangle();
    let style = ArrowStyle::preset(Preset::Forward);
    assert_eq!(
        arrows(1, &outline, 300., 450., 0., false, &style).unwrap(),
        vec![super::super::path_arrow(1, &outline, 300., 450., &style).unwrap()]
    );
    let baseline = Path(outline.cubic_sections(600., 300.).unwrap());
    let zero = baseline.bowed(0., 1.).unwrap();
    for (a, b) in baseline.0.iter().zip(&zero.0) {
        assert_eq!(a.points, b.points);
    }
    // Start at the upper-right corner: only the outgoing down tangent is
    // used, so the lane starts right of the corner without an upward miter.
    let outside = baseline.offset(5., 1.).unwrap();
    near(outside.0[0].points[0], [305., -150.]);
    // End at the lower-right corner uses its incoming down tangent only.
    near(outside.0.last().unwrap().points[3], [305., 150.]);
}

#[test]
fn midpoint_rounding_at_the_perimeter_seam_does_not_drop_the_shaft() {
    let outline = rectangle();
    for (start, length) in [(1033.12, 1533.76), (766.88, -1533.76)] {
        let sections = outline.cubic_sections(start, length).unwrap();
        assert_eq!(sections.last().unwrap().end_t, 1.);
        assert!(sections.iter().any(|s| s.end_t == 0.5));
        let arrows = arrows(1, &outline, start, length, 40., true, &ArrowStyle::DEFAULT).unwrap();
        assert_eq!(arrows.len(), 2);
    }
}

#[test]
fn midpoints_adjacent_to_source_corners_keep_every_nonzero_piece_in_both_directions() {
    let outline = rectangle();
    for start in [0., 13.12, 300., 600., 766.88, 1033.12, 1799.9] {
        for corner in [600., 900., 1500., 1800., 2400., 2700.] {
            let length = 2. * (corner - start);
            if length <= 0. || length > outline.length() {
                continue;
            }
            for length in [length.next_down(), length, length.next_up()] {
                for (start, length) in [(start, length), (start + length, -length)] {
                    let sections = outline.cubic_sections(start, length).unwrap();
                    let path = Path(sections);
                    assert!(path.normals(length.signum()).is_some());
                    assert!(
                        path.0.iter().all(|c| tangent(c.points, true).is_some()
                            && tangent(c.points, false).is_some()),
                        "start={start}, length={length}, sections={:?}",
                        path.0
                    );
                    assert!(
                        arrows(1, &outline, start, length, 40., true, &ArrowStyle::DEFAULT)
                            .is_some()
                    );
                }
            }
        }
    }
}

#[test]
fn extreme_bow_and_numerically_folded_corners_have_a_finite_deterministic_fallback() {
    let outline = rectangle();
    for bow in [
        -DEFAULT.world(720.) as f64,
        DEFAULT.world(720.) as f64,
        f64::MAX,
    ] {
        for reversible in [false, true] {
            let arrows = arrows(
                1,
                &outline,
                300.,
                1400.,
                bow,
                reversible,
                &ArrowStyle::DEFAULT,
            )
            .unwrap();
            assert_eq!(arrows.len(), if reversible { 2 } else { 1 });
            for arrow in arrows {
                arrow.validate().unwrap();
                assert!(
                    arrow
                        .paths()
                        .iter()
                        .flat_map(|p| &p.commands)
                        .flat_map(|c| c.points())
                        .all(|p| p.is_finite())
                );
            }
        }
    }

    let mut g = Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(600., 10.),
        Default::default(),
        Default::default(),
        false,
    );
    g.axis_y = Point::new(600., 1e-8);
    g.cycle = Some(Box::new(Cycle {
        shape: Shape::Rectangle,
        ..Default::default()
    }));
    let outline = crate::cycles::outline(&g).unwrap();
    let path = Path(outline.cubic_sections(0., 1400.).unwrap());
    let normals = path.normals(1.).unwrap();
    assert!(normals.iter().all(|n| n[0].hypot(n[1]) <= 4.0000001));
    assert!(normals.iter().any(|n| n[0].hypot(n[1]) > 3.9));
    let first = arrows(1, &outline, 0., 1400., 40., true, &ArrowStyle::DEFAULT).unwrap();
    let second = arrows(1, &outline, 0., 1400., 40., true, &ArrowStyle::DEFAULT).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    for arrow in first {
        arrow.validate().unwrap();
        assert!(arrow.nodes.iter().any(|n| n.kind == PathNodeType::Corner));
        assert!(
            arrow
                .paths()
                .iter()
                .flat_map(|p| &p.commands)
                .flat_map(|c| c.points())
                .all(|p| p.is_finite())
        );
    }

    // Even when the bisector itself rounds to zero, choose the same
    // canonical one-sided normal when the very same path is reversed.
    let line = |a: V, b: V| {
        [
            a,
            add(a, mul(sub(b, a), 1. / 3.)),
            add(a, mul(sub(b, a), 2. / 3.)),
            b,
        ]
    };
    let folded = Path(vec![
        CubicSection {
            points: line([0., 0.], [1., 0.]),
            end_t: 0.5,
            corner_after: true,
        },
        CubicSection {
            points: line([1., 0.], [0., 0.]),
            end_t: 1.,
            corner_after: false,
        },
    ]);
    let normal = folded.normals(1.).unwrap();
    let mut reverse = folded.reversed().normals(-1.).unwrap();
    reverse.reverse();
    assert_eq!(normal, reverse);
}
