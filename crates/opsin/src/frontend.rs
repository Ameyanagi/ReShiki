//! The complete pinned OPSIN `Parser.java` name-to-parse-tree stage.
//! OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::{
    ParseOptions, ParsingError, cas,
    parse_rules::ParseTokens,
    parse_tree::{Arena, NodeId, ParseTree},
    resources::Resources,
    tokenizer::{self, ParseWord, Tokenization},
    word_rules::WordRules,
};
use std::collections::VecDeque;

pub(crate) fn parse(
    resources: &Resources,
    name: &str,
    options: &ParseOptions,
) -> Result<Vec<ParseTree>, ParsingError> {
    let (name, ratios) = extract_stoichiometry(name)?;
    let mut tokenized = None;
    if name.contains(", ") {
        if let Ok(uninverted) = cas::uninvert(resources, name)
            && let Ok(tokens) = tokenizer::tokenize(resources, &uninverted, false, false)
            && tokens.is_successful()
        {
            tokenized = Some(tokens);
        }
    } else if name.contains("; ") {
        let tokens = tokenizer::tokenize(resources, &name.replace("; ", " "), false, false)?;
        if tokens.is_successful() {
            tokenized = Some(tokens);
        }
    }
    let allow_space_removal = tokenized.is_none();
    let tokenized = if let Some(tokens) = tokenized {
        tokens
    } else {
        let tokens = tokenizer::tokenize(resources, name, true, false)?;
        if !tokens.is_successful() {
            if options.detailed_failure_analysis {
                return Err(exact_failure_reason(resources, &tokens, name)?);
            }
            return Err(ParsingError(format!(
                "{name} is unparsable due to the following being uninterpretable: {} The following was not parseable: {}",
                tokens.uninterpretable_name, tokens.unparseable_name
            )));
        }
        tokens
    };
    let combinations = generate_combinations(&tokenized.words)?;
    if combinations.is_empty() {
        return Err(ParsingError(format!("No parses could be found for {name}")));
    }
    let word_rules = WordRules::new()?;
    let mut results = Vec::new();
    let mut precise_error = None;
    for words in combinations {
        let mut arena = Arena::default();
        let molecule = arena.grouping("molecule");
        arena[molecule].add_attribute("name", name);
        for (word, tokens) in words {
            let element = arena.grouping("word");
            arena.add_child(molecule, element);
            let word_type = tokens.word_type()?;
            arena[element].add_attribute("type", word_type.as_str());
            arena[element].add_attribute("value", word.strip_prefix('-').unwrap_or(word));
            write_word_xml(&mut arena, element, tokens)?;
        }
        if word_rules
            .group(
                &mut arena,
                molecule,
                options,
                allow_space_removal,
                ratios.as_deref(),
            )
            .is_err()
        {
            continue;
        }
        if let Some(ratios) = &ratios {
            let word_rules = arena.children_named(molecule, "wordRule");
            if word_rules.len() != ratios.len() {
                precise_error = Some(ParsingError(format!(
                    "Component and stoichiometry indication indication mismatch. OPSIN believes there to be {} components but {} ratios were given!",
                    word_rules.len(),
                    ratios.len()
                )));
                continue;
            }
            for (&word_rule, ratio) in word_rules.iter().zip(ratios) {
                arena[word_rule].add_attribute("stoichiometry", ratio.to_string());
            }
        }
        if arena[molecule].attribute("isSalt").is_some()
            && arena.children_named(molecule, "wordRule").len() < 2
        {
            precise_error = Some(ParsingError(format!(
                "{name} is apparently a salt, but the name only contained one component. The name could be describing a class of compounds"
            )));
            continue;
        }
        results.push(ParseTree {
            arena,
            root: molecule,
        });
    }
    if results.is_empty() {
        return Err(precise_error.unwrap_or_else(|| ParsingError(format!("{name} could be parsed but OPSIN was unsure of the meaning of the words. This error will occur, by default, if a name is just a substituent"))));
    }
    Ok(results)
}

pub(crate) fn write_word_xml(
    arena: &mut Arena,
    word: NodeId,
    tokens: &ParseTokens,
) -> Result<(), ParsingError> {
    let mut last_chunk = None;
    for chunk_tokens in tokens.chunks() {
        let chunk = arena.grouping("substituent");
        arena.add_child(word, chunk);
        last_chunk = Some(chunk);
        let mut last_token = None;
        for token in chunk_tokens {
            if !token.ignored {
                let element = arena.token(&token.tag_name, &token.text);
                for (name, value) in &token.attribute_order {
                    arena[element].add_attribute(name, value);
                }
                arena.add_child(chunk, element);
                last_token = Some(element);
            } else if let Some(previous) = last_token
                && !token.text.is_empty()
            {
                let value = format!(
                    "{}{}",
                    arena[previous]
                        .attribute("subsequentUnsemanticToken")
                        .unwrap_or(""),
                    token.text
                );
                arena[previous].set_attribute("subsequentUnsemanticToken", value);
            }
        }
    }
    let last_chunk = last_chunk
        .ok_or_else(|| ParsingError("OPSIN bug: Word has no annotation chunks".into()))?;
    match tokens.word_type()? {
        crate::parse_rules::WordType::Full => arena[last_chunk].name = "root".into(),
        crate::parse_rules::WordType::FunctionalTerm => {
            arena[last_chunk].name = "functionalTerm".into()
        }
        _ => {}
    }
    Ok(())
}

