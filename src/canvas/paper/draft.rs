//! Previews of drags that edit the draft document directly (tilts, pointer edits, transforms and moves) or select a region.

use super::Draft;
use crate::canvas::hit::region_selection;
use crate::canvas::snapping::arrow_endpoint;
use crate::canvas::{Gesture, MoleculeCanvas, State, Tool, World, tilt};
use iced::{Point, Rectangle};
use reshiki::graphics::{Graphic, GraphicKind};

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
            *template_notice = Some((
                format!("3D tilt · X {x:+.0}° · Y {y:+.0}° · Shift snaps · Escape cancels"),
                true,
            ));
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
            ..
        } = draft;
        if let Some(p) = state.cursor {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            if let (Some(Gesture::Graphic { start }), Tool::Graphic(kind)) =
                (&state.gesture, self.tool)
            {
                if kind == GraphicKind::Path {
                    if bounds.contains(p)
                        && let Some(stroke) = self.pen_stroke(*start, end)
                        && let Ok((doc, id, _)) = crate::canvas::pen::apply(
                            self.doc,
                            self.selected,
                            self.graphic_style,
                            stroke,
                        )
                    {
                        *preview = std::borrow::Cow::Owned(doc);
                        *ring_selection = Some(vec![id]);
                    }
                } else if matches!(kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
                    let drawing = reshiki::scientific::Drawing {
                        kind,
                        style: self.graphic_style.clone(),
                        phase: self.orbital_phase,
                        flipped: self.phase_flipped,
                        attach: self.attach_symbols,
                    };
                    if let Ok(id) = drawing.place(
                        preview.to_mut(),
                        *start,
                        end,
                        state.modifiers.shift(),
                        10. / self.camera.zoom,
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
                            self.graphic_style.clone(),
                            self.bracket_sides,
                            state.modifiers.shift() || self.graphic_constrain,
                        )
                        .with_arc(self.graphic_arc),
                    );
                    *ring_selection = Some(vec![id]);
                }
            }
            if let Some(Gesture::AtomMark { id, index }) = &state.gesture
                && let Some(a) = preview.to_mut().atom_mut(*id)
                && let Some(mark) = a.marks.get_mut(*index)
            {
                mark.offset = World::new(end.x - a.position.x, end.y - a.position.y);
            }
            if let Some(Gesture::ArrowHandle { id, index }) = &state.gesture
                && let Some(arrow) = self.doc.arrows.iter().find(|a| a.id == *id)
            {
                let end = if *index < 2 {
                    let (end, guides) = self.arrow_end(state, arrow, *index, end, bounds);
                    *smart = guides;
                    end
                } else {
                    end
                };
                if let Some(a) = preview.to_mut().arrows.iter_mut().find(|a| a.id == *id) {
                    a.edit_handle(*index, end);
                }
            }
            if let Some(Gesture::Draw { start, .. }) = &state.gesture
                && self.tool == Tool::Arrow
                && start.distance(end) >= 3.0 / self.camera.zoom
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
                    self.arrow_preset,
                    self.arrow_style.clone(),
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
                g.edit_point(*index, end);
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
        if let (Some(Gesture::Move { start, ids, .. }), Some(p)) = (&state.gesture, state.cursor) {
            let p = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (delta, guides) =
                self.move_delta(state, ids, World::new(p.x - start.x, p.y - start.y), bounds);
            if start.distance(p) < 1.0 / self.camera.zoom {
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
