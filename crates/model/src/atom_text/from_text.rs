//! Convert an ordinary caption into a complete atom-entry candidate.
use super::{Mode, apply_with_scripts, hydride, manual_chemical_entry};
use crate::{
    document::{Annotation, Document, Point},
    style,
    typography::{self, Script, ScriptRun},
};

pub const TEXT_TOO_LONG: &str =
    "Only a single line of up to 32 characters can become an atom label";
const MOLECULE_NAME_TEXT: &str = "This text is a molecule name; it stays a caption";
const REACTION_TEXT: &str = "This text belongs to a reaction; it stays a caption";

#[cfg(test)]
thread_local! {
    static CONVERSION_ENTRIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Visible world-space ink, using the same fragments as the annotation scene.
pub fn text_ink_bounds(annotation: &Annotation) -> Option<(Point, Point)> {
    ink_bounds(
        typography::layout(&annotation.text, &annotation.format)
            .fragments
            .into_iter()
            .flat_map(|fragment| {
                let origin = annotation
                    .position
                    .offset(fragment.position.x, fragment.position.y);
                style::text_ink_boxes(&fragment.text, fragment.style.size(), &fragment.style)
                    .into_iter()
                    .map(move |(lo, hi)| (origin.offset(lo.x, lo.y), origin.offset(hi.x, hi.y)))
            }),
    )
}

/// The visible center of the first non-whitespace character.
pub fn text_first_glyph_center(annotation: &Annotation) -> Option<Point> {
    for fragment in typography::layout(&annotation.text, &annotation.format).fragments {
        let Some((byte, character)) = fragment
            .text
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
        else {
            continue;
        };
        let size = fragment.style.size();
        let advance = style::styled_text_width(fragment.text.get(..byte)?, size, &fragment.style);
        let (lo, hi) = ink_bounds(style::text_ink_boxes(
            &character.to_string(),
            size,
            &fragment.style,
        ))?;
        return Some(annotation.position.offset(
            fragment.position.x + advance + (lo.x + hi.x) / 2.,
            fragment.position.y + (lo.y + hi.y) / 2.,
        ));
    }
    None
}

/// Where `text_to_atom` puts the new atom, for bond previews and targets.
pub fn text_atom_position(doc: &Document, id: u64) -> Option<Point> {
    text_atom_preview(doc, id)?
        .atoms
        .first()
        .map(|atom| atom.position)
}

/// A caption-sized drawing of the converted label, with one anchor atom.
/// Abbreviations retain their label metadata but never expand their chemistry.
/// Atom IDs and group membership are local to this temporary scene drawing.
/// This borrows the source drawing: it neither clones nor validates its graph.
/// The actual edit, including full admission validation, happens in `text_to_atom`.
fn text_atom_preview(doc: &Document, id: u64) -> Option<Document> {
    if text_atom_refusal(doc, id).is_some() {
        return None;
    }
    let annotation = doc
        .annotations
        .iter()
        .find(|annotation| annotation.id == id)?;
    let target = text_first_glyph_center(annotation)?;
    let text = annotation.text.trim();
    let lead = annotation.text.len() - annotation.text.trim_start().len();
    let runs: Vec<ScriptRun> = manual_runs(annotation)
        .into_iter()
        .map(|run| ScriptRun {
            start: run.start.clamp(lead, lead + text.len()) - lead,
            end: run.end.clamp(lead, lead + text.len()) - lead,
            script: run.script,
        })
        .collect();
    let runs = typography::normalize_label_scripts(text, &runs).ok()?;
    let mut preview = Document {
        drawing_style: doc.drawing_style.clone(),
        atom_labels: doc.atom_labels.clone(),
        ..Document::default()
    };
    let atom = preview.add_atom("*", target);
    let chemical = if runs.is_empty() {
        None
    } else {
        manual_chemical_entry(text, &runs).ok().flatten()
    };
    let populated = if let Some(chemical) = chemical {
        preview_entry(&mut preview, atom, &chemical, false)
    } else if runs.is_empty() {
        preview_entry(&mut preview, atom, text, true)
    } else {
        Err(())
    };
    if populated.is_err() {
        // Automatic entry falls back to a manually scripted text label.
        preview.abbreviations.clear();
        preview.generic = Default::default();
        preview.atoms.clear();
        preview.add_atom("*", target);
        let label = preview.atom_mut(atom)?;
        label.display.set_variable(Some(text.to_owned()));
        label.display.label_scripts = runs;
    }
    preview.atom_mut(atom)?.text_style = Some(first_glyph_style(annotation)?);
    let shift = first_glyph_shift(&preview, atom, target);
    let label = preview.atom_mut(atom)?;
    label.position = label.position.offset(shift.x, shift.y);
    Some(preview)
}

/// Materialize only the fields that determine atom-label ink. All parsing is
/// bounded by the admitted caption, and uses the ordinary entry parsers.
fn preview_entry(doc: &mut Document, atom: u64, text: &str, allow_generic: bool) -> Result<(), ()> {
    let text = text.trim();
    if text.is_empty() {
        return Err(());
    }
    if allow_generic && !super::catalog_text(text) {
        if let Some((choices, excluded)) = crate::generic::alternative(text).map_err(|_| ())? {
            if choices.iter().enumerate().any(|(index, choice)| {
                choices
                    .iter()
                    .take(index)
                    .any(|previous| previous == choice)
            }) {
                return Err(());
            }
            doc.atom_mut(atom)
                .ok_or(())?
                .display
                .set_variable(Some(text.to_owned()));
            doc.generic
                .alternatives
                .push(crate::generic::AlternativeSite {
                    atom,
                    choices,
                    excluded,
                });
            return Ok(());
        }
        if crate::generic::split_link_label(text)
            .map_err(|_| ())?
            .is_some()
        {
            // Conversion adds an isolated atom. Link entry requires two bonds,
            // so its ordinary candidate always falls back to a text label.
            return Err(());
        }
    }
    let element = crate::editing::ELEMENTS.contains(&text) || text == "*";
    if crate::ligands::LABELS.contains(&text)
        || super::formula_group(text).is_some()
        || !element && super::known_group(text) && !super::wrong_side(doc, atom, text)
    {
        let (label, reverse_label) = preview_group_names(text).ok_or(())?;
        doc.abbreviations.push(crate::abbreviations::Abbreviation {
            label_style: None,
            label_color_override: false,
            highlight: None,
            label,
            reverse_label,
            anchor: atom,
            members: vec![atom],
            alignment: Default::default(),
        });
        return Ok(());
    }
    let explicit = if element {
        None
    } else {
        hydride::parse(text).map_err(|_| ())?
    };
    let label = doc.atom_mut(atom).ok_or(())?;
    if let Some(explicit) = explicit {
        label.element = explicit.element;
        label.explicit_h = explicit.hydrogens;
        label.charge = explicit.charge;
        label.no_implicit = true;
    } else if element {
        label.element = text.to_owned();
        label.no_implicit = text == "*";
    } else {
        label.display.set_variable(Some(text.to_owned()));
    }
    if label.element == "C" {
        label.display.carbons = Some(crate::atom_labels::Carbons::All);
    }
    Ok(())
}

/// The stored label and reverse label of a typed group, as Apply makes them.
fn preview_group_names(text: &str) -> Option<(String, String)> {
    use crate::chemistry::abbreviations::Spelling;
    let typed = |reverse: &str| Some((text.to_owned(), reverse.to_owned()));
    if let Some((_, reverse)) = super::formula_group(text) {
        return typed(reverse);
    }
    match text {
        "Cp" | "Cp*" => typed(text),
        "MgBr" => typed("BrMg"),
        "N3" => typed(""),
        _ => crate::chemistry::abbreviations::lookup(text)
            .ok()
            .flatten()
            .map(|(preset, spelling)| match spelling {
                Spelling::Label => (text.to_owned(), preset.reverse_label.clone()),
                Spelling::Alias => (text.to_owned(), String::new()),
                Spelling::Reverse => (preset.label.clone(), preset.reverse_label.clone()),
            }),
    }
}

/// Cheap admission check for menus and pointer hints; no entry or layout work.
pub fn text_atom_refusal(doc: &Document, id: u64) -> Option<String> {
    let Some(annotation) = doc
        .annotations
        .iter()
        .find(|annotation| annotation.id == id)
    else {
        return Some("Select a text annotation to convert to an atom".into());
    };
    if crate::molecule_names::is_caption(doc, id) {
        return Some(MOLECULE_NAME_TEXT.into());
    }
    if crate::reactions::includes_text(doc, id) {
        return Some(REACTION_TEXT.into());
    }
    let text = annotation.text.trim();
    if text.is_empty()
        || annotation.text.chars().take(33).count() > 32
        || annotation
            .text
            .chars()
            .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
    {
        return Some(TEXT_TOO_LONG.into());
    }
    None
}

/// Return a complete candidate and its new atom ID; errors leave `doc` intact.
pub fn text_to_atom(doc: &Document, id: u64) -> Result<(Document, u64), String> {
    #[cfg(test)]
    CONVERSION_ENTRIES.with(|entries| entries.set(entries.get() + 1));
    if let Some(reason) = text_atom_refusal(doc, id) {
        return Err(reason);
    }
    let annotation = doc
        .annotations
        .iter()
        .find(|annotation| annotation.id == id)
        .ok_or("Select a text annotation to convert to an atom")?;
    let position = text_first_glyph_center(annotation)
        .ok_or("This text has no visible character to anchor an atom")?;
    let text_style = first_glyph_style(annotation).ok_or(TEXT_TOO_LONG)?;
    let runs = manual_runs(annotation);
    let mut candidate = doc.clone();
    let atom = candidate.add_atom("*", position);
    let mut candidate = apply_with_scripts(&candidate, atom, &annotation.text, &runs, Mode::Auto)
        .or_else(|_| apply_with_scripts(&candidate, atom, &annotation.text, &runs, Mode::Text))
        .map_err(|_| TEXT_TOO_LONG.to_owned())?;
    candidate
        .atom_mut(atom)
        .ok_or("The atom is no longer available")?
        .text_style = Some(text_style);
    align_first_glyph(doc, &mut candidate, atom, position);
    candidate.delete(&[id]);
    candidate.validate()?;
    Ok((candidate, atom))
}

/// The scene centers the whole main symbol on the atom (all of "Pd", not "P").
/// Move the new atom, and any group atoms the entry added, so the label's
/// first glyph lands on the caption's first glyph and the text does not jump.
fn align_first_glyph(doc: &Document, candidate: &mut Document, atom: u64, target: Point) {
    let shift = first_glyph_shift(candidate, atom, target);
    let existing: std::collections::HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    for added in candidate
        .atoms
        .iter_mut()
        .filter(|a| !existing.contains(&a.id))
    {
        added.position = added.position.offset(shift.x, shift.y);
    }
}

fn first_glyph_shift(doc: &Document, atom: u64, target: Point) -> Point {
    doc.atom(atom)
        .and_then(|atom| {
            crate::scene::atom_label_ink_boxes(atom, doc)
                .into_iter()
                .next()
        })
        .map_or_else(Point::default, |(lo, hi)| {
            Point::new(target.x - (lo.x + hi.x) / 2., target.y - (lo.y + hi.y) / 2.)
        })
}

fn first_glyph_style(annotation: &Annotation) -> Option<typography::TextStyle> {
    let first = annotation
        .text
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map(|(byte, _)| byte)?;
    let mut style = annotation.format.at(first).clone();
    style.script = Script::Normal;
    style.formula = false;
    Some(style)
}

fn manual_runs(annotation: &Annotation) -> Vec<ScriptRun> {
    let mut runs: Vec<ScriptRun> = vec![];
    for (start, character) in annotation.text.char_indices() {
        // Formula layout is automatic; only explicit scripts become manual runs.
        let script = annotation.format.at(start).script;
        if script == Script::Normal {
            continue;
        }
        let end = start + character.len_utf8();
        if let Some(last) = runs.last_mut()
            && last.end == start
            && last.script == script
        {
            last.end = end;
        } else {
            runs.push(ScriptRun { start, end, script });
        }
    }
    runs
}

fn ink_bounds(boxes: impl IntoIterator<Item = (Point, Point)>) -> Option<(Point, Point)> {
    boxes.into_iter().reduce(|(lo, hi), (a, b)| {
        (
            Point::new(lo.x.min(a.x), lo.y.min(a.y)),
            Point::new(hi.x.max(b.x), hi.y.max(b.y)),
        )
    })
}

#[cfg(test)]
mod tests;
