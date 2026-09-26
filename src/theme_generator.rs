//! Reference-based theme recipes. Final palettes are portable snapshots, while the
//! optional recipe lets the editor reopen the four controls without guessing.
use crate::{
    canvas_theme::{CanvasTheme, ColorTheme, jmol},
    color_contrast::{Oklch, Rgb},
    theme_files::{ColorValue, ThemeFile},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Freeze a reference's element seeds, without nesting its own generator recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    pub light: BTreeMap<String, ColorValue>,
    pub dark: BTreeMap<String, ColorValue>,
}
impl Reference {
    pub fn capture(theme: &ThemeFile) -> Self {
        let colors = |mode| {
            crate::editing::ELEMENTS
                .iter()
                .map(|&element| {
                    let color = theme
                        .palette(mode)
                        .elements
                        .get(element)
                        .cloned()
                        .unwrap_or_else(|| {
                            ColorValue::Rgb(theme.base.element_color(element, mode))
                        });
                    (element.into(), color)
                })
                .collect()
        };
        Self {
            id: theme.id.clone(),
            name: theme.name.clone(),
            author: theme.author.clone(),
            source: theme.source.clone(),
            license: theme.license.clone(),
            light: colors(CanvasTheme::Light),
            dark: colors(CanvasTheme::Dark),
        }
    }
    fn validate(&self) -> Result<(), String> {
        // Reuse the portable file's identity, credit, and color validators.
        let palette = |elements: &BTreeMap<String, ColorValue>| crate::theme_files::Palette {
            elements: elements.clone(),
            ..Default::default()
        };
        ThemeFile {
            version: 1,
            id: self.id.clone(),
            name: self.name.clone(),
            author: self.author.clone(),
            source: self.source.clone(),
            license: self.license.clone(),
            base: ColorTheme::Publication,
            light: palette(&self.light),
            dark: palette(&self.dark),
            generator: None,
        }
        .validate()?;
        if [self.light.len(), self.dark.len()] != [118, 118] {
            return Err("A generator reference needs all 118 elements in both modes".into());
        }
        Ok(())
    }
    fn color(&self, element: &str, mode: CanvasTheme) -> Option<Rgb> {
        (if mode.is_dark() {
            &self.dark
        } else {
            &self.light
        })
        .get(element)
        .map(ColorValue::rgb)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tone {
    pub lightness: f64,
    pub chroma: f64,
}
impl Tone {
    pub const MAX_CHROMA_FACTOR: f64 = 2.;

    pub(crate) fn swatch(self, rgb: Rgb) -> Rgb {
        let source = Oklch::from_rgb(rgb);
        Oklch {
            l: self.lightness,
            c: source.c * self.chroma,
            ..source
        }
        .to_rgb()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub light: Tone,
    pub dark: Tone,
    /// Omitted for the original Jmol table; otherwise a portable, fixed snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
}
impl Recipe {
    pub const PRESENTATION: Self = Self {
        reference: None,
        light: Tone {
            lightness: 0.50,
            chroma: 2.,
        },
        dark: Tone {
            lightness: 0.70,
            chroma: 1.,
        },
    };
    pub const PASTEL: Self = Self {
        reference: None,
        light: Tone {
            lightness: 0.55,
            chroma: 0.35,
        },
        dark: Tone {
            lightness: 0.83,
            chroma: 0.40,
        },
    };
    pub fn tone(&self, mode: CanvasTheme) -> Tone {
        if mode.is_dark() {
            self.dark
        } else {
            self.light
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(reference) = &self.reference {
            reference.validate()?;
        }
        for tone in [self.light, self.dark] {
            if !tone.lightness.is_finite() || !(0. ..=1.).contains(&tone.lightness) {
                return Err("Theme lightness must be between 0 and 1".into());
            }
            if !tone.chroma.is_finite() || !(0. ..=Tone::MAX_CHROMA_FACTOR).contains(&tone.chroma) {
                return Err("Theme chroma factor must be between 0 and 2".into());
            }
        }
        Ok(())
    }
    /// Regenerate element roles; retain the template's identity and ring fills.
    /// Carbon and hydrogen stay neutral; Jmol has neutral fallbacks beyond Mt.
    pub fn generate(&self, template: &ThemeFile) -> Result<ThemeFile, String> {
        self.validate()?;
        let mut theme = template.clone();
        theme.base = ColorTheme::Presentation;
        theme.generator = Some(self.clone());
        theme.source = self.reference.as_ref().map_or_else(
            || "https://jmol.sourceforge.net/jscolors/".into(),
            |reference| reference.source.clone(),
        );
        for mode in CanvasTheme::ALL {
            let palette = if mode.is_dark() {
                &mut theme.dark
            } else {
                &mut theme.light
            };
            palette.elements.clear();
            palette.tile_seeds.clear();
            for &element in crate::editing::ELEMENTS {
                let seed = self
                    .reference
                    .as_ref()
                    .map_or_else(
                        || jmol::swatch(element),
                        |reference| reference.color(element, mode),
                    )
                    .map(|rgb| self.tone(mode).swatch(rgb));
                let ink = if matches!(element, "C" | "H") {
                    mode.color([0; 3])
                } else {
                    seed.unwrap_or_else(|| mode.color([0; 3]))
                };
                palette
                    .elements
                    .insert(element.into(), ColorValue::Rgb(ink));
                if let Some(seed) = seed {
                    palette
                        .tile_seeds
                        .insert(element.into(), ColorValue::Rgb(seed));
                }
            }
        }
        theme.validate()?;
        Ok(theme)
    }
}
