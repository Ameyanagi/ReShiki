//! 3D model images on the canvas (#386). Iced has no radial gradient, so model
//! primitives are rasterized at device resolution by resvg from the same SVG
//! that figure export renders, then drawn as cached image tiles: smooth
//! shading at any zoom and the same pixels as PNG export.
use crate::canvas::{Camera, World, layered};
use iced::advanced::image::Handle;
use iced::widget::canvas;
use iced::{Point, Rectangle, Size};
use reshiki::scene::{self, Primitive};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Device pixels per logical pixel: Retina exactly, downsampled on 1× screens.
const OVERSAMPLE: f32 = 2.;
/// Device pixels per logical pixel while a model turns or morphs: a quarter
/// of the pixels to render, sharp again once it stops.
const TURNING: f32 = 1.;
/// Tile side in device pixels, on a grid fixed to the drawing so a pan
/// reuses its tiles. Iced uploads images of 2 MB or more on a worker and
/// skips them until then, so a new zoom would flash blank.
const TILE: i64 = 512;
/// Tile pixels kept for redraws, pans and zoom steps back: a little more than
/// a 5K screen.
const BUDGET: usize = 64 << 20;
/// Drawing units around each primitive's bounds: its outline and edge.
const MARGIN: f32 = 1.;

/// Rendered tiles by content key: the image, its bytes and when last drawn.
#[derive(Default)]
struct Cache {
    tiles: HashMap<u64, (Handle, usize, u64)>,
    bytes: usize,
    clock: u64,
}

impl Cache {
    fn get(
        &mut self,
        key: u64,
        size: [u32; 2],
        render: impl FnOnce() -> Option<Vec<u8>>,
    ) -> Option<Handle> {
        self.clock += 1;
        if let Some(tile) = self.tiles.get_mut(&key) {
            tile.2 = self.clock;
            return Some(tile.0.clone());
        }
        let pixels = render()?;
        let bytes = pixels.len();
        let handle = Handle::from_rgba(size[0], size[1], pixels);
        self.tiles.insert(key, (handle.clone(), bytes, self.clock));
        self.bytes += bytes;
        while self.bytes > BUDGET {
            let Some(oldest) = self.tiles.iter().min_by_key(|(_, t)| t.2).map(|(k, _)| *k) else {
                break;
            };
            if let Some((_, bytes, _)) = self.tiles.remove(&oldest) {
                self.bytes -= bytes;
            }
        }
        Some(handle)
    }
}

