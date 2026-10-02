//! Document page colors, independent of journal dimensions and application chrome.
use crate::palette::{Color, Palette};
use serde::{Deserialize, Serialize};
pub(crate) mod jmol;
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasTheme {
    #[default]
    Light,
    Dark,
}
impl CanvasTheme {
    pub const ALL: [Self; 2] = [Self::Light, Self::Dark];
    pub fn is_light(&self) -> bool {
        *self == Self::Light
    }
    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }
    pub fn toggled(self) -> Self {
        if self.is_dark() {
            Self::Light
        } else {
            Self::Dark
        }
    }
    /// A reversible lightness change retains hue and all geometry.
    pub fn color(self, rgb: [u8; 3]) -> [u8; 3] {
        if self.is_light() {
            return rgb;
        }
        let [r, g, b] = rgb;
        let offset = 255 - i16::from(r.max(g).max(b)) - i16::from(r.min(g).min(b));
        rgb.map(|value| (i16::from(value) + offset) as u8)
    }
    pub fn background(self) -> [u8; 3] {
        self.color([255; 3])
    }
}
impl std::fmt::Display for CanvasTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.is_dark() { "Dark" } else { "Light" })
    }
}

/// Materialize the visible colors of a drawing from another source (ChemDraw,
/// CDX/CDXML, text) or bound for another editor. Such drawings keep their source
/// appearance as custom colors, which are exact on any canvas. Ink pasted onto
/// the same canvas already looks the same, so it stays Ink and keeps following
/// the canvas. Templates still inherit their destination canvas.
pub fn for_paste(doc: crate::document::Document, target: CanvasTheme) -> crate::document::Document {
    let mut doc = resolved_document(&doc).into_owned();
    keep_typography(&mut doc);
    for atom in &mut doc.atoms {
        atom.display.color_override = true;
    }
    if doc.canvas_theme != target {
        let ink = Color::Custom(doc.canvas_theme.color([0; 3]));
        crate::palette::for_each_color_mut(&mut doc, |color| {
            if *color == Color::Ink {
                *color = ink;
            }
        });
    }
    doc.canvas_theme = target;
    doc
}

/// ReShiki's own clipboard data pastes like a duplicate: Ink, palette colors and
/// automatic atom colors resolve in the target's theme, canvas and hues, and
/// custom colors stay exact. Only the source typography is kept.
pub fn for_native_paste(
    mut doc: crate::document::Document,
    target: CanvasTheme,
) -> crate::document::Document {
    keep_typography(&mut doc);
    doc.canvas_theme = target;
    doc
}

fn keep_typography(doc: &mut crate::document::Document) {
    let text = doc.drawing_style.text_style();
    for atom in &mut doc.atoms {
        atom.text_style.get_or_insert_with(|| text.clone());
    }
    for arrow in &mut doc.arrows {
        arrow.style = Some(arrow.appearance());
    }
}

