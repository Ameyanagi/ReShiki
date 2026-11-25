use std::fmt::Write;

use super::{Error, Frame, atoms, clean_title, single_line};

pub struct SdfEntry<'a> {
    pub frame: &'a Frame,
    pub title: &'a str,
    pub bonds: &'a [(usize, usize, u8)],
    pub formal_charges: &'a [i32],
    pub properties: &'a [(String, String)],
}

pub fn write_sdf(entries: &[SdfEntry]) -> Result<String, Error> {
    let mut text = String::new();
    for entry in entries {
        let frame = entry.frame;
        let atom_count = frame.numbers.len();
        if entry.formal_charges.len() > atom_count {
            return Err(Error::Invalid(
                "There are more formal charges than atoms.".into(),
            ));
        }
        if entry
            .formal_charges
            .iter()
            .any(|&charge| i8::try_from(charge).is_err())
        {
            return Err(Error::Invalid(
                "A formal charge is outside the supported range.".into(),
            ));
        }
        let bond_count = i32::try_from(entry.bonds.len())
            .map_err(|_| Error::Invalid("There are too many bonds to write to SDF.".into()))?;
        for &(a, b, order) in entry.bonds {
            if a >= atom_count || b >= atom_count {
                return Err(Error::Invalid(
                    "A bond refers to an atom outside the structure.".into(),
                ));
            }
            if !(1..=4).contains(&order) {
                return Err(Error::Invalid("Bond orders must be 1 to 4.".into()));
            }
        }
        let title = clean_title(entry.title);
        let _ = writeln!(
            text,
            "{title}\n{:<20}3D\n\n  0  0  0     0  0            999 V3000",
            "  ReShiki"
        );
        text.push_str("M  V30 BEGIN CTAB\n");
        let _ = writeln!(text, "M  V30 COUNTS {atom_count} {bond_count} 0 0 0");
        text.push_str("M  V30 BEGIN ATOM\n");
        for (index, (_, element, [x, y, z])) in atoms(frame).enumerate() {
            let sym = element.symbol;
            let _ = write!(text, "M  V30 {} {sym} {x:.6} {y:.6} {z:.6} 0", index + 1);
            if let Some(&charge) = entry.formal_charges.get(index).filter(|&&c| c != 0) {
                let _ = write!(text, " CHG={charge}");
            }
            if let Some(&isotope) = frame.isotopes.get(index).filter(|&&i| i != 0) {
                let _ = write!(text, " MASS={isotope}");
            }
            text.push('\n');
        }
        text.push_str("M  V30 END ATOM\n");
        if !entry.bonds.is_empty() {
            text.push_str("M  V30 BEGIN BOND\n");
            for (index, &(a, b, order)) in entry.bonds.iter().enumerate() {
                let _ = writeln!(text, "M  V30 {} {order} {} {}", index + 1, a + 1, b + 1);
            }
            text.push_str("M  V30 END BOND\n");
        }
        text.push_str("M  V30 END CTAB\nM  END\n");
        for (name, value) in entry.properties {
            let name = single_line(name);
            let value = single_line(value);
            let _ = writeln!(text, "> <{name}>\n{value}\n");
        }
        text.push_str("$$$$\n");
    }
    Ok(text)
}
