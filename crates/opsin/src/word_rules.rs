//! Ordered word-rule matching and parser rewrites from OPSIN `WordRules.java`.
//! OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::{
    ParseOptions, ParsingError,
    graph::Element as ChemEl,
    parse_tree::{Arena, NodeId},
    resources, valence,
};

#[derive(Debug, Clone)]
struct WordDescription {
    word_type: String,
    value: Option<String>,
    functional_type: Option<String>,
    functional_sub_type: Option<String>,
    ends_with: Option<String>,
    ends_with_regex: Option<String>,
    group_type: Option<String>,
    group_sub_type: Option<String>,
}

#[derive(Debug, Clone)]
struct RuleDescription {
    name: String,
    word_type: String,
    words: Vec<WordDescription>,
}

#[derive(Debug, Clone)]
pub(crate) struct WordRules {
    rules: Vec<RuleDescription>,
}

fn error(message: impl Into<String>) -> ParsingError {
    ParsingError(message.into())
}

impl WordRules {
    pub(crate) fn new() -> Result<Self, ParsingError> {
        let document = resources::document("wordRules.xml").map_err(|e| error(e.to_string()))?;
        let mut rules = Vec::new();
        for node in document
            .descendants()
            .filter(|n| n.has_tag_name("wordRule"))
        {
            let attr = |name| {
                node.attribute(name)
                    .map(str::to_owned)
                    .ok_or_else(|| error(format!("Malformed wordRule: missing {name}")))
            };
            let mut words = Vec::new();
            for word in node.children().filter(|n| n.has_tag_name("word")) {
                let get = |name| word.attribute(name).map(str::to_owned);
                let word_type =
                    get("type").ok_or_else(|| error("Malformed wordRule, no type specified"))?;
                let ends_with_regex = get("endsWithRegex");
                if let Some(pattern) = &ends_with_regex {
                    validate_ending_pattern(pattern)?;
                }
                words.push(WordDescription {
                    word_type,
                    value: get("value"),
                    functional_type: get("functionalGroupType"),
                    functional_sub_type: get("functionalGroupSubType"),
                    ends_with: get("endsWith"),
                    ends_with_regex,
                    group_type: get("endsWithGroupType"),
                    group_sub_type: get("endsWithGroupSubType"),
                });
            }
            rules.push(RuleDescription {
                name: attr("name")?,
                word_type: attr("type")?,
                words,
            });
        }
        Ok(Self { rules })
    }

