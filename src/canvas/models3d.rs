//! 3D models on the canvas (#386). Iced has no radial gradient, so each run of
//! model primitives is rasterized at device resolution by resvg from the same
//! SVG that figure export renders, then drawn as cached image tiles: smooth
//! shading at any zoom and the same pixels as PNG export.
use super::{Camera, World, layered};
use iced::advanced::image::Handle;
use iced::widget::canvas;
use iced::{Point, Rectangle, Size};
use reshiki::scene::{self, Primitive};
use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Device pixels per logical pixel: Retina exactly, downsampled on 1× screens.
const OVERSAMPLE: f32 = 2.;
/// Larger models are rasterized only where the paper shows them.
const MAX_PIXELS: f32 = 3000. * 3000.;
/// Images kept for redraws and zoom steps back.
const CACHED: usize = 4;
/// Tile side in device pixels. Iced uploads images of 2 MB or more on a
/// worker and skips them until then, so a new zoom would flash blank.
const TILE: u32 = 512;

/// A tile's offset and size in device pixels within the model image.
type Tile = ([u32; 4], Handle);

thread_local! {
    static IMAGES: RefCell<Vec<(u64, Vec<Tile>)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn is_model(primitive: &Primitive) -> bool {
    matches!(
        primitive,
        Primitive::Sphere { .. }
            | Primitive::ShadedQuad { .. }
            | Primitive::Wire { .. }
            | Primitive::Card { .. }
    )
}

/// Draw a run of consecutive model primitives as one image.
pub(super) fn draw(
    frame: &mut layered::Frame<'_>,
    run: &[Primitive],
    camera: Camera,
    bounds: Rectangle,
    opacity: f32,
) {
    let (lo, hi) = scene::bounds(run);
    let (a, b) = (camera.screen(lo, bounds), camera.screen(hi, bounds));
    let whole = Rectangle::new(a, Size::new(b.x - a.x, b.y - a.y));
    let (area, origin, clipped) = if whole.area() * OVERSAMPLE * OVERSAMPLE <= MAX_PIXELS {
        (whole, lo, false)
    } else {
        let Some(visible) = whole.intersection(&Rectangle::with_size(bounds.size())) else {
            return;
        };
        (visible, camera.world(visible.position(), bounds), true)
    };
    let scale = camera.zoom * OVERSAMPLE;
    let width = (area.width * OVERSAMPLE).ceil().max(1.);
    let height = (area.height * OVERSAMPLE).ceil().max(1.);
    let mut hasher = DefaultHasher::new();
    // Debug output names every coordinate and colour exactly.
    format!("{run:?}").hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    if clipped {
        [origin.x, origin.y, width, height]
            .map(f32::to_bits)
            .hash(&mut hasher);
    }
    let key = hasher.finish();
    let tiles = IMAGES.with_borrow_mut(|images| {
        let tiles = match images.iter().position(|(k, _)| *k == key) {
            Some(i) => images.remove(i).1,
            None => tiles(run, origin, scale, width as u32, height as u32),
        };
        images.insert(0, (key, tiles.clone()));
        images.truncate(CACHED);
        tiles
    });
    // Snapped to whole device pixels, so the tiles are not resampled.
    let at = Point::new(
        (area.x * OVERSAMPLE).round() / OVERSAMPLE,
        (area.y * OVERSAMPLE).round() / OVERSAMPLE,
    );
    frame.split();
    for ([x, y, w, h], handle) in tiles {
        let device = |v: u32| v as f32 / OVERSAMPLE;
        frame.draw_image(
            Rectangle::new(
                at + iced::Vector::new(device(x), device(y)),
                Size::new(device(w), device(h)),
            ),
            canvas::Image::new(handle).opacity(opacity),
        );
    }
    frame.split();
}

/// `run` from `origin` at `scale` pixels per drawing unit, as image tiles.
fn tiles(run: &[Primitive], origin: World, scale: f32, width: u32, height: u32) -> Vec<Tile> {
    let Some(pixels) = pixels(run, origin, scale, width, height) else {
        return vec![];
    };
    let row = width as usize * 4;
    let mut tiles = vec![];
    for y in (0..height).step_by(TILE as usize) {
        for x in (0..width).step_by(TILE as usize) {
            let (w, h) = (TILE.min(width - x), TILE.min(height - y));
            let tile: Vec<u8> = (y..y + h)
                .flat_map(|line| {
                    let start = line as usize * row + x as usize * 4;
                    pixels
                        .get(start..start + w as usize * 4)
                        .unwrap_or_default()
                        .iter()
                        .copied()
                })
                .collect();
            tiles.push(([x, y, w, h], Handle::from_rgba(w, h, tile)));
        }
    }
    tiles
}

/// Straight-alpha RGBA pixels of `run`, rendered as figure export does.
pub(super) fn pixels(
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
