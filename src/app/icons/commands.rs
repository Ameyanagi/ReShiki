use super::{Icon, line};
use crate::canvas::layered::Frame;
use iced::widget::canvas::{Path, Stroke};
use iced::{Color, Point};

pub(super) fn trash(f: &mut Frame<'_>, ink: Color) {
    line(f, ink, &[(4., 6.), (20., 6.)]);
    line(f, ink, &[(9., 6.), (9., 3.), (15., 3.), (15., 6.)]);
    line(f, ink, &[(6., 6.), (7., 21.), (17., 21.), (18., 6.)]);
    line(f, ink, &[(10., 9.), (10., 18.)]);
    line(f, ink, &[(14., 9.), (14., 18.)]);
}

pub(super) fn new_document(f: &mut Frame<'_>, ink: Color) {
    line(
        f,
        ink,
        &[
            (6., 3.),
            (16., 3.),
            (20., 7.),
            (20., 21.),
            (6., 21.),
            (6., 3.),
        ],
    );
    line(f, ink, &[(16., 3.), (16., 7.), (20., 7.)]);
    line(f, ink, &[(10., 13.), (16., 13.)]);
    line(f, ink, &[(13., 10.), (13., 16.)]);
}

pub(super) fn open(f: &mut Frame<'_>, ink: Color) {
    line(
        f,
        ink,
        &[
            (3., 20.),
            (3., 5.),
            (10., 5.),
            (13., 8.),
            (20., 8.),
            (20., 11.),
            (6., 11.),
            (3., 20.),
            (19., 20.),
            (22., 11.),
            (20., 11.),
        ],
    )
}

pub(super) fn save(f: &mut Frame<'_>, ink: Color) {
    line(
        f,
        ink,
        &[
            (4., 3.),
            (18., 3.),
            (21., 6.),
            (21., 21.),
            (4., 21.),
            (4., 3.),
        ],
    );
    line(f, ink, &[(8., 3.), (8., 9.), (17., 9.), (17., 3.)]);
    line(f, ink, &[(8., 21.), (8., 14.), (17., 14.), (17., 21.)]);
}

pub(super) fn save_as(f: &mut Frame<'_>, ink: Color) {
    line(
        f,
        ink,
        &[
            (8., 21.),
            (3., 21.),
            (3., 3.),
            (16., 3.),
            (19., 6.),
            (19., 8.),
        ],
    );
    line(f, ink, &[(7., 3.), (7., 9.), (15., 9.), (15., 3.)]);
    line(f, ink, &[(7., 21.), (7., 14.), (10., 14.)]);
    line(
        f,
        ink,
        &[
            (10., 22.),
            (11., 17.),
            (19., 9.),
            (23., 13.),
            (15., 21.),
            (10., 22.),
        ],
    );
    line(f, ink, &[(17., 11.), (21., 15.)]);
}

pub(super) fn assistant(f: &mut Frame<'_>, ink: Color, working: bool) {
    for (x, y, r) in [(9., 13., 7.), (19., 5., 3.)] {
        let inset = r * 0.28;
        line(
            f,
            ink,
            &[
                (x, y - r),
                (x + inset, y - inset),
                (x + r, y),
                (x + inset, y + inset),
                (x, y + r),
                (x - inset, y + inset),
                (x - r, y),
                (x - inset, y - inset),
                (x, y - r),
            ],
        );
    }
    if working {
        f.fill(
            &Path::circle(Point::new(20., 20.), 2.2),
            Color::from_rgb8(17, 126, 108),
        );
    }
}

pub(super) fn check(f: &mut Frame<'_>, ink: Color) {
    f.stroke(
        &Path::circle(Point::new(12., 12.), 9.),
        Stroke::default().with_width(1.6).with_color(ink),
    );
    line(f, ink, &[(7., 12.), (10.5, 15.5), (17., 8.5)]);
}

pub(super) fn cleanup(f: &mut Frame<'_>, ink: Color) {
    line(f, ink, &[(18., 2.), (11., 11.)]);
    line(
        f,
        ink,
        &[(10., 10.), (15., 14.), (10., 22.), (2., 17.), (10., 10.)],
    );
    line(f, ink, &[(8., 15.), (5., 18.)]);
    line(f, ink, &[(11., 17.), (8., 20.)]);
}

pub(super) fn undo_redo(f: &mut Frame<'_>, ink: Color, icon: Icon) {
    let points = [
        (5., 9.),
        (15., 9.),
        (19., 13.),
        (19., 17.),
        (15., 21.),
        (9., 21.),
    ];
    let flip = |p: (f32, f32)| {
        if matches!(icon, Icon::Redo) {
            (24. - p.0, p.1)
        } else {
            p
        }
    };
    line(f, ink, &points.map(flip));
    line(f, ink, &[(10., 4.), (5., 9.), (10., 14.)].map(flip));
}

pub(super) fn import_export(f: &mut Frame<'_>, ink: Color, icon: Icon) {
    line(f, ink, &[(4., 15.), (4., 21.), (20., 21.), (20., 15.)]);
    let points = if matches!(icon, Icon::Import) {
        [(7., 10.), (12., 15.), (17., 10.)]
    } else {
        [(7., 8.), (12., 3.), (17., 8.)]
    };
    line(f, ink, &points);
    line(f, ink, &[(12., 3.), (12., 15.)]);
}

pub(super) fn inspector(f: &mut Frame<'_>, ink: Color) {
    line(
        f,
        ink,
        &[(3., 4.), (21., 4.), (21., 20.), (3., 20.), (3., 4.)],
    );
    line(f, ink, &[(15., 4.), (15., 20.)]);
}
