use super::*;

/// Text outlined at a zoom step and scaled up to a zoom between steps is
/// kept while any of it reaches the viewport, as is its halo.
#[test]
fn zoomed_text_partly_on_screen_between_zoom_steps_is_not_culled() {
    let zoom = 1.09;
    let step = text_cache::zoom_step(zoom);
    assert!(step < zoom);
    let paths = text_cache::outline("OH", 40., step, [0, 0, 0], &Default::default());
    let text = paths.bounds.unwrap();
    let scale = zoom / step;
    // Only the scaled text's last pixels reach the left edge.
    let right = (text.x + text.width) * scale;
    let at = Point::new(1. - right, 10.);
    let viewport = Rectangle::with_size(iced::Size::new(200., 100.));
    assert!(on_screen(&paths, at, scale, 0., viewport));
    assert!(on_screen(&paths, at, scale, 3., viewport));
    // Wholly off screen, it is culled.
    let at = Point::new(-right - 1., 10.);
    assert!(!on_screen(&paths, at, scale, 0., viewport));
}
