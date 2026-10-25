//! 3D models on the paper (#386): turning a selected model and the 2D → 3D
//! morph.
//! [`tiles`] draws the models themselves as resvg image tiles.
use super::hit::{above_models, hit_selection};
use super::pick::{DRAG_SLOP_PX, PICK_RADIUS_PX};
use super::selection::FRAME_PADDING_PX;
use super::{Edit, MoleculeCanvas, State, World, command_held};
use iced::time::Instant;
use iced::widget::canvas::Action;
use iced::{Point, Rectangle};
use reshiki::models3d;
use std::ops::ControlFlow;
use std::time::Duration;

mod tiles;
pub(super) use tiles::{begin, draw, end, is_model};

/// Screen pixels on either side of the drawn selection outline that move it.
const EDGE_PX: f32 = 8.;
/// The prototype's morph length.
pub(crate) const MORPH: Duration = Duration::from_millis(750);
const DOUBLE_CLICK_MS: u128 = 450;

/// A drag turning a selected model. Each step turns about the screen axes as
/// they are then, as the prototype does, so the preview and the commit share
/// [`Turn::rotation_at`].
#[derive(Debug)]
pub(super) struct Turn {
    pub atoms: Vec<u64>,
    pub clicked: Vec<u64>,
    pub start: World,
    press: Point,
    last: Point,
    rotation: [f32; 4],
    moved: bool,
}

impl Turn {
    /// The view with the drag carried on to `point`; nothing turns within
    /// the drag slop, so a click never turns the model.
    pub fn rotation_at(&self, point: Point, roll: bool) -> [f32; 4] {
        if !self.moved && point.distance(self.press) < DRAG_SLOP_PX {
            return self.rotation;
        }
        models3d::turn(
            self.rotation,
            point.x - self.last.x,
            point.y - self.last.y,
            roll,
        )
    }

    pub fn track(&mut self, point: Point, roll: bool) {
        self.rotation = self.rotation_at(point, roll);
        self.moved |= point.distance(self.press) >= DRAG_SLOP_PX;
        if self.moved {
            self.last = point;
        }
    }
}

impl MoleculeCanvas<'_> {
    /// Moving a selected model's outline uses its drawn screen geometry,
    /// including the padding outside its actual model frame. Ordinary hits
    /// and objects painted above the model retain their existing precedence.
    pub(super) fn model_frame_hit(
        &self,
        state: &State,
        p: World,
        bounds: Rectangle,
    ) -> Option<Vec<u64>> {
        if !self.tool.selects() || !models3d::shows_3d(self.doc, self.selected) {
            return None;
        }
        let selection =
            state
                .scene
                .borrow_mut()
                .selection(self.doc, self.selected, self.camera, bounds)?;
        if !selection.edge_hit(self.camera.screen(p, bounds), EDGE_PX) {
            return None;
        }
        let atoms = models3d::whole_molecule(self.doc, self.selected)?;
        let hit = hit_selection(self.doc, p, self.camera.world_len(PICK_RADIUS_PX));
        if hit.iter().any(|id| {
            (above_models(self.doc, *id) && !self.selected.contains(id))
                || (!atoms.contains(id)
                    && models3d::model_of(self.doc, *id)
                        .is_some_and(|model| model.display != models3d::Display::TwoD))
        }) {
            return None;
        }
        Some(atoms)
    }

    /// The selected model that a drag at `p` turns: inside its frame, not at
    /// its edge, with its molecule the selection, and no Alt or copy key.
    /// Also returns what a click there selects.
    pub(super) fn turn_target(&self, state: &State, p: World) -> Option<(Vec<u64>, Vec<u64>)> {
        if !self.tool.selects() || state.modifiers.alt() || command_held(state.modifiers) {
            return None;
        }
        let selected = models3d::whole_molecule(self.doc, self.selected)?;
        let model = models3d::model_of(self.doc, *selected.first()?)?;
        if model.display == models3d::Display::TwoD {
            return None;
        }
        // The padded interior is part of the visible selection frame too;
        // its edge has already been reserved for moving before this runs.
        let (lo, hi) = models3d::frame(self.doc, model)?;
        let pad = self.camera.world_len(FRAME_PADDING_PX);
        if p.x < lo.x - pad || p.x > hi.x + pad || p.y < lo.y - pad || p.y > hi.y + pad {
            return None;
        }
        let atoms: Vec<_> = model.atoms.iter().map(|a| a.id).collect();
        let hit = hit_selection(self.doc, p, self.camera.world_len(PICK_RADIUS_PX));
        let clicked = if hit.is_empty() {
            atoms.clone()
        } else {
            self.doc.expand_groups(&hit)
        };
        (selected.len() == atoms.len()
            && atoms.iter().all(|id| selected.contains(id))
            && (hit.is_empty() || hit.first().is_some_and(|id| model.contains(*id))))
        .then_some((atoms, clicked))
    }

    pub(super) fn press_turn(
        &self,
        state: &mut State,
        p: World,
        point: Point,
    ) -> Option<Action<Edit>> {
        let (atoms, clicked) = self.turn_target(state, p)?;
        let rotation = models3d::model_of(self.doc, *atoms.first()?)?.rotation;
        state.gesture = Some(super::Gesture::Turn(Turn {
            atoms,
            clicked,
            start: p,
            press: point,
            last: point,
            rotation,
            moved: false,
        }));
        Some(Action::publish(Edit::Hover(None)).and_capture())
    }

    /// A drag commits the turn; a click selects as usual, and a double-click
    /// restores the opening view.
    pub(super) fn release_turn(
        &self,
        state: &mut State,
        turn: Turn,
        position: Point,
        bounds: Rectangle,
    ) -> ControlFlow<Action<Edit>, Edit> {
        let roll = state.modifiers.shift();
        if turn.moved || position.distance(turn.press) >= DRAG_SLOP_PX {
            state.last_click = None;
            return ControlFlow::Continue(Edit::ModelView {
                rotation: turn.rotation_at(position, roll),
                atoms: turn.atoms,
            });
        }
        let now = Instant::now();
        if state.last_click.is_some_and(|(time, id)| {
            turn.atoms.contains(&id) && now.duration_since(time).as_millis() < DOUBLE_CLICK_MS
        }) && let Some(model) = turn
            .atoms
            .first()
            .and_then(|id| models3d::model_of(self.doc, *id))
        {
            state.last_click = None;
            return ControlFlow::Continue(Edit::ModelView {
                rotation: model.initial_view(self.doc),
                atoms: turn.atoms,
            });
        }
        // Below the turn threshold the release is a click: it selects at the
        // press and never moves the molecule.
        self.release_move(
            state,
            turn.start,
            turn.atoms,
            turn.clicked,
            turn.start,
            bounds,
        )
    }

    /// The model morphing from its drawing into 3D, and how far along it is.
    pub(super) fn morph(&self, now: Instant) -> Option<(u64, f32)> {
        let (id, start) = self.morph?;
        let t = now.checked_duration_since(start)?.as_secs_f32() / MORPH.as_secs_f32();
        (t < 1.).then_some((id, t))
    }
}

/// Show `turn`'s view on the preview while it is dragged.
pub(super) fn preview_turn(
    doc: &mut reshiki::document::Document,
    turn: &Turn,
    point: Point,
    roll: bool,
) {
    if let Some(id) = turn.atoms.first() {
        models3d::set_rotation(doc, *id, turn.rotation_at(point, roll));
    }
}

#[cfg(test)]
mod tests;
