use super::{Graphic, GraphicKind, PathCommand, PathNodeType};
use crate::document::{Document, Point};
use PathNodeType::{Corner, Smooth, Symmetric};

fn curved(incoming: Point, outgoing: Point) -> Graphic {
    let mut graphic = Graphic::pen_curve(
        1,
        Point::new(-100., 0.),
        Point::new(100., 0.),
        Default::default(),
    );
    graphic.path = vec![
        PathCommand::Move(Point::new(-100., 0.)),
        PathCommand::Cubic(Point::new(-80., 0.), incoming, Point::default()),
        PathCommand::Cubic(outgoing, Point::new(80., 0.), Point::new(100., 0.)),
    ];
    graphic.node_types.clear();
    graphic
}

fn closed() -> Graphic {
    let mut graphic = curved(Point::new(-10., 0.), Point::new(20., 0.));
    graphic.path = vec![
        PathCommand::Move(Point::default()),
        PathCommand::Cubic(
            Point::new(20., 0.),
            Point::new(100., -20.),
            Point::new(100., 0.),
        ),
        PathCommand::Cubic(
            Point::new(100., 20.),
            Point::new(-20., 100.),
            Point::new(0., 100.),
        ),
        PathCommand::Cubic(
            Point::new(20., 100.),
            Point::new(-20., 0.),
            Point::default(),
        ),
        PathCommand::Close,
    ];
    graphic
}

