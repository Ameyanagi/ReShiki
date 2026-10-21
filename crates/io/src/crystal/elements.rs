//! Element symbols as crystal files write them, and covalent radii for
//! perceiving bonds.
use reshiki_model::editing::ELEMENTS;

/// Covalent radii in ångström by atomic number from 1 (Cordero et al., Dalton
/// Trans. 2008, 2832; Mn, Fe and Co between their low- and high-spin values).
const COVALENT: [f64; 96] = [
    0.31, 0.28, 1.28, 0.96, 0.84, 0.76, 0.71, 0.66, 0.57, 0.58, // H–Ne
    1.66, 1.41, 1.21, 1.11, 1.07, 1.05, 1.02, 1.06, // Na–Ar
    2.03, 1.76, 1.70, 1.60, 1.53, 1.39, 1.50, 1.42, 1.38, 1.24, 1.32, 1.22, // K–Zn
    1.22, 1.20, 1.19, 1.20, 1.20, 1.16, // Ga–Kr
    2.20, 1.95, 1.90, 1.75, 1.64, 1.54, 1.47, 1.46, 1.42, 1.39, 1.45, 1.44, // Rb–Cd
    1.42, 1.39, 1.39, 1.38, 1.39, 1.40, // In–Xe
    2.44, 2.15, 2.07, 2.04, 2.03, 2.01, 1.99, 1.98, 1.98, 1.96, 1.94, 1.92, 1.92, 1.89, 1.90, 1.87,
    1.87, // Cs–Lu
    1.75, 1.70, 1.62, 1.51, 1.44, 1.41, 1.36, 1.36, 1.32, // Hf–Hg
    1.45, 1.46, 1.48, 1.40, 1.50, 1.50, // Tl–Rn
    2.60, 2.21, 2.15, 2.06, 2.00, 1.96, 1.90, 1.87, 1.80, 1.69, // Fr–Cm
];
/// The radius of heavier elements.
const DEFAULT_RADIUS: f64 = 1.5;

/// The covalent radius of a valid element symbol.
pub(super) fn covalent_radius(symbol: &str) -> f64 {
    ELEMENTS
        .iter()
        .position(|e| *e == symbol)
        .and_then(|i| COVALENT.get(i))
        .copied()
        .unwrap_or(DEFAULT_RADIUS)
}

fn known(symbol: &str) -> Option<String> {
    match symbol {
        // Deuterium and tritium are hydrogen.
        "D" | "T" => Some("H".into()),
        s => ELEMENTS.contains(&s).then(|| s.to_owned()),
    }
}

fn capitalized(letters: &str) -> String {
    let mut chars = letters.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
    })
}

/// The leading letters of `text`, at most two.
fn letters(text: &str) -> &str {
    let end = text
        .char_indices()
        .take_while(|(i, c)| *i < 2 && c.is_ascii_alphabetic())
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    text.get(..end).unwrap_or_default()
}

/// An element and charge from a symbol column such as `Na`, `O2-`, `Fe3+`,
/// `Cl1-` or `S`, in any case; the charge is 0 without a sign.
pub(super) fn from_symbol(text: &str) -> Option<(String, i32)> {
    let text = text.trim();
    let head = letters(text);
    let element = known(&capitalized(head)).or_else(|| known(&capitalized(head.get(..1)?)))?;
    let rest = text
        .get(element_length(text, &element)..)
        .unwrap_or_default();
    Some((element, charge(rest)))
}

/// How many characters of `text` spell `element` (D and T spell hydrogen).
fn element_length(text: &str, element: &str) -> usize {
    if element == "H" && !text.to_ascii_uppercase().starts_with('H') {
        1
    } else {
        element.len()
    }
}

/// The charge in what follows a symbol: `2-`, `+3`, `+` or nothing.
fn charge(rest: &str) -> i32 {
    let rest = rest.trim();
    let sign = if rest.contains('-') {
        -1
    } else if rest.contains('+') {
        1
    } else {
        return 0;
    };
    let digits: String = rest.chars().filter(char::is_ascii_digit).collect();
    sign * digits.parse::<i32>().unwrap_or(1).min(9)
}

/// An element from an atom label such as `C1`, `Cl2A` or `O1W`: a second
/// lowercase letter makes a two-letter symbol when there is one, else the
/// first letter alone, else both letters.
pub(super) fn from_label(text: &str) -> Option<String> {
    let head = letters(text.trim());
    let one = head.get(..1).and_then(|l| known(&capitalized(l)));
    let two = (head.len() == 2)
        .then(|| known(&capitalized(head)))
        .flatten();
    let lower = head.chars().nth(1).is_some_and(|c| c.is_ascii_lowercase());
    if lower { two.or(one) } else { one.or(two) }
}

/// The element of atomic number `z`.
pub(super) fn from_number(z: usize) -> Option<String> {
    ELEMENTS.get(z.checked_sub(1)?).map(|s| (*s).to_owned())
}

/// Whether an element is a metal: not a nonmetal, metalloid or noble gas.
pub(super) fn metal(symbol: &str) -> bool {
    !matches!(
        symbol,
        "H" | "He"
            | "B"
            | "C"
            | "N"
            | "O"
            | "F"
            | "Ne"
            | "Si"
            | "P"
            | "S"
            | "Cl"
            | "Ar"
            | "Ge"
            | "As"
            | "Se"
            | "Br"
            | "Kr"
            | "Sb"
            | "Te"
            | "I"
            | "Xe"
            | "At"
            | "Rn"
    )
}

/// Whether an element forms the anions of salts, oxides and carbides.
pub(super) fn anion(symbol: &str) -> bool {
    matches!(
        symbol,
        "C" | "N" | "O" | "F" | "P" | "S" | "Cl" | "Se" | "Br" | "Te" | "I"
    )
}
