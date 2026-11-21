//! The drawing paper, layer by layer: background, the document as the active gesture would leave it, the document, then editing overlays.

mod bond_text;
mod draft;
mod fragment;
pub(super) use fragment::Remainder;
mod geometry;
pub(super) use geometry::SceneGeometryCache;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod orbital_tests;
mod overlays;
#[cfg(test)]
mod review_tests;
pub(super) use overlays::OrbitGuideCache;
mod placement;
#[cfg(test)]
mod preview_tests;

use super::render::{draw_document, draw_primitives};
use super::smart_guides::Guide;
use super::{
    Camera, MoleculeCanvas, State, World, layered, markers, models3d, pages, smart_guides,
};
use iced::{Color, Point, Rectangle, mouse};
use reshiki::document::Document;
use std::borrow::Cow;

/// Space above the paper's bottom edge occupied by the hint strip and its gap.
pub(crate) const HINT_STRIP_HEIGHT_PX: f32 = 34.;

/// The document as the in-progress gesture would leave it, plus its transient decorations.
struct Draft<'d> {
    preview: Cow<'d, Document>,
    bond_preview: Option<Document>,
    ring_selection: Option<Vec<u64>>,
    chain_badge: Option<(World, String, bool)>,
    template_notice: Option<(String, bool)>,
    rejection: Option<reshiki::editing::RingRejection>,
    translation: Option<World>,
    smart: Vec<Guide>,
    /// Where a part would join a catalytic cycle if dropped now.
    cycle_drop: Option<reshiki::cycles::Target>,
    /// The joint a moving equipment piece would snap to.
    seat_hint: Option<SeatHint>,
}

/// A joint a drag or placement snaps to: its mouth, its radius in world
/// units, whether it seats as is, and what releasing does.
struct SeatHint {
    at: World,
    radius: f32,
    fits: bool,
    text: String,
}

impl SeatHint {
    fn new(doc: &Document, snap: &reshiki::equipment::solve::Snap) -> Self {
        use reshiki::equipment::solve::Offer;
        let p = snap.at;
        Self {
            at: World::new(p[0] as f32, p[1] as f32),
            radius: (snap.target.diameter() / 2.) as f32,
            fits: matches!(snap.offer, Offer::Seat | Offer::Clip),
            text: crate::app::equipment::snap_text(doc, snap),
        }
    }
}

struct SelectionPreview<'a> {
    document: &'a Document,
    ids: &'a [u64],
    translation: Option<World>,
    ink: &'a [(World, World)],
}

impl<'d> Draft<'d> {
    fn new(base: &'d Document) -> Self {
        Self {
            preview: Cow::Borrowed(base),
            bond_preview: None,
            ring_selection: None,
            chain_badge: None,
            template_notice: None,
            rejection: None,
            translation: None,
            smart: Vec::new(),
            cycle_drop: None,
            seat_hint: None,
        }
    }
}

fn reconcile_preview(doc: &mut Document, base: &Document) {
    reshiki::arrow_anchors::reconcile(doc);
    reshiki::arrow_labels::reconcile(doc, base);
    reshiki::shape_text::reconcile_changes(doc, base);
}

