//! Predicted shifts written onto a drawing as atom numbers, for scripts. The
//! app's **Show labels** overlay uses the same text but stays on screen.
use crate::{
    atom_labels::{Number, number_style},
    document::Document,
};

/// What a label shows, as the menu next to **Show labels** offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelFormat {
    /// `#3`
    Numbers,
    /// `1.280 ppm`
    Ppm,
    /// `#3 · 1.280 ppm`
    Both,
}

impl LabelFormat {
    /// `numbers`, `ppm` or `both`, as scripts name the formats.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "numbers" => Some(Self::Numbers),
            "ppm" => Some(Self::Ppm),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

/// The label of atom `atom` at `ppm`.
pub fn label_text(format: LabelFormat, atom: u64, ppm: f64) -> String {
    match format {
        LabelFormat::Numbers => format!("#{atom}"),
        LabelFormat::Ppm => format!("{ppm:.3} ppm"),
        LabelFormat::Both => format!("#{atom} · {ppm:.3} ppm"),
    }
}

/// Writes each `(atom, ppm)` row's label as that atom's number, replacing
/// any number it had: the style is kept ([`number_style`] when there was
/// none) and the offset cleared. Rows without a ppm are skipped and other
/// atoms are untouched. Every row is checked before anything changes: an ID
/// that is not an atom (in any row), a ppm that is not finite or a label
/// over 32 characters is an error. Returns the labelled atom IDs in row
/// order.
pub fn write_labels(
    doc: &mut Document,
    rows: &[(u64, Option<f64>)],
    format: LabelFormat,
) -> Result<Vec<u64>, String> {
    let mut labels = Vec::new();
    for &(atom, ppm) in rows {
        if doc.atom(atom).is_none() {
            return Err(format!("Atom {atom} is not in the drawing"));
        }
        let Some(ppm) = ppm else { continue };
        if !ppm.is_finite() {
            return Err(format!("The ppm of atom {atom} is not a finite number"));
        }
        let text = label_text(format, atom, ppm);
        if text.chars().count() > 32 {
            return Err(format!(
                "The label of atom {atom} is longer than 32 characters"
            ));
        }
        labels.push((atom, text));
    }
    let mut labelled = Vec::with_capacity(labels.len());
    for (id, text) in labels {
        if let Some(atom) = doc.atom_mut(id) {
            let style = atom
                .display
                .number
                .take()
                .map_or_else(number_style, |n| n.style);
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
