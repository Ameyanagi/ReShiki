use iced::widget::canvas::{self, Action, Geometry, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, Vector, mouse};
use reshiki::{
    chains::{self, BondDrawing, ChainDrawing, ChainMode},
    document::{Document, Point as World},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
};
mod cache;
mod dashes;
mod grid;
pub mod guides;
mod hit;
#[cfg(test)]
pub(crate) mod label_click_tests;
pub mod layered;
mod markers;
mod movement;
pub(crate) mod optimization;
mod pages;
#[cfg(test)]
mod performance;
mod render;
#[cfg(test)]
mod render_parity_tests;
#[cfg(test)]
pub(crate) mod rotation_gesture_tests;
mod selection;
mod smart_guides;
#[cfg(test)]
mod template_style_tests;
mod text_cache;
pub(crate) use text_cache::prepare_fonts;
pub(crate) mod tilt;
use grid::GridCache;
#[cfg(test)]
use grid::grid_dots;
#[cfg(test)]
use hit::bond_target;
use hit::{bond_target_with, hit_selection, plane_endpoint, region_selection};
pub use hit::{distance_to_segment, hit_object};
use render::{draw_document, draw_document_with_minimum_stroke, draw_primitives};
#[cfg(test)]
use reshiki::scene::{Primitive, primitives};
use selection::{Handle, SelectionBox, TransformDrag};
use smart_guides::{Axis, Guide};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Lasso,
    Tilt,
    Chain(ChainMode),
    Bond(u8),
    StyledBond(reshiki::bonds::BondPreset),
    Wedge,
    Hash,
    Wavy,
    Atom,
    Ring,
    RingPreset(reshiki::rings::Preset),
    Template,
    Arrow,
    Text,
    Erase,
    Graphic(GraphicKind),
    EditPoints,
}
impl Tool {
    pub fn bond_preset(self) -> Option<reshiki::bonds::BondPreset> {
        use reshiki::bonds::BondPreset as P;
        Some(match self {
            Self::Bond(1) => P::Single,
            Self::Bond(2) => P::Double,
            Self::Bond(3) => P::Triple,
            Self::Wedge => P::Wedge,
            Self::Hash => P::HashedWedge,
            Self::Wavy => P::Wavy,
            Self::StyledBond(p) => p,
            _ => return None,
        })
    }
    pub fn selects(self) -> bool {
        matches!(self, Self::Select | Self::Lasso)
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Select => {
                "Bonded drags use Length/Angles · Option/Alt moves freely, without guides · Shift locks an axis · Ctrl/Cmd drag copies · Drag a ring edge to fuse"
            }
            Self::Lasso => "Draw around objects · Shift adds · Option/Alt drag subtracts",
            Self::Tilt => "Drag a ring or selection to tilt · Shift snaps to 15° · Escape cancels",
            Self::Chain(_) => {
                "Drag a chain · Ctrl bends · Shift flips · Click places the chosen number of carbons"
            }
            Self::Bond(2) => {
                "Click a bond to make it double · Click again to shift centered / left / right"
            }
            Self::Bond(_) => {
                "Click an endpoint to grow · Drag to draw · Click a bond to cycle single → double → triple"
            }
            Self::Wedge | Self::Hash | Self::Wavy | Self::StyledBond(_) => {
                "Click an endpoint to grow a chain · Drag to choose direction · Click a bond to change it"
            }
            Self::Atom => "Click to add an atom or replace an existing element",
            Self::Ring | Self::RingPreset(_) => {
                "Click or drag onto an atom or bond to attach · Drag from a bond to choose the side"
            }
            Self::Template => {
                "Preview, then click an atom or bond to attach · Drag to choose the side · Escape cancels"
            }
            Self::Arrow => {
                "Click to place or change an arrow · Click again to switch direction or half-head side · Drag the middle handle to bend"
            }
            Self::Text => "Click to type a label · Double-click a label to edit · Escape cancels",
            Self::Erase => {
                "Drag to erase atoms, bonds and objects along the stroke · Undo restores the whole stroke"
            }
            Self::Graphic(GraphicKind::Symbol(_)) => {
                "Click an atom to attach · Drag from an atom to position · Click empty space for a free symbol"
            }
            Self::Graphic(GraphicKind::Orbital(_)) => {
                "Click to place · Drag from the node for size/direction · Shift snaps to 15°"
            }
            Self::Graphic(_) => {
                "Drag to draw · Shift constrains proportions or angle · Escape cancels"
            }
            Self::EditPoints => {
                "Drag a curve handle or attachment point only · Escape returns to Select"
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformField {
    Rotation,
    Scale,
    Width,
    Height,
}

#[cfg(test)]
mod transform_shortcut_tests;

#[derive(Debug, Clone)]
pub enum Edit {
    ContextMenu {
        position: Point,
        selected: Vec<u64>,
    },
    EraseStart(World),
    EraseTo(World, World),
    EraseEnd,
    BeginText(u64),
    /// Open an existing numeric control without transforming the drawing.
    BeginTransform(TransformField),
    Hover(Option<World>),
    Chain {
        points: Vec<World>,
        source: Option<u64>,
        target: Option<u64>,
    },
    Graphic(World, World, bool),
    GraphicPoint(u64, usize, World),
    AtomMark(u64, usize, World),
    AtomIndicator(reshiki::atom_labels::Owner, World),
    ArrowHandle(u64, usize, World),
    ArrowClick(u64),
    Select(Vec<u64>),
    /// Preserve the exact mouse position for a keyboard hotspot handoff.
    SelectAt(Vec<u64>, World),
    RelaxDragStart {
        session: u64,
        atom: u64,
    },
    RelaxDragTarget {
        session: u64,
        atom: u64,
        target: World,
    },
    RelaxDragEnd {
        session: u64,
        atom: u64,
        target: Option<World>,
    },
    RelaxDragCancel {
        session: u64,
    },
    RelaxRotate {
        session: u64,
        x: f64,
        y: f64,
    },
    Move(Vec<u64>, f32, f32),
    /// Copy the objects to the offset, leaving the originals in place.
    Duplicate(Vec<u64>, f32, f32),
    Tilt {
        ids: Vec<u64>,
        x: f32,
        y: f32,
    },
    Transform {
        ids: Vec<u64>,
        pivot: World,
        scale: f32,
        rotation: f32,
    },
    ScaleAxes {
        ids: Vec<u64>,
        pivot: World,
        x: f32,
        y: f32,
    },
    Bond(World, World, Option<u64>, Option<u64>),
    PlaneBond(u64, reshiki::projection::growth::Endpoint),
    Ring(World, Option<World>),
    DelocalizedRing(World, Option<World>, u8),
    RingPreset(reshiki::rings::Preset, World, Option<World>, bool, bool),
    Template(World, Option<World>),
    Click(World),
    Pan(f32, f32),
    Zoom(f32, World),
}
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub center: World,
    pub zoom: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: World::default(),
            zoom: 1.0,
        }
    }
}
impl Camera {
    pub(crate) fn screen(self, p: World, bounds: Rectangle) -> Point {
        Point::new(
            (p.x - self.center.x) * self.zoom + bounds.width / 2.0,
            (p.y - self.center.y) * self.zoom + bounds.height / 2.0,
        )
    }
    fn world(self, p: Point, bounds: Rectangle) -> World {
        World::new(
            (p.x - bounds.width / 2.0) / self.zoom + self.center.x,
            (p.y - bounds.height / 2.0) / self.zoom + self.center.y,
        )
    }
}
#[derive(Default)]
pub struct State {
    scene: std::cell::RefCell<cache::SceneCache>,
    grid: std::cell::RefCell<GridCache>,
    text: std::cell::RefCell<text_cache::TextCache>,
    gesture: Option<Gesture>,
    relaxation: Option<optimization::Drag>,
    cursor: Option<Point>,
    last_click: Option<(std::time::Instant, u64)>,
    last_transform_click: Option<HandleClick>,
    modifiers: iced::keyboard::Modifiers,
}

