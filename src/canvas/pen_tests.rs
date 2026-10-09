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
    let bounds = Rectangle::new(Point::new(37., 53.), iced::Size::new(1600., 1000.));
    let from = camera.screen(original, bounds) + Vector::new(bounds.x, bounds.y) + grab_offset;
    let end = from + motion;
    // Iced can report the final cursor while delivering earlier events in a batch.
    let cursor = mouse::Cursor::Available(end);
    let mut state = State::default();
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
                assert!(
                    events
                        .before_release
                        .iter()
                        .all(|edit| matches!(edit, Edit::Hover(_)))
                );
                let Edit::GraphicPoint(id, index, point) = events.release else {
                    panic!("handle selection")
                };
                assert_eq!((id, index, point), (graphic.id, handle.index, handle.point));
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
                        pen::apply(doc, selected, canvas.graphic_style, stroke).unwrap();
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
    let Edit::GraphicPoint(actual, index, point) = edit else {
        panic!("node drag")
    };
    assert_eq!((actual, index, point), (id, 3, to));
    let mut committed = doc.clone();
    committed.graphics[0].edit_point(index, point);
    let moved = committed.graphics[0].edit_points();
    for index in [2, 3, 4] {
        assert_eq!(moved[index], points[index].offset(20., -10.));
    }
    for index in [0, 1, 5, 6, 7, 8, 9] {
        assert_eq!(moved[index], points[index]);
    }
}
