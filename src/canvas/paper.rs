//! The drawing paper, layer by layer: background, the document as the active gesture would leave it, the document, then editing overlays.

mod bond_text;
mod draft;
mod fragment;
#[cfg(test)]
pub(super) use fragment::SPLIT;
pub(super) use fragment::ScenePart;
mod geometry;
pub(super) use geometry::SceneGeometryCache;
mod memo;
pub(super) use memo::Preview as PreviewMemo;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod orbital_tests;
mod overlays;
#[cfg(test)]
mod review_tests;
pub(super) use overlays::OrbitGuideCache;
pub(crate) use overlays::{BondChangeKind, BondChangeMark, draw_bond_changes};
mod placement;
#[cfg(test)]
mod preview_tests;

use super::render::{draw_document, draw_primitives};
use super::smart_guides::Guide;
use super::{
    Camera, MoleculeCanvas, State, Tool, World, layered, markers, models3d, pages, smart_guides,
};
use fragment::Fragment;
use iced::{Color, Point, Rectangle, mouse};
use reshiki::document::Document;
use std::borrow::Cow;
use std::cell::OnceCell;

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
    move_delta: Option<World>,
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
            move_delta: None,
            smart: Vec::new(),
            cycle_drop: None,
            seat_hint: None,
        }
    }

    /// The memo only takes ownership of a draft that already owns its drawing.
    fn freeze(self) -> Draft<'static> {
        Draft {
            preview: Cow::Owned(self.preview.into_owned()),
            bond_preview: self.bond_preview,
            ring_selection: self.ring_selection,
            chain_badge: self.chain_badge,
            template_notice: self.template_notice,
            rejection: self.rejection,
            translation: self.translation,
            move_delta: self.move_delta,
            smart: self.smart,
            cycle_drop: self.cycle_drop,
            seat_hint: self.seat_hint,
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

    fn build_preview<'d>(
        &self,
        base: &'d Document,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
    ) -> Draft<'d> {
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
        if let Some(id) = self.hidden_annotation {
            draft.preview.to_mut().annotations.retain(|a| a.id != id);
        }
        draft
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
        let inputs = memo::Inputs::new(self, state, bounds);
        // The test reference draws each complete preview from scratch.
        #[cfg(test)]
        let inputs = inputs.filter(|_| fragment::SPLIT.with(std::cell::Cell::get));
        let previous = inputs.as_ref().and_then(|inputs| {
            state
                .scene
                .borrow()
                .preview
                .as_ref()
                .filter(|record| record.matches(inputs, self.piece))
                .cloned()
        });
        let reuse = previous.is_some();
        let (memo, live) = if let Some(record) = previous {
            (Some(record), None)
        } else {
            let draft = self.build_preview(base, frame, state, bounds);
            // The rejected template and bond paths can draw directly into the
            // frame. Those paths leave the draft borrowed and stay live.
            match inputs {
                Some(inputs) if matches!(draft.preview, Cow::Owned(_)) => (
                    Some(PreviewMemo::new(inputs, self.piece, draft.freeze())),
                    None,
                ),
                _ => (None, Some(draft)),
            }
        };
        state.scene.borrow_mut().preview = memo.clone();
        let draft = memo.as_ref().map(|record| &record.draft).or(live.as_ref());
        let Some(draft) = draft else { return };
        let Draft {
            preview,
            bond_preview,
            ring_selection,
            chain_badge,
            template_notice,
            rejection,
            translation,
            move_delta,
            smart,
            cycle_drop,
            seat_hint,
        } = draft;
        let selected = ring_selection.as_deref().unwrap_or(self.selected);
        let nmr_ids = self
            .nmr
            .map(|context| context.marker_ids())
            .unwrap_or_default();
        let cached_camera = (self.hidden_annotation.is_none()
            && (translation.is_some()
                || matches!(preview, Cow::Borrowed(doc) if std::ptr::eq(*doc, self.doc))
                || memo.as_ref().map_or_else(
                    || preview.as_ref() == self.doc,
                    |record| {
                        *record
                            .matches_document
                            .get_or_init(|| preview.as_ref() == self.doc)
                    },
                )))
        .then(|| {
            let delta = translation.unwrap_or_default();
            Camera {
                center: self.camera.center.offset(-delta.x, -delta.y),
                ..self.camera
            }
        });
        let preview = preview.as_ref();
        models3d::begin(preview);
        let mut rotation_ink = Vec::new();
        let rotating = matches!(&state.gesture, Some(super::Gesture::Transform(drag)) if matches!(drag.handle, super::Handle::Rotate));
        let live_fragment = OnceCell::new();
        let fragment = memo
            .as_ref()
            .map_or(&live_fragment, |record| &record.fragment)
            .get_or_init(|| {
                (cached_camera.is_none() && !rotating)
                    .then(|| {
                        Fragment::new(
                            &mut state.scene.borrow_mut(),
                            self.doc,
                            preview,
                            self.camera,
                            *move_delta,
                        )
                    })
                    .flatten()
            })
            .as_ref();
        if let Some(fragment) = fragment {
            if !self.reviewing_pair {
                let live_markers;
                let markers = if let Some(record) = &memo {
                    record.markers(selected).as_ref()
                } else {
                    live_markers = markers::Markers::new(preview, selected);
                    &live_markers
                };
                markers.draw_with_nmr(frame, self.camera, bounds, true, &nmr_ids);
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
            if let Some(delta) = fragment.shift {
                let camera = Camera {
                    center: self.camera.center.offset(-delta.x, -delta.y),
                    ..self.camera
                };
                state
                    .scene
                    .borrow_mut()
                    .moved_geometry
                    .get_or_insert_with(SceneGeometryCache::with_pan_margin)
                    .draw(
                        frame,
                        (fragment.remainder.markers.clone(), fragment.scene.clone()),
                        camera,
                        bounds,
                        &[],
                        self.reviewing_pair,
                    );
            } else if let Some(record) = &memo
                && self.tool == Tool::Equipment
            {
                record.geometry.borrow_mut().draw(
                    frame,
                    (fragment.remainder.markers.clone(), fragment.scene.clone()),
                    self.camera,
                    bounds,
                    &[],
                    self.reviewing_pair,
                );
            } else {
                draw_primitives(frame, &fragment.scene, self.camera, bounds, 0.);
            }
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
        } else if let Some(record) = &memo
            && reuse
        {
            // Render a complete/overlapping preview into the enclosing frame.
            // Rendering it in a separate layer changes coverage at clip edges.
            if !self.reviewing_pair {
                record
                    .markers(selected)
                    .draw_with_nmr(frame, self.camera, bounds, true, &nmr_ids);
            }
            let scene = record
                .whole
                .get_or_init(|| reshiki::scene::primitives(preview).into());
            draw_primitives(frame, scene, self.camera, bounds, 0.);
        } else {
            if !self.reviewing_pair {
                markers::Markers::new(preview, selected).draw_with_nmr(
                    frame,
                    self.camera,
                    bounds,
                    true,
                    &nmr_ids,
                );
            }
            if rotating {
                let scene = reshiki::scene::primitives(preview);
                rotation_ink = reshiki::scene::label_ink_boxes(&scene);
                draw_primitives(frame, &scene, self.camera, bounds, 0.);
            } else {
                draw_document(frame, preview, self.camera, bounds);
            }
        }
        if let Some(drawing) = bond_preview {
            draw_document(frame, drawing, self.camera, bounds);
        }
        models3d::end();
        let nmr_cache = (cached_camera.is_some() && translation.is_none()).then_some(&state.scene);
        self.draw_nmr_assignments(frame, preview, selected, bounds, nmr_cache);
        self.draw_outdated_models(frame, preview, bounds);
        draw_bond_changes(frame, preview, &self.bond_changes, self.camera, bounds);
        self.draw_editor_markers(frame, preview, bounds);
        self.draw_cycle_drop(frame, cycle_drop.as_ref(), bounds);
        self.draw_seat_hint(frame, seat_hint.as_ref(), bounds);
        self.draw_pull_out(frame, state, bounds);
        self.draw_orbital_target(frame, state, bounds);
        self.draw_mark_orbit(frame, state, preview, bounds);
        self.draw_arrow_handles(frame, preview, selected, bounds);
        self.draw_piece_handles(frame, preview, selected, state, bounds);
        self.draw_tube_bend_handle(frame, state, bounds);
        self.draw_arrow_targets(frame, state, bounds, cursor);
        self.draw_notices(
            frame,
            state,
            bounds,
            template_notice,
            chain_badge,
            rejection,
        );
        self.draw_edit_points(frame, preview, selected, bounds);
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
                document: preview,
                ids: selected,
                translation: cached_camera.map(|_| translation.unwrap_or_default()),
                ink: &rotation_ink,
            },
        );
        self.draw_tilt_bounds(frame, preview, selected, bounds);
        self.draw_model_generation(frame, bounds, iced::time::Instant::now());
        self.draw_result_label(frame, bounds);
        // Above the selection, so that gap labels stay legible.
        smart_guides::draw(
            frame,
            smart,
            self.camera,
            bounds,
            self.guides.unit,
            self.doc.canvas_theme.is_dark(),
        );
        smart_guides::draw_arrow_label_gaps(
            frame,
            preview,
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