struct HandleClick {
    at: std::time::Instant,
    handle: Handle,
    ids: Vec<u64>,
    position: Point,
}
impl State {
    /// Every drag ends here, so a Ctrl/Cmd drag copy never outlives its drag.
    fn end_gesture(&mut self) -> Option<Gesture> {
        self.scene.get_mut().release_copy();
        self.gesture.take()
    }
}
#[derive(Debug)]
enum Gesture {
    Erase {
        last: World,
    },
    Chain {
        start: World,
        pressed: World,
        source: Option<u64>,
        points: Vec<World>,
        snaking: bool,
        dragged: bool,
    },
    Graphic {
        start: World,
    },
    AtomIndicator {
        owner: reshiki::atom_labels::Owner,
    },
    AtomMark {
        id: u64,
        index: usize,
    },
    GraphicPoint {
        id: u64,
        index: usize,
    },
    ArrowHandle {
        id: u64,
        index: usize,
    },
    Transform(Box<TransformDrag>),
    Tilt(tilt::TiltDrag),
    Draw {
        start: World,
        id: Option<u64>,
    },
    Ring {
        start: World,
        attached: bool,
    },
    Move {
        start: World,
        ids: Vec<u64>,
        clicked: Vec<u64>,
    },
    Select {
        start: World,
    },
    Lasso {
        points: Vec<World>,
    },
    Pan {
        last: Point,
    },
}
// Cmd on macOS, Ctrl elsewhere.
fn command_held(modifiers: iced::keyboard::Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.logo()
    } else {
        modifiers.control()
    }
}
// Chairs and Haworth projections retain their geometry.
fn delocalized_ring_size(tool: Tool, size: u8, modifiers: iced::keyboard::Modifiers) -> Option<u8> {
    if !command_held(modifiers) {
        return None;
    }
    use reshiki::rings::Preset;
    match tool {
        Tool::Ring => Some(size),
        Tool::RingPreset(Preset::Regular | Preset::Benzene) => Some(6),
        Tool::RingPreset(Preset::Cyclopentadiene) => Some(5),
        _ => None,
    }
}

