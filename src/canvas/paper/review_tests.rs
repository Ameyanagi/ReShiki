use super::super::*;
use iced::widget::canvas::Program;

fn handles(doc: &Document, ids: &[u64], camera: Camera, bounds: Rectangle) -> Vec<(Point, Handle)> {
    let (lo, hi) = reshiki::scene::selection_bounds(doc, ids).unwrap();
    let lo = camera.screen(lo, bounds) - Vector::new(12., 12.);
    let hi = camera.screen(hi, bounds) + Vector::new(12., 12.);
    let x = (lo.x + hi.x) / 2.;
    let y = (lo.y + hi.y) / 2.;
    [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)]
        .into_iter()
        .enumerate()
        .map(|(i, p)| (p, Handle::Resize(i)))
        .chain(
            [
                Point::new(x, lo.y),
                Point::new(hi.x, y),
                Point::new(x, hi.y),
                Point::new(lo.x, y),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, p)| (p, Handle::Edge(i))),
        )
        .chain([(Point::new(x, lo.y - 28.), Handle::Rotate)])
        .collect()
}

#[test]
fn reviewing_pair_disables_all_transform_handle_hits_and_cursors() {
    let mut doc = Document::default();
    let ids = [
        doc.add_atom("N", World::new(-120., -35.)),
        doc.add_atom("N", World::new(120., 35.)),
    ];
    let bounds = Rectangle::new(Point::new(40., 65.), iced::Size::new(640., 400.));
    for reviewing_pair in [false, true] {
        let canvas = MoleculeCanvas {
            tool: Tool::Select,
            selected: &ids,
            reviewing_pair,
            ..tests::chain_canvas(&doc, ChainMode::Straight)
        };
        let selection = SelectionBox::new(&doc, &ids, canvas.camera, bounds).unwrap();
        for (point, handle) in handles(&doc, &ids, canvas.camera, bounds) {
            assert_eq!(selection.hit(point), Some(handle));
            let position = point + Vector::new(bounds.x, bounds.y);
            let cursor = mouse::Cursor::Available(position);
            let mut state = State::default();
            assert_eq!(
                canvas.mouse_interaction(&state, bounds, cursor),
                if reviewing_pair {
                    mouse::Interaction::default()
                } else {
                    handle.cursor()
                },
                "{handle:?}, reviewing={reviewing_pair}",
            );
            for event in [
                mouse::Event::CursorMoved { position },
                mouse::Event::ButtonPressed(mouse::Button::Left),
            ] {
                let _ = canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
            }
            assert_eq!(
                matches!(state.gesture, Some(Gesture::Transform(_))),
                !reviewing_pair
            );
        }
    }
}
