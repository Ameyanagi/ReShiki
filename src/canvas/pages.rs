use super::{Camera, rgb};
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::{Point, Rectangle, Size};

pub fn draw(frame: &mut Frame, layout: &moruno::pages::Layout, camera: Camera, bounds: Rectangle) {
    if layout.validate().is_err() {
        return;
    }
    frame.fill_rectangle(Point::ORIGIN, bounds.size(), rgb([232, 236, 239]));
    for i in 0..layout.count() {
        let Some((lo, hi)) = layout.bounds(i) else {
            continue;
        };
        let a = camera.screen(lo, bounds);
        let b = camera.screen(hi, bounds);
        let rect = Rectangle::new(a, Size::new((b.x - a.x).max(0.), (b.y - a.y).max(0.)));
        if !rect.intersects(&Rectangle::with_size(bounds.size())) {
            continue;
        }
        frame.fill_rectangle(
            Point::new(a.x + 3., a.y + 3.),
            rect.size(),
            rgb([218, 223, 227]),
        );
        frame.fill_rectangle(a, rect.size(), iced::Color::WHITE);
        frame.stroke(
            &Path::rectangle(a, rect.size()),
            Stroke::default().with_color(rgb([197, 206, 211])),
        );
        if let Some((lo, hi)) = layout.content_bounds(i) {
            let a = camera.screen(lo, bounds);
            let b = camera.screen(hi, bounds);
            frame.stroke(
                &Path::rectangle(a, Size::new(b.x - a.x, b.y - a.y)),
                Stroke {
                    line_dash: canvas::LineDash {
                        segments: &[3., 4.],
                        offset: 0,
                    },
                    ..Stroke::default().with_color(rgb([205, 218, 215]))
                },
            );
        }
        frame.fill_text(canvas::Text {
            content: format!("{}", i + 1),
            position: Point::new(a.x + 5., a.y - 17.),
            size: 11.into(),
            color: rgb([97, 112, 120]),
            ..Default::default()
        });
    }
}
