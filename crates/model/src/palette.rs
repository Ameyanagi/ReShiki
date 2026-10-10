//! Theme palettes: eight named hues plus Ink, in a Strong row (bonds, text,
//! strokes) and a Tint row (fills, ring interiors, highlight boxes). Each row has
//! one OKLCH lightness and target chroma per theme and canvas.
use crate::{
    canvas_theme::{CanvasTheme, ColorTheme},
    color_contrast::{Oklch, Rgb},
    document::Document,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hue {
    Red,
    Orange,
    Amber,
    Green,
    Teal,
    Blue,
    Indigo,
    Purple,
}
impl Hue {
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Amber,
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Indigo,
        Self::Purple,
    ];
    pub fn default_degrees(self) -> u16 {
        slot([25, 55, 85, 145, 185, 255, 285, 320], self)
    }
    pub fn name(self) -> &'static str {
        slot(
            [
                "Red", "Orange", "Amber", "Green", "Teal", "Blue", "Indigo", "Purple",
            ],
            self,
        )
    }
    fn key(self) -> &'static str {
        slot(
            [
                "red", "orange", "amber", "green", "teal", "blue", "indigo", "purple",
            ],
            self,
        )
    }
}
fn slot<T>(values: [T; 8], hue: Hue) -> T {
    let [red, orange, amber, green, teal, blue, indigo, purple] = values;
    match hue {
        Hue::Red => red,
        Hue::Orange => orange,
        Hue::Amber => amber,
        Hue::Green => green,
        Hue::Teal => teal,
        Hue::Blue => blue,
        Hue::Indigo => indigo,
        Hue::Purple => purple,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Row {
    Strong,
    Tint,
}
impl Row {
    pub fn name(self) -> &'static str {
        match self {
            Self::Strong => "Strong",
            Self::Tint => "Tint",
        }
    }
}

/// A document color. Palette colors follow the theme, canvas and hues; custom
/// colors are exact on both canvases. Stored as "blue.strong" or "blue.tint",
/// as `[r, g, b]` (read with the `legacy` table, so Ink is black), or as
/// "#1F4E79" for a custom color that the table would turn into a swatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Color {
    #[default]
    Ink,
    Palette(Hue, Row),
    Custom(Rgb),
}
impl Color {
    /// The five hard-coded swatches and four fill swatches of earlier versions.
    pub fn legacy(rgb: Rgb) -> Self {
        use {Hue::*, Row::*};
        match rgb {
            [0, 0, 0] => Self::Ink,
            [32, 80, 145] => Self::Palette(Blue, Strong),
            [17, 126, 108] => Self::Palette(Teal, Strong),
            [180, 50, 55] => Self::Palette(Red, Strong),
            [116, 65, 147] => Self::Palette(Purple, Strong),
            [220, 239, 233] => Self::Palette(Teal, Tint),
            [221, 232, 248] => Self::Palette(Blue, Tint),
            [253, 239, 203] => Self::Palette(Amber, Tint),
            [249, 223, 225] => Self::Palette(Red, Tint),
            rgb => Self::Custom(rgb),
        }
    }
    /// Colors read from ChemDraw or another program. Black is ChemDraw's default
    /// foreground, so it stays visible on the dark canvas as Ink.
    pub fn imported(rgb: Rgb) -> Self {
        if rgb == [0; 3] {
            Self::Ink
        } else {
            Self::Custom(rgb)
        }
    }
    /// Stored bytes: black for Ink and the exact value of a custom color, which
    /// scene primitives hold in canonical light-canvas form. Palette colors need a
    /// `Palette`; here they fall back to Publication on the light canvas.
    pub fn rgb(self) -> Rgb {
        match self {
            Self::Ink => [0; 3],
            Self::Custom(rgb) => rgb,
            Self::Palette(hue, row) => Palette::new(
                ColorTheme::Publication.tones(CanvasTheme::Light),
                Hues::default(),
                CanvasTheme::Light,
            )
            .swatch(hue, row),
        }
    }
    pub fn name(self) -> String {
        match self {
            Self::Ink => "Ink".into(),
            Self::Palette(hue, row) => format!("{} · {}", hue.name(), row.name()),
            Self::Custom(rgb) => hex(rgb),
        }
    }
}
impl std::str::FromStr for Color {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if let Some(hex) = s.strip_prefix('#') {
            return parse_hex(hex)
                .map(Self::Custom)
                .ok_or_else(|| format!("Invalid color {s}"));
        }
        let (hue, row) = s
            .split_once('.')
            .ok_or_else(|| format!("Unknown color {s}"))?;
        let hue = Hue::ALL
            .into_iter()
            .find(|h| h.key() == hue)
            .ok_or_else(|| format!("Unknown hue {hue}"))?;
        let row = match row {
            "strong" => Row::Strong,
            "tint" => Row::Tint,
            _ => return Err(format!("Unknown palette row {row}")),
        };
        Ok(Self::Palette(hue, row))
    }
}
impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Self::Palette(hue, row) => {
                serializer.serialize_str(&format!("{}.{}", hue.key(), row.name().to_lowercase()))
            }
            // Ink and custom colors that cannot be taken for an old swatch keep
            // the byte form, so earlier versions still open such drawings.
            color if Self::legacy(color.rgb()) == color => color.rgb().serialize(serializer),
            color => serializer.serialize_str(&hex(color.rgb())),
        }
    }
}
/// Stored form: a current string or a legacy byte array.
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum StoredColor {
    Legacy(Rgb),
    Current(String),
}
impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match StoredColor::deserialize(deserializer)? {
            StoredColor::Legacy(rgb) => Ok(Self::legacy(rgb)),
            StoredColor::Current(s) => s.parse().map_err(serde::de::Error::custom),
        }
    }
}

