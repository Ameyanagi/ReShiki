use super::{
    Alternative, RepeatCount,
    limits::{
        MAX_ALTERNATIVES, MAX_DEFINITIONS, MAX_LABEL_CHARS, MAX_REPEAT_COUNT, MAX_REPEAT_MEMBERS,
    },
};
use crate::document::Document;
use std::collections::{HashMap, HashSet};

pub(super) fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
pub(super) fn check_range(min: u32, max: u32) -> Result<(), String> {
    if min == 0 || max < min || max > MAX_REPEAT_COUNT {
        Err("Repeat counts must be an ordered range from 1 to 1000".into())
    } else {
        Ok(())
    }
}
pub(super) fn printable(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= MAX_LABEL_CHARS
        && !text.chars().any(char::is_control)
}
fn degrees(doc: &Document) -> HashMap<u64, usize> {
    let mut degrees = HashMap::new();
    for b in &doc.bonds {
        *degrees.entry(b.a).or_default() += 1;
        *degrees.entry(b.b).or_default() += 1;
    }
    degrees
}
pub fn validate(doc: &Document) -> Result<(), String> {
    if doc.generic.alternatives.len() + doc.generic.links.len() + doc.generic.repeats.len()
        > MAX_DEFINITIONS
    {
        return Err("Too many generic definitions".into());
    }
    let atoms: HashMap<_, _> = doc.atoms.iter().map(|a| (a.id, a)).collect();
    let graphics: HashMap<_, _> = doc.graphics.iter().map(|g| (g.id, g)).collect();
    let degrees = degrees(doc);
    let mut sites = HashSet::new();
    for s in &doc.generic.alternatives {
        let a = atoms.get(&s.atom).ok_or("Missing alternative atom")?;
        if !sites.insert(s.atom)
            || a.element != "*"
            || a.attachment.is_some()
            || !a.centroid.is_empty()
            || s.choices.is_empty()
            || s.choices.len() > MAX_ALTERNATIVES
        {
            return Err("Invalid alternative atom definition".into());
        }
        let mut unique = HashSet::new();
        for choice in &s.choices {
            let key = match choice {
                Alternative::Element { symbol, isotope } => {
                    if !crate::editing::ELEMENTS.contains(&symbol.as_str())
                        || *isotope > u16::MAX.into()
                    {
                        return Err("Invalid alternative element".into());
                    }
                    format!("{symbol}:{isotope}")
                }
                Alternative::Group { label } | Alternative::Unresolved { label } => {
                    if !printable(label) {
                        return Err("Invalid alternative label".into());
                    }
                    label.clone()
                }
            };
            if !unique.insert(key) {
                return Err("Duplicate alternative choice".into());
            }
        }
    }
    for link in &doc.generic.links {
        check_range(link.min, link.max)?;
        let a = atoms.get(&link.atom).ok_or("Missing link atom")?;
        if !sites.insert(link.atom)
            || a.attachment.is_some()
            || !a.centroid.is_empty()
            || degrees.get(&link.atom) != Some(&2)
        {
            return Err("A link node requires exactly two connecting bonds".into());
        }
    }
    let mut members = 0usize;
    for unit in &doc.generic.repeats {
        members = members.saturating_add(unit.members.len() + unit.crossings.len());
        let set: HashSet<_> = unit.members.iter().copied().collect();
        if members > MAX_REPEAT_MEMBERS
            || set.is_empty()
            || set.len() != unit.members.len()
            || set.iter().any(|id| !atoms.contains_key(id))
            || !printable(&unit.usage)
            || !printable(&unit.label)
        {
            return Err("Invalid repeating unit".into());
        }
        if let Some(id) = unit.bracket
            && graphics.get(&id).is_none_or(|g| !g.kind.brackets())
        {
            return Err("Missing repeating-unit bracket".into());
        }
        if unit
            .bracket_objects
            .iter()
            .any(|id| graphics.get(id).is_none_or(|g| !g.kind.brackets()))
        {
            return Err("Missing additional repeating-unit bracket".into());
        }
        if !unit.crossing_brackets.is_empty()
            && (unit.crossing_brackets.len() != unit.crossings.len()
                || unit
                    .crossing_brackets
                    .iter()
                    .any(|id| unit.bracket != Some(*id) && !unit.bracket_objects.contains(id)))
        {
            return Err("Invalid crossing-bond bracket association".into());
        }
        if unit
            .head_count
            .is_some_and(|n| n == 0 || n > unit.crossings.len())
        {
            return Err("Invalid repeat head-bond count".into());
        }
        match &unit.count {
            RepeatCount::Finite { min, max } => check_range(*min, *max)?,
            RepeatCount::Indefinite { label } => {
                if !printable(label) {
                    return Err("Invalid indefinite repeat label".into());
                }
            }
        }
        let actual: HashSet<_> = doc
            .bonds
            .iter()
            .filter(|b| set.contains(&b.a) != set.contains(&b.b))
            .map(|b| pair(b.a, b.b))
            .collect();
        let expected: HashSet<_> = unit
            .crossings
            .iter()
            .map(|c| pair(c.inside, c.outside))
            .collect();
        if actual != expected
            || expected.len() != unit.crossings.len()
            || unit
                .crossings
                .iter()
                .any(|c| !set.contains(&c.inside) || set.contains(&c.outside))
        {
            return Err("Repeating-unit crossing bonds must match its atom membership".into());
        }
    }
    Ok(())
}
pub fn prune(doc: &mut Document) {
    let atoms: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    let graphics: HashSet<_> = doc.graphics.iter().map(|g| g.id).collect();
    doc.generic.alternatives.retain(|s| atoms.contains(&s.atom));
    let degrees = degrees(doc);
    let removed: Vec<_> = doc
        .generic
        .links
        .iter()
        .filter(|s| !atoms.contains(&s.atom) || degrees.get(&s.atom) != Some(&2))
        .map(|s| s.atom)
        .collect();
    doc.generic.links.retain(|s| !removed.contains(&s.atom));
    for id in removed {
        if let Some(atom) = doc.atom_mut(id) {
            atom.display.set_variable(None);
        }
    }
    let bonds: HashSet<_> = doc.bonds.iter().map(|b| pair(b.a, b.b)).collect();
    doc.generic.repeats.retain(|u| {
        u.members.iter().all(|id| atoms.contains(id))
            && u.crossings.iter().all(|c| atoms.contains(&c.outside))
            && u.bracket.is_none_or(|id| graphics.contains(&id))
            && u.bracket_objects.iter().all(|id| graphics.contains(id))
            && u.crossings
                .iter()
                .all(|c| bonds.contains(&pair(c.inside, c.outside)))
    });
}
pub fn selection(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut set: HashSet<_> = ids.iter().copied().collect();
    // Shared endpoints can connect more than one retained unit. Compute the
    // closure so selecting any referenced object never makes it look concrete.
    loop {
        let before = set.len();
        for unit in &doc.generic.repeats {
            if unit.bracket.is_some_and(|id| set.contains(&id))
                || unit.bracket_objects.iter().any(|id| set.contains(id))
                || unit.members.iter().any(|id| set.contains(id))
                || unit.crossings.iter().any(|c| set.contains(&c.outside))
            {
                set.extend(unit.bracket_objects.iter().copied());
                set.extend(unit.members.iter().copied());
                set.extend(unit.crossings.iter().map(|c| c.outside));
                if let Some(id) = unit.bracket {
                    set.insert(id);
                }
            }
        }
        if set.len() == before {
            break;
        }
    }
    let mut result: Vec<_> = set.into_iter().collect();
    result.sort_unstable();
    result
}

#[cfg(test)]
mod tests;