fn generate_combinations(
    words: &[ParseWord],
) -> Result<Vec<Vec<(&str, &ParseTokens)>>, ParsingError> {
    let mut count = 1usize;
    for word in words {
        count = count.saturating_mul(word.alternatives.len());
        if count > 128 {
            return Err(ParsingError("Too many different combinations of word interpretation are possible (>128) i.e. name contains too many terms that OPSIN finds ambiguous to interpret".into()));
        }
    }
    if count == 1 {
        return Ok(vec![
            words
                .iter()
                .map(|word| (word.word.as_str(), &word.alternatives[0]))
                .collect(),
        ]);
    }
    let mut queue = VecDeque::from([Vec::new()]);
    let mut parses = Vec::new();
    while let Some(current) = queue.pop_front() {
        if current.len() == words.len() {
            parses.push(current);
            continue;
        }
        let word = &words[current.len()];
        for alternative in word.alternatives.iter().rev() {
            let mut next = current.clone();
            next.push((word.word.as_str(), alternative));
            queue.push_back(next);
        }
    }
    Ok(parses)
}

/// Pinned Parser accepts brackets independently and maps unknown ratios to 1.
pub fn process_stoichiometry_indication(ratio: &str) -> Result<Vec<i32>, ParsingError> {
    let ratio = ratio.trim();
    if ratio.len() < 2 {
        return Err(ParsingError("Invalid component ratio declaration".into()));
    }
    let inner = &ratio[1..ratio.len() - 1];
    let colon: Vec<_> = inner.split(':').collect();
    let parts = if colon.len() == 1 {
        inner.split('/').collect()
    } else {
        colon
    };
    let mut values = Vec::with_capacity(parts.len());
    for part in parts {
        if part.contains('/') {
            return Err(ParsingError(
                "Unexpected / in component ratio declaration".into(),
            ));
        }
        let value = if part == "?" {
            1
        } else {
            part.parse::<i32>()
                .map_err(|_| ParsingError(format!("Invalid component ratio: {part}")))?
        };
        values.push(value);
    }
    Ok(values)
}

fn extract_stoichiometry(name: &str) -> Result<(&str, Option<Vec<i32>>), ParsingError> {
    if !name.ends_with([')', ']', '}']) {
        return Ok((name, None));
    }
    let Some(open) = name.rfind(['(', '[', '{']) else {
        return Ok((name, None));
    };
    let inner = &name[open + 1..name.len() - 1];
    let mut had_separator = false;
    let valid = inner.split([':', '/']).all(|part| {
        !part.is_empty() && (part == "?" || part.bytes().all(|ch| ch.is_ascii_digit()))
    });
    for ch in inner.chars() {
        if matches!(ch, ':' | '/') {
            had_separator = true;
        }
    }
    if !valid || !had_separator {
        return Ok((name, None));
    }
    let start = if open > 0 && name.as_bytes()[open - 1] == b' ' {
        open - 1
    } else {
        open
    };
    Ok((
        &name[..start],
        Some(process_stoichiometry_indication(&name[start..])?),
    ))
}

fn exact_failure_reason(
    resources: &Resources,
    forward: &Tokenization,
    name: &str,
) -> Result<ParsingError, ParsingError> {
    let reverse = tokenizer::tokenize(resources, &forward.uninterpretable_name, true, true)?;
    let truncate = forward.uninterpretable_name.len() - forward.unparseable_name.len();
    let mut message = name.to_owned();
    if !reverse.uninterpretable_name.is_empty() {
        message.push_str(" was uninterpretable due to the following section of the name: ");
        message.push_str(&reverse.uninterpretable_name);
        if truncate <= reverse.unparseable_name.len() {
            let context = &reverse.unparseable_name[truncate..];
            if !context.is_empty() {
                message.push_str(
                    "  The following was not understandable in the context it was used: ",
                );
                message.push_str(context);
            }
        }
    } else {
        message.push_str(" has no tokens unknown to OPSIN but does not conform to its grammar. From left to right it is unparsable due to the following being uninterpretable:");
        message.push_str(&forward.uninterpretable_name);
        message.push_str(" The following of which was not parseable: ");
        message.push_str(&forward.unparseable_name);
    }
    Ok(ParsingError(message))
}
