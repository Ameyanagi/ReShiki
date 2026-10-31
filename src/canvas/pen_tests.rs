use super::*;
use iced::widget::canvas::Program;

pub(crate) struct PointGesture {
    pub before_release: Vec<Edit>,
    pub pressed_preview: Document,
    pub preview: Document,
    pub release: Edit,
}

/// Use real press/motion/release events, including inset bounds and a grab offset.
pub(crate) fn point_gesture(
    doc: &Document,
    id: u64,
    camera: Camera,
    index: usize,
    grab_offset: Vector,
    motion: Vector,
) -> PointGesture {
    point_gesture_with_modifiers(
        doc,
        id,
        camera,
        index,
        grab_offset,
        motion,
        iced::keyboard::Modifiers::empty(),
    )
}

pub(crate) fn point_gesture_with_modifiers(
    doc: &Document,
    id: u64,
    camera: Camera,
    index: usize,
    grab_offset: Vector,
    motion: Vector,
    modifiers: iced::keyboard::Modifiers,
) -> PointGesture {
    let graphic = doc.graphics.iter().find(|g| g.id == id).unwrap();
    let original = graphic
        .path_handles()
        .unwrap()
        .into_iter()
        .find(|handle| handle.index == index)
        .unwrap()
        .point;
    let mut canvas = tests::chain_canvas(doc, ChainMode::Straight);
    canvas.tool = Tool::EditPoints;
    canvas.selected = std::slice::from_ref(&id);
    canvas.camera = camera;
    let bounds = Rectangle::new(Point::new(37., 53.), iced::Size::new(1600., 1800.));
    let from = camera.screen(original, bounds) + Vector::new(bounds.x, bounds.y) + grab_offset;
    let end = from + motion;
    // Iced can report the final cursor while delivering earlier events in a batch.
    let cursor = mouse::Cursor::Available(end);
    let mut state = State::default();
    canvas.update(
        &mut state,
        &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)),
        bounds,
        cursor,
    );
    let mut before_release = Vec::new();
    for event in [
        mouse::Event::CursorMoved { position: from },
        mouse::Event::ButtonPressed(mouse::Button::Left),
    ] {
        before_release.extend(
            canvas
                .update(&mut state, &Event::Mouse(event), bounds, cursor)
                .and_then(|action| action.into_inner().0),
        );
    }
    let pressed_preview = canvas.pointer_preview_document(&state, bounds);
    if motion != Vector::ZERO {
        before_release.extend(
            canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::CursorMoved { position: end }),
                    bounds,
                    cursor,
                )
                .and_then(|action| action.into_inner().0),
        );
    }
    let preview = canvas.pointer_preview_document(&state, bounds);
    let release = canvas
        .update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .and_then(|action| action.into_inner().0)
        .expect("point selection or completed drag");
    PointGesture {
        before_release,
        pressed_preview,
        preview,
        release,
    }
}

#[test]
fn pen_offset_click_selects_exact_nodes_and_controls_without_changing_the_preview() {
    let mut doc = Document::from_native_file(include_bytes!(
        "../../tests/fixtures/tunable-pen-lines-67/before.rsk"
    ))
    .unwrap();
    let single = doc.next_id();
    doc.graphics.push(Graphic::pen_curve(
        single,
        World::new(-80., -10.),
        World::new(80., 40.),
        GraphicStyle::default(),
    ));
    for graphic in &doc.graphics {
        for handle in graphic.path_handles().unwrap() {
            for zoom in [1.48, 2.5] {
                let events = point_gesture(
                    &doc,
                    graphic.id,
                    Camera {
                        center: World::new(111.13, 28.7),
                        zoom,
                    },
                    handle.index,
                    Vector::new(2.25, -1.75),
                    Vector::ZERO,
                );
                assert_eq!(events.pressed_preview, doc);
                assert_eq!(events.preview, doc);
                assert!(events.before_release.iter().all(|edit| edit.is_hover()));
                if handle.node {
                    assert!(matches!(
                        events.release,
                        Edit::PathNodeClick(id, index) if (id, index) == (graphic.id, handle.index)
                    ));
                } else {
                    let Edit::GraphicPoint(id, index, point) = events.release else {
                        panic!("control selection")
                    };
                    assert_eq!((id, index, point), (graphic.id, handle.index, handle.point));
                }
            }
        }
    }
}

