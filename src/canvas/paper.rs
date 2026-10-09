//! The drawing paper, layer by layer: background, the document as the active gesture would leave it, the document, then editing overlays.

mod draft;
mod overlays;
mod placement;

use super::render::{draw_document, draw_primitives};
use super::smart_guides::Guide;
use super::{Camera, MoleculeCanvas, State, World, layered, markers, pages, smart_guides};
use iced::{Color, Point, Rectangle, mouse};
use reshiki::document::Document;
use std::borrow::Cow;

/// The document as the in-progress gesture would leave it, plus its transient decorations.
struct Draft<'d> {
    preview: Cow<'d, Document>,
    ring_selection: Option<Vec<u64>>,
    chain_badge: Option<(World, String, bool)>,
    template_notice: Option<(String, bool)>,
    rejection: Option<reshiki::editing::RingRejection>,
    translation: Option<World>,
    smart: Vec<Guide>,
}

impl<'d> Draft<'d> {
    fn new(base: &'d Document) -> Self {
        Self {
            preview: Cow::Borrowed(base),
            ring_selection: None,
            chain_badge: None,
            template_notice: None,
            rejection: None,
            translation: None,
            smart: Vec::new(),
        }
    }
}

impl MoleculeCanvas<'_> {
    #[cfg(test)]
    pub(super) fn pointer_preview_document(&self, state: &State, bounds: Rectangle) -> Document {
        let mut draft = Draft::new(self.doc);
        self.preview_pointer_edits(&mut draft, state, bounds);
        draft.preview.into_owned()
    }

    pub(super) fn draw_paper(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) {
        self.draw_background(frame, state, bounds);
        let mut draft = Draft::new(self.joining.map(|(j, _)| &j.base).unwrap_or(self.doc));
        self.preview_tilt(&mut draft, state, bounds);
        self.preview_chain(&mut draft, frame, state, bounds);
        self.preview_pointer_edits(&mut draft, state, bounds);
        self.preview_transform(&mut draft, state, bounds);
        self.preview_ring(&mut draft, state, bounds);
        self.preview_ring_preset(&mut draft, state, bounds);
        self.preview_template(&mut draft, frame, state, bounds);
        self.preview_move(&mut draft, state, bounds);
        self.preview_region(&mut draft, state, bounds);
        self.preview_delocalized_ring(&mut draft, state, bounds);
        self.preview_bonded_atom(&mut draft, state, bounds);
        self.preview_bond(&mut draft, frame, state, bounds);
        let Draft {
            mut preview,
            ring_selection,
            chain_badge,
            template_notice,
            rejection,
            translation,
            smart,
        } = draft;
        if let Some(id) = self.hidden_annotation {
            preview.to_mut().annotations.retain(|a| a.id != id);
        }
        let selected = ring_selection.as_deref().unwrap_or(self.selected);
        let cached_camera = (self.hidden_annotation.is_none()
            && (translation.is_some() || *preview == *self.doc))
            .then(|| {
                let delta = translation.unwrap_or_default();
                Camera {
                    center: self.camera.center.offset(-delta.x, -delta.y),
                    ..self.camera
                }
            });
        if let Some(camera) = cached_camera {
            let (markers, scene) = state.scene.borrow_mut().render(self.doc, selected);
            markers.draw(frame, camera, bounds, true);
            draw_primitives(frame, &scene, camera, bounds, 0.);
        } else {
            markers::Markers::new(&preview, selected).draw(frame, self.camera, bounds, true);
            draw_document(frame, &preview, self.camera, bounds);
        }
        self.draw_editor_markers(frame, &preview, bounds);
        self.draw_arrow_handles(frame, &preview, selected, bounds);
        self.draw_notices(
            frame,
            state,
            bounds,
            template_notice,
            chain_badge,
            rejection,
        );
        self.draw_edit_points(frame, &preview, selected, bounds);
        if self
            .draw_gesture_overlay(frame, state, bounds, cursor)
            .is_break()
        {
            return;
        }
        self.draw_erase_cursor(frame, state, bounds, cursor);
        self.draw_selection(
            frame,
            state,
            bounds,
            &preview,
            selected,
            cached_camera.map(|_| translation.unwrap_or_default()),
        );
        self.draw_tilt_bounds(frame, &preview, selected, bounds);
        // Above the selection, so that gap labels stay legible.
        smart_guides::draw(
            frame,
            &smart,
            self.camera,
            bounds,
            self.guides.unit,
            self.doc.canvas_theme.is_dark(),
        );
    }

    fn draw_background(&self, frame: &mut layered::Frame<'_>, state: &State, bounds: Rectangle) {
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        if let Some(layout) = &self.doc.page_layout {
            pages::draw(frame, layout, self.camera, bounds);
        }
        if self.grid {
            state.grid.borrow_mut().draw(
                frame,
                self.camera,
                bounds,
                self.doc.drawing_style.bond_length_world,
                self.doc.canvas_theme.is_dark(),
            );
        }
        self.guides.draw_crosshair(
            frame,
            self.camera,
            state
                .cursor
                .filter(|p| bounds.contains(*p))
                .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y)),
        );
    }
}
