//! Predicted shifts written onto a drawing as atom numbers, for scripts. The
//! app's **Show labels** overlay uses the same text but stays on screen.
use super::Nucleus;
use crate::{
    atom_labels::{Number, shift_label_style},
    document::Document,
};

/// What a label shows: the shift alone, as journals print it, or one of
/// the formats the menu next to **Show labels** offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelFormat {
    /// `1.28`, the default for scripts; the caption states δ (ppm) once.
    Value,
    /// `#3`
    Numbers,
    /// `1.28 ppm`
    Ppm,
    /// `#3 · 1.28 ppm`
    Both,
}

impl LabelFormat {
    /// `value`, `numbers`, `ppm` or `both`, as scripts name the formats.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "value" => Some(Self::Value),
            "numbers" => Some(Self::Numbers),
            "ppm" => Some(Self::Ppm),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

/// The most decimal places a ppm label may have.
pub const MAX_DECIMALS: u8 = 4;

/// Journal-style decimal places: 2 for ¹H (`7.26`), 1 for ¹³C
/// (`128.4`), and 2 when the rows do not say which nucleus.
pub fn default_decimals(nucleus: Option<Nucleus>) -> u8 {
    match nucleus {
        Some(Nucleus::C13) => 1,
        Some(Nucleus::H1) | None => 2,
    }
}

/// The label of atom `atom` at `ppm`, with `decimals` decimal places.
pub fn label_text(format: LabelFormat, atom: u64, ppm: f64, decimals: u8) -> String {
    let decimals = usize::from(decimals);
    match format {
        LabelFormat::Value => format!("{ppm:.decimals$}"),
        LabelFormat::Numbers => format!("#{atom}"),
        LabelFormat::Ppm => format!("{ppm:.decimals$} ppm"),
        LabelFormat::Both => format!("#{atom} · {ppm:.decimals$} ppm"),
    }
}

/// Writes each `(atom, ppm)` row's label as that atom's number, replacing
/// any number it had: the style is kept ([`shift_label_style`] when there was
/// none) and the offset cleared. Rows without a ppm are skipped and other
/// atoms are untouched. Every row is checked before anything changes: more
/// than [`MAX_DECIMALS`], an ID that is not an atom (in any row), a ppm that
/// is not finite or a label over 32 characters is an error. Returns the
/// labelled atom IDs in row order.
pub fn write_labels(
    doc: &mut Document,
    rows: &[(u64, Option<f64>)],
    format: LabelFormat,
    decimals: u8,
) -> Result<Vec<u64>, String> {
    if decimals > MAX_DECIMALS {
        return Err(format!("decimals must be 0 to {MAX_DECIMALS}"));
    }
    let mut labels = Vec::new();
    for &(atom, ppm) in rows {
        if doc.atom(atom).is_none() {
            return Err(format!("Atom {atom} is not in the drawing"));
        }
        let Some(ppm) = ppm else { continue };
        if !ppm.is_finite() {
            return Err(format!("The ppm of atom {atom} is not a finite number"));
        }
        let text = label_text(format, atom, ppm, decimals);
        if text.chars().count() > 32 {
            return Err(format!(
                "The label of atom {atom} is longer than 32 characters"
            ));
        }
        labels.push((atom, text));
    }
    let default = shift_label_style(doc);
    let mut labelled = Vec::with_capacity(labels.len());
    for (id, text) in labels {
        if let Some(atom) = doc.atom_mut(id) {
            let style = atom
                .display
                .number
                .take()
                .map_or_else(|| default.clone(), |n| n.style);
            atom.display.number = Some(Number {
                text,
                offset: None,
                style,
            });
            labelled.push(id);
        }
    }
    Ok(labelled)
}