fn escape() -> Event {
    use iced::keyboard::{self, key};
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(key::Named::Escape),
        modified_key: keyboard::Key::Named(key::Named::Escape),
        physical_key: key::Physical::Code(key::Code::Escape),
        location: keyboard::Location::Standard,
        modifiers: Default::default(),
        text: None,
        repeat: false,
    })
}

#[test]
fn pen_preview_commit_and_escape_share_segments_for_new_append_and_close() {
    let mut open = Document::default();
    let mut graphic = Graphic::pen_curve(
        1,
        World::new(-60., 0.),
        World::new(0., 20.),
        GraphicStyle::default(),
    );
    graphic.append_pen_node(World::new(50., 0.), None).unwrap();
    open.graphics.push(graphic);
    let empty = Document::default();
    for (doc, selected, from, to, closing) in [
        (
            &empty,
            &[][..],
            World::new(-70., -20.),
            World::new(40., 20.),
            false,
        ),
        (
            &open,
            &[1][..],
            World::new(100., 30.),
            World::new(100., 30.),
            false,
        ),
        (
            &open,
            &[1][..],
            World::new(100., 30.),
            World::new(100., 75.),
            false,
        ),
        (
            &open,
            &[1][..],
            World::new(-60., 0.),
            World::new(-60., 0.),
            true,
        ),
    ] {
        for zoom in [0.75, 1.75] {
            let mut canvas = tests::chain_canvas(doc, ChainMode::Straight);
            canvas.tool = Tool::Graphic(GraphicKind::Path);
            canvas.selected = selected;
            canvas.camera.zoom = zoom;
            let bounds = Rectangle::new(Point::new(30., 40.), iced::Size::new(500., 400.));
            let screen = |p| canvas.camera.screen(p, bounds) + Vector::new(bounds.x, bounds.y);
            let cursor = mouse::Cursor::Available(screen(to));
            for cancel in [false, true] {
                let mut state = State::default();
                for event in [
                    mouse::Event::CursorMoved {
                        position: screen(from),
                    },
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    mouse::Event::CursorMoved {
                        position: screen(to),
                    },
                ] {
                    assert!(!matches!(
                        canvas
                            .update(&mut state, &Event::Mouse(event), bounds, cursor)
                            .and_then(|a| a.into_inner().0),
                        Some(Edit::PenSegment(_))
                    ));
                }
                let preview = canvas.pointer_preview_document(&state, bounds);
                assert_eq!(preview.graphics.len(), 1);
                assert_eq!(preview.graphics[0].path_closed(), closing);
                assert_eq!(doc.graphics.len(), if selected.is_empty() { 0 } else { 1 });
                if cancel {
                    canvas.update(&mut state, &escape(), bounds, cursor);
                    assert_eq!(canvas.pointer_preview_document(&state, bounds), *doc);
                }
                let edit = canvas
                    .update(
                        &mut state,
                        &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                        bounds,
                        cursor,
                    )
                    .and_then(|a| a.into_inner().0);
                if cancel {
                    assert!(!matches!(edit, Some(Edit::PenSegment(_))));
                } else {
                    let Some(Edit::PenSegment(stroke)) = edit else {
                        panic!("one completed pen segment")
                    };
                    let (committed, _, _) =
                        pen::apply(doc, selected, canvas.graphics.style, stroke).unwrap();
                    assert_eq!(committed, preview);
                    assert_eq!(committed.atoms, doc.atoms);
                    assert_eq!(committed.bonds, doc.bonds);
                }
            }
        }
    }
}