thread_local! {
    static TILES: RefCell<Cache> = RefCell::default();
    // Paper frames release TILES when no model is present on the drawing.
    // Result-panel thumbnails keep their small cache independently.
    static MODEL_IMAGES: RefCell<Cache> = RefCell::default();
    static SAMPLES: std::cell::Cell<f32> = const { std::cell::Cell::new(OVERSAMPLE) };
    static ROTATING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
thread_local! {
    pub(in crate::canvas) static TILE_RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A small prepared model image outside the paper, rasterized and cached by
/// the same SVG path as its canvas tiles and figure exports.
#[derive(Clone, Debug)]
pub(crate) struct ModelImage {
    run: std::rc::Rc<[Primitive]>,
    lo: World,
    hi: World,
    key: u64,
}

impl ModelImage {
    pub(crate) fn new(run: Vec<Primitive>, lo: World, hi: World) -> Self {
        let mut hasher = DefaultHasher::new();
        // Keep thumbnails in a separate cache-key domain from paper tiles.
        "model thumbnail".hash(&mut hasher);
        for primitive in &run {
            key(primitive, &mut hasher);
        }
        for value in [lo.x, lo.y, hi.x, hi.y] {
            value.to_bits().hash(&mut hasher);
        }
        Self {
            run: run.into(),
            lo,
            hi,
            key: hasher.finish(),
        }
    }

    /// Draw at `origin`, with `scale` logical pixels per drawing unit.
    pub(crate) fn draw(
        &self,
        frame: &mut canvas::Frame,
        origin: Point,
        scale: f32,
        theme: reshiki::canvas_theme::CanvasTheme,
    ) {
        let raster_scale = scale * OVERSAMPLE;
        let width = ((self.hi.x - self.lo.x) * raster_scale).ceil();
        let height = ((self.hi.y - self.lo.y) * raster_scale).ceil();
        if !raster_scale.is_finite()
            || raster_scale <= 0.
            || !(1. ..=4096.).contains(&width)
            || !(1. ..=4096.).contains(&height)
        {
            return;
        }
        let size = [width as u32, height as u32];
        let mut hasher = DefaultHasher::new();
        self.key.hash(&mut hasher);
        raster_scale.to_bits().hash(&mut hasher);
        size.hash(&mut hasher);
        theme.is_dark().hash(&mut hasher);
        let render = || pixels_on(&self.run, self.lo, raster_scale, size[0], size[1], theme);
        let Some(handle) =
            MODEL_IMAGES.with_borrow_mut(|cache| cache.get(hasher.finish(), size, render))
        else {
            return;
        };
        frame.draw_image(
            Rectangle::new(origin, Size::new(width / OVERSAMPLE, height / OVERSAMPLE)),
            canvas::Image::new(handle),
        );
    }
}

/// Set up the paper's next frame of `doc`: models render coarser while one
/// turns or morphs, and with no tiled paint the tiles are let go.
pub(in crate::canvas) fn begin(doc: &reshiki::document::Document) {
    use reshiki::models3d::Display;
    let turning = doc.models.iter().any(|m| m.turning);
    let moving = turning || doc.models.iter().any(|m| m.morph.is_some_and(|t| t < 1.));
    ROTATING.set(turning);
    SAMPLES.set(if moving { TURNING } else { OVERSAMPLE });
    if doc.models.iter().all(|m| m.display == Display::TwoD)
        && !reshiki::graphics::tiled(doc)
        && !doc.graphics.iter().any(|g| g.table.is_some())
    {
        TILES.take();
    }
}

/// End the paper's frame: other drawings render sharp.
pub(in crate::canvas) fn end() {
    SAMPLES.set(OVERSAMPLE);
    ROTATING.set(false);
}

/// A model primitive, or one fading in during the 2D → 3D morph.
pub(in crate::canvas) fn is_model(primitive: &Primitive) -> bool {
    match primitive {
        Primitive::Opacity { primitive, .. } => is_model(primitive),
        p => matches!(
            p,
            Primitive::Sphere { .. }
                | Primitive::Clip { .. }
                | Primitive::Ellipsoid { .. }
                | Primitive::ShadedQuad { .. }
                | Primitive::ShadedPolygon { .. }
                | Primitive::Contour { .. }
                | Primitive::Wire { .. }
                | Primitive::Card { .. }
                | Primitive::Surface { .. }
                | Primitive::Gradient { .. }
                | Primitive::Glow { .. }
        ),
    }
}

/// A primitive of a run, or one polygon of a contour in it.
type Part = (usize, Option<usize>);

/// A visible tile: its key, its device-pixel rectangle (the grid cell cut to
/// what it shows) and the parts reaching it, in order. The key covers only
/// those parts, so editing one model leaves another's tiles cached.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tile {
    key: u64,
    rect: [i64; 4],
    parts: Vec<Part>,
}

/// A surface reaches several tiles as a whole. During rotation a bounded
/// image avoids parsing and painting that full surface for each tile.
fn turning_tile(tiles: &[Tile]) -> Option<Tile> {
    let [first, rest @ ..] = tiles else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
    let mut rect = first.rect;
    for tile in rest {
        let [x0, y0, x1, y1] = tile.rect;
        rect = [
            rect[0].min(x0),
            rect[1].min(y0),
            rect[2].max(x1),
            rect[3].max(y1),
        ];
    }
    let [x0, y0, x1, y1] = rect;
    let (width, height) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    // Stay within the same upload budget as one ordinary tile, including for
    // long, narrow models.
    if !(1..=2 * TILE).contains(&width)
        || !(1..=2 * TILE).contains(&height)
        || width * height > TILE * TILE
    {
        return None;
    }
    let mut parts: Vec<_> = tiles
        .iter()
        .flat_map(|tile| tile.parts.iter().copied())
        .collect();
    parts.sort_unstable();
    parts.dedup();
    let mut hasher = DefaultHasher::new();
    "turning orbital".hash(&mut hasher);
    for tile in tiles {
        tile.key.hash(&mut hasher);
    }
    Some(Tile {
        key: hasher.finish(),
        rect,
        parts,
    })
}

fn has_orbital(primitive: &Primitive) -> bool {
    match primitive {
        Primitive::Opacity { primitive, .. } => has_orbital(primitive),
        Primitive::Surface { .. } => true,
        _ => false,
    }
}

/// A contour's polygons, also while it fades in.
fn contour(primitive: &Primitive) -> Option<&[Vec<World>]> {
    match primitive {
        Primitive::Opacity { primitive, .. } => contour(primitive),
        Primitive::Contour { polygons, .. } => Some(polygons),
        _ => None,
    }
}

/// `primitive` with only the contour polygons in `keep`.
fn keep_polygons(primitive: &Primitive, keep: &[usize]) -> Primitive {
    match primitive {
        Primitive::Opacity { alpha, primitive } => Primitive::Opacity {
            alpha: *alpha,
            primitive: Box::new(keep_polygons(primitive, keep)),
        },
        Primitive::Contour { polygons, outline } => Primitive::Contour {
            polygons: keep
                .iter()
                .filter_map(|&i| polygons.get(i).cloned())
                .collect(),
            outline: *outline,
        },
        p => p.clone(),
    }
}

/// The primitives of `parts`: a contour's polygons reaching a tile stay one
/// stroke, so where they overlap they still do not darken.
fn primitives(run: &[Primitive], parts: &[Part]) -> Vec<Primitive> {
    parts
        .chunk_by(|a, b| a.0 == b.0)
        .filter_map(|chunk| {
            let &(i, polygon) = chunk.first()?;
            let p = run.get(i)?;
            Some(match polygon {
                None => p.clone(),
                Some(_) => keep_polygons(p, &chunk.iter().filter_map(|c| c.1).collect::<Vec<_>>()),
            })
        })
        .collect()
}

/// A fitted path's structure and every coordinate bit, including controls.
fn path_key(commands: &[reshiki::graphics::PathCommand], hasher: &mut DefaultHasher) {
    use reshiki::graphics::PathCommand;
    let point = |p: World, hasher: &mut DefaultHasher| {
        p.x.to_bits().hash(hasher);
        p.y.to_bits().hash(hasher);
    };
    commands.len().hash(hasher);
    for command in commands {
        match command {
            PathCommand::Move(p) => {
                0_u8.hash(hasher);
                point(*p, hasher);
            }
            PathCommand::Line(p) => {
                1_u8.hash(hasher);
                point(*p, hasher);
            }
            PathCommand::Cubic(a, b, c) => {
                2_u8.hash(hasher);
                point(*a, hasher);
                point(*b, hasher);
                point(*c, hasher);
            }
            PathCommand::Close => 3_u8.hash(hasher),
        }
    }
}

/// Hash every coordinate and colour of `primitive` exactly: its `Debug`
/// output, but a ball's contact and a surface by their bits, as that output
/// would be long.
fn key(primitive: &Primitive, hasher: &mut DefaultHasher) {
    match primitive {
        Primitive::Opacity { alpha, primitive } => {
            alpha.to_bits().hash(hasher);
            key(primitive, hasher);
        }
        Primitive::Surface { groups } => {
            groups.len().hash(hasher);
            for group in groups {
                group.alpha.to_bits().hash(hasher);
                group.fills.len().hash(hasher);
                for fill in &group.fills {
                    fill.color.hash(hasher);
                    path_key(&fill.region, hasher);
                    fill.tones.len().hash(hasher);
                    for (commands, color) in &fill.tones {
                        color.hash(hasher);
                        path_key(commands, hasher);
                    }
                }
                group.strokes.len().hash(hasher);
                for stroke in &group.strokes {
                    stroke.outline.color.hash(hasher);
                    stroke.outline.alpha.to_bits().hash(hasher);
                    stroke.outline.width.to_bits().hash(hasher);
                    path_key(&stroke.paths, hasher);
                }
            }
        }
        Primitive::Sphere {
            center,
            radius,
            focal,
            stops,
            outline,
            contact,
        } => {
            format!("{center:?}{radius:?}{focal:?}{stops:?}{outline:?}").hash(hasher);
            contact.hash(hasher);
        }
        p => format!("{p:?}").hash(hasher),
    }
}

/// Bounds and hashes of an immutable scene run. The paper owns this alongside
/// the Rc scene, so neither pointer reuse nor an edited preview can stale it.
pub(in crate::canvas) struct PreparedRun {
    parts: Vec<PreparedPart>,
    plan: Option<(PlanKey, std::rc::Rc<[Tile]>)>,
    #[cfg(test)]
    plans: usize,
}

struct PreparedPart {
    lo: World,
    hi: World,
    key: u64,
    part: Part,
}

struct TileBuilder {
    hasher: DefaultHasher,
    rect: [i64; 4],
    parts: Vec<Part>,
}

#[derive(PartialEq, Eq)]
struct PlanKey {
    scale: u32,
    dark: bool,
    cells: [[i64; 2]; 2],
}

impl PreparedRun {
    pub fn new(run: &[Primitive]) -> Self {
        // A contour strokes every bond's outline at once; only its polygons
        // reaching a tile are copied for rasterization, with their order kept.
        let parts = run
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                let count = contour(p).map_or(1, |polygons| polygons.len());
                (0..count).map(move |j| {
                    let (part, p) = if contour(p).is_some() {
                        ((i, Some(j)), Cow::Owned(keep_polygons(p, &[j])))
                    } else {
                        ((i, None), Cow::Borrowed(p))
                    };
                    let (lo, hi) = scene::bounds(std::slice::from_ref(&*p));
                    let mut hasher = DefaultHasher::new();
                    key(&p, &mut hasher);
                    PreparedPart {
                        lo: lo.offset(-MARGIN, -MARGIN),
                        hi: hi.offset(MARGIN, MARGIN),
                        key: hasher.finish(),
                        part,
                    }
                })
            })
            .collect();
        Self {
            parts,
            plan: None,
            #[cfg(test)]
            plans: 0,
        }
    }

    fn plan(
        &mut self,
        camera: Camera,
        bounds: Rectangle,
        theme: reshiki::canvas_theme::CanvasTheme,
    ) -> std::rc::Rc<[Tile]> {
        let scale = camera.zoom * SAMPLES.get();
        let cells = [Point::ORIGIN, Point::new(bounds.width, bounds.height)].map(|p| {
            let p = camera.world(p, bounds);
            [p.x, p.y].map(|v| ((v * scale).floor() as i64).div_euclid(TILE))
        });
        let key = PlanKey {
            scale: scale.to_bits(),
            dark: theme.is_dark(),
            cells,
        };
        if let Some((cached, tiles)) = &self.plan
            && *cached == key
        {
            return tiles.clone();
        }
        #[cfg(test)]
        {
            self.plans += 1;
        }
        // Visit only cells each part reaches, instead of testing every part
        // against every visible cell. Sorted cells preserve the old tile order.
        let mut cells: BTreeMap<(i64, i64), TileBuilder> = BTreeMap::new();
        let [lo, hi] = key.cells;
        for part in &self.parts {
            let b = [
                (part.lo.x * scale).floor(),
                (part.lo.y * scale).floor(),
                (part.hi.x * scale).ceil() + 1.,
                (part.hi.y * scale).ceil() + 1.,
            ]
            .map(|v| v as i64);
            let first = [
                b[0].div_euclid(TILE).max(lo[0]),
                b[1].div_euclid(TILE).max(lo[1]),
            ];
            let last = [
                (b[2] - 1).div_euclid(TILE).min(hi[0]),
                (b[3] - 1).div_euclid(TILE).min(hi[1]),
            ];
            for ty in first[1]..=last[1] {
                for tx in first[0]..=last[0] {
                    let cell = cells.entry((ty, tx)).or_insert_with(|| {
                        let mut hasher = DefaultHasher::new();
                        key.scale.hash(&mut hasher);
                        key.dark.hash(&mut hasher);
                        TileBuilder {
                            hasher,
                            rect: [i64::MAX, i64::MAX, i64::MIN, i64::MIN],
                            parts: vec![],
                        }
                    });
                    part.key.hash(&mut cell.hasher);
                    cell.rect = [
                        cell.rect[0].min(b[0]),
                        cell.rect[1].min(b[1]),
                        cell.rect[2].max(b[2]),
                        cell.rect[3].max(b[3]),
                    ];
                    cell.parts.push(part.part);
                }
            }
        }
        let tiles: std::rc::Rc<[Tile]> = cells
            .into_iter()
            .map(|((ty, tx), cell)| {
                let TileBuilder {
                    mut hasher,
                    rect,
                    parts,
                } = cell;
                let rect = [
                    rect[0].max(tx * TILE),
                    rect[1].max(ty * TILE),
                    rect[2].min((tx + 1) * TILE),
                    rect[3].min((ty + 1) * TILE),
                ];
                rect.hash(&mut hasher);
                Tile {
                    key: hasher.finish(),
                    rect,
                    parts,
                }
            })
            .collect();
        self.plan = Some((key, tiles.clone()));
        tiles
    }

    pub fn draw(
        &mut self,
        frame: &mut layered::Frame<'_>,
        run: &[Primitive],
        camera: Camera,
        bounds: Rectangle,
        opacity: f32,
    ) {
        let tiles = self.plan(camera, bounds, frame.canvas_theme());
        draw_tiles(frame, run, &tiles, camera, bounds, opacity);
    }
}

