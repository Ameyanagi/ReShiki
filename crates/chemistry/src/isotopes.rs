//! Mass-number entry and nuclide hints for atom editing.
use crate::{ELEMENTS, ISOTOPES};

/// Read a mass number, optionally followed by the selected element's symbol.
/// Zero removes the isotope label; unknown nuclides remain editable.
pub fn parse_mass_number(input: &str, element: &str) -> Result<u32, String> {
    let input = input.trim();
    let mut value = 0u32;
    let mut end = 0;
    for (index, character) in input.char_indices() {
        let digit = match character {
            '0'..='9' => u32::from(character) - u32::from('0'),
            '⁰' => 0,
            '¹' => 1,
            '²' => 2,
            '³' => 3,
            '⁴' => 4,
            '⁵' => 5,
            '⁶' => 6,
            '⁷' => 7,
            '⁸' => 8,
            '⁹' => 9,
            _ => break,
        };
        value = value.saturating_mul(10).saturating_add(digit);
        end = index + character.len_utf8();
    }
    if end == 0 || value > 300 {
        return Err("Enter a mass number from 0 to 300 (0 clears it), such as 13 or ¹³C".into());
    }
    let symbol = input.get(end..).unwrap_or_default().trim();
    if !symbol.is_empty() && symbol != element {
        return Err(format!(
            "The element symbol must match the selected element {element}"
        ));
    }
    Ok(value)
}

/// Whether the mass data includes this element and mass-number pair.
pub fn is_known_nuclide(element: &str, mass_number: u32) -> bool {
    let Some(number) = ELEMENTS.iter().position(|e| e.symbol == element) else {
        return false;
    };
    let (Ok(number), Ok(mass_number)) = (u8::try_from(number), u16::try_from(mass_number)) else {
        return false;
    };
    ISOTOPES
        .binary_search_by_key(&(number, mass_number), |&(n, i, _)| (n, i))
        .is_ok()
}

pub fn nuclide_symbol(element: &str, mass_number: u32) -> String {
    let mut symbol: String = mass_number
        .to_string()
        .chars()
        .map(|digit| match digit {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            _ => digit,
        })
        .collect();
    symbol.push_str(element);
    symbol
}

/// An informational warning; zero and known nuclides need no hint.
pub fn nuclide_warning(element: &str, mass_number: u32) -> Option<String> {
    (mass_number != 0 && !is_known_nuclide(element, mass_number)).then(|| {
        format!(
            "{} is not a known nuclide. Molecular weights use its mass number as an approximation.",
            nuclide_symbol(element, mass_number)
        )
    })
}

pub fn common_mass_numbers(element: &str) -> &'static [u32] {
    match element {
        "H" => &[2, 3],
        "C" => &[13, 14],
        "N" => &[15],
        "O" | "F" => &[18],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_number_accepts_digits_superscripts_and_matching_symbols() {
        for input in ["13", "13C", "¹³", "¹³C", " ¹³C ", "13 C"] {
            assert_eq!(parse_mass_number(input, "C"), Ok(13), "{input}");
        }
        assert_eq!(parse_mass_number("³⁷Cl", "Cl"), Ok(37));
        for input in ["0", "⁰", "0C"] {
            assert_eq!(parse_mass_number(input, "C"), Ok(0));
        }
        assert_eq!(parse_mass_number("300", "H"), Ok(300));
    }

    #[test]
    fn mass_number_rejects_mismatched_symbols_and_malformed_input() {
        for input in ["13N", "¹³N", "13Cl", "13c"] {
            assert!(parse_mass_number(input, "C").is_err(), "{input}");
        }
        for input in [
            "",
            "C",
            "C13",
            "-1",
            "+13",
            "1.3",
            "301",
            "9999999999999999999",
        ] {
            assert!(parse_mass_number(input, "C").is_err(), "{input}");
        }
    }

    #[test]
    fn unknown_nuclides_warn_without_rejecting_the_mass_number() {
        for (input, element, label) in [("12H", "H", "¹²H"), ("2C", "C", "²C")] {
            let mass_number = parse_mass_number(input, element).unwrap();
            assert!(!is_known_nuclide(element, mass_number));
            assert!(
                nuclide_warning(element, mass_number)
                    .unwrap()
                    .starts_with(label)
            );
        }
        assert!(is_known_nuclide("C", 13));
        assert_eq!(nuclide_warning("C", 13), None);
        assert_eq!(nuclide_warning("H", 0), None);
        assert!(!is_known_nuclide("Xx", 13));
    }

    #[test]
    fn common_labelling_nuclides_have_exact_mass_data() {
        let mut labels = Vec::new();
        for element in ["H", "C", "N", "O", "F"] {
            for &mass_number in common_mass_numbers(element) {
                assert!(is_known_nuclide(element, mass_number));
                labels.push(nuclide_symbol(element, mass_number));
            }
        }
        assert_eq!(labels, ["²H", "³H", "¹³C", "¹⁴C", "¹⁵N", "¹⁸O", "¹⁸F"]);
        assert!(common_mass_numbers("Cl").is_empty());
        let fluorine = crate::properties(&[crate::AtomFacts {
            atomic_number: 9,
            isotope: 18,
            charge: 0,
            hydrogens: 0,
            radical_electrons: 0,
        }])
        .unwrap();
        assert_eq!(fluorine.mass, 18.000938);
        assert_eq!(fluorine.exact_mass, 18.000938);
    }
}
