//! Conservative geometry observations, independent of numerical convergence.

use std::collections::HashMap;

use crate::Frame;

/// Hydrogens that clearly lost or gained at least one metal partner while
/// bridging two or more d-block metals. A partner exchange counts in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HydrideBridgeChanges {
    pub lost: usize,
    pub gained: usize,
}

impl HydrideBridgeChanges {
    /// Shared wording for desktop results and command-line warnings.
    pub fn warning(self) -> String {
        let unit = |count| if count == 1 { "hydrogen" } else { "hydrogens" };
        format!(
            "Hydride coordination changed during optimization. {} {} lost a bridging metal contact and {} {} gained one. Inspect the final geometry.",
            self.lost,
            unit(self.lost),
            self.gained,
            unit(self.gained)
        )
    }
}

/// Report clear changes of metal–hydride bridging from two validated frames.
///
/// This is a distance observation, not a bond-order or oxidation-state assignment.
/// Only d-block metals are considered. A definite M–H contact is shorter than
/// the Cordero covalent-radius sum plus 0.25 Å; it must exceed that sum plus
/// 0.55 Å before a previously definite contact is reported lost. The 0.30 Å
/// buffer suppresses contact-boundary noise. Ordinary X–H and H–H neighbors
/// exclude an H from hydride perception, including agostic C–H contacts.
///
/// Complete drawing IDs map independently reordered frames. Otherwise the
/// calculation's atom order must be retained, as it is for optimization results.
/// Rotations and translations do not change the observation.
pub fn hydride_bridge_changes(input: &Frame, output: &Frame) -> Option<HydrideBridgeChanges> {
    let mut metals = input
        .numbers
        .iter()
        .filter(|&&number| metal_radius(number).is_some());
    metals.next()?;
    metals.next()?;
    if input.numbers.len() != output.numbers.len() {
        return None;
    }
    let complete_ids = |frame: &Frame| {
        !frame.drawing_atoms.is_empty() && frame.drawing_atoms.iter().all(Option::is_some)
    };
    let order: Vec<_> = if complete_ids(input) && complete_ids(output) {
        let ids: HashMap<_, _> = output
            .drawing_atoms
            .iter()
            .enumerate()
            .map(|(atom, &id)| (id, atom))
            .collect();
        input
            .drawing_atoms
            .iter()
            .map(|id| ids.get(id).copied())
            .collect::<Option<_>>()?
    } else {
        (0..input.numbers.len()).collect()
    };
    if !input
        .numbers
        .iter()
        .zip(&order)
        .all(|(&number, &atom)| number == output.numbers[atom])
    {
        return None;
    }
    let output_positions: Vec<_> = order
        .iter()
        .map(|&atom| output.positions_angstrom[atom])
        .collect();
    let mut changes = HydrideBridgeChanges { lost: 0, gained: 0 };
    for (hydrogen, &number) in input.numbers.iter().enumerate() {
        if number != 1 {
            continue;
        }
        let before = metal_neighbors(&input.numbers, &input.positions_angstrom, hydrogen);
        let after = metal_neighbors(&input.numbers, &output_positions, hydrogen);
        if before.definite.len() >= 2
            && before
                .definite
                .iter()
                .any(|metal| !after.possible.contains(metal))
        {
            changes.lost += 1;
        }
        if after.definite.len() >= 2
            && after
                .definite
                .iter()
                .any(|metal| !before.possible.contains(metal))
        {
            changes.gained += 1;
        }
    }
    (changes.lost > 0 || changes.gained > 0).then_some(changes)
}

#[derive(Default)]
struct Neighbors {
    definite: Vec<usize>,
    possible: Vec<usize>,
}

fn metal_neighbors(numbers: &[u8], positions: &[[f64; 3]], hydrogen: usize) -> Neighbors {
    let mut neighbors = Neighbors::default();
    for (atom, &number) in numbers.iter().enumerate() {
        if atom == hydrogen {
            continue;
        }
        let distance = positions[hydrogen]
            .iter()
            .zip(positions[atom])
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        if let Some(radius) = metal_radius(number) {
            if distance > 0.4 && distance < radius + 0.31 + 0.55 {
                neighbors.possible.push(atom);
                if distance < radius + 0.31 + 0.25 {
                    neighbors.definite.push(atom);
                }
            }
        } else {
            let radius = match number {
                1 => 0.31,
                5 => 0.84,
                6 => 0.76,
                7 => 0.71,
                8 => 0.66,
                9 => 0.57,
                14 => 1.11,
                15 => 1.07,
                16 => 1.05,
                17 => 1.02,
                32 | 34 | 35 => 1.20,
                33 => 1.19,
                51 | 53 => 1.39,
                52 => 1.38,
                _ => continue,
            };
            if distance < radius + 0.31 + 0.4 {
                return Neighbors::default();
            }
        }
    }
    neighbors
}

// Cordero et al., Dalton Trans. 2008, 2832, as used by crystal import.
// Mn/Fe/Co are between the low- and high-spin values. Keeping this observation
// to the d block avoids treating long ionic contacts as molecular bridges.
fn metal_radius(number: u8) -> Option<f64> {
    const FIRST: [f64; 10] = [1.70, 1.60, 1.53, 1.39, 1.50, 1.42, 1.38, 1.24, 1.32, 1.22];
    const SECOND: [f64; 10] = [1.90, 1.75, 1.64, 1.54, 1.47, 1.46, 1.42, 1.39, 1.45, 1.44];
    const THIRD: [f64; 9] = [1.75, 1.70, 1.62, 1.51, 1.44, 1.41, 1.36, 1.36, 1.32];
    match number {
        21..=30 => Some(FIRST[usize::from(number - 21)]),
        39..=48 => Some(SECOND[usize::from(number - 39)]),
        72..=80 => Some(THIRD[usize::from(number - 72)]),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