#[test]
fn pen_first_click_is_inert_and_point_drag_carries_adjacent_tangents() {
    let empty = Document::default();
    let mut canvas = tests::chain_canvas(&empty, ChainMode::Straight);
    canvas.tool = Tool::Graphic(GraphicKind::Path);
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let cursor = mouse::Cursor::Available(Point::new(200., 150.));
    let mut state = State::default();
    for event in [
        mouse::Event::CursorMoved {
            position: Point::new(200., 150.),
        },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ] {
        assert!(!matches!(
            canvas
                .update(&mut state, &Event::Mouse(event), bounds, cursor)
                .and_then(|a| a.into_inner().0),
            Some(Edit::PenSegment(_))
        ));
    }
    assert_eq!(canvas.pointer_preview_document(&state, bounds), empty);
    let doc = Document::from_native_file(include_bytes!(
        "../../tests/fixtures/tunable-pen-lines-67/before.rsk"
    ))
    .unwrap();
    let id = doc.graphics[0].id;
    let mut canvas = tests::chain_canvas(&doc, ChainMode::Straight);
    canvas.selected = std::slice::from_ref(&id);
    canvas.tool = Tool::EditPoints;
    canvas.camera.center = World::new(100., 30.);
    let points = doc.graphics[0].edit_points();
    let to = points[3].offset(20., -10.);
    let screen = |p| canvas.camera.screen(p, bounds);
    let edit = tests::pointer_gesture(&canvas, screen(points[3]), screen(to));
    let Edit::PathPoint {
        id: actual,
        index,
        point,
        free: false,
    } = edit
    else {
        panic!("node drag")
    };
    assert_eq!((actual, index, point), (id, 3, to));
    let mut committed = doc.clone();
    committed.graphics[0].move_path_point(index, point).unwrap();
    let moved = committed.graphics[0].edit_points();
    for index in [2, 3, 4] {
        assert_eq!(moved[index], points[index].offset(20., -10.));
    }
    for index in [0, 1, 5, 6, 7, 8, 9] {
        assert_eq!(moved[index], points[index]);
    }
}

fn three_node_path(node_type: reshiki::graphics::PathNodeType) -> Document {
    let mut graphic = Graphic::pen_curve(
        1,
        World::new(-80., 0.),
        World::default(),
        GraphicStyle::default(),
    );
    graphic
        .append_pen_node(World::new(100., 20.), Some(World::new(120., 30.)))
        .unwrap();
    graphic.set_path_node_type(3, node_type).unwrap();
    Document {
        graphics: vec![graphic],
        ..Document::default()
    }
}

fn commit_point(doc: &Document, edit: Edit) -> Document {
    let Edit::PathPoint {
        id,
        index,
        point,
        free,
    } = edit
    else {
        panic!("one path handle drag")
    };
    let mut committed = doc.clone();
    let graphic = committed.graphics.iter_mut().find(|g| g.id == id).unwrap();
    if free {
        graphic.move_path_point_free(index, point).unwrap();
    } else {
        graphic.move_path_point(index, point).unwrap();
    }
    committed
}

#[test]
fn smooth_and_symmetric_control_drags_preview_the_opposite_handle_and_commit_same_data() {
    use reshiki::graphics::PathNodeType;
    for node_type in [PathNodeType::Smooth, PathNodeType::Symmetric] {
        let doc = three_node_path(node_type);
        let before = doc.graphics[0].edit_points();
        for (index, opposite) in [(2, 4), (4, 2)] {
            let camera = Camera {
                center: World::new(17., -19.),
                zoom: 1.75,
            };
            let motion = Vector::new(21., -32.);
            let events = point_gesture(&doc, 1, camera, index, Vector::new(2., -1.5), motion);
            assert!(events.before_release.iter().all(Edit::is_hover));
            assert_eq!(events.pressed_preview, doc);
            let moved = events.preview.graphics[0].edit_points();
            assert!(
                moved[index]
                    .distance(before[index].offset(motion.x / camera.zoom, motion.y / camera.zoom))
                    < 0.0001
            );
            assert_eq!(moved[3], before[3]);
            let (a, b) = (moved[index], moved[opposite]);
            let (ax, ay) = (a.x - moved[3].x, a.y - moved[3].y);
            let (bx, by) = (b.x - moved[3].x, b.y - moved[3].y);
            assert!((ax * by - ay * bx).abs() < 0.001);
            assert!(ax * bx + ay * by < 0.);
            let expected_length = if node_type == PathNodeType::Symmetric {
                moved[3].distance(moved[index])
            } else {
                before[3].distance(before[opposite])
            };
            assert!((moved[3].distance(moved[opposite]) - expected_length).abs() < 0.0001);
            assert_ne!(moved[opposite], before[opposite]);
            assert_eq!(commit_point(&doc, events.release), events.preview);
            assert_eq!(doc.graphics[0].edit_points(), before);
        }
    }
}

