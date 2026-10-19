//! Text at an atom endpoint: an element, a real collapsed group, or a named dummy.
use crate::{
    document::Document,
    editing::ELEMENTS,
    typography::{Script, ScriptRun},
};
fn known_group(text: &str) -> bool {
    crate::abbreviations::is_group_label(text)
}
mod hydride;
pub use hydride::entry;
mod from_text;
pub use from_text::{
    TEXT_TOO_LONG, text_atom_position, text_atom_refusal, text_first_glyph_center, text_ink_bounds,
    text_to_atom,
};

/// Unambiguous condensed spellings of existing structural definitions.
fn formula_group(text: &str) -> Option<(&'static str, &'static str)> {
    match text {
        "C2H5" | "C₂H₅" => Some(("Et", "H5C2")),
        "CH2CH3" | "CH₂CH₃" => Some(("Et", "H3CH2C")),
        "OCH3" | "OCH₃" => Some(("OMe", "H3CO")),
        "OC2H5" | "OC₂H₅" => Some(("OEt", "H5C2O")),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Auto,
    Text,
    Group,
}
impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Automatic",
            Self::Text => "Text label (dummy atom)",
            Self::Group => "Chemical abbreviation",
        })
    }
}
/// Describes what Apply creates. Manual script runs in Automatic mode keep the
/// text out of the generic grammar, as in [`apply_with_scripts`].
pub fn description(text: &str, runs: &[ScriptRun], mode: Mode) -> &'static str {
    let generic = mode == Mode::Group || mode == Mode::Auto && runs.is_empty();
    if generic && text.contains(',') {
        "A finite list of element or chemical group alternatives. Expand generic structures generates each combination."
    } else if mode == Mode::Text {
        "Text on a dummy atom; existing bonds stay connected."
    } else if generic && repeat_label(text.trim()) {
        "A repeat of one atom over the typed count range. Expand generic structures generates each count."
    } else if crate::ligands::LABELS.contains(&text.trim()) {
        "A real Cp (C5H5−) or Cp* (C10H15−) ligand with a five-center attachment. Metal charge stays as entered."
    } else if mode == Mode::Group || group_label(text.trim()) {
        "A real chemical group. Expand it later from Abbreviations. Requires a single-bond endpoint."
    } else if let Ok(Some(explicit)) = hydride::parse(text.trim()) {
        if explicit.charged_element {
            "An element with the typed charge. Hydrogens are not added; type the element alone to restore automatic hydrogens."
        } else {
            "An atom with the entered hydrogen count. Checking chemistry will not rewrite this label. Type the element alone to restore automatic hydrogens."
        }
    } else if ELEMENTS.contains(&text.trim()) || text.trim() == "*" {
        "An element symbol (or * for a dummy atom)."
    } else {
        "Text on a dummy atom; existing bonds stay connected."
    }
}

/// Automatic entry turns these labels into a chemical group, not one atom.
fn group_label(text: &str) -> bool {
    formula_group(text).is_some() || known_group(text) && !ELEMENTS.contains(&text)
}

/// The finite link-node spelling that Apply turns into a repeat of one atom
/// (`[CH2]1-3`). Apply rejects groups, ligands and invalid atom labels there.
fn repeat_label(text: &str) -> bool {
    let Ok(Some((label, range))) = crate::generic::split_link_label(text) else {
        return false;
    };
    let label = label.trim();
    matches!(
        crate::generic::RepeatCount::parse(range),
        Ok(crate::generic::RepeatCount::Finite { .. })
    ) && (1..=32).contains(&label.chars().count())
        && !group_label(label)
        && !crate::ligands::LABELS.contains(&label)
        && (ELEMENTS.contains(&label) || label == "*" || hydride::parse(label).is_ok())
}

/// Text in the charged element or hydride grammar (Pd2+, NH4-2) is one
/// charged atom, so generic labels never read its digits as a link range.
pub(crate) fn charged_label(text: &str) -> bool {
    hydride::parse(text.trim()).is_ok_and(|parsed| parsed.is_some_and(|h| h.charge != 0))
}

/// Returns a complete candidate; failures never partially modify the drawing.
pub fn apply(doc: &Document, id: u64, text: &str, mode: Mode) -> Result<Document, String> {
    apply_entry(doc, id, text, mode, true)
}

