//! Prepared ball-and-stick thumbnails using the paper's model renderer.

use crate::canvas::{BondChangeKind, ModelImage};
use iced::widget::canvas;
use iced::{Color, Point, Size, Theme, Vector};
use reshiki::canvas_theme::CanvasTheme;
use reshiki::document::{Document, Point as World};
use reshiki::models3d::{self, Display, ForceField, Model3d, Placed};
use reshiki::palette::Palette;
use reshiki::qcdata::Frame;

#[cfg(not(target_arch = "wasm32"))]
mod animation;
#[cfg(not(target_arch = "wasm32"))]
pub(in crate::app) use animation::{AnimationPlot, AnimationRenderer};

#[derive(Clone, Copy, Debug)]
struct Change {
    kind: BondChangeKind,
    points: [World; 2],
}

#[derive(Clone, Debug)]
pub(in crate::app) struct Thumbnail {
    images: [ModelImage; 2],
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
        // Small figures need their back atoms to remain distinct from the
        // backdrop; the full paper model uses a gentler, deeper fade.
        model.depth_fade = 0.16;
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
        let artwork = themed_primitives(&mut doc);
        if artwork.iter().any(Vec::is_empty) {
            return None;
        }
        let bounds = artwork
            .each_ref()
            .map(|primitives| reshiki::scene::bounds(primitives));
        let lo = World::new(
            bounds[0].0.x.min(bounds[1].0.x),
            bounds[0].0.y.min(bounds[1].0.y),
        );
        let hi = World::new(
            bounds[0].1.x.max(bounds[1].1.x),
            bounds[0].1.y.max(bounds[1].1.y),
        );
        // Keep outlines and the antialiased sphere edges inside the image.
        let lo = lo.offset(-1., -1.);
        let hi = hi.offset(1., 1.);
        Some(Self {
            images: artwork.map(|primitives| ModelImage::new(primitives, lo, hi)),
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
        let [light, dark] = &self.images;
        let (mode, image) = if crate::appearance::is_dark(theme) {
            (CanvasTheme::Dark, dark)
        } else {
            (CanvasTheme::Light, light)
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
        image.draw(frame, origin, scale, mode);
    }
}

/// Model shading resolves depth fade, backdrop and outlines before rasterizing.
/// Keep both variants with the prepared geometry rather than recolouring a
/// light-mode raster when the application theme changes.
fn themed_primitives(doc: &mut Document) -> [Vec<reshiki::scene::Primitive>; 2] {
    let original = doc.canvas_theme;
    let artwork = CanvasTheme::ALL.map(|mode| {
        doc.canvas_theme = mode;
        let mut primitives = models3d::primitives(doc);
        for primitive in &mut primitives {
            thumbnail_contrast(primitive, mode);
        }
        primitives
    });
    doc.canvas_theme = original;
    artwork
}

/// Preserve the model renderer's theme colours and projection, with a firmer
/// rim and deeper sphere edge that survive reduction into a 72 px figure.
fn thumbnail_contrast(primitive: &mut reshiki::scene::Primitive, mode: CanvasTheme) {
    use reshiki::scene::Primitive;

    let strengthen = |outline: &mut Option<reshiki::scene::Outline>| {
        if let Some(outline) = outline {
            outline.width *= 1.6;
            outline.alpha = if mode.is_dark() { 0.78 } else { 0.95 };
        }
    };
    match primitive {
        Primitive::Sphere { stops, outline, .. } => {
            if let Some((_, color)) = stops.first_mut() {
                *color = color.map(|channel| {
                    (f32::from(channel) + (255. - f32::from(channel)) * 0.12).round() as u8
                });
            }
            if let Some((_, color)) = stops.last_mut() {
                *color = color.map(|channel| (f32::from(channel) * 0.78).round() as u8);
            }
            strengthen(outline);
        }
        Primitive::ShadedQuad { outline, .. } | Primitive::ShadedPolygon { outline, .. } => {
            strengthen(outline);
        }
        Primitive::Opacity { primitive, .. } => thumbnail_contrast(primitive, mode),
        _ => {}
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
    fn thumbnail_contrast_keeps_theme_rims_and_adds_legible_edge_shading() {
        use reshiki::scene::{Outline, Primitive};

        for (mode, color, alpha) in [
            (CanvasTheme::Light, [30, 34, 38], 0.85),
            (CanvasTheme::Dark, [225, 232, 238], 0.55),
        ] {
            let mut sphere = Primitive::Sphere {
                center: World::default(),
                radius: 4.,
                focal: World::default(),
                stops: vec![(0., [200; 3]), (0.6, [120; 3]), (1., [98; 3])],
                outline: Some(Outline {
                    color,
                    alpha,
                    width: 0.6,
                }),
                contact: None,
            };
            thumbnail_contrast(&mut sphere, mode);
            let Primitive::Sphere {
                stops,
                outline: Some(outline),
                ..
            } = sphere
            else {
                unreachable!()
            };
            assert_eq!(outline.color, color);
            assert!(outline.alpha > alpha);
            assert!(outline.width >= 0.96);
            assert!(stops[0].1[0] > 200);
            assert_eq!(stops[1].1, [120; 3]);
            assert!(stops[2].1[0] < 98);
        }
    }

    #[test]
    fn dark_thumbnail_prepares_dark_backdrop_shading_and_outlines_before_rasterization() {
        use reshiki::scene::{self, Primitive};

        let geometry = frame();
        let mut doc = Document::default();
        let atoms = geometry
            .numbers
            .iter()
            .zip(&geometry.positions_angstrom)
            .map(|(&number, xyz)| {
                let xyz = xyz.map(|value| value as f32);
                Placed {
                    id: doc.add_atom(
                        reshiki::editing::ELEMENTS[usize::from(number) - 1],
                        World::new(xyz[0] * 30., xyz[1] * 30.),
                    ),
                    xyz,
                }
            })
            .collect::<Vec<_>>();
        for pair in atoms.windows(2) {
            doc.add_bond(pair[0].id, pair[1].id, 1, "plain");
        }
        let mut model = Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s);
        model.display = Display::ThreeD;
        model.rotation = [1., 0., 0., 0.];
        model.size = 100.;
        model.perspective = 0.;
        model.depth_fade = 0.8;
        doc.models.push(model);
        doc.canvas_theme = CanvasTheme::Dark;
        let [light, dark] = themed_primitives(&mut doc);
        assert_eq!(doc.canvas_theme, CanvasTheme::Dark);
        let spheres = |artwork: &[Primitive]| {
            artwork
                .iter()
                .filter_map(|primitive| match primitive {
                    Primitive::Sphere { stops, outline, .. } => Some((stops.clone(), *outline)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let light_spheres = spheres(&light);
        let dark_spheres = spheres(&dark);
        assert!(!light_spheres.is_empty());
        assert_eq!(light_spheres.len(), dark_spheres.len());
        for ((light_stops, light_outline), (dark_stops, dark_outline)) in
            light_spheres.iter().zip(&dark_spheres)
        {
            assert_ne!(
                light_stops, dark_stops,
                "depth shading must resolve against the selected backdrop"
            );
            assert_ne!(
                light_outline, dark_outline,
                "dark thumbnails need the model renderer's light rim"
            );
        }
        let (lo, hi) = scene::bounds(&light);
        let raster = |artwork: &[Primitive], mode: CanvasTheme| {
            let svg = scene::svg_part_on(artwork, lo, hi, mode, Some(mode.background()));
            let tree = reshiki::export::parse_svg(svg).unwrap();
            let mut pixels = resvg::tiny_skia::Pixmap::new(160, 120).unwrap();
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(160. / (hi.x - lo.x), 120. / (hi.y - lo.y)),
                &mut pixels.as_mut(),
            );
            pixels.data().to_vec()
        };
        let light_pixels = raster(&light, CanvasTheme::Light);
        let dark_pixels = raster(&dark, CanvasTheme::Dark);
        assert_ne!(light_pixels, dark_pixels);
        // Changing only the raster theme used to leave the prepared molecule
        // unchanged. This also compares under one raster theme to isolate the
        // sphere/bond shading from the raster theme and its cache key.
        assert_ne!(raster(&light, CanvasTheme::Dark), dark_pixels);
        assert!(
            dark_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] > 0)
        );
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
