//! Pointer releases: finish the active gesture as one edit.

use crate::canvas::hit::{bond_target_with_text, hit_object, plane_endpoint, region_selection};
use crate::canvas::pick::{CLICK_SLOP_PX, DRAG_SLOP_PX, HANDLE_PICK_PX};
use crate::canvas::snapping::arrow_endpoint;
use crate::canvas::{
    Edit, Gesture, HandleClick, MoleculeCanvas, State, Tool, TransformDrag, World,
    delocalized_ring_size, tilt,
};
use iced::widget::canvas::Action;
use iced::{Point, Rectangle};
use reshiki::arrows::ArrowHandle;
use reshiki::graphics::GraphicKind;
use std::ops::ControlFlow;

impl MoleculeCanvas<'_> {
    pub(super) fn button_released(
        &self,
        state: &mut State,
        point: Option<Point>,
        bounds: Rectangle,
        inside: bool,
    ) -> Option<Action<Edit>> {
        let gesture = state.end_gesture()?;
        if matches!(gesture, Gesture::Erase { .. }) {
            return Some(Action::publish(Edit::EraseEnd).and_capture());
        }
        let position = point?;
        let p = self.camera.world(position, bounds);
        let edit = match gesture {
            Gesture::Chain {
                start,
                pressed,
                source,
                source_text,
                points,
                snaking,
                dragged,
            } => {
                if !inside {
                    return Some(Action::request_redraw().and_capture());
                }
                let (points, target) = self.chain_plan(
                    (start, pressed),
                    (source, source_text),
                    &points,
                    (snaking, dragged),
                    p,
                    state.modifiers,
                );
                if dragged && points.len() < 2 {
                    return Some(Action::request_redraw().and_capture());
                }
                Edit::Chain {
                    points,
                    source,
                    source_text,
                    target,
                }
            }
            Gesture::Graphic { start } => match self.release_graphic(state, start, p, inside) {
                ControlFlow::Continue(edit) => edit,
                ControlFlow::Break(action) => return Some(action),
            },
            Gesture::ArrowHandle { id, index } => {
                match self.release_arrow_handle(state, id, index, p, bounds, inside) {
                    ControlFlow::Continue(edit) => edit,
                    ControlFlow::Break(action) => return Some(action),
                }
            }
            Gesture::AtomMark { id, index } => {
                if !inside {
                    return Some(Action::request_redraw().and_capture());
                }
                Edit::AtomMark(
                    id,
                    index,
                    p,
                    state.modifiers.alt() || state.modifiers.shift(),
                )
            }
            Gesture::AtomIndicator { owner } => Edit::AtomIndicator(owner, p),
            Gesture::GraphicPoint { id, index } => Edit::GraphicPoint(id, index, p),
            Gesture::PathPoint(drag) => Edit::GraphicPoint(drag.id, drag.index, drag.target(p)),
            Gesture::Transform(drag) => {
                match self.release_transform(state, *drag, p, position, inside) {
                    ControlFlow::Continue(edit) => edit,
                    ControlFlow::Break(action) => return Some(action),
                }
            }
            Gesture::Tilt(drag) => match self.release_tilt(state, drag, position, inside) {
                ControlFlow::Continue(edit) => edit,
                ControlFlow::Break(action) => return Some(action),
            },
            Gesture::Ring { start, attached } => {
                match self.release_ring(state, start, attached, p, inside) {
                    ControlFlow::Continue(edit) => edit,
                    ControlFlow::Break(action) => return Some(action),
                }
            }
            Gesture::Draw {
                start,
                pressed,
                id,
                text,
            } => match self.release_draw(state, (start, pressed), id, text, p, inside) {
                ControlFlow::Continue(edit) => edit,
                ControlFlow::Break(action) => return Some(action),
            },
            Gesture::Move {
                start,
                ids,
                clicked,
            } => match self.release_move(state, start, ids, clicked, p, bounds) {
                ControlFlow::Continue(edit) => edit,
                ControlFlow::Break(action) => return Some(action),
            },
            Gesture::StretchBond { start, plan } => {
                match self.release_stretch(start, plan, p, inside) {
                    ControlFlow::Continue(edit) => edit,
                    ControlFlow::Break(action) => return Some(action),
                }
            }
            Gesture::Select { start } => {
                let polygon = vec![start, World::new(p.x, start.y), p, World::new(start.x, p.y)];
                Edit::Select(region_selection(
                    self.doc,
                    self.selected,
                    &polygon,
                    state.modifiers,
                ))
            }
            Gesture::Lasso { mut points } => {
                points.push(p);
                Edit::Select(region_selection(
                    self.doc,
                    self.selected,
                    &points,
                    state.modifiers,
                ))
            }
            Gesture::Pan { .. } | Gesture::Erase { .. } => {
                return Some(Action::request_redraw());
            }
        };
        let edit = match edit {
            Edit::Select(ids) => self.pointer_selection(ids, p),
            edit => edit,
        };
        Some(Action::publish(edit).and_capture())
    }
    fn release_graphic(
        &self,
        state: &State,
        start: World,
        p: World,
        inside: bool,
    ) -> ControlFlow<Action<Edit>, Edit> {
        if self.tool == Tool::Graphic(GraphicKind::Path) {
            if !inside {
                return ControlFlow::Break(Action::request_redraw().and_capture());
            }
            let Some(stroke) = self.pen_stroke(start, p) else {
                return ControlFlow::Break(Action::request_redraw().and_capture());
            };
            return ControlFlow::Break(Action::publish(Edit::PenSegment(stroke)).and_capture());
        }
        if !inside
            || (start.distance(p) < self.camera.world_len(DRAG_SLOP_PX)
                && !matches!(
                    self.tool,
                    Tool::Graphic(GraphicKind::Symbol(_) | GraphicKind::Orbital(_))
                ))
        {
            return ControlFlow::Break(Action::request_redraw().and_capture());
        }
        let constrain = match self.tool {
            Tool::Graphic(kind) => self.graphic_constrain(kind, state.modifiers),
            _ => state.modifiers.shift() || self.graphics.constrain,
        };
        ControlFlow::Continue(
            if matches!(self.tool, Tool::Graphic(GraphicKind::Orbital(_))) {
                Edit::Orbital(
                    start,
                    p,
                    constrain,
                    self.graphics.snap && !state.modifiers.alt(),
                )
            } else {
                Edit::Graphic(start, p, constrain)
            },
        )
    }
    fn release_arrow_handle(
        &self,
        state: &State,
        id: u64,
        index: usize,
        p: World,
        bounds: Rectangle,
        inside: bool,
    ) -> ControlFlow<Action<Edit>, Edit> {
        if !inside {
            return ControlFlow::Break(Action::request_redraw().and_capture());
        }
        let handle = ArrowHandle::from_index(index);
        if (self.tool == Tool::Arrow || handle.is_some_and(ArrowHandle::is_direction))
            && self
                .doc
                .arrows
                .iter()
                .find(|a| a.id == id)
                .and_then(|a| a.handles().get(index).copied())
                .is_some_and(|handle| handle.distance(p) < self.camera.world_len(DRAG_SLOP_PX))
        {
            if handle.is_some_and(ArrowHandle::is_direction) {
                return ControlFlow::Break(Action::request_redraw().and_capture());
            }
            return ControlFlow::Break(Action::publish(Edit::ArrowClick(id)).and_capture());
        }
        let end = if handle.is_some_and(ArrowHandle::is_endpoint) {
            self.doc
                .arrows
                .iter()
                .find(|a| a.id == id)
                .map(|a| self.arrow_end(state, a, index, p, bounds).0)
                .unwrap_or(p)
        } else {
            p
        };
        ControlFlow::Continue(Edit::ArrowHandle(id, index, end))
    }
    fn release_transform(
        &self,
        state: &mut State,
        drag: TransformDrag,
        p: World,
        position: Point,
        inside: bool,
    ) -> ControlFlow<Action<Edit>, Edit> {
        if inside && drag.is_click(p, self.camera.zoom) {
            let now = std::time::Instant::now();
            let repeated = state.last_transform_click.take().is_some_and(|last| {
                last.handle == drag.handle
                    && last.ids == drag.ids
                    && last.position.distance(position) < 4.
                    && now.duration_since(last.at).as_millis() < 450
            });
            if repeated {
                return ControlFlow::Break(
                    Action::publish(Edit::BeginTransform(drag.handle.field())).and_capture(),
                );
            }
            state.last_transform_click = Some(HandleClick {
                at: now,
                handle: drag.handle,
                ids: drag.ids,
                position,
            });
            return ControlFlow::Break(Action::request_redraw().and_capture());
        }
        state.last_transform_click = None;
        ControlFlow::Continue(drag.into_edit(p, state.modifiers.shift()))
    }
    fn release_tilt(
        &self,
        state: &State,
        drag: tilt::TiltDrag,
        position: Point,
        inside: bool,
    ) -> ControlFlow<Action<Edit>, Edit> {
        if !inside || self.tool != Tool::Tilt {
            return ControlFlow::Break(Action::request_redraw().and_capture());
        }
        let (x, y) = drag.angles(position, state.modifiers.shift());
        ControlFlow::Continue(if x == 0. && y == 0. {
            Edit::Select(drag.ids)
        } else {
            Edit::Tilt {
                ids: drag.ids,
                x,
                y,
            }
        })
    }
    fn release_stretch(
        &self,
        start: World,
        plan: reshiki::editing::reference::Stretch,
        p: World,
        inside: bool,
    ) -> ControlFlow<Action<Edit>, Edit> {
        if !inside || start.distance(p) < self.camera.world_len(CLICK_SLOP_PX) {
            return ControlFlow::Break(Action::request_redraw().and_capture());
        }
        ControlFlow::Continue(Edit::StretchBond {
            fixed: plan.fixed,
            moving: plan.moving,
            length: plan.dragged_length(World::new(p.x - start.x, p.y - start.y)),
        })
    }
    fn release_ring(
        &self,
        state: &State,
        start: World,
        attached: bool,
        p: World,
        inside: bool,
    ) -> ControlFlow<Action<Edit>, Edit> {
        if !inside {
            return ControlFlow::Break(Action::request_redraw().and_capture());
        }
        if self.tool == Tool::Template {
            let (anchor, direction) = self.template_gesture(start, p, state.modifiers);
            return ControlFlow::Continue(Edit::Template(anchor, direction));
        }
        let (anchor, direction) = self.ring_drag(start, p, attached, state.modifiers);
        ControlFlow::Continue(
            if let Some(size) = delocalized_ring_size(self.tool, self.ring_size, state.modifiers) {
                Edit::DelocalizedRing(anchor, direction, size)
            } else if let Tool::RingPreset(preset) = self.tool {
                Edit::RingPreset(
                    preset,
                    anchor,
                    direction,
                    state.modifiers.alt(),
                    state.modifiers.shift(),
                )
            } else {
                Edit::Ring(anchor, direction)
            },
        )
    }
    fn release_draw(
        &self,
        state: &State,
        origin: (World, World),
        id: Option<u64>,
        text: Option<u64>,
        p: World,
        inside: bool,
    ) -> ControlFlow<Action<Edit>, Edit> {
        let (start, pressed) = origin;
        if !inside {
            return ControlFlow::Break(Action::request_redraw().and_capture());
        }
        ControlFlow::Continue(
            if pressed.distance(p) < self.camera.world_len(DRAG_SLOP_PX) {
                if text.is_some() {
                    Edit::TextBond {
                        source: id,
                        source_text: text,
                        start,
                        end: None,
                        target: None,
                        target_text: None,
                    }
                } else if self.arrow_attach_mode() {
                    Edit::ArrowTarget(p, state.modifiers.alt())
                } else {
                    Edit::Click(p)
                }
            } else {
                let origin = id
                    .and_then(|id| self.doc.atom(id).map(|a| a.position))
                    .unwrap_or(start);
                let target = if self.tool == Tool::Arrow {
                    Ok((
                        arrow_endpoint(
                            start,
                            p,
                            self.bond_drawing.fixed_angles && !state.modifiers.alt(),
                        ),
                        None,
                        None,
                    ))
                } else {
                    bond_target_with_text(
                        self.doc,
                        origin,
                        p,
                        (id, text),
                        12.0 / self.camera.zoom,
                        self.bond_drawing.unconstrained(state.modifiers.alt()),
                        self.tool,
                    )
                };
                let (end, target, target_text) = match target {
                    Ok(target) => target,
                    Err(reason) => return ControlFlow::Continue(Edit::TextBondRefused(reason)),
                };
                if text.is_some() || target_text.is_some() {
                    Edit::TextBond {
                        source: id,
                        source_text: text,
                        start: origin,
                        end: Some(end),
                        target,
                        target_text,
                    }
                } else if self.tool != Tool::Arrow
                    && target.is_none()
                    && let Some((id, endpoint)) = id.and_then(|id| {
                        plane_endpoint(
                            self.doc,
                            id,
                            p,
                            self.bond_drawing.unconstrained(state.modifiers.alt()),
                        )
                        .map(|endpoint| (id, endpoint))
                    })
                {
                    Edit::PlaneBond(id, endpoint)
                } else {
                    Edit::Bond(origin, end, id, target)
                }
            },
        )
    }
    fn release_move(
        &self,
        state: &mut State,
        start: World,
        ids: Vec<u64>,
        clicked: Vec<u64>,
        p: World,
        bounds: Rectangle,
    ) -> ControlFlow<Action<Edit>, Edit> {
        let click_slop = self.camera.world_len(CLICK_SLOP_PX);
        ControlFlow::Continue(if start.distance(p) < click_slop {
            if !state.modifiers.shift()
                && let Some(label) = hit_object(self.doc, p, self.camera.world_len(HANDLE_PICK_PX))
                    .filter(|id| self.doc.annotations.iter().any(|a| a.id == *id))
            {
                let now = std::time::Instant::now();
                if state.last_click.is_some_and(|(time, id)| {
                    id == label && now.duration_since(time).as_millis() < 450
                }) {
                    state.last_click = None;
                    return ControlFlow::Break(
                        Action::publish(Edit::BeginText(label)).and_capture(),
                    );
                }
                state.last_click = Some((now, label));
            } else if let Some(atom) = clicked
                .first()
                .copied()
                .filter(|id| self.doc.atom(*id).is_some())
            {
                let now = std::time::Instant::now();
                if state.last_click.is_some_and(|(time, id)| {
                    id == atom && now.duration_since(time).as_millis() < 450
                }) {
                    state.last_click = None;
                    let connected = reshiki::editing::groups(self.doc, &self.doc.all_ids())
                        .into_iter()
                        .find(|g| g.contains(&atom))
                        .unwrap_or(ids);
                    return ControlFlow::Break(
                        Action::publish(self.pointer_selection(connected, p)).and_capture(),
                    );
                }
                state.last_click = Some((now, atom));
            }
            if state.modifiers.shift() {
                let remove = clicked.iter().all(|id| self.selected.contains(id));
                Edit::Select(reshiki::selection_region::combine(
                    self.selected,
                    &clicked,
                    true,
                    remove,
                ))
            } else {
                Edit::Select(clicked)
            }
        } else {
            state.last_click = None;
            let (delta, _) = self.move_delta(
                state,
                &ids,
                World::new(p.x - start.x, p.y - start.y),
                bounds,
            );
            if self.copies(state.modifiers) {
                Edit::Duplicate(ids, delta.x, delta.y)
            } else {
                Edit::Move(ids, delta.x, delta.y)
            }
        })
    }
}
