//! Bounded, position-independent text outlines. Pan and translation reuse glyphs,
//! and zoom reuses them between quarter-octave steps.
mod glyphs;
use super::{Point, Rectangle, rgb};
use iced::{
    Color,
    widget::canvas::{self, Path},
};
use reshiki::typography::TextStyle;
use std::{collections::HashMap, rc::Rc};

pub(crate) fn prepare_fonts() {
    glyphs::prepare_fonts();
}

/// The zoom text is outlined at: the nearest power of 2^(1/4). Drawing scales
/// the rest (at most 2^(1/8), about 9%), so zooming reuses cached outlines.
pub(super) fn zoom_step(zoom: f32) -> f32 {
    if zoom.is_finite() && zoom > 0. {
        ((zoom.log2() * 4.).round() / 4.).exp2()
    } else {
        zoom
    }
}

const BUDGET: usize = 8 * 1024 * 1024;
const LIMIT: usize = 2048;

#[derive(Hash, PartialEq, Eq)]
struct Key {
    text: String,
    family: String,
    size: u32,
    zoom: u32,
    bold: bool,
    italic: bool,
    color: [u8; 3],
}

#[derive(Default)]
pub(super) struct TextCache {
    entries: HashMap<Key, Entry>,
    cost: usize,
    clock: u64,
    glyphs: glyphs::Glyphs,
}

struct Entry {
    text: Rc<OutlinedText>,
    cost: usize,
    used: u64,
}

impl TextCache {
    pub fn get(
        &mut self,
        text: &str,
        size: f32,
        zoom: f32,
        color: [u8; 3],
        style: &TextStyle,
    ) -> Rc<OutlinedText> {
        let key = Key {
            text: text.into(),
            family: style.family.clone(),
            size: size.to_bits(),
            zoom: zoom.to_bits(),
            bold: style.bold,
            italic: style.italic,
            color,
        };
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            return entry.text.clone();
        }
        let result = Rc::new(outline_with_glyphs(
            text,
            size,
            zoom,
            color,
            style,
            Some(&mut self.glyphs),
        ));
        // Bound both the entry count and the retained path data; large imported
        // captions may still render but cannot permanently fill this cache.
        let cost = result.cost + key.text.len() + key.family.len() + std::mem::size_of::<Key>();
        if cost <= BUDGET {
            if self.cost + cost > BUDGET || self.entries.len() >= LIMIT {
                self.evict(cost);
            }
            self.cost += cost;
            self.entries.insert(
                key,
                Entry {
                    text: result.clone(),
                    cost,
                    used: self.clock,
                },
            );
        }
        result
    }

    /// Drop the least recently used outlines until one more of `incoming`
    /// cost fits in three quarters of the limits; text on screen stays cached.
    fn evict(&mut self, incoming: usize) {
        let mut ages: Vec<_> = self.entries.values().map(|e| (e.used, e.cost)).collect();
        ages.sort_unstable();
        let mut len = self.entries.len() + 1;
        let mut total = self.cost + incoming;
        let mut keep_from = 0;
        for (used, cost) in ages {
            if total <= BUDGET * 3 / 4 && len <= LIMIT * 3 / 4 {
                break;
            }
            total -= cost;
            len -= 1;
            keep_from = used + 1;
        }
        self.entries.retain(|_, e| e.used >= keep_from);
        self.cost = total - incoming;
    }
}

pub(super) struct OutlinedText {
    pub paths: Vec<(Path, Color)>,
    pub bounds: Option<Rectangle>,
    cost: usize,
}
impl OutlinedText {
    fn new(paths: Vec<(Path, Color)>) -> Self {
        use iced::widget::canvas::path::lyon_path::Event;
        let mut low = Point::new(f32::INFINITY, f32::INFINITY);
        let mut high = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut cost = 0;
        for (path, _) in &paths {
            for event in path.raw().iter() {
                // An event bounds the storage of its verb and control points.
                cost += std::mem::size_of_val(&event);
                let points = match event {
                    Event::Begin { at } => [at; 4],
                    Event::Line { from, to } => [from, to, from, to],
                    Event::Quadratic { from, ctrl, to } => [from, ctrl, to, to],
                    Event::Cubic {
                        from,
                        ctrl1,
                        ctrl2,
                        to,
                    } => [from, ctrl1, ctrl2, to],
                    Event::End { last, first, .. } => [last, first, last, first],
                };
                for point in points {
                    low.x = low.x.min(point.x);
                    low.y = low.y.min(point.y);
                    high.x = high.x.max(point.x);
                    high.y = high.y.max(point.y);
                }
            }
        }
        let bounds = (cost > 0).then_some(Rectangle {
            x: low.x - 2.,
            y: low.y - 2.,
            width: high.x - low.x + 4.,
            height: high.y - low.y + 4.,
        });
        cost += paths.len() * std::mem::size_of::<(Path, Color)>();
        Self {
            paths,
            bounds,
            cost,
        }
    }
}

pub(super) fn outline(
    text: &str,
    size: f32,
    zoom: f32,
    color: [u8; 3],
    style: &TextStyle,
) -> OutlinedText {
    outline_with_glyphs(text, size, zoom, color, style, None)
}

fn outline_with_glyphs(
    text: &str,
    size: f32,
    zoom: f32,
    color: [u8; 3],
    style: &TextStyle,
    glyphs: Option<&mut glyphs::Glyphs>,
) -> OutlinedText {
    #[cfg(target_arch = "wasm32")]
    crate::web::text_drawn(text);
    let t = canvas::Text {
        content: text.to_owned(),
        position: Point::ORIGIN,
        size: (size * zoom).into(),
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
    // Iced centers its font metrics within line height, whereas
    // SVG's text-before-edge uses the font ascent. Align actual ink
    // to our shared font metrics so screen and export agree.
    let mut paths = Vec::new();
    if let Some(glyphs) = glyphs {
        glyphs.draw(&t, |path, color| paths.push((path, color)));
    } else {
        glyphs::Glyphs::default().draw(&t, |path, color| paths.push((path, color)));
    }
    let actual_top = paths
        .iter()
        .flat_map(|(path, _)| path.raw().iter())
        .map(|event| {
            use iced::widget::canvas::path::lyon_path::{Event, geom};
            match event {
                Event::Begin { at } => at.y,
                Event::Line { from, to } => from.y.min(to.y),
                Event::Quadratic { from, ctrl, to } => {
                    geom::QuadraticBezierSegment { from, ctrl, to }
                        .bounding_box()
                        .min
                        .y
                }
                Event::Cubic {
                    from,
                    ctrl1,
                    ctrl2,
                    to,
                } => {
                    geom::CubicBezierSegment {
                        from,
                        ctrl1,
                        ctrl2,
                        to,
                    }
                    .bounding_box()
                    .min
                    .y
                }
                Event::End { last, first, .. } => last.y.min(first.y),
            }
        })
        .reduce(f32::min);
    let mut metrics_style = style.clone();
    metrics_style.underline = false;
    let expected_top = reshiki::style::text_ink_boxes(&t.content, size, &metrics_style)
        .iter()
        .map(|(lo, _)| lo.y)
        .reduce(f32::min)
        .map(|top| top * zoom);
    let offset = actual_top
        .zip(expected_top)
        .map_or(0., |(actual, expected)| expected - actual);
    let transform = iced::widget::canvas::path::lyon_path::math::Transform::translation(0., offset);
    let paths: Vec<_> = paths
        .into_iter()
        .map(|(path, color)| (path.transform(&transform), color))
        .collect();
    OutlinedText::new(paths)
}

#[cfg(test)]
mod tests;
