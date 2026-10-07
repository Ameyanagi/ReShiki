use super::{Icon, line};
use crate::canvas::layered::Frame;
use iced::widget::canvas::{Path, Stroke};
use iced::{Color, Point};

pub(super) fn arrow(f: &mut Frame<'_>, ink: Color, icon: Icon) {
    use reshiki::{
        arrows::{ArrowStyle, Preset},
        document::{Arrow, Point as World},
        graphics::PathCommand,
    };
    let preset = match icon {
        Icon::Arrow(preset) => preset,
        _ => Preset::Forward,
    };
    if preset == Preset::Forward {
        line(f, ink, &[(3., 12.), (21., 12.)]);
        line(f, ink, &[(15., 6.), (21., 12.), (15., 18.)]);
        return;
    }
    let style = ArrowStyle {
        head_length_pt: 8.,
        head_width_pt: 4.,
        gap_pt: 3.2,
        ..ArrowStyle::preset(preset)
    };
    let arrow = Arrow::new(1, World::default(), World::new(80., 0.), preset, style);
    let (lo, hi) = arrow.bounds();
    let scale = (20. / (hi.x - lo.x).max(1.)).min(18. / (hi.y - lo.y).max(1.));
    let point = |p: World| {
        Point::new(
            12. + (p.x - (lo.x + hi.x) / 2.) * scale,
            12. + (p.y - (lo.y + hi.y) / 2.) * scale,
        )
    };
    for part in arrow.paths() {
        let path = Path::new(|b| {
            for command in part.commands {
                match command {
                    PathCommand::Move(p) => b.move_to(point(p)),
                    PathCommand::Line(p) => b.line_to(point(p)),
                    PathCommand::Cubic(a, z, p) => b.bezier_curve_to(point(a), point(z), point(p)),
                    PathCommand::Close => b.close(),
                }
            }
        });
        if part.filled {
            f.fill(&path, ink);
        } else {
            f.stroke(&path, Stroke::default().with_width(1.4).with_color(ink));
        }
    }
}