pub fn hex([r, g, b]: Rgb) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}
fn parse_hex(hex: &str) -> Option<Rgb> {
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Hue angles in degrees, one per slot. Slots keep their names when moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "BTreeMap<Hue, u16>", into = "BTreeMap<Hue, u16>")]
pub struct Hues(pub [u16; 8]);
impl Default for Hues {
    fn default() -> Self {
        Self(Hue::ALL.map(Hue::default_degrees))
    }
}
impl Hues {
    /// The document theme's hues.
    pub fn of(doc: &Document) -> Self {
        doc.custom_theme
            .as_ref()
            .map_or_else(Self::default, |t| t.hues)
    }
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn get(&self, hue: Hue) -> u16 {
        slot(self.0, hue)
    }
    pub fn set(&mut self, hue: Hue, degrees: u16) {
        *slot(self.0.each_mut(), hue) = degrees % 360;
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.0.iter().any(|&h| h >= 360) {
            return Err("Palette hues must be 0–359 degrees".into());
        }
        Ok(())
    }
}
impl From<BTreeMap<Hue, u16>> for Hues {
    fn from(map: BTreeMap<Hue, u16>) -> Self {
        let mut hues = Self::default();
        for (hue, degrees) in map {
            *slot(hues.0.each_mut(), hue) = degrees;
        }
        hues
    }
}
impl From<Hues> for BTreeMap<Hue, u16> {
    fn from(hues: Hues) -> Self {
        Hue::ALL.into_iter().map(|h| (h, hues.get(h))).collect()
    }
}

/// OKLCH lightness and target chroma of each row on one canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tones {
    pub strong: [f64; 2],
    pub tint: [f64; 2],
}
impl Tones {
    /// The document's theme (built-in or embedded) on a canvas.
    pub fn of(doc: &Document, canvas: CanvasTheme) -> Self {
        doc.custom_theme.as_ref().map_or_else(
            || doc.color_theme.tones(canvas),
            |theme| theme.tones(canvas),
        )
    }
    /// A row's color at any hue angle; chroma drops at fixed L and h to fit sRGB.
    pub fn rgb(&self, row: Row, degrees: u16) -> Rgb {
        let [l, c] = match row {
            Row::Strong => self.strong,
            Row::Tint => self.tint,
        };
        Oklch {
            l,
            c,
            h: f64::from(degrees).to_radians(),
        }
        .to_rgb()
    }
}
impl ColorTheme {
    pub fn tones(self, canvas: CanvasTheme) -> Tones {
        let (strong, tint) = match (self, canvas.is_dark()) {
            (Self::Publication, false) => ([0.50, 0.13], [0.93, 0.040]),
            (Self::Publication, true) => ([0.70, 0.12], [0.49, 0.065]),
            (Self::Presentation, false) => ([0.50, 0.17], [0.90, 0.060]),
            (Self::Presentation, true) => ([0.70, 0.15], [0.45, 0.080]),
            (Self::Pastel, false) => ([0.55, 0.07], [0.95, 0.030]),
            // Tint L 0.53, not 0.55, keeps white ink and bonds at 5:1 on every hue.
            (Self::Pastel, true) => ([0.83, 0.06], [0.53, 0.040]),
            (Self::Jmol, false) => ([0.50, 0.14], [0.92, 0.045]),
            (Self::Jmol, true) => ([0.70, 0.13], [0.49, 0.065]),
        };
        Tones { strong, tint }
    }
}

