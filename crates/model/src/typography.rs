//! Editable rich text, independent of the UI and chemistry engine.
use crate::{document::Point, style};
use serde::{Deserialize, Serialize};
use std::ops::Range;

mod name_markup;
pub use name_markup::{NameMarkup, NameRun, name_markup};

/// Conservative recognition for newly typed standalone neutral formulas.
/// Captions, units, reaction conditions and unknown symbols remain plain text.
pub fn is_formula(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.len() > 512 {
        return false;
    }
    let mut chars = text.chars().peekable();
    let mut groups: Vec<char> = vec![];
    let mut item = false;
    let mut number = false;
    let mut elements = 0;
    while let Some(c) = chars.next() {
        match c {
            'A'..='Z' => {
                let mut symbol = c.to_string();
                if chars.peek().is_some_and(char::is_ascii_lowercase)
                    && let Some(c) = chars.next()
                {
                    symbol.push(c);
                }
                if !crate::editing::ELEMENTS.contains(&symbol.as_str()) {
                    return false;
                }
                elements += 1;
                item = true;
            }
            '(' | '[' => {
                if groups.len() >= 16 {
                    return false;
                }
                groups.push(c);
                item = false;
            }
            ')' | ']' => {
                if !item || groups.pop() != Some(if c == ')' { '(' } else { '[' }) {
                    return false;
                }
                item = true;
            }
            '1'..='9' if item => {
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
                number = true;
            }
            _ => return false,
        }
    }
    item && groups.is_empty() && number && elements > 0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Script {
    #[default]
    Normal,
    Subscript,
    Superscript,
}

/// A manually selected script on unchanged UTF-8 label text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptRun {
    pub start: usize,
    pub end: usize,
    pub script: Script,
}

/// Validate stored manual ranges without changing their meaning.
pub fn validate_label_scripts(text: &str, runs: &[ScriptRun]) -> Result<(), String> {
    let mut end = 0;
    for run in runs {
        if run.start < end
            || run.start >= run.end
            || run.end > text.len()
            || !text.is_char_boundary(run.start)
            || !text.is_char_boundary(run.end)
        {
            return Err(
                "Atom label script ranges must be ordered, nonoverlapping UTF-8 boundaries".into(),
            );
        }
        end = run.end;
    }
    Ok(())
}

/// Canonical manual formatting: normal gaps are implicit, adjacent equal
/// scripts merge, and one full normal run records an explicitly plain label.
pub fn normalize_label_scripts(text: &str, runs: &[ScriptRun]) -> Result<Vec<ScriptRun>, String> {
    let mut nonempty = vec![];
    for run in runs {
        if run.start > run.end
            || run.end > text.len()
            || !text.is_char_boundary(run.start)
            || !text.is_char_boundary(run.end)
        {
            return Err(
                "Atom label script ranges must be ordered, nonoverlapping UTF-8 boundaries".into(),
            );
        }
        if run.start != run.end {
            nonempty.push(*run);
        }
    }
    validate_label_scripts(text, &nonempty)?;
    if nonempty.is_empty() {
        return Ok(vec![]);
    }
    if nonempty.iter().all(|run| run.script == Script::Normal) {
        return Ok(vec![ScriptRun {
            start: 0,
            end: text.len(),
            script: Script::Normal,
        }]);
    }
    let mut normalized: Vec<ScriptRun> = vec![];
    for run in nonempty
        .into_iter()
        .filter(|run| run.script != Script::Normal)
    {
        if let Some(last) = normalized.last_mut()
            && last.end == run.start
            && last.script == run.script
        {
            last.end = run.end;
        } else {
            normalized.push(run);
        }
    }
    Ok(normalized)
}