    pub(crate) fn group(
        &self,
        arena: &mut Arena,
        molecule: NodeId,
        options: &ParseOptions,
        allow_space_removal: bool,
        component_ratios: Option<&[i32]>,
    ) -> Result<(), ParsingError> {
        let mut words = arena.children_named(molecule, "word");
        let mut index = 0;
        while index < words.len() {
            if self.match_rule(
                arena,
                molecule,
                &mut words,
                index,
                options.allow_radicals,
                allow_space_removal,
                component_ratios.map(<[i32]>::len),
            )? {
                index = 0;
            } else {
                index += 1;
            }
        }
        for &child in &arena[molecule].children {
            if arena[child].name != "wordRule" {
                return Err(error(format!(
                    "Unable to assign wordRule to: {}",
                    arena[child].attribute("value").unwrap_or("null")
                )));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn match_rule(
        &self,
        arena: &mut Arena,
        molecule: NodeId,
        words: &mut Vec<NodeId>,
        first: usize,
        allow_radicals: bool,
        allow_space_removal: bool,
        expected_components: Option<usize>,
    ) -> Result<bool, ParsingError> {
        'rule: for rule in &self.rules {
            let mut count = rule.words.len();
            if first + count > words.len() {
                continue;
            }
            for (offset, description) in rule.words.iter().enumerate() {
                if !word_matches(arena, words[first + offset], description)? {
                    continue 'rule;
                }
            }
            let word_rule = arena.grouping("wordRule");
            arena[word_rule].add_attribute("type", &rule.word_type);
            arena[word_rule].add_attribute("wordRule", &rule.name);
            match rule.name.as_str() {
                "functionGroupAsGroup" => {
                    let functional = words[first + count - 1];
                    if arena[functional].attribute("type") != Some("functionalTerm") || count > 2 {
                        return Err(error(
                            "OPSIN bug: Problem with functionGroupAsGroup wordRule",
                        ));
                    }
                    convert_functional_group(arena, functional)?;
                    if count == 2 {
                        join_words(arena, words, words[first], functional)?;
                        count = 1;
                    }
                    arena[word_rule].set_attribute("wordRule", "simple");
                }
                "carbonylDerivative" | "acidReplacingFunctionalGroup" => {
                    let mut offset = 1;
                    while offset < count - 1 {
                        if arena[words[first + offset]].attribute("type") == Some("substituent") {
                            let word = words[first + offset];
                            join_words(arena, words, word, words[first + offset + 1])?;
                            count -= 1;
                            let terms = arena.descendants_named(word, "functionalTerm");
                            if terms.len() != 1 {
                                return Err(error(format!(
                                    "OPSIN bug: Problem with {} wordRule",
                                    rule.name
                                )));
                            }
                            arena[terms[0]].name = "root".into();
                            let groups = arena.descendants_named(terms[0], "functionalGroup");
                            if groups.len() != 1 {
                                return Err(error(format!(
                                    "OPSIN bug: Problem with {} wordRule",
                                    rule.name
                                )));
                            }
                            arena[groups[0]].name = "group".into();
                            arena[word].set_attribute("type", "full");
                        }
                        offset += 1;
                    }
                }
                "additionCompound" | "oxide" => {
                    let elementary_word = words[first];
                    let atoms: Vec<_> = arena
                        .descendants_named(elementary_word, "group")
                        .into_iter()
                        .filter(|&id| arena[id].attribute("type") == Some("elementaryAtom"))
                        .collect();
                    if atoms.len() == 1 {
                        let atom = atoms[0];
                        let element = elementary_atom_element(arena, atom)?;
                        if rule.name == "oxide" {
                            if count != 2 {
                                return Err(error("OPSIN bug: Problem with oxide wordRule"));
                            }
                            let oxide = words[first + 1];
                            let oxide_element = functional_element(arena, oxide)?;
                            if !is_covalent(element, oxide_element) || element == ChemEl::Ag {
                                let group = convert_functional_group(arena, oxide)?;
                                set_oxide_structure(arena, group, atom)?;
                                apply_single_rule(arena, words, first, elementary_word, "simple");
                                continue 'rule;
                            }
                        } else {
                            for offset in 1..count {
                                let functional_word = words[first + offset];
                                let second = functional_element(arena, functional_word)?;
                                if !is_covalent(element, second) {
                                    let special = if second.is_halogen() && count == 2 {
                                        match element {
                                            ChemEl::Mg => arena[elementary_word].children.len() > 1,
                                            ChemEl::Al => matches!(
                                                second,
                                                ChemEl::Cl | ChemEl::Br | ChemEl::I
                                            ),
                                            ChemEl::Ti => {
                                                matches!(
                                                    second,
                                                    ChemEl::Cl | ChemEl::Br | ChemEl::I
                                                ) && oxidation_number_or_multiplier(
                                                    arena,
                                                    atom,
                                                    functional_word,
                                                    4,
                                                )?
                                            }
                                            ChemEl::V => {
                                                second == ChemEl::Cl
                                                    && oxidation_number_or_multiplier(
                                                        arena,
                                                        atom,
                                                        functional_word,
                                                        4,
                                                    )?
                                            }
                                            ChemEl::Zr | ChemEl::Hf => {
                                                second == ChemEl::Br
                                                    && oxidation_number_or_multiplier(
                                                        arena,
                                                        atom,
                                                        functional_word,
                                                        4,
                                                    )?
                                            }
                                            ChemEl::U => {
                                                matches!(second, ChemEl::F | ChemEl::Cl)
                                                    && oxidation_number_or_multiplier(
                                                        arena,
                                                        atom,
                                                        functional_word,
                                                        6,
                                                    )?
                                            }
                                            ChemEl::Np | ChemEl::Pu => {
                                                second == ChemEl::F
                                                    && oxidation_number_or_multiplier(
                                                        arena,
                                                        atom,
                                                        functional_word,
                                                        6,
                                                    )?
                                            }
                                            _ => false,
                                        }
                                    } else {
                                        matches!(second, ChemEl::H | ChemEl::C)
                                            && count == 2
                                            && element == ChemEl::Al
                                    };
                                    if !special {
                                        continue 'rule;
                                    }
                                }
                            }
                        }
                    }
                }
                "potentialAlcoholEster" => {
                    if expected_components == Some(arena[molecule].children.len()) {
                        continue;
                    }
                    let last = first + count - 1;
                    if arena[words[last]].attribute("isSalt").is_some() {
                        continue;
                    }
                    if let Some(&next) = words.get(last + 1)
                        && arena[next].attribute("type") == Some("functionalTerm")
                        && arena[next]
                            .attribute("value")
                            .is_some_and(|v| v.eq_ignore_ascii_case("salt"))
                    {
                        continue;
                    }
                }
                "monovalentFunctionalGroup" => {
                    let last = arena
                        .last_leaf(words[0])
                        .ok_or_else(|| error("OPSIN bug: Empty word"))?;
                    if matches!(arena[last].value.as_deref(), Some("oxy" | "oxo")) {
                        return Err(error(format!(
                            "{}{} is unlikely to be intended to be a molecule",
                            arena.value(words[0]),
                            arena.value(words[1])
                        )));
                    }
                }
                _ => {}
            }
            let parent = arena[words[first]].parent.expect("Word has no parent");
            let insertion = arena
                .index_of(parent, words[first])
                .expect("Word absent from parent");
            let mut values = Vec::new();
            for _ in 0..count {
                let word = words.remove(first);
                arena.detach(word);
                arena.add_child(word_rule, word);
                values.push(arena[word].attribute("value").unwrap_or("null").to_owned());
            }
            arena[word_rule].add_attribute("value", values.join(" "));
            arena.insert_child(parent, word_rule, insertion);
            words.insert(first, word_rule);
            return Ok(true);
        }
        let word = words[first];
        if arena[word].name == "word" && arena[word].attribute("type") == Some("full") {
            apply_single_rule(arena, words, first, word, "simple");
            return Ok(false);
        } else if allow_space_removal && arena[word].attribute("type") == Some("substituent") {
            if let Some(&next) = words.get(first + 1)
                && matches!(arena[next].attribute("type"), Some("full" | "substituent"))
            {
                join_words(arena, words, word, next)?;
                return Ok(true);
            }
        } else if arena[word].attribute("type") == Some("functionalTerm")
            && arena[word]
                .attribute("value")
                .is_some_and(|v| v.eq_ignore_ascii_case("salt"))
        {
            if first == 0 {
                return Err(error("The word salt appeared in an unexpected location"));
            }
            if arena[words[first - 1]].attribute("isSalt").is_none() {
                arena[words[first - 1]].add_attribute("isSalt", "yes");
            }
            words.remove(first);
            arena.detach(word);
            if arena[molecule].attribute("isSalt").is_none() {
                arena[molecule].add_attribute("isSalt", "yes");
            }
            return Ok(true);
        }
        if words.len() == 1
            && first == 0
            && arena[word].name == "word"
            && arena[word].attribute("type") == Some("substituent")
        {
            if arena[word]
                .attribute("value")
                .is_some_and(|v| v.eq_ignore_ascii_case("dihydrogen"))
            {
                convert_dihydrogen(arena, word);
                return Ok(true);
            }
            if allow_radicals {
                apply_single_rule(arena, words, first, word, "substituent");
            }
        }
        Ok(false)
    }
}

fn word_matches(
    arena: &Arena,
    word: NodeId,
    description: &WordDescription,
) -> Result<bool, ParsingError> {
    if arena[word].attribute("type") != Some(description.word_type.as_str()) {
        return Ok(false);
    }
    if description.functional_type.is_some() || description.functional_sub_type.is_some() {
        if arena[word].attribute("type") != Some("functionalTerm") {
            return Ok(false);
        }
        let mut last = arena.last_leaf(word);
        while let Some(id) = last {
            if !matches!(
                arena[id].name.as_str(),
                "closebracket" | "structuralCloseBracket"
            ) {
                break;
            }
            last = arena.previous_sibling(id);
        }
        let last = last.ok_or_else(|| {
            error("OPSIN Bug: Cannot find the functional element in a functionalTerm")
        })?;
        if description
            .functional_type
            .as_deref()
            .is_some_and(|v| arena[last].attribute("type") != Some(v))
            || description
                .functional_sub_type
                .as_deref()
                .is_some_and(|v| arena[last].attribute("subType") != Some(v))
        {
            return Ok(false);
        }
    }
    if let Some(predicate) = &description.ends_with
        && !ends_with_group(arena, word, predicate)
    {
        return Ok(false);
    }
    let value = arena[word].attribute("value").unwrap_or("null");
    if description
        .value
        .as_deref()
        .is_some_and(|v| value.to_ascii_lowercase() != v)
    {
        return Ok(false);
    }
    if description
        .ends_with_regex
        .as_deref()
        .is_some_and(|pattern| !ending_pattern_matches(pattern, value))
    {
        return Ok(false);
    }
    if description.group_type.is_some() || description.group_sub_type.is_some() {
        let last_group = last_group(arena, word);
        let Some(group) = last_group else {
            return Ok(false);
        };
        if description
            .group_type
            .as_deref()
            .is_some_and(|v| arena[group].attribute("type") != Some(v))
            || description
                .group_sub_type
                .as_deref()
                .is_some_and(|v| arena[group].attribute("subType") != Some(v))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn acid_ending(value: &str) -> bool {
    ["ic", "ous", "icacid", "ousacid", "ic acid", "ous acid"]
        .iter()
        .any(|ending| value.ends_with(ending))
}
fn ate_ending(value: &str) -> bool {
    ["at", "ate", "it", "ite", "amid", "amide"]
        .iter()
        .any(|ending| value.ends_with(ending))
}

fn ends_with_group(arena: &Arena, word: NodeId, predicate: &str) -> bool {
    let mut last = arena.last_leaf(word);
    while let Some(id) = last {
        if !matches!(
            arena[id].name.as_str(),
            "closebracket" | "structuralCloseBracket" | "isotopeSpecification"
        ) {
            break;
        }
        last = arena.previous_sibling(id);
    }
    let Some(mut id) = last else {
        return false;
    };
    if predicate == "acid" {
        if arena[id].name == "suffix" {
            return acid_ending(arena[id].attribute("value").unwrap_or(""));
        }
        if arena[id].name == "group" {
            return arena[id].attribute("functionalIDs").is_some()
                && (acid_ending(&arena.value(id))
                    || arena[id].attribute("type") == Some("aminoAcid"));
        }
    } else if predicate == "ateGroup" {
        if arena[id].name == "group" {
            return arena[id].attribute("functionalIDs").is_some() && ate_ending(&arena.value(id));
        }
        while arena[id].name == "suffix" {
            if arena[id]
                .attribute("value")
                .is_some_and(|v| ate_ending(v) || v == "glycoside")
            {
                return true;
            }
            let Some(previous) = arena.previous_sibling_named(id, "suffix") else {
                break;
            };
            id = previous;
        }
    }
    false
}

fn last_group(arena: &Arena, word: NodeId) -> Option<NodeId> {
    let last = arena.last_leaf(word)?;
    if arena[last].name == "group" {
        return Some(last);
    }
    arena
        .children_named(arena[last].parent?, "group")
        .last()
        .copied()
}

fn apply_single_rule(
    arena: &mut Arena,
    words: &mut [NodeId],
    index: usize,
    word: NodeId,
    rule: &str,
) {
    let parent = arena[word].parent.expect("Word has no parent");
    let insertion = arena
        .index_of(parent, word)
        .expect("Word absent from parent");
    let group = arena.grouping("wordRule");
    arena[group].add_attribute("wordRule", rule);
    arena[group].add_attribute("type", "full");
    let value = arena[word].attribute("value").unwrap_or("null").to_owned();
    arena[group].add_attribute("value", value);
    arena.detach(word);
    arena.add_child(group, word);
    words[index] = group;
    arena.insert_child(parent, group, insertion);
}

fn join_words(
    arena: &mut Arena,
    words: &mut Vec<NodeId>,
    first: NodeId,
    second: NodeId,
) -> Result<(), ParsingError> {
    if let Some(index) = words.iter().position(|&id| id == second) {
        words.remove(index);
    }
    arena.detach(second);
    let subs = arena.children_named(first, "substituent");
    let Some(&final_sub) = subs.last() else {
        return Err(error(
            "OPSIN Bug: Substituent element not found where substituent element expected",
        ));
    };
    if arena
        .last_child(final_sub)
        .is_none_or(|id| arena[id].name != "hyphen")
    {
        let hyphen = arena.token("hyphen", "-");
        arena.add_child(final_sub, hyphen);
    }
    let children = arena[second].children.clone();
    for &child in children.iter().rev() {
        arena.detach(child);
        arena.insert_after(final_sub, child);
    }
    if arena[second].attribute("type") == Some("full") {
        arena[first].set_attribute("type", "full");
    }
    let value = format!(
        "{}{}",
        arena[first].attribute("value").unwrap_or("null"),
        arena[second].attribute("value").unwrap_or("null")
    );
    arena[first].set_attribute("value", value);
    Ok(())
}

fn convert_functional_group(arena: &mut Arena, word: NodeId) -> Result<NodeId, ParsingError> {
    arena[word].set_attribute("type", "full");
    let terms = arena.descendants_named(word, "functionalTerm");
    if terms.len() != 1 {
        return Err(error(
            "OPSIN Bug: Exactly 1 functionalTerm expected in functionalGroupAsGroup wordRule",
        ));
    }
    arena[terms[0]].name = "root".into();
    let groups = arena.children_named(terms[0], "functionalGroup");
    if groups.len() != 1 {
        return Err(error(
            "OPSIN Bug: Exactly 1 functionalGroup expected in functionalGroupAsGroup wordRule",
        ));
    }
    let group = groups[0];
    arena[group].name = "group".into();
    arena[group].set_attribute("type", "simpleGroup");
    arena[group].add_attribute("subType", "simpleGroup");
    Ok(group)
}

fn convert_dihydrogen(arena: &mut Arena, word: NodeId) {
    arena[word].set_attribute("type", "full");
    for child in arena[word].children.clone() {
        arena.detach(child);
    }
    let root = arena.grouping("root");
    let group = arena.token("group", "dihydrogen");
    arena[group].add_attribute("type", "simpleGroup");
    arena[group].add_attribute("subType", "simpleGroup");
    arena[group].add_attribute("value", "[H][H]");
    arena.add_child(root, group);
    arena.add_child(word, root);
}

fn integer_attribute(arena: &Arena, id: NodeId, name: &str) -> Result<i32, ParsingError> {
    arena[id]
        .attribute(name)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| error(format!("OPSIN bug: Invalid {name} on {}", arena[id].name)))
}

fn oxidation_number_or_multiplier(
    arena: &Arena,
    elementary: NodeId,
    functional_word: NodeId,
    expected: i32,
) -> Result<bool, ParsingError> {
    let groups = arena.descendants_named(functional_word, "functionalGroup");
    if groups.len() != 1 {
        return Err(error(
            "OPSIN bug: Unable to find functional group in oxide or addition compound rule",
        ));
    }
    if let Some(multiplier) = arena.previous_sibling(groups[0])
        && arena[multiplier].name == "multiplier"
    {
        return Ok(integer_attribute(arena, multiplier, "value")? == expected);
    }
    if let Some(oxidation) = arena.next_sibling(elementary)
        && arena[oxidation].name == "oxidationNumberSpecifier"
    {
        return Ok(integer_attribute(arena, oxidation, "value")? == expected);
    }
    Ok(false)
}

fn elementary_atom_element(arena: &Arena, id: NodeId) -> Result<ChemEl, ParsingError> {
    let smiles = arena[id].attribute("value").unwrap_or("");
    let symbol = if let Some(inner) = smiles.strip_prefix('[') {
        let start = inner
            .find(|ch: char| ch.is_ascii_alphabetic())
            .ok_or_else(|| error("OPSIN bug: Elementary atom has no element"))?;
        let length = if inner
            .as_bytes()
            .get(start + 1)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            2
        } else {
            1
        };
        &inner[start..start + length]
    } else {
        smiles
    };
    ChemEl::from_symbol(symbol)
        .ok_or_else(|| error(format!("OPSIN bug: Unrecognised elementary atom {symbol}")))
}

fn functional_element(arena: &Arena, word: NodeId) -> Result<ChemEl, ParsingError> {
    let groups = arena.descendants_named(word, "functionalGroup");
    if groups.len() != 1 {
        return Err(error(
            "OPSIN bug: Unable to find functional group in oxide or addition compound rule",
        ));
    }
    let smiles = arena[groups[0]].attribute("value").unwrap_or("");
    let start = smiles
        .find(|ch: char| ch.is_ascii_uppercase())
        .ok_or_else(|| error("OPSIN bug: Functional group has no element"))?;
    let length = if smiles
        .as_bytes()
        .get(start + 1)
        .is_some_and(u8::is_ascii_lowercase)
    {
        2
    } else {
        1
    };
    ChemEl::from_symbol(&smiles[start..start + length])
        .ok_or_else(|| error("OPSIN bug: Unrecognised functional group element"))
}

fn is_covalent(first: ChemEl, second: ChemEl) -> bool {
    let (Some(first), Some(second)) = (
        valence::pauling_electronegativity(first),
        valence::pauling_electronegativity(second),
    ) else {
        return false;
    };
    let mean = (first + second) / 2.0;
    if mean < 1.6 {
        return false;
    }
    (first - second).abs() < 1.76 * mean - 3.03
}

fn set_oxide_structure(
    arena: &mut Arena,
    oxide: NodeId,
    elementary: NodeId,
) -> Result<(), ParsingError> {
    let mut chain = false;
    let mut multiplier_value = 0;
    let multiplier = arena
        .previous_sibling(oxide)
        .filter(|&id| arena[id].name == "multiplier");
    if let Some(multiplier) = multiplier {
        multiplier_value = integer_attribute(arena, multiplier, "value")?;
        if multiplier_value > 1 {
            let max = arena[elementary]
                .attribute("commonOxidationStatesAndMax")
                .and_then(|v| v.split(':').nth(1))
                .and_then(|v| v.parse::<i32>().ok());
            chain = max.is_none_or(|max| max <= 2);
        }
    }
    let smiles = arena[oxide].attribute("value").unwrap_or("");
    let element = if smiles == "O" {
        "O"
    } else if smiles == "S" {
        "S"
    } else if smiles.starts_with("[Se") {
        "Se"
    } else if smiles.starts_with("[Te") {
        "Te"
    } else {
        return Err(error(format!(
            "OPSIN Bug: Unexpected smiles for oxideGroup: {smiles}"
        )));
    };
    let value = if chain {
        let mut value = format!("[{element}-]");
        for _ in 2..multiplier_value {
            value.push_str(&format!("[{element}]"));
        }
        value.push_str(&format!("[{element}-]"));
        arena.detach(multiplier.expect("Chain oxide has multiplier"));
        value
    } else {
        format!("[{element}-2]")
    };
    arena[oxide].set_attribute("value", value);
    Ok(())
}

// These are all five distinct Java Pattern expressions in pinned wordRules.xml.
// Matching them directly retains its lookbehind semantics without a second
// regex engine; unexpected resource patterns are rejected on initialization.
fn validate_ending_pattern(pattern: &str) -> Result<(), ParsingError> {
    if matches!(
        pattern,
        "amid[e]?[\\]\\)\\}]*"
            | "(diyl|ylen[e]?)[\\]\\)\\}]*"
            | "(?<!ic|ous)"
            | "^(N,N-)?diacet(ic[ ]?acid|at[e]?)"
            | "ol[\\]\\)\\}]*"
    ) {
        Ok(())
    } else {
        Err(error(format!(
            "Unexpected pinned word-rule expression: {pattern}"
        )))
    }
}

fn ending_pattern_matches(pattern: &str, value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let stripped = lower.trim_end_matches([']', ')', '}']);
    match pattern {
        "amid[e]?[\\]\\)\\}]*" => stripped.ends_with("amid") || stripped.ends_with("amide"),
        "(diyl|ylen[e]?)[\\]\\)\\}]*" => ["diyl", "ylen", "ylene"]
            .iter()
            .any(|suffix| stripped.ends_with(suffix)),
        "(?<!ic|ous)" => !lower.ends_with("ic") && !lower.ends_with("ous"),
        "^(N,N-)?diacet(ic[ ]?acid|at[e]?)" => [
            "diaceticacid",
            "diacetic acid",
            "diacetat",
            "diacetate",
            "n,n-diaceticacid",
            "n,n-diacetic acid",
            "n,n-diacetat",
            "n,n-diacetate",
        ]
        .contains(&lower.as_str()),
        "ol[\\]\\)\\}]*" => stripped.ends_with("ol"),
        _ => false,
    }
}
