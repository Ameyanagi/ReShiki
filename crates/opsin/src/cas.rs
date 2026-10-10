//! CAS index-name uninversion, ported from OPSIN `CASTools.java`.
//! OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::{
    ParsingError,
    parse_rules::{self, WordType},
    resources::Resources,
    tokenizer,
};

fn error(message: impl Into<String>) -> ParsingError {
    ParsingError(message.into())
}
fn strip_close_brackets(text: &str) -> &str {
    text.trim_end_matches([']', ')', '}'])
}
fn is_acid(text: &str) -> bool {
    strip_close_brackets(text).eq_ignore_ascii_case("acid")
}

fn split_spaces(text: &str) -> Vec<&str> {
    let mut parts: Vec<_> = text.split(' ').collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

fn prefixed_functional_term(text: &str) -> bool {
    let text = strip_close_brackets(text).to_ascii_lowercase();
    matches!(
        text.as_str(),
        "amide"
            | "hydrazide"
            | "oxime"
            | "thioxime"
            | "selenoxime"
            | "telluroxime"
            | "hydrazone"
            | "semicarbazone"
            | "thiosemicarbazone"
            | "selenosemicarbazone"
            | "tellurosemicarbazone"
            | "isosemicarbazone"
            | "isothiosemicarbazone"
            | "isoselenosemicarbazone"
            | "isotellurosemicarbazone"
            | "imide"
            | "imine"
            | "semioxamazone"
    )
}

pub(crate) fn uninvert(resources: &Resources, name: &str) -> Result<String, ParsingError> {
    // Java Pattern.split discards trailing empty fields.
    let components: Vec<_> = name
        .split(", ")
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .skip_while(|text| text.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut parent = components.first().copied().unwrap_or_default().to_owned();
    let mut parent_parts = split_spaces(&parent);
    if parent_parts.len() != 1 {
        if tokenizer::is_cas_collective_index(parent_parts.last().unwrap()) {
            parent = parent_parts[..parent_parts.len() - 1].concat();
            parent_parts = split_spaces(&parent);
        }
        for part in parent_parts.into_iter().skip(1) {
            if !is_acid(part)
                && parse_rules::parse_word(resources, part, false)?
                    .parses
                    .is_empty()
            {
                return Err(error(
                    "Invalid CAS name. Parent compound was followed by an unexpected term",
                ));
            }
        }
    }
    let mut substituents = Vec::new();
    let mut separate_substituents = Vec::new();
    let mut functional_terms = Vec::new();
    let mut added_bracket = false;
    let mut ester_encountered = false;
    for &name_component in components.iter().skip(1) {
        let lower = name_component.to_ascii_lowercase();
        let compound_with = ["compd. with ", "compound with ", "and "]
            .iter()
            .any(|prefix| lower.starts_with(prefix));
        // Pinned CASTools computes a stripped nameComponent but splits the
        // original component. Keep that observable upstream behavior.
        let parts = split_spaces(name_component);
        let mut index = 0;
        while index < parts.len() {
            let component = parts[index];
            index += 1;
            if compound_with {
                functional_terms.push(component.to_owned());
                continue;
            }
            if component.ends_with('-') {
                if let Some(close) = missing_close_bracket(component) {
                    if added_bracket {
                        return Err(error("Close bracket appears to be missing"));
                    }
                    parent.push(close);
                    added_bracket = true;
                }
                substituents.push(component.to_owned());
                continue;
            }
            let results = parse_rules::parse_word(resources, component, false)?;
            if results.parses.is_empty() {
                if !tokenizer::is_cas_collective_index(component) {
                    return Err(error(format!(
                        "Unable to interpret: {component} (as part of a CAS index name)"
                    )));
                }
                continue;
            }
            let words = tokenizer::split_into_words(results.parses, component);
            let word_type = unique_word_type(&words[0].alternatives, component)?;
            if words.len() == 1 {
                match word_type {
                    WordType::FunctionalTerm => {
                        if component.eq_ignore_ascii_case("ester") {
                            if separate_substituents.is_empty() {
                                return Err(error(
                                    "ester encountered but no substituents were specified in potential CAS name!",
                                ));
                            }
                            if ester_encountered {
                                return Err(error(
                                    "ester formation was mentioned more than once in CAS name!",
                                ));
                            }
                            parent = uninvert_ester(&parent)?;
                            ester_encountered = true;
                        } else {
                            functional_terms.push(component.to_owned());
                        }
                    }
                    WordType::Substituent => separate_substituents.push(component.to_owned()),
                    WordType::Full => {
                        let lower = component.to_ascii_lowercase();
                        if [
                            "ate",
                            "ite",
                            "ium",
                            "hydrofluoride",
                            "hydrochloride",
                            "hydrobromide",
                            "hydroiodide",
                        ]
                        .iter()
                        .any(|end| lower.ends_with(end))
                        {
                            functional_terms.push(component.to_owned());
                        } else if lower.ends_with("ic")
                            && index < parts.len()
                            && parts[index].eq_ignore_ascii_case("acid")
                        {
                            functional_terms.push(component.to_owned());
                            functional_terms.push(parts[index].to_owned());
                            index += 1;
                        } else {
                            return Err(error(format!(
                                "Unable to interpret: {component} (as part of a CAS index name)- A full word was encountered where a substituent or functionalTerm was expected"
                            )));
                        }
                    }
                    _ => return Err(error("Unrecognised CAS index name form")),
                }
            } else if words.len() == 2 && word_type == WordType::Substituent {
                let second_type = unique_word_type(&words[1].alternatives, component)?;
                if second_type == WordType::FunctionalTerm
                    && prefixed_functional_term(&words[1].word)
                {
                    functional_terms.push(component.to_owned());
                } else {
                    return Err(error(
                        "Unrecognised CAS index name form, could have a missing space?",
                    ));
                }
            } else {
                return Err(error("Unrecognised CAS index name form"));
            }
        }
    }
    let mut result = String::new();
    for substituent in separate_substituents {
        result.push_str(&substituent);
        result.push(' ');
    }
    for substituent in substituents.iter().rev() {
        result.push_str(substituent);
    }
    result.push_str(&parent);
    for functional_term in functional_terms {
        result.push(' ');
        result.push_str(&functional_term);
    }
    Ok(result)
}

fn unique_word_type(
    alternatives: &[crate::parse_rules::ParseTokens],
    component: &str,
) -> Result<WordType, ParsingError> {
    let first = alternatives[0].word_type()?;
    for alternative in alternatives.iter().skip(1) {
        if alternative.word_type()? != first {
            return Err(error(format!(
                "{component}can be interpreted in multiple ways. For the sake of precision OPSIN has decided not to process this as a CAS name"
            )));
        }
    }
    Ok(first)
}

fn missing_close_bracket(component: &str) -> Option<char> {
    let mut level = 0i32;
    let mut missing = None;
    for ch in component.chars() {
        if matches!(ch, '(' | '[' | '{') {
            level += 1;
            if level == 1 {
                missing = Some(ch);
            }
        }
        if matches!(ch, ')' | ']' | '}') {
            level -= 1;
            if level < 0 {
                return None;
            }
        }
    }
    if level != 1 {
        return None;
    }
    match missing {
        Some('(') => Some(')'),
        Some('[') => Some(']'),
        Some('{') => Some('}'),
        _ => None,
    }
}

fn uninvert_ester(parent: &str) -> Result<String, ParsingError> {
    let lower = parent.to_ascii_lowercase();
    let replacements = if parent.ends_with(')') {
        [
            ("ic acid)", 8, "ate)"),
            ("ous acid)", 9, "ite)"),
            ("ine)", 2, "ate)"),
        ]
    } else {
        [
            ("ic acid", 7, "ate"),
            ("ous acid", 8, "ite"),
            ("ine", 1, "ate"),
        ]
    };
    for (ending, removed, replacement) in replacements {
        if lower.ends_with(ending) {
            return Ok(format!(
                "{}{replacement}",
                &parent[..parent.len() - removed]
            ));
        }
    }
    Err(error("Failed to uninvert CAS ester"))
}
