//! Molecular geometries from plain and extended XYZ.
use super::{
    Error, Format, Job, JobKind, QcFile, Step, check_file_size, frame, lowest_multiplicity,
    xyz_species,
};
use crate::crystal::xyz::{self, FrameError, value};
use reshiki_qcdata::{Frame, MAX_FRAME_ATOMS, MAX_FRAMES};

const EV_PER_HARTREE: f64 = 27.211_386_245_988;

/// Read the first XYZ geometry and title with neutral charge and lowest spin.
pub fn read_geometry(text: &str) -> Result<(Frame, &str), Error> {
    check_file_size(text.len())?;
    let mut lines = text.trim_end().lines().enumerate();
    let header = xyz::header(&mut lines)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .ok_or_else(|| Error::Invalid("The file has no geometry".into()))?;
    check_atom_count(&header, 1)?;
    let parsed = read_atoms(&header, &mut lines, 1)?;
    let (atoms, positions): (Vec<_>, Vec<_>) = parsed.atoms.into_iter().unzip();
    let frame = frame(&atoms, positions, 0, lowest_multiplicity(&atoms, 0))?;
    Ok((frame, header.comment.trim()))
}

fn check_atom_count(header: &xyz::Header<'_>, frame: usize) -> Result<(), Error> {
    if header.count > MAX_FRAME_ATOMS {
        return Err(Error::Unsupported(format!(
            "Frame {frame} has {} atoms; ReShiki reads at most {MAX_FRAME_ATOMS}",
            header.count
        )));
    }
    Ok(())
}

fn incomplete_frame(line: usize, frame: usize, count: usize) -> Error {
    Error::at(
        line,
        format!("The file ends before frame {frame}'s {count} atoms"),
    )
}

fn read_atoms(
    header: &xyz::Header<'_>,
    lines: &mut xyz::Lines<'_>,
    frame: usize,
) -> Result<xyz::Frame<(u8, u16)>, Error> {
    xyz::read_frame(header, lines, xyz_species, |_| Ok(())).map_err(|error| match error {
        FrameError::Invalid(error) => Error::Invalid(error.to_string()),
        FrameError::Incomplete(line) => incomplete_frame(line, frame, header.count),
    })
}

/// Read XYZ atom rows and any recognized per-frame metadata.
pub(super) fn read(text: &str) -> Result<QcFile, Error> {
    let text = text.trim_end();
    let mut lines = text.lines().enumerate();
    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    let mut program = None;
    let mut program_version = None;
    let (mut assumed_spin, mut ignored_cell) = (false, false);
    let mut frames = 0;
    while let Some(header) = xyz::header(&mut lines).map_err(|e| Error::Invalid(e.to_string()))? {
        frames += 1;
        check_atom_count(&header, frames)?;
        if frames > MAX_FRAMES {
            if !xyz::skip_atoms(&mut lines, header.count) {
                return Err(incomplete_frame(
                    text.lines().count() + 1,
                    frames,
                    header.count,
                ));
            }
            continue;
        }

        let parsed = read_atoms(&header, &mut lines, frames)?;
        let (atoms, positions): (Vec<_>, Vec<_>) = parsed.atoms.into_iter().unzip();
        let property = |key| value(&parsed.pairs, key);
        let line = header.line + 1;
        let charge = property("charge")
            .map(|s| s.parse().map_err(|_| Error::at(line, "Invalid XYZ charge")))
            .transpose()?;
        let multiplicity = property("multiplicity")
            .or_else(|| property("mult"))
            .map(|s| {
                s.parse()
                    .map_err(|_| Error::at(line, "Invalid XYZ multiplicity"))
            })
            .transpose()?;
        assumed_spin |= charge.is_none() || multiplicity.is_none();
        let charge = charge.unwrap_or(0);
        let multiplicity = multiplicity.unwrap_or_else(|| lowest_multiplicity(&atoms, charge));
        ignored_cell |= property("lattice").is_some()
            && !property("pbc").is_some_and(|flags| {
                let mut axes = flags.split_whitespace();
                (&mut axes)
                    .take(3)
                    .filter(|f| {
                        f.eq_ignore_ascii_case("f") || f.eq_ignore_ascii_case("false") || *f == "0"
                    })
                    .count()
                    == 3
                    && axes.next().is_none()
            });

        let label = header.comment.trim();
        let energy = |s: &str| {
            s.parse::<f64>()
                .ok()
                .filter(|e| e.is_finite())
                .ok_or_else(|| Error::at(line, "Invalid XYZ energy"))
        };
        let energy_hartree = if let Some(ev) = property("energy") {
            Some(energy(ev)? / EV_PER_HARTREE)
        } else if let Some((data, version)) = label.split_once("xtb:") {
            if program.is_none() {
                program = Some("xtb".into());
                program_version = (!version.trim().is_empty()).then(|| version.trim().into());
            }
            data.strip_prefix("energy:")
                .and_then(|s| s.split_whitespace().next())
                .map(energy)
                .transpose()?
        } else if let Some(data) = label.strip_prefix("Coordinates from ORCA-job ") {
            program.get_or_insert_with(|| "ORCA".into());
            data.rsplit_once(" E ")
                .map(|(_, s)| energy(s.trim()))
                .transpose()?
        } else if let Some(value) = reshiki_energy(label) {
            Some(energy(value.trim())?)
        } else {
            // CREST writes each conformer's energy (Eh) as the whole comment,
            // with decimals; a bare frame number is not an energy.
            label
                .split_once('.')
                .filter(|(_, decimals)| {
                    decimals.len() >= 4 && decimals.bytes().all(|b| b.is_ascii_digit())
                })
                .and_then(|_| label.parse::<f64>().ok())
                .filter(|e| e.is_finite())
        };
        steps.push(Step {
            frame: frame(&atoms, positions, charge, multiplicity)?,
            energy_hartree,
            label: (!label.is_empty()).then(|| label.into()),
        });
    }
    if assumed_spin {
        warnings.push(
            "Charge and spin are not recorded in XYZ; assumed neutral and the lowest spin state"
                .into(),
        );
    }
    if ignored_cell {
        warnings.push("The cell is ignored; atoms are read as a molecule".into());
    }
    if frames > MAX_FRAMES {
        warnings.push(format!("Read the first {MAX_FRAMES} of {frames} frames"));
    }
    Ok(QcFile {
        format: Format::Xyz,
        program,
        program_version,
        jobs: vec![Job {
            index: 0,
            kind: JobKind::Geometry,
            method: None,
            basis: None,
            keywords: Vec::new(),
            completion: None,
            steps,
            result: None,
        }],
        warnings,
    })
}

fn reshiki_energy(label: &str) -> Option<&str> {
    // IRC starts with the energy, while conformer and optimization XYZ end with it.
    let data = if let Some(data) = label.strip_prefix("E = ") {
        data.split_once(" · ").map_or(data, |(energy, _)| energy)
    } else {
        label.rsplit_once(" E = ")?.1
    };
    data.trim().strip_suffix("Eh")
}

#[cfg(test)]
#[path = "tests/xyz.rs"]
mod tests;