/// Resolve the displayed scripts once for rendering and exchange. Any manual
/// run overrides automatic formula inference; uncovered text stays normal.
pub fn effective_label_runs(
    display: &crate::atom_labels::AtomDisplay,
) -> Vec<(Range<usize>, Script)> {
    let Some(text) = display.variable.as_deref() else {
        return vec![];
    };
    if display.label_scripts.is_empty() {
        return label_runs(text);
    }
    let mut runs: Vec<(Range<usize>, Script)> = vec![];
    let mut manual = display.label_scripts.iter().peekable();
    for (start, c) in text.char_indices() {
        while manual.peek().is_some_and(|run| run.end <= start) {
            manual.next();
        }
        let script = manual
            .peek()
            .filter(|run| run.start <= start && start < run.end)
            .map_or(Script::Normal, |run| run.script);
        let end = start + c.len_utf8();
        if let Some((range, previous)) = runs.last_mut()
            && *previous == script
        {
            range.end = end;
        } else {
            runs.push((start..end, script));
        }
    }
    runs
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub family: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: crate::palette::Color,
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
            color: crate::palette::Color::Ink,
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
    /// Allow generated chemical-name captions to wrap after hyphens and commas.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub name_breaks: bool,
}
impl Default for TextFormat {
    fn default() -> Self {
        Self {
            style: TextStyle::default(),
            spans: vec![],
            alignment: TextAlign::Left,
            line_spacing: 1.2,
            width_pt: None,
            name_breaks: false,
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
    Color(crate::palette::Color),
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
        if old != new {
            self.name_breaks = false;
        }
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

/// Store Unicode subscript digits as editable ASCII with explicit script styles.
/// Resolve styles against the original UTF-8 offsets before rebuilding ranges
/// on the shorter text, preserving every other character's formatting.
pub fn normalize_subscript_digits(text: &mut String, format: &mut TextFormat) {
    if !text.chars().any(|c| matches!(c, '₀'..='₉')) {
        return;
    }
    let mut normalized = String::with_capacity(text.len());
    let mut styles = Vec::new();
    for (offset, c) in text.char_indices() {
        let mut style = format.at(offset).clone();
        if matches!(c, '₀'..='₉') {
            normalized.push(char::from(b'0' + (c as u32 - '₀' as u32) as u8));
            StyleChange::Script(Script::Subscript).apply(&mut style);
        } else {
            normalized.push(c);
        }
        styles.push(style);
    }
    *text = normalized;
    format.pack(text, styles);
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

/// Formula-style free labels, with a final single digit interpreted as charge
/// unless it directly follows a letter/bracket in a label with multiple tokens.
/// Runs cover the unchanged source text, including an optional charge space.
pub fn label_runs(text: &str) -> Vec<(Range<usize>, Script)> {
    let mut charge = text.len();
    let mut tail = text.char_indices().rev();
    if let Some((sign, '+' | '-' | '−' | '⁺' | '⁻')) = tail.next() {
        charge = sign;
        if let Some((digit, '1'..='9')) = tail.next() {
            let before = text.get(..digit).unwrap_or_default();
            let count = before
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphabetic() || matches!(c, ')' | ']'))
                && before.chars().filter(|c| c.is_uppercase()).count() > 1;
            if !count {
                charge = digit;
            }
        }
        if text.get(..charge).is_some_and(|s| s.ends_with(' ')) {
            charge -= 1;
        }
        if !text
            .get(..charge)
            .is_some_and(|s| s.chars().any(|c| !c.is_whitespace()))
        {
            charge = text.len();
        }
    }
    if text
        .get(..charge)
        .is_some_and(|s| s.chars().any(char::is_whitespace))
    {
        return vec![(0..text.len(), Script::Normal)];
    }
    let mut runs: Vec<(Range<usize>, Script)> = vec![];
    let mut previous = ' ';
    let mut subscript = false;
    for (start, c) in text.char_indices() {
        let script = if start >= charge {
            Script::Superscript
        } else if c.is_ascii_digit()
            && (previous.is_alphabetic() || matches!(previous, ')' | ']') || subscript)
        {
            Script::Subscript
        } else {
            Script::Normal
        };
        let end = start + c.len_utf8();
        if let Some((range, last)) = runs.last_mut()
            && *last == script
        {
            range.end = end;
        } else {
            runs.push((start..end, script));
        }
        previous = c;
        subscript = script == Script::Subscript;
    }
    runs
}

/// ChemDraw's Formula face for one character after `previous`: a digit after
/// a letter, a closing bracket or a lowered digit is lowered, and a sign after
/// a symbol is raised.
pub fn formula_script(previous: char, c: char, lowered: bool) -> Script {
    if c.is_ascii_digit() && (previous.is_alphabetic() || matches!(previous, ')' | ']') || lowered)
    {
        Script::Subscript
    } else if matches!(c, '+' | '−' | '-')
        && (previous.is_alphanumeric() || matches!(previous, ')' | ']'))
    {
        Script::Superscript
    } else {
        Script::Normal
    }
}

/// Whether a line may wrap between `before` and `after`: after whitespace, or
/// next to a CJK character, which needs no spaces between words, but never
/// before a kana voicing mark. `name_break` allows a chemical-name break.
fn breaks_between(before: char, after: char, name_break: bool) -> bool {
    !matches!(after, '\u{3099}'..='\u{309C}' | '\u{FF9E}' | '\u{FF9F}')
        && (before.is_whitespace() || cjk(before) || cjk(after) || name_break)
}

fn locant_character(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '′' | '″' | '‴')
}

fn cjk(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{9FFF}' | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}'
            | '\u{FF00}'..='\u{FFEF}' | '\u{20000}'..='\u{3FFFF}')
}

/// One layout feeds canvas, hit testing, selection, SVG, PDF and PNG.
pub fn layout(text: &str, format: &TextFormat) -> Layout {
    layout_with_metrics(text, format, style::glyph_metrics)
}

fn layout_with_metrics(
    text: &str,
    format: &TextFormat,
    glyph_metrics: impl Fn(char, &TextStyle) -> (&'static str, f32),
) -> Layout {
    let mut lines: Vec<Vec<(char, TextStyle, f32)>> = vec![vec![]];
    let max_width = format.width_pt.map(|w| style::DEFAULT.world(w));
    let mut width = 0.0;
    let mut formula_digit = false;
    let mut previous = ' ';
    let mut before_previous = ' ';
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    // Characters kept on the line at its last allowed break.
    let mut break_at = None;
    // Protected name punctuation is usable only if this line has no preferred
    // break and would otherwise overflow. Keep its position through a split.
    let mut name_fallback_at = None;
    for (index, c) in text.char_indices() {
        if c == '\n' {
            lines.push(vec![]);
            width = 0.0;
            break_at = None;
            name_fallback_at = None;
            previous = ' ';
            before_previous = ' ';
            formula_digit = false;
            continue;
        }
        let before = previous;
        let mut s = format.at(index).clone();
        if s.formula && s.script == Script::Normal {
            s.script = formula_script(previous, c, formula_digit);
            formula_digit = s.script == Script::Subscript;
        }
        previous = c;
        let (family, advance) = glyph_metrics(c, &s);
        s.family = family.into();
        let advance = advance * s.size();
        if let Some(line) = lines.last()
            && !line.is_empty()
        {
            let name_punctuation = format.name_breaks && matches!(before, '-' | ',');
            // Locant sets and bracketed groups stay whole; a closed group may
            // end a line before the next word.
            let preferred_name_break = parentheses == 0
                && brackets == 0
                && ((name_punctuation
                    && !(before == ','
                        && locant_character(before_previous)
                        && locant_character(c)))
                    || (format.name_breaks && matches!(before, ')' | ']') && c.is_alphabetic()));
            if breaks_between(before, c, preferred_name_break) {
                break_at = Some(line.len());
            }
            if name_punctuation && breaks_between(before, c, true) {
                name_fallback_at = Some(line.len());
            }
        }
        // Prefer word, CJK and unprotected name boundaries. An indivisible word
        // wider than the box still overflows instead of being cut mid-word.
        if max_width.is_some_and(|limit| width + advance > limit)
            && let Some(split) = break_at.take().or(name_fallback_at)
            && let Some(line) = lines.last_mut()
        {
            let overflow = line.split_off(split);
            width = overflow.iter().map(|(_, _, w)| w).sum();
            lines.push(overflow);
            name_fallback_at = name_fallback_at
                .and_then(|at| at.checked_sub(split))
                .filter(|at| *at > 0);
        }
        if let Some(line) = lines.last_mut() {
            line.push((c, s, advance));
        }
        width += advance;
        before_previous = before;
        if format.name_breaks {
            match c {
                '(' => parentheses += 1,
                ')' => parentheses = parentheses.saturating_sub(1),
                '[' => brackets += 1,
                ']' => brackets = brackets.saturating_sub(1),
                _ => {}
            }
        }
    }
    let natural_width = lines
        .iter()
        .map(|l| l.iter().map(|(_, _, w)| w).sum::<f32>())
        .fold(0.0, f32::max);
    let width = max_width.unwrap_or(natural_width).max(natural_width);
    let mut fragments: Vec<TextFragment> = vec![];
    // Keep the current run's advance. Re-measuring the growing string for every
    // appended character made long captions quadratic in their character count.
    let mut fragment_width = 0.;
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
                && (last.position.x + fragment_width - position.x).abs() < 0.01
            {
                last.text.push(c);
                fragment_width += advance;
            } else {
                fragments.push(TextFragment {
                    position,
                    text: c.to_string(),
                    style: s,
                });
                fragment_width = advance;
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod name_break_tests {
    use super::{Layout, TextFormat, layout_with_metrics};

    fn test_layout(text: &str, format: &TextFormat) -> Layout {
        // The built-in fallback metric keeps these checks independent of any
        // installed fonts, including the Unicode locant and CJK examples.
        layout_with_metrics(text, format, |_, _| ("Arial", 0.6))
    }

    fn width_pt(text: &str) -> f32 {
        test_layout(text, &TextFormat::default()).width * crate::style::DEFAULT.points_per_world()
    }

    fn lines(text: &str, width_pt: f32, name_breaks: bool) -> Vec<String> {
        let format = TextFormat {
            width_pt: Some(width_pt),
            name_breaks,
            ..Default::default()
        };
        let mut lines: Vec<(f32, String)> = vec![];
        for fragment in test_layout(text, &format).fragments {
            match lines.last_mut() {
                Some((y, line)) if (*y - fragment.position.y).abs() < 0.001 => {
                    line.push_str(&fragment.text);
                }
                _ => lines.push((fragment.position.y, fragment.text)),
            }
        }
        lines.into_iter().map(|(_, line)| line).collect()
    }

    #[test]
    fn name_breaks_keep_locant_commas_after_a_preferred_hyphen() {
        for locants in ["2,4", "N,N", "2',4'", "2′,4″", "2″,4‴", "N¹,N²", "N⁰,N⁹"] {
            let text = format!("hexa-{locants}-diene");
            let comma = text.find(',').unwrap() + 1;
            let next = text[comma..].chars().next().unwrap().to_string();
            // Overflow just after the comma, where the old rule split locants.
            let width = width_pt(&text[..comma]) + width_pt(&next) * 0.5;
            let wrapped = lines(&text, width, true);
            assert_eq!(wrapped[0], "hexa-", "{text}: {wrapped:?}");
            assert!(
                wrapped.iter().any(|line| line.contains(locants)),
                "{text}: {wrapped:?}"
            );
            assert_eq!(wrapped.concat(), text);
        }

        // A closed fusion descriptor may end a line before the parent name.
        let text = "furo[2,3-b]pyridine";
        let width = width_pt("furo[2,3-b]p") - width_pt("p") * 0.5;
        assert_eq!(lines(text, width, true), ["furo[2,3-b]", "pyridine"]);

        let text = "hexa-,(group)";
        let width = width_pt("hexa-,") + width_pt("(") * 0.5;
        assert_eq!(lines(text, width, true)[0], "hexa-,");
    }

    #[test]
    fn name_breaks_keep_stereo_and_fusion_groups_after_a_preferred_hyphen() {
        for (group, partial, next) in [
            ("(2R,3S)", "(2R,", "3"),
            ("(propan-2-yl)", "(propan-2-", "y"),
            ("[1,2-a]", "[1,2-", "a"),
            ("[(2R,3S)-1,2-a]", "[(2R,3S)-1,2-", "a"),
        ] {
            let text = format!("hexa-{group}-diene");
            let width = width_pt(&format!("hexa-{partial}")) + width_pt(next) * 0.5;
            assert!(width_pt(&format!("{group}-")) < width);
            let wrapped = lines(&text, width, true);
            assert_eq!(wrapped[0], "hexa-", "{text}: {wrapped:?}");
            assert!(
                wrapped.iter().any(|line| line.contains(group)),
                "{text}: {wrapped:?}"
            );
            assert_eq!(wrapped.concat(), text);
        }
    }

    #[test]
    fn name_breaks_use_protected_punctuation_when_a_segment_overflows() {
        let text = "(1,2,3,4,5,6,7,8,9,10)";
        let width = width_pt("(1,2,") + width_pt("3") * 0.5;
        let wrapped = lines(text, width, true);
        assert_eq!(wrapped[0], "(1,2,");
        assert!(wrapped.len() > 2, "{wrapped:?}");
        assert_eq!(wrapped.concat(), text);
        for line in &wrapped {
            assert!(width_pt(line) <= width + 0.001, "{line}");
        }

        // The fallback comma survives moving a suffix after the preferred '-'.
        let text = "hexa-(1,2mmmmmmmmmmmmmmmmmmmm)";
        let width = width_pt("hexa-(1,") + width_pt("2") * 0.5;
        let wrapped = lines(text, width, true);
        assert_eq!(&wrapped[..2], ["hexa-", "(1,"]);
        assert_eq!(wrapped.concat(), text);
        // The remaining long word keeps the ordinary overflow behavior.
        assert_eq!(wrapped.len(), 3);
    }

    #[test]
    fn name_breaks_leave_ordinary_text_wrapping_unchanged() {
        let text = "hexa-2,4-diene";
        let width = width_pt("hexa-2,") + width_pt("4") * 0.5;
        assert_eq!(lines(text, width, false), [text]);
        for text in [
            "alpha beta gamma delta",
            "日本語の長い説明文です",
            "ﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞ",
        ] {
            let plain = lines(text, 30., false);
            assert!(plain.len() > 1, "{text}: {plain:?}");
            assert_eq!(lines(text, 30., true), plain);
        }
    }
}
