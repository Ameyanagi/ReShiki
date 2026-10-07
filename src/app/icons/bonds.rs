use super::{line, polygon};
use crate::canvas::layered::Frame;
use iced::widget::canvas::{Path, Stroke};
use iced::{Color, Point};

pub(super) fn bond(f: &mut Frame<'_>, ink: Color, order: u8) {
    let offsets: &[f32] = match order {
        2 => &[-2., 2.],
        3 => &[-4., 0., 4.],
        _ => &[0.],
    };
    for dy in offsets {
        line(f, ink, &[(4., 18. + dy), (20., 6. + dy)]);
    }
}

pub(super) fn styled_bond(f: &mut Frame<'_>, ink: Color, preset: reshiki::bonds::BondPreset) {
    use reshiki::bonds::BondPreset as P;
    match preset {
        P::Dative => {
            line(f, ink, &[(3., 19.), (20., 5.), (13., 6.)]);
            line(f, ink, &[(20., 5.), (18., 12.)]);
        }
        P::Quadruple => {
            for dy in [-4.5, -1.5, 1.5, 4.5] {
                line(f, ink, &[(4., 17. + dy), (20., 7. + dy)]);
            }
        }
        P::HollowWedge => line(f, ink, &[(4., 19.), (17., 3.), (22., 10.), (4., 19.)]),
        P::Bold => f.stroke(
            &Path::line(Point::new(4., 19.), Point::new(20., 5.)),
            Stroke::default().with_width(4.).with_color(ink),
        ),
        P::Dotted => {
            for i in 0..6 {
                let t = i as f32 / 5.;
                f.fill(
                    &Path::circle(Point::new(4. + 16. * t, 19. - 14. * t), 1.1),
                    ink,
                );
            }
        }
        P::Dashed => {
            for i in 0..4 {
                let t = i as f32 / 4.;
                line(
                    f,
                    ink,
                    &[
                        (4. + 16. * t, 19. - 14. * t),
                        (4. + 16. * (t + 0.13), 19. - 14. * (t + 0.13)),
                    ],
                );
            }
        }
        P::Hashed => {
            for i in 0..6 {
                let t = i as f32 / 5.;
                let x = 5. + 14. * t;
                let y = 19. - 13. * t;
                line(f, ink, &[(x - 2.5, y - 2.5), (x + 2.5, y + 2.5)]);
            }
        }
        P::CrossedDouble => {
            line(f, ink, &[(4., 19.), (20., 5.)]);
            line(f, ink, &[(4., 15.), (20., 9.)]);
        }
        _ => {
            line(f, ink, &[(4., 19.), (20., 7.)]);
            line(f, ink, &[(4., 15.), (20., 3.)]);
        }
    }
}

pub(super) fn wedge(f: &mut Frame<'_>, ink: Color) {
    polygon(f, ink, &[(4., 19.), (17., 3.), (22., 10.)])
}

pub(super) fn hash(f: &mut Frame<'_>, ink: Color) {
    for i in 0..6 {
        let t = i as f32 / 5.;
        let x = 5. + 14. * t;
        let y = 19. - 13. * t;
        line(
            f,
            ink,
            &[(x - 3. * t, y - 3. * t), (x + 3. * t, y + 3. * t)],
        );
    }
}

pub(super) fn wavy(f: &mut Frame<'_>, ink: Color) {
    use reshiki::{document::Point as World, graphics::PathCommand};
    let path = Path::new(|p| {
        for command in reshiki::bonds::wavy_path(World::new(3., 12.), World::new(21., 12.), 6., 3.)
        {
            match command {
                PathCommand::Move(a) => p.move_to(Point::new(a.x, a.y)),
                PathCommand::Cubic(a, b, c) => p.bezier_curve_to(
                    Point::new(a.x, a.y),
                    Point::new(b.x, b.y),
                    Point::new(c.x, c.y),
                ),
                _ => {}
            }
        }
    });
    f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
}
