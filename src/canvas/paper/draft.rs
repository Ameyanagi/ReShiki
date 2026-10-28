//! Previews of drags that edit the draft document directly (tilts, pointer edits, transforms and moves) or select a region.

use super::Draft;
use crate::canvas::hit::region_selection;
use crate::canvas::snapping::arrow_endpoint;
use crate::canvas::{Gesture, MoleculeCanvas, State, Tool, World, models3d, pick, tilt};
use iced::{Point, Rectangle};
use reshiki::arrows::ArrowHandle;
use reshiki::graphics::{Graphic, GraphicKind};
use std::borrow::Cow;

impl MoleculeCanvas<'_> {
    pub(super) fn preview_tilt(&self, draft: &mut Draft<'_>, state: &State, bounds: Rectangle) {
        let Draft {
            preview,
            ring_selection,
            template_notice,
            ..
        } = draft;
        if let (Some(Gesture::Tilt(drag)), Some(p)) = (&state.gesture, state.cursor)
            && self.tool == Tool::Tilt
        {
            let (x, y) = drag.angles(
                Point::new(p.x - bounds.x, p.y - bounds.y),
                state.modifiers.shift(),
            );
            tilt::apply(preview.to_mut(), &drag.ids, x, y);
            *ring_selection = Some(drag.ids.clone());
            let (x, y) = (tilt::wrap(x), tilt::wrap(y));
            *template_notice = Some((
                format!("3D tilt · X {x:+.0}° · Y {y:+.0}° · Shift snaps · Escape cancels"),
                true,
            ));
        }
    }

    /// A 3D model turned by the drag, or morphing in from its drawing.
    pub(super) fn preview_model(&self, draft: &mut Draft<'_>, state: &State, bounds: Rectangle) {
        let Draft {
            preview,
            template_notice,
            ..
        } = draft;
        if let (Some(Gesture::Turn(turn)), Some(p)) = (&state.gesture, state.cursor) {
            let point = Point::new(p.x - bounds.x, p.y - bounds.y);
            models3d::preview_turn(preview.to_mut(), turn, point, state.modifiers.shift());
            *template_notice = Some((
                "Drag to rotate · Shift-drag to roll · Double-click to reset".into(),
                true,
            ));
        } else if state.gesture.is_none()
            && self.tool.selects()
            && state.cursor.is_some_and(|p| bounds.contains(p))
            && reshiki::models3d::shows_3d(self.doc, self.selected)
            && reshiki::models3d::whole_molecule(self.doc, self.selected)
                .is_some_and(|atoms| reshiki::models3d::only_shown(self.doc, &atoms))
        {
            *template_notice = Some((
                "Drag to rotate · Shift-drag to roll · Double-click to reset".into(),
                true,
            ));
        }
        // A model turned by the drag or the tilt draws quickly until it is
        // let go.
        if let Cow::Owned(doc) = preview {
            for (model, before) in doc.models.iter_mut().zip(&self.doc.models) {
                model.turning |= model.rotation != before.rotation;
            }
        }
        if let Some((id, t)) = self.morph(iced::time::Instant::now())
            && let Some(model) = preview.to_mut().models.iter_mut().find(|m| m.contains(id))
        {
            model.morph = Some(t);
        }
    }

    pub(super) fn preview_pointer_edits(
        &self,
        draft: &mut Draft<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            smart,
            template_notice,
            ..
        } = draft;
        if let Some(p) = state.cursor {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            if let (Some(Gesture::Graphic { start, hydrogens }), Tool::Graphic(kind)) =
                (&state.gesture, self.tool)
            {
                if kind == GraphicKind::Path {
                    if bounds.contains(p)
                        && let Some(stroke) = self.pen_stroke(*start, end)
                        && let Ok((doc, id, _)) = crate::canvas::pen::apply(
                            self.doc,
                            self.selected,
                            self.graphics.style,
                            stroke,
                        )
                    {
                        *preview = std::borrow::Cow::Owned(doc);
                        *ring_selection = Some(vec![id]);
                    }
                } else if matches!(kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
                    let drawing = self.scientific_drawing(kind, state.modifiers);
                    if matches!(kind, GraphicKind::Orbital(_)) {
                        let target = drawing.orbital_target(
                            self.doc,
                            *start,
                            self.camera.world_len(pick::PICK_RADIUS_PX),
                        );
                        *template_notice = Some((
                            target.map_or_else(
                                || "Orbital · Free node placement · Shift snaps angle".into(),
                                |id| {
                                    let label = self
                                        .doc
                                        .atom(id)
                                        .map_or("atom", |atom| atom.element.as_str());
                                    format!("Orbital · Node snapped to {label} · Option/Alt frees")
                                },
                            ),
                            true,
                        ));
                    }
                    let mut hydrogens = hydrogens.borrow_mut();
                    if hydrogens.0 != self.graphics.revision {
                        *hydrogens = (self.graphics.revision, Default::default());
                    }
                    if let Ok(id) = drawing.place_with(
                        preview.to_mut(),
                        *start,
                        end,
                        self.graphic_constrain(kind, state.modifiers),
                        self.camera.world_len(pick::PICK_RADIUS_PX),
                        &hydrogens.1,
                    ) {
                        *ring_selection = Some(vec![id]);
                    }
                } else {
                    let id = preview.next_id();
                    preview.to_mut().graphics.push(
                        Graphic::dragged(
                            id,
                            kind,
                            *start,
                            end,
                            self.graphics.style.clone(),
                            self.graphics.bracket_sides,
                            state.modifiers.shift() || self.graphics.constrain,
                        )
                        .with_arc(self.graphics.arc),
                    );
                    *ring_selection = Some(vec![id]);
                }
            }
            if let Some(Gesture::AtomMark { id, index, start }) = &state.gesture
                && start.distance(end) >= self.camera.world_len(pick::DRAG_SLOP_PX)
                && let Some(a) = preview.atom(*id)
                && let Some(mark) = a.marks.get(*index)
            {
                let free = state.modifiers.alt() || state.modifiers.shift();
                let placed = reshiki::scientific::orbit_mark(preview, a, mark, end, free);
                let degrees = (placed.offset.y.atan2(placed.offset.x).to_degrees().round() as i32)
                    .rem_euclid(360);
                let turn = if matches!(
                    placed.kind,
                    reshiki::scientific::MarkKind::LonePair
                        | reshiki::scientific::MarkKind::LonePairBar
                ) {
                    "dots tangent"
                } else {
                    "upright"
                };
                *template_notice = Some((
                    if free {
                        format!("Free · {degrees}° · {turn}")
                    } else {
                        format!("Orbit · {degrees}° · {turn} · Option/Alt or Shift frees")
                    },
                    true,
                ));
                if let Some(mark) = preview
                    .to_mut()
                    .atom_mut(*id)
                    .and_then(|a| a.marks.get_mut(*index))
                {
                    *mark = placed;
                }
            }
            if let Some(Gesture::ArrowHandle { id, index }) = &state.gesture
                && let Some(arrow) = self.doc.arrows.iter().find(|a| a.id == *id)
            {
                let end = if ArrowHandle::from_index(*index).is_some_and(ArrowHandle::is_endpoint) {
                    let (end, guides) = self.arrow_end(state, arrow, *index, end, bounds);
                    *smart = guides;
                    end
                } else {
                    end
                };
                reshiki::arrow_anchors::edit_handle(preview.to_mut(), *id, *index, end);
            }
            if let Some(Gesture::Draw { start, .. }) = &state.gesture
                && self.tool == Tool::Arrow
                && start.distance(end) >= self.camera.world_len(pick::DRAG_SLOP_PX)
            {
                let end = arrow_endpoint(
                    *start,
                    end,
                    self.bond_drawing.fixed_angles && !state.modifiers.alt(),
                );
                let id = preview.next_id();
                preview.to_mut().arrows.push(reshiki::document::Arrow::new(
                    id,
                    *start,
                    end,
                    self.arrows.preset,
                    self.arrows.style.clone(),
                ));
            }
            if let Some(Gesture::AtomIndicator { owner }) = &state.gesture
                && let Some(anchor) = owner.anchor(preview)
            {
                owner.set_offset(
                    preview.to_mut(),
                    Some(World::new(end.x - anchor.x, end.y - anchor.y)),
                );
            }
            if let Some(Gesture::GraphicPoint { id, index }) = &state.gesture
                && let Some(g) = preview.to_mut().graphics.iter_mut().find(|g| g.id == *id)
            {
                let _ = g.edit_point(*index, end);
            }
            if let Some(Gesture::PathPoint(drag)) = &state.gesture
                && let Some(g) = preview
                    .to_mut()
                    .graphics
                    .iter_mut()
                    .find(|g| g.id == drag.id)
            {
                let _ = g.edit_point(drag.index, drag.target(end));
            }
        }
    }

    pub(super) fn preview_transform(
        &self,
        draft: &mut Draft<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
        let Draft {
            preview,
            ring_selection,
            ..
        } = draft;
        if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor) {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            drag.apply(preview.to_mut(), end, state.modifiers.shift());
            if drag.resizes_models() {
                // Even an atom-only subset in Both resizes its entire model.
                // Follow the captions of those actual frames, as release does.
                let resized: Vec<_> = preview
                    .models
                    .iter()
                    .filter(|model| {
                        model.display != reshiki::models3d::Display::TwoD
                            && drag.ids.iter().any(|id| model.contains(*id))
                    })
                    .flat_map(|model| model.linked_ids(preview))
                    .collect();
                reshiki::molecule_names::preview_layout(preview.to_mut(), &resized);
            }
            *ring_selection = Some(drag.ids.clone());
        }
    }

    pub(super) fn preview_move(&self, draft: &mut Draft<'_>, state: &State, bounds: Rectangle) {
        let Draft {
            preview,
            ring_selection,
            translation,
            smart,
            ..
        } = draft;
        if let (Some(Gesture::StretchBond { start, plan }), Some(p)) =
            (&state.gesture, state.cursor)
        {
            let p = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            // The press-time plan measures the drag, as release does. The
            // branch moves with the plan resolved on the current drawing, as
            // the commit resolves it, so an Undo during the drag still agrees.
            let length = plan.dragged_length(World::new(p.x - start.x, p.y - start.y));
            let current = state
                .scene
                .borrow_mut()
                .stretch(self.doc, plan.fixed, plan.moving);
            if let Some(current) = current
                && let Ok(candidate) = current.translated(self.doc, length)
            {
                *preview = Cow::Owned(candidate);
                *ring_selection = Some(plan.ids.clone());
            }
            return;
        }
        if let (Some(Gesture::Move { start, ids, .. }), Some(p)) = (&state.gesture, state.cursor) {
            let p = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (delta, guides) =
                self.move_delta(state, ids, World::new(p.x - start.x, p.y - start.y), bounds);
            if start.distance(p) < self.camera.world_len(pick::CLICK_SLOP_PX) {
                *ring_selection = Some(ids.clone());
            } else if self.copies(state.modifiers) {
                let part = state.scene.borrow_mut().copy(self.doc, ids);
                *ring_selection = Some(reshiki::editing::append(preview.to_mut(), &part, delta));
                *smart = guides;
            } else if state.scene.borrow_mut().whole_document(self.doc, ids) {
                // Moving every object cannot change their relative geometry,
                // chemical labels, crossing gaps or ring attachment targets.
                preview.to_mut().translate(ids, delta.x, delta.y);
                *ring_selection = Some(ids.clone());
                *translation = Some(delta);
            } else if let Some(snapped) =
                reshiki::editing::snap_ring(preview.to_mut(), ids, delta, 14.0 / self.camera.zoom)
            {
                // Fusing onto a ring wins over the guides, on release too.
                *ring_selection = Some(snapped);
            } else {
                preview.to_mut().translate(ids, delta.x, delta.y);
                *ring_selection = Some(ids.clone());
                *smart = guides;
            }
        }
    }

    pub(super) fn preview_region(&self, draft: &mut Draft<'_>, state: &State, bounds: Rectangle) {
        let Draft { ring_selection, .. } = draft;
        if let Some(p) = state.cursor {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let polygon = match &state.gesture {
                Some(Gesture::Select { start }) => Some(vec![
                    *start,
                    World::new(end.x, start.y),
                    end,
                    World::new(start.x, end.y),
                ]),
                Some(Gesture::Lasso { points }) => {
                    let mut p = points.clone();
                    p.push(end);
                    Some(p)
                }
                _ => None,
            };
            if let Some(polygon) = polygon {
                *ring_selection = Some(region_selection(
                    self.doc,
                    self.selected,
                    &polygon,
                    state.modifiers,
                ));
            }
        }
    }
}

#[cfg(test)]
mod stretch_tests;
