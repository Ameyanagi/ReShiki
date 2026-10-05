use iced::widget::canvas::{self, Action, Geometry, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, Vector, mouse};
#[cfg(test)]
use reshiki::chains;
#[cfg(test)]
use reshiki::graphics::Graphic;
use reshiki::{
    chains::{BondDrawing, ChainDrawing, ChainMode},
    document::{Document, Point as World},
    graphics::{BracketSides, GraphicKind, GraphicStyle, PathCommand},
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
mod paper;
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
use hit::{bond_target, region_selection};
use hit::{bond_target_with, hit_selection};
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
#[cfg(test)]
use snapping::ring_gesture;

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
        let mut draft = paper::Draft::new(self.joining.map(|(j, _)| &j.base).unwrap_or(self.doc));
        self.preview_tilt(&mut draft, state, bounds);
        self.preview_chain(&mut draft, frame, state, bounds);
        self.preview_pointer_edits(&mut draft, state, bounds);
        self.preview_transform(&mut draft, state, bounds);
        self.preview_ring(&mut draft, state, bounds);
        self.preview_ring_preset(&mut draft, state, bounds);
        self.preview_template(&mut draft, frame, state, bounds);
        self.preview_move(&mut draft, state, bounds);
        self.preview_region(&mut draft, state, bounds);
        self.preview_delocalized_ring(&mut draft, state, bounds);
        self.preview_bonded_atom(&mut draft, state, bounds);
        self.preview_bond(&mut draft, frame, state, bounds);
        let paper::Draft {
            mut preview,
            ring_selection,
            chain_badge,
            template_notice,
            rejection,
            translation,
            smart,
        } = draft;
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
