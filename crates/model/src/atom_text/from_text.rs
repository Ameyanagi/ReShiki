//! Convert an ordinary caption into a complete atom-entry candidate.
use super::{Mode, apply_with_scripts};
use crate::{
    document::{Annotation, Document, Point},
    style,
    typography::{self, Script, ScriptRun},
};

pub const TEXT_TOO_LONG: &str =
    "Only a single line of up to 32 characters can become an atom label";
const MOLECULE_NAME_TEXT: &str = "This text is a molecule name; it stays a caption";
const REACTION_TEXT: &str = "This text belongs to a reaction; it stays a caption";

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
    let (converted, atom) = text_to_atom(doc, id).ok()?;
    converted.atom(atom).map(|atom| atom.position)
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
        || annotation.text.chars().count() > 32
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
    let first = annotation
        .text
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map(|(byte, _)| byte)
        .ok_or(TEXT_TOO_LONG)?;
    let mut text_style = annotation.format.at(first).clone();
    text_style.script = Script::Normal;
    text_style.formula = false;
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
    let Some((lo, hi)) = candidate.atom(atom).and_then(|a| {
        crate::scene::atom_label_ink_boxes(a, candidate)
            .into_iter()
            .next()
    }) else {
        return;
    };
    let dx = target.x - (lo.x + hi.x) / 2.;
    let dy = target.y - (lo.y + hi.y) / 2.;
    let existing: std::collections::HashSet<u64> = doc.atoms.iter().map(|a| a.id).collect();
    for added in candidate
        .atoms
        .iter_mut()
        .filter(|a| !existing.contains(&a.id))
    {
        added.position = added.position.offset(dx, dy);
    }
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
