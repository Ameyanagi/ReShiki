use super::*;

fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds {
    Bounds {
        lo: World::new(x, y),
        hi: World::new(x + w, y + h),
    }
}

#[test]
fn edges_match_edges_and_centers_match_centers() {
    // A 40-wide object whose center is 3 px right of a 100-wide target's center.
    let target = b(0., 0., 100., 20.);
    let moving = b(0., 100., 40., 20.);
    let snapped = snap(moving, World::new(33., 0.), &[target], &[Axis::X], 1.);
    assert_eq!(snapped, World::new(30., 0.), "center to center");
    // Its left edge 4 px right of the target's right edge snaps to it.
    let snapped = snap(moving, World::new(104., 0.), &[target], &[Axis::X], 1.);
    assert_eq!(snapped, World::new(100., 0.), "left edge to right edge");
    // An edge never pairs with a center: left edge 2 px from the target's center.
    let snapped = snap(moving, World::new(52., 0.), &[target], &[Axis::X], 1.);
    assert_eq!(snapped, World::new(52., 0.));
    // The nearest candidate wins on each axis, independently.
    let near = b(200., 0., 40., 20.);
    let snapped = snap(
        moving,
        World::new(203., -101.),
        &[target, near],
        &Axis::BOTH,
        1.,
    );
    assert_eq!(snapped, World::new(200., -100.));
    // A locked axis only snaps along itself.
    let snapped = snap(moving, World::new(203., -101.), &[near], &[Axis::X], 1.);
    assert_eq!(snapped, World::new(200., -101.));
}

#[test]
fn reach_is_six_screen_pixels_at_any_zoom() {
    let target = b(0., 0., 400., 20.);
    let moving = b(0., 100., 40., 20.);
    for zoom in [0.25_f32, 1., 4.] {
        let pixel = 1. / zoom;
        let near = snap(
            moving,
            World::new(5.5 * pixel, 0.),
            &[target],
            &[Axis::X],
            pixel,
        );
        assert!(near.x.abs() < 1e-4, "{zoom}: {near:?}");
        let far = World::new(6.5 * pixel, 0.);
        assert_eq!(
            snap(moving, far, &[target], &[Axis::X], pixel),
            far,
            "{zoom}"
        );
    }
}

#[test]
fn equal_spacing_snaps_to_the_midpoint_or_a_neighbouring_gap() {
    let (left, right) = (b(0., 0., 40., 40.), b(200., 0., 40., 40.));
    let moving = b(0., 200., 60., 40.);
    // Midpoint between two neighbours in a row: gaps of 50 on both sides.
    let snapped = snap(
        moving,
        World::new(92., -200.),
        &[left, right],
        &[Axis::X],
        1.,
    );
    assert_eq!(snapped.x, 90.);
    let guides = guides(
        moving.translated(World::new(90., -200.)),
        &[left, right],
        1.,
    );
    let gaps: Vec<_> = guides
        .iter()
        .filter_map(|g| match *g {
            Guide::Gap { from, to, .. } => Some((from, to)),
            _ => None,
        })
        .collect();
    assert_eq!(gaps, [(40., 90.), (150., 200.)]);
    // A gap equal to the neighbouring gap: 0..40, 70..110, then 140..200.
    let (a, c) = (b(0., 0., 40., 40.), b(70., 0., 40., 40.));
    let snapped = snap(moving, World::new(137., -200.), &[a, c], &[Axis::X], 1.);
    assert_eq!(snapped.x, 140.);
    // And on the other side: 300..340, 370..410; the object ends at 270.
    let (d, e) = (b(300., 0., 40., 40.), b(370., 0., 40., 40.));
    let snapped = snap(moving, World::new(212., -200.), &[d, e], &[Axis::X], 1.);
    assert_eq!(snapped.x, 210.);
    // Objects outside the row (no vertical overlap) are not neighbours.
    let (a, c) = (b(0., 300., 40., 40.), b(70., 300., 40., 40.));
    let raw = World::new(137., -200.);
    assert_eq!(snap(moving, raw, &[a, c], &[Axis::X], 1.), raw);
}

