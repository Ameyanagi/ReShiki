use super::line;
use crate::canvas::layered::Frame;
use iced::widget::canvas::{Path, Stroke};
use iced::{Color, Point};

pub(super) fn color_tiles(f: &mut Frame<'_>, ink: Color, soft: bool) {
    for (i, color) in [
        Color::from_rgb8(63, 103, 185),
        Color::from_rgb8(188, 73, 63),
        Color::from_rgb8(48, 132, 100),
        Color::from_rgb8(148, 105, 166),
    ]
    .into_iter()
    .enumerate()
    {
        let at = Point::new(2. + (i % 2) as f32 * 11., 2. + (i / 2) as f32 * 11.);
        let tile = Path::rounded_rectangle(at, iced::Size::new(9., 9.), 2.into());
        f.fill(
            &tile,
            if soft {
                Color { a: 0.22, ..color }
            } else {
                color
            },
        );
        if soft {
            f.stroke(&tile, Stroke::default().with_width(1.).with_color(ink));
        }
    }
}

pub(super) fn sun(f: &mut Frame<'_>, ink: Color) {
    f.stroke(
        &Path::circle(Point::new(12., 12.), 4.2),
        Stroke::default().with_width(1.6).with_color(ink),
    );
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        line(
            f,
            ink,
            &[
                (12. + 7. * a.cos(), 12. + 7. * a.sin()),
                (12. + 10. * a.cos(), 12. + 10. * a.sin()),
            ],
        );
    }
}

pub(super) fn moon(f: &mut Frame<'_>, ink: Color) {
    let path = Path::new(|p| {
        p.move_to(Point::new(17., 3.));
        p.bezier_curve_to(
            Point::new(0., 0.),
            Point::new(0., 23.),
            Point::new(16., 21.),
        );
        p.bezier_curve_to(
            Point::new(20., 20.),
            Point::new(22., 17.),
            Point::new(22., 14.),
        );
        p.bezier_curve_to(
            Point::new(12., 19.),
            Point::new(9., 7.),
            Point::new(17., 3.),
        );
        p.close();
    });
    f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
}

pub(super) fn keyboard(f: &mut Frame<'_>, ink: Color) {
    let outline =
        Path::rounded_rectangle(Point::new(1., 5.), iced::Size::new(22., 15.), 2.5.into());
    f.stroke(&outline, Stroke::default().with_width(1.3).with_color(ink));
    for y in [9., 12.5] {
        for x in [5., 9.5, 14., 18.5] {
            line(f, ink, &[(x, y), (x + 0.6, y)]);
        }
    }
    line(f, ink, &[(7., 16.5), (17., 16.5)]);
}

pub(super) fn text_align(f: &mut Frame<'_>, ink: Color, alignment: reshiki::typography::TextAlign) {
    for i in 0..4 {
        let width = if i % 2 == 0 || alignment == reshiki::typography::TextAlign::Justified {
            16.0
        } else {
            10.0
        };
        let x = match alignment {
            reshiki::typography::TextAlign::Right => 20.0 - width,
            reshiki::typography::TextAlign::Center => (24.0 - width) / 2.0,
            _ => 4.0,
        };
        line(
            f,
            ink,
            &[(x, 5.0 + i as f32 * 4.5), (x + width, 5.0 + i as f32 * 4.5)],
        );
    }
}

pub(super) fn more(f: &mut Frame<'_>, ink: Color) {
    for x in [5., 12., 19.] {
        f.fill(&Path::circle(Point::new(x, 12.), 1.8), ink);
    }
}

// Drawn in a 20 px box; an open lock lifts its shackle clear.
pub(super) fn lock(f: &mut Frame<'_>, ink: Color, locked: bool) {
    let body = Path::rounded_rectangle(Point::new(5., 10.), iced::Size::new(10., 8.), 1.5.into());
    f.stroke(&body, Stroke::default().with_width(1.4).with_color(ink));
    let lift = if locked { 0. } else { 3. };
    let shackle = Path::new(|p| {
        p.move_to(Point::new(7., 10.));
        p.line_to(Point::new(7., 7. - lift));
        p.bezier_curve_to(
            Point::new(7., 3. - lift),
            Point::new(13., 3. - lift),
            Point::new(13., 7. - lift),
        );
        p.line_to(Point::new(13., if locked { 10. } else { 6. }));
    });
    f.stroke(&shackle, Stroke::default().with_width(1.4).with_color(ink));
}
