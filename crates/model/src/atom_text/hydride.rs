//! Explicit atom labels are chemical input, independent of valence checking.

pub(super) struct Hydride {
    pub element: String,
    pub hydrogens: u32,
    pub charge: i32,
    pub charged_element: bool,
}

pub(super) fn parse(input: &str) -> Result<Option<Hydride>, String> {
    // Keep the typographic boundary before flattening PH₂²⁻ to ASCII digits.
    let separated = input
        .char_indices()
        .next_back()
        .and_then(|(index, sign)| {
            if matches!(sign, '⁺' | '⁻') {
                let before = input.get(..index)?;
                let body = before.trim_end_matches(|c| {
                    matches!(c, '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹')
                });
                return Some((
                    body,
                    if sign == '⁺' { '+' } else { '-' },
                    before.get(body.len()..)?,
                ));
            }
            let (body, suffix) = input.rsplit_once(char::is_whitespace)?;
            let (index, sign) = suffix.char_indices().next_back()?;
            if !matches!(sign, '+' | '-' | '−') {
                return None;
            }
            let count = suffix.get(..index)?;
            normalize(count)
                .chars()
                .all(|c| c.is_ascii_digit())
                .then_some((body.trim_end(), if sign == '+' { '+' } else { '-' }, count))
        })
        .map(|(body, sign, count)| (normalize(body), sign, normalize(count)));
    let normalized = normalize(input);
    let (body, charge_suffix) = if let Some((body, sign, count)) = &separated {
        (body.as_str(), Some((*sign, count.as_str())))
    } else if let Some((i, sign)) = normalized
        .char_indices()
        .find(|(_, c)| matches!(c, '+' | '-'))
    {
        let suffix = normalized
            .get(i + sign.len_utf8()..)
            .ok_or("Invalid charge")?;
        (
            normalized.get(..i).ok_or("Invalid atom label")?,
            Some((sign, suffix)),
        )
    } else {
        (normalized.as_str(), None)
    };
    // Match a complete symbol: Hf/Hg/He are elements, not hydrogen prefixes.
    let valid = |symbol: &str| symbol != "H" && crate::editing::ELEMENTS.contains(&symbol);
    let forward = crate::editing::ELEMENTS
        .iter()
        .filter(|e| valid(e))
        .filter_map(|element| {
            body.strip_prefix(element)
                .and_then(|s| s.strip_prefix('H'))
                .map(|count| (*element, count))
        })
        .find(|(_, count)| count.chars().all(|c| c.is_ascii_digit()));
    let parsed = if let Some((element, count)) = forward {
        if count.len() > 1
            && separated.is_none()
            && charge_suffix.is_some_and(|(_, suffix)| suffix.is_empty())
        {
            return Err("Write the charge as e.g. NH₄⁺, PH₂²⁻ or NH4+2".into());
        }
        Some((element, count))
    } else if let Some(rest) = body.strip_prefix('H') {
        let split = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let count = rest.get(..split).ok_or("Invalid hydrogen count")?;
        let element = rest.get(split..).ok_or("Invalid element")?;
        valid(element).then_some((element, count))
    } else {
        None
    };
    let Some((element, count)) = parsed else {
        return charged_element(&normalized);
    };
    if !count.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let charge = if let Some((sign, suffix)) = charge_suffix {
        let count = if suffix.is_empty() {
            1
        } else {
            suffix
                .parse::<i32>()
                .map_err(|_| "Use a charge such as +, - or +2")?
        };
        if !(1..=8).contains(&count) {
            return Err("Enter a charge from -8 to +8".into());
        }
        if sign == '+' { count } else { -count }
    } else {
        0
    };
    let hydrogens = if count.is_empty() {
        1
    } else {
        count
            .parse::<u32>()
            .map_err(|_| "Hydrogen count is too large")?
    };
    if hydrogens > u32::from(u8::MAX) {
        return Err("Hydrogen count must be at most 255".into());
    }
    Ok(Some(Hydride {
        element: element.into(),
        hydrogens,
        charge,
        charged_element: false,
    }))
}

fn normalize(input: &str) -> String {
    input
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
        .collect()
}

fn charged_element(input: &str) -> Result<Option<Hydride>, String> {
    let split = input
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(input.len());
    let element = input.get(..split).ok_or("Invalid element")?;
    if !crate::editing::ELEMENTS.contains(&element) {
        return Ok(None);
    }
    let suffix = input.get(split..).ok_or("Invalid charge")?;
    let (sign, count) = if let Some(count) = suffix.strip_prefix('+') {
        ('+', count)
    } else if let Some(count) = suffix.strip_prefix('-') {
        ('-', count)
    } else if let Some(count) = suffix.strip_suffix('+') {
        ('+', count)
    } else if let Some(count) = suffix.strip_suffix('-') {
        ('-', count)
    } else {
        return Ok(None);
    };
    if !count.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let magnitude = if count.is_empty() {
        1
    } else {
        count
            .parse::<i32>()
            .map_err(|_| "Enter a charge from -8 to +8")?
    };
    if !(1..=8).contains(&magnitude) {
        return Err("Enter a charge from -8 to +8".into());
    }
    Ok(Some(Hydride {
        element: element.into(),
        hydrogens: 0,
        charge: if sign == '+' { magnitude } else { -magnitude },
        charged_element: true,
    }))
}

