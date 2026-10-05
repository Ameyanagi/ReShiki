//! Thumbnail, inspector, palette and proposal canvas programs; they render through the same scene path as the paper.

use super::render::{draw_document, draw_document_with_minimum_stroke};
use super::{Camera, Document, World, layered};
use iced::widget::canvas::{self, Action, Geometry, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, mouse};

/// Noninteractive thumbnail rendered from the same molecule and scene as placement.
pub struct TemplateThumbnail<'a>(pub &'a Document);
impl<Message> canvas::Program<Message> for TemplateThumbnail<'_> {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame =
            layered::Frame::new(renderer, bounds.size()).with_canvas(self.0.canvas_theme);
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        let (lo, hi) = reshiki::scene::selection_bounds(self.0, &self.0.all_ids())
            .unwrap_or_else(|| self.0.bounds());
        let camera = Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - (bounds.width * 0.16).min(24.0)) / (hi.x - lo.x).max(60.0))
                .min((bounds.height - (bounds.height * 0.14).min(20.0)) / (hi.y - lo.y).max(50.0))
                .min(0.85),
        };
        draw_document(&mut frame, self.0, camera, bounds);
        frame.finish()
    }
}

/// Owns a temporary diagram for a tool inspector preview.
pub struct DrawingThumbnail(pub Document);
impl<Message> canvas::Program<Message> for DrawingThumbnail {
    type State = ();
    fn draw(
        &self,
        state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        <TemplateThumbnail<'_> as canvas::Program<Message>>::draw(
            &TemplateThumbnail(&self.0),
            state,
            renderer,
            theme,
            bounds,
            cursor,
        )
    }
}

/// The source diagram is a real hit-tested canvas, so anchors identify the
/// exact atom/bond the user picked rather than a nearest compatible substitute.
pub struct TemplateAnchorPreview<'a> {
    pub document: &'a Document,
    pub anchor: reshiki::templates::Anchor,
}
impl TemplateAnchorPreview<'_> {
    pub(super) fn camera(&self, bounds: Rectangle) -> Camera {
        let (lo, hi) = reshiki::scene::selection_bounds(self.document, &self.document.all_ids())
            .unwrap_or_else(|| self.document.bounds());
        Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - 30.) / (hi.x - lo.x).max(60.))
                .min((bounds.height - 30.) / (hi.y - lo.y).max(50.))
                .min(1.2),
        }
    }
    fn hit(&self, p: Point, bounds: Rectangle) -> Option<reshiki::templates::Anchor> {
        let camera = self.camera(bounds);
        let p = camera.world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
        self.document
            .nearest(p, 8. / camera.zoom)
            .map(reshiki::templates::Anchor::Atom)
            .or_else(|| {
                reshiki::editing::nearest_bond(self.document, p, 6. / camera.zoom)
                    .and_then(|i| self.document.bonds.get(i))
                    .map(|b| reshiki::templates::Anchor::Bond(b.a, b.b))
            })
    }
}
impl canvas::Program<reshiki::templates::Anchor> for TemplateAnchorPreview<'_> {
    type State = Option<reshiki::templates::Anchor>;
    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<reshiki::templates::Anchor>> {
        let p = match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => Some(*position),
            _ => cursor.position(),
        };
        let hit = p
            .filter(|p| bounds.contains(*p))
            .and_then(|p| self.hit(p, bounds));
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                hit.map(|anchor| Action::publish(anchor).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if *state != hit {
                    *state = hit;
                    Some(Action::request_redraw())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    fn mouse_interaction(
        &self,
        _: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor
            .position()
            .filter(|p| bounds.contains(*p))
            .and_then(|p| self.hit(p, bounds))
            .is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame =
            layered::Frame::new(renderer, bounds.size()).with_canvas(self.document.canvas_theme);
        let camera = self.camera(bounds);
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        draw_document(&mut frame, self.document, camera, bounds);
        for (anchor, color) in [
            (Some(self.anchor), Color::from_rgb8(17, 126, 108)),
            (*state, Color::from_rgba8(17, 126, 108, 0.45)),
        ] {
            match anchor {
                Some(reshiki::templates::Anchor::Atom(id)) => {
                    if let Some(a) = self.document.atom(id) {
                        frame.stroke(
                            &Path::circle(camera.screen(a.position, bounds), 9.),
                            Stroke::default().with_color(color).with_width(2.),
                        );
                    }
                }
                Some(reshiki::templates::Anchor::Bond(a, b)) => {
                    if let (Some(a), Some(b)) = (self.document.atom(a), self.document.atom(b)) {
                        frame.stroke(
                            &Path::line(
                                camera.screen(a.position, bounds),
                                camera.screen(b.position, bounds),
                            ),
                            Stroke::default().with_color(color).with_width(5.),
                        );
                    }
                }
                _ => {}
            }
        }
        frame.finish()
    }
}

pub struct ArrowPreview {
    pub arrow: reshiki::document::Arrow,
}
impl canvas::Program<crate::app::Message> for ArrowPreview {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let mut doc = Document::default();
        doc.arrows.push(self.arrow.clone());
        let (lo, hi) = self.arrow.bounds();
        let camera = Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - 22.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 18.) / (hi.y - lo.y).max(1.))
                .min(1.4),
        };
        draw_document(&mut frame, &doc, camera, bounds);
        frame.finish()
    }
}