fn apply_entry(
    doc: &Document,
    id: u64,
    text: &str,
    mode: Mode,
    allow_generic: bool,
) -> Result<Document, String> {
    if text.chars().any(char::is_control) {
        return Err("Use a single line of printable text".into());
    }
    let text = text.trim();
    if allow_generic
        && mode != Mode::Text
        && let Some(candidate) = crate::generic::apply_label(doc, id, text)?
    {
        return Ok(candidate);
    }
    if text.is_empty() || text.chars().count() > 32 {
        return Err("Enter 1–32 characters for the atom label".into());
    }
    let atom = doc.atom(id).ok_or("The atom is no longer available")?;
    if mode != Mode::Text && crate::ligands::LABELS.contains(&text) {
        return crate::ligands::replace(doc, id, text);
    }
    if !atom.centroid.is_empty() {
        return Err(
            "This is a tracked attachment point. Edit the atom bonded to it instead.".into(),
        );
    }
    let element = ELEMENTS.contains(&text) || text == "*";
    let formula = (mode != Mode::Text).then(|| formula_group(text)).flatten();
    if mode == Mode::Group
        && !known_group(text)
        && formula.is_none()
        && let Some(group) = doc.abbreviation(id)
    {
        let mut result = doc.clone();
        let renamed = result
            .abbreviations
            .iter_mut()
            .find(|g| g.anchor == id)
            .ok_or("Missing group")?;
        if group.label != text {
            renamed.label = text.into();
            renamed.reverse_label.clear();
        }
        result.validate()?;
        return Ok(result);
    }
    if mode == Mode::Group
        || formula.is_some()
        || mode == Mode::Auto && !element && known_group(text)
    {
        let selected = doc
            .abbreviation(id)
            .map(|a| a.members.clone())
            .unwrap_or_else(|| vec![id]);
        if doc.abbreviation(id).is_some_and(|a| a.label == text) {
            return Ok(doc.clone());
        }
        if doc.generic.repeats.iter().any(|u| u.members.contains(&id)) {
            return Err(
                "Clear the repeating unit before replacing a member with a chemical group".into(),
            );
        }
        let mut clean = doc.clone();
        clean.generic.alternatives.retain(|s| s.atom != id);
        clean.generic.links.retain(|s| s.atom != id);
        if let Some(atom) = clean.atom_mut(id) {
            atom.display.set_variable(None);
        }
        let mut result = crate::chemistry::abbreviations::replace(
            &clean,
            &selected,
            formula.map(|f| f.0).unwrap_or(text),
        )
        .map_err(|e| format!("{e}. Choose Text label to use the text without a chemical group."))?;
        // The template is planar; retain the original anchor's position and depth.
        let anchor = result
            .atom(id)
            .ok_or("The abbreviation has no attachment atom")?;
        let shift = crate::document::Point::new(
            atom.position.x - anchor.position.x,
            atom.position.y - anchor.position.y,
        );
        let members = result
            .abbreviation(id)
            .ok_or("The abbreviation is unavailable")?
            .members
            .clone();
        for a in result.atoms.iter_mut().filter(|a| members.contains(&a.id)) {
            a.position = a.position.offset(shift.x, shift.y);
            a.depth = atom.depth;
        }
        if let Some((_, reverse)) = formula
            && let Some(group) = result.abbreviations.iter_mut().find(|g| g.anchor == id)
        {
            group.label = text.into();
            group.reverse_label = reverse.into();
        }
        result.invalidate_chemistry(&selected);
        result.validate()?;
        return Ok(result);
    }
    if doc.abbreviations.iter().any(|a| a.members.contains(&id)) {
        return Err(
            "Expand this abbreviation before replacing an individual atom with text or an element."
                .into(),
        );
    }
    let explicit = if mode != Mode::Text && !element {
        hydride::parse(text)?
    } else {
        None
    };
    let new_element = if let Some(h) = &explicit {
        h.element.as_str()
    } else if mode != Mode::Text && element {
        text
    } else {
        "*"
    };
    let variable =
        (new_element == "*" && (text != "*" || mode == Mode::Text)).then(|| text.to_owned());
    let mut result = doc.clone();
    result.generic.alternatives.retain(|s| s.atom != id);
    result.generic.links.retain(|s| s.atom != id);
    let a = result
        .atom_mut(id)
        .ok_or("The atom is no longer available")?;
    let previous = (a.element.clone(), a.charge, a.explicit_h, a.no_implicit);
    if a.element != new_element {
        a.element = new_element.into();
        a.charge = 0;
        a.isotope = 0;
        a.explicit_h = 0;
        a.label_h = 0;
        a.no_implicit = new_element == "*";
        a.radical_electrons = 0;
        a.stereo = None;
    }
    a.display.set_variable(variable);
    if let Some(h) = &explicit {
        a.explicit_h = h.hydrogens;
        a.no_implicit = true;
        a.label_h = h.hydrogens;
        a.charge = h.charge;
        // An entered charge is shown, even where a shortcut hid one.
        a.display.hide_charge = false;
    } else if mode != Mode::Text && element && new_element != "*" {
        a.explicit_h = 0;
        a.no_implicit = false;
    }
    if new_element == "*" {
        a.no_implicit = true;
        a.explicit_h = 0;
        a.label_h = 0;
    }
    if new_element == "C" {
        a.display.carbons = Some(crate::atom_labels::Carbons::All);
    }
    if previous != (a.element.clone(), a.charge, a.explicit_h, a.no_implicit) {
        result.invalidate_chemistry(&[id]);
    }
    result.validate()?;
    Ok(result)
}