#[test]
fn option_control_drag_previews_corner_and_carries_the_same_free_commit() {
    use reshiki::graphics::PathNodeType;
    for node_type in [PathNodeType::Smooth, PathNodeType::Symmetric] {
        for index in [2, 4] {
            let doc = three_node_path(node_type);
            let before = doc.graphics[0].edit_points();
            let camera = Camera {
                center: World::new(17., -19.),
                zoom: 2.,
            };
            let motion = Vector::new(24., -34.);
            let events = point_gesture_with_modifiers(
                &doc,
                1,
                camera,
                index,
                Vector::new(-2., 1.5),
                motion,
                iced::keyboard::Modifiers::ALT,
            );
            assert!(events.before_release.iter().all(Edit::is_hover));
            assert_eq!(events.pressed_preview, doc);
            let graphic = &events.preview.graphics[0];
            assert_eq!(graphic.path_node_types().unwrap()[1], PathNodeType::Corner);
            let moved = graphic.edit_points();
            for (other, old) in before.iter().enumerate() {
                let target = if other == index {
                    old.offset(motion.x / camera.zoom, motion.y / camera.zoom)
                } else {
                    *old
                };
                assert!(moved[other].distance(target) < 0.0001);
            }
            assert!(matches!(events.release, Edit::PathPoint { free: true, .. }));
            assert_eq!(commit_point(&doc, events.release), events.preview);
        }
    }
}

#[test]
fn option_control_click_selects_without_converting_the_node_or_moving_a_handle() {
    use reshiki::graphics::PathNodeType;
    let doc = three_node_path(PathNodeType::Symmetric);
    for index in [2, 4] {
        let events = point_gesture_with_modifiers(
            &doc,
            1,
            Camera {
                center: World::new(17., -19.),
                zoom: 2.5,
            },
            index,
            Vector::new(2., -1.5),
            Vector::new(1., 0.5),
            iced::keyboard::Modifiers::ALT,
        );
        assert!(events.before_release.iter().all(Edit::is_hover));
        assert_eq!(events.pressed_preview, doc);
        assert_eq!(events.preview, doc);
        let Edit::GraphicPoint(id, actual, point) = events.release else {
            panic!("select the control without a drag")
        };
        assert_eq!(
            (id, actual, point),
            (1, index, doc.graphics[0].edit_points()[index])
        );
    }
}

#[test]
fn edit_points_sends_each_node_click_without_preview_movement_or_a_drag_edit() {
    use reshiki::graphics::PathNodeType;
    let doc = three_node_path(PathNodeType::Symmetric);
    for zoom in [0.75, 2.5] {
        for modifiers in [
            iced::keyboard::Modifiers::empty(),
            iced::keyboard::Modifiers::ALT,
        ] {
            // The app counts double-clicks; the canvas sends each node click.
            for _ in 0..2 {
                let events = point_gesture_with_modifiers(
                    &doc,
                    1,
                    Camera {
                        center: World::new(17., -19.),
                        zoom,
                    },
                    3,
                    Vector::new(2., -1.5),
                    Vector::new(1., 0.5),
                    modifiers,
                );
                assert!(events.before_release.iter().all(Edit::is_hover));
                assert_eq!(events.pressed_preview, doc);
                assert_eq!(events.preview, doc);
                assert!(matches!(events.release, Edit::PathNodeClick(1, 3)));
            }
        }
    }
}
