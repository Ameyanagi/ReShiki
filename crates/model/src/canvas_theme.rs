//! Document page colors, independent of journal dimensions and application chrome.
use crate::palette::{Color, Palette};
use serde::{Deserialize, Serialize};
pub(crate) mod jmol;
pub(crate) mod vesta;
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
    // Card papers belong to the source's display, not to the pasted copy.
    doc.on_other_canvas.clear();
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
    Vesta,
}
impl ColorTheme {
    pub const ALL: [Self; 5] = [
        Self::Publication,
        Self::Presentation,
        Self::Pastel,
        Self::Jmol,
        Self::Vesta,
    ];
    /// Credit for the element reference colors; empty for Publication.
    pub fn source(self) -> &'static str {
        match self {
            Self::Publication => "",
            Self::Presentation | Self::Pastel | Self::Jmol => jmol::SOURCE,
            Self::Vesta => vesta::SOURCE,
        }
    }
    /// Terms of copied reference data; Jmol's colors are cited, not copied text.
    pub fn license(self) -> &'static str {
        match self {
            Self::Vesta => vesta::LICENSE,
            _ => "",
        }
    }
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
    /// Presentation and Pastel share Jmol's element mapping with different tones;
    /// Jmol and VESTA show their reference colors unchanged.
    pub fn element_swatch(self, element: &str, canvas: CanvasTheme) -> Option<[u8; 3]> {
        match self {
            Self::Publication => None,
            Self::Jmol => jmol::swatch(element),
            Self::Vesta => vesta::swatch(element),
            Self::Presentation | Self::Pastel => {
                jmol::swatch(element).map(|rgb| jmol::soften(rgb, canvas, self == Self::Pastel))
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
            atom.display.mapping.style.color = Color::Ink;
            atom.display.stereo_group.style.color = Color::Ink;
            if let Some(number) = &mut atom.display.number {
                number.style.color = Color::Ink;
            }
        }
        for group in &mut doc.abbreviations {
            if let Some(style) = &mut group.label_style {
                style.color = Color::Ink;
            }
            group.label_color_override = false;
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
            Self::Vesta => "VESTA",
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
    if let Some(group) = doc.abbreviation(atom.id) {
        label_ink_on(
            doc,
            palette,
            atom,
            group.text_style(doc).color,
            group.color_override(doc),
            crate::highlights::atom_color(doc, atom),
        )
    } else {
        atom_ink_on(doc, palette, atom, atom.display.highlight)
    }
}
fn atom_ink_on(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
    highlight: Option<Color>,
) -> [u8; 3] {
    label_ink_on(
        doc,
        palette,
        atom,
        atom.text_style.as_ref().map_or(Color::Ink, |s| s.color),
        atom.display.color_override,
        highlight,
    )
}
fn label_ink_on(
    doc: &crate::document::Document,
    palette: &Palette,
    atom: &crate::document::Atom,
    explicit: Color,
    color_override: bool,
    highlight: Option<Color>,
) -> [u8; 3] {
    if color_override || explicit != Color::Ink {
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
    // Only its own group exempts an anchor; another group may still hide it.
    let hidden: std::collections::HashSet<_> = doc
        .abbreviations
        .iter()
        .flat_map(|group| {
            group
                .members
                .iter()
                .copied()
                .filter(move |id| *id != group.anchor)
        })
        .collect();
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
            if hidden.contains(&atom.id) || crate::attachments::hidden(atom, doc) {
                return false;
            }
            if doc.abbreviation(atom.id).is_none()
                && !crate::atom_labels::visible_with_degree(atom, doc, || {
                    degrees.get(&atom.id).copied().unwrap_or(0)
                })
            {
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
    resolve_document(doc)
}

/// Editable formats keep the contracted label and its underlying atoms as
/// separate text objects. Each wrapper and internal atom resolves its own ink
/// against its own halo, so expansion keeps the internal presentation.
pub fn resolved_exchange_document(
    doc: &crate::document::Document,
) -> std::borrow::Cow<'_, crate::document::Document> {
    resolve_document(doc)
}

fn resolve_document(
    doc: &crate::document::Document,
) -> std::borrow::Cow<'_, crate::document::Document> {
    use std::borrow::Cow;
    let carded = on_fills(doc);
    let doc = carded.as_ref();
    if doc.custom_theme.is_none()
        && doc.color_theme.is_publication()
        && doc.ring_fills.is_empty()
        && !crate::highlights::any(doc)
        && !crate::palette::any_color(doc, |c| matches!(c, Color::Palette(..) | Color::Paper))
    {
        return carded;
    }
    Cow::Owned(resolve_all(doc))
}

/// The canvas whose ink reads on `rgb`: black ink on light paper.
pub fn ink_canvas(rgb: [u8; 3]) -> CanvasTheme {
    use crate::color_contrast::contrast;
    if contrast([0; 3], rgb) >= contrast([255; 3], rgb) {
        CanvasTheme::Light
    } else {
        CanvasTheme::Dark
    }
}

/// Objects drawn on a filled shape take the colors of the canvas whose ink
/// reads on that fill: black ink on a light card even on the dark canvas, and
/// white ink on a dark card on the light one. Palette fills follow the canvas,
/// so this mostly concerns custom and Ink fills. The topmost filled shape
/// under an object's middle decides; shapes stack in paint order.
fn on_fills(doc: &crate::document::Document) -> std::borrow::Cow<'_, crate::document::Document> {
    use crate::{document::Point, graphics::flattened};
    /// A filled shape: its outline, bounds and the canvas its contents take.
    struct Card {
        outline: Vec<Vec<Point>>,
        bounds: (Point, Point),
        canvas: CanvasTheme,
    }
    let canvas = doc.canvas_theme;
    let [light, dark] = CanvasTheme::ALL.map(|mode| Palette::in_mode(doc, mode));
    let under = |cards: &[Card], p: Point| {
        cards
            .iter()
            .rev()
            .find(|card| {
                let (lo, hi) = card.bounds;
                (lo.x..=hi.x).contains(&p.x)
                    && (lo.y..=hi.y).contains(&p.y)
                    && crate::graphics::fills(p, &card.outline)
            })
            .map_or(canvas, |card| card.canvas)
    };
    let middle = |(lo, hi): (Point, Point)| Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    // Shapes in paint order; the chemistry lies over those behind it.
    let mut order: Vec<_> = doc.graphics.iter().enumerate().collect();
    order.sort_by_key(|(_, g)| g.layer);
    let mut cards: Vec<Card> = vec![];
    let mut graphics = vec![canvas; doc.graphics.len()];
    // How many cards lie under each shape's text: those painted before it
    // and its own.
    let mut through = vec![0; doc.graphics.len()];
    let mut behind_chemistry = None;
    for (i, g) in order {
        if g.layer >= 0 && behind_chemistry.is_none() {
            behind_chemistry = Some(cards.len());
        }
        let bounds = g.bounds();
        let mode = under(&cards, middle(bounds));
        if let Some(slot) = graphics.get_mut(i) {
            *slot = mode;
        }
        let palette = if mode.is_dark() { dark } else { light };
        if let Some(rgb) = g.backing(|c| palette.rgb(c), mode.background()) {
            // Shading is white at its highlight and mostly light; ink reads
            // on its middle band.
            let rgb = if g.style.effects.shaded {
                crate::graphics::shade(rgb, 0.5)
            } else {
                rgb
            };
            cards.push(Card {
                outline: flattened(&g.commands()),
                bounds,
                canvas: ink_canvas(rgb),
            });
        }
        if let Some(slot) = through.get_mut(i) {
            *slot = cards.len();
        }
    }
    if cards.iter().all(|card| card.canvas == canvas) {
        return std::borrow::Cow::Borrowed(doc);
    }
    let chemistry = cards
        .get(..behind_chemistry.unwrap_or(cards.len()))
        .unwrap_or_default();
    let on = |p: Point| under(chemistry, p);
    let positions: std::collections::HashMap<u64, Point> =
        doc.atoms.iter().map(|a| (a.id, a.position)).collect();
    let position = |id| positions.get(&id).copied().unwrap_or_default();
    let atoms: Vec<_> = doc.atoms.iter().map(|a| on(a.position)).collect();
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .map(|b| on(middle((position(b.a), position(b.b)))))
        .collect();
    let arrows: Vec<_> = doc
        .arrows
        .iter()
        .map(|a| on(middle((a.start, a.end))))
        .collect();
    let texts: Vec<_> = if doc.annotations.is_empty() {
        vec![]
    } else {
        // Text in a shape in front of the drawing is painted right after its
        // shape, so the cards up to that shape decide.
        let held: std::collections::HashMap<u64, usize> = doc
            .graphics
            .iter()
            .zip(&through)
            .filter(|(g, _)| g.layer >= 0)
            .filter_map(|(g, count)| Some((g.text?, *count)))
            .collect();
        let each: Vec<_> = doc.annotations.iter().map(|t| vec![t.id]).collect();
        crate::scene::selections_bounds(doc, &each)
            .into_iter()
            .zip(&doc.annotations)
            .map(|(bounds, t)| {
                bounds.map_or(canvas, |bounds| match held.get(&t.id) {
                    Some(count) => under(cards.get(..*count).unwrap_or_default(), middle(bounds)),
                    None => on(middle(bounds)),
                })
            })
            .collect()
    };
    let modes: std::collections::HashMap<u64, CanvasTheme> = doc
        .atoms
        .iter()
        .map(|a| a.id)
        .zip(atoms.iter().copied())
        .collect();
    let atom_mode = |id| modes.get(&id).copied().unwrap_or(canvas);
    // The other canvas's look, with Ink and Paper as exact colors.
    let mut flipped = doc.clone();
    flipped.canvas_theme = canvas.toggled();
    for arrow in &mut flipped.arrows {
        arrow.style = Some(arrow.appearance());
    }
    let mut shown = resolve_all(&flipped);
    let (ink, paper) = (
        Color::Custom(flipped.canvas_theme.color([0; 3])),
        Color::Custom(flipped.canvas_theme.background()),
    );
    crate::palette::for_each_color_mut(&mut shown, |color| match color {
        Color::Ink => *color = ink,
        Color::Paper => *color = paper,
        _ => {}
    });
    let mut carded = doc.clone();
    fn swap<T: Clone>(canvas: CanvasTheme, modes: &[CanvasTheme], target: &mut [T], source: &[T]) {
        for ((mode, item), shown) in modes.iter().zip(target).zip(source) {
            if *mode != canvas {
                item.clone_from(shown);
            }
        }
    }
    swap(canvas, &atoms, &mut carded.atoms, &shown.atoms);
    swap(canvas, &bonds, &mut carded.bonds, &shown.bonds);
    swap(canvas, &arrows, &mut carded.arrows, &shown.arrows);
    swap(canvas, &texts, &mut carded.annotations, &shown.annotations);
    swap(canvas, &graphics, &mut carded.graphics, &shown.graphics);
    let groups: Vec<_> = doc
        .abbreviations
        .iter()
        .map(|g| atom_mode(g.anchor))
        .collect();
    swap(
        canvas,
        &groups,
        &mut carded.abbreviations,
        &shown.abbreviations,
    );
    let fills: Vec<_> = doc
        .ring_fills
        .iter()
        .map(|f| f.atoms.first().map_or(canvas, |id| atom_mode(*id)))
        .collect();
    swap(canvas, &fills, &mut carded.ring_fills, &shown.ring_fills);
    let atom_ids = doc.atoms.iter().map(|a| a.id).zip(&atoms);
    let arrow_ids = doc.arrows.iter().map(|a| a.id).zip(&arrows);
    carded.on_other_canvas = atom_ids
        .chain(arrow_ids)
        .filter(|(_, mode)| **mode != canvas)
        .map(|(id, _)| id)
        .collect();
    std::borrow::Cow::Owned(carded)
}

/// Every theme color as it shows on the document's canvas; see
/// [`resolved_document`].
fn resolve_all(doc: &crate::document::Document) -> crate::document::Document {
    let palette = Palette::of(doc);
    let mut resolved = doc.clone();
    crate::palette::for_each_color_mut(&mut resolved, |color| {
        if matches!(color, Color::Palette(..) | Color::Paper) {
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
    for (group, original) in resolved.abbreviations.iter_mut().zip(&doc.abbreviations) {
        if let Some(anchor) = doc.atom(original.anchor) {
            let mut style = original.text_style(doc);
            style.color = visible(atom_ink(doc, &palette, anchor));
            group.label_style = Some(style);
            group.label_color_override = true;
        }
    }
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
        let highlight = original.display.highlight;
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
    resolved
}

/// A resolved document in the renderers' canonical light-canvas bytes: custom
/// colors are converted so the renderer's single light/dark conversion shows
/// them exactly. Scene code only; never store or resolve the result again.
pub fn canonical_document(
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
mod contrast_tests;

#[cfg(test)]
mod fill_ink_tests;
