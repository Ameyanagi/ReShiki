//! Bounded, position-independent text outlines. Pan and translation reuse glyphs.
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
    entries: HashMap<Key, Rc<OutlinedText>>,
    cost: usize,
    glyphs: glyphs::Glyphs,
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
        if let Some(paths) = self.entries.get(&key) {
            return paths.clone();
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
        const BUDGET: usize = 8 * 1024 * 1024;
        let cost = result.cost + key.text.len() + key.family.len() + std::mem::size_of::<Key>();
        if cost <= BUDGET {
            if self.cost + cost > BUDGET || self.entries.len() >= 2048 {
                self.entries.clear();
                self.cost = 0;
            }
            self.cost += cost;
            self.entries.insert(key, result.clone());
        }
        result
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
mod tests {
    use super::*;

    #[test]
    fn text_cache_keys_cover_shaping_color_and_zoom() {
        let mut cache = TextCache::default();
        let style = TextStyle::default();
        let first = cache.get("NH2 α", 14., 1., [0, 0, 0], &style);
        assert!(Rc::ptr_eq(
            &first,
            &cache.get("NH2 α", 14., 1., [0, 0, 0], &style)
        ));
        for (text, size, zoom, color, changed) in [
            ("NH3 α", 14., 1., [0, 0, 0], style.clone()),
            ("NH2 α", 16., 1., [0, 0, 0], style.clone()),
            ("NH2 α", 14., 2., [0, 0, 0], style.clone()),
            ("NH2 α", 14., 1., [128, 0, 0], style.clone()),
            (
                "NH2 α",
                14.,
                1.,
                [0, 0, 0],
                TextStyle {
                    bold: true,
                    ..style.clone()
                },
            ),
            (
                "NH2 α",
                14.,
                1.,
                [0, 0, 0],
                TextStyle {
                    italic: true,
                    ..style.clone()
                },
            ),
            (
                "NH2 α",
                14.,
                1.,
                [0, 0, 0],
                TextStyle {
                    family: "Times New Roman".into(),
                    ..style.clone()
                },
            ),
        ] {
            let cached = cache.get(text, size, zoom, color, &changed);
            assert!(!Rc::ptr_eq(&first, &cached));
            let fresh = outline(text, size, zoom, color, &changed);
            assert_eq!(cached.bounds, fresh.bounds);
            assert_eq!(cached.paths.len(), fresh.paths.len());
            for ((a, ac), (b, bc)) in cached.paths.iter().zip(&fresh.paths) {
                assert_eq!(ac, bc);
                assert_eq!(
                    a.raw().iter().collect::<Vec<_>>(),
                    b.raw().iter().collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn shared_glyphs_exactly_match_fresh_outlines_at_each_zoom_and_font() {
        let mut cache = TextCache::default();
        for family in ["Arial", "Times New Roman", "Courier New"] {
            for zoom in [0.08, 0.45, 0.455, 0.5, 0.7, 1., 1.25, 2.5, 8.] {
                for (bold, italic) in [(false, false), (true, false), (false, true)] {
                    let style = TextStyle {
                        family: family.into(),
                        bold,
                        italic,
                        ..Default::default()
                    };
                    for text in [
                        "NH2 OH α β →",
                        "AV office fi",
                        "日本語 العربية 🧪",
                        "First\nSecond",
                    ] {
                        let cached = cache.get(text, 14., zoom, [25, 90, 140], &style);
                        let fresh = outline(text, 14., zoom, [25, 90, 140], &style);
                        assert_eq!(cached.bounds, fresh.bounds, "{family} {zoom} {text}");
                        assert_eq!(cached.paths.len(), fresh.paths.len());
                        for ((a, ac), (b, bc)) in cached.paths.iter().zip(&fresh.paths) {
                            assert_eq!(ac, bc);
                            assert_eq!(
                                a.raw().iter().collect::<Vec<_>>(),
                                b.raw().iter().collect::<Vec<_>>(),
                                "{family} {zoom} {text}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn changing_text_does_not_grow_the_cache_without_limit() {
        let mut cache = TextCache::default();
        for i in 0..2100 {
            cache.get(&i.to_string(), 14., 1., [0, 0, 0], &TextStyle::default());
            assert!(cache.entries.len() <= 2048);
            assert!(cache.cost <= 8 * 1024 * 1024);
            assert!(cache.glyphs.within_budget());
        }
    }
}