/// Apply manually formatted input as one complete candidate. With no manual
/// runs, entry retains the ordinary element, group and generic-label grammar.
pub fn apply_with_scripts(
    doc: &Document,
    id: u64,
    text: &str,
    runs: &[ScriptRun],
    mode: Mode,
) -> Result<Document, String> {
    if mode == Mode::Group || runs.is_empty() {
        return apply(doc, id, text, mode);
    }
    if text.chars().any(char::is_control) {
        return Err("Use a single line of printable text".into());
    }
    // Store the trimmed label, as plain entry does, with the runs moved along.
    let lead = text.len() - text.trim_start().len();
    let text = text.trim();
    let runs: Vec<ScriptRun> = runs
        .iter()
        .map(|run| ScriptRun {
            start: run.start.clamp(lead, lead + text.len()) - lead,
            end: run.end.clamp(lead, lead + text.len()) - lead,
            script: run.script,
        })
        .collect();
    let runs = crate::typography::normalize_label_scripts(text, &runs)?;
    if runs.is_empty() {
        return apply(doc, id, text, mode);
    }
    if mode == Mode::Auto
        && let Some(chemical) = manual_chemical_entry(text, &runs)?
    {
        return apply_entry(doc, id, &chemical, Mode::Auto, false);
    }
    let mut result = apply(doc, id, text, Mode::Text)?;
    let atom = result
        .atom_mut(id)
        .ok_or("The atom is no longer available")?;
    atom.display.set_variable(Some(text.to_owned()));
    atom.display.label_scripts = runs;
    result.validate()?;
    Ok(result)
}

/// The chemical spelling of manually scripted input, when atom entry accepts it.
/// Caption previews use this same classification without applying an edit.
fn manual_chemical_entry(text: &str, runs: &[ScriptRun]) -> Result<Option<String>, String> {
    if let Some(chemical) = manual_chemical_text(text, runs) {
        let element = ELEMENTS.contains(&chemical.as_str()) || chemical == "*";
        let digit = |c: char| {
            c.is_ascii_digit()
                || ('₀'..='₉').contains(&c)
                || matches!(c, '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹')
        };
        let lowered = |run: &ScriptRun| {
            text.get(run.start..run.end)
                .filter(|_| run.script == Script::Subscript)
        };
        // Lowered counts in a known group (CO2Me) are its ordinary formula
        // look, so the label stays the real group rather than dummy text.
        if !element
            && (known_group(&chemical) || formula_group(&chemical).is_some())
            && runs
                .iter()
                .all(|run| lowered(run).is_some_and(|s| s.chars().all(digit)))
        {
            return Ok(Some(chemical));
        }
        // A manually lowered digit is a count, never an element charge.
        let lowered_count = runs
            .iter()
            .any(|run| lowered(run).is_some_and(|s| s.chars().any(digit)));
        let lowered_element_count = lowered_count
            && chemical
                .find(|c: char| !c.is_ascii_alphabetic())
                .and_then(|end| chemical.get(..end))
                .is_some_and(|symbol| symbol != "H" && ELEMENTS.contains(&symbol));
        let explicit = if element || lowered_element_count {
            None
        } else {
            hydride::parse(&chemical)?
        };
        if element || explicit.is_some() {
            return Ok(Some(chemical));
        }
    }
    Ok(None)
}

/// A trailing manual superscript distinguishes a charge from an H count:
/// NH with lowered 4 and raised 2+ enters the unambiguous grammar NH4+2.
fn manual_chemical_text(text: &str, runs: &[ScriptRun]) -> Option<String> {
    let Some(charge) = runs
        .last()
        .filter(|run| run.script == Script::Superscript && run.end == text.len())
    else {
        return (!runs.iter().any(|run| run.script == Script::Superscript))
            .then(|| text.trim().to_owned());
    };
    if runs
        .iter()
        .any(|run| run.script == Script::Superscript && run.start != charge.start)
    {
        return None;
    }
    let suffix: String = text
        .get(charge.start..charge.end)?
        .trim()
        .chars()
        .map(|c| match c {
            '₀' | '⁰' => '0',
            '₁' | '¹' => '1',
            '₂' | '²' => '2',
            '₃' | '³' => '3',
            '₄' | '⁴' => '4',
            '₅' | '⁵' => '5',
            '₆' | '⁶' => '6',
            '₇' | '⁷' => '7',
            '₈' | '⁸' => '8',
            '₉' | '⁹' => '9',
            '⁺' => '+',
            '⁻' | '−' => '-',
            other => other,
        })
        .collect();
    let (sign, count) = if let Some(count) = suffix.strip_prefix('+') {
        ('+', count)
    } else if let Some(count) = suffix.strip_prefix('-') {
        ('-', count)
    } else if let Some(count) = suffix.strip_suffix('+') {
        ('+', count)
    } else {
        ('-', suffix.strip_suffix('-')?)
    };
    if !count.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let body = text.get(..charge.start)?.trim_end();
    let count = if count.is_empty() { "1" } else { count };
    Some(format!("{body}{sign}{count}"))
}

#[cfg(test)]
mod tests;
