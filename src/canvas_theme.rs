//! Document page colors, independent of journal dimensions and application chrome.
use serde::{Deserialize, Serialize};
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
}
impl ColorTheme {
    pub const ALL: [Self; 3] = [Self::Publication, Self::Presentation, Self::Pastel];
    pub fn is_publication(&self) -> bool {
        *self == Self::Publication
    }
    /// Visible RGB, with muted conventional element hues and neutral carbon/hydrogen.
    pub fn element_color(self, element: &str, canvas: CanvasTheme) -> [u8; 3] {
        let index = match element {
            "N" => 0,
            "O" => 1,
            "F" | "Cl" => 2,
            "S" => 3,
            "P" => 4,
            "Br" => 5,
            "I" => 6,
            "B" | "Si" => 7,
            "Li" | "Na" | "K" | "Mg" | "Ca" | "Fe" | "Co" | "Ni" | "Cu" | "Zn" | "Ru" | "Rh"
            | "Pd" | "Ag" | "Ir" | "Pt" | "Au" => 8,
            _ => return canvas.color([0; 3]),
        };
        let colors = match (self, canvas) {
            (Self::Publication, _) => return canvas.color([0; 3]),
            (Self::Presentation, CanvasTheme::Light) => [
                [55, 94, 158],
                [190, 67, 76],
                [49, 126, 86],
                [145, 116, 27],
                [174, 99, 37],
                [147, 77, 59],
                [125, 81, 157],
                [123, 109, 86],
                [54, 119, 132],
            ],
            (Self::Presentation, CanvasTheme::Dark) => [
                [122, 163, 219],
                [230, 128, 134],
                [116, 191, 146],
                [212, 186, 105],
                [217, 165, 114],
                [203, 145, 123],
                [180, 143, 211],
                [190, 178, 154],
                [120, 185, 198],
            ],
            (Self::Pastel, CanvasTheme::Light) => [
                [94, 126, 170],
                [170, 100, 113],
                [89, 135, 110],
                [141, 124, 64],
                [166, 117, 78],
                [157, 111, 98],
                [137, 112, 167],
                [137, 126, 105],
                [88, 132, 145],
            ],
            (Self::Pastel, CanvasTheme::Dark) => [
                [168, 191, 223],
                [231, 174, 181],
                [164, 209, 182],
                [223, 207, 155],
                [230, 192, 159],
                [216, 178, 164],
                [203, 180, 226],
                [211, 200, 180],
                [163, 207, 215],
            ],
        };
        colors
            .get(index)
            .copied()
            .unwrap_or_else(|| canvas.color([0; 3]))
    }
    /// Selecting a theme resets atom color overrides, but preserves all typography.
    pub fn apply(self, doc: &mut crate::document::Document) {
        doc.color_theme = self;
        for atom in &mut doc.atoms {
            if let Some(style) = &mut atom.text_style {
                style.color = [0; 3];
            }
            atom.display.color_override = false;
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
        })
    }
}

/// Canonical atom ink used by the canvas, exporters, and selection controls.
pub fn atom_color(doc: &crate::document::Document, atom: &crate::document::Atom) -> [u8; 3] {
    let explicit = atom.text_style.as_ref().map_or([0; 3], |s| s.color);
    if atom.display.color_override || explicit != [0; 3] {
        explicit
    } else {
        let mut ink = doc
            .color_theme
            .element_color(&atom.element, doc.canvas_theme);
        if doc.canvas_theme.is_dark() && !doc.color_theme.is_publication() {
            let backgrounds: Vec<_> = doc
                .ring_fills
                .iter()
                .filter(|fill| fill.atoms.contains(&atom.id))
                .map(|fill| fill.visible_color(doc.canvas_theme))
                .collect();
            ink = ring_label_ink(ink, &backgrounds);
        }
        doc.canvas_theme.color(ink)
    }
}

/// Lift automatic element colors just enough to read over a ring highlight.
/// Explicit colors remain untouched. The same resolved ink reaches the canvas,
/// clipboard and editable exchange, without changing stored atom typography.
fn ring_label_ink(ink: [u8; 3], backgrounds: &[[u8; 3]]) -> [u8; 3] {
    let luminance = |rgb: [u8; 3]| {
        rgb.into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(v, w)| {
                let v = f32::from(v) / 255.;
                w * if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            })
            .sum::<f32>()
    };
    let legible = |rgb| {
        let foreground = luminance(rgb);
        backgrounds.iter().all(|&background| {
            let background = luminance(background);
            (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05) >= 4.5
        })
    };
    for step in 0..=100 {
        let tint =
            ink.map(|v| (f32::from(v) + (255. - f32::from(v)) * step as f32 / 100.).round() as u8);
        if legible(tint) {
            return tint;
        }
    }
    // A custom pale fill may not admit lighter ink; preserve its chosen theme.
    ink
}

/// Materialize a theme for renderers and editable exchange. Colors are stored in
/// the canvas's canonical space; the final light/dark conversion happens once.
pub fn resolved_document(
    doc: &crate::document::Document,
) -> std::borrow::Cow<'_, crate::document::Document> {
    if doc.color_theme.is_publication() && doc.ring_fills.iter().all(|fill| fill.fixed_color) {
        return std::borrow::Cow::Borrowed(doc);
    }
    let mut resolved = doc.clone();
    for fill in &mut resolved.ring_fills {
        fill.color = doc.canvas_theme.color(fill.visible_color(doc.canvas_theme));
        fill.fixed_color = true;
    }
    for atom in &mut resolved.atoms {
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
    }
    resolved.color_theme = ColorTheme::Publication;
    std::borrow::Cow::Owned(resolved)
}
