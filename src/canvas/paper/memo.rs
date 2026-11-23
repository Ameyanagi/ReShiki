//! One completed preview for an unchanged placement or drag.
use super::{Draft, Fragment, SceneGeometryCache};
use crate::canvas::{Gesture, MoleculeCanvas, State, Tool, World, markers::Markers};
use iced::{Point, Rectangle, keyboard::Modifiers};
use reshiki::{
    equipment::format::Definition,
    scene::Primitive,
    templates::{Anchor, Connection, Template},
};
use std::{
    borrow::Cow,
    cell::{OnceCell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(PartialEq)]
enum GestureKey<'a> {
    Idle,
    Ring(World, bool),
    Hose(World),
    Bond(World, World, Option<u64>, Option<u64>),
    Move(World, Cow<'a, [u64]>),
}

#[derive(PartialEq)]
pub(super) struct Inputs<'a> {
    cursor: Point,
    bounds: Rectangle,
    camera: (World, f32),
    modifiers: Modifiers,
    tool: Tool,
    gesture: GestureKey<'a>,
    selected: Cow<'a, [u64]>,
    bond: (f32, bool, bool),
    ring: (u8, bool),
    guides: bool,
    pull_out: bool,
    template: Option<(Cow<'a, Template>, Anchor)>,
    connection: Connection,
}

impl<'a> Inputs<'a> {
    pub(super) fn new(
        canvas: &'a MoleculeCanvas<'_>,
        state: &'a State,
        bounds: Rectangle,
    ) -> Option<Self> {
        // These can mutate the draft independently of the pointer, or blend live
        // review/authoring content. They keep their existing drawing path.
        if canvas.joining.is_some()
            || canvas.connect.is_some()
            || canvas.hidden_annotation.is_some()
            || canvas.morph.is_some()
            || canvas.equipment_authoring.is_some()
            || canvas.reviewing_pair
        {
            return None;
        }
        let cursor = state.cursor.filter(|point| bounds.contains(*point))?;
        let gesture = match (canvas.tool, &state.gesture) {
            (Tool::Equipment, None) if canvas.piece.is_some() => GestureKey::Idle,
            (Tool::Equipment, Some(Gesture::Hose { start })) if canvas.piece.is_some() => {
                GestureKey::Hose(*start)
            }
            (Tool::Ring | Tool::RingPreset(_) | Tool::Template, None) => GestureKey::Idle,
            (
                Tool::Ring | Tool::RingPreset(_) | Tool::Template,
                Some(Gesture::Ring { start, attached }),
            ) => GestureKey::Ring(*start, *attached),
            (
                tool,
                Some(Gesture::Draw {
                    start,
                    pressed,
                    id,
                    text,
                }),
            ) if tool.bond_preset().is_some() => GestureKey::Bond(*start, *pressed, *id, *text),
            (tool, Some(Gesture::Move { start, ids, .. })) if tool.selects() => {
                GestureKey::Move(*start, Cow::Borrowed(ids))
            }
            _ => return None,
        };
        Some(Self {
            cursor,
            bounds,
            camera: (canvas.camera.center, canvas.camera.zoom),
            modifiers: state.modifiers,
            tool: canvas.tool,
            gesture,
            selected: Cow::Borrowed(canvas.selected),
            bond: (
                canvas.bond_drawing.length,
                canvas.bond_drawing.fixed_angles,
                canvas.bond_drawing.fixed_length,
            ),
            ring: (canvas.ring_size, canvas.aromatic_ring),
            guides: canvas.smart_guides,
            pull_out: state.pull_out,
            template: canvas
                .template
                .map(|(t, anchor)| (Cow::Borrowed(t), anchor)),
            connection: canvas.template_connection,
        })
    }

    fn owned(self) -> Inputs<'static> {
        let gesture = match self.gesture {
            GestureKey::Idle => GestureKey::Idle,
            GestureKey::Ring(start, attached) => GestureKey::Ring(start, attached),
            GestureKey::Hose(start) => GestureKey::Hose(start),
            GestureKey::Bond(start, pressed, id, text) => {
                GestureKey::Bond(start, pressed, id, text)
            }
            GestureKey::Move(start, ids) => GestureKey::Move(start, Cow::Owned(ids.into_owned())),
        };
        Inputs {
            gesture,
            selected: Cow::Owned(self.selected.into_owned()),
            template: self.template.map(|(t, a)| (Cow::Owned(t.into_owned()), a)),
            ..self
        }
    }
}

pub(in crate::canvas) struct Preview {
    inputs: Inputs<'static>,
    piece: Option<Arc<Definition>>,
    pub(super) draft: Draft<'static>,
    pub(super) fragment: OnceCell<Option<Fragment>>,
    pub(super) whole: OnceCell<(Rc<Markers>, Rc<[Primitive]>)>,
    pub(super) geometry: RefCell<SceneGeometryCache>,
    pub(super) matches_document: OnceCell<bool>,
}

impl Preview {
    pub(super) fn new(
        inputs: Inputs<'_>,
        piece: Option<&Arc<Definition>>,
        draft: Draft<'static>,
    ) -> Rc<Self> {
        Rc::new(Self {
            inputs: inputs.owned(),
            piece: piece.cloned(),
            draft,
            fragment: OnceCell::new(),
            whole: OnceCell::new(),
            geometry: RefCell::default(),
            matches_document: OnceCell::new(),
        })
    }

    pub(super) fn matches(&self, inputs: &Inputs<'_>, piece: Option<&Arc<Definition>>) -> bool {
        let same_piece = match (&self.piece, piece) {
            (None, None) => true,
            (Some(old), Some(new)) => Arc::ptr_eq(old, new),
            _ => false,
        };
        same_piece && self.inputs == *inputs
    }
}
