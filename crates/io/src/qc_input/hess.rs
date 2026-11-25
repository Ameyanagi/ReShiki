//! ORCA `.hess` files from a Cartesian Hessian, for `InHess Read`.
use std::fmt::Write;

use crate::qcdata::ANGSTROM_PER_BOHR;

use super::{Error, Frame, atoms, check_atoms};

pub fn write_orca_hess(frame: &Frame, hessian_hartree_per_bohr2: &[f64]) -> Result<String, Error> {
    check_atoms(frame)?;
    let dimension = frame.numbers.len() * 3;
    let hessian = hessian_hartree_per_bohr2;
    if dimension.checked_pow(2) != Some(hessian.len()) || hessian.iter().any(|x| !x.is_finite()) {
        return Err(Error::Invalid(
            "The Hessian does not match the structure.".into(),
        ));
    }

    let mut text = String::from(
        "\n$orca_hessian_file\n\n$act_atom\n  0\n\n$act_coord\n  0\n\n$act_energy\n        0.000000\n\n$hessian\n",
    );
    let _ = writeln!(text, "{dimension}");
    for start in (0..dimension).step_by(5) {
        let end = (start + 5).min(dimension);
        for column in start..end {
            let _ = write!(text, "{column:>19}");
        }
        text.push('\n');
        for (row, values) in hessian.chunks_exact(dimension).enumerate() {
            let _ = write!(text, "{row:>7}");
            for &value in values.iter().skip(start).take(end - start) {
                let _ = write!(text, "{:>19}", fortran_scientific(value));
            }
            text.push('\n');
        }
    }

    let _ = writeln!(text, "$atoms\n{}", frame.numbers.len());
    for (index, (_, element, pos)) in atoms(frame).enumerate() {
        let sym = element.symbol;
        let mass = frame
            .masses_amu
            .get(index)
            .copied()
            .unwrap_or_else(|| element.average_mass_amu());
        let [x, y, z] = pos.map(|coordinate| coordinate / ANGSTROM_PER_BOHR);
        let _ = writeln!(
            text,
            " {sym:<2} {mass:>12.5} {x:>17.10} {y:>17.10} {z:>17.10}"
        );
    }
    text.push_str("\n$end\n");
    Ok(text)
}

fn fortran_scientific(value: f64) -> String {
    let formatted = format!("{value:.10E}");
    if let Some((mantissa, exponent)) = formatted.split_once('E') {
        let (sign, digits) = exponent
            .strip_prefix('-')
            .map_or(('+', exponent), |digits| ('-', digits));
        format!("{mantissa}E{sign}{digits:0>2}")
    } else {
        formatted
    }
}
