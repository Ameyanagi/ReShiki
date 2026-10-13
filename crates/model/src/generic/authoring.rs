use super::{
    Alternative, AlternativeSite, Crossing, LinkNode, RepeatCount, RepeatPattern, RepeatUnit,
    limits::MAX_ALTERNATIVES, printable, reject_affected_stereo,
};
use crate::document::Document;
use std::collections::HashSet;

pub fn alternative(text: &str) -> Result<Option<(Vec<Alternative>, bool)>, String> {
    let text = text.trim();
    let bracketed = text.starts_with('[') && text.ends_with(']');
    let inside = if bracketed {
        text.strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or("Invalid alternative list")?
    } else {
        text
    };
    if !inside.contains(',') {
        return Ok(None);
    }
    if text.contains(['[', ']']) && !bracketed {
        return Err("Use matching brackets for an alternative list".into());
    }
    let (inside, excluded) = if let Some(s) = inside.trim().strip_prefix("NOT ") {
        (s, true)
    } else if let Some(s) = inside.trim().strip_prefix('-') {
        (s, true)
    } else {
        (inside, false)
    };
    let mut choices = Vec::new();
    for item in inside.split(',') {
        let item = item.trim();
        if !printable(item) {
            return Err("Each alternative must be a printable element or defined group".into());
        }
        let choice = if crate::editing::ELEMENTS.contains(&item) {
            Alternative::Element {
                symbol: item.into(),
                isotope: 0,
            }
        } else if matches!(item, "D" | "T") {
            Alternative::Element {
                symbol: "H".into(),
                isotope: if item == "D" { 2 } else { 3 },
            }
        } else if !excluded
            && (crate::abbreviations::PRESETS.contains(&item)
                || crate::common_groups::LABELS.contains(&item)
                || item == "OPh")
        {
            Alternative::Group { label: item.into() }
        } else if !excluded
            && (item == "R"
                || item == "X"
                || item
                    .strip_prefix('R')
                    .is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())))
        {
            Alternative::Unresolved { label: item.into() }
        } else {
            return Err(format!(
                "Alternative {item} has no defined chemical fragment"
            ));
        };
        choices.push(choice);
    }
    if choices.len() > MAX_ALTERNATIVES {
        return Err("Too many alternatives".into());
    }
    Ok(Some((choices, excluded)))
}
pub fn apply_label(doc: &Document, id: u64, text: &str) -> Result<Option<Document>, String> {
    if let Some((choices, excluded)) = alternative(text)? {
        reject_affected_stereo(doc, &[id])?;
        if doc.abbreviation(id).is_some() {
            return Err("Expand this abbreviation before defining alternatives".into());
        }
        let mut result = doc.clone();
        result.generic.alternatives.retain(|s| s.atom != id);
        result.generic.links.retain(|s| s.atom != id);
        let a = result.atom_mut(id).ok_or("Missing alternative atom")?;
        if a.attachment.is_some() || !a.centroid.is_empty() {
            return Err("Use an ordinary atom for alternatives".into());
        }
        a.element = "*".into();
        a.no_implicit = true;
        a.explicit_h = 0;
        a.isotope = 0;
        a.charge = 0;
        a.aromatic = false;
        a.label_h = 0;
        a.display.set_variable(Some(text.into()));
        result.generic.alternatives.push(AlternativeSite {
            atom: id,
            choices,
            excluded,
        });
        result.invalidate_chemistry(&[id]);
        result.validate()?;
        return Ok(Some(result));
    }
    let mut base = None;
    for (open, close) in [('[', ']'), ('(', ')'), ('{', '}')] {
        if let Some(after) = text.strip_prefix(open)
            && let Some((label, range)) = after.split_once(close)
            && !range.is_empty()
        {
            base = Some((label, range));
            break;
        }
    }
    // A trailing sign is a charge (O2-, SO4 2-), never a range, and so is a
    // charged hydride (NH4-2, CH2-3). A space before the range separates it
    // from the label's own digits (CH2 1-3).
    if base.is_none()
        && !text.trim_end().ends_with(['+', '-', '−', '⁺', '⁻'])
        && !crate::atom_text::charged_label(text)
        && let Some(start) = text
            .split_once(char::is_whitespace)
            .filter(|(_, range)| range.trim_start().starts_with(|c: char| c.is_ascii_digit()))
            .map(|(label, _)| label.len())
            .or_else(|| text.find(|c: char| c.is_ascii_digit()))
        && text.get(start..).is_some_and(|s| s.contains('-'))
    {
        base = Some((
            text.get(..start).ok_or("Invalid link label")?,
            text.get(start..).ok_or("Invalid link range")?,
        ));
    }
    let Some((label, range)) = base else {
        return Ok(None);
    };
    let RepeatCount::Finite { min, max } = RepeatCount::parse(range)? else {
        return Err("A link node requires a finite number range".into());
    };
    reject_affected_stereo(doc, &[id])?;
    let mut result = doc.clone();
    result.generic.links.retain(|s| s.atom != id);
    result.generic.alternatives.retain(|s| s.atom != id);
    // Use the existing element/hydride input contract for the repeated atom.
    result = crate::atom_text::apply(&result, id, label, crate::atom_text::Mode::Auto)?;
    if result.abbreviation(id).is_some() {
        return Err("A link node must be one atom or a generic atom label".into());
    }
    result
        .atom_mut(id)
        .ok_or("Missing link atom")?
        .display
        .set_variable(Some(text.into()));
    result.generic.links.push(LinkNode { atom: id, min, max });
    result.validate()?;
    Ok(Some(result))
}
pub fn set_repeat_unit(doc: &mut Document, unit: RepeatUnit) -> Result<(), String> {
    let mut candidate = doc.clone();
    if let Some(id) = unit.bracket {
        candidate.generic.repeats.retain(|u| u.bracket != Some(id));
    }
    candidate.generic.repeats.push(unit);
    candidate.validate()?;
    *doc = candidate;
    Ok(())
}

