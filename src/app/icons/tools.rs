use super::line;
use crate::canvas::layered::Frame;
use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Point};

pub(super) fn chain(f: &mut Frame<'_>, ink: Color, mode: reshiki::chains::ChainMode) {
    if mode == reshiki::chains::ChainMode::Straight {
        line(
            f,
            ink,
            &[(2., 15.), (7., 8.), (12., 15.), (17., 8.), (22., 15.)],
        );
    } else {
        line(
            f,
            ink,
            &[
                (2., 19.),
                (7., 13.),
                (5., 6.),
                (12., 3.),
                (17., 9.),
                (23., 7.),
            ],
        );
    }
}

pub(super) fn graphic(f: &mut Frame<'_>, ink: Color, kind: reshiki::graphics::GraphicKind) {
    use reshiki::{
        document::Point as World,
        graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
    };
    let (start, end) = match kind {
        GraphicKind::Orbital(_) => (World::new(12., 12.), World::new(12., 2.)),
        GraphicKind::Symbol(_) => (World::new(12., 12.), World::new(28., 12.)),
        _ => (World::new(4., 5.), World::new(21., 20.)),
    };
    let g = Graphic::dragged(
        1,
        kind,
        start,
        end,
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    );
    let path = Path::new(|b| {
        for c in g.commands() {
            match c {
                PathCommand::Move(p) => b.move_to(Point::new(p.x, p.y)),
                PathCommand::Line(p) => b.line_to(Point::new(p.x, p.y)),
                PathCommand::Cubic(a, z, p) => b.bezier_curve_to(
                    Point::new(a.x, a.y),
                    Point::new(z.x, z.y),
                    Point::new(p.x, p.y),
                ),
                PathCommand::Close => b.close(),
            }
        }
    });
    f.stroke(&path, Stroke::default().with_color(ink).with_width(1.5));
}

pub(super) fn edit_points(f: &mut Frame<'_>, ink: Color) {
    line(f, ink, &[(4., 19.), (9., 5.), (20., 12.)]);
    for p in [
        Point::new(4., 19.),
        Point::new(9., 5.),
        Point::new(20., 12.),
    ] {
        f.fill(&Path::circle(p, 2.5), ink);
    }
}

pub(super) fn select(f: &mut Frame<'_>, ink: Color) {
    line(
        f,
        ink,
        &[
            (5., 3.),
            (5., 20.),
            (10., 15.),
            (14., 22.),
            (17., 20.),
            (13., 13.),
            (21., 12.),
            (5., 3.),
        ],
    );
}

pub(super) fn lasso(f: &mut Frame<'_>, ink: Color) {
    let path = Path::new(|b| {
        b.move_to(Point::new(6., 17.));
        b.bezier_curve_to(
            Point::new(-1., 12.),
            Point::new(8., 1.),
            Point::new(16., 4.),
        );
        b.bezier_curve_to(
            Point::new(27., 8.),
            Point::new(17., 23.),
            Point::new(7., 17.),
        );
        b.bezier_curve_to(
            Point::new(1., 13.),
            Point::new(1., 23.),
            Point::new(9., 22.),
        );
    });
    f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
}

pub(super) fn tilt(f: &mut Frame<'_>, ink: Color) {
    // A foreshortened ring and curved rotation arrow.
    line(
        f,
        ink,
        &[
            (3., 13.),
            (8., 8.),
            (18., 8.),
            (22., 13.),
            (17., 18.),
            (7., 18.),
            (3., 13.),
        ],
    );
    let arc = Path::new(|p| {
        p.move_to(Point::new(5., 6.));
        p.bezier_curve_to(Point::new(8., 0.), Point::new(19., 0.), Point::new(22., 6.));
    });
    f.stroke(&arc, Stroke::default().with_width(1.6).with_color(ink));
    line(f, ink, &[(17., 4.), (22., 6.), (22., 1.)]);
}

pub(super) fn text(f: &mut Frame<'_>, ink: Color) {
    line(f, ink, &[(4., 7.), (4., 4.), (20., 4.), (20., 7.)]);
    line(f, ink, &[(12., 4.), (12., 21.)]);
    line(f, ink, &[(8., 21.), (16., 21.)]);
}

pub(super) fn atom(f: &mut Frame<'_>, ink: Color) {
    f.fill_text(canvas::Text {
        content: "C".into(),
        position: Point::new(5., 1.),
        size: 20.into(),
        color: ink,
        ..Default::default()
    });
}

pub(super) fn erase(f: &mut Frame<'_>, ink: Color) {
    line(
        f,
        ink,
        &[
            (3., 15.),
            (14., 4.),
            (22., 12.),
            (13., 21.),
            (9., 21.),
            (3., 15.),
        ],
    );
    line(f, ink, &[(8., 10.), (16., 18.)]);
}