/// Element colors are independent of journal dimensions and paper brightness.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorTheme {
    #[default]
    Publication,
    Presentation,
    Pastel,
    Jmol,
}
impl ColorTheme {
    pub const ALL: [Self; 4] = [
        Self::Publication,
        Self::Presentation,
        Self::Pastel,
        Self::Jmol,
    ];
    pub fn is_publication(&self) -> bool {
        *self == Self::Publication
    }
    /// Visible label RGB with enough contrast against the document's paper.
    pub fn element_color(self, element: &str, canvas: CanvasTheme) -> [u8; 3] {
        if matches!(self, Self::Presentation | Self::Pastel) && matches!(element, "C" | "H") {
            return canvas.color([0; 3]);
        }
        self.element_swatch(element, canvas)
            .map(|rgb| jmol::label_ink(rgb, canvas))
            .unwrap_or_else(|| canvas.color([0; 3]))
    }
    /// Tiles show the palette's hues separately from canvas-label contrast.
    /// Presentation and Pastel share Jmol's element mapping with different tones.
    pub fn element_swatch(self, element: &str, canvas: CanvasTheme) -> Option<[u8; 3]> {
        let rgb = jmol::swatch(element)?;
        match self {
            Self::Publication => None,
            Self::Jmol => Some(rgb),
            Self::Presentation | Self::Pastel => {
                Some(jmol::soften(rgb, canvas, self == Self::Pastel))
            }
        }
    }
    /// Selecting a theme resets atom color overrides, but preserves all typography.
    pub fn apply(self, doc: &mut crate::document::Document) {
        doc.color_theme = self;
        doc.custom_theme = None;
        for atom in &mut doc.atoms {
            if let Some(style) = &mut atom.text_style {
                style.color = Color::Ink;
            }
            atom.display.color_override = false;
            atom.display.hydrogen_color = None;
            atom.display.stereo.style.color = Color::Ink;
            if let Some(number) = &mut atom.display.number {
                number.style.color = Color::Ink;
            }
        }
    }
}
impl std::fmt::Display for ColorTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Publication => "Publication",
            Self::Presentation => "Presentation",
            Self::Pastel => "Pastel",
            Self::Jmol => "Jmol",
        })
    }
}

/// Canonical atom ink used by the canvas, exporters, and selection controls.
pub fn atom_color(doc: &crate::document::Document, atom: &crate::document::Atom) -> [u8; 3] {
    atom_ink(doc, &Palette::of(doc), atom)
}
fn atom_ink(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
) -> [u8; 3] {
    atom_ink_on(doc, palette, atom, crate::highlights::atom_color(doc, atom))
}
fn atom_ink_on(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
    highlight: Option<Color>,
) -> [u8; 3] {
    let explicit = atom.text_style.as_ref().map_or(Color::Ink, |s| s.color);
    if atom.display.color_override || explicit != Color::Ink {
        palette.canonical(explicit)
    } else {
        let mut ink = element_color(doc, &atom.element, doc.canvas_theme);
        if !doc.ring_fills.is_empty() || highlight.is_some() {
            let backgrounds = label_backgrounds_on(doc, palette, atom, highlight);
            ink = crate::color_contrast::ensure_contrast(
                ink,
                &backgrounds,
                crate::color_contrast::TEXT_TARGET,
            )
            .or_else(|| {
                crate::color_contrast::ensure_contrast(
                    ink,
                    &backgrounds,
                    crate::color_contrast::TEXT_MIN,
                )
            })
            .unwrap_or(ink);
        }
        doc.canvas_theme.color(ink)
    }
}

/// Attached hydrogen follows the H palette, with the parent's explicit label overrides.
pub fn hydrogen_color(doc: &crate::document::Document, atom: &crate::document::Atom) -> [u8; 3] {
    hydrogen_ink(doc, &Palette::of(doc), atom)
}
fn hydrogen_ink(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
) -> [u8; 3] {
    hydrogen_ink_on(doc, palette, atom, crate::highlights::atom_color(doc, atom))
}
fn hydrogen_ink_on(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
    highlight: Option<Color>,
) -> [u8; 3] {
    atom.display.hydrogen_color.map_or_else(
        || {
            let mut hydrogen = atom.clone();
            hydrogen.element = "H".into();
            atom_ink_on(doc, palette, &hydrogen, highlight)
        },
        |color| palette.canonical(color),
    )
}

fn label_backgrounds(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
) -> Vec<[u8; 3]> {
    label_backgrounds_on(doc, palette, atom, crate::highlights::atom_color(doc, atom))
}
fn label_backgrounds_on(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
    highlight: Option<Color>,
) -> Vec<[u8; 3]> {
    if let Some(color) = highlight {
        // The atom halo covers the complete label, so its ink is read against
        // the halo rather than the paper or ring fill beneath it.
        return vec![palette.rgb(color)];
    }
    std::iter::once(doc.canvas_theme.background())
        .chain(
            doc.ring_fills
                .iter()
                .filter(|fill| fill.atoms.contains(&atom.id))
                .map(|fill| palette.rgb(fill.color)),
        )
        .collect()
}