pub fn entry(atom: &crate::document::Atom) -> String {
    let element = if atom.isotope == 0 || atom.element == "*" {
        atom.element.clone()
    } else {
        format!("{}{}", atom.isotope, atom.element)
    };
    // A typed H+ or H- has no hydrogens of its own; other H atoms stay bare.
    let charged_hydrogen =
        atom.element == "H" && atom.no_implicit && atom.explicit_h == 0 && atom.charge != 0;
    if atom.explicit_h == 0 && !atom.no_implicit
        || atom.element == "*"
        || atom.element == "H" && !charged_hydrogen
    {
        return atom.display.variable.clone().unwrap_or(element);
    }
    let sign = if atom.charge > 0 { '+' } else { '-' };
    let magnitude = atom.charge.unsigned_abs();
    if atom.explicit_h == 0 && atom.charge != 0 {
        // A charged element without hydrogens: digits, then the sign (Pd2+, Na+).
        let amount = if magnitude > 1 {
            magnitude.to_string()
        } else {
            String::new()
        };
        return format!("{element}{amount}{sign}");
    }
    let mut text = format!("{element}H{}", atom.explicit_h);
    if atom.explicit_h == 1 {
        text.pop();
    }
    if atom.charge != 0 {
        text.push(sign);
        // parse rejects a bare sign after a multi-digit count (NH42+), so write NH12+1.
        if magnitude > 1 || atom.explicit_h >= 10 {
            text.push_str(&magnitude.to_string());
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atom_text::{Mode, apply};
    use crate::document::{Document, Point};

    #[test]
    fn superscript_and_spaced_charges_keep_the_hydrogen_count() -> Result<(), String> {
        for (text, element, hydrogens, charge) in [
            ("PH₂²⁻", "P", 2, -2),
            ("AlH₆³⁻", "Al", 6, -3),
            ("NH₄⁺", "N", 4, 1),
            ("NH4²⁺", "N", 4, 2),
            ("NH₁₂⁺", "N", 12, 1),
            ("H₃O⁺", "O", 3, 1),
            ("NH4 2+", "N", 4, 2),
            ("NH4 +", "N", 4, 1),
            ("NH4 2-", "N", 4, -2),
            ("NH12 +", "N", 12, 1),
            ("NH4+2", "N", 4, 2),
            ("NH12-1", "N", 12, -1),
            ("CH2⁻³", "C", 2, -3),
        ] {
            let parsed = parse(text)?.ok_or_else(|| format!("Missing hydride: {text}"))?;
            assert_eq!(parsed.element, element, "{text}");
            assert_eq!(parsed.hydrogens, hydrogens, "{text}");
            assert_eq!(parsed.charge, charge, "{text}");
            assert!(!parsed.charged_element, "{text}");
        }
        Ok(())
    }

    #[test]
    fn explicit_charge_boundaries_keep_limits_and_ambiguous_ascii_guidance() {
        for text in ["NH42-", "CH32+", "NH12-"] {
            assert_eq!(
                parse(text).err().as_deref(),
                Some("Write the charge as e.g. NH₄⁺, PH₂²⁻ or NH4+2"),
                "{text}"
            );
        }
        for text in ["PH₂⁰⁻", "PH₂⁹⁻", "NH4 0+", "NH4 9-"] {
            assert_eq!(
                parse(text).err().as_deref(),
                Some("Enter a charge from -8 to +8"),
                "{text}"
            );
        }
        for text in ["Pd 2+", "SO4 2-", "ligand 2-"] {
            assert!(matches!(parse(text), Ok(None)), "{text}");
        }
    }

    #[test]
    fn automatic_entry_reopens_superscript_and_spaced_hydride_charges() -> Result<(), String> {
        let mut source = Document::default();
        let id = source.add_atom("C", Point::default());
        for (text, element, hydrogens, charge, canonical) in [
            ("PH₂²⁻", "P", 2, -2, "PH2-2"),
            ("AlH₆³⁻", "Al", 6, -3, "AlH6-3"),
            ("NH₄⁺", "N", 4, 1, "NH4+"),
            ("NH4 2+", "N", 4, 2, "NH4+2"),
            ("NH4 +", "N", 4, 1, "NH4+"),
            ("NH4 2-", "N", 4, -2, "NH4-2"),
        ] {
            let typed = apply(&source, id, text, Mode::Auto)?;
            let atom = typed.atom(id).ok_or("Missing atom")?;
            assert_eq!(atom.element, element, "{text}");
            assert_eq!(atom.explicit_h, hydrogens, "{text}");
            assert_eq!(atom.charge, charge, "{text}");
            assert!(atom.no_implicit, "{text}");
            assert!(atom.display.variable.is_none(), "{text}");
            assert_eq!(entry(atom), canonical, "{text}");
            assert_eq!(apply(&source, id, canonical, Mode::Auto)?, typed, "{text}");
        }
        Ok(())
    }

    #[test]
    fn entry_preserves_the_isotope_for_elements_hydrides_and_charges() -> Result<(), String> {
        let mut source = Document::default();
        let id = source.add_atom("C", Point::default());
        for (text, isotope, expected) in [
            ("C", 13, "13C"),
            ("CH3", 13, "13CH3"),
            ("NH4+", 15, "15NH4+"),
            ("Fe2+", 57, "57Fe2+"),
            ("H", 2, "2H"),
            ("H+", 3, "3H+"),
        ] {
            let mut typed = apply(&source, id, text, Mode::Auto)?;
            let atom = typed.atom_mut(id).ok_or("Missing atom")?;
            atom.isotope = isotope;
            assert_eq!(entry(atom), expected, "{text}");
        }
        Ok(())
    }
}
