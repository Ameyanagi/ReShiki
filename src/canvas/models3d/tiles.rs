//! 3D model images on the canvas (#386). Iced has no radial gradient, so model
//! primitives are rasterized at device resolution by resvg from the same SVG
//! that figure export renders, then drawn as cached image tiles: smooth
//! shading at any zoom and the same pixels as PNG export.
use crate::canvas::{Camera, World, layered};
use iced::advanced::image::Handle;
use iced::widget::canvas;
use iced::{Point, Rectangle, Size};
use reshiki::scene::{self, Primitive};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Device pixels per logical pixel: Retina exactly, downsampled on 1× screens.
const OVERSAMPLE: f32 = 2.;
/// Tile side in device pixels, on a grid fixed to the drawing so a pan
/// reuses its tiles. Iced uploads images of 2 MB or more on a worker and
/// skips them until then, so a new zoom would flash blank.
const TILE: i64 = 512;
/// Tile pixels kept for redraws, pans and zoom steps back.
const BUDGET: usize = 128 << 20;
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
}

/// A model primitive, or one fading in during the 2D → 3D morph.
pub(in crate::canvas) fn is_model(primitive: &Primitive) -> bool {
    match primitive {
        Primitive::Opacity { primitive, .. } => is_model(primitive),
        p => matches!(
            p,
            Primitive::Sphere { .. }
                | Primitive::ShadedQuad { .. }
                | Primitive::ShadedPolygon { .. }
                | Primitive::Contour { .. }
                | Primitive::Wire { .. }
                | Primitive::Card { .. }
        ),
    }
}

/// A visible tile: its key, its device-pixel rectangle (the grid cell cut to
/// what it shows) and the primitives reaching it, in order. The key covers
/// only those primitives, so editing one model leaves another's tiles cached.
struct Tile {
    key: u64,
    rect: [i64; 4],
    parts: Vec<usize>,
}

fn plan(run: &[Primitive], camera: Camera, bounds: Rectangle) -> Vec<Tile> {
    let scale = camera.zoom * OVERSAMPLE;
    let device = |p: World| [p.x * scale, p.y * scale];
    let parts: Vec<_> = run
        .iter()
        .map(|p| {
            let (lo, hi) = scene::bounds(std::slice::from_ref(p));
            let ([x0, y0], [x1, y1]) = (
                device(lo.offset(-MARGIN, -MARGIN)),
                device(hi.offset(MARGIN, MARGIN)),
            );
            let mut hasher = DefaultHasher::new();
            // Debug output names every coordinate and colour exactly.
            format!("{p:?}").hash(&mut hasher);
            (
                [x0.floor(), y0.floor(), x1.ceil() + 1., y1.ceil() + 1.].map(|v| v as i64),
                hasher.finish(),
            )
        })
        .collect();
    let [lo, hi] = [Point::ORIGIN, Point::new(bounds.width, bounds.height)]
        .map(|p| device(camera.world(p, bounds)).map(|v| (v.floor() as i64).div_euclid(TILE)));
    let mut tiles = vec![];
    for ty in lo[1]..=hi[1] {
        for tx in lo[0]..=hi[0] {
            let cell = [tx * TILE, ty * TILE, (tx + 1) * TILE, (ty + 1) * TILE];
            let mut hasher = DefaultHasher::new();
            scale.to_bits().hash(&mut hasher);
            let mut rect = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
            let mut reaching = vec![];
            for (i, (b, key)) in parts.iter().enumerate() {
                if b[0] < cell[2] && b[2] > cell[0] && b[1] < cell[3] && b[3] > cell[1] {
                    key.hash(&mut hasher);
                    rect = [
                        rect[0].min(b[0]),
                        rect[1].min(b[1]),
                        rect[2].max(b[2]),
                        rect[3].max(b[3]),
                    ];
                    reaching.push(i);
                }
            }
            if reaching.is_empty() {
                continue;
            }
            let rect = [
                rect[0].max(cell[0]),
                rect[1].max(cell[1]),
                rect[2].min(cell[2]),
                rect[3].min(cell[3]),
            ];
            rect.hash(&mut hasher);
            tiles.push(Tile {
                key: hasher.finish(),
                rect,
                parts: reaching,
            });
        }
    }
    tiles
}

/// Draw a run of consecutive model primitives as the tiles that show.
pub(in crate::canvas) fn draw(
    frame: &mut layered::Frame<'_>,
    run: &[Primitive],
    camera: Camera,
    bounds: Rectangle,
    opacity: f32,
) {
    let scale = camera.zoom * OVERSAMPLE;
    let tiles = plan(run, camera, bounds);
    let Some([ox, oy, ..]) = tiles.first().map(|tile| tile.rect) else {
        return;
    };
    // Snapped to whole device pixels once, so the tiles are not resampled
    // and meet exactly.
    let world = |x: i64, y: i64| World::new(x as f32 / scale, y as f32 / scale);
    let anchor = camera.screen(world(ox, oy), bounds);
    let anchor = Point::new(
        (anchor.x * OVERSAMPLE).round() / OVERSAMPLE,
        (anchor.y * OVERSAMPLE).round() / OVERSAMPLE,
    );
    let logical = |v: i64| v as f32 / OVERSAMPLE;
    frame.split();
    for tile in tiles {
        let [x0, y0, x1, y1] = tile.rect;
        let size = [(x1 - x0) as u32, (y1 - y0) as u32];
        let render = || {
            let part: Vec<_> = tile
                .parts
                .iter()
                .filter_map(|&i| run.get(i).cloned())
                .collect();
            pixels(&part, world(x0, y0), scale, size[0], size[1])
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
pub(in crate::canvas) fn pixels(
    run: &[Primitive],
    origin: World,
    scale: f32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let hi = origin.offset(width as f32 / scale, height as f32 / scale);
    let svg = scene::svg_part(run, origin, hi);
    let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).ok()?;
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