/// Resolves document colors for one canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub canvas: CanvasTheme,
    strong: [Rgb; 8],
    tint: [Rgb; 8],
}
impl Palette {
    pub fn new(tones: Tones, hues: Hues, canvas: CanvasTheme) -> Self {
        let row = |row| Hue::ALL.map(|hue| tones.rgb(row, hues.get(hue)));
        Self {
            canvas,
            strong: row(Row::Strong),
            tint: row(Row::Tint),
        }
    }
    /// The document's theme (built-in or embedded) on its own canvas.
    pub fn of(doc: &Document) -> Self {
        Self::in_mode(doc, doc.canvas_theme)
    }
    pub fn in_mode(doc: &Document, canvas: CanvasTheme) -> Self {
        Self::new(Tones::of(doc, canvas), Hues::of(doc), canvas)
    }
    pub fn swatch(&self, hue: Hue, row: Row) -> Rgb {
        slot(
            match row {
                Row::Strong => self.strong,
                Row::Tint => self.tint,
            },
            hue,
        )
    }
    /// Display sRGB on this palette's canvas.
    pub fn rgb(&self, color: Color) -> Rgb {
        match color {
            Color::Ink => self.canvas.color([0; 3]),
            Color::Palette(hue, row) => self.swatch(hue, row),
            Color::Custom(rgb) => rgb,
        }
    }
    /// Stored light-canvas bytes whose single canvas conversion shows `rgb(color)`.
    pub fn canonical(&self, color: Color) -> Rgb {
        self.canvas.color(self.rgb(color))
    }
    /// The palette color (Ink or a swatch) nearest to `rgb`, with its OKLab
    /// distance ×100.
    pub fn closest(&self, rgb: Rgb) -> (Color, f64) {
        let swatches = Hue::ALL
            .into_iter()
            .flat_map(|hue| [Row::Strong, Row::Tint].map(|row| Color::Palette(hue, row)));
        std::iter::once(Color::Ink)
            .chain(swatches)
            .map(|color| (color, crate::color_contrast::delta_e(rgb, self.rgb(color))))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((Color::Ink, 0.))
    }
}

