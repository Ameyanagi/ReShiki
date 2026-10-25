//! Decisions made when a chemical label is placed, using its actual glyph ink.
use super::*;
use crate::abbreviations::LabelAlignment;

/// Change the bond-based side only when it collides and the other side is clear.
/// This is an edit-time decision; rendering and hit tests never recursively
/// choose new directions for neighboring groups.
pub(crate) fn clearer_orientation(doc: &Document, id: u64) -> Option<LabelAlignment> {
    let atom = doc.atom(id)?;
    let group = doc.abbreviation(id)?;
    if group.reverse_label.trim().is_empty() {
        return None;
    }
    let mut forward = group.clone();
    forward.alignment = LabelAlignment::Left;
    let mut reverse = group.clone();
    reverse.alignment = LabelAlignment::Right;
    let forward = abbreviation_label_runs(atom, doc, &forward);
    let reverse = abbreviation_label_runs(atom, doc, &reverse);
    let (lo, hi) = label_ink_boxes(&forward)
        .into_iter()
        .chain(label_ink_boxes(&reverse))
        .reduce(|(lo, hi), (a, b)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        })?;
    let reach = LabelReach::new(doc);
    let mut nearby = Vec::new();
    for other in &doc.atoms {
        if other.id == id || !doc.atom_visible(other.id) || crate::models3d::hides(doc, other.id) {
            continue;
        }
        let limit = reach.of(other)?;
        if other.position.x + limit < lo.x
            || other.position.x - limit > hi.x
            || other.position.y + limit < lo.y
            || other.position.y - limit > hi.y
        {
            continue;
        }
        // Keep crowded or very large drawings responsive. An incomplete
        // obstacle set must never justify changing the established side.
        if nearby.len() == 64 {
            return None;
        }
        nearby.push(other);
    }
    let obstacles = measuring(|| {
        nearby
            .into_iter()
            .flat_map(|other| label_ink_boxes(&atom_label_runs(other, doc)))
            .collect::<Vec<_>>()
    })?;
    let bonds = unrelated_bonds(atom, doc);
    let size = STYLE.world(group.text_style(doc).size_pt);
    let (preferred, alternative, alignment) = if group.faces_left(doc) {
        (&reverse, &forward, LabelAlignment::Left)
    } else {
        (&forward, &reverse, LabelAlignment::Right)
    };
    (!clear_of(preferred, &obstacles, &bonds, size)
        && clear_of(alternative, &obstacles, &bonds, size))
    .then_some(alignment)
}

/// Only the displayed attachment element gets core-glyph priority. Nicknames
/// such as Ac, hydrogens and charges still belong to their complete label.
pub(super) fn element_glyph_hit(
    atom: &Atom,
    doc: &Document,
    runs: &[Primitive],
    point: Point,
) -> bool {
    let (content, range) = if let Some(group) = doc.abbreviation(atom.id) {
        let content = group.text(doc);
        (
            content,
            crate::abbreviations::anchor_range(content, group.faces_left(doc)),
        )
    } else if atom.display.variable.is_none() {
        (atom.element.as_str(), 0..atom.element.len())
    } else {
        return false;
    };
    if content.get(range.clone()) != Some(atom.element.as_str()) {
        return false;
    }
    let start = content[..range.start].chars().count();
    let end = start + atom.element.chars().count();
    let mut index = 0;
    let mut picked = String::new();
    let mut hit = false;
    for run in runs {
        let Primitive::Text {
            position,
            text,
            size,
            style,
            ..
        } = run
        else {
            continue;
        };
        let mut x = position.x;
        for character in text.chars() {
            if (start..end).contains(&index) {
                picked.push(character);
                hit |= crate::style::text_ink_boxes(&character.to_string(), *size, style)
                    .into_iter()
                    .any(|(lo, hi)| {
                        point.x >= x + lo.x
                            && point.x <= x + hi.x
                            && point.y >= position.y + lo.y
                            && point.y <= position.y + hi.y
                    });
            }
            x += crate::style::glyph_metrics(character, style).1 * size;
            index += 1;
        }
    }
    hit && picked == atom.element
}