/// Atom IDs whose visible ink cannot meet the text minimum on the paper and
/// associated ring fills. Explicit user colors are checked but never rewritten.
/// Arbitrary overlapping artwork and unknown paste destinations are not covered.
pub fn label_contrast_issues(doc: &crate::document::Document) -> Vec<u64> {
    let palette = Palette::of(doc);
    let mut degrees = std::collections::HashMap::<u64, usize>::new();
    for bond in &doc.bonds {
        *degrees.entry(bond.a).or_default() += 1;
        if bond.a != bond.b {
            *degrees.entry(bond.b).or_default() += 1;
        }
    }
    doc.atoms
        .iter()
        .filter(|atom| {
            if !crate::atom_labels::visible_with_degree(atom, doc, || {
                degrees.get(&atom.id).copied().unwrap_or(0)
            }) {
                return false;
            }
            let ink = doc.canvas_theme.color(atom_ink(doc, &palette, atom));
            !crate::color_contrast::meets(
                ink,
                &label_backgrounds(doc, &palette, atom),
                crate::color_contrast::TEXT_MIN,
            )
        })
        .map(|atom| atom.id)
        .collect()
}

/// Materialize the theme for renderers and editable exchange: palette colors
/// become custom colors as they appear on the document's canvas, and automatic
/// atom ink becomes explicit. Ink and custom colors are kept, so resolving a
/// resolved document changes nothing.
pub fn resolved_document(
    doc: &crate::document::Document,
) -> std::borrow::Cow<'_, crate::document::Document> {
    resolve_document(doc, false)
}

/// Editable formats keep the contracted label and its underlying atoms as
/// separate text objects. Resolve internal atom ink against its own halo;
/// the writer resolves each visible wrapper label against the group paint.
pub(crate) fn resolved_exchange_document(
    doc: &crate::document::Document,
) -> std::borrow::Cow<'_, crate::document::Document> {
    resolve_document(doc, true)
}

fn resolve_document(
    doc: &crate::document::Document,
    expanded_atoms: bool,
) -> std::borrow::Cow<'_, crate::document::Document> {
    if doc.custom_theme.is_none()
        && doc.color_theme.is_publication()
        && doc.ring_fills.is_empty()
        && !crate::highlights::any(doc)
        && !crate::palette::any_color(doc, |c| matches!(c, Color::Palette(..)))
    {
        return std::borrow::Cow::Borrowed(doc);
    }
    let palette = Palette::of(doc);
    let mut resolved = doc.clone();
    crate::palette::for_each_color_mut(&mut resolved, |color| {
        if matches!(color, Color::Palette(..)) {
            *color = Color::Custom(palette.rgb(*color));
        }
    });
    let ink = palette.rgb(Color::Ink);
    let visible = |canonical| {
        let rgb = doc.canvas_theme.color(canonical);
        if rgb == ink {
            Color::Ink
        } else {
            Color::Custom(rgb)
        }
    };
    for (atom, original) in resolved.atoms.iter_mut().zip(&doc.atoms) {
        // Existing colored files remain explicit overrides, including old files
        // that predate the override flag. The flag also supports explicit black.
        if original.display.color_override
            || original
                .text_style
                .as_ref()
                .is_some_and(|s| s.color != Color::Ink)
        {
            continue;
        }
        let style = atom
            .text_style
            .get_or_insert_with(|| doc.drawing_style.text_style());
        let highlight = if expanded_atoms {
            original.display.highlight
        } else {
            crate::highlights::atom_color(doc, original)
        };
        style.color = visible(atom_ink_on(doc, &palette, original, highlight));
        atom.display.hydrogen_color =
            Some(visible(hydrogen_ink_on(doc, &palette, original, highlight)));
        atom.display.color_override = true;
    }
    for (bond, original) in resolved.bonds.iter_mut().zip(&doc.bonds) {
        if original.color != Color::Ink {
            continue;
        }
        if let Some(background) = original.highlight.map(|color| palette.rgb(color))
            && let Some(rgb) = crate::color_contrast::ensure_contrast(
                ink,
                &[background],
                crate::color_contrast::TEXT_MIN,
            )
            && rgb != ink
        {
            // Automatic ink can change contrast on a highlight without
            // overwriting the stored bond color or a manual foreground color.
            bond.color = Color::Custom(rgb);
        }
    }
    resolved.color_theme = ColorTheme::Publication;
    resolved.custom_theme = None;
    std::borrow::Cow::Owned(resolved)
}

