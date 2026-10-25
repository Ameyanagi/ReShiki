//! Matched held-gesture captures through the actual canvas program and renderer.
use super::*;
use iced::advanced::{Renderer as _, graphics::geometry::Renderer as _, renderer::Headless};
use iced::widget::canvas::Program;

#[tokio::test]
#[ignore = "Real canvas renderer: phenylalanine held at30/45/60degrees at200percent"]
async fn drawing_polish_renderer_rotation_overlays() {
    let doc = Document::from_json(include_bytes!(
        "../../tests/fixtures/phenylalanine-rotation.rsk"
    ))
    .unwrap();
    let ids = doc.all_ids();
    let bounds = Rectangle::with_size(iced::Size::new(1600., 1000.));
    let camera = Camera {
        center: reshiki::editing::center(&doc, &ids),
        zoom: 2.,
    };
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let path = std::env::var_os("RESHIKI_DRAWING_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-drawing-polish"));
    std::fs::create_dir_all(&path).unwrap();
    for (degrees, radius) in [(30_f32, 171_f32), (45., 230.), (60., 230.)] {
        let mut canvas = tests::chain_canvas(&doc, reshiki::chains::ChainMode::Straight);
        canvas.tool = Tool::Select;
        canvas.selected = &ids;
        canvas.camera = camera;
        let selection = SelectionBox::new(&doc, &ids, camera, bounds).unwrap();
        let pivot = camera.screen(selection.pivot, bounds);
        let (lo, hi) = reshiki::scene::selection_bounds(&doc, &ids).unwrap();
        let top = camera.screen(World::new((lo.x + hi.x) * 0.5, lo.y), bounds);
        let start = Point::new(top.x, top.y - 40.);
        assert_eq!(selection.hit(start), Some(Handle::Rotate));
        let dx = start.x - pivot.x;
        let dy = start.y - pivot.y;
        let distance = (dx * dx + dy * dy).sqrt();
        let (s, c) = degrees.to_radians().sin_cos();
        let end = Point::new(
            pivot.x + (dx * c - dy * s) * radius / distance,
            pivot.y + (dx * s + dy * c) * radius / distance,
        );
        let mut state = State::default();
        for event in [
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::SHIFT,
            )),
            Event::Mouse(mouse::Event::CursorMoved { position: start }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::CursorMoved { position: end }),
        ] {
            let _ = canvas.update(&mut state, &event, bounds, mouse::Cursor::Unavailable);
        }
        assert!(
            matches!(&state.gesture,Some(Gesture::Transform(d)) if matches!(d.handle,Handle::Rotate))
        );
        renderer.reset(bounds);
        for geometry in canvas.draw(
            &state,
            &renderer,
            &iced::Theme::Light,
            bounds,
            mouse::Cursor::Available(end),
        ) {
            renderer.draw_geometry(geometry);
        }
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(1600, 1000),
            1.,
            iced::Color::WHITE,
        );
        image::save_buffer(
            path.join(format!("rotation-held-{degrees}.png")),
            &pixels,
            1600,
            1000,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