#[test]
fn a_same_size_neighbour_shows_only_its_center_guide() {
    let twin = b(0., 0., 40., 20.);
    let wider = b(-20., 100., 80., 20.);
    let moved = b(0., 50., 40., 20.);
    let aligned: Vec<_> = guides(moved, &[twin, wider], 1.)
        .into_iter()
        .filter_map(|g| match g {
            Guide::Align {
                axis: Axis::X,
                at,
                from,
                to,
            } => Some((at, from, to)),
            _ => None,
        })
        .collect();
    // One center guide spans all three; the edges would only repeat it for the twin.
    assert_eq!(aligned, [(20., 0., 120.)]);
    // Edge guides still show for a different-size neighbour.
    let left = b(0., 100., 80., 20.);
    let aligned = guides(moved, &[left], 1.);
    assert!(aligned.contains(&Guide::Align {
        axis: Axis::X,
        at: 0.,
        from: 50.,
        to: 120.
    }));
}

#[test]
fn arrow_ends_snap_to_edges_and_centers_along_their_direction() {
    let molecule = b(100., -30., 60., 60.);
    // Free angles: both axes snap independently.
    assert_eq!(
        snap_point(World::new(97., 2.), &[molecule], None, 1.),
        World::new(100., 0.)
    );
    // Fixed angles: a horizontal arrow only slides along its line.
    let end = snap_point(
        World::new(96., 3.),
        &[molecule],
        Some(World::new(0., 3.)),
        1.,
    );
    assert_eq!(end, World::new(100., 3.));
    assert_eq!(
        point_guides(end, &[molecule], 1.),
        [Guide::Align {
            axis: Axis::X,
            at: 100.,
            from: -30.,
            to: 30.
        }]
    );
}

#[test]
fn attached_gap_dimensions_keep_the_actual_distance_at_every_angle_and_zoom() {
    use reshiki::arrow_labels::{GapGeometry, Side};
    let bounds = Rectangle::with_size(Size::new(800., 600.));
    for (tangent, above) in [
        (World::new(1., 0.), World::new(0., -1.)),
        (World::new(-1., 0.), World::new(0., -1.)),
        (World::new(0., 1.), World::new(-1., 0.)),
        (World::new(0.6, 0.8), World::new(0.8, -0.6)),
    ] {
        for side in [Side::Above, Side::Below] {
            let normal = if side == Side::Above {
                above
            } else {
                World::new(-above.x, -above.y)
            };
            for gap in [0., 18.] {
                for zoom in [0.25, 1., 4.] {
                    let from = World::new(27., -14.);
                    let geometry = GapGeometry {
                        from,
                        to: from.offset(normal.x * gap, normal.y * gap),
                        tangent,
                        half_span: 35.,
                    };
                    let camera = Camera {
                        center: World::new(-10., 50.),
                        zoom,
                    };
                    let dimension = LabelGapDimension::new(geometry, side, camera, bounds);
                    assert!((dimension.world - gap).abs() < 1e-4);
                    assert!((dimension.from.distance(dimension.to) - gap * zoom).abs() < 1e-3);
                    assert!((dimension.normal.x - normal.x).abs() < 1e-4);
                    assert!((dimension.normal.y - normal.y).abs() < 1e-4);
                    // The measure clears the projected label by 12 screen pixels,
                    // while its numerical value stays in world units.
                    let offset = dimension.from - camera.screen(from, bounds);
                    let along = offset.x * dimension.tangent.x + offset.y * dimension.tangent.y;
                    assert!((along - (35. * zoom + 12.)).abs() < 1e-3);
                    let size = Size::new(62., 16.);
                    let center = dimension.badge_corner(size) + Vector::new(31., 8.);
                    let middle = Point::new(
                        (dimension.from.x + dimension.to.x) / 2.,
                        (dimension.from.y + dimension.to.y) / 2.,
                    );
                    let away = center - middle;
                    let normal_extent =
                        (normal.x.abs() * size.width + normal.y.abs() * size.height) / 2.;
                    assert!(
                        away.x * normal.x + away.y * normal.y >= normal_extent + 3.99,
                        "even zero-gap opposite-side badges stay outside their support planes"
                    );
                }
            }
        }
    }
}