/// The bracket frame defines the atom membership; connectivity still comes
/// exclusively from existing bonds crossing that explicit member set.
pub fn define_bracket(
    source: &Document,
    bracket: u64,
    label: &str,
    pattern: RepeatPattern,
) -> Result<Document, String> {
    source.validate()?;
    let graphic = source
        .graphics
        .iter()
        .find(|g| g.id == bracket && g.kind.brackets())
        .ok_or("Select one bracket around the repeating unit")?;
    let det = graphic.frame_determinant();
    if det.abs() < 0.01 {
        return Err("Draw a bracket with a nonzero enclosed area".into());
    }
    let members: Vec<_> = source
        .atoms
        .iter()
        .filter(|a| {
            let p = graphic.world_to_local(a.position, det);
            (0.0..=1.).contains(&p.x) && (0.0..=1.).contains(&p.y)
        })
        .map(|a| a.id)
        .collect();
    if members.is_empty() {
        return Err("The bracket encloses no atoms".into());
    }
    let set: HashSet<_> = members.iter().copied().collect();
    let mut crossings: Vec<_> = source
        .bonds
        .iter()
        .filter(|b| set.contains(&b.a) != set.contains(&b.b))
        .map(|b| {
            if set.contains(&b.a) {
                Crossing {
                    inside: b.a,
                    outside: b.b,
                }
            } else {
                Crossing {
                    inside: b.b,
                    outside: b.a,
                }
            }
        })
        .collect();
    crossings.sort_by(|a, b| {
        let projection = |c: &Crossing| {
            source
                .atom(c.outside)
                .map(|a| {
                    (a.position.x - graphic.origin.x) * graphic.axis_x.x
                        + (a.position.y - graphic.origin.y) * graphic.axis_x.y
                })
                .unwrap_or(0.)
        };
        projection(a).total_cmp(&projection(b))
    });
    let count = RepeatCount::parse(label)?;
    let head_count = RepeatUnit::default_head_count(crossings.len());
    let mut candidate = source.clone();
    set_repeat_unit(
        &mut candidate,
        RepeatUnit {
            bracket: Some(bracket),
            bracket_objects: Vec::new(),
            members,
            crossings,
            crossing_brackets: Vec::new(),
            head_count,
            usage: "SRU".into(),
            pattern,
            count,
            label: label.trim().into(),
            flip: "Unspecified".into(),
        },
    )?;
    Ok(candidate)
}

#[cfg(test)]
mod tests;
