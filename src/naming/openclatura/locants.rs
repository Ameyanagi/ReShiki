//! Atom and multiple-bond locants for acyclic and monocyclic parents.
pub use super::assembly_utils::parse_locant;
use super::molecule::{AtomSet, Molecule};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemLocant {
    pub base: usize,
    pub fusion_suffix: String,
    pub interior_distance: Option<usize>,
}
#[cfg(test)]
impl SystemLocant {
    pub fn render(&self, unicode_superscript: bool) -> String {
        let mut text = format!("{}{}", self.base, self.fusion_suffix);
        if let Some(distance) = self.interior_distance {
            if unicode_superscript {
                const SUPERSCRIPTS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
                text.extend(
                    distance
                        .to_string()
                        .bytes()
                        .map(|b| SUPERSCRIPTS[(b - b'0') as usize]),
                );
            } else {
                text.push_str(&format!("^{distance}"));
            }
        }
        text
    }
}
pub fn parse_system_locant(value: &str) -> Result<SystemLocant, String> {
    let mut normalized = String::new();
    let mut in_superscript = false;
    for c in value.trim().chars() {
        if let Some(index) = "⁰¹²³⁴⁵⁶⁷⁸⁹".chars().position(|n| n == c) {
            if !in_superscript {
                normalized.push('^');
                in_superscript = true;
            }
            normalized.push(char::from(b'0' + index as u8));
        } else {
            normalized.push(c);
        }
    }
    let (stem, distance) = normalized
        .split_once('^')
        .map_or((normalized.as_str(), None), |(s, d)| (s, Some(d)));
    let count = stem.bytes().take_while(u8::is_ascii_digit).count();
    let invalid = || format!("invalid completed-system locant: {value:?}");
    if count == 0 || stem.starts_with('0') || !stem[count..].chars().all(|c| c.is_ascii_lowercase())
    {
        return Err(invalid());
    }
    let base: usize = stem[..count].parse().map_err(|_| invalid())?;
    if base == 0 {
        return Err(invalid());
    }
    let interior_distance = distance
        .map(|d| {
            if d.starts_with('0') || !d.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            d.parse::<usize>()
                .ok()
                .filter(|&n| n > 0)
                .ok_or_else(invalid)
        })
        .transpose()?;
    Ok(SystemLocant {
        base,
        fusion_suffix: stem[count..].into(),
        interior_distance,
    })
}
pub fn retained_locant_sort_key(value: &str) -> (usize, u8, String, usize) {
    if let Ok(locant) = parse_system_locant(value) {
        return (
            locant.base,
            u8::from(!locant.fusion_suffix.is_empty()),
            locant.fusion_suffix,
            locant.interior_distance.unwrap_or(0),
        );
    }
    let count = value.bytes().take_while(u8::is_ascii_digit).count();
    (
        value[..count].parse().unwrap_or(10_000),
        u8::from(count < value.len()),
        value[count..].into(),
        0,
    )
}
pub fn canonical_locant_pair(left: &str, right: &str) -> (String, String) {
    if retained_locant_sort_key(left) <= retained_locant_sort_key(right) {
        (left.into(), right.into())
    } else {
        (right.into(), left.into())
    }
}

pub fn get_atom_locants(oriented_path: &[usize], target_indices: &AtomSet) -> Vec<usize> {
    oriented_path
        .iter()
        .enumerate()
        .filter_map(|(i, idx)| target_indices.contains(idx).then_some(i + 1))
        .collect()
}

pub fn get_bond_locants(
    mol: &Molecule,
    oriented_path: &[usize],
    is_bicycle: bool,
    is_spiro: bool,
    is_polycycle: bool,
) -> (Vec<usize>, Vec<usize>) {
    let mut doubles = Vec::new();
    let mut triples = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, &u) in oriented_path.iter().enumerate() {
        for &v in mol.get_neighbors(u) {
            let Some(j) = oriented_path.iter().position(|&idx| idx == v) else {
                continue;
            };
            let Some(bond) = mol.get_bond(u, v) else {
                continue;
            };
            if bond.order <= 1 || !seen.insert(bond.idx) {
                continue;
            }
            let (low, high) = ((i + 1).min(j + 1), (i + 1).max(j + 1));
            let locant = if high == low + 1 {
                low
            } else if low == 1
                && high == oriented_path.len()
                && !(is_bicycle || is_spiro || is_polycycle)
            {
                high
            } else {
                low
            };
            match bond.order {
                2 => doubles.push(locant),
                3 => triples.push(locant),
                _ => (),
            }
        }
    }
    doubles.sort();
    triples.sort();
    (doubles, triples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_system_locants_preserve_interior_distance() {
        let locant = parse_system_locant("9a¹²").unwrap();
        assert_eq!(locant.render(false), "9a^12");
        assert_eq!(locant.render(true), "9a¹²");
        let mut labels = ["9a^12", "10", "9a^2", "9a", "9"];
        labels.sort_by_key(|l| retained_locant_sort_key(l));
        assert_eq!(labels, ["9", "9a", "9a^2", "9a^12", "10"]);
        for invalid in ["0", "01", "9^0", "9^01", "9'", "9^2^3", "9A"] {
            assert!(parse_system_locant(invalid).is_err(), "{invalid}");
        }
    }
}
