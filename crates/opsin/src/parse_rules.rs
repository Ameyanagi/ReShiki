//! The left/right DFS annotation search in upstream ParseRules/ReverseParseRules.
use crate::{ParsingError, resources::Resources};
use std::collections::BTreeMap;

pub const END_OF_MAIN_GROUP: u16 = 226;
pub const END_OF_SUBSTITUENT: u16 = 233;
pub const END_OF_FUNCTIONAL_TERM: u16 = 251;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordType {
    Full,
    Substituent,
    FunctionalTerm,
    Polymer,
}

impl WordType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Substituent => "substituent",
            Self::FunctionalTerm => "functionalTerm",
            Self::Polymer => "polymer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub annotation: u16,
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
    pub attribute_order: Vec<(String, String)>,
    pub ignored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTokens {
    pub tokens: Vec<Token>,
}

impl ParseTokens {
    pub fn text(&self) -> String {
        self.tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect()
    }
    pub fn word_type(&self) -> Result<WordType, ParsingError> {
        match self.tokens.last().map(|t| t.annotation) {
            Some(END_OF_MAIN_GROUP) => Ok(WordType::Full),
            Some(END_OF_SUBSTITUENT) => Ok(WordType::Substituent),
            Some(END_OF_FUNCTIONAL_TERM) => Ok(WordType::FunctionalTerm),
            _ => Err(ParsingError(
                "OPSIN bug: annotations do not terminate in a word marker".into(),
            )),
        }
    }
    pub fn chunks(&self) -> Vec<&[Token]> {
        let mut start = 0;
        let mut chunks = Vec::new();
        for (i, token) in self.tokens.iter().enumerate() {
            if matches!(
                token.annotation,
                END_OF_MAIN_GROUP | END_OF_SUBSTITUENT | END_OF_FUNCTIONAL_TERM
            ) {
                chunks.push(&self.tokens[start..=i]);
                start = i + 1;
            }
        }
        chunks
    }
}

#[derive(Debug, Clone)]
pub struct ParseRulesResult {
    pub parses: Vec<ParseTokens>,
    pub uninterpretable_name: String,
    pub unparseable_name: String,
}

#[derive(Debug, Clone, Copy)]
struct AnnotationState {
    state: usize,
    annotation: u16,
    position: usize,
    case_sensitive: bool,
    previous: Option<usize>,
}

pub(crate) fn parse_word(
    resources: &Resources,
    input: &str,
    reverse: bool,
) -> Result<ParseRulesResult, ParsingError> {
    if !input.is_ascii() {
        return Err(ParsingError(
            "Chemical words must be normalized before annotation".into(),
        ));
    }
    let lower = input.to_ascii_lowercase();
    let reversed_input: Vec<u8> = input.bytes().rev().collect();
    let reversed_lower: Vec<u8> = lower.bytes().rev().collect();
    let chemical = if reverse {
        &resources.reverse_chemical
    } else {
        &resources.chemical
    };
    let mut arena = vec![AnnotationState {
        state: chemical.initial,
        annotation: 0,
        position: if reverse { input.len() } else { 0 },
        case_sensitive: true,
        previous: None,
    }];
    let mut stack = vec![0];
    let mut best_position = arena[0].position;
    let mut longest = 0;
    let mut successful = Vec::new();
    while let Some(index) = stack.pop() {
        let current = arena[index];
        let farther = |first: usize, second: usize| {
            if reverse {
                first < second
            } else {
                first > second
            }
        };
        if chemical.is_accept(current.state)
            && (current.position == best_position || farther(current.position, best_position))
        {
            if farther(current.position, best_position) {
                successful.clear();
                best_position = current.position;
            } else if successful.len() > 128 {
                return Err(ParsingError("Ambiguity in OPSIN's chemical grammar has produced more than 128 annotations. Parsing has been aborted. Please report this as a bug".into()));
            }
            successful.push(index);
        }
        if farther(current.position, arena[longest].position) {
            longest = index;
        }
        for symbol_tokens in &resources.symbols {
            let annotation = symbol_tokens.symbol;
            let Some(state) = chemical.step(current.state, annotation) else {
                continue;
            };
            let mut push = |position, case_sensitive| {
                stack.push(arena.len());
                arena.push(AnnotationState {
                    state,
                    annotation,
                    position,
                    case_sensitive,
                    previous: Some(index),
                });
            };
            let (dictionary, word, offset) = if reverse {
                (
                    &symbol_tokens.reverse_dictionary,
                    reversed_lower.as_slice(),
                    input.len() - current.position,
                )
            } else {
                (
                    &symbol_tokens.dictionary,
                    lower.as_bytes(),
                    current.position,
                )
            };
            for end in dictionary.matches(word, offset) {
                push(if reverse { input.len() - end } else { end }, false);
            }
            let automaton = if reverse {
                &symbol_tokens.reverse_automaton
            } else {
                &symbol_tokens.automaton
            };
            if let Some(automaton) = automaton {
                let word = if reverse {
                    reversed_input.as_slice()
                } else {
                    input.as_bytes()
                };
                if let Some(length) = automaton.run(word, offset) {
                    push(
                        if reverse {
                            current.position - length
                        } else {
                            current.position + length
                        },
                        true,
                    );
                }
            }
            // The only two Java Pattern tokens use transparent-boundary lookarounds.
            let boundary_matches = match annotation {
                258 => {
                    current.position == 0
                        || !input.as_bytes()[current.position - 1].is_ascii_alphabetic()
                }
                259 => {
                    current.position == input.len()
                        || !input.as_bytes()[current.position].is_ascii_alphabetic()
                }
                _ => false,
            };
            if boundary_matches {
                push(current.position, true);
            }
        }
    }
    let mut parses = Vec::with_capacity(successful.len());
    for index in successful {
        let mut index = index;
        let mut tokens = Vec::new();
        while let Some(previous) = arena[index].previous {
            let current = arena[index];
            let bounds = if reverse {
                current.position..arena[previous].position
            } else {
                arena[previous].position..current.position
            };
            let text = if current.case_sensitive {
                &input[bounds]
            } else {
                &lower[bounds]
            };
            let definition = resources
                .definition(text, current.annotation)
                .ok_or_else(|| {
                    ParsingError(format!(
                        "No token definition for {text:?} annotation {}",
                        current.annotation
                    ))
                })?;
            tokens.push(Token {
                text: text.to_owned(),
                annotation: current.annotation,
                tag_name: definition.tag_name.clone(),
                attributes: definition.attributes.clone(),
                ignored: definition.ignored,
                attribute_order: definition.attribute_order.clone(),
            });
            index = previous;
        }
        if !reverse {
            tokens.reverse();
        }
        parses.push(ParseTokens { tokens });
    }
    let uninterpretable_name = if parses.is_empty() {
        input
    } else if reverse {
        &input[..best_position]
    } else {
        &input[best_position..]
    };
    let unparseable_name = if reverse {
        &input[..arena[longest].position]
    } else {
        &input[arena[longest].position..]
    };
    Ok(ParseRulesResult {
        parses,
        uninterpretable_name: uninterpretable_name.into(),
        unparseable_name: unparseable_name.into(),
    })
}