pub struct MoleculeCanvas<'a> {
    pub(crate) optimizer: Option<optimization::Context<'a>>,
    pub(crate) keyboard_target: Option<World>,
    pub joining: Option<(&'a reshiki::joining::Prepared, reshiki::templates::Anchor)>,
    pub hidden_annotation: Option<u64>,
    pub bond_drawing: BondDrawing,
    pub chain_drawing: ChainDrawing,
    pub doc: &'a Document,
    pub element: &'a str,
    pub selected: &'a [u64],
    pub tool: Tool,
    pub camera: Camera,
    pub grid: bool,
    pub guides: guides::Guides,
    /// Snap dragged objects and arrow ends to other objects.
    pub smart_guides: bool,
    pub ring_size: u8,
    pub aromatic_ring: bool,
    pub template_connection: reshiki::templates::Connection,
    pub template: Option<(&'a reshiki::templates::Template, reshiki::templates::Anchor)>,
    pub arrow_preset: reshiki::arrows::Preset,
    pub arrow_style: &'a reshiki::arrows::ArrowStyle,
    pub orbital_phase: reshiki::scientific::Phase,
    pub phase_flipped: bool,
    pub attach_symbols: bool,
    pub graphic_constrain: bool,
    pub graphic_arc: reshiki::graphics::ArcGeometry,
    pub graphic_style: &'a GraphicStyle,
    pub bracket_sides: BracketSides,
}
fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

impl MoleculeCanvas<'_> {
    fn pointer_selection(&self, ids: Vec<u64>, point: World) -> Edit {
        if self.keyboard_target.is_some() {
            Edit::SelectAt(ids, point)
        } else {
            Edit::Select(ids)
        }
    }

    /// Ctrl/Cmd drags place a copy with the selection tools; Edit Points only moves points.
    fn copies(&self, modifiers: iced::keyboard::Modifiers) -> bool {
        self.tool.selects() && command_held(modifiers)
    }
    /// Drag offset for the preview and release, and the smart guides it meets.
    /// A Ctrl/Cmd copy has no bonds to constrain.
    fn move_delta(
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
    fn arrow_end(
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
    fn guide_targets(
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
    fn template_gesture(
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

    fn chain_plan(
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

impl canvas::Program<Edit> for MoleculeCanvas<'_> {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Edit>> {
        let canvas_bounds = bounds;
        let bounds = self.guides.paper(bounds);
        // Iced may dispatch a batch with the final cursor position. Preserve the
        // position carried by each motion event so fast drags retain their origin.
        let was_inside = state.cursor.is_some_and(|p| bounds.contains(p));
        let pointer_moved = matches!(event, Event::Mouse(mouse::Event::CursorMoved { position }) if state.cursor != Some(*position));
        if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
            state.cursor = Some(*position);
        }
        if let Event::Keyboard(
            iced::keyboard::Event::ModifiersChanged(modifiers)
            | iced::keyboard::Event::KeyPressed { modifiers, .. }
            | iced::keyboard::Event::KeyReleased { modifiers, .. },
        ) = event
        {
            state.modifiers = *modifiers;
        }
        let point = state
            .cursor
            .or(cursor.position())
            .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
        let inside = point.is_some_and(|p| Rectangle::with_size(bounds.size()).contains(p));
        if let Some(context) = self.optimizer {
            return optimization::update(self, context, state, event, bounds, point, inside);
        }
        state.relaxation = None;
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && !inside
            || matches!(
                event,
                Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
                    | Event::Window(iced::window::Event::Unfocused)
                    | Event::Mouse(
                        mouse::Event::ButtonPressed(mouse::Button::Right | mouse::Button::Middle)
                            | mouse::Event::WheelScrolled { .. }
                            | mouse::Event::CursorLeft
                    )
            )
        {
            state.last_transform_click = None;
        }
        match event {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(_)) => {
                Some(Action::request_redraw())
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => {
                if matches!(state.end_gesture(), Some(Gesture::Erase { .. })) {
                    return Some(Action::publish(Edit::EraseEnd));
                }
                Some(Action::request_redraw())
            }
            Event::Window(iced::window::Event::Unfocused) => {
                let erasing = matches!(state.end_gesture(), Some(Gesture::Erase { .. }));
                state.last_click = None;
                state.cursor = None;
                Some(Action::publish(if erasing {
                    Edit::EraseEnd
                } else {
                    Edit::Hover(None)
                }))
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                state.cursor = None;
                if matches!(state.gesture, Some(Gesture::Erase { .. })) {
                    state.gesture = None;
                    return Some(Action::publish(Edit::EraseEnd));
                }
                Some(Action::publish(Edit::Hover(None)))
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if inside && state.gesture.is_none() =>
            {
                let (x, y, zoom_amount) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x * 40., *y * 40., *y * 0.12),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y, *y * 0.003),
                };
                if !x.is_finite() || !y.is_finite() {
                    return None;
                }
                let edit = if state.modifiers.command() || state.modifiers.control() {
                    Edit::Zoom(zoom_amount.exp(), self.camera.world(point?, bounds))
                } else {
                    Edit::Pan(x / self.camera.zoom, y / self.camera.zoom)
                };
                Some(Action::publish(edit).and_capture())
            }
            // macOS can report Control-click as a secondary click.
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if inside && self.tool == Tool::Template && state.modifiers.control() =>
            {
                state.gesture = Some(Gesture::Ring {
                    start: self.camera.world(point?, bounds),
                    attached: true,
                });
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if inside => {
                let p = self.camera.world(point?, bounds);
                let mut hit = hit_selection(self.doc, p, 10. / self.camera.zoom);
                if hit.is_empty() {
                    hit = reshiki::editing::ring_at(self.doc, p).unwrap_or_default();
                }
                hit = self.doc.expand_groups(&hit);
                let inside_selection = hit.is_empty()
                    && reshiki::scene::selection_bounds(self.doc, self.selected).is_some_and(
                        |(lo, hi)| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y,
                    );
                let selected = if inside_selection
                    || (!hit.is_empty() && hit.iter().all(|id| self.selected.contains(id)))
                {
                    self.selected.to_vec()
                } else {
                    hit
                };
                state.end_gesture();
                state.last_click = None;
                let position = point?
                    + iced::Vector::new(bounds.x - canvas_bounds.x, bounds.y - canvas_bounds.y);
                Some(Action::publish(Edit::ContextMenu { position, selected }).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) if inside => {
                state.gesture = Some(Gesture::Pan { last: point? });
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
                // Only another press on a transform handle can complete the pair.
                let last_transform_click = state.last_transform_click.take();
                let p = self.camera.world(point?, bounds);
                if self.tool == Tool::Erase {
                    state.gesture = Some(Gesture::Erase { last: p });
                    return Some(Action::publish(Edit::EraseStart(p)).and_capture());
                }
                if self.tool == Tool::Tilt {
                    let mut hit = hit_selection(self.doc, p, 10. / self.camera.zoom);
                    if hit.is_empty() {
                        hit = reshiki::editing::ring_at(self.doc, p).unwrap_or_default();
                    }
                    let in_selection = reshiki::scene::selection_bounds(self.doc, self.selected)
                        .is_some_and(|(lo, hi)| {
                            p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
                        });
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
                        Gesture::Tilt(tilt::TiltDrag { ids, start: point? })
                    } else {
                        Gesture::Select { start: p }
                    });
                    return Some(Action::publish(Edit::Hover(None)).and_capture());
                }
                if (self.tool.selects() || matches!(self.tool, Tool::Arrow | Tool::EditPoints))
                    && self.selected.len() == 1
                {
                    for a in
                        self.doc.arrows.iter().filter(|a| {
                            self.selected.contains(&a.id) && self.doc.atom_visible(a.id)
                        })
                    {
                        if let Some(index) = a
                            .handles()
                            .iter()
                            .position(|q| q.distance(p) < 8.0 / self.camera.zoom)
                        {
                            state.gesture = Some(Gesture::ArrowHandle { id: a.id, index });
                            return Some(Action::request_redraw().and_capture());
                        }
                    }
                }
                if self.tool == Tool::EditPoints {
                    if let Some(indicator) = reshiki::atom_labels::indicators(self.doc)
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
                    for a in
                        self.doc.atoms.iter().filter(|a| {
                            self.selected.contains(&a.id) && self.doc.atom_visible(a.id)
                        })
                    {
                        if let Some(index) = a.marks.iter().position(|m| {
                            a.position.offset(m.offset.x, m.offset.y).distance(p)
                                < 8. / self.camera.zoom
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
                if self.tool.selects()
                    && let Some(selection) = state.scene.borrow_mut().selection(
                        self.doc,
                        self.selected,
                        self.camera,
                        bounds,
                    )
                    && let Some(handle) = selection.hit(point?)
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
                                    reshiki::selection_region::combine(
                                        self.selected,
                                        &hit,
                                        true,
                                        false,
                                    )
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
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(Gesture::Erase { last }) = &mut state.gesture {
                    if self.tool != Tool::Erase {
                        state.gesture = None;
                        return Some(Action::publish(Edit::EraseEnd));
                    }
                    let p = self.camera.world(point?, bounds);
                    let from = *last;
                    *last = p;
                    return Some(Action::publish(Edit::EraseTo(from, p)).and_capture());
                }
                if let Some(Gesture::Chain {
                    start,
                    source,
                    points,
                    snaking,
                    pressed,
                    dragged,
                }) = &mut state.gesture
                {
                    let p = self.camera.world(point?, bounds);
                    *dragged |= pressed.distance(p) > 3.0 / self.camera.zoom;
                    let bond = self.bond_drawing.unconstrained(state.modifiers.alt());
                    *snaking |= state.modifiers.control();
                    if *snaking {
                        chains::snake(
                            points,
                            p,
                            source.is_some(),
                            bond,
                            self.chain_drawing,
                            state.modifiers.shift(),
                        );
                    } else {
                        *points = chains::straight(
                            *start,
                            p,
                            source.is_some(),
                            bond,
                            self.chain_drawing,
                            state.modifiers.shift(),
                        );
                    }
                }
                if let Some(Gesture::Transform(drag)) = &mut state.gesture {
                    drag.track_pointer(self.camera.world(point?, bounds), self.camera.zoom);
                }
                if let Some(Gesture::Lasso { points }) = &mut state.gesture {
                    let p = self.camera.world(point?, bounds);
                    if points
                        .last()
                        .is_none_or(|last| last.distance(p) > 2.0 / self.camera.zoom)
                    {
                        points.push(p);
                    }
                }
                if let Some(Gesture::Pan { last }) = &mut state.gesture {
                    let p = point?;
                    let delta = p - *last;
                    *last = p;
                    return Some(
                        Action::publish(Edit::Pan(
                            delta.x / self.camera.zoom,
                            delta.y / self.camera.zoom,
                        ))
                        .and_capture(),
                    );
                }
                if state.gesture.is_some() {
                    // A drag only changes its canvas preview. Avoid rebuilding and laying
                    // out the whole application for every intermediate pointer event.
                    return Some(Action::request_redraw().and_capture());
                }
                if inside || was_inside {
                    if self.keyboard_target.is_some() && !pointer_moved {
                        return Some(Action::request_redraw());
                    }
                    let hover = if inside && state.gesture.is_none() {
                        point.map(|p| self.camera.world(p, bounds))
                    } else {
                        None
                    };
                    Some(Action::publish(Edit::Hover(hover)))
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                let gesture = state.end_gesture()?;
                if matches!(gesture, Gesture::Erase { .. }) {
                    return Some(Action::publish(Edit::EraseEnd).and_capture());
                }
                let p = self.camera.world(point?, bounds);
                let edit = match gesture {
                    Gesture::Chain {
                        start,
                        pressed,
                        source,
                        points,
                        snaking,
                        dragged,
                    } => {
                        if !inside {
                            return Some(Action::request_redraw().and_capture());
                        }
                        let (points, target) = self.chain_plan(
                            (start, pressed),
                            source,
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
                            target,
                        }
                    }
                    Gesture::Graphic { start } => {
                        if !inside
                            || (start.distance(p) < 3.0 / self.camera.zoom
                                && !matches!(
                                    self.tool,
                                    Tool::Graphic(GraphicKind::Symbol(_) | GraphicKind::Orbital(_))
                                ))
                        {
                            return Some(Action::request_redraw().and_capture());
                        }
                        Edit::Graphic(start, p, state.modifiers.shift() || self.graphic_constrain)
                    }
                    Gesture::ArrowHandle { id, index } => {
                        if !inside {
                            return Some(Action::request_redraw().and_capture());
                        }
                        if self.tool == Tool::Arrow
                            && self
                                .doc
                                .arrows
                                .iter()
                                .find(|a| a.id == id)
                                .and_then(|a| a.handles().get(index).copied())
                                .is_some_and(|handle| handle.distance(p) < 3. / self.camera.zoom)
                        {
                            return Some(Action::publish(Edit::ArrowClick(id)).and_capture());
                        }
                        let end = if index < 2 {
                            self.doc
                                .arrows
                                .iter()
                                .find(|a| a.id == id)
                                .map(|a| self.arrow_end(state, a, index, p, bounds).0)
                                .unwrap_or(p)
                        } else {
                            p
                        };
                        Edit::ArrowHandle(id, index, end)
                    }
                    Gesture::AtomMark { id, index } => {
                        if !inside {
                            return Some(Action::request_redraw().and_capture());
                        }
                        Edit::AtomMark(id, index, p)
                    }
                    Gesture::AtomIndicator { owner } => Edit::AtomIndicator(owner, p),
                    Gesture::GraphicPoint { id, index } => Edit::GraphicPoint(id, index, p),
                    Gesture::Transform(drag) => {
                        if inside && drag.is_click(p, self.camera.zoom) {
                            let position = point?;
                            let now = std::time::Instant::now();
                            let repeated = state.last_transform_click.take().is_some_and(|last| {
                                last.handle == drag.handle
                                    && last.ids == drag.ids
                                    && last.position.distance(position) < 4.
                                    && now.duration_since(last.at).as_millis() < 450
                            });
                            if repeated {
                                return Some(
                                    Action::publish(Edit::BeginTransform(drag.handle.field()))
                                        .and_capture(),
                                );
                            }
                            state.last_transform_click = Some(HandleClick {
                                at: now,
                                handle: drag.handle,
                                ids: drag.ids,
                                position,
                            });
                            return Some(Action::request_redraw().and_capture());
                        }
                        state.last_transform_click = None;
                        drag.into_edit(p, state.modifiers.shift())
                    }
                    Gesture::Tilt(drag) => {
                        if !inside || self.tool != Tool::Tilt {
                            return Some(Action::request_redraw().and_capture());
                        }
                        let (x, y) = drag.angles(point?, state.modifiers.shift());
                        if x == 0. && y == 0. {
                            Edit::Select(drag.ids)
                        } else {
                            Edit::Tilt {
                                ids: drag.ids,
                                x,
                                y,
                            }
                        }
                    }
                    Gesture::Ring { start, attached } => {
                        if !inside {
                            return Some(Action::request_redraw().and_capture());
                        }
                        let (anchor, direction) = ring_gesture(
                            start,
                            p,
                            attached
                                || self.tool == Tool::Template
                                || matches!(self.tool, Tool::RingPreset(_)),
                            10.0 / self.camera.zoom,
                        );
                        if let Some(size) =
                            delocalized_ring_size(self.tool, self.ring_size, state.modifiers)
                        {
                            Edit::DelocalizedRing(anchor, direction, size)
                        } else if self.tool == Tool::Template {
                            let (anchor, direction) =
                                self.template_gesture(start, p, state.modifiers);
                            Edit::Template(anchor, direction)
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
                        }
                    }
                    Gesture::Draw { start, id } => {
                        if !inside {
                            return Some(Action::request_redraw().and_capture());
                        }
                        if start.distance(p) < 3.0 / self.camera.zoom {
                            Edit::Click(p)
                        } else {
                            let origin = id
                                .and_then(|id| self.doc.atom(id).map(|a| a.position))
                                .unwrap_or(start);
                            let (end, target) = if self.tool == Tool::Arrow {
                                (
                                    arrow_endpoint(
                                        start,
                                        p,
                                        self.bond_drawing.fixed_angles && !state.modifiers.alt(),
                                    ),
                                    None,
                                )
                            } else {
                                bond_target_with(
                                    self.doc,
                                    origin,
                                    p,
                                    id,
                                    12.0 / self.camera.zoom,
                                    self.bond_drawing.unconstrained(state.modifiers.alt()),
                                )
                            };
                            if self.tool != Tool::Arrow
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
                        }
                    }
                    Gesture::Move {
                        start,
                        ids,
                        clicked,
                    } => {
                        if start.distance(p) < 1.0 / self.camera.zoom {
                            if !state.modifiers.shift()
                                && let Some(label) = hit_object(self.doc, p, 8. / self.camera.zoom)
                                    .filter(|id| self.doc.annotations.iter().any(|a| a.id == *id))
                            {
                                let now = std::time::Instant::now();
                                if state.last_click.is_some_and(|(time, id)| {
                                    id == label && now.duration_since(time).as_millis() < 450
                                }) {
                                    state.last_click = None;
                                    return Some(
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
                                    let connected =
                                        reshiki::editing::groups(self.doc, &self.doc.all_ids())
                                            .into_iter()
                                            .find(|g| g.contains(&atom))
                                            .unwrap_or(ids);
                                    return Some(
                                        Action::publish(self.pointer_selection(connected, p))
                                            .and_capture(),
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
                        }
                    }
                    Gesture::Select { start } => {
                        let polygon =
                            vec![start, World::new(p.x, start.y), p, World::new(start.x, p.y)];
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
            _ => None,
        }
    }
    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let paper = self.guides.paper(bounds);
        let offset = Vector::new(paper.x - bounds.x, paper.y - bounds.y);
        let mut frame = layered::Frame::clipped(
            renderer,
            Rectangle {
                x: offset.x,
                y: offset.y,
                ..paper
            },
            offset,
        )
        .with_canvas(self.doc.canvas_theme)
        .with_text_cache(&state.text);
        self.draw_paper(&mut frame, state, paper, cursor);
        if let Some(context) = self.optimizer {
            optimization::draw_pins(self, context, &mut frame, paper);
        }
        if let Some(target) = self.keyboard_target {
            let point = self.camera.screen(target, paper);
            let ink = rgb([192, 112, 32]);
            frame.stroke(
                &Path::circle(point, 9.),
                Stroke::default().with_width(2.).with_color(ink),
            );
            for (a, b) in [
                (point - Vector::new(12., 0.), point - Vector::new(6., 0.)),
                (point + Vector::new(6., 0.), point + Vector::new(12., 0.)),
                (point - Vector::new(0., 12.), point - Vector::new(0., 6.)),
                (point + Vector::new(0., 6.), point + Vector::new(0., 12.)),
            ] {
                frame.stroke(
                    &Path::line(a, b),
                    Stroke::default().with_width(1.5).with_color(ink),
                );
            }
        }
        let mut layers = frame.finish();
        let mut frame = layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let pointer = state
            .cursor
            .filter(|p| paper.contains(*p))
            .map(|p| Point::new(p.x - paper.x, p.y - paper.y));
        self.guides.draw_rulers(&mut frame, self.camera, pointer);
        layers.extend(frame.finish());
        layers
    }
    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let bounds = self.guides.paper(bounds);
        if let Some(Gesture::Transform(drag)) = &state.gesture {
            return if matches!(drag.handle, Handle::Rotate) {
                mouse::Interaction::Grabbing
            } else {
                drag.handle.cursor()
            };
        }
        if matches!(state.gesture, Some(Gesture::ArrowHandle { .. })) {
            return mouse::Interaction::Grabbing;
        }
        if self.tool == Tool::Tilt && cursor.is_over(bounds) {
            return if matches!(state.gesture, Some(Gesture::Tilt(_))) {
                mouse::Interaction::Grabbing
            } else {
                mouse::Interaction::Grab
            };
        }
        if self.tool == Tool::Erase && cursor.is_over(bounds) {
            return mouse::Interaction::Crosshair;
        }
        if self.selected.len() == 1
            && let Some(p) = cursor.position_in(bounds)
        {
            let p = self.camera.world(p, bounds);
            if self
                .doc
                .arrows
                .iter()
                .filter(|a| self.selected.contains(&a.id) && self.doc.atom_visible(a.id))
                .any(|a| {
                    a.handles()
                        .iter()
                        .any(|q| q.distance(p) < 8. / self.camera.zoom)
                })
            {
                return mouse::Interaction::Grab;
            }
        }
        if cursor.is_over(bounds) {
            if self.tool.selects() {
                if let Some(selection) =
                    state
                        .scene
                        .borrow_mut()
                        .selection(self.doc, self.selected, self.camera, bounds)
                    && let Some(p) = cursor.position()
                    && let Some(handle) = selection.hit(Point::new(p.x - bounds.x, p.y - bounds.y))
                {
                    return handle.cursor();
                }
                mouse::Interaction::default()
            } else {
                mouse::Interaction::Crosshair
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

impl MoleculeCanvas<'_> {
    fn draw_paper(
        &self,
        frame: &mut layered::Frame<'_>,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) {
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
        let mut preview = Cow::Borrowed(self.joining.map(|(j, _)| &j.base).unwrap_or(self.doc));
        let mut ring_selection = None;
        let mut chain_badge = None;
        let mut template_notice = None;
        let mut rejection: Option<reshiki::editing::RingRejection> = None;
        let mut translation = None;
        let mut smart = Vec::new();
        if let (Some(Gesture::Tilt(drag)), Some(p)) = (&state.gesture, state.cursor)
            && self.tool == Tool::Tilt
        {
            let (x, y) = drag.angles(
                Point::new(p.x - bounds.x, p.y - bounds.y),
                state.modifiers.shift(),
            );
            tilt::apply(preview.to_mut(), &drag.ids, x, y);
            ring_selection = Some(drag.ids.clone());
            template_notice = Some((
                format!("3D tilt · X {x:+.0}° · Y {y:+.0}° · Shift snaps · Escape cancels"),
                true,
            ));
        }
        if let (
            Some(Gesture::Chain {
                start,
                pressed,
                source,
                points,
                snaking,
                dragged,
            }),
            Some(p),
        ) = (&state.gesture, state.cursor)
        {
            let cursor = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (points, target) = self.chain_plan(
                (*start, *pressed),
                *source,
                points,
                (*snaking, *dragged),
                cursor,
                state.modifiers,
            );
            let endpoint = target
                .and_then(|id| self.doc.atom(id).map(|a| a.position))
                .or_else(|| points.last().copied())
                .unwrap_or(*start);
            let added = points
                .len()
                .saturating_sub(usize::from(source.is_some()))
                .saturating_sub(usize::from(target.is_some() && points.len() > 1));
            let cancelled = *dragged && points.len() < 2;
            let placement = if cancelled {
                Ok((self.doc.clone(), vec![]))
            } else {
                chains::place(self.doc, &points, *source, target, 10.0 / self.camera.zoom)
            };
            match placement {
                Ok((doc, ids)) => {
                    preview = Cow::Owned(doc);
                    ring_selection = Some(ids);
                    chain_badge = Some((
                        endpoint,
                        if cancelled {
                            "Release to cancel".into()
                        } else {
                            format!("{added} new C · {} bonds", points.len().saturating_sub(1))
                        },
                        true,
                    ));
                }
                Err(_) => {
                    for pair in points.windows(2) {
                        let [a, b] = pair else { continue };
                        frame.stroke(
                            &Path::line(
                                self.camera.screen(*a, bounds),
                                self.camera.screen(*b, bounds),
                            ),
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(rgb([182, 66, 61])),
                        );
                    }
                    chain_badge = Some((endpoint, "Overlap · change direction".into(), false));
                }
            }
        }
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
                        ring_selection = Some(vec![id]);
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
                    ring_selection = Some(vec![id]);
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
                    smart = guides;
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
                && let Some(anchor) = owner.anchor(&preview)
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
        if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor) {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            drag.apply(preview.to_mut(), end, state.modifiers.shift());
            ring_selection = Some(drag.ids.clone());
        }
        if let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
            && self.tool == Tool::Ring
        {
            let radius = 10.0 / self.camera.zoom;
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let placement = |anchor, direction| {
                reshiki::editing::ring_placement(
                    &preview,
                    anchor,
                    self.ring_size,
                    self.aromatic_ring
                        || delocalized_ring_size(self.tool, self.ring_size, state.modifiers)
                            .is_some(),
                    radius,
                    direction,
                )
            };
            match &state.gesture {
                Some(Gesture::Ring { start, attached }) => {
                    let (anchor, direction) = ring_gesture(*start, end, *attached, radius);
                    match placement(anchor, direction) {
                        Ok((doc, ids)) => {
                            preview = Cow::Owned(doc);
                            ring_selection = Some(ids);
                        }
                        Err(reason) => rejection = Some(reason),
                    }
                }
                // Hovering where a click would attach shows only a rejection.
                None if self.doc.nearest(end, radius).is_some()
                    || reshiki::editing::nearest_bond(self.doc, end, radius).is_some() =>
                {
                    rejection = placement(end, None).err();
                }
                _ => {}
            }
        }
        if let Tool::RingPreset(preset) = self.tool
            && let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
            && delocalized_ring_size(self.tool, self.ring_size, state.modifiers).is_none()
        {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (anchor, direction) = if let Some(Gesture::Ring { start, .. }) = &state.gesture {
                ring_gesture(*start, end, true, 10. / self.camera.zoom)
            } else {
                (end, None)
            };
            let drawing = reshiki::rings::Drawing {
                preset,
                length: self.bond_drawing.length,
                alternate: state.modifiers.shift(),
                connect: state.modifiers.alt(),
            };
            match drawing.place(self.doc, anchor, direction, 10. / self.camera.zoom) {
                Ok((doc, ids)) => {
                    preview = Cow::Owned(doc);
                    for b in &mut preview.to_mut().bonds {
                        if self.doc.atom(b.a).is_none() || self.doc.atom(b.b).is_none() {
                            b.color = reshiki::palette::Color::Palette(
                                reshiki::palette::Hue::Teal,
                                reshiki::palette::Row::Strong,
                            );
                        }
                    }
                    ring_selection = Some(ids);
                    template_notice = Some((
                        format!(
                            "{preset} · Drag to orient · Alt connects by a bond · Escape cancels"
                        ),
                        true,
                    ));
                }
                Err(error) => {
                    let radius = 10. / self.camera.zoom;
                    template_notice = Some((error.into(), false));
                    rejection = Some(reshiki::editing::RingRejection {
                        message: error,
                        label: reshiki::editing::attachment_label(self.doc, anchor, radius),
                        outline: vec![],
                        atom: self.doc.nearest(anchor, radius),
                    });
                }
            }
        }
        if self.tool == Tool::Template
            && let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
        {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (anchor, direction) = if let Some(Gesture::Ring { start, .. }) = &state.gesture {
                self.template_gesture(*start, end, state.modifiers)
            } else {
                (end, None)
            };
            let placement = self
                .joining
                .map(|(joining, source_anchor)| {
                    joining.place(
                        anchor,
                        direction,
                        10. / self.camera.zoom,
                        source_anchor,
                        self.template_connection,
                    )
                })
                .or_else(|| {
                    self.template.map(|(template, source_anchor)| {
                        template
                            .place(
                                self.doc,
                                anchor,
                                direction,
                                10. / self.camera.zoom,
                                source_anchor,
                                self.template_connection,
                            )
                            .map_err(str::to_owned)
                    })
                });
            match placement {
                Some(Ok((document, ids))) => {
                    template_notice = Some((
                        if self.joining.is_some() {
                            "Move & attach preview · Click or release to join · Escape cancels"
                                .into()
                        } else {
                            format!(
                                "Preview · {} new objects ({} atoms) · Shift/Ctrl drag snaps 15° · Escape cancels",
                                document
                                    .all_ids()
                                    .len()
                                    .saturating_sub(self.doc.all_ids().len()),
                                document.atoms.len().saturating_sub(self.doc.atoms.len())
                            )
                        },
                        true,
                    ));
                    preview = Cow::Owned(document);
                    // Tint only transient objects. Commit calls the same pure
                    // placement operation again and retains saved/JACS colors.
                    let existing: std::collections::HashSet<_> = self
                        .joining
                        .map(|(j, _)| &j.base)
                        .unwrap_or(self.doc)
                        .all_ids()
                        .into_iter()
                        .collect();
                    use reshiki::palette::{Color as Paint, Hue, Row};
                    let tint = Paint::Palette(Hue::Teal, Row::Strong);
                    for atom in &mut preview.to_mut().atoms {
                        if !existing.contains(&atom.id) {
                            atom.text_style.get_or_insert_with(Default::default).color = tint;
                        }
                    }
                    for group in &mut preview.to_mut().abbreviations {
                        if !existing.contains(&group.anchor)
                            && let Some(style) = &mut group.label_style
                        {
                            style.color = tint;
                            group.label_color_override = true;
                        }
                    }
                    for bond in &mut preview.to_mut().bonds {
                        if !existing.contains(&bond.a) || !existing.contains(&bond.b) {
                            bond.color = tint;
                        }
                    }
                    for label in &mut preview.to_mut().annotations {
                        if !existing.contains(&label.id) {
                            label.format.style.color = tint;
                            for span in &mut label.format.spans {
                                span.style.color = tint;
                            }
                        }
                    }
                    for graphic in &mut preview.to_mut().graphics {
                        if !existing.contains(&graphic.id) {
                            graphic.style.stroke = tint;
                            if graphic.style.fill.is_some() {
                                graphic.style.fill = Some(Paint::Palette(Hue::Teal, Row::Tint));
                            }
                        }
                    }
                    ring_selection = Some(ids);
                }
                Some(Err(error)) => {
                    template_notice = Some((error.to_string(), false));
                    frame.stroke(
                        &Path::circle(self.camera.screen(anchor, bounds), 10.0),
                        Stroke::default()
                            .with_width(2.0)
                            .with_color(Color::from_rgb8(182, 66, 61)),
                    );
                }
                None => {}
            }
        }
        if let (Some(Gesture::Move { start, ids, .. }), Some(p)) = (&state.gesture, state.cursor) {
            let p = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (delta, guides) =
                self.move_delta(state, ids, World::new(p.x - start.x, p.y - start.y), bounds);
            if start.distance(p) < 1.0 / self.camera.zoom {
                ring_selection = Some(ids.clone());
            } else if self.copies(state.modifiers) {
                let part = state.scene.borrow_mut().copy(self.doc, ids);
                ring_selection = Some(reshiki::editing::append(preview.to_mut(), &part, delta));
                smart = guides;
            } else if state.scene.borrow_mut().whole_document(self.doc, ids) {
                // Moving every object cannot change their relative geometry,
                // chemical labels, crossing gaps or ring attachment targets.
                preview.to_mut().translate(ids, delta.x, delta.y);
                ring_selection = Some(ids.clone());
                translation = Some(delta);
            } else if let Some(snapped) =
                reshiki::editing::snap_ring(preview.to_mut(), ids, delta, 14.0 / self.camera.zoom)
            {
                // Fusing onto a ring wins over the guides, on release too.
                ring_selection = Some(snapped);
            } else {
                preview.to_mut().translate(ids, delta.x, delta.y);
                ring_selection = Some(ids.clone());
                smart = guides;
            }
        }
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
                ring_selection = Some(region_selection(
                    self.doc,
                    self.selected,
                    &polygon,
                    state.modifiers,
                ));
            }
        }
        if let Tool::RingPreset(_) = self.tool
            && let Some(size) = delocalized_ring_size(self.tool, self.ring_size, state.modifiers)
            && let Some(p) = state.cursor.filter(|p| bounds.contains(*p))
        {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (anchor, direction) = if let Some(Gesture::Ring { start, .. }) = &state.gesture {
                ring_gesture(*start, end, true, 10. / self.camera.zoom)
            } else {
                (end, None)
            };
            match reshiki::editing::ring_placement(
                &preview,
                anchor,
                size,
                true,
                10. / self.camera.zoom,
                direction,
            ) {
                Ok((doc, ids)) => {
                    preview = Cow::Owned(doc);
                    ring_selection = Some(ids);
                }
                Err(reason) => rejection = Some(reason),
            }
        }
        if let (
            Some(Gesture::Draw {
                start,
                id: Some(id),
            }),
            Some(cursor),
            Tool::Atom,
        ) = (&state.gesture, state.cursor, self.tool)
        {
            let end = self
                .camera
                .world(Point::new(cursor.x - bounds.x, cursor.y - bounds.y), bounds);
            if start.distance(end) >= 3. / self.camera.zoom
                && bounds.contains(cursor)
                && let Some(origin) = self.doc.atom(*id).map(|a| a.position)
            {
                let (end, target) = bond_target_with(
                    self.doc,
                    origin,
                    end,
                    Some(*id),
                    12. / self.camera.zoom,
                    self.bond_drawing.unconstrained(state.modifiers.alt()),
                );
                let drawing = self.bond_drawing.unconstrained(state.modifiers.alt());
                let result = if let Some(endpoint) = target
                    .is_none()
                    .then(|| {
                        plane_endpoint(
                            self.doc,
                            *id,
                            self.camera.world(
                                Point::new(cursor.x - bounds.x, cursor.y - bounds.y),
                                bounds,
                            ),
                            drawing,
                        )
                    })
                    .flatten()
                {
                    reshiki::projection::growth::place(
                        self.doc,
                        *id,
                        endpoint,
                        self.element,
                        reshiki::bonds::BondPreset::Single,
                    )
                } else {
                    reshiki::editing::add_bonded_atom(self.doc, *id, end, target, self.element)
                };
                if let Ok((drawing, added)) = result {
                    preview = Cow::Owned(drawing);
                    ring_selection = Some(vec![*id, added]);
                }
            }
        }
        if let (Some(Gesture::Draw { start, id }), Some(cursor), Some(preset)) =
            (&state.gesture, state.cursor, self.tool.bond_preset())
        {
            let end = self
                .camera
                .world(Point::new(cursor.x - bounds.x, cursor.y - bounds.y), bounds);
            if start.distance(end) >= 3.0 / self.camera.zoom {
                let origin = id
                    .and_then(|id| self.doc.atom(id).map(|a| a.position))
                    .unwrap_or(*start);
                let (end, target) = bond_target_with(
                    self.doc,
                    origin,
                    end,
                    *id,
                    12.0 / self.camera.zoom,
                    self.bond_drawing.unconstrained(state.modifiers.alt()),
                );
                if preset != reshiki::bonds::BondPreset::Dotted
                    || id
                        .zip(target)
                        .is_some_and(|(a, b)| reshiki::bonds::hydrogen_endpoints(self.doc, a, b))
                {
                    let plane = id.filter(|_| target.is_none()).and_then(|id| {
                        plane_endpoint(
                            self.doc,
                            id,
                            self.camera.world(
                                Point::new(cursor.x - bounds.x, cursor.y - bounds.y),
                                bounds,
                            ),
                            self.bond_drawing.unconstrained(state.modifiers.alt()),
                        )
                        .map(|endpoint| (id, endpoint))
                    });
                    if let Some((id, endpoint)) = plane {
                        if let Ok((drawing, added)) =
                            reshiki::projection::growth::place(self.doc, id, endpoint, "C", preset)
                        {
                            preview = Cow::Owned(drawing);
                            ring_selection = Some(vec![id, added]);
                        }
                    } else {
                        let a = id.unwrap_or_else(|| preview.to_mut().add_atom("C", origin));
                        let z = target.unwrap_or_else(|| preview.to_mut().add_atom("C", end));
                        preset.place(preview.to_mut(), a, z);
                        ring_selection = Some(vec![a, z]);
                    }
                } else {
                    frame.stroke(
                        &Path::circle(self.camera.screen(end, bounds), 8.0),
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([182, 66, 61])),
                    );
                }
            }
        }
        if let Some(id) = self.hidden_annotation {
            preview.to_mut().annotations.retain(|a| a.id != id);
        }
        let selected = ring_selection.as_deref().unwrap_or(self.selected);
        let cached_camera = (self.hidden_annotation.is_none()
            && (translation.is_some() || *preview == *self.doc))
            .then(|| {
                let delta = translation.unwrap_or_default();
                Camera {
                    center: self.camera.center.offset(-delta.x, -delta.y),
                    ..self.camera
                }
            });
        if let Some(camera) = cached_camera {
            let (markers, scene) = state.scene.borrow_mut().render(self.doc, selected);
            markers.draw(frame, camera, bounds, true);
            draw_primitives(frame, &scene, camera, bounds, 0.);
        } else {
            markers::Markers::new(&preview, selected).draw(frame, self.camera, bounds, true);
            draw_document(frame, &preview, self.camera, bounds);
        }
        // Editing aids stay out of the shared scene used by figure/Office export.
        for atom in reshiki::attachments::editor_markers(&preview) {
            let center = self.camera.screen(atom.position, bounds);
            let stroke = Stroke::default()
                .with_width(1.2)
                .with_color(rgb([19, 135, 116]));
            frame.stroke(&Path::circle(center, 4.), stroke);
            for delta in [Vector::new(6., 0.), Vector::new(0., 6.)] {
                frame.stroke(&Path::line(center - delta, center + delta), stroke);
            }
        }
        if selected.len() == 1
            && (self.tool.selects() || matches!(self.tool, Tool::Arrow | Tool::EditPoints))
        {
            for a in preview.arrows.iter().filter(|a| selected.contains(&a.id)) {
                for (i, p) in a.handles().into_iter().enumerate() {
                    let p = self.camera.screen(p, bounds);
                    let path = if i == 2 {
                        Path::rectangle(p - Vector::new(4., 4.), iced::Size::new(8., 8.))
                    } else {
                        Path::circle(p, 4.)
                    };
                    frame.fill(&path, Color::WHITE);
                    frame.stroke(
                        &path,
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([19, 135, 116])),
                    );
                }
            }
        }
        if let Some((content, valid)) = template_notice {
            let position = Point::new(14., (bounds.height - 30.).max(4.));
            frame.fill_rectangle(
                position - Vector::new(5., 4.),
                iced::Size::new((bounds.width - 18.).max(1.), 25.),
                rgb(if valid {
                    [225, 242, 237]
                } else {
                    [253, 235, 233]
                }),
            );
            frame.fill_text(canvas::Text {
                content,
                position,
                color: rgb(if valid { [30, 100, 85] } else { [160, 50, 45] }),
                size: 12.into(),
                ..Default::default()
            });
        }
        if let Some((point, content, valid)) = chain_badge {
            badge(
                frame,
                bounds,
                self.camera.screen(point, bounds),
                content,
                valid,
            );
        }
        // A rejected ring shows where it would go, and why, at the pointer;
        // the status bar gets the full message if the user clicks anyway.
        if let (Some(rejection), Some(p)) = (rejection, state.cursor) {
            let error = rgb([182, 66, 61]);
            let pointer = Point::new(p.x - bounds.x, p.y - bounds.y);
            if let [first, rest @ ..] = rejection.outline.as_slice() {
                let path = Path::new(|b| {
                    b.move_to(self.camera.screen(*first, bounds));
                    for point in rest {
                        b.line_to(self.camera.screen(*point, bounds));
                    }
                    b.close();
                });
                frame.fill(&path, Color::from_rgba8(182, 66, 61, 0.08));
                frame.stroke(&path, Stroke::default().with_width(2.).with_color(error));
            }
            let marker = rejection
                .atom
                .and_then(|id| self.doc.atom(id))
                .map(|a| self.camera.screen(a.position, bounds))
                .or(rejection.outline.is_empty().then_some(pointer));
            if let Some(center) = marker {
                frame.stroke(
                    &Path::circle(center, 10.),
                    Stroke::default().with_width(2.).with_color(error),
                );
            }
            badge(frame, bounds, pointer, rejection.label, false);
        }
        if self.tool == Tool::EditPoints {
            for indicator in reshiki::atom_labels::indicators(&preview)
                .into_iter()
                .filter(|i| i.owner.selected(selected))
            {
                if let Some(anchor) = indicator.owner.anchor(&preview) {
                    let center = self.camera.screen(indicator.center, bounds);
                    frame.stroke(
                        &Path::line(self.camera.screen(anchor, bounds), center),
                        Stroke::default().with_color(rgb([120, 170, 155])),
                    );
                    let path = Path::circle(center, 4.);
                    frame.fill(&path, Color::WHITE);
                    frame.stroke(
                        &path,
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([19, 135, 116])),
                    );
                }
            }
            for a in preview
                .atoms
                .iter()
                .filter(|a| selected.contains(&a.id) && preview.atom_visible(a.id))
            {
                for m in &a.marks {
                    let center = self
                        .camera
                        .screen(a.position.offset(m.offset.x, m.offset.y), bounds);
                    frame.stroke(
                        &Path::line(self.camera.screen(a.position, bounds), center),
                        Stroke::default().with_color(rgb([120, 170, 155])),
                    );
                    let path = Path::circle(center, 4.);
                    frame.fill(&path, Color::WHITE);
                    frame.stroke(
                        &path,
                        Stroke::default()
                            .with_width(1.5)
                            .with_color(rgb([19, 135, 116])),
                    );
                }
            }
            for graphic in preview.graphics.iter().filter(|g| selected.contains(&g.id)) {
                if graphic.kind == reshiki::graphics::GraphicKind::Arc {
                    for p in graphic.edit_points() {
                        let path = Path::circle(self.camera.screen(p, bounds), 5.0);
                        frame.fill(&path, Color::WHITE);
                        frame.stroke(
                            &path,
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(rgb([19, 135, 116])),
                        );
                    }
                    continue;
                }
                let mut anchor = World::default();
                for c in graphic.commands() {
                    let points = c.points();
                    if let PathCommand::Cubic(a, b, end) = c {
                        for (from, to) in [(anchor, a), (b, end)] {
                            frame.stroke(
                                &Path::line(
                                    self.camera.screen(from, bounds),
                                    self.camera.screen(to, bounds),
                                ),
                                Stroke::default().with_color(rgb([19, 135, 116])),
                            );
                        }
                        anchor = end;
                    } else if let Some(p) = points.last() {
                        anchor = *p;
                    }
                    for p in points {
                        let path = Path::circle(self.camera.screen(p, bounds), 4.0);
                        frame.fill(&path, Color::WHITE);
                        frame.stroke(
                            &path,
                            Stroke::default()
                                .with_width(1.5)
                                .with_color(rgb([19, 135, 116])),
                        );
                    }
                }
            }
        }
        if let Some(p) = state.cursor {
            let p = Point::new(p.x - bounds.x, p.y - bounds.y);
            match &state.gesture {
                Some(Gesture::Draw { start, id })
                    if self.tool.bond_preset().is_none()
                        && self.tool != Tool::Arrow
                        && self.tool != Tool::Atom =>
                {
                    let origin = id
                        .and_then(|id| self.doc.atom(id).map(|a| a.position))
                        .unwrap_or(*start);
                    let end = self.camera.world(p, bounds);
                    // A preview exists only during an actual drag, never while idle.
                    if start.distance(end) < 3.0 / self.camera.zoom {
                        return;
                    }
                    let end = if self.tool == Tool::Arrow {
                        end
                    } else {
                        bond_target_with(
                            self.doc,
                            origin,
                            end,
                            *id,
                            12.0 / self.camera.zoom,
                            self.bond_drawing.unconstrained(state.modifiers.alt()),
                        )
                        .0
                    };
                    frame.stroke(
                        &Path::line(
                            self.camera.screen(origin, bounds),
                            self.camera.screen(end, bounds),
                        ),
                        Stroke::default()
                            .with_width(2.0)
                            .with_color(rgb([19, 135, 116])),
                    );
                }
                Some(Gesture::Lasso { points }) => {
                    if let Some(first) = points.first() {
                        let path = Path::new(|b| {
                            b.move_to(self.camera.screen(*first, bounds));
                            for point in points.iter().skip(1) {
                                b.line_to(self.camera.screen(*point, bounds));
                            }
                            b.line_to(p);
                            b.close();
                        });
                        frame.fill(&path, Color::from_rgba8(19, 135, 116, 0.08));
                        frame.stroke(&path, Stroke::default().with_color(rgb([19, 135, 116])));
                    }
                }
                Some(Gesture::Select { start }) => {
                    let a = self.camera.screen(*start, bounds);
                    let lo = Point::new(a.x.min(p.x), a.y.min(p.y));
                    let size = iced::Size::new((a.x - p.x).abs(), (a.y - p.y).abs());
                    frame.fill_rectangle(lo, size, Color::from_rgba8(19, 135, 116, 0.08));
                    frame.stroke(
                        &Path::rectangle(lo, size),
                        Stroke::default().with_color(rgb([19, 135, 116])),
                    );
                }
                None if cursor.is_over(bounds) && self.tool != Tool::Erase => {
                    let point = self.camera.world(p, bounds);
                    let mut hit = hit_selection(self.doc, point, 10.0 / self.camera.zoom);
                    if self.tool.selects() && hit.is_empty() {
                        hit = reshiki::editing::ring_at(self.doc, point).unwrap_or_default();
                    }
                    if self.tool.selects() && !state.modifiers.alt() {
                        hit = self.doc.expand_groups(&hit);
                    }
                    markers::Markers::hover(self.doc, &hit).draw(frame, self.camera, bounds, false);
                }
                _ => {}
            }
        }
        if self.tool == Tool::Erase
            && cursor.is_over(bounds)
            && let Some(p) = state.cursor
        {
            frame.stroke(
                &Path::circle(Point::new(p.x - bounds.x, p.y - bounds.y), 7.),
                Stroke::default()
                    .with_width(1.)
                    .with_color(rgb([180, 66, 66])),
            );
        }
        if self.tool.selects() && self.optimizer.is_none() {
            if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor)
                && matches!(drag.handle, Handle::Rotate)
            {
                let end = self
                    .camera
                    .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
                drag.selection
                    .draw(frame, drag.values(end, state.modifiers.shift()).1);
            } else {
                let selection = if cached_camera.is_some() {
                    state
                        .scene
                        .borrow_mut()
                        .selection(self.doc, selected, self.camera, bounds)
                        .map(|selection| selection.translated(translation.unwrap_or_default()))
                } else {
                    SelectionBox::new(&preview, selected, self.camera, bounds)
                };
                if let Some(selection) = selection {
                    selection.draw(frame, 0.0);
                }
            }
            if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor)
                && let Handle::Edge(i) = drag.handle
            {
                let end = self
                    .camera
                    .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
                let (scale, _) = drag.values(end, false);
                let position = Point::new(
                    (p.x - bounds.x + 16.).clamp(4., (bounds.width - 120.).max(4.)),
                    (p.y - bounds.y + 18.).clamp(4., (bounds.height - 28.).max(4.)),
                );
                frame.fill_rectangle(
                    position - Vector::new(4., 3.),
                    iced::Size::new(116., 23.),
                    Color::WHITE,
                );
                frame.fill_text(canvas::Text {
                    content: format!(
                        "{} {:.0}%",
                        if i.is_multiple_of(2) {
                            "Height"
                        } else {
                            "Width"
                        },
                        scale * 100.
                    ),
                    position,
                    color: rgb([30, 100, 85]),
                    size: 12.into(),
                    ..Default::default()
                });
            }
        }
        if self.tool == Tool::Tilt
            && let Some((lo, hi)) = reshiki::scene::selection_bounds(&preview, selected)
        {
            let lo = self.camera.screen(lo, bounds);
            let hi = self.camera.screen(hi, bounds);
            frame.stroke(
                &Path::rectangle(
                    Point::new(lo.x - 7., lo.y - 7.),
                    iced::Size::new(hi.x - lo.x + 14., hi.y - lo.y + 14.),
                ),
                Stroke::default()
                    .with_width(1.)
                    .with_color(rgb([65, 136, 119])),
            );
        }
        // Above the selection, so that gap labels stay legible.
        smart_guides::draw(
            frame,
            &smart,
            self.camera,
            bounds,
            self.guides.unit,
            self.doc.canvas_theme.is_dark(),
        );
    }
}

/// A one-line label beside the pointer at `p`, kept inside the canvas.
fn badge(
    frame: &mut layered::Frame<'_>,
    bounds: Rectangle,
    p: Point,
    content: String,
    valid: bool,
) {
    let width = crate::app::text_width(&content, 12.) + 10.;
    let position = Point::new(
        (p.x + 14.).clamp(4., (bounds.width - width - 4.).max(4.)),
        (p.y + 18.).clamp(20., (bounds.height - 30.).max(20.)),
    );
    frame.fill_rectangle(
        position - Vector::new(5., 4.),
        iced::Size::new(width, 24.),
        rgb(if valid {
            [225, 242, 237]
        } else {
            [253, 235, 233]
        }),
    );
    frame.fill_text(canvas::Text {
        content,
        position,
        color: rgb(if valid { [30, 100, 85] } else { [160, 50, 45] }),
        size: 12.into(),
        font: iced::Font::with_name(reshiki::style::ui_font_family()),
        ..Default::default()
    });
}

fn ring_gesture(start: World, end: World, attached: bool, radius: f32) -> (World, Option<World>) {
    if attached {
        (start, (start.distance(end) > radius).then_some(end))
    } else {
        (end, None)
    }
}

/// Noninteractive thumbnail rendered from the same molecule and scene as placement.
pub struct TemplateThumbnail<'a>(pub &'a Document);
impl<Message> canvas::Program<Message> for TemplateThumbnail<'_> {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame =
            layered::Frame::new(renderer, bounds.size()).with_canvas(self.0.canvas_theme);
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        let (lo, hi) = reshiki::scene::selection_bounds(self.0, &self.0.all_ids())
            .unwrap_or_else(|| self.0.bounds());
        let camera = Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - (bounds.width * 0.16).min(24.0)) / (hi.x - lo.x).max(60.0))
                .min((bounds.height - (bounds.height * 0.14).min(20.0)) / (hi.y - lo.y).max(50.0))
                .min(0.85),
        };
        draw_document(&mut frame, self.0, camera, bounds);
        frame.finish()
    }
}

