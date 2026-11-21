use super::*;

fn frame() -> Frame {
    Frame {
        origin: Point::default(),
        axis_x: Point::new(100., 0.),
        axis_y: Point::new(0., 100.),
    }
}

#[test]
fn pick_radius_cannot_reach_fully_clipped_ink_one_unit_beyond_the_edge() {
    let f = frame();
    let p = Point::new(99., 50.);
    assert!(!box_hit(
        f,
        Point::new(101., 40.),
        Point::new(110., 60.),
        p,
        p,
        5.
    ));
    assert!(!line_hit(
        f,
        Point::new(101., 40.),
        Point::new(101., 60.),
        0.1,
        p,
        p,
        5.
    ));
    assert!(!line_hit(
        f,
        Point::new(101., 50.),
        Point::new(101., 50.),
        0.1,
        p,
        p,
        5.
    ));
    assert!(
        line_hit(
            f,
            Point::new(100.05, 40.),
            Point::new(100.05, 60.),
            0.1,
            p,
            p,
            5.
        ),
        "Stroke width enters the cell before the pointer tolerance is applied"
    );
}

#[test]
fn hidden_nearby_path_sections_do_not_borrow_distant_visible_ink() {
    let f = frame();
    let p = Point::new(99., 50.);
    let path = [
        PathCommand::Move(Point::new(101., 40.)),
        PathCommand::Line(Point::new(101., 60.)),
        PathCommand::Line(Point::new(90., 100.)),
    ];
    assert!(!path_hit(f, &path, 0.1, false, true, p, p, 5.));
    assert!(path_hit(
        f,
        &path,
        0.1,
        false,
        true,
        Point::new(95., 80.),
        Point::new(95., 80.),
        5.
    ));
}

#[test]
fn reflected_affine_polygons_use_cell_coordinates_for_the_clip() {
    let f = Frame {
        origin: Point::new(200., 30.),
        axis_x: Point::new(-100., -20.),
        axis_y: Point::new(40., 120.),
    };
    let points = [
        Point::new(1.01, 0.4),
        Point::new(1.2, 0.4),
        Point::new(1.2, 0.6),
        Point::new(1.01, 0.6),
    ]
    .map(|p| f.point(p));
    let p = f.point(Point::new(0.99, 0.5));
    assert!(!polygon_hit(f, &points, p, p, 5.));
    assert!(polygon_hit(f, &f.corners(), p, p, 0.));
}

fn compound_path(opposite_hole: bool) -> Vec<PathCommand> {
    let outer = [
        Point::new(0., 0.),
        Point::new(100., 0.),
        Point::new(100., 100.),
        Point::new(0., 100.),
    ];
    let mut inner = [
        Point::new(30., 30.),
        Point::new(30., 70.),
        Point::new(70., 70.),
        Point::new(70., 30.),
    ];
    if !opposite_hole {
        inner.reverse();
    }
    [outer, inner]
        .into_iter()
        .flat_map(|contour| {
            std::iter::once(PathCommand::Move(contour[0]))
                .chain(contour[1..].iter().copied().map(PathCommand::Line))
                .chain(std::iter::once(PathCommand::Close))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn compound_nonzero_fill_preserves_opposite_winding_holes_for_points_and_sweeps() {
    let path = compound_path(true);
    let centre = Point::new(50., 50.);
    assert!(!path_hit(
        frame(),
        &path,
        0.,
        true,
        true,
        centre,
        centre,
        0.
    ));
    assert!(!path_hit(
        frame(),
        &path,
        0.,
        true,
        true,
        Point::new(40., 50.),
        Point::new(60., 50.),
        5.,
    ));
    let ink = Point::new(20., 50.);
    assert!(path_hit(frame(), &path, 0., true, true, ink, ink, 0.));
    assert!(
        path_hit(
            frame(),
            &compound_path(false),
            0.,
            true,
            true,
            centre,
            centre,
            0.
        ),
        "Same-winding nested contours remain filled under the nonzero rule"
    );
    let near_hole_edge = Point::new(31., 50.);
    assert!(
        path_hit(
            frame(),
            &path,
            0.,
            true,
            true,
            near_hole_edge,
            near_hole_edge,
            2.
        ),
        "Pick radius still reaches actual ink at the hole boundary"
    );
}

#[test]
fn a_cell_wholly_inside_a_compound_hole_has_no_ghost_clipped_fill_edges() {
    let path = compound_path(true);
    let centre = Point::new(50., 50.);
    for (origin, axis_x) in [
        (Point::new(40., 40.), Point::new(20., 0.)),
        (Point::new(60., 40.), Point::new(-20., 0.)),
    ] {
        let cell = Frame {
            origin,
            axis_x,
            axis_y: Point::new(0., 20.),
        };
        assert!(
            !path_hit(cell, &path, 0.1, true, true, centre, centre, 100.),
            "Neither fill nor stroked contours paint inside the cell; radius cannot recover offcell ink"
        );
    }
}

#[test]
fn nonzero_hole_fix_keeps_the_inner_contour_stroke_hittable() {
    let path = compound_path(true);
    let point = Point::new(30.5, 50.);
    assert!(path_hit(frame(), &path, 1., false, true, point, point, 0.));
    assert!(path_hit(frame(), &path, 1., true, true, point, point, 0.));
}