fn near(actual: Point, expected: Point) {
    assert!(
        actual.distance(expected) < 0.001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn legacy_node_detection_covers_tolerances_endpoints_and_line_neighbors() {
    for (angle, incoming_length, expected) in [
        (0.5_f32, 10.05, Symmetric),
        (0.5, 10.2, Smooth),
        (1.5, 10., Corner),
    ] {
        let angle = angle.to_radians();
        let graphic = curved(
            Point::new(-incoming_length, 0.),
            Point::new(10. * angle.cos(), 10. * angle.sin()),
        );
        assert_eq!(
            graphic.path_node_types(),
            Some(vec![Corner, expected, Corner])
        );
        assert!(
            graphic.node_types.is_empty(),
            "detection does not rewrite old files"
        );
    }
    let mut graphic = curved(Point::default(), Point::new(10., 0.));
    assert_eq!(graphic.path_node_types(), Some(vec![Corner; 3]));
    graphic.path[1] = PathCommand::Line(Point::default());
    assert_eq!(graphic.path_node_types(), Some(vec![Corner; 3]));
}

#[test]
fn node_types_serialize_in_snake_case_and_old_json_still_detects_geometry() {
    let mut graphic = curved(Point::new(-10., 0.), Point::new(10., 0.));
    let old_json = serde_json::to_value(&graphic).unwrap();
    assert!(old_json.get("node_types").is_none());
    let old: Graphic = serde_json::from_value(old_json).unwrap();
    assert!(old.node_types.is_empty());
    assert_eq!(old.path_node_types(), Some(vec![Corner, Symmetric, Corner]));

    let geometry = graphic.commands();
    graphic.set_path_node_type(3, Corner).unwrap();
    assert_eq!(graphic.commands(), geometry);
    assert_eq!(graphic.node_types, vec![Corner; 3]);
    assert_eq!(graphic.path_node_types(), Some(vec![Corner; 3]));
    let value = serde_json::to_value(&graphic).unwrap();
    assert_eq!(
        value["node_types"],
        serde_json::json!(["corner", "corner", "corner"])
    );
    assert_eq!(serde_json::from_value::<Graphic>(value).unwrap(), graphic);
    assert_eq!(
        serde_json::to_value([Corner, Smooth, Symmetric]).unwrap(),
        serde_json::json!(["corner", "smooth", "symmetric"])
    );

    let doc = Document {
        graphics: vec![graphic],
        ..Document::default()
    };
    assert_eq!(
        Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );
}

#[test]
fn smooth_and_symmetric_conversion_average_tangent_directions() {
    for kind in [Smooth, Symmetric] {
        let mut graphic = curved(Point::new(-10., 0.), Point::new(0., 20.));
        // A control selects its owning node as well as a round node does.
        graphic.set_path_node_type(4, kind).unwrap();
        let points = graphic.edit_points();
        let direction = std::f32::consts::FRAC_1_SQRT_2;
        let (incoming_length, outgoing_length) = if kind == Smooth {
            (10., 20.)
        } else {
            (15., 15.)
        };
        near(
            points[2],
            Point::new(-direction * incoming_length, -direction * incoming_length),
        );
        near(
            points[4],
            Point::new(direction * outgoing_length, direction * outgoing_length),
        );
        assert_eq!(graphic.node_types, vec![Corner, kind, Corner]);
        for index in [0, 1, 3, 5, 6] {
            assert_eq!(
                points[index],
                curved(Point::new(-10., 0.), Point::new(0., 20.)).edit_points()[index]
            );
        }
    }
}

#[test]
fn smooth_drag_preserves_opposite_length_and_symmetric_drag_reflects_both_directions() {
    for kind in [Smooth, Symmetric] {
        for selected in [2, 4] {
            let mut graphic = curved(Point::new(-10., 0.), Point::new(20., 0.));
            graphic.set_path_node_type(3, kind).unwrap();
            let before = graphic.edit_points();
            let opposite = if selected == 2 { 4 } else { 2 };
            graphic
                .move_path_point(selected, Point::new(0., 30.))
                .unwrap();
            let points = graphic.edit_points();
            assert_eq!(points[selected], Point::new(0., 30.));
            let length = if kind == Smooth {
                before[opposite].distance(Point::default())
            } else {
                30.
            };
            near(points[opposite], Point::new(0., -length));
            for index in [0, 1, 3, 5, 6] {
                assert_eq!(points[index], before[index]);
            }
            assert_eq!(graphic.node_types[1], kind);
        }
    }
}

#[test]
fn corner_and_free_drags_move_one_control_and_remain_corner_when_aligned() {
    for free in [false, true] {
        let mut graphic = curved(Point::new(-10., 0.), Point::new(10., 0.));
        graphic
            .set_path_node_type(3, if free { Symmetric } else { Corner })
            .unwrap();
        let before = graphic.edit_points();
        if free {
            graphic
                .move_path_point_free(4, Point::new(25., 0.))
                .unwrap();
        } else {
            graphic.move_path_point(4, Point::new(25., 0.)).unwrap();
        }
        let points = graphic.edit_points();
        for index in [0, 1, 2, 3, 5, 6] {
            assert_eq!(points[index], before[index]);
        }
        assert_eq!(points[4], Point::new(25., 0.));
        assert_eq!(graphic.path_node_types(), Some(vec![Corner; 3]));
    }
}

#[test]
fn node_movement_transports_both_handles_and_keeps_stored_behavior() {
    let mut graphic = curved(Point::new(-10., 0.), Point::new(20., 0.));
    graphic.set_path_node_type(3, Smooth).unwrap();
    let before = graphic.edit_points();
    graphic.move_path_point(3, Point::new(12., -7.)).unwrap();
    let after = graphic.edit_points();
    for index in [2, 3, 4] {
        near(after[index], before[index].offset(12., -7.));
    }
    for index in [0, 1, 5, 6] {
        assert_eq!(after[index], before[index]);
    }
    assert_eq!(graphic.node_types, vec![Corner, Smooth, Corner]);
}

#[test]
fn endpoint_and_line_adjacent_constraints_keep_the_existing_segments() {
    let mut endpoint = curved(Point::new(-10., 0.), Point::new(20., 0.));
    endpoint.set_path_node_type(0, Symmetric).unwrap();
    let before = endpoint.edit_points();
    endpoint.move_path_point(1, Point::new(-90., 15.)).unwrap();
    let after = endpoint.edit_points();
    for index in [0, 2, 3, 4, 5, 6] {
        assert_eq!(after[index], before[index]);
    }

    let mut line_adjacent = curved(Point::new(-10., 0.), Point::new(20., 0.));
    line_adjacent.path[1] = PathCommand::Line(Point::default());
    line_adjacent.set_path_node_type(1, Symmetric).unwrap();
    let before = line_adjacent.edit_points();
    line_adjacent
        .move_path_point(2, Point::new(0., 30.))
        .unwrap();
    let after = line_adjacent.edit_points();
    for index in [0, 1, 3, 4] {
        assert_eq!(after[index], before[index]);
    }
    assert_eq!(after[2], Point::new(0., 30.));
    assert!(matches!(line_adjacent.path[1], PathCommand::Line(_)));
    assert_eq!(line_adjacent.node_types, vec![Corner, Symmetric, Corner]);
}

#[test]
fn seam_controls_resolve_to_one_node_and_obey_its_constraint() {
    let mut graphic = closed();
    for index in [0, 1, 8, 9] {
        assert_eq!(graphic.path_node_index(index), Some(0));
    }
    for index in [2, 3, 4] {
        assert_eq!(graphic.path_node_index(index), Some(3));
    }
    assert_eq!(graphic.path_node_index(usize::MAX), None);
    assert_eq!(graphic.path_node_types().unwrap().len(), 3);
    graphic.set_path_node_type(8, Symmetric).unwrap();
    graphic.move_path_point(1, Point::new(30., 12.)).unwrap();
    let points = graphic.edit_points();
    near(points[8], Point::new(-30., -12.));
    assert_eq!(points[9], points[0]);
    graphic.move_path_point(9, Point::new(5., 7.)).unwrap();
    let moved = graphic.edit_points();
    for index in [0, 1, 8, 9] {
        near(moved[index], points[index].offset(5., 7.));
    }
    assert_eq!(graphic.node_types[0], Symmetric);
    graphic.validate().unwrap();
}

#[test]
fn collapsed_and_opposed_tangents_have_deterministic_finite_results() {
    for (incoming, outgoing) in [
        (Point::default(), Point::default()),
        (Point::default(), Point::new(10., 0.)),
        (Point::new(10., 0.), Point::new(20., 0.)),
    ] {
        for kind in [Smooth, Symmetric] {
            let mut first = curved(incoming, outgoing);
            let mut second = first.clone();
            first.set_path_node_type(3, kind).unwrap();
            second.set_path_node_type(3, kind).unwrap();
            assert_eq!(first, second);
            assert!(first.edit_points().into_iter().all(Point::is_finite));
            let before_zero = first.edit_points();
            first.move_path_point(4, Point::default()).unwrap();
            assert!(first.edit_points().into_iter().all(Point::is_finite));
            assert_eq!(
                first.edit_points()[2],
                if kind == Symmetric {
                    Point::default()
                } else {
                    before_zero[2]
                }
            );
        }
    }
}

#[test]
fn explicit_types_follow_insertion_deletion_and_open_close_changes() {
    let mut graphic = curved(Point::new(-10., 0.), Point::new(20., 0.));
    graphic.set_path_node_type(3, Smooth).unwrap();
    let inserted = graphic.insert_path_node(0).unwrap();
    assert_eq!(graphic.node_types, vec![Corner, Smooth, Smooth, Corner]);
    graphic.delete_path_node(inserted).unwrap();
    assert_eq!(graphic.node_types, vec![Corner, Smooth, Corner]);
    graphic.set_path_closed(true).unwrap();
    assert_eq!(graphic.node_types, vec![Corner, Smooth, Corner]);
    graphic.set_path_closed(false).unwrap();
    assert_eq!(graphic.node_types, vec![Corner, Smooth, Corner]);
    graphic.set_path_segment_curved(0, false).unwrap();
    assert_eq!(graphic.node_types, vec![Corner, Smooth, Corner]);
    let inserted = graphic.insert_path_node(0).unwrap();
    assert_eq!(graphic.node_types, vec![Corner, Corner, Smooth, Corner]);
    graphic.delete_path_node(inserted).unwrap();
    graphic.validate().unwrap();

    let mut duplicate = curved(Point::new(-10., 0.), Point::new(20., 0.));
    duplicate
        .append_pen_node(Point::new(-100., 0.), None)
        .unwrap();
    assert_eq!(duplicate.node_types.len(), 4);
    duplicate.set_path_closed(true).unwrap();
    assert_eq!(duplicate.node_types.len(), 3);
    duplicate.validate().unwrap();
}

#[test]
fn insertion_materializes_legacy_types_and_keeps_a_split_cubic_smooth() {
    for cubic in [false, true] {
        let mut graphic = curved(Point::new(-10., 0.), Point::new(20., 0.));
        if !cubic {
            graphic.path[1] = PathCommand::Line(Point::default());
        }
        assert!(graphic.node_types.is_empty());
        let inserted = graphic.insert_path_node(0).unwrap();
        assert_eq!(graphic.node_types.len(), 4);
        let expected = if cubic { Smooth } else { Corner };
        assert_eq!(graphic.node_types[1], expected);
        assert_eq!(graphic.path_node_types().unwrap()[1], expected);
        let roundtrip: Graphic =
            serde_json::from_value(serde_json::to_value(&graphic).unwrap()).unwrap();
        assert_eq!(roundtrip.path_node_types().unwrap()[1], expected);
        graphic.delete_path_node(inserted).unwrap();
        assert_eq!(graphic.node_types.len(), 3);
        graphic.validate().unwrap();
    }
}

#[test]
fn pen_drawing_persists_clicked_and_dragged_node_types_and_populates_old_paths() {
    let mut graphic = Graphic::pen_curve(
        1,
        Point::default(),
        Point::new(100., 0.),
        Default::default(),
    );
    assert_eq!(graphic.node_types, vec![Corner, Symmetric]);
    graphic.append_pen_node(Point::new(200., 0.), None).unwrap();
    graphic
        .append_pen_node(Point::new(300., 0.), Some(Point::new(300., 20.)))
        .unwrap();
    assert_eq!(
        graphic.node_types,
        vec![Corner, Symmetric, Corner, Symmetric]
    );
    graphic.validate().unwrap();

    let mut old = curved(Point::new(-10., 0.), Point::new(20., 0.));
    old.append_pen_node(Point::new(200., 0.), None).unwrap();
    assert_eq!(old.node_types, vec![Corner, Smooth, Corner, Corner]);
}

#[test]
fn document_validation_rejects_invalid_external_node_type_vectors() {
    let graphic = curved(Point::new(-10., 0.), Point::new(10., 0.));
    let doc = Document {
        graphics: vec![graphic],
        ..Document::default()
    };
    let valid: serde_json::Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    for (kind, types) in [
        ("path", serde_json::json!(["corner"])),
        (
            "rectangle",
            serde_json::json!(["corner", "smooth", "corner"]),
        ),
    ] {
        let mut value = valid.clone();
        value["graphics"][0]["kind"] = serde_json::json!(kind);
        value["graphics"][0]["node_types"] = types;
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(
            Document::from_native_file(&bytes)
                .unwrap_err()
                .contains("node types")
        );
    }
    // Arc fallback restoration must leave invalid metadata visible to validation.
    let mut value = valid;
    value["graphics"][0]["arc"] = serde_json::json!({"start_degrees": 0., "sweep_degrees": 90.});
    value["graphics"][0]["node_types"] = serde_json::json!(["corner", "smooth", "corner"]);
    assert!(Document::from_native_file(&serde_json::to_vec(&value).unwrap()).is_err());

    let mut wrong_length = curved(Point::new(-10., 0.), Point::new(10., 0.));
    wrong_length.node_types = vec![Corner];
    assert_eq!(
        wrong_length.path_node_types(),
        Some(vec![Corner, Symmetric, Corner])
    );
}

#[test]
fn failed_node_type_edits_are_atomic_and_non_path_graphics_have_no_node_owner() {
    let mut graphic = curved(Point::new(-10., 0.), Point::new(0., 20.));
    graphic.axis_y = Point::default();
    graphic.depth = [0., 0., 1.];
    let before = graphic.clone();
    assert!(graphic.set_path_node_type(3, Symmetric).is_err());
    assert_eq!(graphic, before);
    assert!(
        graphic
            .move_path_point_free(4, Point::new(30., 20.))
            .is_err()
    );
    assert_eq!(graphic, before);

    graphic.kind = GraphicKind::Rectangle;
    assert_eq!(graphic.path_node_index(0), None);
    assert_eq!(graphic.path_node_types(), None);
    assert!(graphic.set_path_node_type(0, Corner).is_err());
}
