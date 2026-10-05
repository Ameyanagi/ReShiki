//! Where a pointer gesture lands (move deltas under bond constraints and smart guides, arrow ends, ring/template anchors, chain plans), shared by event handling and the drag preview so a preview equals the committed edit.

use super::smart_guides::{self, Axis, Guide};
use super::{Edit, MoleculeCanvas, State, World, command_held, movement};
use iced::{Point, Rectangle};
use reshiki::chains::{self, BondDrawing, ChainDrawing};

impl MoleculeCanvas<'_> {
    pub(super) fn pointer_selection(&self, ids: Vec<u64>, point: World) -> Edit {
        if self.keyboard_target.is_some() {
            Edit::SelectAt(ids, point)
        } else {
            Edit::Select(ids)
        }
    }

    /// Ctrl/Cmd drags place a copy with the selection tools; Edit Points only moves points.
    pub(super) fn copies(&self, modifiers: iced::keyboard::Modifiers) -> bool {
        self.tool.selects() && command_held(modifiers)
    }
    /// Drag offset for the preview and release, and the smart guides it meets.
    /// A Ctrl/Cmd copy has no bonds to constrain.
    pub(super) fn move_delta(
        &self,
        state: &State,
        ids: &[u64],
        requested: World,
        bounds: Rectangle,
    ) -> (World, Vec<Guide>) {
        let modifiers = state.modifiers;
        let copies = self.copies(modifiers);
        let delta = if copies {
            if modifiers.shift() {
                movement::axis_locked(requested)
            } else {
                requested
            }
        } else {
            let drawing = self.bond_drawing.unconstrained(modifiers.alt());
            if modifiers.shift() {
                movement::axis_delta(self.doc, ids, requested, drawing)
            } else {
                movement::delta(self.doc, ids, requested, drawing)
            }
        };
        // Guides move whole objects; part of a molecule keeps its bond constraints.
        // Moving everything leaves nothing to snap to.
        if !self.tool.selects()
            || (!copies && state.scene.borrow_mut().whole_document(self.doc, ids))
        {
            return (delta, vec![]);
        }
        let Some((layout, targets)) = self
            .guide_targets(state, ids, copies, bounds)
            .filter(|(layout, _)| copies || layout.whole)
        else {
            return (delta, vec![]);
        };
        let axes: &[Axis] = match (modifiers.shift(), movement::horizontal(requested)) {
            (false, _) => &Axis::BOTH,
            (true, true) => &[Axis::X],
            (true, false) => &[Axis::Y],
        };
        let pixel = 1. / self.camera.zoom;
        let delta = smart_guides::snap(layout.moving, delta, &targets, axes, pixel);
        let guides = smart_guides::guides(layout.moving.translated(delta), &targets, pixel);
        (delta, guides)
    }
    /// A dragged arrow end: 15° steps under fixed angles, then smart guides.
    pub(super) fn arrow_end(
        &self,
        state: &State,
        arrow: &reshiki::document::Arrow,
        index: usize,
        cursor: World,
        bounds: Rectangle,
    ) -> (World, Vec<Guide>) {
        let origin = if index == 0 { arrow.end } else { arrow.start };
        let fixed = self.bond_drawing.fixed_angles && !state.modifiers.alt();
        let end = arrow_endpoint(origin, cursor, fixed);
        let Some((_, targets)) = self.guide_targets(state, &[arrow.id], false, bounds) else {
            return (end, vec![]);
        };
        let pixel = 1. / self.camera.zoom;
        let end = smart_guides::snap_point(end, &targets, fixed.then_some(origin), pixel);
        (end, smart_guides::point_guides(end, &targets, pixel))
    }
    /// The dragged objects and the on-screen objects they can snap to, unless
    /// smart guides are off or Option/Alt moves freely.
    pub(super) fn guide_targets(
        &self,
        state: &State,
        ids: &[u64],
        copies: bool,
        bounds: Rectangle,
    ) -> Option<(std::rc::Rc<smart_guides::Layout>, Vec<smart_guides::Bounds>)> {
        if !self.smart_guides || state.modifiers.alt() {
            return None;
        }
        let layout = state.scene.borrow_mut().guides(self.doc, ids)?;
        let view = smart_guides::Bounds {
            lo: self.camera.world(Point::ORIGIN, bounds),
            hi: self
                .camera
                .world(Point::new(bounds.width, bounds.height), bounds),
        };
        let targets = layout.targets(view, copies);
        Some((layout, targets))
    }
    pub(super) fn template_gesture(
        &self,
        start: World,
        end: World,
        modifiers: iced::keyboard::Modifiers,
    ) -> (World, Option<World>) {
        let doc = self.joining.map(|(j, _)| &j.base).unwrap_or(self.doc);
        let (anchor, direction) = ring_gesture(start, end, true, 10. / self.camera.zoom);
        if !(modifiers.shift() || modifiers.control()) || modifiers.alt() {
            return (anchor, direction);
        }
        let origin = doc
            .nearest(anchor, 10. / self.camera.zoom)
            .and_then(|id| self.doc.atom(id))
            .map(|a| a.position)
            .unwrap_or(anchor);
        let bond = BondDrawing {
            fixed_angles: true,
            fixed_length: false,
            ..self.bond_drawing
        };
        (anchor, direction.map(|p| bond.endpoint(origin, p)))
    }

    pub(super) fn chain_plan(
        &self,
        origin: (World, World),
        source: Option<u64>,
        points: &[World],
        flags: (bool, bool),
        cursor: World,
        modifiers: iced::keyboard::Modifiers,
    ) -> (Vec<World>, Option<u64>) {
        let (start, pressed) = origin;
        let (snaking, dragged) = flags;
        let click = !dragged && pressed.distance(cursor) < 3.0 / self.camera.zoom;
        if dragged && pressed.distance(cursor) < 3.0 / self.camera.zoom {
            return (vec![start], None);
        }
        let mut bond = self.bond_drawing.unconstrained(modifiers.alt());
        let mut flip = modifiers.shift();
        let chain = if click {
            ChainDrawing {
                atoms: Some(self.chain_drawing.atoms.unwrap_or(6)),
                ..self.chain_drawing
            }
        } else {
            self.chain_drawing
        };
        let end = if click {
            bond.fixed_length = true;
            let first = reshiki::editing::bond_extension(self.doc, start, source, 1);
            let half = (180. - chain.angle).to_radians() / 2.;
            let first_angle = chains::direction(start, first);
            let mut axis = first_angle + half;
            let neighbors: Vec<_> = self
                .doc
                .bonds
                .iter()
                .filter_map(|b| {
                    if Some(b.a) == source {
                        self.doc.atom(b.b)
                    } else if Some(b.b) == source {
                        self.doc.atom(b.a)
                    } else {
                        None
                    }
                })
                .collect();
            if let [previous] = neighbors.as_slice() {
                let incoming = chains::direction(previous.position, start);
                let turn = (first_angle - incoming + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                if turn.abs() > 0.01 {
                    axis = first_angle - if turn > 0. { half } else { -half };
                    flip ^= turn > 0.;
                }
            }
            start.offset(axis.cos() * bond.length, axis.sin() * bond.length)
        } else {
            cursor
        };
        let mut points = if snaking && !click {
            let mut points = points.to_vec();
            chains::snake(&mut points, cursor, source.is_some(), bond, chain, flip);
            points
        } else {
            chains::straight(start, end, source.is_some(), bond, chain, flip)
        };
        let target = if click || points.len() < 2 {
            None
        } else {
            self.doc
                .nearest(cursor, 12.0 / self.camera.zoom)
                .filter(|id| Some(*id) != source || points.len() > 3)
                .filter(|id| {
                    self.doc
                        .atom(*id)
                        .zip(points.last())
                        .is_some_and(|(atom, last)| {
                            atom.position.distance(*last) < bond.length * 0.8
                        })
                })
        };
        // Choose the unoccupied side when attaching; Shift is an explicit override.
        if !snaking
            && !modifiers.shift()
            && source.is_some()
            && chains::place(self.doc, &points, source, target, 8.).is_err()
        {
            let other = chains::straight(start, end, source.is_some(), bond, chain, !flip);
            if chains::place(self.doc, &other, source, target, 8.).is_ok() {
                points = other;
            }
        }
        (points, target)
    }
}

pub(super) fn ring_gesture(
    start: World,
    end: World,
    attached: bool,
    radius: f32,
) -> (World, Option<World>) {
    if attached {
        (start, (start.distance(end) > radius).then_some(end))
    } else {
        (end, None)
    }
}

pub(super) fn arrow_endpoint(start: World, cursor: World, fixed_angles: bool) -> World {
    if !fixed_angles {
        return cursor;
    }
    let step = std::f32::consts::PI / 12.;
    let angle = ((cursor.y - start.y).atan2(cursor.x - start.x) / step).round() * step;
    let length = start.distance(cursor);
    start.offset(length * angle.cos(), length * angle.sin())
}