/// The inspector uses the same geometry and phase fills as the drawing/export.
pub struct ScientificPreview(pub reshiki::graphics::Graphic);
impl canvas::Program<crate::app::Message> for ScientificPreview {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let (lo, hi) = self.0.bounds();
        let camera = Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - 24.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 20.) / (hi.y - lo.y).max(1.))
                .min(1.4),
        };
        let doc = Document {
            graphics: vec![self.0.clone()],
            ..Document::default()
        };
        draw_document(&mut frame, &doc, camera, bounds);
        frame.finish()
    }
}

/// Shared, read-only preview for palettes and reviewed drawing proposals.
#[derive(Default)]
pub struct PreviewState(std::cell::RefCell<Option<PreviewCache>>);
struct PreviewCache {
    document: Document,
    size: iced::Size,
    geometry: Vec<<Geometry as iced::advanced::graphics::cache::Cached>::Cache>,
}
pub struct DrawingPreview<'a>(pub &'a Document);
impl canvas::Program<crate::app::Message> for DrawingPreview<'_> {
    type State = PreviewState;
    fn draw(
        &self,
        state: &PreviewState,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        use iced::advanced::graphics::cache::Cached;
        let mut cache = state.0.borrow_mut();
        if let Some(cached) = cache.as_ref()
            && cached.size == bounds.size()
            && &cached.document == self.0
        {
            return cached.geometry.iter().map(Cached::load).collect();
        }
        let mut frame =
            layered::Frame::new(renderer, bounds.size()).with_canvas(self.0.canvas_theme);
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        let (lo, hi) =
            reshiki::scene::selection_bounds(self.0, &self.0.all_ids()).unwrap_or_default();
        let camera = Camera {
            center: World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
            zoom: ((bounds.width - 24.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 24.) / (hi.y - lo.y).max(1.))
                .clamp(0.001, 1.4),
        };
        draw_document_with_minimum_stroke(&mut frame, self.0, camera, bounds, 0.6);
        let geometry: Vec<_> = frame
            .finish()
            .into_iter()
            .map(|g| g.cache(iced::advanced::graphics::cache::Group::unique(), None))
            .collect();
        let result = geometry.iter().map(Cached::load).collect();
        *cache = Some(PreviewCache {
            document: self.0.clone(),
            size: bounds.size(),
            geometry,
        });
        result
    }
}

/// Palette strokes remain legible even when a large structure is fitted into a tile.
pub struct PalettePreview(pub Document);
impl canvas::Program<crate::app::Message> for PalettePreview {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let (lo, hi) =
            reshiki::scene::selection_bounds(&self.0, &self.0.all_ids()).unwrap_or_default();
        let camera = Camera {
            center: World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
            zoom: ((bounds.width - 14.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 14.) / (hi.y - lo.y).max(1.))
                .clamp(0.001, 1.4),
        };
        draw_document_with_minimum_stroke(&mut frame, &self.0, camera, bounds, 1.5);
        frame.finish()
    }
}

pub struct OwnedDrawingPreview(pub Document);
impl canvas::Program<crate::app::Message> for OwnedDrawingPreview {
    type State = PreviewState;
    fn draw(
        &self,
        state: &PreviewState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        DrawingPreview(&self.0).draw(state, renderer, theme, bounds, cursor)
    }
}
