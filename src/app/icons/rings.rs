use super::{Icon, line};
use crate::canvas::layered::Frame;
use iced::widget::canvas::{Path, Stroke};
use iced::{Color, Point};

pub(super) fn ring_preset(f: &mut Frame<'_>, ink: Color, preset: reshiki::rings::Preset) {
    let doc = preset.document(7., false);
    for bond in &doc.bonds {
        let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
            continue;
        };
        let (a, b) = (a.position, b.position);
        line(f, ink, &[(12. + a.x, 12. + a.y), (12. + b.x, 12. + b.y)]);
        if bond.order == 2 {
            line(
                f,
                ink,
                &[
                    (12. + a.x * 0.68, 12. + a.y * 0.68),
                    (12. + b.x * 0.68, 12. + b.y * 0.68),
                ],
            );
        }
    }
}

pub(super) fn ring(f: &mut Frame<'_>, ink: Color, icon: Icon) {
    let (size, aromatic) = match icon {
        Icon::Ring(size, aromatic) => (size, aromatic),
        _ => (6, false),
    };
    let points: Vec<_> = (0..=size)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / size as f32;
            (12. + 9. * a.cos(), 12. + 9. * a.sin())
        })
        .collect();
    line(f, ink, &points);
    if aromatic {
        f.stroke(
            &Path::circle(Point::new(12., 12.), 5.7),
            Stroke::default().with_width(1.4).with_color(ink),
        );
    }
}