impl MoleculeCanvas<'_> {
    #[cfg(test)]
    pub(super) fn pointer_preview_document(&self, state: &State, bounds: Rectangle) -> Document {
        let mut draft = Draft::new(self.doc);
        self.preview_pointer_edits(&mut draft, state, bounds);
        let mut doc = draft.preview.into_owned();
        reconcile_preview(&mut doc, self.doc);
        doc
    }

    /// The drawing copy an Atom tool drag shows.
    #[cfg(test)]
    pub(super) fn bonded_atom_preview_document(
        &self,
        state: &State,
        bounds: Rectangle,
    ) -> Document {
        let mut draft = Draft::new(self.doc);
        self.preview_bonded_atom(&mut draft, state, bounds);
        draft.preview.into_owned()
    }

    /// The drawing copy a model turn or morph shows.
    #[cfg(test)]
    pub(super) fn model_preview_document(&self, state: &State, bounds: Rectangle) -> Document {
        let mut draft = Draft::new(self.doc);
        self.preview_model(&mut draft, state, bounds);
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
        let base = self.joining.map(|(j, _)| &j.base).unwrap_or(self.doc);
        let mut draft = Draft::new(base);
        self.preview_tilt(&mut draft, state, bounds);
        self.preview_model(&mut draft, state, bounds);
        self.preview_chain(&mut draft, frame, state, bounds);
        self.preview_pointer_edits(&mut draft, state, bounds);
        self.preview_transform(&mut draft, state, bounds);
        self.preview_ring(&mut draft, state, bounds);
        self.preview_ring_preset(&mut draft, state, bounds);
        self.preview_template(&mut draft, frame, state, bounds);
        self.preview_piece(&mut draft, state, bounds);
        self.preview_icon(&mut draft, state, bounds);
        self.preview_connect(&mut draft, state);
        self.preview_piece_handle(&mut draft, state, bounds);
        self.preview_move(&mut draft, state, bounds);
        self.preview_region(&mut draft, state, bounds);
        self.preview_delocalized_ring(&mut draft, state, bounds);
        self.preview_bonded_atom(&mut draft, state, bounds);
        self.preview_bond(&mut draft, frame, state, bounds);
        if let Cow::Owned(doc) = &mut draft.preview {
            reconcile_preview(doc, base);
        }
        let Draft {
            mut preview,
            bond_preview,
            ring_selection,
            chain_badge,
            template_notice,
            rejection,
            translation,
            smart,
            cycle_drop,
            seat_hint,
        } = draft;
        if let Some(id) = self.hidden_annotation {
            preview.to_mut().annotations.retain(|a| a.id != id);
        }
        let selected = ring_selection.as_deref().unwrap_or(self.selected);
        let nmr_ids = self
            .nmr
            .map(|context| context.marker_ids())
            .unwrap_or_default();
        let cached_camera = (self.hidden_annotation.is_none()
            && (translation.is_some()
                || matches!(&preview, Cow::Borrowed(doc) if std::ptr::eq(*doc, self.doc))
                || *preview == *self.doc))
            .then(|| {
                let delta = translation.unwrap_or_default();
                Camera {
                    center: self.camera.center.offset(-delta.x, -delta.y),
                    ..self.camera
                }
            });
        models3d::begin(&preview);
        let mut rotation_ink = Vec::new();
        let rotating = matches!(&state.gesture, Some(super::Gesture::Transform(drag)) if matches!(drag.handle, super::Handle::Rotate));
        let fragment = (cached_camera.is_none() && !rotating)
            .then(|| {
                fragment::Fragment::new(
                    &mut state.scene.borrow_mut(),
                    self.doc,
                    &preview,
                    self.camera,
                )
            })
            .flatten();
        if let Some(fragment) = fragment {
            if !self.reviewing_pair {
                markers::Markers::new(&preview, selected).draw_with_nmr(
                    frame,
                    self.camera,
                    bounds,
                    true,
                    &nmr_ids,
                );
            }
            // Its markers are empty: the preview's markers lie below both parts.
            state.fragment.borrow_mut().draw(
                frame,
                (
                    fragment.remainder.markers.clone(),
                    fragment.remainder.scene.clone(),
                ),
                self.camera,
                bounds,
                &[],
                self.reviewing_pair,
            );
            draw_primitives(frame, &fragment.scene, self.camera, bounds, 0.);
        } else if let Some(camera) = cached_camera {
            let (markers, scene) = state.scene.borrow_mut().render(self.doc, selected);
            if rotating {
                rotation_ink = reshiki::scene::label_ink_boxes(&scene);
            }
            state.geometry.borrow_mut().draw(
                frame,
                (markers, scene),
                camera,
                bounds,
                &nmr_ids,
                self.reviewing_pair,
            );
        } else {
            if !self.reviewing_pair {
                markers::Markers::new(&preview, selected).draw_with_nmr(
                    frame,
                    self.camera,
                    bounds,
                    true,
                    &nmr_ids,
                );
            }
            if rotating {
                let scene = reshiki::scene::primitives(&preview);
                rotation_ink = reshiki::scene::label_ink_boxes(&scene);
                draw_primitives(frame, &scene, self.camera, bounds, 0.);
            } else {
                draw_document(frame, &preview, self.camera, bounds);
            }
        }
        if let Some(drawing) = bond_preview {
            draw_document(frame, &drawing, self.camera, bounds);
        }
        models3d::end();
        let nmr_cache = (cached_camera.is_some() && translation.is_none()).then_some(&state.scene);
        self.draw_nmr_assignments(frame, &preview, selected, bounds, nmr_cache);
        self.draw_editor_markers(frame, &preview, bounds);
        self.draw_cycle_drop(frame, cycle_drop.as_ref(), bounds);
        self.draw_seat_hint(frame, seat_hint.as_ref(), bounds);
        self.draw_pull_out(frame, state, bounds);
        self.draw_orbital_target(frame, state, bounds);
        self.draw_mark_orbit(frame, state, &preview, bounds);
        self.draw_arrow_handles(frame, &preview, selected, bounds);
        self.draw_piece_handles(frame, &preview, selected, state, bounds);
        self.draw_arrow_targets(frame, state, bounds, cursor);
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
            SelectionPreview {
                document: &preview,
                ids: selected,
                translation: cached_camera.map(|_| translation.unwrap_or_default()),
                ink: &rotation_ink,
            },
        );
        self.draw_tilt_bounds(frame, &preview, selected, bounds);
        self.draw_model_generation(frame, bounds, iced::time::Instant::now());
        // Above the selection, so that gap labels stay legible.
        smart_guides::draw(
            frame,
            &smart,
            self.camera,
            bounds,
            self.guides.unit,
            self.doc.canvas_theme.is_dark(),
        );
        smart_guides::draw_arrow_label_gaps(
            frame,
            &preview,
            selected,
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
