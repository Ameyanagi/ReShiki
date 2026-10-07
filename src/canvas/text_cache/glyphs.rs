// Adapted from iced_graphics 0.14.0 Text::draw_with (MIT).
// Copyright 2019 Héctor Ramón, Iced contributors.
// See licenses/iced/NOTICE and licenses/iced/LICENSE.
//! Share hinted glyphs between captions without changing Iced's shaping or size.
use iced::advanced::{graphics::text, text::Paragraph as _};
use iced::widget::canvas::{Path, Text};
use iced::{Color, Point, Size, Vector};
use text::cosmic_text::{Command, SwashCache};

mod variable;

pub(super) fn prepare_fonts() {
    static PREPARED: std::sync::Once = std::sync::Once::new();
    PREPARED.call_once(|| {
        let mut fonts = text::font_system()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        variable::prepare_system_weights(fonts.raw());
    });
}

const BUDGET: usize = 2 * 1024 * 1024;
const LIMIT: usize = 4096;

pub(super) struct Glyphs {
    cache: SwashCache,
    variable_scaler: swash::scale::ScaleContext,
    cost: usize,
}

impl Default for Glyphs {
    fn default() -> Self {
        Self {
            cache: SwashCache::new(),
            variable_scaler: swash::scale::ScaleContext::new(),
            cost: 0,
        }
    }
}

impl Glyphs {
    /// Keep Iced's unwrapped paragraph placement and raster fallback while
    /// honoring variable weights in both family selection and cached outlines.
    pub fn draw(&mut self, t: &Text, mut draw: impl FnMut(Path, Color)) {
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
                let offset = Vector::new(glyph.x + glyph.x_offset, glyph.y_offset + run.line_y);
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
                // An oversize glyph can be drawn, but is never retained beyond
                // this call. Both negative lookups and raster data are bounded.
                if self.cost > BUDGET
                    || self.cache.outline_command_cache.len() + self.cache.image_cache.len() > LIMIT
                {
                    self.cache = SwashCache::new();
                    self.variable_scaler = swash::scale::ScaleContext::new();
                    self.cost = 0;
                }
            }
        }
    }

    #[cfg(test)]
    pub fn within_budget(&self) -> bool {
        self.cost <= BUDGET
            && self.cache.outline_command_cache.len() + self.cache.image_cache.len() <= LIMIT
    }
}

#[cfg(test)]
mod tests;
