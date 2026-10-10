//! Faithful OPSIN isotope specification parsing.
//!
//! Source: OPSIN 2.9.0 `IsotopeSpecificationParser.java`, commit
//! b91b610af5ab07560fedb20730d7aef46bb2bca0. Copyright Daniel Lowe and
//! contributors, MIT (see this crate's retained license).

use std::sync::LazyLock;

use regex::Regex;

use crate::api::ParsingError;
use crate::graph::Element;
use crate::parse_tree::{Arena, NodeId};
use crate::tree_tools::fix_locant_capitalisation;
use crate::xml_declarations::{BOUGHTONSYSTEM_TYPE_VAL, IUPACSYSTEM_TYPE_VAL, TYPE_ATR};

static BOUGHTON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\A(?:-([^,]+(?:,[^,]+)*))?-(([0-9]+)([A-Z][a-z]?)|d)([0-9]+)?\z")
        .expect("OPSIN Boughton isotope pattern")
});
static IUPAC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\A(?:([^,]+(?:,[^,]+)*)-)?([0-9]+)([A-Z][a-z]?)([0-9]+)?\z")
        .expect("OPSIN IUPAC isotope pattern")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsotopeSpecification {
    pub element: Element,
    pub isotope: u32,
    pub multiplier: usize,
    pub locants: Option<Vec<String>>,
}

pub fn parse_isotope_specification(
    arena: &Arena,
    isotope_specification: NodeId,
) -> Result<IsotopeSpecification, ParsingError> {
    parse_isotope_specification_value(
        arena[isotope_specification]
            .attribute(TYPE_ATR)
            .unwrap_or(""),
        &arena.value(isotope_specification),
    )
}

pub fn parse_isotope_specification_value(
    syntax: &str,
    value: &str,
) -> Result<IsotopeSpecification, ParsingError> {
    let (captures, element, isotope, multiplier_index) = match syntax {
        BOUGHTONSYSTEM_TYPE_VAL => {
            let captures = BOUGHTON
                .captures(value)
                .ok_or_else(|| ParsingError(format!("Malformed isotope specification: {value}")))?;
            let (element, isotope) = if &captures[2] == "d" {
                (Element::H, 2)
            } else {
                (
                    parse_element(&captures[4])?,
                    parse_number(&captures[3], value)?,
                )
            };
            (captures, element, isotope, 5)
        }
        IUPACSYSTEM_TYPE_VAL => {
            let captures = IUPAC
                .captures(value)
                .ok_or_else(|| ParsingError(format!("Malformed isotope specification: {value}")))?;
            let element = parse_element(&captures[3])?;
            let isotope = parse_number(&captures[2], value)?;
            (captures, element, isotope, 4)
        }
        _ => {
            return Err(ParsingError(
                "Unsupported isotope specification syntax".into(),
            ));
        }
    };
    let multiplier_text = captures.get(multiplier_index);
    let mut multiplier = multiplier_text
        .map(|m| parse_number(m.as_str(), value).map(|n| n as usize))
        .transpose()?
        .unwrap_or(1);
    let locants = captures.get(1).map(|m| {
        m.as_str()
            .split(',')
            .map(fix_locant_capitalisation)
            .collect::<Vec<_>>()
    });
    if let Some(locants) = &locants {
        if multiplier_text.is_none() {
            multiplier = locants.len();
        } else if locants.len() != multiplier {
            return Err(ParsingError(format!(
                "Mismatch between number of locants: {} and number of {element} isotopes requested: {multiplier}",
                locants.len()
            )));
        }
    }
    Ok(IsotopeSpecification {
        element,
        isotope,
        multiplier,
        locants,
    })
}

fn parse_element(symbol: &str) -> Result<Element, ParsingError> {
    Element::from_symbol(symbol)
        .ok_or_else(|| ParsingError(format!("Unknown chemical element: {symbol}")))
}

fn parse_number(value: &str, specification: &str) -> Result<u32, ParsingError> {
    // Java Integer.parseInt rejects values beyond signed 32-bit range.
    let number = value
        .parse::<i32>()
        .map_err(|_| ParsingError(format!("Malformed isotope specification: {specification}")))?;
    Ok(number as u32)
}
