//! Compound numbers are ordinary bold text, linked to atom instances rather
//! than chemical identity. Text edits pin a number; Renumber only changes the
//! remaining automatic numbers. The optional native links flatten harmlessly
//! to visible annotations in readers that do not know them.
use crate::{
    document::{Annotation, Document, Point},
    typography::{Script, TextAlign, TextFormat},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompoundNumber {
    pub annotation: u64,
    pub atoms: Vec<u64>,
    /// Last written text, to recognize ordinary text editing.
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
}

pub fn label<'a>(doc: &'a Document, atoms: &[u64]) -> Option<&'a CompoundNumber> {
    doc.compound_numbers
        .iter()
        .find(|n| n.atoms.len() == atoms.len() && n.atoms.iter().all(|id| atoms.contains(id)))
}

/// Below the molecule and its linked chemical name/property caption. Manual
/// offsets remain offsets from this anchor, including when another caption grows.
fn anchor(doc: &Document, atoms: &[u64]) -> Option<Point> {
    let (lo, hi) = crate::scene::selection_bounds(doc, atoms)?;
    let mut bottom = hi.y;
    let captions = doc
        .molecule_names
        .iter()
        .map(|n| (n.annotation, &n.atoms))
        .chain(
            doc.property_captions
                .iter()
                .filter_map(|n| Some((n.annotation?, &n.atoms))),
        );
    for (id, members) in captions {
        if members.iter().any(|id| atoms.contains(id))
            && let Some((a, b)) = crate::scene::selection_bounds(doc, &[id])
            && a.x <= hi.x
            && b.x >= lo.x
        {
            bottom = bottom.max(b.y);
        }
    }
    Some(Point::new((lo.x + hi.x) / 2., bottom))
}

/// Add a number to one whole connected component. Existing numbers are not
/// duplicated; adding a new number also puts automatic numbers in reading order.
pub fn number(doc: &mut Document, atoms: &[u64], mut format: TextFormat) -> Result<(), String> {
    let whole = atoms
        .first()
        .map_or(vec![], |id| crate::molecule_names::component(doc, *id));
    if whole.is_empty() || whole.len() != atoms.len() || atoms.iter().any(|id| !whole.contains(id))
    {
        return Err("Select one whole molecule to number it".into());
    }
    if label(doc, &whole).is_some() {
        return Ok(());
    }
    let at = anchor(doc, &whole).ok_or("The molecule has no bounds")?;
    format.spans.clear();
    format.style.bold = true;
    format.style.formula = false;
    format.style.script = Script::Normal;
    format.style.size_pt = format.style.size_pt.max(doc.drawing_style.font_size_pt);
    format.alignment = TextAlign::Center;
    format.width_pt = None;
    format.name_breaks = false;
    let mut caption = Annotation {
        id: doc.next_id(),
        position: at.offset(0., doc.drawing_style.font_size().max(12.) * 0.5),
        text: "1".into(),
        format,
    };
    caption.position.x -= caption.size().0 / 2.;
    doc.compound_numbers.push(CompoundNumber {
        annotation: caption.id,
        atoms: whole,
        text: caption.text.clone(),
        fixed: false,
    });
    doc.annotations.push(caption);
    renumber(doc);
    Ok(())
}

fn pin_edits(doc: &mut Document) {
    for link in &mut doc.compound_numbers {
        if let Some(a) = doc.annotations.iter().find(|a| a.id == link.annotation)
            && a.text != link.text
        {
            link.fixed = true;
            link.text.clone_from(&a.text);
        }
    }
}

/// A typed suffix such as 2a reserves the number family 2. Arbitrary labels
/// stay fixed too, without guessing that unrelated plain text is a number.
fn family(text: &str) -> Option<u64> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse::<u64>().ok().filter(|n| *n > 0)
}

fn write(doc: &mut Document, index: usize, value: u64) {
    let Some(link) = doc.compound_numbers.get_mut(index) else {
        return;
    };
    let text = value.to_string();
    if let Some(a) = doc.annotations.iter_mut().find(|a| a.id == link.annotation) {
        let width = a.size().0;
        a.format.edited(&a.text, &text, 0..a.text.len());
        a.text.clone_from(&text);
        a.position.x += (width - a.size().0) / 2.;
        link.text = text;
    }
}

fn next_free(used: &HashSet<u64>) -> u64 {
    let mut next = 1;
    while used.contains(&next) {
        next += 1;
    }
    next
}

