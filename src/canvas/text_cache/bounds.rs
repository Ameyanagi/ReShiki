//! Cold text can be rejected using shaped positions and whole-font extents,
//! without creating a glyph path or decoding a bitmap. Be deliberately broad:
//! colour/variable fonts and complex scripts use the normal renderer instead.
use super::*;
use iced::advanced::{graphics::text, text::Paragraph as _};
use iced::{Size, Vector};

#[derive(Clone, Copy)]
struct FontBox {
    lo: Point,
    hi: Point,
    ascent: f32,
}

fn font_box(font: swash::FontRef<'_>) -> Option<FontBox> {
    if [*b"fvar", *b"sbix", *b"CBDT", *b"COLR", *b"SVG ", *b"EBDT"]
        .into_iter()
        .any(|tag| font.table(swash::Tag::from_be_bytes(tag)).is_some())
    {
        return None;
    }
    let head = font.table(swash::Tag::from_be_bytes(*b"head"))?;
    let u16_at = |data: &[u8], at| Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?));
    let i16_at = |data: &[u8], at| u16_at(data, at).map(|v| v as i16 as f32);
    let em = u16_at(head, 18)? as f32;
    if em == 0. {
        return None;
    }
    let hhea = font.table(swash::Tag::from_be_bytes(*b"hhea"))?;
    let mut ascent = i16_at(hhea, 4)?;
    if let Some(os2) = font.table(swash::Tag::from_be_bytes(*b"OS/2"))
        && (u16_at(os2, 62)? & (1 << 7) != 0 || ascent == 0.)
    {
        ascent = i16_at(os2, 68)?;
        if ascent == 0. {
            ascent = i16_at(os2, 74)?;
        }
    }
    let lo = Point::new(i16_at(head, 36)? / em, i16_at(head, 38)? / em);
    let hi = Point::new(i16_at(head, 40)? / em, i16_at(head, 42)? / em);
    (lo.x < hi.x && lo.y < hi.y).then_some(FontBox {
        lo,
        hi,
        ascent: ascent / em,
    })
}

// Font metadata does not depend on caption contents or zoom. Keep it bounded
// separately from individual text extents, and let TextCache's font generation
// invalidation discard it together with the exact outlines.
#[derive(Default)]
pub(super) struct Cache {
    iced: std::collections::HashMap<text::cosmic_text::fontdb::ID, Option<FontBox>>,
    styles: Vec<AsciiStyle>,
    #[cfg(test)]
    pub(super) svg_queries: usize,
    #[cfg(test)]
    pub(super) iced_queries: usize,
}

struct AsciiStyle {
    family: String,
    bold: bool,
    boxes: [Option<Option<FontBox>>; 128],
}

/// A cheap work-selection region, never a bound used to reject ink. Text near
/// the view should be outlined once instead of first shaping a broad bound and
/// then shaping/constructing the outlines again. Unusual bearings still take
/// the exact outline path or the conservative font-bound check below.
pub(in crate::canvas) fn preflight_region(text: &str, size: f32, zoom: f32) -> Rectangle {
    let em = (size * zoom).abs().max(1.);
    Rectangle {
        x: -4. * em - 4.,
        y: -4. * em - 4.,
        width: (2. * text.len() as f32 + 8.) * em + 8.,
        height: 12. * em + 8.,
    }
}

pub(in crate::canvas) fn conservative(
    text: &str,
    size: f32,
    zoom: f32,
    style: &TextStyle,
) -> Option<Rectangle> {
    Cache::default().conservative(text, size, zoom, style)
}

impl Cache {
    pub(super) fn conservative(
        &mut self,
        text: &str,
        size: f32,
        zoom: f32,
        style: &TextStyle,
    ) -> Option<Rectangle> {
        // Modern script/bitmap extents and synthetic slants are not guaranteed by
        // an SFNT head box. Their exact outline bounds still cull warm redraws.
        if text.is_empty()
            || !text.is_ascii()
            || text.bytes().any(|b| b.is_ascii_control())
            || style.italic
            || style.family.len() > 4096
        {
            return None;
        }
        glyphs::prepare_fonts();
        let t = canvas_text(text, size, zoom, [0, 0, 0], style);
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
        let (mut lo, mut hi) = (
            Point::new(f32::INFINITY, f32::INFINITY),
            Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        );
        let mut fonts = text::font_system()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for run in paragraph.buffer().layout_runs() {
            for glyph in run.glyphs {
                let key = glyph.physical((0., 0.), 1.).cache_key;
                let b = if let Some(cached) = self.iced.get(&key.font_id) {
                    *cached
                } else {
                    let font = fonts.raw().get_font(key.font_id, key.font_weight)?;
                    let found = font_box(font.as_swash());
                    if self.iced.len() >= 128 {
                        self.iced.clear();
                    }
                    self.iced.insert(key.font_id, found);
                    #[cfg(test)]
                    {
                        self.iced_queries += 1;
                    }
                    found
                }?;
                let offset = Vector::new(
                    glyph.x + glyph.font_size * glyph.x_offset,
                    run.line_y - glyph.font_size * glyph.y_offset,
                );
                lo.x = lo.x.min(offset.x + b.lo.x * glyph.font_size);
                hi.x = hi.x.max(offset.x + b.hi.x * glyph.font_size);
                lo.y = lo.y.min(offset.y - b.hi.y * glyph.font_size);
                hi.y = hi.y.max(offset.y - b.lo.y * glyph.font_size);
            }
        }
        drop(fonts);
        // outline_with_glyphs aligns its actual top to the shared SVG ink top.
        // Bound that top with each resolved face's complete box, not its advances.
        let db = reshiki::style::fontdb();
        let style_index = self
            .styles
            .iter()
            .position(|cached| cached.family == style.family && cached.bold == style.bold)
            .unwrap_or_else(|| {
                if self.styles.len() >= 16 {
                    self.styles.clear();
                }
                self.styles.push(AsciiStyle {
                    family: style.family.clone(),
                    bold: style.bold,
                    boxes: [None; 128],
                });
                self.styles.len() - 1
            });
        let cached = self.styles.get_mut(style_index)?;
        let mut top = f32::INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for c in text.bytes() {
            let slot = cached.boxes.get_mut(usize::from(c))?;
            let b = if let Some(found) = *slot {
                found
            } else {
                let found = reshiki::style::resolve_font(
                    &db,
                    &style.family,
                    resvg::usvg::fontdb::Weight(if style.bold { 700 } else { 400 }),
                    resvg::usvg::fontdb::Style::Normal,
                    char::from(c),
                )
                .and_then(|id| {
                    db.with_face_data(id, |data, index| {
                        font_box(swash::FontRef::from_index(data, index as usize)?)
                    })
                    .flatten()
                });
                *slot = Some(found);
                #[cfg(test)]
                {
                    self.svg_queries += 1;
                }
                found
            }?;
            top = top.min((b.ascent - b.hi.y) * size * zoom);
            bottom = bottom.max((b.ascent - b.lo.y) * size * zoom);
        }
        // Four pixels cover hinting and the outline cache's antialias margin.
        (lo.x.is_finite() && hi.y.is_finite() && top.is_finite() && bottom.is_finite()).then_some(
            Rectangle {
                x: lo.x - 4.,
                y: top - 4.,
                width: hi.x - lo.x + 8.,
                height: bottom - top + hi.y - lo.y + 8.,
            },
        )
    }
}
