//! Word splitting and omitted-space recovery from OPSIN Tokeniser/WordTools.
use crate::{
    ParsingError,
    parse_rules::{self, END_OF_FUNCTIONAL_TERM, END_OF_MAIN_GROUP, ParseTokens, WordType},
    resources::Resources,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseWord {
    pub word: String,
    pub alternatives: Vec<ParseTokens>,
}

#[derive(Debug, Clone)]
pub struct Tokenization {
    pub words: Vec<ParseWord>,
    pub unparsed_name: String,
    pub uninterpretable_name: String,
    pub unparseable_name: String,
}

impl Tokenization {
    pub fn is_successful(&self) -> bool {
        self.unparsed_name.is_empty()
    }
}

pub(crate) fn tokenize(
    resources: &Resources,
    name: &str,
    remove_spaces: bool,
    reverse: bool,
) -> Result<Tokenization, ParsingError> {
    let mut unparsed = if remove_spaces && !reverse {
        remove_spaces_in_brackets(name)?
    } else {
        name.to_owned()
    };
    let mut words = Vec::new();
    let mut saved_error: Option<(String, String, String)> = None;
    while !unparsed.is_empty() {
        let results = parse_rules::parse_word(resources, &unparsed, reverse)?;
        let remaining = &results.uninterpretable_name;
        let parsed_name = if reverse {
            &unparsed[remaining.len()..]
        } else {
            &unparsed[..unparsed.len() - remaining.len()]
        }
        .to_owned();
        let at_boundary = remaining.is_empty()
            || if reverse {
                remaining.ends_with([' ', '-'])
            } else {
                remaining.starts_with([' ', '-'])
            };
        if !results.parses.is_empty() && at_boundary {
            let mut parsed_words = split_into_words(results.parses, &parsed_name);
            if reverse {
                parsed_words.reverse();
            }
            words.extend(parsed_words);
            unparsed = remove_delimiter(remaining, reverse);
            saved_error = None;
            continue;
        }
        if saved_error.is_none() {
            saved_error = Some((
                unparsed.clone(),
                remaining.clone(),
                results.unparseable_name.clone(),
            ));
        }
        let mut fixed = false;
        if !reverse {
            let lower = remaining.to_ascii_lowercase();
            let phrase = ["compd. with ", "compound with ", "and "]
                .iter()
                .find(|phrase| lower.starts_with(**phrase));
            let last_word_full = words
                .last()
                .map(|word: &ParseWord| {
                    word.alternatives.iter().any(|tokens| {
                        matches!(
                            tokens.word_type(),
                            Ok(WordType::Full | WordType::FunctionalTerm)
                        )
                    })
                })
                .unwrap_or(false);
            if let Some(phrase) = phrase.filter(|_| last_word_full) {
                unparsed = format!("{parsed_name}{}", &remaining[phrase.len()..]);
                fixed = true;
            } else if is_cas_collective_index(remaining) {
                unparsed = parsed_name.to_owned();
                fixed = true;
            } else if remove_spaces {
                if let Some(previous) = words.last() {
                    let joined = format!("{}{unparsed}", previous.word);
                    let back = parse_rules::parse_word(resources, &joined, false)?;
                    let back_name = &joined[..joined.len() - back.uninterpretable_name.len()];
                    if back_name.len() > previous.word.len()
                        && !back.parses.is_empty()
                        && (back.uninterpretable_name.is_empty()
                            || back.uninterpretable_name.starts_with([' ', '-']))
                    {
                        words.pop();
                        words.extend(split_into_words(back.parses, back_name));
                        unparsed = if back.uninterpretable_name.is_empty() {
                            String::new()
                        } else {
                            back.uninterpretable_name[1..].to_owned()
                        };
                        fixed = true;
                    }
                }
                if !fixed && let Some(space) = remaining.find(' ') {
                    unparsed = format!(
                        "{parsed_name}{}{}",
                        &remaining[..space],
                        &remaining[space + 1..]
                    );
                    fixed = true;
                }
            }
        } else if remove_spaces && let Some(space) = remaining.rfind(' ') {
            unparsed = format!(
                "{}{}{parsed_name}",
                &remaining[..space],
                &remaining[space + 1..]
            );
            fixed = true;
        }
        if !fixed {
            let (unparsed_name, uninterpretable_name, unparseable_name) =
                saved_error.expect("failure saved before recovery");
            if reverse {
                words.reverse();
            }
            return Ok(Tokenization {
                words,
                unparsed_name,
                uninterpretable_name,
                unparseable_name,
            });
        }
    }
    if reverse {
        words.reverse();
    }
    Ok(Tokenization {
        words,
        unparsed_name: String::new(),
        uninterpretable_name: String::new(),
        unparseable_name: String::new(),
    })
}

fn remove_delimiter(remaining: &str, reverse: bool) -> String {
    if remaining.is_empty() {
        return String::new();
    }
    if reverse {
        let length = if remaining.len() > 3 && remaining.ends_with(" - ") {
            3
        } else {
            1
        };
        remaining[..remaining.len() - length].into()
    } else {
        let length = if remaining.len() > 3 && remaining.starts_with(" - ") {
            3
        } else {
            1
        };
        remaining[length..].into()
    }
}

pub(crate) fn split_into_words(alternatives: Vec<ParseTokens>, name: &str) -> Vec<ParseWord> {
    let mut well_formed = Vec::new();
    let mut splits: Vec<Vec<ParseTokens>> = Vec::new();
    let mut least_words = usize::MAX;
    let mut longest_functional = 0;
    for alternative in alternatives {
        let chunks = alternative.chunks();
        let omitted_space = chunks.len() > 1
            && chunks.iter().any(|chunk| {
                chunk
                    .last()
                    .is_some_and(|t| t.annotation == END_OF_FUNCTIONAL_TERM)
            });
        if !omitted_space {
            well_formed.push(alternative);
            continue;
        }
        let mut words = Vec::new();
        let mut new_tokens = Vec::new();
        let mut current_functional_length = 0;
        let mut position = 0;
        for chunk in chunks {
            let end = chunk.last().expect("nonempty chunk").annotation;
            if end == END_OF_FUNCTIONAL_TERM && !new_tokens.is_empty() {
                words.push(ParseTokens {
                    tokens: std::mem::take(&mut new_tokens),
                });
            }
            new_tokens.extend_from_slice(chunk);
            position += chunk.len();
            if matches!(end, END_OF_MAIN_GROUP | END_OF_FUNCTIONAL_TERM)
                || position == alternative.tokens.len()
            {
                let tokens = ParseTokens {
                    tokens: std::mem::take(&mut new_tokens),
                };
                if end == END_OF_FUNCTIONAL_TERM {
                    current_functional_length = tokens.text().len();
                }
                words.push(tokens);
            }
        }
        if words.len() <= least_words {
            if words.len() < least_words {
                splits.clear();
                least_words = words.len();
                longest_functional = 0;
            }
            if current_functional_length >= longest_functional {
                if current_functional_length > longest_functional {
                    splits.clear();
                    longest_functional = current_functional_length;
                }
                splits.push(words);
            }
        }
    }
    if !well_formed.is_empty() {
        return vec![ParseWord {
            word: name.into(),
            alternatives: well_formed,
        }];
    }
    (0..least_words.min(splits.first().map_or(0, Vec::len)))
        .map(|i| {
            let mut alternatives = Vec::new();
            for split in &splits {
                if !alternatives.contains(&split[i]) {
                    alternatives.push(split[i].clone());
                }
            }
            ParseWord {
                word: alternatives[0].text(),
                alternatives,
            }
        })
        .collect()
}

fn remove_spaces_in_brackets(name: &str) -> Result<String, ParsingError> {
    let mut level = 0i32;
    let mut output = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '(' | '[' | '{' => level += 1,
            ')' | ']' | '}' => level -= 1,
            _ => {}
        }
        if ch != ' ' || level <= 0 {
            output.push(ch);
        }
    }
    if level > 0 {
        return Err(ParsingError(format!(
            "Unmatched opening bracket found in :{output}"
        )));
    }
    if level < 0 {
        return Err(ParsingError(format!(
            "Unmatched closing bracket found in :{output}"
        )));
    }
    Ok(output)
}

