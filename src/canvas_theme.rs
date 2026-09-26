//! Document page colors, independent of journal dimensions and application chrome.
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

/// Materialize the visible colors at the clipboard boundary. Templates still
/// inherit their destination canvas; pasted drawings keep their source appearance.
pub fn for_paste(doc: crate::document::Document, target: CanvasTheme) -> crate::document::Document {
    let mut doc = resolved_document(&doc).into_owned();
    let source = doc.canvas_theme;
    let color = |rgb| target.color(source.color(rgb));
    for atom in &mut doc.atoms {
        atom.display.hydrogen_color = atom.display.hydrogen_color.map(color);
        let style = atom
            .text_style
            .get_or_insert_with(|| doc.drawing_style.text_style());
        style.color = color(style.color);
        atom.display.color_override = true;
        atom.display.stereo.style.color = color(atom.display.stereo.style.color);
        if let Some(number) = &mut atom.display.number {
            number.style.color = color(number.style.color);
        }
    }
    for bond in &mut doc.bonds {
        bond.color = color(bond.color);
        bond.indicator.style.color = color(bond.indicator.style.color);
    }
    for text in &mut doc.annotations {
        text.format.style.color = color(text.format.style.color);
        for span in &mut text.format.spans {
            span.style.color = color(span.style.color);
        }
    }
    for arrow in &mut doc.arrows {
        let mut style = arrow.appearance();
        style.color = color(style.color);
        arrow.style = Some(style);
    }
    for graphic in &mut doc.graphics {
        graphic.style.stroke = color(graphic.style.stroke);
        graphic.style.fill = graphic.style.fill.map(color);
    }
    for fill in &mut doc.ring_fills {
        fill.color = color(fill.color);
        fill.fixed_color = true;
    }
    doc.canvas_theme = target;
    doc
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
                style.color = [0; 3];
            }
            atom.display.color_override = false;
            atom.display.hydrogen_color = None;
            atom.display.stereo.style.color = [0; 3];
            if let Some(number) = &mut atom.display.number {
                number.style.color = [0; 3];
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
    let explicit = atom.text_style.as_ref().map_or([0; 3], |s| s.color);
    if atom.display.color_override || explicit != [0; 3] {
        explicit
    } else {
        let mut ink = element_color(doc, &atom.element, doc.canvas_theme);
        if !doc.ring_fills.is_empty() {
            let backgrounds = label_backgrounds(doc, atom);
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
    atom.display.hydrogen_color.unwrap_or_else(|| {
        let mut hydrogen = atom.clone();
        hydrogen.element = "H".into();
        atom_color(doc, &hydrogen)
    })
}

fn label_backgrounds(
    doc: &crate::document::Document,
    atom: &crate::document::Atom,
) -> Vec<[u8; 3]> {
    std::iter::once(doc.canvas_theme.background())
        .chain(
            doc.ring_fills
                .iter()
                .filter(|fill| fill.atoms.contains(&atom.id))
                .map(|fill| fill_color(doc, fill)),
        )
        .collect()
}

/// Atom IDs whose visible ink cannot meet the text minimum on the paper and
/// associated ring fills. Explicit user colors are checked but never rewritten.
/// Arbitrary overlapping artwork and unknown paste destinations are not covered.
pub fn label_contrast_issues(doc: &crate::document::Document) -> Vec<u64> {
    doc.atoms
        .iter()
        .filter(|atom| {
            if !crate::atom_labels::visible(atom, doc) {
                return false;
            }
            let ink = doc.canvas_theme.color(atom_color(doc, atom));
            !crate::color_contrast::meets(
                ink,
                &label_backgrounds(doc, atom),
                crate::color_contrast::TEXT_MIN,
            )
        })
        .map(|atom| atom.id)
        .collect()
}

/// Materialize a theme for renderers and editable exchange. Colors are stored in
/// the canvas's canonical space; the final light/dark conversion happens once.
pub fn resolved_document(
    doc: &crate::document::Document,
) -> std::borrow::Cow<'_, crate::document::Document> {
    if doc.custom_theme.is_none() && doc.color_theme.is_publication() && doc.ring_fills.is_empty() {
        return std::borrow::Cow::Borrowed(doc);
    }
    let mut resolved = doc.clone();
    for fill in &mut resolved.ring_fills {
        fill.color = doc.canvas_theme.color(fill_color(doc, fill));
        fill.fixed_color = true;
    }
    for atom in &mut resolved.atoms {
        let hydrogen = hydrogen_color(doc, atom);
        // Existing colored files remain explicit overrides, including old files
        // that predate the override flag. The flag also supports explicit black.
        if atom.display.color_override
            || atom.text_style.as_ref().is_some_and(|s| s.color != [0; 3])
        {
            continue;
        }
        let ink = atom_color(doc, atom);
        let style = atom
            .text_style
            .get_or_insert_with(|| doc.drawing_style.text_style());
        style.color = ink;
        atom.display.hydrogen_color = Some(hydrogen);
        atom.display.color_override = true;
    }
    resolved.color_theme = ColorTheme::Publication;
    resolved.custom_theme = None;
    std::borrow::Cow::Owned(resolved)
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
pub fn ring_color(doc: &crate::document::Document, key: [u8; 3]) -> [u8; 3] {
    doc.custom_theme
        .as_ref()
        .and_then(|t| t.fill_color(key, doc.canvas_theme))
        .unwrap_or_else(|| crate::ring_fills::palette_color(key, doc.canvas_theme))
}
pub fn fill_color(doc: &crate::document::Document, fill: &crate::ring_fills::RingFill) -> [u8; 3] {
    if fill.fixed_color {
        fill.visible_color(doc.canvas_theme)
    } else {
        ring_color(doc, fill.color)
    }
}