#[cfg(test)]
fn plan(run: &[Primitive], camera: Camera, bounds: Rectangle) -> Vec<Tile> {
    plan_on(run, camera, bounds, Default::default())
}

#[cfg(test)]
fn plan_on(
    run: &[Primitive],
    camera: Camera,
    bounds: Rectangle,
    theme: reshiki::canvas_theme::CanvasTheme,
) -> Vec<Tile> {
    let mut prepared = PreparedRun::new(run);
    prepared.plan(camera, bounds, theme).to_vec()
}

/// Draw a run of consecutive model primitives as the tiles that show.
pub(in crate::canvas) fn draw(
    frame: &mut layered::Frame<'_>,
    run: &[Primitive],
    camera: Camera,
    bounds: Rectangle,
    opacity: f32,
) {
    PreparedRun::new(run).draw(frame, run, camera, bounds, opacity);
}

fn draw_tiles(
    frame: &mut layered::Frame<'_>,
    run: &[Primitive],
    tiles: &[Tile],
    camera: Camera,
    bounds: Rectangle,
    opacity: f32,
) {
    let oversample = SAMPLES.get();
    let scale = camera.zoom * oversample;
    let theme = frame.canvas_theme();
    let merged = if ROTATING.get() && run.iter().any(has_orbital) {
        turning_tile(tiles)
    } else {
        None
    };
    let tiles = merged.as_ref().map_or(tiles, std::slice::from_ref);
    let Some([ox, oy, ..]) = tiles.first().map(|tile| tile.rect) else {
        return;
    };
    // Snapped to whole device pixels once, so the tiles are not resampled
    // and meet exactly.
    let world = |x: i64, y: i64| World::new(x as f32 / scale, y as f32 / scale);
    let anchor = camera.screen(world(ox, oy), bounds);
    let anchor = Point::new(
        (anchor.x * oversample).round() / oversample,
        (anchor.y * oversample).round() / oversample,
    );
    let logical = |v: i64| v as f32 / oversample;
    frame.split();
    for tile in tiles {
        let [x0, y0, x1, y1] = tile.rect;
        let size = [(x1 - x0) as u32, (y1 - y0) as u32];
        let render = || {
            #[cfg(test)]
            TILE_RENDERS.with(|n| n.set(n.get() + 1));
            pixels_on(
                &primitives(run, &tile.parts),
                world(x0, y0),
                scale,
                size[0],
                size[1],
                theme,
            )
        };
        let Some(handle) = TILES.with_borrow_mut(|cache| cache.get(tile.key, size, render)) else {
            continue;
        };
        frame.draw_image(
            Rectangle::new(
                anchor + iced::Vector::new(logical(x0 - ox), logical(y0 - oy)),
                Size::new(logical(x1 - x0), logical(y1 - y0)),
            ),
            canvas::Image::new(handle).opacity(opacity),
        );
    }
    frame.split();
}

/// Straight-alpha RGBA pixels of `run`, rendered as figure export does.
#[cfg(test)]
pub(in crate::canvas) fn pixels(
    run: &[Primitive],
    origin: World,
    scale: f32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    pixels_on(run, origin, scale, width, height, Default::default())
}

fn pixels_on(
    run: &[Primitive],
    origin: World,
    scale: f32,
    width: u32,
    height: u32,
    theme: reshiki::canvas_theme::CanvasTheme,
) -> Option<Vec<u8>> {
    let hi = origin.offset(width as f32 / scale, height as f32 / scale);
    let svg = scene::svg_part_on(run, origin, hi, theme, Some(theme.background()));
    let tree = reshiki::export::parse_svg(svg).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(
        pixmap
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
