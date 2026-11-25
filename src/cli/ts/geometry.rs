//! XYZ admission and atom mapping before a transition-state search.

use crate::cli::{
    args::Ts,
    calc::{Geometry, read_xyz},
};
use reshiki_optimize::ts::{MAX_ATOMS, least_motion_map};
use serde::Serialize;
use std::path::Path;

pub(super) fn load(command: &Ts) -> Result<(Option<Geometry>, Option<Geometry>), String> {
    let reactant = command.reactant.as_deref().map(endpoint).transpose()?;
    let product = command.product.as_deref().map(endpoint).transpose()?;
    if let (Some(reactant), Some(product)) = (&reactant, &product) {
        check_sequence(&reactant.numbers, &product.numbers)?;
    }
    let atoms = reactant
        .as_ref()
        .or(product.as_ref())
        .map_or(0, |molecule| molecule.numbers.len());
    for (flag, pairs) in [("--form", &command.form), ("--break", &command.break_)] {
        if let Some(&(i, j)) = pairs.iter().find(|&&(_, j)| j > atoms) {
            return Err(format!(
                "{flag} {i}-{j} is out of bounds: the XYZ has {atoms} atoms (1..={atoms})"
            ));
        }
    }
    Ok((reactant, product))
}

fn endpoint(path: &Path) -> Result<Geometry, String> {
    let geometry = read_xyz(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let atoms = geometry.numbers.len();
    if !(2..=MAX_ATOMS).contains(&atoms) {
        return Err(format!(
            "{} has {atoms} atoms; ts requires 2 to {MAX_ATOMS}",
            path.display()
        ));
    }
    Ok(geometry)
}

fn check_sequence(reactant: &[u8], product: &[u8]) -> Result<(), String> {
    if let Some((i, (&r, &p))) = reactant
        .iter()
        .zip(product)
        .enumerate()
        .find(|(_, (r, p))| r != p)
    {
        return Err(format!(
            "endpoint element mismatch at atom {}: reactant {}, product {}",
            i + 1,
            symbol(r),
            symbol(p)
        ));
    }
    let (r, p) = (reactant.len(), product.len());
    if r != p {
        return Err(format!(
            "endpoint element mismatch at atom {}: reactant has {r} atoms, product has {p}",
            r.min(p) + 1
        ));
    }
    Ok(())
}

pub(super) fn symbol(number: u8) -> &'static str {
    crate::chemistry::ELEMENTS
        .get(usize::from(number))
        .map_or("?", |element| element.symbol)
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct MovingHydrogen {
    pub(super) hydrogen: usize,
    pub(super) reactant_heavy_atom: usize,
    pub(super) product_heavy_atom: usize,
}

pub(super) fn remap(
    numbers: &[u8],
    reactant: &[f64],
    product: &mut Vec<f64>,
) -> Result<Vec<MovingHydrogen>, String> {
    let fixed: Vec<_> = numbers
        .iter()
        .enumerate()
        .map(|(i, &z)| (z != 1).then_some(i))
        .collect();
    let mapping = least_motion_map(numbers, reactant, numbers, product, &fixed)
        .map_err(|error| error.to_string())?;
    let rows = product.as_chunks::<3>().0;
    let mut reordered = Vec::with_capacity(product.len());
    for i in mapping {
        let row = rows
            .get(i)
            .ok_or("invalid product atom index in hydrogen map")?;
        reordered.extend_from_slice(row);
    }
    *product = reordered;

    Ok(numbers
        .iter()
        .enumerate()
        .filter_map(|(i, &z)| {
            if z != 1 {
                return None;
            }
            let from = nearest_heavy(numbers, reactant, i)?;
            let to = nearest_heavy(numbers, product, i)?;
            (from != to).then_some(MovingHydrogen {
                hydrogen: i + 1,
                reactant_heavy_atom: from + 1,
                product_heavy_atom: to + 1,
            })
        })
        .collect())
}

fn nearest_heavy(numbers: &[u8], positions: &[f64], hydrogen: usize) -> Option<usize> {
    let rows = positions.as_chunks::<3>().0;
    let hydrogen = rows.get(hydrogen)?;
    numbers
        .iter()
        .zip(rows)
        .enumerate()
        .filter(|(_, (z, _))| **z != 1)
        .map(|(i, (_, point))| {
            let distance: f64 = point
                .iter()
                .zip(hydrogen)
                .map(|(a, b)| (a - b).powi(2))
                .sum();
            (i, distance)
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(tokens: &[&str]) -> Ts {
        let tokens: Vec<_> = tokens.iter().map(std::ffi::OsString::from).collect();
        let crate::cli::args::Command::Ts(command) =
            crate::cli::args::parse(std::ffi::OsStr::new("ts"), &tokens).unwrap()
        else {
            panic!("ts command");
        };
        command
    }

    #[test]
    fn validates_one_based_bond_bounds_after_reading_the_xyz() {
        let dir = tempfile::tempdir().unwrap();
        let xyz = dir.path().join("molecule.xyz");
        std::fs::write(&xyz, "3\n\nC 0 0 0\nN 0 0 1\nH 0 0 2\n").unwrap();
        let xyz = xyz.to_str().unwrap();
        assert!(load(&command(&["--product", xyz, "--form", "1-3"])).is_ok());
        for flag in ["--form", "--break"] {
            let args = command(&["--reactant", xyz, flag, "1-4"]);
            let Err(error) = load(&args) else {
                panic!("out-of-bounds pair accepted");
            };
            assert_eq!(
                error,
                format!("{flag} 1-4 is out of bounds: the XYZ has 3 atoms (1..=3)")
            );
        }
    }

    #[test]
    fn enforces_atom_limits_and_identifies_the_first_element_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let reactant = dir.path().join("r.xyz");
        let product = dir.path().join("p.xyz");
        for atoms in [1, MAX_ATOMS + 1] {
            std::fs::write(
                &reactant,
                format!("{atoms}\n\n{}", "H 0 0 0\n".repeat(atoms)),
            )
            .unwrap();
            let error = endpoint(&reactant).err().unwrap();
            assert!(error.contains(&format!("has {atoms} atoms")));
        }
        std::fs::write(&reactant, "3\n\nC 0 0 0\nO 0 0 1\nH 0 0 2\n").unwrap();
        std::fs::write(&product, "3\n\nC 0 0 0\nN 0 0 1\nF 0 0 2\n").unwrap();
        let args = command(&[
            "--reactant",
            reactant.to_str().unwrap(),
            "--product",
            product.to_str().unwrap(),
        ]);
        assert_eq!(
            load(&args).err().unwrap(),
            "endpoint element mismatch at atom 2: reactant O, product N"
        );
    }

    #[test]
    fn reorders_hydrogens_and_reports_a_transfer() {
        let reactant = [0., 0., 0., 3., 0., 0., 0., 1., 0., 0., -1., 0.];
        let mut product = vec![0., 0., 0., 3., 0., 0., 0., -1., 0., 2., 1., 0.];
        let moved = remap(&[6, 8, 1, 1], &reactant, &mut product).unwrap();
        assert_eq!(product, [0., 0., 0., 3., 0., 0., 2., 1., 0., 0., -1., 0.]);
        assert_eq!(
            moved,
            [MovingHydrogen {
                hydrogen: 3,
                reactant_heavy_atom: 1,
                product_heavy_atom: 2
            }]
        );
    }
}
