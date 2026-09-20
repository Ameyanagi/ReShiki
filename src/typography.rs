//! Editable rich text, independent of the UI and chemistry engine.
use crate::{document::Point, style};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Script {
    #[default]
    Normal,
    Subscript,
    Superscript,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub family: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: [u8; 3],
    pub script: Script,
    pub formula: bool,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: "Arial".into(),
            size_pt: 10.0,
            bold: false,
            italic: false,
            underline: false,
            color: [0, 0, 0],
            script: Script::Normal,
            formula: false,
        }
    }
}
impl TextStyle {
    pub fn size(&self) -> f32 {
        style::DEFAULT.world(self.size_pt)
            * if self.script == Script::Normal {
                1.0
            } else {
                0.7
            }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.family.trim().is_empty()
            || self.family.len() > 256
            || !self.size_pt.is_finite()
            || !(4.0..=144.0).contains(&self.size_pt)
        {
            return Err("Text requires a font name and a size from 4 to 144 pt".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextSpan {
    /// UTF-8 byte offsets, always on character boundaries.
    pub start: usize,
    pub end: usize,
    pub style: TextStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextFormat {
    pub style: TextStyle,
    pub spans: Vec<TextSpan>,
    pub alignment: TextAlign,
    pub line_spacing: f32,
    pub width_pt: Option<f32>,
}
impl Default for TextFormat {
    fn default() -> Self {
        Self {
            style: TextStyle::default(),
            spans: vec![],
            alignment: TextAlign::Left,
            line_spacing: 1.2,
            width_pt: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum StyleChange {
    Family(String),
    Size(f32),
    Bold(bool),
    Italic(bool),
    Underline(bool),
    Color([u8; 3]),
    Script(Script),
    Formula(bool),
}
impl StyleChange {
    pub fn apply(&self, style: &mut TextStyle) {
        match self {
            Self::Family(s) => style.family = s.clone(),
            Self::Size(s) => style.size_pt = *s,
            Self::Bold(v) => style.bold = *v,
            Self::Italic(v) => style.italic = *v,
            Self::Underline(v) => style.underline = *v,
            Self::Color(c) => style.color = *c,
            Self::Script(s) => {
                style.script = *s;
                if *s != Script::Normal {
                    style.formula = false;
                }
            }
            Self::Formula(v) => {
                style.formula = *v;
                if *v {
                    style.script = Script::Normal;
                }
            }
        }
    }
}

impl TextFormat {
    pub fn at(&self, offset: usize) -> &TextStyle {
        self.spans
            .iter()
            .find(|s| s.start <= offset && offset < s.end)
            .map(|s| &s.style)
            .unwrap_or(&self.style)
    }
    pub fn validate(&self, text: &str) -> Result<(), String> {
        self.style.validate()?;
        if !self.line_spacing.is_finite()
            || !(0.8..=3.0).contains(&self.line_spacing)
            || self
                .width_pt
                .is_some_and(|w| !w.is_finite() || !(10.0..=2000.0).contains(&w))
        {
            return Err("Invalid text spacing or width".into());
        }
        let mut end = 0;
        for span in &self.spans {
            if span.start < end
                || span.start >= span.end
                || span.end > text.len()
                || !text.is_char_boundary(span.start)
                || !text.is_char_boundary(span.end)
            {
                return Err(
                    "Text style ranges must be ordered, nonoverlapping UTF-8 boundaries".into(),
                );
            }
            span.style.validate()?;
            end = span.end;
        }
        Ok(())
    }
    fn pack(&mut self, text: &str, styles: Vec<TextStyle>) {
        self.spans.clear();
        for ((start, c), style) in text.char_indices().zip(styles) {
            if style == self.style {
                continue;
            }
            let end = start + c.len_utf8();
            if let Some(last) = self.spans.last_mut()
                && last.end == start
                && last.style == style
            {
                last.end = end;
            } else {
                self.spans.push(TextSpan { start, end, style });
            }
        }
    }
    pub fn apply(&mut self, text: &str, range: Option<Range<usize>>, change: &StyleChange) {
        let mut styles: Vec<_> = text
            .char_indices()
            .map(|(i, _)| self.at(i).clone())
            .collect();
        for ((i, _), style) in text.char_indices().zip(&mut styles) {
            if range.as_ref().is_none_or(|r| r.contains(&i)) {
                change.apply(style);
            }
        }
        if range.is_none() {
            change.apply(&mut self.style);
        }
        self.pack(text, styles);
    }
    /// Preserve spans across an editor replacement. The original range resolves
    /// ambiguous diffs such as replacing or deleting one of several equal letters.
    pub fn edited(&mut self, old: &str, new: &str, replaced: Range<usize>) {
        let exact = old
            .get(..replaced.start)
            .is_some_and(|prefix| new.starts_with(prefix))
            && old.get(replaced.end..).is_some_and(|suffix| {
                new.ends_with(suffix)
                    && replaced
                        .start
                        .checked_add(suffix.len())
                        .is_some_and(|end| end <= new.len())
            });
        let mut prefix = if exact {
            replaced.start
        } else {
            old.chars()
                .zip(new.chars())
                .take_while(|(a, b)| a == b)
                .map(|(c, _)| c.len_utf8())
                .sum::<usize>()
        };
        while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let suffix = if exact {
            old.len() - replaced.end
        } else {
            old.get(prefix..)
                .unwrap_or_default()
                .chars()
                .rev()
                .zip(new.get(prefix..).unwrap_or_default().chars().rev())
                .take_while(|(a, b)| a == b)
                .map(|(c, _)| c.len_utf8())
                .sum::<usize>()
        };
        let inherited = self.at(prefix.min(old.len().saturating_sub(1))).clone();
        let styles = new
            .char_indices()
            .map(|(i, _)| {
                if i < prefix {
                    self.at(i).clone()
                } else if i >= new.len() - suffix {
                    self.at(old.len() - (new.len() - i)).clone()
                } else {
                    inherited.clone()
                }
            })
            .collect();
        self.pack(new, styles);
    }
}

#[derive(Debug, Clone)]
pub struct TextFragment {
    pub position: Point,
    pub text: String,
    pub style: TextStyle,
}
pub struct Layout {
    pub fragments: Vec<TextFragment>,
    pub width: f32,
    pub height: f32,
}

/// One layout feeds canvas, hit testing, selection, SVG, PDF and PNG.
pub fn layout(text: &str, format: &TextFormat) -> Layout {
    let mut lines: Vec<Vec<(char, TextStyle, f32)>> = vec![vec![]];
    let max_width = format.width_pt.map(|w| style::DEFAULT.world(w));
    let mut width = 0.0;
    let mut formula_digit = false;
    let mut previous = ' ';
    for (index, c) in text.char_indices() {
        if c == '\n' {
            lines.push(vec![]);
            width = 0.0;
            previous = ' ';
            formula_digit = false;
            continue;
        }
        let mut s = format.at(index).clone();
        if s.formula && s.script == Script::Normal {
            if c.is_ascii_digit()
                && (previous.is_alphabetic() || matches!(previous, ')' | ']') || formula_digit)
            {
                s.script = Script::Subscript;
                formula_digit = true;
            } else {
                formula_digit = false;
                if matches!(c, '+' | '−' | '-')
                    && (previous.is_alphanumeric() || matches!(previous, ')' | ']'))
                {
                    s.script = Script::Superscript;
                }
            }
        }
        previous = c;
        let (family, advance) = style::glyph_metrics(c, &s);
        s.family = family.into();
        let advance = advance * s.size();
        if max_width.is_some_and(|limit| width + advance > limit)
            && lines.last().is_some_and(|line| !line.is_empty())
            && let Some(line) = lines.last_mut()
        {
            let overflow = line
                .iter()
                .rposition(|(c, _, _)| c.is_whitespace())
                .map(|split| line.split_off(split + 1))
                .unwrap_or_default();
            width = overflow.iter().map(|(_, _, w)| w).sum();
            lines.push(overflow);
        }
        if let Some(line) = lines.last_mut() {
            line.push((c, s, advance));
        }
        width += advance;
    }
    let natural_width = lines
        .iter()
        .map(|l| l.iter().map(|(_, _, w)| w).sum::<f32>())
        .fold(0.0, f32::max);
    let width = max_width.unwrap_or(natural_width).max(natural_width);
    let mut fragments: Vec<TextFragment> = vec![];
    let mut y = 0.0;
    let line_count = lines.len();
    for (line_index, line) in lines.into_iter().enumerate() {
        let line_width: f32 = line.iter().map(|(_, _, w)| w).sum();
        let size = line
            .iter()
            .map(|(_, s, _)| style::DEFAULT.world(s.size_pt))
            .fold(style::DEFAULT.world(format.style.size_pt), f32::max);
        let script_margin = if line.iter().any(|(_, s, _)| s.script == Script::Superscript) {
            size * 0.3
        } else {
            0.0
        };
        let mut x = match format.alignment {
            TextAlign::Center => (width - line_width) * 0.5,
            TextAlign::Right => width - line_width,
            _ => 0.0,
        };
        let spaces = line.iter().filter(|(c, _, _)| *c == ' ').count();
        let gap = if format.alignment == TextAlign::Justified
            && line_index + 1 < line_count
            && spaces > 0
        {
            (width - line_width) / spaces as f32
        } else {
            0.0
        };
        for (c, s, advance) in line {
            let shift = match s.script {
                Script::Normal => 0.0,
                Script::Subscript => size * 0.35,
                Script::Superscript => -size * 0.3,
            };
            let position = Point::new(x, y + script_margin + shift);
            // Character positions are explicit so font metrics and selection agree.
            if let Some(last) = fragments.last_mut()
                && last.style == s
                && (last.position.y - position.y).abs() < 0.001
                && (last.position.x
                    + style::styled_text_width(&last.text, last.style.size(), &last.style)
                    - position.x)
                    .abs()
                    < 0.01
            {
                last.text.push(c);
            } else {
                fragments.push(TextFragment {
                    position,
                    text: c.to_string(),
                    style: s,
                });
            }
            x += advance + if c == ' ' { gap } else { 0.0 };
        }
        y += size * format.line_spacing + script_margin;
    }
    Layout {
        fragments,
        width,
        height: y,
    }
}