/// A resolved document in the renderers' canonical light-canvas bytes: custom
/// colors are converted so the renderer's single light/dark conversion shows
/// them exactly. Scene code only; never store or resolve the result again.
pub(crate) fn canonical_document(
    doc: &crate::document::Document,
) -> std::borrow::Cow<'_, crate::document::Document> {
    let resolved = resolved_document(doc);
    let canvas = resolved.canvas_theme;
    if canvas.is_light() {
        return resolved;
    }
    let mut canonical = resolved.into_owned();
    crate::palette::for_each_color_mut(&mut canonical, |color| {
        if let Color::Custom(rgb) = *color {
            *color = Color::Custom(canvas.color(rgb));
        }
    });
    std::borrow::Cow::Owned(canonical)
}

/// The document's selected palette, including an embedded user theme.
pub fn element_color(doc: &crate::document::Document, element: &str, mode: CanvasTheme) -> [u8; 3] {
    doc.custom_theme.as_ref().map_or_else(
        || doc.color_theme.element_color(element, mode),
        |t| t.element_color(element, mode),
    )
}
pub fn element_swatch(
    doc: &crate::document::Document,
    element: &str,
    mode: CanvasTheme,
) -> Option<[u8; 3]> {
    doc.custom_theme.as_ref().map_or_else(
        || doc.color_theme.element_swatch(element, mode),
        |t| t.element_swatch(element, mode),
    )
}

#[cfg(test)]
mod contrast_tests {
    use super::*;
    use crate::{atom_labels::Carbons, document::Document};

    #[test]
    fn indexed_contrast_matches_label_visibility_for_themes_and_carbon_modes() {
        let mut doc: Document =
            serde_json::from_str(include_str!("../assets/examples/shortcut-examples.rsk")).unwrap();
        for (index, atom) in doc.atoms.iter_mut().enumerate() {
            if index % 3 == 0 {
                atom.display.color_override = true;
                atom.text_style = Some(crate::typography::TextStyle {
                    color: Color::Custom([235; 3]),
                    ..Default::default()
                });
            }
        }
        let isolated = doc.add_atom("C", Default::default());
        doc.atom_mut(isolated).unwrap().display.color_override = true;
        doc.atom_mut(isolated).unwrap().text_style = Some(crate::typography::TextStyle {
            color: Color::Custom([235; 3]),
            ..Default::default()
        });
        for canvas in CanvasTheme::ALL {
            for carbons in Carbons::ALL {
                doc.canvas_theme = canvas;
                doc.atom_labels.carbons = carbons;
                let before = doc.clone();
                let expected: Vec<_> = doc
                    .atoms
                    .iter()
                    .filter(|atom| {
                        crate::atom_labels::visible(atom, &doc)
                            && !crate::color_contrast::meets(
                                doc.canvas_theme.color(atom_color(&doc, atom)),
                                &label_backgrounds(&doc, &Palette::of(&doc), atom),
                                crate::color_contrast::TEXT_MIN,
                            )
                    })
                    .map(|atom| atom.id)
                    .collect();
                assert_eq!(label_contrast_issues(&doc), expected);
                assert_eq!(doc, before);
            }
        }
    }
}
