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
mod input;
#[cfg(test)]
pub(crate) mod label_click_tests;
pub mod layered;
mod markers;
mod movement;
pub(crate) mod optimization;
mod pages;
#[cfg(test)]
mod performance;
mod previews;
mod render;
#[cfg(test)]
mod render_parity_tests;
#[cfg(test)]
pub(crate) mod rotation_gesture_tests;
mod selection;
mod smart_guides;
mod snapping;
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
#[cfg(test)]
use previews::PreviewState;
pub use previews::{
    ArrowPreview, DrawingPreview, DrawingThumbnail, OwnedDrawingPreview, PalettePreview,
    ScientificPreview, TemplateAnchorPreview, TemplateThumbnail,
};
use render::{draw_document, draw_primitives};
#[cfg(test)]
use reshiki::scene::{Primitive, primitives};
use selection::{Handle, SelectionBox, TransformDrag};
#[cfg(test)]
use smart_guides::{Axis, Guide};
use snapping::{arrow_endpoint, ring_gesture};
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

impl canvas::Program<Edit> for MoleculeCanvas<'_> {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Edit>> {
        self.handle_event(state, event, bounds, cursor)
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
        self.pointer_interaction(state, bounds, cursor)
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
