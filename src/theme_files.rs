//! Portable canvas palettes; typography and bond dimensions belong to style files.
use crate::{
    canvas_theme::{CanvasTheme, ColorTheme},
    color_contrast::{self, Oklch, Rgb},
    document::Document,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const LIMIT: usize = 256 * 1024;

/// RGB bytes are exact sRGB. OKLCH is an authoring alternative, with hue in
/// degrees; out-of-gamut chroma is reduced before the contrast solver runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ColorValue {
    Rgb(Rgb),
    Oklch { oklch: [f64; 3] },
}
impl ColorValue {
    pub fn rgb(&self) -> Rgb {
        match *self {
            Self::Rgb(rgb) => rgb,
            Self::Oklch { oklch: [l, c, h] } => Oklch {
                l,
                c,
                h: h.to_radians(),
            }
            .to_rgb(),
        }
    }
    fn validate(&self) -> Result<(), String> {
        if let Self::Oklch { oklch: [l, c, h] } = *self
            && !(l.is_finite()
                && c.is_finite()
                && h.is_finite()
                && (0. ..=1.).contains(&l)
                && (0. ..=0.4).contains(&c)
                && (0. ..=360.).contains(&h))
        {
            return Err("OKLCH needs L in 0–1, C in 0–0.4, and hue in 0–360 degrees".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    /// Automatic label colors. The renderer solves contrast against paper/fills.
    #[serde(default)]
    pub elements: BTreeMap<String, ColorValue>,
    /// Independent seeds for pale/dark interface tiles, not label ink.
    #[serde(default)]
    pub tile_seeds: BTreeMap<String, ColorValue>,
    /// Named stable ring slots: Sky, Mint, Rose, Lilac, Sand.
    #[serde(default)]
    pub ring_fills: BTreeMap<String, ColorValue>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// Missing roles inherit this built-in. Full exports enumerate every element.
    pub base: ColorTheme,
    pub light: Palette,
    pub dark: Palette,
    /// Optional editor recipe. Explicit palette values remain authoritative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<crate::theme_generator::Recipe>,
}
impl ThemeFile {
    pub fn palette(&self, mode: CanvasTheme) -> &Palette {
        if mode.is_dark() {
            &self.dark
        } else {
            &self.light
        }
    }
    pub fn element_color(&self, element: &str, mode: CanvasTheme) -> Rgb {
        let seed = self
            .palette(mode)
            .elements
            .get(element)
            .map(ColorValue::rgb)
            .unwrap_or_else(|| self.base.element_color(element, mode));
        color_contrast::ensure_contrast(seed, &[mode.background()], color_contrast::TEXT_TARGET)
            .unwrap_or_else(|| mode.color([0; 3]))
    }
    pub fn element_swatch(&self, element: &str, mode: CanvasTheme) -> Option<Rgb> {
        self.palette(mode)
            .tile_seeds
            .get(element)
            .map(ColorValue::rgb)
            .or_else(|| self.base.element_swatch(element, mode))
    }
    pub fn fill_color(&self, key: Rgb, mode: CanvasTheme) -> Option<Rgb> {
        let name = crate::ring_fills::PALETTE
            .iter()
            .find(|(_, rgb)| *rgb == key)?
            .0;
        self.palette(mode).ring_fills.get(name).map(ColorValue::rgb)
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(recipe) = &self.generator {
            recipe.validate()?;
        }
        if self.version != 1 {
            return Err(format!("Unsupported theme version {}", self.version));
        }
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err("Theme ID needs 1–64 lowercase letters, digits or hyphens".into());
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
        {
            return Err("Theme name needs 1–80 printable characters".into());
        }
        for text in [&self.author, &self.source, &self.license] {
            if text.len() > 2048 || text.chars().any(char::is_control) {
                return Err("Invalid theme credits".into());
            }
        }
        for mode in CanvasTheme::ALL {
            let palette = self.palette(mode);
            for map in [&palette.elements, &palette.tile_seeds] {
                for (symbol, value) in map {
                    if !crate::editing::ELEMENTS.contains(&symbol.as_str()) {
                        return Err(format!("Unknown theme element {symbol}"));
                    }
                    value.validate()?;
                }
            }
            for (name, color) in &palette.ring_fills {
                if !crate::ring_fills::PALETTE.iter().any(|(n, _)| n == name) {
                    return Err(format!("Unknown ring-fill slot {name}"));
                }
                color.validate()?;
                // Neutral bonds and publication labels must stay visible. This
                // also guarantees a feasible shared ink for automatic labels.
                if color_contrast::contrast(color.rgb(), mode.color([0; 3]))
                    < color_contrast::TEXT_TARGET
                {
                    return Err(format!(
                        "{mode} ring fill {name} needs at least 5:1 contrast with {} ink",
                        if mode.is_dark() { "white" } else { "black" }
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn apply(self, doc: &mut Document) -> Result<(), String> {
        self.validate()?;
        self.base.apply(doc);
        doc.version = doc.version.max(16);
        doc.custom_theme = Some(Box::new(self.expanded()));
        Ok(())
    }
    /// Freeze inherited roles when embedding or exporting, so a later change
    /// to a built-in does not alter a user's saved theme. Preserve authored OKLCH.
    fn expanded(mut self) -> Self {
        for mode in CanvasTheme::ALL {
            let base = self.base;
            let palette = if mode.is_dark() {
                &mut self.dark
            } else {
                &mut self.light
            };
            for &element in crate::editing::ELEMENTS {
                palette
                    .elements
                    .entry(element.into())
                    .or_insert_with(|| ColorValue::Rgb(base.element_color(element, mode)));
                if let Some(rgb) = base.element_swatch(element, mode) {
                    palette
                        .tile_seeds
                        .entry(element.into())
                        .or_insert(ColorValue::Rgb(rgb));
                }
            }
            for (name, key) in crate::ring_fills::PALETTE {
                palette.ring_fills.entry(name.into()).or_insert_with(|| {
                    ColorValue::Rgb(crate::ring_fills::palette_color(key, mode))
                });
            }
        }
        self
    }
    /// Export a self-contained snapshot of both modes. No geometry, fonts,
    /// journal settings, interface mode, or individual object overrides.
    pub fn capture(doc: &Document) -> Self {
        if let Some(theme) = &doc.custom_theme {
            return theme.as_ref().clone().expanded();
        }
        let base = doc.color_theme;
        let palette = |mode| Palette {
            elements: crate::editing::ELEMENTS
                .iter()
                .map(|&e| (e.into(), ColorValue::Rgb(base.element_color(e, mode))))
                .collect(),
            tile_seeds: crate::editing::ELEMENTS
                .iter()
                .filter_map(|&e| {
                    base.element_swatch(e, mode)
                        .map(|rgb| (e.into(), ColorValue::Rgb(rgb)))
                })
                .collect(),
            ring_fills: crate::ring_fills::PALETTE
                .iter()
                .map(|&(name, key)| {
                    (
                        name.into(),
                        ColorValue::Rgb(crate::ring_fills::palette_color(key, mode)),
                    )
                })
                .collect(),
        };
        Self {
            version: 1,
            id: base.to_string().to_lowercase(),
            name: base.to_string(),
            author: String::new(),
            source: if base.is_publication() {
                String::new()
            } else {
                "https://jmol.sourceforge.net/jscolors/".into()
            },
            license: String::new(),
            base,
            light: palette(CanvasTheme::Light),
            dark: palette(CanvasTheme::Dark),
            generator: None,
        }
    }
}
pub fn load(path: &Path) -> Result<ThemeFile, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Theme file exceeds 256 KB".into());
    }
    let theme: ThemeFile =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid theme: {e}"))?;
    theme.validate()?;
    Ok(theme)
}
pub fn save(path: &Path, theme: &ThemeFile) -> Result<(), String> {
    theme.validate()?;
    let bytes = serde_json::to_vec_pretty(theme).map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Theme file exceeds 256 KB".into());
    }
    crate::storage::write_atomic(path, &bytes)
}

/// Curated additions are code-reviewed with their source and contrast checks.
/// Add include_str! entries here when accepting GitHub theme contributions.
pub fn bundled() -> Result<Vec<ThemeFile>, String> {
    [include_str!("../presets/themes/soft-jmol.reshiki-theme")]
        .into_iter()
        .map(|json| {
            let theme: ThemeFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
            theme.validate()?;
            Ok(theme)
        })
        .collect()
}