/// Stable reading rows: top to bottom, then left to right. A row has a common
/// vertical band, so a tall molecule cannot chain two otherwise separate rows.
fn reading_order(doc: &Document) -> Vec<usize> {
    let tolerance = doc.drawing_style.bond_length_world * 0.25;
    let mut items: Vec<_> = doc
        .compound_numbers
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let (lo, hi) = crate::scene::selection_bounds(doc, &n.atoms)?;
            Some((i, lo, hi, n.atoms.iter().copied().min().unwrap_or(0)))
        })
        .collect();
    items.sort_by(|a, b| {
        a.1.y
            .total_cmp(&b.1.y)
            .then(a.1.x.total_cmp(&b.1.x))
            .then(a.3.cmp(&b.3))
    });
    let mut rows: Vec<(f32, f32, Vec<_>)> = vec![];
    for item in items {
        let lo = item.1.y - tolerance;
        let hi = item.2.y + tolerance;
        if let Some(row) = rows.iter_mut().find(|row| lo <= row.1 && hi >= row.0) {
            row.0 = row.0.max(lo);
            row.1 = row.1.min(hi);
            row.2.push(item);
        } else {
            rows.push((lo, hi, vec![item]));
        }
    }
    rows.into_iter()
        .flat_map(|(_, _, mut row)| {
            row.sort_by(|a, b| {
                a.1.x
                    .total_cmp(&b.1.x)
                    .then(a.1.y.total_cmp(&b.1.y))
                    .then(a.3.cmp(&b.3))
            });
            row.into_iter().map(|item| item.0)
        })
        .collect()
}

/// Renumber all managed automatic labels in one caller-owned transaction.
/// Moving molecules does not change their numbers until this is requested.
pub fn renumber(doc: &mut Document) {
    pin_edits(doc);
    let mut used: HashSet<_> = doc
        .compound_numbers
        .iter()
        .filter(|n| n.fixed)
        .filter_map(|n| family(&n.text))
        .collect();
    for index in reading_order(doc) {
        if doc.compound_numbers.get(index).is_some_and(|n| !n.fixed) {
            let value = next_free(&used);
            used.insert(value);
            write(doc, index, value);
        }
    }
}

/// Follow atom-only canvas previews without text edits, pinning, renumbering
/// or history work. Captions explicitly transformed with the atoms are already
/// in place. Related captions lay out first, as in the committed transaction.
pub fn preview_layout(doc: &mut Document, before: &Document) {
    if !doc.molecule_names.is_empty() {
        let moved: Vec<_> = doc
            .compound_numbers
            .iter()
            .filter(|link| {
                link.atoms.iter().any(|id| {
                    let position = |doc: &Document| doc.atom(*id).map(|a| (a.position, a.depth));
                    position(doc) != position(before)
                })
            })
            .flat_map(|link| link.atoms.iter().copied())
            .collect();
        crate::molecule_names::preview_layout(doc, &moved);
    }
    crate::linked_properties::preview_layout(doc, before);
    for link in &doc.compound_numbers {
        let Some(old) = before
            .compound_numbers
            .iter()
            .find(|n| n.annotation == link.annotation)
        else {
            continue;
        };
        let Some(caption) = doc.annotations.iter().find(|a| a.id == link.annotation) else {
            continue;
        };
        if !before
            .annotations
            .iter()
            .any(|a| a.id == caption.id && a.position == caption.position)
        {
            continue;
        }
        if let Some((from, to)) = anchor(before, &old.atoms).zip(anchor(doc, &link.atoms))
            && let Some(a) = doc.annotations.iter_mut().find(|a| a.id == link.annotation)
        {
            a.position = a.position.offset(to.x - from.x, to.y - from.y);
        }
    }
}

/// Keep existing automatic values where possible. Inserting a copied molecule
/// gives its automatic label a fresh value, while fixed copies remain fixed.
fn unique(doc: &mut Document, before: &Document) {
    let mut used: HashSet<_> = doc
        .compound_numbers
        .iter()
        .filter(|n| n.fixed)
        .filter_map(|n| family(&n.text))
        .collect();
    let mut indices: Vec<_> = (0..doc.compound_numbers.len()).collect();
    indices.sort_by_key(|i| {
        doc.compound_numbers.get(*i).map(|n| {
            (
                !before
                    .compound_numbers
                    .iter()
                    .any(|old| old.annotation == n.annotation),
                n.annotation,
            )
        })
    });
    let mut pending = vec![];
    for index in indices {
        let Some(n) = doc.compound_numbers.get(index).filter(|n| !n.fixed) else {
            continue;
        };
        if n.text
            .parse::<u64>()
            .ok()
            .filter(|v| *v > 0)
            .is_none_or(|v| !used.insert(v))
        {
            pending.push(index);
        }
    }
    for index in pending {
        let value = next_free(&used);
        used.insert(value);
        write(doc, index, value);
    }
}

