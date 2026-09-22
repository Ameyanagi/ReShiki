use super::{
    Attributes, BYTE_LIMIT, Error, Fonts, NativeColor, PROPERTY_LIMIT, Palette, Result, bounded,
    named, numeric,
};
use crate::typography::{self, Script, TextAlign};
use roxmltree::Node;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeTextStyle {
    pub family: String,
    pub size_pt: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: NativeColor,
    pub script: Script,
    pub formula: bool,
}
impl NativeTextStyle {
    fn fields(self) -> Result<typography::TextStyle> {
        Ok(typography::TextStyle {
            family: self.family,
            size_pt: self.size_pt as f32,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            color: self.color.into_document()?,
            script: self.script,
            formula: self.formula,
        })
    }
    /// Convert a standalone style after the caller has resolved atom-label
    /// formula/script handling and any mixed-style restrictions.
    pub fn into_document(self) -> Result<typography::TextStyle> {
        let style = self.fields()?;
        style.validate().map_err(Error::Document)?;
        Ok(style)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeSpan {
    pub start: usize,
    pub end: usize,
    pub style: NativeTextStyle,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeFormat {
    pub style: NativeTextStyle,
    pub spans: Vec<NativeSpan>,
    pub alignment: TextAlign,
    pub line_spacing: f64,
    pub width_pt: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeText {
    pub text: String,
    pub format: NativeFormat,
}
impl NativeText {
    /// Preserve raw helper acceptance separately from final document validation.
    /// In particular, cross-run CR/LF normalization may produce native spans
    /// that the editable document subsequently rejects; do not repair them here.
    pub fn into_document(self) -> Result<(String, typography::TextFormat)> {
        let format = typography::TextFormat {
            style: self.format.style.fields()?,
            spans: self
                .format
                .spans
                .into_iter()
                .map(|span| {
                    Ok(typography::TextSpan {
                        start: span.start,
                        end: span.end,
                        style: span.style.fields()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            alignment: self.format.alignment,
            line_spacing: self.format.line_spacing as f32,
            width_pt: self.format.width_pt.map(|width| width as f32),
        };
        format.validate(&self.text).map_err(Error::Document)?;
        Ok((self.text, format))
    }
}

/// Cached root defaults, font lookup and raw palette. Synthetic label elements
/// from separate documents are supported, matching the original importer.
pub struct TextReader<'a, 'input> {
    root: Node<'a, 'input>,
    fonts: Fonts,
    palette: Palette,
}
impl<'a, 'input> TextReader<'a, 'input> {
    pub fn new(root: Node<'a, 'input>) -> Result<Self> {
        bounded(root.document())?;
        Ok(Self {
            root,
            fonts: Fonts::from_root(root),
            palette: Palette::from_root(root)?,
        })
    }
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn read(
        &self,
        element: Node<'_, '_>,
        defaults: Option<&Attributes>,
        atom: bool,
    ) -> Result<NativeText> {
        if !std::ptr::eq(self.root.document(), element.document()) {
            bounded(element.document())?;
        }
        if let Some(defaults) = defaults {
            let bytes = defaults
                .iter()
                .try_fold(0usize, |sum, (key, value)| {
                    sum.checked_add(key.len())
                        .and_then(|n| n.checked_add(value.len()))
                })
                .ok_or(Error::Limit)?;
            if defaults.len() > PROPERTY_LIMIT || bytes > BYTE_LIMIT {
                return Err(Error::Limit);
            }
        }
        let inherited = |key: &str| {
            element
                .attribute(key)
                .or_else(|| defaults.and_then(|attrs| attrs.get(key).map(String::as_str)))
                .or_else(|| self.root.attribute(key))
        };
        let prefix = if atom { "Label" } else { "Caption" };
        let style = |run: Node<'_, '_>| -> Result<NativeTextStyle> {
            let face = numeric::integer(
                run.attribute("face")
                    .or_else(|| inherited(&format!("{prefix}Face")))
                    .unwrap_or("0"),
            )?;
            let face = face
                .filter(|face| face & !(1 | 2 | 4 | 32 | 64) == 0)
                .ok_or(Error::Face)?;
            let script = face & 96;
            let color = numeric::integer(
                run.attribute("color")
                    .or_else(|| inherited(&format!("{prefix}Color")))
                    .unwrap_or("3"),
            )?;
            let size_pt = numeric::float(
                run.attribute("size")
                    .or_else(|| inherited(&format!("{prefix}Size")))
                    .unwrap_or("10"),
            )?;
            let color = color
                .and_then(|color| usize::try_from(color).ok())
                .and_then(|color| self.palette.colors.get(color))
                .copied();
            if color.is_none() || !(4.0..=144.0).contains(&size_pt) {
                return Err(Error::TextColorSize);
            }
            Ok(NativeTextStyle {
                family: self.fonts.get(
                    run.attribute("font")
                        .or_else(|| inherited(&format!("{prefix}Font"))),
                ),
                size_pt,
                bold: face & 1 != 0,
                italic: face & 2 != 0,
                underline: face & 4 != 0,
                color: color.ok_or(Error::TextColorSize)?,
                script: match script {
                    32 => Script::Subscript,
                    64 => Script::Superscript,
                    _ => Script::Normal,
                },
                formula: script == 96,
            })
        };
        let mut runs = Vec::new();
        for run in element.children().filter(|n| named(*n, "s")) {
            // ElementTree ignores comments/PIs and uses only text before the
            // first child element, excluding nested text and subsequent tails.
            let mut value = String::new();
            for child in run.children() {
                if child.is_element() {
                    break;
                }
                if child.is_text() {
                    value.push_str(child.text().unwrap_or(""));
                }
            }
            runs.push((value, style(run)?));
        }
        let base = runs
            .first()
            .map(|(_, style)| style.clone())
            .ok_or(Error::Runs)?;
        let text = normalize(
            &runs
                .iter()
                .map(|(value, _)| value.as_str())
                .collect::<String>(),
        );
        let (mut spans, mut offset) = (Vec::new(), 0usize);
        for (value, style) in runs {
            let end = offset
                .checked_add(normalize(&value).len())
                .ok_or(Error::Limit)?;
            if style != base && end > offset {
                spans.push(NativeSpan {
                    start: offset,
                    end,
                    style,
                });
            }
            offset = end;
        }
        let alignment = inherited(&format!("{prefix}Justification"))
            .or_else(|| inherited("Justification"))
            .unwrap_or("Left");
        if !atom && !["Left", "Center", "Right", "Full"].contains(&alignment) {
            return Err(Error::Alignment);
        }
        let height = inherited(&format!("{prefix}LineHeight"))
            .or_else(|| inherited("LineHeight"))
            .unwrap_or("auto");
        let line_spacing = if ["auto", "variable", "0", "1"].contains(&height) {
            1.2
        } else {
            numeric::float(height)? / base.size_pt
        };
        let width = numeric::float(inherited("WordWrapWidth").unwrap_or("0"))?;
        if !(0.8..=3.0).contains(&line_spacing) || width != 0.0 && !(10.0..=2000.0).contains(&width)
        {
            return Err(Error::Paragraph);
        }
        if numeric::float(inherited("RotationAngle").unwrap_or("0"))? != 0.0 {
            return Err(Error::Rotation);
        }
        Ok(NativeText {
            text,
            format: NativeFormat {
                style: base,
                spans,
                alignment: match alignment {
                    "Center" => TextAlign::Center,
                    "Right" => TextAlign::Right,
                    "Full" => TextAlign::Justified,
                    _ => TextAlign::Left,
                },
                line_spacing,
                width_pt: (width != 0.0).then_some(width),
            },
        })
    }
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
