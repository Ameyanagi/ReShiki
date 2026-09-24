//! Explicit atom labels are chemical input, independent of valence checking.

pub(super) struct Hydride {
    pub element: String,
    pub hydrogens: u32,
    pub charge: i32,
}

pub(super) fn parse(input: &str) -> Result<Option<Hydride>, String> {
    let normalized: String = input
        .chars()
        .map(|c| match c {
            '₀' => '0',
            '₁' => '1',
            '₂' => '2',
            '₃' => '3',
            '₄' => '4',
            '₅' => '5',
            '₆' => '6',
            '₇' => '7',
            '₈' => '8',
            '₉' => '9',
            '⁺' => '+',
            '⁻' | '−' => '-',
            other => other,
        })
        .collect();
    let (body, charge_suffix) = if let Some((i, sign)) = normalized
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
        return Ok(None);
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
    }))
}

pub fn entry(atom: &crate::document::Atom) -> String {
    if atom.explicit_h == 0 && !atom.no_implicit || matches!(atom.element.as_str(), "*" | "H") {
        return atom
            .display
            .variable
            .clone()
            .unwrap_or_else(|| atom.element.clone());
    }
    let mut text = format!("{}H{}", atom.element, atom.explicit_h);
    if atom.explicit_h == 1 {
        text.pop();
    }
    if atom.charge != 0 {
        text.push(if atom.charge > 0 { '+' } else { '-' });
        if atom.charge.unsigned_abs() > 1 {
            text.push_str(&atom.charge.unsigned_abs().to_string());
        }
    }
    text
}
