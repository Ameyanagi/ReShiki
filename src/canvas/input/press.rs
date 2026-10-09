//! Mouse presses on the paper: the context menu and the gesture a left press starts.

use crate::canvas::hit::hit_selection;
use crate::canvas::{
    Edit, Gesture, HandleClick, MoleculeCanvas, State, Tool, TransformDrag, World, tilt,
};
use iced::widget::canvas::Action;
use iced::{Point, Rectangle};
use reshiki::{chains::ChainMode, graphics::GraphicKind};

impl MoleculeCanvas<'_> {
    pub(super) fn right_press(
        &self,
        state: &mut State,
        point: Option<Point>,
        bounds: Rectangle,
        canvas_bounds: Rectangle,
    ) -> Option<Action<Edit>> {
        let p = self.camera.world(point?, bounds);
        let mut hit = hit_selection(self.doc, p, 10. / self.camera.zoom);
        if hit.is_empty() {
            hit = reshiki::editing::ring_at(self.doc, p).unwrap_or_default();
        }
        hit = self.doc.expand_groups(&hit);
        let inside_selection = hit.is_empty()
            && reshiki::scene::selection_bounds(self.doc, self.selected)
                .is_some_and(|(lo, hi)| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y);
        let selected = if inside_selection
            || (!hit.is_empty() && hit.iter().all(|id| self.selected.contains(id)))
        {
            self.selected.to_vec()
        } else {
            hit
        };
        state.end_gesture();
        state.last_click = None;
        let position =
            point? + iced::Vector::new(bounds.x - canvas_bounds.x, bounds.y - canvas_bounds.y);
        Some(Action::publish(Edit::ContextMenu { position, selected }).and_capture())
    }

    pub(super) fn left_press(
        &self,
        state: &mut State,
        point: Option<Point>,
        bounds: Rectangle,
    ) -> Option<Action<Edit>> {
        // Only another press on a transform handle can complete the pair.
        let last_transform_click = state.last_transform_click.take();
        let point = point?;
        let p = self.camera.world(point, bounds);
        if self.tool == Tool::Erase {
            state.gesture = Some(Gesture::Erase { last: p });
            return Some(Action::publish(Edit::EraseStart(p)).and_capture());
        }
        if self.tool == Tool::Tilt {
            return self.press_tilt(state, p, point);
        }
        if let Some(action) = self.press_arrow_handle(state, p) {
            return Some(action);
        }
        if let Some(action) = self.press_edit_points(state, p) {
            return Some(action);
        }
        if let Some(action) =
            self.press_transform_handle(state, p, point, bounds, last_transform_click)
        {
            return Some(action);
        }
        self.start_gesture(state, p)
    }

    fn press_tilt(&self, state: &mut State, p: World, point: Point) -> Option<Action<Edit>> {
        let mut hit = hit_selection(self.doc, p, 10. / self.camera.zoom);
        if hit.is_empty() {
            hit = reshiki::editing::ring_at(self.doc, p).unwrap_or_default();
        }
        let in_selection = reshiki::scene::selection_bounds(self.doc, self.selected)
            .is_some_and(|(lo, hi)| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y);
        let ids = if tilt::available(self.doc, self.selected)
            && ((hit.is_empty() && in_selection)
                || (!hit.is_empty() && hit.iter().all(|id| self.selected.contains(id))))
        {
            self.selected.to_vec()
        } else {
            let mut ids = self.doc.expand_groups(&hit);
            if !tilt::available(self.doc, &ids) {
                ids = reshiki::editing::groups(self.doc, &self.doc.all_ids())
                    .into_iter()
                    .filter(|g| {
                        g.iter()
                            .any(|id| hit.contains(id) && self.doc.atom(*id).is_some())
                    })
                    .flatten()
                    .collect();
            }
            ids
        };
        state.last_click = None;
        state.gesture = Some(if tilt::available(self.doc, &ids) {
            Gesture::Tilt(tilt::TiltDrag { ids, start: point })
        } else {
            Gesture::Select { start: p }
        });
        Some(Action::publish(Edit::Hover(None)).and_capture())
    }

    fn press_arrow_handle(&self, state: &mut State, p: World) -> Option<Action<Edit>> {
        if (self.tool.selects() || matches!(self.tool, Tool::Arrow | Tool::EditPoints))
            && self.selected.len() == 1
        {
            for a in self
                .doc
                .arrows
                .iter()
                .filter(|a| self.selected.contains(&a.id) && self.doc.atom_visible(a.id))
            {
                if let Some(index) = a.handle_at(p, 8.0 / self.camera.zoom) {
                    state.gesture = Some(Gesture::ArrowHandle { id: a.id, index });
                    return Some(Action::request_redraw().and_capture());
                }
            }
        }
        None
    }

    fn press_edit_points(&self, state: &mut State, p: World) -> Option<Action<Edit>> {
        if self.tool == Tool::EditPoints {
            if let Some(indicator) =
                reshiki::atom_labels::indicators(self.doc)
                    .into_iter()
                    .find(|i| {
                        i.owner.selected(self.selected)
                            && i.center.distance(p) < 9. / self.camera.zoom
                    })
            {
                state.gesture = Some(Gesture::AtomIndicator {
                    owner: indicator.owner,
                });
                return Some(Action::request_redraw().and_capture());
            }
            for a in self
                .doc
                .atoms
                .iter()
                .filter(|a| self.selected.contains(&a.id) && self.doc.atom_visible(a.id))
            {
                if let Some(index) = a.marks.iter().position(|m| {
                    a.position.offset(m.offset.x, m.offset.y).distance(p) < 8. / self.camera.zoom
                }) {
                    state.gesture = Some(Gesture::AtomMark { id: a.id, index });
                    return Some(Action::request_redraw().and_capture());
                }
            }
        }
        if self.tool == Tool::EditPoints {
            if let Some(id) = self.doc.nearest(p, 10. / self.camera.zoom).filter(|id| {
                self.doc.atom(*id).is_some_and(|a| a.attachment.is_some())
                    && self.doc.abbreviation(*id).is_none()
            }) {
                state.gesture = Some(Gesture::Move {
                    start: p,
                    ids: vec![id],
                    clicked: vec![id],
                });
                return Some(Action::request_redraw().and_capture());
            }
            for g in self
                .doc
                .graphics
                .iter()
                .filter(|g| self.selected.contains(&g.id))
            {
                if let Some(handles) = g.path_handles() {
                    if let Some(handle) = handles
                        .into_iter()
                        .filter(|h| h.point.distance(p) < 8. / self.camera.zoom)
                        .min_by(|a, b| a.point.distance(p).total_cmp(&b.point.distance(p)))
                    {
                        state.gesture = Some(Gesture::PathPoint(crate::canvas::pen::PointDrag {
                            id: g.id,
                            index: handle.index,
                            pressed: p,
                            original: handle.point,
                        }));
                        return Some(Action::request_redraw().and_capture());
                    }
                    continue;
                }
                let points = g.edit_points();
                let hit = |q: &World| q.distance(p) < 8.0 / self.camera.zoom;
                let index = if g.kind == GraphicKind::Arc {
                    // Coincident full-circle endpoints must expose the end.
                    points.iter().rposition(hit)
                } else {
                    points.iter().position(hit)
                };
                if let Some(index) = index {
                    state.gesture = Some(Gesture::GraphicPoint { id: g.id, index });
                    return Some(Action::request_redraw().and_capture());
                }
            }
        }
        None
    }

    fn press_transform_handle(
        &self,
        state: &mut State,
        p: World,
        point: Point,
        bounds: Rectangle,
        last_transform_click: Option<HandleClick>,
    ) -> Option<Action<Edit>> {
        if self.tool.selects()
            && let Some(selection) =
                state
                    .scene
                    .borrow_mut()
                    .selection(self.doc, self.selected, self.camera, bounds)
            && let Some(handle) = selection.hit(point)
        {
            state.last_click = None;
            state.last_transform_click = last_transform_click;
            state.gesture = Some(Gesture::Transform(Box::new(TransformDrag::new(
                selection,
                handle,
                p,
                self.selected,
            ))));
            return Some(Action::request_redraw().and_capture());
        }
        None
    }

    fn start_gesture(&self, state: &mut State, p: World) -> Option<Action<Edit>> {
        let mut hit = hit_selection(self.doc, p, 10.0 / self.camera.zoom);
        if self.tool.selects() && hit.is_empty() {
            hit = reshiki::editing::ring_at(self.doc, p).unwrap_or_default();
        }
        if self.tool.selects() {
            hit = if state.modifiers.alt() {
                self.doc.expand_integral_groups(&hit)
            } else {
                self.doc.expand_groups(&hit)
            };
        }
        state.gesture = match self.tool {
            Tool::Chain(mode) => {
                let source = self.doc.nearest(p, 10.0 / self.camera.zoom);
                let start = source
                    .and_then(|id| self.doc.atom(id).map(|a| a.position))
                    .unwrap_or(p);
                Some(Gesture::Chain {
                    start,
                    pressed: p,
                    source,
                    points: vec![start],
                    snaking: mode == ChainMode::Snaking,
                    dragged: false,
                })
            }
            Tool::Graphic(_) => Some(Gesture::Graphic { start: p }),
            Tool::EditPoints => {
                return Some(Action::publish(Edit::Select(hit)).and_capture());
            }
            Tool::Select | Tool::Lasso => Some(if !hit.is_empty() {
                Gesture::Move {
                    start: p,
                    ids: reshiki::attachments::movement_selection(
                        self.doc,
                        &if state.modifiers.shift() {
                            reshiki::selection_region::combine(self.selected, &hit, true, false)
                        } else if !state.modifiers.alt()
                            && hit.iter().all(|id| self.selected.contains(id))
                        {
                            self.selected.to_vec()
                        } else {
                            hit.clone()
                        },
                    ),
                    clicked: hit,
                }
            } else if self.tool == Tool::Lasso {
                Gesture::Lasso { points: vec![p] }
            } else {
                Gesture::Select { start: p }
            }),
            Tool::Atom if self.doc.nearest(p, 10.0 / self.camera.zoom).is_some() => {
                Some(Gesture::Draw {
                    start: p,
                    id: self.doc.nearest(p, 10.0 / self.camera.zoom),
                })
            }
            Tool::Bond(_)
            | Tool::StyledBond(_)
            | Tool::Wedge
            | Tool::Hash
            | Tool::Wavy
            | Tool::Arrow => Some(Gesture::Draw {
                start: p,
                id: if self.tool == Tool::Arrow {
                    None
                } else {
                    self.doc.nearest(p, 10.0 / self.camera.zoom)
                },
            }),
            Tool::Ring | Tool::RingPreset(_) | Tool::Template => Some(Gesture::Ring {
                start: p,
                attached: self.doc.nearest(p, 10.0 / self.camera.zoom).is_some()
                    || reshiki::editing::nearest_bond(self.doc, p, 10.0 / self.camera.zoom)
                        .is_some(),
            }),
            _ => return Some(Action::publish(Edit::Click(p)).and_capture()),
        };
        Some(Action::publish(Edit::Hover(None)).and_capture())
    }
}
