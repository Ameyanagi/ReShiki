//! The molecule canvas: the iced program that draws the paper and turns pointer
//! events into drawing edits. This root holds the shared canvas types and the
//! `Program` entry points; the submodules do the work:
//!
//! - `cache`: document-derived scene, selection and guide data reused across frames.
//! - `dashes`: dashed strokes that keep curves inside each dash.
//! - `grid`: the paper's dot grid and its retained geometry.
//! - `guides`: rulers, crosshair and measuring units.
//! - `hit`: which objects a pointer or region selects, and where a drawn bond attaches.
//! - `input`: pointer and keyboard events, split into presses, motion and releases.
//! - `layered`: keeps geometry order across vector and raster layers.
//! - `markers`: selection decorations retained with the current scene.
//! - `movement`: the move constraint shared by the drag preview and the committed edit.
//! - `optimization`: pointer interaction for a detached optimization preview.
//! - `pages`: page sheets behind the drawing.
//! - `paper`: draws the paper layer by layer, previewing the active gesture.
//! - `previews`: thumbnail, inspector, palette and proposal canvas programs.
//! - `render`: tessellates scene primitives into a layered frame.
//! - `selection`: the selection box, its transform handles and drags.
//! - `smart_guides`: snaps dragged edges, centers and gaps to other objects.
//! - `snapping`: where a gesture lands, shared by event handling and the drag preview.
//! - `text_cache`: bounded, position-independent text outlines.
//! - `tilt`: screen-space tilt gestures.
//! - `tool`: drawing tools and their status-bar hints.

mod cache;
mod dashes;
mod grid;
pub mod guides;
mod hit;
mod input;
pub mod layered;
mod markers;
mod movement;
pub(crate) mod nmr;
pub(crate) mod optimization;
mod pages;
mod paper;
mod previews;
mod render;
mod selection;
mod smart_guides;
mod snapping;
mod text_cache;
pub(crate) mod tilt;
mod tool;

#[cfg(test)]
pub(crate) mod label_click_tests;
#[cfg(test)]
mod performance;
#[cfg(test)]
mod render_parity_tests;
#[cfg(test)]
pub(crate) mod rotation_gesture_tests;
#[cfg(test)]
mod template_style_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod pen_tests;
#[cfg(test)]
mod transform_shortcut_tests;

use grid::GridCache;
pub use hit::{distance_to_segment, hit_object};
use iced::widget::canvas::{self, Action, Geometry, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, Vector, mouse};
pub use previews::{
    ArrowPreview, DrawingPreview, DrawingThumbnail, OwnedDrawingPreview, PalettePreview,
    ScientificPreview, TemplateAnchorPreview, TemplateThumbnail,
};
use reshiki::{
    chains::{BondDrawing, ChainDrawing},
    document::{Document, Point as World},
    graphics::{BracketSides, GraphicStyle},
};
use selection::{Handle, SelectionBox, TransformDrag};
pub(crate) use text_cache::prepare_fonts;
pub use tool::Tool;

// Names the use super::* test modules resolve through this root.
#[cfg(test)]
use grid::grid_dots;
#[cfg(test)]
use hit::{bond_target, bond_target_with, hit_selection, region_selection};
#[cfg(test)]
use previews::PreviewState;
#[cfg(test)]
use render::draw_primitives;
#[cfg(test)]
use reshiki::chains::{self, ChainMode};
#[cfg(test)]
use reshiki::graphics::{Graphic, GraphicKind, PathCommand};
pub(crate) mod pen;
#[cfg(test)]
use reshiki::scene::{Primitive, primitives};
#[cfg(test)]
use smart_guides::{Axis, Guide};
#[cfg(test)]
use snapping::ring_gesture;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformField {
    Rotation,
    Scale,
    Width,
    Height,
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
    /// Open an existing numeric control without transforming the drawing.
    BeginTransform(TransformField),
    Hover(Option<World>),
    Chain {
        points: Vec<World>,
        source: Option<u64>,
        target: Option<u64>,
    },
    Graphic(World, World, bool),
    PenSegment(pen::Stroke),
    /// Capture orbital snap preference and temporary bypass at release, just
    /// as the live preview uses them; a chemical symbol's attachment is separate.
    Orbital(World, World, bool, bool),
    GraphicPoint(u64, usize, World),
    AtomMark(u64, usize, World),
    AtomIndicator(reshiki::atom_labels::Owner, World),
    ArrowHandle(u64, usize, World),
    ArrowClick(u64),
    ArrowTarget(World, bool),
    CancelArrowSource,
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
    /// Stretch a bridge without invoking automatic ring fusion or atom merging.
    StretchBond {
        fixed: u64,
        moving: u64,
        length: f32,
    },
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
    PathPoint(pen::PointDrag),
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
    StretchBond {
        start: World,
        plan: reshiki::editing::reference::Stretch,
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
    pub(crate) nmr: Option<nmr::Context<'a>>,
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
    pub arrow_source: Option<&'a reshiki::arrow_anchors::Pick>,
    pub attach_arrow_targets: bool,
    pub orbital_phase: reshiki::scientific::Phase,
    pub phase_flipped: bool,
    pub attach_symbols: bool,
    pub snap_orbitals: bool,
    pub graphic_constrain: bool,
    pub graphic_arc: reshiki::graphics::ArcGeometry,
    pub graphic_style: &'a GraphicStyle,
    pub graphic_point: Option<(u64, usize)>,
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