/// Follow atom instances through moves and edits. A split follows its largest
/// surviving component (lowest atom ID breaks a tie); a join keeps a fixed
/// label in preference to an automatic one, then the lowest annotation ID.
pub fn reconcile(doc: &mut Document, before: &Document) {
    if doc.compound_numbers.is_empty() {
        return;
    }
    pin_edits(doc);
    let mut links = std::mem::take(&mut doc.compound_numbers);
    links.sort_by_key(|n| (!n.fixed, n.annotation));
    let mut claimed = HashSet::new();
    for mut link in links {
        let Some(caption) = doc.annotations.iter().find(|a| a.id == link.annotation) else {
            continue;
        };
        let mut seen = HashSet::new();
        let mut pieces = vec![];
        for id in &link.atoms {
            if seen.insert(*id) {
                let piece = crate::molecule_names::component(doc, *id);
                seen.extend(piece.iter().copied());
                if !piece.is_empty() {
                    pieces.push(piece);
                }
            }
        }
        pieces.sort_by_key(|piece| (std::cmp::Reverse(piece.len()), piece.iter().copied().min()));
        let Some(whole) = pieces
            .into_iter()
            .next()
            .filter(|p| p.iter().all(|id| !claimed.contains(id)))
        else {
            doc.delete(&[link.annotation]);
            continue;
        };
        let shift = before
            .compound_numbers
            .iter()
            .find(|old| old.annotation == link.annotation)
            .filter(|_| {
                before.annotations.iter().any(|a| {
                    a.id == caption.id
                        && (a.position == caption.position
                            || !link.fixed
                                && a.text != caption.text
                                && a.position.y == caption.position.y
                                && (a.position.x + a.size().0 / 2.
                                    - caption.position.x
                                    - caption.size().0 / 2.)
                                    .abs()
                                    < 0.001)
                })
            })
            .and_then(|old| anchor(before, &old.atoms))
            .zip(anchor(doc, &whole));
        if let Some((from, to)) = shift
            && let Some(a) = doc.annotations.iter_mut().find(|a| a.id == link.annotation)
        {
            a.position = a.position.offset(to.x - from.x, to.y - from.y);
        }
        claimed.extend(whole.iter().copied());
        link.atoms = whole;
        doc.compound_numbers.push(link);
    }
    unique(doc, before);
}

pub fn prune(doc: &mut Document) {
    let mut removed = HashSet::new();
    let atoms: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    doc.compound_numbers.retain_mut(|n| {
        n.atoms.retain(|id| atoms.contains(id));
        if n.atoms.is_empty() {
            removed.insert(n.annotation);
        }
        !n.atoms.is_empty() && doc.annotations.iter().any(|a| a.id == n.annotation)
    });
    doc.annotations.retain(|a| !removed.contains(&a.id));
}

pub fn validate(doc: &Document) -> Result<(), String> {
    let mut captions = HashSet::new();
    let mut atoms = HashSet::new();
    let mut text = 0usize;
    for n in &doc.compound_numbers {
        text = text.saturating_add(n.text.len());
        if !captions.insert(n.annotation)
            || !doc.annotations.iter().any(|a| a.id == n.annotation)
            || doc
                .molecule_names
                .iter()
                .any(|l| l.annotation == n.annotation)
            || doc
                .property_captions
                .iter()
                .any(|l| l.annotation == Some(n.annotation))
            || doc
                .arrow_labels
                .iter()
                .any(|l| l.annotation == n.annotation)
            || n.atoms.is_empty()
            || n.atoms
                .iter()
                .any(|id| !atoms.insert(*id) || doc.atom(*id).is_none())
            || n.text.len() > crate::document::MAX_ANNOTATION_TEXT
            || text > crate::document::MAX_DRAWING_TEXT
            || !n.fixed
                && n.text
                    .parse::<u64>()
                    .ok()
                    .is_none_or(|v| v == 0 || v.to_string() != n.text)
        {
            return Err("Invalid compound number link".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
