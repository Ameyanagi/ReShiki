// Adapted from iced_graphics 0.14.0 Text::draw_with (MIT).
// Copyright 2019 Héctor Ramón, Iced contributors.
// See licenses/iced/NOTICE and licenses/iced/LICENSE.
//! Share hinted glyphs between captions without changing Iced's shaping or size.
use iced::advanced::{graphics::text, text::Paragraph as _};
use iced::widget::canvas::{Path, Text};
use iced::widget::image::Handle;
use iced::{Color, Point, Rectangle, Size, Vector};
use std::collections::HashMap;
use text::cosmic_text::{CacheKey, Command, FontSystem, SubpixelBin, SwashCache, SwashContent};

mod variable;

pub(super) fn prepare_fonts() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static PREPARED: std::sync::Once = std::sync::Once::new();
    /// How many of the fonts added at runtime Iced has.
    static ADDED: AtomicUsize = AtomicUsize::new(0);
    PREPARED.call_once(|| {
        let mut fonts = text::font_system()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(not(target_arch = "wasm32"))]
        reshiki::style::load_bundled_fonts(fonts.raw().db_mut());
        // The browser has no system fonts; use the faces the drawing renders with.
        #[cfg(target_arch = "wasm32")]
        {
            reshiki::style::load_fonts(fonts.raw().db_mut());
            // Liberation Sans lacks the Mac key symbols that shortcut labels
            // use; see licenses/dejavu-fonts.
            fonts
                .raw()
                .db_mut()
                .load_font_data(include_bytes!("../../../assets/fonts/ReShikiKeys.ttf").to_vec());
            ADDED.store(reshiki::style::font_generation(), Ordering::Relaxed);
        }
        variable::prepare_system_weights(fonts.raw());
    });
    let added = reshiki::style::added_fonts(ADDED.load(Ordering::Relaxed));
    if !added.is_empty() {
        let mut fonts = text::font_system()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ADDED.fetch_add(added.len(), Ordering::Relaxed);
        for source in added {
            match source {
                // Through Iced, so text it already shaped is shaped again
                // and draws glyphs that were missing.
                text::cosmic_text::fontdb::Source::Binary(bytes) => {
                    fonts.load_font((*bytes).as_ref().to_vec().into());
                }
                source => {
                    fonts.raw().db_mut().load_font_source(source);
                }
            }
        }
        variable::prepare_system_weights(fonts.raw());
        reshiki::style::prefer_added_arial(fonts.raw().db_mut());
    }
}

const BUDGET: usize = 2 * 1024 * 1024;
const LIMIT: usize = 4096;
/// Colour glyphs are rasterized at twice their size, so they stay sharp on a
/// Retina screen, but at most this many pixels per em: emoji bitmaps are at
/// most 160 px, and larger pictures only cost memory.
const PICTURE_PPEM: f32 = 256.;

/// A colour glyph (emoji) as a picture: where it sits, in pixels at the
/// paragraph's size, relative to the glyph's origin.
type Picture = Option<(Rectangle, Handle)>;

/// Whether the face has colour glyphs: an sbix, CBDT or COLR font.
fn colour_face(fonts: &mut FontSystem, key: CacheKey) -> bool {
    fonts
        .get_font(key.font_id, key.font_weight)
        .is_some_and(|font| {
            let face = font.as_swash();
            [*b"sbix", *b"CBDT", *b"COLR"]
                .into_iter()
                .any(|tag| face.table(swash::Tag::from_be_bytes(tag)).is_some())
        })
}

/// The colour picture of `key` from a colour face, or `None` when this glyph
/// has only an outline. Apple Color Emoji's `glyf` outlines are empty
/// markers, so its emoji must draw from their pictures.
fn picture(fonts: &mut FontSystem, cache: &mut SwashCache, key: CacheKey) -> Picture {
    let size = f32::from_bits(key.font_size_bits);
    let ppem = (size * 2.).min(PICTURE_PPEM);
    let image = cache.get_image_uncached(
        fonts,
        CacheKey {
            font_size_bits: ppem.to_bits(),
            ..key
        },
    )?;
    let p = image.placement;
    if image.content != SwashContent::Color || p.width == 0 || p.height == 0 {
        return None;
    }
    let mut rgba = image.data;
    // Swash composites COLR layers premultiplied; Iced expects straight alpha.
    if matches!(image.source, swash::scale::Source::ColorOutline(_)) {
        for px in rgba.as_chunks_mut::<4>().0 {
            let a = u32::from(px[3]);
            if a > 0 && a < 255 {
                for c in &mut px[..3] {
                    *c = (u32::from(*c) * 255 / a).min(255) as u8;
                }
            }
        }
    }
    let scale = size / ppem;
    Some((
        Rectangle {
            x: p.left as f32 * scale,
            y: -p.top as f32 * scale,
            width: p.width as f32 * scale,
            height: p.height as f32 * scale,
        },
        Handle::from_rgba(p.width, p.height, rgba),
    ))
}

pub(super) struct Glyphs {
    cache: SwashCache,
    variable_scaler: swash::scale::ScaleContext,
    /// Which faces have colour glyphs.
    colour_faces: HashMap<text::cosmic_text::fontdb::ID, bool>,
    /// One picture per glyph of a colour face, so repeated emoji share one image.
    pictures: HashMap<CacheKey, Picture>,
    cost: usize,
}

impl Default for Glyphs {
    fn default() -> Self {
        Self {
            cache: SwashCache::new(),
            variable_scaler: swash::scale::ScaleContext::new(),
            colour_faces: HashMap::new(),
            pictures: HashMap::new(),
            cost: 0,
        }
    }
}

