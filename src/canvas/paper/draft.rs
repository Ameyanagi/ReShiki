//! Previews of drags that edit the draft document directly: tilts, pointer edits and transforms.

use super::Draft;
use crate::canvas::snapping::arrow_endpoint;
use crate::canvas::{Gesture, MoleculeCanvas, State, Tool, World, tilt};
use iced::{Point, Rectangle};
use reshiki::graphics::{Graphic, GraphicKind};

impl MoleculeCanvas<'_> {
    pub(in crate::canvas) fn preview_tilt(
        &self,
        draft: &mut Draft<'_>,
        state: &State,
        bounds: Rectangle,
    ) {
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

    pub(in crate::canvas) fn preview_pointer_edits(
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
                if matches!(kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
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

    pub(in crate::canvas) fn preview_transform(
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
}