/// Owns a temporary diagram for a tool inspector preview.
pub struct DrawingThumbnail(pub Document);
impl<Message> canvas::Program<Message> for DrawingThumbnail {
    type State = ();
    fn draw(
        &self,
        state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        <TemplateThumbnail<'_> as canvas::Program<Message>>::draw(
            &TemplateThumbnail(&self.0),
            state,
            renderer,
            theme,
            bounds,
            cursor,
        )
    }
}

/// The source diagram is a real hit-tested canvas, so anchors identify the
/// exact atom/bond the user picked rather than a nearest compatible substitute.
pub struct TemplateAnchorPreview<'a> {
    pub document: &'a Document,
    pub anchor: reshiki::templates::Anchor,
}
impl TemplateAnchorPreview<'_> {
    fn camera(&self, bounds: Rectangle) -> Camera {
        let (lo, hi) = reshiki::scene::selection_bounds(self.document, &self.document.all_ids())
            .unwrap_or_else(|| self.document.bounds());
        Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - 30.) / (hi.x - lo.x).max(60.))
                .min((bounds.height - 30.) / (hi.y - lo.y).max(50.))
                .min(1.2),
        }
    }
    fn hit(&self, p: Point, bounds: Rectangle) -> Option<reshiki::templates::Anchor> {
        let camera = self.camera(bounds);
        let p = camera.world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
        self.document
            .nearest(p, 8. / camera.zoom)
            .map(reshiki::templates::Anchor::Atom)
            .or_else(|| {
                reshiki::editing::nearest_bond(self.document, p, 6. / camera.zoom)
                    .and_then(|i| self.document.bonds.get(i))
                    .map(|b| reshiki::templates::Anchor::Bond(b.a, b.b))
            })
    }
}
impl canvas::Program<reshiki::templates::Anchor> for TemplateAnchorPreview<'_> {
    type State = Option<reshiki::templates::Anchor>;
    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<reshiki::templates::Anchor>> {
        let p = match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => Some(*position),
            _ => cursor.position(),
        };
        let hit = p
            .filter(|p| bounds.contains(*p))
            .and_then(|p| self.hit(p, bounds));
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                hit.map(|anchor| Action::publish(anchor).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if *state != hit {
                    *state = hit;
                    Some(Action::request_redraw())
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    fn mouse_interaction(
        &self,
        _: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor
            .position()
            .filter(|p| bounds.contains(*p))
            .and_then(|p| self.hit(p, bounds))
            .is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame =
            layered::Frame::new(renderer, bounds.size()).with_canvas(self.document.canvas_theme);
        let camera = self.camera(bounds);
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        draw_document(&mut frame, self.document, camera, bounds);
        for (anchor, color) in [
            (Some(self.anchor), Color::from_rgb8(17, 126, 108)),
            (*state, Color::from_rgba8(17, 126, 108, 0.45)),
        ] {
            match anchor {
                Some(reshiki::templates::Anchor::Atom(id)) => {
                    if let Some(a) = self.document.atom(id) {
                        frame.stroke(
                            &Path::circle(camera.screen(a.position, bounds), 9.),
                            Stroke::default().with_color(color).with_width(2.),
                        );
                    }
                }
                Some(reshiki::templates::Anchor::Bond(a, b)) => {
                    if let (Some(a), Some(b)) = (self.document.atom(a), self.document.atom(b)) {
                        frame.stroke(
                            &Path::line(
                                camera.screen(a.position, bounds),
                                camera.screen(b.position, bounds),
                            ),
                            Stroke::default().with_color(color).with_width(5.),
                        );
                    }
                }
                _ => {}
            }
        }
        frame.finish()
    }
}

fn arrow_endpoint(start: World, cursor: World, fixed_angles: bool) -> World {
    if !fixed_angles {
        return cursor;
    }
    let step = std::f32::consts::PI / 12.;
    let angle = ((cursor.y - start.y).atan2(cursor.x - start.x) / step).round() * step;
    let length = start.distance(cursor);
    start.offset(length * angle.cos(), length * angle.sin())
}

pub struct ArrowPreview {
    pub arrow: reshiki::document::Arrow,
}
impl canvas::Program<crate::app::Message> for ArrowPreview {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let mut doc = Document::default();
        doc.arrows.push(self.arrow.clone());
        let (lo, hi) = self.arrow.bounds();
        let camera = Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - 22.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 18.) / (hi.y - lo.y).max(1.))
                .min(1.4),
        };
        draw_document(&mut frame, &doc, camera, bounds);
        frame.finish()
    }
}

