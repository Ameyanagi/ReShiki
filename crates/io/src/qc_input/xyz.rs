//! XYZ files, one frame or a trajectory.
use std::fmt::Write;

use crate::{
    qcdata::{MAX_FRAME_ATOMS, Trajectory},
    qcread::xyz_species,
};

use super::{
    Error, Frame, Program, XyzColumns, check_atoms, clean_title, single_line, write_atoms,
};

pub(super) fn frame(frame: &Frame, title: &str) -> String {
    let mut text = String::new();
    write_frame(&mut text, frame, title);
    text
}

pub fn write_xyz_trajectory(path: &Trajectory, title: &str) -> Result<String, Error> {
    let title = clean_title(title);
    let comments = (0..path.steps()).map(|step| {
        let mut comment = path.labels.get(step).map_or_else(
            || format!("{title} step {}", step + 1),
            |label| single_line(label),
        );
        if let Some(energy) = path.energies_hartree.get(step) {
            let _ = write!(comment, "  E = {energy:.10} Eh");
        }
        comment
    });
    write_xyz_trajectory_with_comments(path, comments)
}

/// Write ångström frames in trajectory order, with one comment line per frame.
pub fn write_xyz_trajectory_with_comments(
    path: &Trajectory,
    comments: impl ExactSizeIterator<Item = String>,
) -> Result<String, Error> {
    let mut text = String::new();
    if path.positions_angstrom.is_empty() {
        return Ok(text);
    }
    if comments.len() != path.steps() {
        return Err(Error::Invalid("Each XYZ frame needs one comment.".into()));
    }
    for (step, comment) in comments.enumerate() {
        let Some(frame) = path.frame(step) else { break };
        check_atoms(&frame)?;
        write_frame(&mut text, &frame, &single_line(&comment));
    }
    Ok(text)
}

fn write_frame(text: &mut String, frame: &Frame, comment: &str) {
    let _ = writeln!(text, "{}\n{comment}", frame.numbers.len());
    write_atoms(text, frame, Program::Xyz);
}

/// Reads the first Cartesian XYZ frame in ångströms, including extended XYZ columns.
/// Later frames may contain a different structure.
pub fn parse_xyz(text: &str) -> Result<Frame, String> {
    let mut lines = text.lines();
    let atoms = lines
        .next()
        .and_then(|line| line.trim().parse::<usize>().ok())
        .ok_or("The XYZ input needs an atom count on the first line.")?;
    if atoms > MAX_FRAME_ATOMS {
        return Err(format!("The XYZ input exceeds {MAX_FRAME_ATOMS} atoms."));
    }
    let comment = lines.next().ok_or("The XYZ input needs a comment line.")?;
    let columns = XyzColumns::from_comment(comment)
        .ok_or("XYZ Properties needs species:S:1 and pos:R:3 columns.")?;
    let mut numbers = Vec::with_capacity(atoms);
    let mut isotopes = Vec::with_capacity(atoms);
    let mut positions_angstrom = Vec::with_capacity(atoms);
    for atom in 0..atoms {
        let invalid = || {
            format!(
                "Invalid XYZ atom {}: expected an element and finite x y z coordinates.",
                atom + 1
            )
        };
        let fields: Vec<_> = lines
            .next()
            .ok_or_else(invalid)?
            .split_whitespace()
            .collect();
        let symbol = *fields.get(columns.species).ok_or_else(invalid)?;
        let (number, isotope) = xyz_species(symbol).ok_or_else(invalid)?;
        let mut position = [0.0; 3];
        for (offset, coordinate) in position.iter_mut().enumerate() {
            *coordinate = fields
                .get(columns.position + offset)
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .ok_or_else(invalid)?;
        }
        numbers.push(number);
        isotopes.push(isotope);
        positions_angstrom.push(position);
    }
    if isotopes.iter().all(|&isotope| isotope == 0) {
        isotopes.clear();
    }
    Ok(Frame {
        numbers,
        positions_angstrom,
        isotopes,
        masses_amu: Vec::new(),
        charge: 0,
        multiplicity: 1,
        drawing_atoms: Vec::new(),
    })
}