impl Glyphs {
    /// Keep Iced's unwrapped paragraph placement and raster fallback while
    /// honoring variable weights in both family selection and cached outlines.
    /// Colour glyphs (emoji) come out as pictures in `picture`.
    pub fn draw(
        &mut self,
        t: &Text,
        mut draw: impl FnMut(Path, Color),
        mut picture: impl FnMut(Rectangle, Handle),
    ) {
        prepare_fonts();
        let paragraph = text::Paragraph::with_text(iced::advanced::text::Text {
            content: &t.content,
            bounds: Size::new(t.max_width, f32::INFINITY),
            size: t.size,
            line_height: t.line_height,
            font: t.font,
            align_x: t.align_x,
            align_y: t.align_y,
            shaping: t.shaping,
            wrapping: Default::default(),
        });
        // Our outline caller always uses the origin with top-left alignment.
        debug_assert_eq!(t.position, Point::ORIGIN);
        debug_assert_eq!(t.align_x, iced::advanced::text::Alignment::Default);
        debug_assert_eq!(t.align_y, iced::alignment::Vertical::Top);
        let mut fonts = text::font_system()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for run in paragraph.buffer().layout_runs() {
            for glyph in run.glyphs {
                let key = glyph.physical((0., 0.), 1.).cache_key;
                // Shaping offsets are in ems, and positive y is up.
                let offset = Vector::new(
                    glyph.x + glyph.font_size * glyph.x_offset,
                    run.line_y - glyph.font_size * glyph.y_offset,
                );
                let colour = *self
                    .colour_faces
                    .entry(key.font_id)
                    .or_insert_with(|| colour_face(fonts.raw(), key));
                // Pictures are placed at the exact offset, so every subpixel
                // position shares one.
                let picture_key = CacheKey {
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                    ..key
                };
                let found = match self.pictures.get(&picture_key) {
                    _ if !colour => None,
                    Some(found) => found.clone(),
                    None => {
                        let found = self::picture(fonts.raw(), &mut self.cache, picture_key);
                        self.cost += std::mem::size_of::<(CacheKey, Picture)>()
                            + match &found {
                                Some((_, Handle::Rgba { pixels, .. })) => pixels.len(),
                                _ => 0,
                            };
                        self.pictures.insert(picture_key, found.clone());
                        found
                    }
                };
                if let Some((rect, handle)) = found {
                    picture(rect + offset, handle);
                    self.trim();
                    continue;
                }
                let new_outline = !self.cache.outline_command_cache.contains_key(&key);
                if new_outline {
                    variable::cache_outline(
                        fonts.raw(),
                        &mut self.cache,
                        &mut self.variable_scaler,
                        key,
                    );
                }
                if let Some(commands) = self.cache.get_outline_commands(fonts.raw(), key) {
                    if new_outline {
                        self.cost += std::mem::size_of_val(commands)
                            + std::mem::size_of_val(&key)
                            + std::mem::size_of::<Option<Box<[Command]>>>();
                    }
                    draw(
                        Path::new(|path| {
                            for command in commands {
                                match command {
                                    Command::MoveTo(p) => {
                                        path.move_to(Point::new(p.x, -p.y) + offset);
                                    }
                                    Command::LineTo(p) => {
                                        path.line_to(Point::new(p.x, -p.y) + offset);
                                    }
                                    Command::CurveTo(a, b, p) => path.bezier_curve_to(
                                        Point::new(a.x, -a.y) + offset,
                                        Point::new(b.x, -b.y) + offset,
                                        Point::new(p.x, -p.y) + offset,
                                    ),
                                    Command::QuadTo(a, p) => path.quadratic_curve_to(
                                        Point::new(a.x, -a.y) + offset,
                                        Point::new(p.x, -p.y) + offset,
                                    ),
                                    Command::Close => path.close(),
                                }
                            }
                        }),
                        t.color,
                    );
                } else {
                    if new_outline {
                        self.cost += std::mem::size_of_val(&key)
                            + std::mem::size_of::<Option<Box<[Command]>>>();
                    }
                    // Keep Iced's raster fallback for bitmap/color glyphs.
                    let new_image = !self.cache.image_cache.contains_key(&key);
                    let [r, g, b, a] = t.color.into_rgba8();
                    self.cache.with_pixels(
                        fonts.raw(),
                        key,
                        text::cosmic_text::Color::rgba(r, g, b, a),
                        |x, y, color| {
                            draw(
                                Path::rectangle(
                                    Point::new(x as f32, y as f32) + offset,
                                    Size::new(1., 1.),
                                ),
                                Color::from_rgba8(
                                    color.r(),
                                    color.g(),
                                    color.b(),
                                    color.a() as f32 / 255.,
                                ),
                            );
                        },
                    );
                    if new_image {
                        self.cost += self.cache.image_cache.get(&key).map_or(0, |image| {
                            std::mem::size_of_val(image)
                                + image.as_ref().map_or(0, |image| image.data.len())
                        }) + std::mem::size_of_val(&key);
                    }
                }
                self.trim();
            }
        }
    }

    /// An oversize glyph can be drawn, but is never retained beyond this
    /// call. Negative lookups, raster data and pictures are all bounded.
    fn trim(&mut self) {
        if self.cost > BUDGET || self.len() > LIMIT {
            self.cache = SwashCache::new();
            self.variable_scaler = swash::scale::ScaleContext::new();
            self.pictures.clear();
            self.cost = 0;
        }
    }

    fn len(&self) -> usize {
        self.cache.outline_command_cache.len() + self.cache.image_cache.len() + self.pictures.len()
    }

    #[cfg(test)]
    pub fn within_budget(&self) -> bool {
        self.cost <= BUDGET && self.len() <= LIMIT
    }
}

#[cfg(test)]
mod tests;
