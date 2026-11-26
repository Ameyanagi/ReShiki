//! Prepared ball-and-stick thumbnails using the paper's model renderer.

use crate::canvas::{BondChangeKind, ModelImage};
use iced::widget::canvas;
use iced::{Color, Point, Size, Theme, Vector};
use reshiki::canvas_theme::CanvasTheme;
use reshiki::document::{Document, Point as World};
use reshiki::models3d::{self, Display, ForceField, Model3d, Placed};
use reshiki::palette::Palette;
use reshiki::qcdata::Frame;

#[derive(Clone, Copy, Debug)]
struct Change {
    kind: BondChangeKind,
    points: [World; 2],
}

#[derive(Clone, Debug)]
pub(in crate::app) struct Thumbnail {
    image: ModelImage,
    lo: World,
    hi: World,
    changes: Vec<Change>,
    palettes: [Palette; 2],
}

impl Thumbnail {
    /// `bonds` and change pairs use the exact frame atom order. Changed bonds
    /// are shown only as dashed overlays, rather than solid model sticks.
    pub(in crate::app) fn new(
        frame: &Frame,
        bonds: &[[usize; 2]],
        forming: &[[usize; 2]],
        breaking: &[[usize; 2]],
        rotation: [f32; 4],
    ) -> Option<Self> {
        frame.validate().ok()?;
        if frame.numbers.is_empty() {
            return None;
        }
        let positions: Vec<_> = frame
            .positions_angstrom
            .iter()
            .map(|point| point.map(|value| value as f32))
            .collect();
        if positions.iter().flatten().any(|value| !value.is_finite()) {
            return None;
        }
        let mut doc = Document::default();
        let atoms: Vec<_> = frame
            .numbers
            .iter()
            .zip(&positions)
            .map(|(&number, &xyz)| {
                Some(Placed {
                    id: doc.add_atom(
                        reshiki::editing::ELEMENTS.get(usize::from(number) - 1)?,
                        World::new(xyz[0] * 30., xyz[1] * 30.),
                    ),
                    xyz,
                })
            })
            .collect::<Option<_>>()?;
        for &[a, b] in bonds {
            let (a, b) = (atoms.get(a)?, atoms.get(b)?);
            if a.id != b.id {
                doc.add_bond(a.id, b.id, 1, "plain");
            }
        }
        let mut model = Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s);
        model.display = Display::ThreeD;
        model.rotation = rotation;
        model.size = 100.;
        model.perspective = 0.;
        let projected = models3d::project(&doc, &model, &positions)?;
        let changes = [
            (BondChangeKind::Form, forming),
            (BondChangeKind::Break, breaking),
        ]
        .into_iter()
        .flat_map(|(kind, pairs)| {
            let projected = &projected;
            pairs.iter().filter_map(move |&[a, b]| {
                (a != b).then_some(Change {
                    kind,
                    points: [*projected.get(a)?, *projected.get(b)?],
                })
            })
        })
        .collect();
        // The frozen graph makes these thumbnails independent of later edits
        // to the drawing, including transferred and geometry-only hydrogens.
        if let Some(generated) = &mut model.generated {
            generated.bonds.retain(|pair| {
                !forming
                    .iter()
                    .chain(breaking)
                    .any(|change| pair == change || pair == &[change[1], change[0]])
            });
        }
        doc.models.push(model);
        let primitives = models3d::primitives(&doc);
        if primitives.is_empty() {
            return None;
        }
        let (lo, hi) = reshiki::scene::bounds(&primitives);
        // Keep outlines and the antialiased sphere edges inside the image.
        let lo = lo.offset(-1., -1.);
        let hi = hi.offset(1., 1.);
        Some(Self {
            image: ModelImage::new(primitives, lo, hi),
            lo,
            hi,
            changes,
            palettes: CanvasTheme::ALL.map(|mode| Palette::in_mode(&doc, mode)),
        })
    }

    /// Bind colours after the geometry-cache lookup, so changing a document's
    /// built-in or embedded palette never reuses stale overlay swatches.
    pub(in crate::app) fn in_document(mut self, doc: &Document) -> Self {
        self.palettes = CanvasTheme::ALL.map(|mode| Palette::in_mode(doc, mode));
        self
    }

    fn change_color(&self, kind: BondChangeKind, mode: CanvasTheme) -> Color {
        let [light, dark] = &self.palettes;
        let palette = if mode.is_dark() { dark } else { light };
        let [r, g, b] = palette.rgb(kind.color());
        Color::from_rgb8(r, g, b)
    }

    pub(in crate::app) fn draw(
        &self,
        frame: &mut canvas::Frame,
        center: Point,
        size: Size,
        theme: &Theme,
    ) {
        let scale =
            (size.width / (self.hi.x - self.lo.x)).min(size.height / (self.hi.y - self.lo.y));
        if !scale.is_finite() || scale <= 0. {
            return;
        }
        let origin = center
            - Vector::new(
                (self.hi.x - self.lo.x) * scale * 0.5,
                (self.hi.y - self.lo.y) * scale * 0.5,
            );
        let project = |point: World| {
            origin + Vector::new((point.x - self.lo.x) * scale, (point.y - self.lo.y) * scale)
        };
        let dark = crate::appearance::is_dark(theme);
        let mode = if dark {
            CanvasTheme::Dark
        } else {
            CanvasTheme::Light
        };
        for change in &self.changes {
            let color = self.change_color(change.kind, mode);
            let mut stroke = canvas::Stroke::default()
                .with_color(color)
                .with_width(1.6)
                .with_line_cap(canvas::LineCap::Round);
            stroke.line_dash.segments = &[3., 2.5];
            frame.stroke(
                &canvas::Path::line(project(change.points[0]), project(change.points[1])),
                stroke,
            );
        }
        // Canvas paints images above strokes, so the atom balls naturally
        // cover the dashed bond ends, exactly as they cover ordinary sticks.
        self.image.draw(frame, origin, scale, mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            numbers: vec![6, 6, 8],
            positions_angstrom: vec![[-1.2, 0., 0.1], [0., 0.3, -0.1], [1.2, 0., 0.2]],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }
    }

    #[test]
    fn prepared_thumbnail_keeps_real_atom_projection_and_change_directions() {
        let thumb = Thumbnail::new(
            &frame(),
            &[[0, 1], [1, 2]],
            &[[0, 2]],
            &[[1, 2]],
            [1., 0., 0., 0.],
        )
        .unwrap();
        assert_eq!(thumb.changes.len(), 2);
        assert_eq!(thumb.changes[0].kind, BondChangeKind::Form);
        assert_eq!(thumb.changes[1].kind, BondChangeKind::Break);
        assert_eq!(thumb.changes[0].points[1], thumb.changes[1].points[1]);
        assert!(thumb.hi.x > thumb.lo.x && thumb.hi.y > thumb.lo.y);
        for point in thumb.changes.iter().flat_map(|change| change.points) {
            assert!(point.x >= thumb.lo.x && point.x <= thumb.hi.x);
            assert!(point.y >= thumb.lo.y && point.y <= thumb.hi.y);
        }
    }

    #[test]
    fn invalid_geometry_and_bond_indices_do_not_produce_a_thumbnail() {
        assert!(Thumbnail::new(&frame(), &[[0, 9]], &[], &[], [1., 0., 0., 0.]).is_none());
        let mut invalid = frame();
        invalid.positions_angstrom[0][0] = f64::NAN;
        assert!(Thumbnail::new(&invalid, &[], &[], &[], [1., 0., 0., 0.]).is_none());
    }

    #[test]
    fn bond_changes_share_drawing_swatches_for_default_and_custom_palettes_in_both_modes() {
        use reshiki::palette::{self, Hue, Row};
        let thumb = Thumbnail::new(&frame(), &[], &[[0, 2]], &[[1, 2]], [1., 0., 0., 0.]).unwrap();
        let default = Document::default();
        let mut custom = Document::default();
        let mut theme = reshiki::theme_files::ThemeFile::capture(&custom);
        theme.hues.set(Hue::Green, 190);
        theme.hues.set(Hue::Red, 330);
        custom.custom_theme = Some(Box::new(theme));
        for doc in [&default, &custom] {
            let bound = thumb.clone().in_document(doc);
            for mode in CanvasTheme::ALL {
                let palette = Palette::in_mode(doc, mode);
                for (kind, hue) in [
                    (BondChangeKind::Form, Hue::Green),
                    (BondChangeKind::Break, Hue::Red),
                ] {
                    assert_eq!(kind.color(), palette::Color::Palette(hue, Row::Strong));
                    // The layered paper renderer converts these canonical bytes
                    // once; the ordinary thumbnail frame uses the display RGB.
                    let [r, g, b] = mode.color(palette.canonical(kind.color()));
                    assert_eq!(bound.change_color(kind, mode), Color::from_rgb8(r, g, b));
                }
            }
        }
        let default = thumb.clone().in_document(&default);
        let custom = thumb.in_document(&custom);
        for mode in CanvasTheme::ALL {
            for kind in [BondChangeKind::Form, BondChangeKind::Break] {
                assert_ne!(
                    default.change_color(kind, mode),
                    custom.change_color(kind, mode)
                );
            }
        }
        assert_ne!(
            custom.change_color(BondChangeKind::Form, CanvasTheme::Light),
            custom.change_color(BondChangeKind::Form, CanvasTheme::Dark)
        );
    }
}
