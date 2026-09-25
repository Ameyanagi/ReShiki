use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, Vector, mouse};
use reshiki::{
    chains::{self, BondDrawing, ChainDrawing, ChainMode},
    document::{Document, Point as World},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
    scene::{Primitive, primitives},
};
mod dashes;
pub mod guides;
pub mod layered;
mod movement;
mod pages;
mod selection;
pub(crate) mod tilt;
use selection::{Handle, SelectionBox, TransformDrag};

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
                "Bonded drags use Length/Angles · Option/Alt frees movement · Drag a ring edge to fuse"
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
    Move(Vec<u64>, f32, f32),
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
            zoom: 1.7,
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
    gesture: Option<Gesture>,
    cursor: Option<Point>,
    last_click: Option<(std::time::Instant, u64)>,
    modifiers: iced::keyboard::Modifiers,
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
// Cmd on macOS, Ctrl elsewhere. Chairs and Haworth projections retain their geometry.
fn delocalized_ring_size(tool: Tool, size: u8, modifiers: iced::keyboard::Modifiers) -> Option<u8> {
    let held = if cfg!(target_os = "macos") {
        modifiers.logo()
    } else {
        modifiers.control()
    };
    if !held {
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
    pub joining: Option<&'a reshiki::joining::Prepared>,
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
    pub ring_size: u8,
    pub aromatic_ring: bool,
    pub template_connection: reshiki::templates::Connection,
    pub template: Option<(&'a Document, reshiki::templates::Anchor)>,
    pub arrow_preset: reshiki::arrows::Preset,
    pub arrow_style: &'a reshiki::arrows::ArrowStyle,
    pub orbital_phase: reshiki::scientific::Phase,
    pub phase_flipped: bool,
    pub attach_symbols: bool,
    pub graphic_constrain: bool,
    pub graphic_style: &'a GraphicStyle,
    pub bracket_sides: BracketSides,
}
fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

impl MoleculeCanvas<'_> {
    fn template_gesture(
        &self,
        start: World,
        end: World,
        modifiers: iced::keyboard::Modifiers,
    ) -> (World, Option<World>) {
        let doc = self.joining.map(|j| &j.base).unwrap_or(self.doc);
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
        match event {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(_)) => {
                Some(Action::request_redraw())
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => {
                if matches!(state.gesture.take(), Some(Gesture::Erase { .. })) {
                    return Some(Action::publish(Edit::EraseEnd));
                }
                Some(Action::request_redraw())
            }
            Event::Window(iced::window::Event::Unfocused) => {
                let erasing = matches!(state.gesture.take(), Some(Gesture::Erase { .. }));
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
                state.gesture = None;
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
                        if let Some((index, _)) = g
                            .commands()
                            .iter()
                            .flat_map(PathCommand::points)
                            .enumerate()
                            .find(|(_, q)| q.distance(p) < 8.0 / self.camera.zoom)
                        {
                            state.gesture = Some(Gesture::GraphicPoint { id: g.id, index });
                            return Some(Action::request_redraw().and_capture());
                        }
                    }
                }
                if self.tool.selects()
                    && let Some(selection) =
                        SelectionBox::new(self.doc, self.selected, self.camera, bounds)
                    && let Some(handle) = selection.hit(point?)
                {
                    state.last_click = None;
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
                let gesture = state.gesture.take()?;
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
                                .map(|a| {
                                    arrow_endpoint(
                                        if index == 0 { a.end } else { a.start },
                                        p,
                                        self.bond_drawing.fixed_angles && !state.modifiers.alt(),
                                    )
                                })
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
                    Gesture::Transform(drag) => drag.into_edit(p, state.modifiers.shift()),
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
                                        Action::publish(Edit::Select(connected)).and_capture(),
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
                            let delta = movement::delta(
                                self.doc,
                                &ids,
                                World::new(p.x - start.x, p.y - start.y),
                                self.bond_drawing.unconstrained(state.modifiers.alt()),
                            );
                            Edit::Move(ids, delta.x, delta.y)
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
                Some(Action::publish(edit).and_capture())
            }
            _ => None,
        }
    }
    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        _theme: &Theme,
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
        );
        self.draw_paper(&mut frame, state, paper, cursor);
        let mut layers = frame.finish();
        let mut frame = Frame::new(renderer, bounds.size());
        let pointer = state
            .cursor
            .filter(|p| paper.contains(*p))
            .map(|p| Point::new(p.x - paper.x, p.y - paper.y));
        self.guides.draw_rulers(&mut frame, self.camera, pointer);
        layers.push(frame.into_geometry());
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
                    SelectionBox::new(self.doc, self.selected, self.camera, bounds)
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
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgb8(253, 253, 250),
        );
        if let Some(layout) = &self.doc.page_layout {
            pages::draw(frame, layout, self.camera, bounds);
        }
        if self.grid {
            let spacing = 28.0 * self.camera.zoom;
            if spacing > 9.0 {
                let origin = self.camera.screen(World::default(), bounds);
                let mut x = origin.x.rem_euclid(spacing);
                while x < bounds.width {
                    let mut y = origin.y.rem_euclid(spacing);
                    while y < bounds.height {
                        frame.fill(
                            &Path::circle(Point::new(x, y), 0.7),
                            Color::from_rgb8(218, 226, 224),
                        );
                        y += spacing;
                    }
                    x += spacing;
                }
            }
        }
        self.guides.draw_crosshair(
            frame,
            self.camera,
            state
                .cursor
                .filter(|p| bounds.contains(*p))
                .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y)),
        );
        let mut preview = self.joining.map(|j| &j.base).unwrap_or(self.doc).clone();
        let mut ring_selection = None;
        let mut chain_badge = None;
        let mut template_notice = None;
        if let (Some(Gesture::Tilt(drag)), Some(p)) = (&state.gesture, state.cursor)
            && self.tool == Tool::Tilt
        {
            let (x, y) = drag.angles(
                Point::new(p.x - bounds.x, p.y - bounds.y),
                state.modifiers.shift(),
            );
            tilt::apply(&mut preview, &drag.ids, x, y);
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
                    preview = doc;
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
                        &mut preview,
                        *start,
                        end,
                        state.modifiers.shift(),
                        10. / self.camera.zoom,
                    ) {
                        ring_selection = Some(vec![id]);
                    }
                } else {
                    let id = preview.next_id();
                    preview.graphics.push(Graphic::dragged(
                        id,
                        kind,
                        *start,
                        end,
                        self.graphic_style.clone(),
                        self.bracket_sides,
                        state.modifiers.shift() || self.graphic_constrain,
                    ));
                    ring_selection = Some(vec![id]);
                }
            }
            if let Some(Gesture::AtomMark { id, index }) = &state.gesture
                && let Some(a) = preview.atom_mut(*id)
                && let Some(mark) = a.marks.get_mut(*index)
            {
                mark.offset = World::new(end.x - a.position.x, end.y - a.position.y);
            }
            if let Some(Gesture::ArrowHandle { id, index }) = &state.gesture
                && let Some(a) = preview.arrows.iter_mut().find(|a| a.id == *id)
            {
                let end = if *index < 2 {
                    arrow_endpoint(
                        if *index == 0 { a.end } else { a.start },
                        end,
                        self.bond_drawing.fixed_angles && !state.modifiers.alt(),
                    )
                } else {
                    end
                };
                a.edit_handle(*index, end);
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
                preview.arrows.push(reshiki::document::Arrow::new(
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
                    &mut preview,
                    Some(World::new(end.x - anchor.x, end.y - anchor.y)),
                );
            }
            if let Some(Gesture::GraphicPoint { id, index }) = &state.gesture
                && let Some(g) = preview.graphics.iter_mut().find(|g| g.id == *id)
            {
                g.edit_point(*index, end);
            }
        }
        if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor) {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            drag.apply(&mut preview, end, state.modifiers.shift());
            ring_selection = Some(drag.ids.clone());
        }
        if let (Some(Gesture::Ring { start, attached }), Some(p)) = (&state.gesture, state.cursor)
            && bounds.contains(p)
            && self.tool == Tool::Ring
        {
            let end = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let (anchor, direction) = ring_gesture(*start, end, *attached, 10.0 / self.camera.zoom);
            ring_selection = Some(reshiki::editing::ring_oriented(
                &mut preview,
                anchor,
                self.ring_size,
                self.aromatic_ring
                    || delocalized_ring_size(self.tool, self.ring_size, state.modifiers).is_some(),
                10.0 / self.camera.zoom,
                direction,
            ));
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
                    preview = doc;
                    for b in &mut preview.bonds {
                        if self.doc.atom(b.a).is_none() || self.doc.atom(b.b).is_none() {
                            b.color = [17, 126, 108];
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
                Err(error) => template_notice = Some((error.into(), false)),
            }
        }
        if self.tool == Tool::Template
            && let Some((template, source_anchor)) = self.template
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
            let placement = if let Some(joining) = self.joining {
                joining.place(
                    anchor,
                    direction,
                    10. / self.camera.zoom,
                    source_anchor,
                    self.template_connection,
                )
            } else {
                reshiki::templates::place_with_mode(
                    self.doc,
                    template,
                    anchor,
                    direction,
                    10. / self.camera.zoom,
                    source_anchor,
                    self.template_connection,
                )
                .map_err(str::to_owned)
            };
            match placement {
                Ok((document, ids)) => {
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
                    preview = document;
                    // Tint only transient objects. Commit calls the same pure
                    // placement operation again and retains saved/JACS colors.
                    let existing: std::collections::HashSet<_> = self
                        .joining
                        .map(|j| &j.base)
                        .unwrap_or(self.doc)
                        .all_ids()
                        .into_iter()
                        .collect();
                    let tint = [17, 126, 108];
                    for atom in &mut preview.atoms {
                        if !existing.contains(&atom.id) {
                            atom.text_style.get_or_insert_with(Default::default).color = tint;
                        }
                    }
                    for bond in &mut preview.bonds {
                        if !existing.contains(&bond.a) || !existing.contains(&bond.b) {
                            bond.color = tint;
                        }
                    }
                    for label in &mut preview.annotations {
                        if !existing.contains(&label.id) {
                            label.format.style.color = tint;
                            for span in &mut label.format.spans {
                                span.style.color = tint;
                            }
                        }
                    }
                    for graphic in &mut preview.graphics {
                        if !existing.contains(&graphic.id) {
                            graphic.style.stroke = tint;
                            if graphic.style.fill.is_some() {
                                graphic.style.fill = Some([220, 239, 233]);
                            }
                        }
                    }
                    ring_selection = Some(ids);
                }
                Err(error) => {
                    template_notice = Some((error.to_string(), false));
                    frame.stroke(
                        &Path::circle(self.camera.screen(anchor, bounds), 10.0),
                        Stroke::default()
                            .with_width(2.0)
                            .with_color(Color::from_rgb8(182, 66, 61)),
                    );
                }
            }
        }
        if let (Some(Gesture::Move { start, ids, .. }), Some(p)) = (&state.gesture, state.cursor) {
            let p = self
                .camera
                .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
            let delta = movement::delta(
                self.doc,
                ids,
                World::new(p.x - start.x, p.y - start.y),
                self.bond_drawing.unconstrained(state.modifiers.alt()),
            );
            if start.distance(p) < 1.0 / self.camera.zoom {
                ring_selection = Some(ids.clone());
            } else if let Some(snapped) =
                reshiki::editing::snap_ring(&mut preview, ids, delta, 14.0 / self.camera.zoom)
            {
                ring_selection = Some(snapped);
            } else {
                preview.translate(ids, delta.x, delta.y);
                ring_selection = Some(ids.clone());
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
            ring_selection = Some(reshiki::editing::ring_oriented(
                &mut preview,
                anchor,
                size,
                true,
                10. / self.camera.zoom,
                direction,
            ));
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
                    preview = drawing;
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
                            preview = drawing;
                            ring_selection = Some(vec![id, added]);
                        }
                    } else {
                        let a = id.unwrap_or_else(|| preview.add_atom("C", origin));
                        let z = target.unwrap_or_else(|| preview.add_atom("C", end));
                        preset.place(&mut preview, a, z);
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
        preview
            .annotations
            .retain(|a| Some(a.id) != self.hidden_annotation);
        let selected = ring_selection.as_deref().unwrap_or(self.selected);
        draw_atom_markers(frame, &preview, selected, self.camera, bounds, true);
        for id in selected {
            for a in preview.annotations.iter().filter(|a| a.id == *id) {
                frame.stroke(
                    &Path::rectangle(
                        self.camera.screen(a.position, bounds),
                        iced::Size::new(
                            a.size().0 * self.camera.zoom,
                            a.size().1 * self.camera.zoom,
                        ),
                    ),
                    Stroke::default().with_color(Color::from_rgb8(20, 130, 112)),
                );
            }
        }
        draw_document(frame, &preview, self.camera, bounds);
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
            let p = self.camera.screen(point, bounds);
            let position = Point::new(
                (p.x + 14.).clamp(4., (bounds.width - 200.).max(4.)),
                (p.y + 18.).clamp(20., (bounds.height - 30.).max(20.)),
            );
            frame.fill_rectangle(
                position - Vector::new(5., 4.),
                iced::Size::new(196., 24.),
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
        }
        if self.tool == Tool::EditPoints {
            for graphic in preview.graphics.iter().filter(|g| selected.contains(&g.id)) {
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
                    draw_atom_markers(frame, self.doc, &hit, self.camera, bounds, false);
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
        if self.tool.selects() {
            if let (Some(Gesture::Transform(drag)), Some(p)) = (&state.gesture, state.cursor)
                && matches!(drag.handle, Handle::Rotate)
            {
                let end = self
                    .camera
                    .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
                drag.selection
                    .draw(frame, drag.values(end, state.modifiers.shift()).1);
            } else if let Some(selection) =
                SelectionBox::new(&preview, selected, self.camera, bounds)
            {
                selection.draw(frame, 0.0);
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
    }
}

fn region_selection(
    doc: &Document,
    selected: &[u64],
    polygon: &[World],
    mods: iced::keyboard::Modifiers,
) -> Vec<u64> {
    let hits = doc.expand_groups(&reshiki::selection_region::objects(doc, polygon));
    reshiki::selection_region::combine(selected, &hits, mods.shift(), mods.alt())
}

fn draw_atom_markers(
    frame: &mut Frame,
    doc: &Document,
    ids: &[u64],
    camera: Camera,
    bounds: Rectangle,
    selected: bool,
) {
    // At page-fit zoom the selection box is more useful than overlapping atom rings.
    if selected && ids.len() > 1 && camera.zoom < 0.12 {
        return;
    }
    let marker_scale = camera.zoom.clamp(0.15, 1.);
    for bond in doc.bonds.iter().filter(|b| doc.bond_visible(b.a, b.b)) {
        if ids.contains(&bond.a)
            && ids.contains(&bond.b)
            && let Some((a, b)) = doc.atom(bond.a).zip(doc.atom(bond.b))
        {
            frame.stroke(
                &Path::line(
                    camera.screen(a.position, bounds),
                    camera.screen(b.position, bounds),
                ),
                Stroke::default()
                    .with_width(7.0 * marker_scale)
                    .with_color(Color::from_rgba8(19, 135, 116, 0.16)),
            );
        }
    }
    for id in ids {
        if let Some(atom) = doc.atom(*id).filter(|a| doc.atom_visible(a.id)) {
            let circle = Path::circle(camera.screen(atom.position, bounds), 8.0 * marker_scale);
            if selected {
                frame.fill(&circle, Color::from_rgba8(19, 135, 116, 0.12));
            }
            frame.stroke(
                &circle,
                Stroke::default()
                    .with_width((if selected { 1.8 } else { 1.4 }) * marker_scale.sqrt())
                    .with_color(rgb([19, 135, 116])),
            );
        }
    }
}

fn ring_gesture(start: World, end: World, attached: bool, radius: f32) -> (World, Option<World>) {
    if attached {
        (start, (start.distance(end) > radius).then_some(end))
    } else {
        (end, None)
    }
}

/// Bonds are manipulated through their two endpoint atoms; they have no separate ID.
fn hit_selection(doc: &Document, p: World, r: f32) -> Vec<u64> {
    if let Some(i) = reshiki::atom_labels::indicators(doc).into_iter().find(|i| {
        (p.x - i.center.x).abs() < i.width / 2. + r && (p.y - i.center.y).abs() < i.height / 2. + r
    }) {
        return match i.owner {
            reshiki::atom_labels::Owner::Number(id)
            | reshiki::atom_labels::Owner::AtomStereo(id) => {
                vec![id]
            }
            reshiki::atom_labels::Owner::BondStereo(a, b) => vec![a, b],
        };
    }
    if let Some(id) = hit_object(doc, p, r) {
        return vec![id];
    }
    reshiki::editing::nearest_bond(doc, p, r * 0.7)
        .and_then(|i| doc.bonds.get(i))
        .map(|b| vec![b.a, b.b])
        .unwrap_or_default()
}
/// Use the same attachment resolution for the drag preview and committed bond.
/// Excluding the source lets short drags grow a bond rather than snap to themselves.
#[cfg(test)]
fn bond_target(
    doc: &Document,
    start: World,
    end: World,
    source: Option<u64>,
    radius: f32,
) -> (World, Option<u64>) {
    bond_target_with(doc, start, end, source, radius, BondDrawing::default())
}
fn bond_target_with(
    doc: &Document,
    start: World,
    end: World,
    source: Option<u64>,
    radius: f32,
    mut drawing: BondDrawing,
) -> (World, Option<u64>) {
    let origin = source.and_then(|id| doc.atom(id));
    if origin.is_some_and(|a| !a.centroid.is_empty()) {
        // A centre-to-metal contact often extends beyond the ring radius.
        // A normal fixed length can land exactly on a member atom instead.
        drawing.fixed_length = false;
    }
    let nearest = |p: World| {
        doc.atoms
            .iter()
            .filter(|a| {
                doc.atom_visible(a.id)
                    && Some(a.id) != source
                    && a.position.distance(p) < radius
                    && !origin.is_some_and(|o| {
                        o.attachment.is_some()
                            && (o.centroid.contains(&a.id) || !a.centroid.is_empty())
                    })
                    && !(a.attachment.is_some()
                        && source.is_some_and(|id| a.centroid.contains(&id)))
            })
            .min_by(|a, b| a.position.distance(p).total_cmp(&b.position.distance(p)))
    };
    let snapped = source
        .and_then(|id| plane_endpoint(doc, id, end, drawing))
        .map(|p| p.position)
        .unwrap_or_else(|| drawing.endpoint(start, end));
    if let Some(atom) = nearest(end).or_else(|| nearest(snapped)) {
        (atom.position, Some(atom.id))
    } else {
        (snapped, None)
    }
}
fn plane_endpoint(
    doc: &Document,
    id: u64,
    cursor: World,
    drawing: BondDrawing,
) -> Option<reshiki::projection::growth::Endpoint> {
    reshiki::projection::growth::Plane::at(doc, id)?.endpoint(cursor, drawing)
}
pub fn hit_object(doc: &Document, p: World, r: f32) -> Option<u64> {
    let graphic = |front| {
        doc.graphics
            .iter()
            .enumerate()
            .filter(|(_, g)| (g.layer >= 0) == front && g.hit(p, r))
            .max_by_key(|(i, g)| (g.layer, *i))
            .map(|(_, g)| g.id)
    };
    graphic(true)
        .or_else(|| {
            doc.atoms
                .iter()
                .rev()
                .find(|a| {
                    doc.atom_visible(a.id)
                        && reshiki::scientific::styled_mark_parts(a, &doc.drawing_style)
                            .iter()
                            .any(|part| {
                                reshiki::graphics::flattened(&part.commands)
                                    .iter()
                                    .any(|points| {
                                        points.windows(2).any(
                                    |q| matches!(q, [a, b] if distance_to_segment(p, *a, *b) < r),
                                )
                                    })
                            })
                })
                .map(|a| a.id)
        })
        .or_else(|| doc.nearest(p, r))
        .or_else(|| {
            doc.annotations
                .iter()
                .rev()
                .find(|a| {
                    p.x >= a.position.x
                        && p.x < a.position.x + a.size().0
                        && p.y >= a.position.y
                        && p.y < a.position.y + a.size().1
                })
                .map(|a| a.id)
        })
        .or_else(|| doc.arrows.iter().rev().find(|a| a.hit(p, r)).map(|a| a.id))
        .or_else(|| {
            if reshiki::editing::nearest_bond(doc, p, r * 0.7).is_none() {
                graphic(false)
            } else {
                None
            }
        })
}
pub fn distance_to_segment(p: World, a: World, b: World) -> f32 {
    let v = Vector::new(b.x - a.x, b.y - a.y);
    let len = v.x * v.x + v.y * v.y;
    if len < 0.001 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * v.x + (p.y - a.y) * v.y) / len).clamp(0.0, 1.0);
    p.distance(a.offset(v.x * t, v.y * t))
}

fn draw_document(
    frame: &mut layered::Frame<'_>,
    doc: &Document,
    camera: Camera,
    bounds: Rectangle,
) {
    draw_document_with_minimum_stroke(frame, doc, camera, bounds, 0.);
}

fn draw_document_with_minimum_stroke(
    frame: &mut layered::Frame<'_>,
    doc: &Document,
    camera: Camera,
    bounds: Rectangle,
    minimum: f32,
) {
    for primitive in primitives(doc) {
        match primitive {
            Primitive::Picture(g) => {
                if let Some(picture) = &g.picture {
                    let flip = g.axis_x.x * g.axis_y.y - g.axis_x.y * g.axis_y.x < 0.;
                    if let Some(handle) = picture.handle(flip) {
                        let width = g.axis_x.distance(World::default()) * camera.zoom;
                        let height = g.axis_y.distance(World::default()) * camera.zoom;
                        let center = camera.screen(
                            g.origin.offset(
                                (g.axis_x.x + g.axis_y.x) / 2.,
                                (g.axis_x.y + g.axis_y.y) / 2.,
                            ),
                            bounds,
                        );
                        frame.split();
                        frame.draw_image(
                            Rectangle {
                                x: center.x - width / 2.,
                                y: center.y - height / 2.,
                                width,
                                height,
                            },
                            canvas::Image::new(handle).rotation(g.axis_x.y.atan2(g.axis_x.x)),
                        );
                        frame.split();
                    }
                }
            }
            Primitive::Path {
                commands,
                style,
                filled,
            } => {
                let path = Path::new(|b| {
                    for c in commands {
                        match c {
                            PathCommand::Move(p) => b.move_to(camera.screen(p, bounds)),
                            PathCommand::Line(p) => b.line_to(camera.screen(p, bounds)),
                            PathCommand::Cubic(a, z, p) => b.bezier_curve_to(
                                camera.screen(a, bounds),
                                camera.screen(z, bounds),
                                camera.screen(p, bounds),
                            ),
                            PathCommand::Close => b.close(),
                        }
                    }
                });
                if filled && let Some(c) = style.fill {
                    frame.fill(&path, rgb(c));
                }
                let dashes: Vec<_> = style.dashes().iter().map(|v| v * camera.zoom).collect();
                let stroke = Stroke::default()
                    .with_color(rgb(style.stroke))
                    .with_width(if style.width_pt > 0. {
                        (style.width() * camera.zoom).max(minimum)
                    } else {
                        0.
                    })
                    .with_line_cap(canvas::LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round);
                if dashes.is_empty() {
                    frame.stroke(&path, stroke);
                } else {
                    frame.stroke(&dashes::dashed(&path, &dashes), stroke);
                }
            }
            Primitive::Line(a, b, width) => frame.stroke(
                &Path::line(camera.screen(a, bounds), camera.screen(b, bounds)),
                Stroke::default()
                    .with_width((width * camera.zoom).max(minimum))
                    .with_line_cap(canvas::LineCap::Round)
                    .with_color(Color::BLACK),
            ),
            Primitive::Polygon(points) => {
                let path = Path::new(|builder| {
                    if let Some(p) = points.first() {
                        builder.move_to(camera.screen(*p, bounds));
                        for p in points.iter().skip(1) {
                            builder.line_to(camera.screen(*p, bounds));
                        }
                        builder.close();
                    }
                });
                frame.fill(&path, Color::BLACK);
            }
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                let t = canvas::Text {
                    content: text,
                    position: camera.screen(position, bounds),
                    size: (size * camera.zoom).into(),
                    font: iced::Font {
                        family: iced::font::Family::Name(reshiki::style::font_name(&style.family)),
                        weight: if style.bold {
                            iced::font::Weight::Bold
                        } else {
                            iced::font::Weight::Normal
                        },
                        style: if style.italic {
                            iced::font::Style::Italic
                        } else {
                            iced::font::Style::Normal
                        },
                        ..Default::default()
                    },
                    line_height: iced::widget::text::LineHeight::Relative(1.0),
                    color: rgb(color),
                    shaping: iced::widget::text::Shaping::Advanced,
                    ..Default::default()
                };
                t.draw_with(|path, color| frame.fill(&path, color));
                if style.underline {
                    let width = reshiki::style::styled_text_width(&t.content, size, &style);
                    frame.stroke(
                        &Path::line(
                            camera.screen(position.offset(0.0, size * 0.95), bounds),
                            camera.screen(position.offset(width, size * 0.95), bounds),
                        ),
                        Stroke::default()
                            .with_width((size * 0.045 * camera.zoom).max(0.5))
                            .with_color(rgb(color)),
                    );
                }
            }
        }
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
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size());
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
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size());
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
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size());
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
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size());
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
        _: &Theme,
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
        let mut frame = layered::Frame::new(renderer, bounds.size());
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
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = layered::Frame::new(renderer, bounds.size());
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;

    #[test]
    fn unbounded_canvas_edges_accept_input_with_or_without_crosshair() -> Result<(), String> {
        let doc = Document::default();
        let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(400., 300.));
        for crosshair in [false, true] {
            let mut canvas = chain_canvas(&doc, ChainMode::Straight);
            canvas.tool = Tool::Atom;
            canvas.guides.crosshair = crosshair;
            assert_eq!(canvas.guides.paper(bounds), bounds);
            for p in [Point::new(21., 51.), Point::new(419., 349.)] {
                let mut state = State::default();
                let cursor = mouse::Cursor::Available(p);
                canvas.update(
                    &mut state,
                    &Event::Mouse(mouse::Event::CursorMoved { position: p }),
                    bounds,
                    cursor,
                );
                let action = canvas
                    .update(
                        &mut state,
                        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        bounds,
                        cursor,
                    )
                    .ok_or("Edge click should reach the canvas")?;
                let Some(Edit::Click(world)) = action.into_inner().0 else {
                    return Err("Expected atom placement".into());
                };
                let expected = canvas
                    .camera
                    .world(Point::new(p.x - bounds.x, p.y - bounds.y), bounds);
                assert!(world.distance(expected) < 0.001);
            }
        }
        Ok(())
    }

    #[test]
    fn scroll_pans_both_axes_and_command_or_control_zooms_at_pointer() -> Result<(), String> {
        use iced::keyboard::Modifiers;
        let doc = Document::default();
        let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(500., 350.));
        for rulers in [false, true] {
            for zoom in [0.5, 2.] {
                let mut canvas = chain_canvas(&doc, ChainMode::Straight);
                canvas.guides.rulers = rulers;
                canvas.camera.zoom = zoom;
                let paper = canvas.guides.paper(bounds);
                let local = Point::new(123., 87.);
                let pointer = Point::new(paper.x + local.x, paper.y + local.y);
                let anchor = canvas.camera.world(local, paper);
                let cursor = mouse::Cursor::Available(pointer);
                for (delta, expected_x, expected_y) in [
                    (mouse::ScrollDelta::Pixels { x: 24., y: -18. }, 24., -18.),
                    (mouse::ScrollDelta::Lines { x: -2., y: 1. }, -80., 40.),
                ] {
                    let mut state = State::default();
                    let action = canvas
                        .update(
                            &mut state,
                            &Event::Mouse(mouse::Event::WheelScrolled { delta }),
                            bounds,
                            cursor,
                        )
                        .ok_or("Scroll action")?;
                    let Some(Edit::Pan(dx, dy)) = action.into_inner().0 else {
                        return Err("Unmodified scroll must pan".into());
                    };
                    let mut moved = canvas.camera;
                    moved.center = moved.center.offset(-dx, -dy);
                    let position = moved.screen(anchor, paper);
                    assert!((position.x - local.x - expected_x).abs() < 0.001);
                    assert!((position.y - local.y - expected_y).abs() < 0.001);
                    for modifiers in [Modifiers::CTRL, Modifiers::COMMAND] {
                        let mut state = State {
                            modifiers,
                            ..Default::default()
                        };
                        let action = canvas
                            .update(
                                &mut state,
                                &Event::Mouse(mouse::Event::WheelScrolled { delta }),
                                bounds,
                                cursor,
                            )
                            .ok_or("Zoom action")?;
                        let Some(Edit::Zoom(factor, point)) = action.into_inner().0 else {
                            return Err("Modified scroll must zoom".into());
                        };
                        assert_eq!(point, anchor);
                        assert!((factor - 1.) * expected_y > 0.);
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn rulers_exclude_editing_and_pointer_coordinates_use_the_inset_paper() {
        let doc = Document::default();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            tool: Tool::Atom,
            guides: guides::Guides {
                rulers: true,
                crosshair: true,
                ..Default::default()
            },
            ..chain_canvas(&doc, ChainMode::Straight)
        };
        let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(400., 300.));
        let mut state = State::default();
        for p in [Point::new(45., 200.), Point::new(200., 63.)] {
            let cursor = mouse::Cursor::Available(p);
            canvas.update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved { position: p }),
                bounds,
                cursor,
            );
            assert!(
                canvas
                    .update(
                        &mut state,
                        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        bounds,
                        cursor
                    )
                    .is_none()
            );
            assert!(state.gesture.is_none());
        }
        let p = Point::new(239., 213.);
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: p }),
            bounds,
            mouse::Cursor::Available(p),
        );
        let click = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(p),
            )
            .expect("atom click on paper");
        assert!(matches!(
            click.into_inner().0,
            Some(Edit::Click(World { x: 0., y: 0. }))
        ));
        state.modifiers = iced::keyboard::Modifiers::COMMAND;
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: 1. },
                }),
                bounds,
                mouse::Cursor::Available(p),
            )
            .expect("zoom on drawing");
        assert!(matches!(
            action.into_inner().0,
            Some(Edit::Zoom(_, World { x: 0., y: 0. }))
        ));
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorLeft),
            bounds,
            mouse::Cursor::Unavailable,
        );
        assert!(state.cursor.is_none());
    }

    #[test]
    fn free_ring_preset_drag_keeps_its_start_as_rotation_anchor() {
        let doc = Document::default();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            tool: Tool::RingPreset(reshiki::rings::Preset::ChairUp),
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            ..chain_canvas(&doc, ChainMode::Straight)
        };
        assert!(matches!(
            pointer_gesture(&canvas, Point::new(200., 150.), Point::new(200., 80.)),
            Edit::RingPreset(
                reshiki::rings::Preset::ChairUp,
                World { x: 0., y: 0. },
                Some(World { x: 0., y: -70. }),
                false,
                false
            )
        ));
        assert!(doc.atoms.is_empty());
    }

    #[test]
    fn arrow_handle_drag_is_one_edit_and_midpoint_hit_follows_curve() {
        use reshiki::arrows::{ArrowStyle, Preset};
        let mut doc = Document::default();
        doc.arrows.push(reshiki::document::Arrow::new(
            1,
            World::new(-60., 0.),
            World::new(60., 0.),
            Preset::Forward,
            ArrowStyle::default(),
        ));
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            tool: Tool::Select,
            selected: &[1],
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            ..chain_canvas(&doc, ChainMode::Straight)
        };
        assert!(matches!(
            pointer_gesture(&canvas, Point::new(200., 150.), Point::new(200., 95.)),
            Edit::ArrowHandle(1, 2, World { x: 0., y: -55. })
        ));
        assert!(doc.arrows[0].control.is_none());
        doc.arrows[0].edit_handle(2, World::new(0., -55.));
        assert_eq!(hit_object(&doc, World::new(0., -55.), 4.), Some(1));
        assert_eq!(hit_object(&doc, World::new(0., 0.), 4.), None);
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            tool: Tool::Arrow,
            selected: &[1],
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            ..chain_canvas(&doc, ChainMode::Straight)
        };
        assert!(matches!(
            pointer_gesture(&canvas, Point::new(200., 95.), Point::new(200., 95.)),
            Edit::ArrowClick(1)
        ));
        assert!(matches!(
            pointer_gesture(&canvas, Point::new(260., 150.), Point::new(300., 150.)),
            Edit::ArrowHandle(1, 1, World { x: 100., y: 0. })
        ));
    }

    #[test]
    fn template_anchor_preview_hit_tests_atoms_bonds_and_empty_space() {
        use reshiki::templates::Anchor;
        let mut doc = Document::default();
        let a = doc.add_atom("N", World::new(-42., 0.));
        let b = doc.add_atom("C", World::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let preview = TemplateAnchorPreview {
            document: &doc,
            anchor: Anchor::Auto,
        };
        let bounds = Rectangle::new(Point::new(20., 30.), iced::Size::new(220., 150.));
        let camera = preview.camera(bounds);
        for (point, expected) in [
            (World::new(-42., 0.), Anchor::Atom(a)),
            (World::default(), Anchor::Bond(a, b)),
        ] {
            let p = camera.screen(point, bounds) + Vector::new(bounds.x, bounds.y);
            let action = preview
                .update(
                    &mut None,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(p),
                )
                .unwrap();
            assert_eq!(action.into_inner().0, Some(expected));
        }
        assert!(
            preview
                .update(
                    &mut None,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(Point::new(21., 31.))
                )
                .is_none()
        );
    }

    #[test]
    fn aromatic_drag_carries_the_same_xyz_as_the_live_preview() -> Result<(), String> {
        use reshiki::projection::growth::{self, Plane};
        let mut doc = reshiki::rings::Preset::Benzene.document(42., false);
        let ids = doc.all_ids();
        reshiki::projection::tilt(&mut doc, &ids, 55., false);
        let atom = doc.atoms.get(1).ok_or("Ring atom")?;
        let plane = Plane::at(&doc, atom.id).ok_or("Plane")?;
        let cursor = plane.outward(84.).ok_or("Cursor")?.position;
        let expected = plane
            .endpoint(cursor, BondDrawing::default())
            .ok_or("Preview")?;
        for tool in [Tool::Atom, Tool::Bond(1)] {
            let mut canvas = chain_canvas(&doc, ChainMode::Straight);
            canvas.tool = tool;
            let press = Point::new(200. + atom.position.x, 150. + atom.position.y);
            let release = Point::new(200. + cursor.x, 150. + cursor.y);
            let Edit::PlaneBond(id, end) = pointer_gesture(&canvas, press, release) else {
                return Err("Expected a bond with retained depth".into());
            };
            assert_eq!(id, atom.id);
            assert_eq!(end, expected);
            assert!(end.depth.abs() > 1.);
            let (preview, added) =
                growth::place(&doc, id, end, "C", reshiki::bonds::BondPreset::Single)?;
            assert_eq!(preview.atom(added).ok_or("Added")?.depth, end.depth);
        }
        // Connecting an existing atom keeps its existing coordinates/depth.
        let (p, target) = bond_target_with(
            &doc,
            atom.position,
            doc.atoms.first().ok_or("Target")?.position,
            Some(atom.id),
            12.,
            BondDrawing::default(),
        );
        assert_eq!(target, doc.atoms.first().map(|a| a.id));
        assert_eq!(p, doc.atoms.first().ok_or("Target")?.position);
        Ok(())
    }

    #[test]
    fn atom_click_and_drag_use_distinct_edits_and_bond_constraints() -> Result<(), String> {
        let mut doc = Document::default();
        let atom = doc.add_atom("C", World::default());
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Atom;
        canvas.element = "O";
        let start = Point::new(200., 150.);
        assert!(matches!(
            pointer_gesture(&canvas, start, start),
            Edit::Click(_)
        ));
        let Edit::Bond(a, b, Some(id), None) =
            pointer_gesture(&canvas, start, Point::new(255., 178.))
        else {
            return Err("Atom drag must create a bond edit".into());
        };
        assert_eq!(id, atom);
        assert!((a.distance(b) - 42.).abs() < 0.001);
        let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees();
        assert!((angle - 30.).abs() < 0.001);
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        let mut state = State {
            modifiers: iced::keyboard::Modifiers::ALT,
            ..Default::default()
        };
        let end = Point::new(255., 178.);
        for event in [
            mouse::Event::CursorMoved { position: start },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(
                &mut state,
                &Event::Mouse(event),
                bounds,
                mouse::Cursor::Available(end),
            );
        }
        let edit = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(end),
            )
            .and_then(|a| a.into_inner().0)
            .ok_or("Missing free drag")?;
        assert!(
            matches!(edit, Edit::Bond(_, p, Some(_), None) if p.distance(World::new(55.,28.)) < 0.001),
            "{edit:?}"
        );
        Ok(())
    }

    #[test]
    fn ring_modifier_draws_a_circle_and_does_not_change_chairs_or_haworth() -> Result<(), String> {
        use reshiki::rings::Preset;
        let modifier = if cfg!(target_os = "macos") {
            iced::keyboard::Modifiers::LOGO
        } else {
            iced::keyboard::Modifiers::CTRL
        };
        let doc = Document::default();
        for (tool, size) in [
            (Tool::Ring, 7),
            (Tool::RingPreset(Preset::Benzene), 6),
            (Tool::RingPreset(Preset::Cyclopentadiene), 5),
        ] {
            let mut canvas = chain_canvas(&doc, ChainMode::Straight);
            canvas.tool = tool;
            canvas.ring_size = 7;
            let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
            let p = Point::new(200., 150.);
            let mut state = State {
                modifiers: modifier,
                ..Default::default()
            };
            for event in [
                mouse::Event::CursorMoved { position: p },
                mouse::Event::ButtonPressed(mouse::Button::Left),
            ] {
                canvas.update(
                    &mut state,
                    &Event::Mouse(event),
                    bounds,
                    mouse::Cursor::Available(p),
                );
            }
            let edit = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(p),
                )
                .and_then(|a| a.into_inner().0)
                .ok_or("Missing ring gesture")?;
            assert!(matches!(edit, Edit::DelocalizedRing(_, _, n) if n == size));
        }
        for preset in [
            Preset::ChairUp,
            Preset::ChairDown,
            Preset::HaworthFive,
            Preset::HaworthSix,
        ] {
            assert_eq!(
                delocalized_ring_size(Tool::RingPreset(preset), 6, modifier),
                None
            );
        }
        assert_eq!(
            delocalized_ring_size(Tool::Ring, 6, iced::keyboard::Modifiers::empty()),
            None
        );
        Ok(())
    }

    fn chain_canvas(doc: &Document, mode: ChainMode) -> MoleculeCanvas<'_> {
        static STYLE: std::sync::LazyLock<GraphicStyle> =
            std::sync::LazyLock::new(GraphicStyle::default);
        MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            doc,
            selected: &[],
            tool: Tool::Chain(mode),
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &STYLE,
            bracket_sides: BracketSides::Both,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
        }
    }

    fn tilt_pointer(
        canvas: &MoleculeCanvas<'_>,
        state: &mut State,
        event: mouse::Event,
    ) -> Option<Edit> {
        let bounds = Rectangle::new(Point::new(20., 30.), iced::Size::new(400., 300.));
        canvas
            .update(
                state,
                &Event::Mouse(event),
                bounds,
                mouse::Cursor::Unavailable,
            )
            .and_then(|action| action.into_inner().0)
    }

    #[test]
    fn tilt_drag_preserves_partial_selection_and_has_zoom_independent_snapping()
    -> Result<(), String> {
        let mut doc = Document::default();
        let ring = reshiki::editing::ring(&mut doc, World::default(), 6, true, 5.);
        let selected: Vec<_> = ring.iter().take(3).copied().collect();
        let atom = doc
            .atom(*selected.first().ok_or("selected atom")?)
            .ok_or("atom")?;
        let before = doc.clone();
        for zoom in [0.5, 1., 2.] {
            let mut canvas = chain_canvas(&doc, ChainMode::Straight);
            canvas.tool = Tool::Tilt;
            canvas.selected = &selected;
            canvas.camera.zoom = zoom;
            let mut state = State {
                modifiers: iced::keyboard::Modifiers::SHIFT,
                ..Default::default()
            };
            let start = Point::new(220. + atom.position.x * zoom, 180. + atom.position.y * zoom);
            tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::CursorMoved { position: start },
            );
            tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::ButtonPressed(mouse::Button::Left),
            );
            assert!(matches!(&state.gesture, Some(Gesture::Tilt(drag)) if drag.ids == selected));
            let end = Point::new(start.x + 36., start.y - 26.);
            let motion = tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::CursorMoved { position: end },
            );
            assert!(motion.is_none(), "Motion must only redraw the preview");
            assert_eq!(doc, before);
            let edit = tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left),
            );
            assert!(matches!(edit, Some(Edit::Tilt { ids, x: 15., y: 15. }) if ids == selected));
            assert!(state.gesture.is_none());
        }
        Ok(())
    }

    #[test]
    fn tilt_click_selects_a_molecule_without_rotating_and_blank_drag_selects() -> Result<(), String>
    {
        let mut doc = Document::default();
        let a = doc.add_atom("N", World::new(-30., 0.));
        let b = doc.add_atom("C", World::new(30., 0.));
        doc.add_bond(a, b, 1, "plain");
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Tilt;
        let mut state = State::default();
        for (start, end, expected) in [
            (Point::new(190., 180.), Point::new(191., 180.), vec![a, b]),
            (Point::new(170., 145.), Point::new(270., 210.), vec![a, b]),
            (Point::new(350., 300.), Point::new(350., 300.), vec![]),
        ] {
            tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::CursorMoved { position: start },
            );
            tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::ButtonPressed(mouse::Button::Left),
            );
            tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::CursorMoved { position: end },
            );
            let edit = tilt_pointer(
                &canvas,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left),
            )
            .ok_or("selection edit")?;
            let Edit::Select(mut ids) = edit else {
                return Err("A click must not rotate".into());
            };
            ids.sort_unstable();
            assert_eq!(ids, expected);
        }
        Ok(())
    }

    #[test]
    fn aromatic_ring_interior_opens_its_menu_and_starts_tilt_without_prior_selection()
    -> Result<(), String> {
        let mut doc = Document::default();
        let mut ids = reshiki::editing::ring(&mut doc, World::default(), 6, true, 5.);
        ids.sort_unstable();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Tilt;
        let mut state = State::default();
        let center = Point::new(220., 180.);
        tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::CursorMoved { position: center },
        );
        let edit = tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonPressed(mouse::Button::Right),
        )
        .ok_or("context menu")?;
        let Edit::ContextMenu { mut selected, .. } = edit else {
            return Err("Expected context menu".into());
        };
        selected.sort_unstable();
        assert_eq!(selected, ids);
        tilt_pointer(
            &canvas,
            &mut state,
            mouse::Event::ButtonPressed(mouse::Button::Left),
        );
        let Some(Gesture::Tilt(drag)) = &state.gesture else {
            return Err("Expected tilt from ring interior".into());
        };
        let mut selected = drag.ids.clone();
        selected.sort_unstable();
        assert_eq!(selected, ids);
        Ok(())
    }

    #[test]
    fn tilt_cancel_focus_loss_outside_release_and_tool_switch_do_not_commit() {
        let doc = reshiki::rings::Preset::Regular.document(42., false);
        let selected = doc.all_ids();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Tilt;
        canvas.selected = &selected;
        let bounds = Rectangle::new(Point::new(20., 30.), iced::Size::new(400., 300.));
        for cancel in [
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                physical_key: iced::keyboard::key::Physical::Code(
                    iced::keyboard::key::Code::Escape,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers: iced::keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            Event::Window(iced::window::Event::Unfocused),
        ] {
            let mut state = State {
                gesture: Some(Gesture::Tilt(tilt::TiltDrag {
                    ids: selected.clone(),
                    start: Point::new(200., 150.),
                })),
                cursor: Some(Point::new(250., 160.)),
                ..Default::default()
            };
            canvas.update(&mut state, &cancel, bounds, mouse::Cursor::Unavailable);
            assert!(state.gesture.is_none());
            assert!(
                tilt_pointer(
                    &canvas,
                    &mut state,
                    mouse::Event::ButtonReleased(mouse::Button::Left)
                )
                .is_none()
            );
        }
        for (tool, position) in [
            (Tool::Tilt, Point::new(500., 150.)),
            (Tool::Select, Point::new(250., 160.)),
        ] {
            canvas.tool = tool;
            let mut state = State {
                gesture: Some(Gesture::Tilt(tilt::TiltDrag {
                    ids: selected.clone(),
                    start: Point::new(200., 150.),
                })),
                cursor: Some(position),
                ..Default::default()
            };
            assert!(
                tilt_pointer(
                    &canvas,
                    &mut state,
                    mouse::Event::ButtonReleased(mouse::Button::Left)
                )
                .is_none()
            );
            assert!(state.gesture.is_none());
        }
    }

    #[test]
    fn dragging_redraws_the_canvas_without_publishing_intermediate_application_updates() {
        let mut doc = Document::default();
        let atom = doc.add_atom("C", World::default());
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
        let mut state = State::default();
        let start = Point::new(200., 150.);
        let end = Point::new(260., 180.);
        let cursor = mouse::Cursor::Available(end);
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: start }),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        for x in 201..=260 {
            let action = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::CursorMoved {
                        position: Point::new(x as f32, 180.),
                    }),
                    bounds,
                    cursor,
                )
                .unwrap();
            assert!(action.into_inner().0.is_none());
        }
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap();
        assert!(
            matches!(action.into_inner().0, Some(Edit::Move(ids, 60., 30.)) if ids == vec![atom])
        );
        assert_eq!(doc.atom(atom).unwrap().position, World::default());
    }

    #[test]
    fn centroid_drag_moves_ligand_and_point_edit_mode_moves_only_the_point() -> Result<(), String> {
        let mut doc = Document::default();
        let metal = doc.add_atom("Fe", World::default());
        let (doc, _) = reshiki::hotkeys::atom_edit(&doc, metal, "j", 42.).ok_or("Shortcut")??;
        let anchor = doc
            .atoms
            .iter()
            .find(|a| a.attachment.is_some())
            .ok_or("Point")?;
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
        let start = canvas.camera.screen(anchor.position, bounds);
        let end = Point::new(start.x + 35., start.y + 25.);
        for tool in [Tool::Select, Tool::EditPoints] {
            canvas.tool = tool;
            let mut state = State {
                modifiers: iced::keyboard::Modifiers::ALT,
                ..Default::default()
            };
            canvas.update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(start),
            );
            let Some(Gesture::Move {
                ids: preview_ids, ..
            }) = &state.gesture
            else {
                return Err("Missing drag preview".into());
            };
            let preview_ids = preview_ids.clone();
            let result = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(end),
                )
                .and_then(|action| action.into_inner().0)
                .ok_or("Missing move")?;
            let Edit::Move(ids, dx, dy) = result else {
                return Err("Expected move".into());
            };
            assert_eq!(ids, preview_ids);
            assert!((dx - 35.).abs() < 0.001 && (dy - 25.).abs() < 0.001);
            if tool == Tool::Select {
                assert_eq!(ids.len(), 6);
                assert!(anchor.centroid.iter().all(|id| ids.contains(id)));
                assert!(!ids.contains(&metal));
            } else {
                assert_eq!(ids, vec![anchor.id]);
            }
            let mut preview = doc.clone();
            preview.translate(&preview_ids, dx, dy);
            let mut committed = doc.clone();
            committed.translate(&ids, dx, dy);
            assert_eq!(preview, committed);
            assert_eq!(committed.atom(metal), doc.atom(metal));
        }
        Ok(())
    }

    #[test]
    fn bonded_move_release_uses_constraints_and_live_option_override() -> Result<(), String> {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::default());
        let b = doc.add_atom("O", World::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
        for free in [false, true] {
            let mut state = State {
                gesture: Some(Gesture::Move {
                    start: World::new(44., 1.),
                    ids: vec![b],
                    clicked: vec![b],
                }),
                ..Default::default()
            };
            let end = Point::new(279., 183.);
            if free {
                canvas.update(
                    &mut state,
                    &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                        iced::keyboard::Modifiers::ALT,
                    )),
                    bounds,
                    mouse::Cursor::Available(end),
                );
            }
            let result = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(end),
                )
                .and_then(|action| action.into_inner().0)
                .ok_or("Missing release edit")?;
            let Edit::Move(ids, dx, dy) = result else {
                return Err("Expected movement".into());
            };
            let requested = World::new(35., 32.);
            let preview = movement::delta(
                &doc,
                &[b],
                requested,
                canvas.bond_drawing.unconstrained(free),
            );
            assert_eq!(ids, vec![b]);
            assert_eq!(World::new(dx, dy), preview);
            if free {
                assert_eq!(preview, requested);
            } else {
                assert!((World::new(42. + dx, dy).distance(World::default()) - 42.).abs() < 0.001);
            }
        }
        assert_eq!(doc.atom(b).ok_or("atom")?.position, World::new(42., 0.));
        Ok(())
    }

    #[test]
    fn secondary_click_selects_its_target_and_preserves_an_existing_multi_selection() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::default());
        let b = doc.add_atom("N", World::new(60., 0.));
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Select;
        let selected = [a, b];
        canvas.selected = &selected;
        let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
        let mut state = State::default();
        for (point, expected) in [
            (Point::new(200., 150.), vec![a, b]),
            (Point::new(350., 250.), vec![]),
        ] {
            let cursor = mouse::Cursor::Available(point);
            canvas.update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved { position: point }),
                bounds,
                cursor,
            );
            let action = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
                    bounds,
                    cursor,
                )
                .unwrap();
            assert!(
                matches!(action.into_inner().0, Some(Edit::ContextMenu { selected, position }) if selected == expected && position == point)
            );
            assert!(state.gesture.is_none());
        }
    }

    #[test]
    fn chain_click_and_fast_drag_keep_attachment_count_and_preview_geometry() {
        let mut doc = Document::default();
        let source = doc.add_atom("N", World::new(-120., 0.));
        let target = doc.add_atom("O", World::new(120., 0.));
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.chain_drawing.atoms = Some(7);
        let Edit::Chain {
            points,
            source: a,
            target: b,
        } = pointer_gesture(&canvas, Point::new(80., 150.), Point::new(320., 150.))
        else {
            panic!("chain gesture")
        };
        assert_eq!((a, b), (Some(source), Some(target)));
        assert_eq!(points.len(), 7);
        let (preview, preview_target) = canvas.chain_plan(
            (World::new(-120., 0.), World::new(-120., 0.)),
            Some(source),
            &[World::new(-120., 0.)],
            (false, true),
            World::new(120., 0.),
            iced::keyboard::Modifiers::empty(),
        );
        assert_eq!(preview, points);
        assert_eq!(preview_target, b);
        canvas.chain_drawing.atoms = Some(4);
        let Edit::Chain {
            points,
            source: a,
            target: b,
        } = pointer_gesture(&canvas, Point::new(84., 153.), Point::new(84., 153.))
        else {
            panic!("attached click")
        };
        assert_eq!(a, Some(source));
        assert_eq!(b, None);
        assert_eq!(points.len(), 4);
        assert_eq!(points[0], doc.atom(source).unwrap().position);

        let initial = chains::straight(
            World::default(),
            World::new(200., 0.),
            false,
            BondDrawing::default(),
            ChainDrawing {
                atoms: Some(6),
                ..Default::default()
            },
            false,
        );
        let (doc, ids) = chains::place(&Document::default(), &initial, None, None, 8.).unwrap();
        let canvas = chain_canvas(&doc, ChainMode::Straight);
        let last = *initial.last().unwrap();
        let (extension, _) = canvas.chain_plan(
            (last, last),
            ids.last().copied(),
            &[last],
            (false, false),
            last,
            iced::keyboard::Modifiers::empty(),
        );
        assert_eq!(extension.len(), 6);
        assert!(extension.iter().all(|p| p.y.abs() <= 21.001));
        assert!(extension.windows(2).all(|p| p[1].x > p[0].x));
    }

    #[test]
    fn snaking_gesture_bends_with_control_and_cancels_on_focus_loss() {
        let doc = Document::default();
        let canvas = chain_canvas(&doc, ChainMode::Straight);
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(600., 500.));
        let mut state = State::default();
        let positions = [
            Point::new(100., 280.),
            Point::new(325., 280.),
            Point::new(325., 80.),
        ];
        let cursor = mouse::Cursor::Available(positions[2]);
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: positions[0],
            }),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: positions[1],
            }),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::CTRL,
            )),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved {
                position: positions[2],
            }),
            bounds,
            cursor,
        );
        let Some(Gesture::Chain {
            points, snaking, ..
        }) = &state.gesture
        else {
            panic!()
        };
        assert!(*snaking);
        assert!(points.len() > 7);
        assert!(points.last().unwrap().y < -100.);
        canvas.update(
            &mut state,
            &Event::Window(iced::window::Event::Unfocused),
            bounds,
            cursor,
        );
        assert!(
            canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    cursor
                )
                .is_none()
        );
        assert!(doc.atoms.is_empty());
    }

    #[test]
    fn attachment_bond_drag_reaches_beyond_the_ring_and_snaps_to_metal() -> Result<(), String> {
        let mut doc = Document::default();
        let members = reshiki::editing::ring(&mut doc, World::new(0., 0.), 6, true, 5.);
        let point =
            reshiki::attachments::add(&mut doc, &members, reshiki::attachments::Kind::MultiCenter)?;
        let start = doc.atom(point).ok_or("Missing attachment")?.position;
        let settings = BondDrawing::default();
        for degrees in (0..360).step_by(30) {
            let angle = (degrees as f32).to_radians();
            let cursor = start.offset(126. * angle.cos(), 126. * angle.sin());
            let (end, target) = bond_target_with(&doc, start, cursor, Some(point), 8., settings);
            assert!(target.is_none());
            assert!((end.distance(start) - 126.).abs() < 0.01);
        }
        let metal = doc.add_atom("Fe", start.offset(110., 91.));
        let target = doc.atom(metal).ok_or("Missing metal")?.position;
        assert_eq!(
            bond_target_with(&doc, start, target, Some(point), 8., settings),
            (target, Some(metal))
        );
        assert_eq!(
            bond_target_with(&doc, target, start, Some(metal), 8., settings),
            (start, Some(point))
        );
        assert!(settings.fixed_length);
        Ok(())
    }

    #[test]
    fn alt_temporarily_frees_bond_constraints_without_changing_tool_preferences() {
        let doc = Document::default();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = Tool::Bond(1);
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        let mut state = State::default();
        let start = Point::new(120., 150.);
        let end = Point::new(246., 202.);
        let cursor = mouse::Cursor::Available(end);
        canvas.update(
            &mut state,
            &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::ALT,
            )),
            bounds,
            cursor,
        );
        for event in [
            mouse::Event::CursorMoved { position: start },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
        }
        let edit = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        let Edit::Bond(a, b, None, None) = edit else {
            panic!()
        };
        assert_eq!(a, World::new(-80., 0.));
        assert_eq!(b, World::new(46., 52.));
        assert!(canvas.bond_drawing.fixed_length && canvas.bond_drawing.fixed_angles);
    }

    #[test]
    fn freeform_selection_tracks_events_adds_subtracts_and_cancels() {
        let mut doc = Document::default();
        doc.add_atom("C", World::new(-50., 0.));
        doc.add_atom("O", World::new(50., 0.));
        let style = GraphicStyle::default();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[2],
            tool: Tool::Lasso,
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &style,
            bracket_sides: BracketSides::Both,
        };
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        let points = [
            Point::new(100., 100.),
            Point::new(190., 100.),
            Point::new(190., 190.),
            Point::new(100., 190.),
            Point::new(100., 100.),
        ];
        for (mods, expected) in [
            (iced::keyboard::Modifiers::empty(), vec![1]),
            (iced::keyboard::Modifiers::SHIFT, vec![2, 1]),
        ] {
            let mut state = State {
                modifiers: mods,
                ..Default::default()
            };
            for (i, p) in points.iter().enumerate() {
                canvas.update(
                    &mut state,
                    &Event::Mouse(mouse::Event::CursorMoved { position: *p }),
                    bounds,
                    mouse::Cursor::Available(*p),
                );
                if i == 0 {
                    canvas.update(
                        &mut state,
                        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        bounds,
                        mouse::Cursor::Available(*p),
                    );
                }
            }
            let result = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(points[0]),
                )
                .unwrap()
                .into_inner()
                .0
                .unwrap();
            let Edit::Select(ids) = result else {
                panic!("expected region selection")
            };
            assert_eq!(ids, expected);
        }
        assert_eq!(
            region_selection(
                &doc,
                &[1, 2],
                &[
                    World::new(-100., -50.),
                    World::new(0., -50.),
                    World::new(0., 50.),
                    World::new(-100., 50.)
                ],
                iced::keyboard::Modifiers::ALT
            ),
            vec![2]
        );
        let mut state = State {
            gesture: Some(Gesture::Lasso {
                points: vec![World::default()],
            }),
            ..Default::default()
        };
        canvas.update(
            &mut state,
            &Event::Window(iced::window::Event::Unfocused),
            bounds,
            mouse::Cursor::Unavailable,
        );
        assert!(state.gesture.is_none());
        assert_eq!(doc.atoms.len(), 2);
    }

    #[test]
    fn group_clicks_move_all_members_and_alt_selects_a_member() {
        let mut doc = Document::default();
        doc.add_atom("C", World::new(-50., 0.));
        doc.add_atom("O", World::new(50., 0.));
        doc.group_selection(&[1, 2]).unwrap();
        let style = GraphicStyle::default();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Select,
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &style,
            bracket_sides: BracketSides::Both,
        };
        let result = pointer_gesture(&canvas, Point::new(150., 150.), Point::new(170., 170.));
        let Edit::Move(ids, dx, dy) = result else {
            panic!("expected group move")
        };
        assert_eq!(ids, vec![1, 2]);
        assert_eq!((dx, dy), (20., 20.));
        let mut state = State {
            modifiers: iced::keyboard::Modifiers::ALT,
            ..Default::default()
        };
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        let cursor = mouse::Cursor::Available(Point::new(150., 150.));
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        let result = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        let Edit::Select(ids) = result else {
            panic!("expected member selection")
        };
        assert_eq!(ids, vec![1]);

        // Alt-drag edits a member immediately, even when its group is selected.
        let selected = [1, 2];
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            selected: &selected,
            ..canvas
        };
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        let result = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(175., 150.)),
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        let Edit::Move(ids, dx, dy) = result else {
            panic!("expected member move")
        };
        assert_eq!(ids, vec![1]);
        assert_eq!((dx, dy), (25., 0.));
    }

    #[test]
    fn filled_background_leaves_atoms_and_bonds_selectable() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::new(-20., 0.));
        let b = doc.add_atom("O", World::new(20., 0.));
        doc.add_bond(a, b, 1, "plain");
        let mut g = Graphic::dragged(
            3,
            reshiki::graphics::GraphicKind::Rectangle,
            World::new(-50., -50.),
            World::new(50., 50.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        );
        g.style.fill = Some([220, 239, 233]);
        doc.graphics.push(g);
        assert_eq!(hit_selection(&doc, World::new(-20., 0.), 5.), vec![a]);
        assert_eq!(hit_selection(&doc, World::default(), 5.), vec![a, b]);
        assert_eq!(hit_selection(&doc, World::new(0., 30.), 5.), vec![3]);
        doc.graphics[0].layer = 1;
        assert_eq!(hit_selection(&doc, World::default(), 5.), vec![3]);
    }

    #[test]
    fn graphic_and_curve_point_drags_publish_one_edit_and_do_not_mutate_preview() {
        let doc = Document::default();
        let style = GraphicStyle::default();
        let mut canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Graphic(reshiki::graphics::GraphicKind::Rectangle),
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &style,
            bracket_sides: BracketSides::Both,
        };
        assert!(matches!(
            pointer_gesture(&canvas, Point::new(150., 100.), Point::new(250., 200.)),
            Edit::Graphic(World { x: -50., y: -50. }, World { x: 50., y: 50. }, false)
        ));
        assert!(doc.graphics.is_empty());
        let mut with_curve = doc.clone();
        with_curve.graphics.push(Graphic::dragged(
            1,
            reshiki::graphics::GraphicKind::Curve,
            World::new(-50., 0.),
            World::new(50., 0.),
            style.clone(),
            BracketSides::Both,
            false,
        ));
        canvas.doc = &with_curve;
        canvas.tool = Tool::EditPoints;
        canvas.selected = &[1];
        let original = with_curve.clone();
        assert!(matches!(
            pointer_gesture(&canvas, Point::new(183., 100.), Point::new(200., 80.)),
            Edit::GraphicPoint(1, 1, _)
        ));
        assert_eq!(with_curve, original);
    }

    #[test]
    fn double_click_edits_grouped_labels_but_drag_moves_the_group() {
        let mut doc = Document::default();
        let atom = doc.add_atom("C", World::new(-100., -80.));
        let label = doc.next_id();
        doc.annotations.push(reshiki::document::Annotation {
            id: label,
            position: World::default(),
            text: "Reaction conditions".into(),
            format: Default::default(),
        });
        doc.group_selection(&[atom, label]).unwrap();
        let style = GraphicStyle::default();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Select,
            camera: Camera {
                center: World::default(),
                zoom: 1.,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &style,
            bracket_sides: BracketSides::Both,
        };
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        let point = Point::new(230., 160.);
        let cursor = mouse::Cursor::Available(point);
        let mut state = State::default();
        for second in [false, true] {
            canvas.update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor,
            );
            let edit = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    cursor,
                )
                .unwrap()
                .into_inner()
                .0
                .unwrap();
            if second {
                assert!(matches!(edit, Edit::BeginText(id) if id == label));
            } else {
                assert!(
                    matches!(edit, Edit::Select(ids) if ids.contains(&label) && ids.contains(&atom))
                );
            }
        }
        assert!(
            matches!(pointer_gesture(&canvas, point, Point::new(245., 180.)), Edit::Move(ids, _, _) if ids.contains(&label) && ids.contains(&atom))
        );
    }

    fn pointer_gesture(canvas: &MoleculeCanvas<'_>, start: Point, end: Point) -> Edit {
        let mut state = State::default();
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
        let cursor = mouse::Cursor::Available(end);
        for event in [
            mouse::Event::CursorMoved { position: start },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
        }
        canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap()
    }

    #[test]
    fn template_drag_uses_the_target_bond_and_can_be_cancelled() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::new(-30.0, 0.0));
        let b = doc.add_atom("C", World::new(30.0, 0.0));
        doc.add_bond(a, b, 1, "plain");
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Template,
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: Some((
                &reshiki::templates::LIBRARY[0].document,
                reshiki::templates::Anchor::Auto,
            )),
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &GraphicStyle::default(),
            bracket_sides: BracketSides::Both,
        };
        let start = Point::new(200.0, 150.0);
        let end = Point::new(200.0, 200.0);
        let Edit::Template(anchor, direction) = pointer_gesture(&canvas, start, end) else {
            panic!("expected template placement")
        };
        assert_eq!(anchor, World::default());
        assert_eq!(direction, Some(World::new(0.0, 50.0)));
        // Snap about the actual attachment atom, including an off-center click.
        let origin = doc.atom(a).unwrap().position;
        let pressed = origin.offset(2., 1.);
        let target = origin.offset(52., 23.);
        use iced::keyboard::Modifiers;
        for modifiers in [Modifiers::SHIFT, Modifiers::CTRL] {
            let (anchor, direction) = canvas.template_gesture(pressed, target, modifiers);
            assert_eq!(anchor, pressed);
            let direction = direction.unwrap();
            let expected = BondDrawing {
                fixed_length: false,
                ..Default::default()
            }
            .endpoint(origin, target);
            assert!(direction.distance(expected) < 0.0001);
            assert!((chains::direction(origin, direction).to_degrees() - 30.).abs() < 0.0001);
            assert!(
                canvas
                    .template_gesture(pressed, pressed, modifiers)
                    .1
                    .is_none()
            );
        }
        assert_eq!(
            canvas
                .template_gesture(pressed, target, Modifiers::empty())
                .1,
            Some(target)
        );
        assert_eq!(
            canvas
                .template_gesture(pressed, target, Modifiers::SHIFT | Modifiers::ALT)
                .1,
            Some(target)
        );
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        for button in [mouse::Button::Left, mouse::Button::Right] {
            let mut drag = State {
                modifiers: Modifiers::CTRL,
                ..Default::default()
            };
            let cursor = mouse::Cursor::Available(Point::new(252., 173.));
            for event in [
                mouse::Event::CursorMoved {
                    position: Point::new(200., 150.),
                },
                mouse::Event::ButtonPressed(button),
                mouse::Event::CursorMoved {
                    position: Point::new(252., 173.),
                },
            ] {
                canvas.update(&mut drag, &Event::Mouse(event), bounds, cursor);
            }
            let result = canvas
                .update(
                    &mut drag,
                    &Event::Mouse(mouse::Event::ButtonReleased(button)),
                    bounds,
                    cursor,
                )
                .unwrap()
                .into_inner()
                .0
                .unwrap();
            let Edit::Template(anchor, Some(direction)) = result else {
                panic!("Expected constrained template, not pan")
            };
            assert!((chains::direction(anchor, direction).to_degrees() - 30.).abs() < 0.0001);
        }
        let mut state = State::default();
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
        let cursor = mouse::Cursor::Available(start);
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Window(iced::window::Event::Unfocused),
            bounds,
            cursor,
        );
        assert!(
            canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    cursor
                )
                .is_none()
        );
        assert_eq!(doc.atoms.len(), 2);
    }

    #[test]
    fn selection_handles_resize_and_rotate_without_moving_or_merging_atoms() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::new(-20.0, -10.0));
        let b = doc.add_atom("C", World::new(20.0, 10.0));
        doc.add_bond(a, b, 1, "plain");
        let original = doc.clone();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[a, b],
            tool: Tool::Select,
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &GraphicStyle::default(),
            bracket_sides: BracketSides::Both,
        };
        for (start, end, expected_pivot, expected_scale, expected_rotation) in [
            (
                Point::new(232.0, 172.0),
                Point::new(272.0, 192.0),
                World::new(-20.0, -10.0),
                2.0,
                0.0,
            ),
            (
                Point::new(200.0, 100.0),
                Point::new(250.0, 150.0),
                World::default(),
                1.0,
                90.0,
            ),
        ] {
            let Edit::Transform {
                ids,
                pivot,
                scale,
                rotation,
            } = pointer_gesture(&canvas, start, end)
            else {
                panic!("handle must produce a transform, not an atom move");
            };
            assert_eq!(ids, vec![a, b]);
            assert_eq!(pivot, expected_pivot);
            assert!((scale - expected_scale).abs() < 0.001);
            assert!((rotation - expected_rotation).abs() < 0.001);
        }
        assert_eq!(
            doc, original,
            "pointer previews must not mutate the document"
        );
        let mut state = State::default();
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
        let cursor = mouse::Cursor::Available(Point::new(232.0, 172.0));
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        assert!(matches!(state.gesture, Some(Gesture::Transform(_))));
        canvas.update(
            &mut state,
            &Event::Window(iced::window::Event::Unfocused),
            bounds,
            cursor,
        );
        assert!(state.gesture.is_none());
        assert!(
            canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    cursor
                )
                .is_none()
        );
    }

    #[test]
    fn bond_midpoints_select_and_drag_both_atoms_without_losing_atom_targets() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::new(-21.0, 0.0));
        let b = doc.add_atom("C", World::new(21.0, 0.0));
        doc.add_bond(a, b, 1, "plain");
        assert_eq!(hit_selection(&doc, World::default(), 10.0), vec![a, b]);
        assert_eq!(hit_selection(&doc, World::new(-20.0, 0.0), 10.0), vec![a]);
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Select,
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &GraphicStyle::default(),
            bracket_sides: BracketSides::Both,
        };
        assert!(
            matches!(pointer_gesture(&canvas, Point::new(200.0, 150.0), Point::new(200.0, 150.0)), Edit::Select(ids) if ids == vec![a, b])
        );
        assert!(
            matches!(pointer_gesture(&canvas, Point::new(200.0, 150.0), Point::new(230.0, 170.0)), Edit::Move(ids, 30.0, 20.0) if ids == vec![a, b])
        );
        let selected = [a, b];
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            selected: &selected,
            ..canvas
        };
        assert!(
            matches!(pointer_gesture(&canvas, Point::new(179.0, 150.0), Point::new(179.0, 150.0)), Edit::Select(ids) if ids == vec![a])
        );
        assert!(
            matches!(pointer_gesture(&canvas, Point::new(179.0, 150.0), Point::new(209.0, 170.0)), Edit::Move(ids, 30.0, 20.0) if ids == vec![a, b])
        );
    }

    #[test]
    fn ring_drag_snaps_at_release_or_keeps_its_initial_attachment() {
        let mut doc = Document::default();
        let a = doc.add_atom("C", World::new(-21.0, 0.0));
        let b = doc.add_atom("C", World::new(21.0, 0.0));
        doc.add_bond(a, b, 1, "plain");
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Ring,
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 5,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &GraphicStyle::default(),
            bracket_sides: BracketSides::Both,
        };
        assert!(
            matches!(pointer_gesture(&canvas, Point::new(100.0, 50.0), Point::new(200.0, 150.0)), Edit::Ring(anchor, None) if anchor == World::default())
        );
        assert!(
            matches!(pointer_gesture(&canvas, Point::new(200.0, 150.0), Point::new(200.0, 100.0)), Edit::Ring(anchor, Some(side)) if anchor == World::default() && side == World::new(0.0, -50.0))
        );
    }

    #[test]
    fn leaving_the_canvas_requests_a_redraw_and_leaving_the_window_clears_hover() {
        let doc = Document::default();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Bond(1),
            camera: Camera::default(),
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &GraphicStyle::default(),
            bracket_sides: BracketSides::Both,
        };
        let bounds = Rectangle::new(Point::new(100.0, 100.0), iced::Size::new(400.0, 300.0));
        let mut state = State {
            cursor: Some(Point::new(200.0, 150.0)),
            ..Default::default()
        };
        assert!(
            canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::CursorMoved {
                        position: Point::new(20.0, 20.0)
                    }),
                    bounds,
                    mouse::Cursor::Unavailable
                )
                .is_some()
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorLeft),
            bounds,
            mouse::Cursor::Unavailable,
        );
        assert!(state.cursor.is_none());
    }

    #[test]
    fn short_endpoint_drag_grows_instead_of_snapping_to_its_source() {
        let mut doc = Document::default();
        let source = doc.add_atom("C", World::default());
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Bond(1),
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &GraphicStyle::default(),
            bracket_sides: BracketSides::Both,
        };
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400.0, 300.0));
        let mut state = State::default();
        let end = Point::new(206.0, 150.0);
        let cursor = mouse::Cursor::Available(end);
        for event in [
            mouse::Event::CursorMoved {
                position: Point::new(200.0, 150.0),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(&mut state, &Event::Mouse(event), bounds, cursor);
        }
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap();
        let Some(Edit::Bond(start, end, Some(id), None)) = action.into_inner().0 else {
            panic!("short drag must extend, not connect the source to itself");
        };
        assert_eq!(id, source);
        assert_eq!(start, World::default());
        assert_eq!(end, World::new(42.0, 0.0));
    }

    #[test]
    fn drag_reuses_atoms_at_the_cursor_or_the_snapped_endpoint() {
        let mut doc = Document::default();
        let source = doc.add_atom("C", World::default());
        let target = doc.add_atom("C", World::new(42.0, 0.0));
        for cursor in [World::new(42.0, 3.0), World::new(80.0, 0.0)] {
            let (end, id) = bond_target(&doc, World::default(), cursor, Some(source), 12.0);
            assert_eq!(id, Some(target));
            assert_eq!(end, World::new(42.0, 0.0));
        }
    }

    #[test]
    fn fast_drag_uses_each_motion_event_instead_of_final_cursor_snapshot() {
        let doc = Document::default();
        let canvas = MoleculeCanvas {
            element: "C",
            joining: None,
            hidden_annotation: None,
            bond_drawing: Default::default(),
            chain_drawing: Default::default(),
            doc: &doc,
            selected: &[],
            tool: Tool::Bond(1),
            camera: Camera {
                center: World::default(),
                zoom: 1.0,
            },
            grid: false,
            guides: Default::default(),
            ring_size: 6,
            aromatic_ring: false,
            template_connection: reshiki::templates::Connection::Auto,
            template: None,
            arrow_preset: Default::default(),
            arrow_style: &reshiki::arrows::ArrowStyle::DEFAULT,
            orbital_phase: Default::default(),
            phase_flipped: false,
            attach_symbols: true,
            graphic_constrain: false,
            graphic_style: &GraphicStyle::default(),
            bracket_sides: BracketSides::Both,
        };
        let mut state = State::default();
        let bounds = Rectangle {
            x: 200.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
        };
        let origin = Point::new(350.0, 250.0);
        let end = Point::new(410.0, 250.0);
        let snapshot = mouse::Cursor::Available(end);
        for event in [
            mouse::Event::CursorMoved { position: origin },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved { position: end },
        ] {
            canvas.update(&mut state, &Event::Mouse(event), bounds, snapshot);
        }
        let action = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                snapshot,
            )
            .unwrap();
        let Some(Edit::Bond(start, end, None, None)) = action.into_inner().0 else {
            panic!("drag must produce a bond, not a click");
        };
        assert_eq!(start, World::new(-50.0, 0.0));
        assert_eq!(end, World::new(-8.0, 0.0));
    }
}