fn attached_gap_document() -> Document {
    use reshiki::{
        arrow_labels::{ArrowLabel, Side},
        arrows::{ArrowStyle, Preset},
        document::{Annotation, Arrow},
    };
    let mut doc = Document::default();
    doc.arrows.push(Arrow::new(
        1,
        World::new(0., 0.),
        World::new(120., 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    doc.annotations.push(Annotation {
        id: 2,
        position: World::new(40., -20.),
        text: "heat, 2 h".into(),
        format: Default::default(),
    });
    doc.arrow_labels.push(ArrowLabel {
        arrow: 1,
        annotation: 2,
        side: Side::Above,
        gap: None,
    });
    reshiki::arrow_labels::set_gap(&mut doc, &[2], Some(16.)).unwrap();
    let before = doc.clone();
    reshiki::arrow_labels::reconcile(&mut doc, &before);
    doc
}

#[test]
fn only_selected_visible_attached_text_gets_an_actual_geometry_measure() {
    let mut doc = attached_gap_document();
    let camera = Camera::default();
    let bounds = Rectangle::with_size(Size::new(800., 600.));
    assert_eq!(label_gap_dimensions(&doc, &[], camera, bounds).count(), 0);
    assert_eq!(label_gap_dimensions(&doc, &[1], camera, bounds).count(), 0);
    assert!(
        (label_gap_dimensions(&doc, &[2], camera, bounds)
            .next()
            .unwrap()
            .world
            - 16.)
            .abs()
            < 1e-3
    );
    // Measuring the supplied preview must not just repeat the stored setting.
    doc.annotations
        .iter_mut()
        .find(|a| a.id == 2)
        .unwrap()
        .position
        .y -= 9.;
    assert_eq!(reshiki::arrow_labels::gap(&doc, 2), Some(16.));
    let before = doc.clone();
    assert!(
        (label_gap_dimensions(&doc, &[2], camera, bounds)
            .next()
            .unwrap()
            .world
            - 25.)
            .abs()
            < 1e-3
    );
    assert_eq!(doc, before, "the dimension adds no document/export ink");
    // Inline text editing removes its annotation from the paper preview.
    doc.annotations.clear();
    assert_eq!(label_gap_dimensions(&doc, &[2], camera, bounds).count(), 0);
    doc = before;
    doc.arrow_labels.clear();
    assert_eq!(label_gap_dimensions(&doc, &[2], camera, bounds).count(), 0);
}

#[test]
fn attached_gap_measure_follows_the_reconciled_arrow_preview_not_the_raw_cursor() {
    use crate::canvas::{Gesture, MoleculeCanvas, State, Tool, tests::chain_canvas};
    let doc = attached_gap_document();
    let bounds = Rectangle::new(Point::new(20., 30.), Size::new(800., 600.));
    let canvas = MoleculeCanvas {
        tool: Tool::Select,
        selected: &[2],
        ..chain_canvas(&doc, reshiki::chains::ChainMode::Straight)
    };
    let requested = World::new(8., 8.);
    let state = State {
        gesture: Some(Gesture::ArrowHandle {
            id: 1,
            index: reshiki::arrows::ArrowHandle::End.index(),
        }),
        cursor: Some(canvas.camera.screen(requested, bounds) + Vector::new(bounds.x, bounds.y)),
        ..Default::default()
    };
    let preview = canvas.pointer_preview_document(&state, bounds);
    let end = preview.arrows.iter().find(|a| a.id == 1).unwrap().end;
    assert!(
        end.distance(requested) > 1.,
        "attached text clamps the short raw pointer target"
    );
    let dimension = label_gap_dimensions(&preview, &[2], canvas.camera, bounds)
        .next()
        .unwrap();
    let base = label_gap_dimensions(&doc, &[2], canvas.camera, bounds)
        .next()
        .unwrap();
    assert!((dimension.world - 16.).abs() < 1e-3);
    assert!(
        dimension.from.distance(base.from) > 5.,
        "the guide moves with the reconciled preview"
    );
}
