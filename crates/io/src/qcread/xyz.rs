//! Molecular geometries from plain and extended XYZ.
use super::{Error, Format, Job, JobKind, QcFile, Step, element, frame, lowest_multiplicity};
use crate::crystal::xyz::{self, FrameError, value};
use reshiki_qcdata::{MAX_FRAME_ATOMS, MAX_FRAMES};

const EV_PER_HARTREE: f64 = 27.211_386_245_988;

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
        if header.count > MAX_FRAME_ATOMS {
            return Err(Error::Unsupported(format!(
                "Frame {frames} has {} atoms; ReShiki reads at most {MAX_FRAME_ATOMS}",
                header.count
            )));
        }
        let incomplete = |line| {
            Error::at(
                line,
                format!(
                    "The file ends before frame {frames}'s {} atoms",
                    header.count
                ),
            )
        };
        if frames > MAX_FRAMES {
            if !xyz::skip_atoms(&mut lines, header.count) {
                return Err(incomplete(text.lines().count() + 1));
            }
            continue;
        }

        let parsed = xyz::read_frame(
            &header,
            &mut lines,
            |name| {
                if let Ok(z) = name.parse::<u8>() {
                    (1..=118).contains(&z).then_some((z, 0))
                } else {
                    let symbol = name.trim_start_matches(|c: char| c.is_ascii_digit());
                    let (z, isotope) = element(symbol)?;
                    let mass = name.strip_suffix(symbol)?;
                    if mass.is_empty() {
                        Some((z, isotope))
                    } else {
                        Some((z, mass.parse().ok()?))
                    }
                }
            },
            |_| Ok(()),
        )
        .map_err(|error| match error {
            FrameError::Invalid(error) => Error::Invalid(error.to_string()),
            FrameError::Incomplete(line) => incomplete(line),
        })?;
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
        } else {
            None
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

#[cfg(test)]
#[path = "tests/xyz.rs"]
mod tests;