/// The inspector uses the same geometry and phase fills as the drawing/export.
pub struct ScientificPreview(pub reshiki::graphics::Graphic);
impl canvas::Program<crate::app::Message> for ScientificPreview {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let (lo, hi) = self.0.bounds();
        let camera = Camera {
            center: World::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5),
            zoom: ((bounds.width - 24.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 20.) / (hi.y - lo.y).max(1.))
                .min(1.4),
        };
        let doc = Document {
            graphics: vec![self.0.clone()],
            ..Document::default()
        };
        draw_document(&mut frame, &doc, camera, bounds);
        frame.finish()
    }
}

/// Shared, read-only preview for palettes and reviewed drawing proposals.
#[derive(Default)]
pub struct PreviewState(std::cell::RefCell<Option<PreviewCache>>);
struct PreviewCache {
    document: Document,
    size: iced::Size,
    geometry: Vec<<Geometry as iced::advanced::graphics::cache::Cached>::Cache>,
}
pub struct DrawingPreview<'a>(pub &'a Document);
impl canvas::Program<crate::app::Message> for DrawingPreview<'_> {
    type State = PreviewState;
    fn draw(
        &self,
        state: &PreviewState,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        use iced::advanced::graphics::cache::Cached;
        let mut cache = state.0.borrow_mut();
        if let Some(cached) = cache.as_ref()
            && cached.size == bounds.size()
            && &cached.document == self.0
        {
            return cached.geometry.iter().map(Cached::load).collect();
        }
        let mut frame =
            layered::Frame::new(renderer, bounds.size()).with_canvas(self.0.canvas_theme);
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Color::WHITE);
        let (lo, hi) =
            reshiki::scene::selection_bounds(self.0, &self.0.all_ids()).unwrap_or_default();
        let camera = Camera {
            center: World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
            zoom: ((bounds.width - 24.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 24.) / (hi.y - lo.y).max(1.))
                .clamp(0.001, 1.4),
        };
        draw_document_with_minimum_stroke(&mut frame, self.0, camera, bounds, 0.6);
        let geometry: Vec<_> = frame
            .finish()
            .into_iter()
            .map(|g| g.cache(iced::advanced::graphics::cache::Group::unique(), None))
            .collect();
        let result = geometry.iter().map(Cached::load).collect();
        *cache = Some(PreviewCache {
            document: self.0.clone(),
            size: bounds.size(),
            geometry,
        });
        result
    }
}