/// A typed color: `#1F4E79`, `#17B`, `31, 78, 121`, `rgb(31 78 121)` or
/// `oklch(0.42 0.09 250)`. OKLCH outside sRGB loses chroma at fixed L and h.
pub fn parse_color(text: &str) -> Option<Rgb> {
    let text = text.trim().to_ascii_lowercase();
    let parts = |body: &str| -> Vec<String> {
        body.split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    };
    if let Some(body) = text
        .strip_prefix("oklch(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let [l, c, h] = parts(body).try_into().ok()?;
        let l = match l.strip_suffix('%') {
            Some(percent) => percent.parse::<f64>().ok()? / 100.,
            None => l.parse().ok()?,
        };
        let c: f64 = c.parse().ok()?;
        let h: f64 = h.strip_suffix("deg").unwrap_or(&h).parse().ok()?;
        if !(0. ..=1.).contains(&l) || !(0. ..=0.4).contains(&c) || !(0. ..=360.).contains(&h) {
            return None;
        }
        return Some(
            Oklch {
                l,
                c,
                h: h.to_radians(),
            }
            .to_rgb(),
        );
    }
    let body = text
        .strip_prefix("rgb(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&text);
    if let Ok([r, g, b]) = <[String; 3]>::try_from(parts(body)) {
        return Some([r.parse().ok()?, g.parse().ok()?, b.parse().ok()?]);
    }
    let hex = text.strip_prefix('#').unwrap_or(&text);
    match hex.len() {
        3 => parse_hex(&hex.chars().flat_map(|c| [c, c]).collect::<String>()),
        _ => parse_hex(hex),
    }
}

/// Every stored document color, for resolution and migration passes.
pub fn for_each_color_mut(doc: &mut Document, mut visit: impl FnMut(&mut Color)) {
    for atom in &mut doc.atoms {
        if let Some(color) = &mut atom.display.highlight {
            visit(color);
        }
        if let Some(style) = &mut atom.text_style {
            visit(&mut style.color);
        }
        if let Some(color) = &mut atom.display.hydrogen_color {
            visit(color);
        }
        visit(&mut atom.display.stereo.style.color);
        visit(&mut atom.display.mapping.style.color);
        if let Some(number) = &mut atom.display.number {
            visit(&mut number.style.color);
        }
    }
    for bond in &mut doc.bonds {
        if let Some(color) = &mut bond.highlight {
            visit(color);
        }
        visit(&mut bond.color);
        visit(&mut bond.indicator.style.color);
    }
    for text in &mut doc.annotations {
        visit(&mut text.format.style.color);
        for span in &mut text.format.spans {
            visit(&mut span.style.color);
        }
    }
    for arrow in &mut doc.arrows {
        if let Some(style) = &mut arrow.style {
            visit(&mut style.color);
        }
    }
    for graphic in &mut doc.graphics {
        visit(&mut graphic.style.stroke);
        if let Some(fill) = &mut graphic.style.fill {
            visit(fill);
        }
    }
    for fill in &mut doc.ring_fills {
        visit(&mut fill.color);
    }
    for group in &mut doc.abbreviations {
        if let Some(style) = &mut group.label_style {
            visit(&mut style.color);
        }
        if let Some(color) = &mut group.highlight {
            visit(color);
        }
    }
}
/// Read-only twin of `for_each_color_mut`; a test keeps the two in step.
pub fn any_color(doc: &Document, test: impl Fn(Color) -> bool) -> bool {
    let atoms = doc.atoms.iter().flat_map(|a| {
        a.text_style
            .as_ref()
            .map(|s| s.color)
            .into_iter()
            .chain(a.display.highlight)
            .chain(a.display.hydrogen_color)
            .chain([a.display.stereo.style.color, a.display.mapping.style.color])
            .chain(a.display.number.as_ref().map(|n| n.style.color))
    });
    let bonds = doc.bonds.iter().flat_map(|b| {
        [b.color, b.indicator.style.color]
            .into_iter()
            .chain(b.highlight)
    });
    let texts = doc.annotations.iter().flat_map(|t| {
        std::iter::once(t.format.style.color).chain(t.format.spans.iter().map(|s| s.style.color))
    });
    let arrows = doc
        .arrows
        .iter()
        .filter_map(|a| a.style.as_ref().map(|s| s.color));
    let graphics = doc
        .graphics
        .iter()
        .flat_map(|g| std::iter::once(g.style.stroke).chain(g.style.fill));
    let fills = doc.ring_fills.iter().map(|f| f.color);
    let abbreviations = doc.abbreviations.iter().flat_map(|g| {
        g.label_style
            .as_ref()
            .map(|style| style.color)
            .into_iter()
            .chain(g.highlight)
    });
    atoms
        .chain(bonds)
        .chain(texts)
        .chain(arrows)
        .chain(graphics)
        .chain(fills)
        .chain(abbreviations)
        .any(test)
}

pub const RECENT_LIMIT: usize = 8;
impl Document {
    /// Remember a custom color for the color picker, newest first.
    pub fn remember_color(&mut self, rgb: Rgb) {
        self.recent_colors.retain(|c| *c != rgb);
        self.recent_colors.insert(0, rgb);
        self.recent_colors.truncate(RECENT_LIMIT);
    }
}

/// Change the theme's hues. A built-in theme is embedded as a copy named
/// "<Theme> · custom hues"; atom color overrides are left as they are.
pub fn set_hues(doc: &mut Document, hues: Hues) {
    if doc.custom_theme.is_none() {
        let base = doc.color_theme;
        let mut theme = crate::theme_files::ThemeFile::capture(doc);
        theme.id = format!("{}-custom-hues", base.to_string().to_lowercase());
        theme.name = format!("{base} · custom hues");
        doc.custom_theme = Some(Box::new(theme));
    }
    if let Some(theme) = &mut doc.custom_theme {
        theme.hues = hues;
    }
    doc.version = doc.version.max(16);
}

#[cfg(test)]
mod tests;
