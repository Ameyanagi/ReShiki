//! Token dictionaries and immutable OPSIN resource metadata.
use crate::{
    InitializationError,
    automaton::{self, Automaton},
    trie::Trie,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TokenDefinition {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
    pub attribute_order: Vec<(String, String)>,
    pub ignored: bool,
}

#[derive(Debug)]
pub(crate) struct SymbolTokens {
    pub symbol: u16,
    pub dictionary: Trie,
    pub reverse_dictionary: Trie,
    pub automaton: Option<Automaton>,
    pub reverse_automaton: Option<Automaton>,
}

#[derive(Debug)]
pub(crate) struct Resources {
    pub chemical: Automaton,
    pub reverse_chemical: Automaton,
    pub symbols: Vec<SymbolTokens>,
    pub definitions: BTreeMap<(String, u16), TokenDefinition>,
    pub regex_definitions: BTreeMap<u16, TokenDefinition>,
    pub suffix_rules: crate::suffix_rules::SuffixRules,
}

impl Resources {
    pub fn new() -> Result<Self, InitializationError> {
        let mut automata = automaton::load()?;
        let chemical = automata
            .remove("chemical")
            .ok_or_else(|| error("Missing chemical DFA"))?;
        let reverse_chemical = automata
            .remove("chemical-reversed")
            .ok_or_else(|| error("Missing reverse chemical DFA"))?;
        if chemical.symbols != reverse_chemical.symbols {
            return Err(error("Forward/reverse annotation alphabets differ"));
        }
        let mut symbols: Vec<_> = chemical
            .symbols
            .iter()
            .map(|&symbol| SymbolTokens {
                symbol,
                dictionary: Trie::default(),
                reverse_dictionary: Trie::default(),
                automaton: automata.remove(&format!("token-{symbol}")),
                reverse_automaton: automata.remove(&format!("token-reversed-{symbol}")),
            })
            .collect();
        if !automata.is_empty() {
            return Err(error("Unmapped OPSIN DFA tables"));
        }
        let mut definitions = BTreeMap::new();
        let index = document("index.xml")?;
        for filename in index.descendants().filter(|n| n.has_tag_name("tokenFile")) {
            let xml = document(
                filename
                    .text()
                    .ok_or_else(|| error("Empty token file name"))?,
            )?;
            for list in xml.descendants().filter(|n| n.has_tag_name("tokenList")) {
                let tag_name = required(list, "tagname")?.to_owned();
                let symbol = annotation(list)?;
                let position = chemical
                    .symbols
                    .binary_search(&symbol)
                    .map_err(|_| error("Token annotation not in grammar"))?;
                let ignored = list.attribute("ignoreWhenWritingXML") == Some("yes");
                for token in list.children().filter(|n| n.has_tag_name("token")) {
                    let mut attributes = BTreeMap::new();
                    let mut attribute_order = Vec::new();
                    for key in ["type", "subType"] {
                        if let Some(value) = list.attribute(key) {
                            attributes.insert(key.into(), value.into());
                            attribute_order.push((key.into(), value.into()));
                        }
                    }
                    for attr in token.attributes() {
                        attributes.insert(attr.name().into(), attr.value().into());
                        attribute_order.push((attr.name().into(), attr.value().into()));
                    }
                    let definition = TokenDefinition {
                        tag_name: tag_name.clone(),
                        attributes,
                        attribute_order,
                        ignored,
                    };
                    for text in split_token_text(token.text().unwrap_or_default())? {
                        symbols[position].dictionary.insert(&text);
                        symbols[position]
                            .reverse_dictionary
                            .insert(&text.chars().rev().collect::<String>());
                        definitions.insert((text, symbol), definition.clone());
                    }
                }
            }
        }
        let mut regex_definitions = BTreeMap::new();
        let xml = document("regexTokens.xml")?;
        for token in xml.descendants().filter(|n| n.has_tag_name("regexToken")) {
            let symbol = annotation(token)?;
            let mut attributes = BTreeMap::new();
            let mut attribute_order = Vec::new();
            for key in ["type", "subType", "value"] {
                if let Some(value) = token.attribute(key) {
                    attributes.insert(key.into(), value.into());
                    attribute_order.push((key.into(), value.into()));
                }
            }
            let definition = TokenDefinition {
                tag_name: required(token, "tagname")?.to_owned(),
                attributes,
                attribute_order,
                ignored: token.attribute("ignoreWhenWritingXML") == Some("yes"),
            };
            if regex_definitions.insert(symbol, definition).is_some() {
                return Err(error("Duplicate regex token annotation"));
            }
            if token.attribute("determinise") != Some("yes") && !matches!(symbol, 258 | 259) {
                return Err(error("Unsupported non-DFA token predicate"));
            }
        }
        let suffix_rules = crate::suffix_rules::SuffixRules::new()?;
        Ok(Self {
            chemical,
            reverse_chemical,
            symbols,
            definitions,
            regex_definitions,
            suffix_rules,
        })
    }

    pub fn definition(&self, text: &str, symbol: u16) -> Option<&TokenDefinition> {
        self.definitions
            .get(&(text.to_owned(), symbol))
            .or_else(|| self.regex_definitions.get(&symbol))
    }
}

pub(crate) fn xml(name: &str) -> Result<&'static str, InitializationError> {
    crate::resource_data::XML_RESOURCES
        .iter()
        .find(|(filename, _)| *filename == name)
        .map(|(_, xml)| *xml)
        .ok_or_else(|| error(&format!("Unknown embedded OPSIN resource {name}")))
}

pub(crate) fn document(name: &str) -> Result<roxmltree::Document<'static>, InitializationError> {
    roxmltree::Document::parse_with_options(
        xml(name)?,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|e| error(&format!("{name}: {e}")))
}

pub(crate) fn required<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Result<&'a str, InitializationError> {
    node.attribute(name).ok_or_else(|| {
        error(&format!(
            "Missing {name} attribute on {}",
            node.tag_name().name()
        ))
    })
}

fn annotation(node: roxmltree::Node<'_, '_>) -> Result<u16, InitializationError> {
    let mut chars = required(node, "symbol")?.encode_utf16();
    let symbol = chars.next().ok_or_else(|| error("Empty annotation"))?;
    if chars.next().is_some() {
        return Err(error("Annotation must be one UTF-16 code unit"));
    }
    Ok(symbol)
}

fn split_token_text(input: &str) -> Result<Vec<String>, InitializationError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => current.push(
                chars
                    .next()
                    .ok_or_else(|| error("Trailing escape in OPSIN token text"))?,
            ),
            '|' => {
                tokens.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    tokens.push(current);
    Ok(tokens)
}

fn error(message: &str) -> InitializationError {
    InitializationError(message.into())
}