/// Palette strokes remain legible even when a large structure is fitted into a tile.
pub struct PalettePreview(pub Document);
impl canvas::Program<crate::app::Message> for PalettePreview {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let (lo, hi) =
            reshiki::scene::selection_bounds(&self.0, &self.0.all_ids()).unwrap_or_default();
        let camera = Camera {
            center: World::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.),
            zoom: ((bounds.width - 14.) / (hi.x - lo.x).max(1.))
                .min((bounds.height - 14.) / (hi.y - lo.y).max(1.))
                .clamp(0.001, 1.4),
        };
        draw_document_with_minimum_stroke(&mut frame, &self.0, camera, bounds, 1.5);
        frame.finish()
    }
}

pub struct OwnedDrawingPreview(pub Document);
impl canvas::Program<crate::app::Message> for OwnedDrawingPreview {
    type State = PreviewState;
    fn draw(
        &self,
        state: &PreviewState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        DrawingPreview(&self.0).draw(state, renderer, theme, bounds, cursor)
    }
}

/// Every edit a Select drag of `selected` from `from` to `to` publishes, with
/// smart guides on, at zoom 1.
#[cfg(test)]
pub(crate) fn select_drag(
    doc: &Document,
    selected: &[u64],
    from: World,
    to: World,
    modifiers: iced::keyboard::Modifiers,
) -> Vec<Edit> {
    let mut canvas = tests::chain_canvas(doc, ChainMode::Straight);
    canvas.tool = Tool::Select;
    canvas.selected = selected;
    tests::guided_drag(&canvas, from, to, modifiers).0
}

#[cfg(test)]
mod tests;