pub(crate) fn is_cas_collective_index(text: &str) -> bool {
    let bytes = text.as_bytes();
    fn term(bytes: &[u8], offset: &mut usize) -> bool {
        let start = *offset;
        if !bytes.get(*offset).is_some_and(|b| matches!(b, b'1'..=b'9')) {
            return false;
        }
        *offset += 1;
        if bytes.get(*offset).is_some_and(u8::is_ascii_digit) {
            *offset += 1;
        }
        if bytes
            .get(*offset..*offset + 2)
            .is_some_and(|word| word.eq_ignore_ascii_case(b"ci"))
        {
            *offset += 2;
            true
        } else {
            *offset = start;
            false
        }
    }
    let mut i = 0;
    if term(bytes, &mut i) && i == bytes.len() {
        return true;
    }
    i = 0;
    while i < bytes.len() {
        if !matches!(bytes[i], b'[' | b'(' | b'{') {
            return false;
        }
        i += 1;
        if !term(bytes, &mut i) {
            return false;
        }
        loop {
            if bytes.get(i).is_some_and(|b| matches!(b, b',' | b' ')) {
                i += 1;
            }
            if !term(bytes, &mut i) {
                break;
            }
        }
        if !bytes
            .get(i)
            .is_some_and(|b| matches!(b, b']' | b')' | b'}'))
        {
            return false;
        }
        i += 1;
    }
    !bytes.is_empty()
}
