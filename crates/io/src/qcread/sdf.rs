//! Molecular geometries from native MOL and SD records.
use super::{Error, Format, Job, JobKind, QcFile, Step, frame, lowest_multiplicity};
use crate::chemistry::molfile::{self, ReadError};
use reshiki_qcdata::{MAX_FRAME_ATOMS, MAX_FRAMES};

/// Read each MOL block as one geometry, preserving the native atom order.
pub(super) fn read(text: &str) -> Result<QcFile, Error> {
    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    let mut program = None;
    let mut program_version = None;
    let mut start = 0;
    let mut end = 0;
    let mut empty_record = None;
    let mut records = 0;
    let record_ends = text.split_inclusive('\n').filter_map(|line| {
        let at = end;
        end += line.len();
        line.starts_with("$$$$").then_some((at, end))
    });
    for (index, (end, next)) in record_ends
        .chain(std::iter::once((text.len(), text.len())))
        .enumerate()
    {
        let k = index + 1;
        let record = text.split_at(start).1.split_at(end - start).0;
        start = next;
        if record.trim().is_empty() {
            empty_record.get_or_insert(k);
            continue;
        }
        records += 1;
        // Only scan delimiter lines after the frame cap. Discarded records
        // need neither MOL parsing nor atom/coordinate allocations.
        if steps.len() == MAX_FRAMES {
            continue;
        }
        if let Some(empty) = empty_record {
            return Err(Error::Invalid(format!("Record {empty} has no molfile")));
        }
        let molfile_end = record
            .split_inclusive('\n')
            .scan(0, |size, line| {
                *size += line.len();
                Some((line, *size))
            })
            .skip(4)
            .find_map(|(line, end)| line.starts_with("M  END").then_some(end))
            .unwrap_or(record.len());
        let imported = molfile::read(record.split_at(molfile_end).0).map_err(|e| match e {
            ReadError::Unsupported(reason) => Error::Unsupported(format!(
                "Record {k} contains unsupported MOL chemistry: {reason}"
            )),
            ReadError::Limit => {
                Error::Unsupported(format!("Record {k} exceeds the MOL size or work limit"))
            }
            _ => Error::Invalid(format!("Record {k}: {e}")),
        })?;
        let is_3d = imported.annotations.is_3d;
        let molecule = imported.molecule;
        let graph = &molecule.state.graph;
        if graph.atoms.len() > MAX_FRAME_ATOMS {
            return Err(Error::Unsupported(format!(
                "Record {k} has {} atoms; ReShiki reads at most {MAX_FRAME_ATOMS}",
                graph.atoms.len()
            )));
        }
        if graph.atoms.iter().any(|a| a.atomic_number == 0) {
            return Err(Error::Unsupported(format!(
                "Record {k} contains query atoms, R groups or dummy atoms; use real elements with coordinates"
            )));
        }
        if graph
            .atoms
            .iter()
            .zip(&molecule.state.valences)
            .any(|(a, v)| a.explicit_hydrogens != 0 || v.implicit_hydrogens != 0)
        {
            return Err(Error::Unsupported(format!(
                "Record {k} has implicit hydrogens; a 3D calculation needs every atom with coordinates"
            )));
        }
        let atoms: Vec<_> = graph
            .atoms
            .iter()
            .map(|a| (a.atomic_number, a.isotope))
            .collect();
        let charge = graph.atoms.iter().map(|a| i32::from(a.charge)).sum();
        let (radical_centres, radicals) = graph
            .atoms
            .iter()
            .zip(&imported.annotations.singlets)
            .filter(|(atom, singlet)| !**singlet && atom.radical_electrons != 0)
            .fold((0usize, 0u32), |(centres, electrons), (atom, _)| {
                (centres + 1, electrons + u32::from(atom.radical_electrons))
            });
        let lowest = lowest_multiplicity(&atoms, charge);
        let multiplicity = if (1 + radicals) % 2 == lowest % 2 {
            if radical_centres > 1 {
                warnings.push(format!(
                    "Record {k}: several radical centres; assumed the high-spin state"
                ));
            }
            1 + radicals
        } else {
            warnings.push(format!(
                "Record {k}: radical count disagrees with electron parity; assumed the lowest spin state"
            ));
            lowest
        };
        let positions: Vec<_> = molecule.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
        let label = record.lines().next().unwrap_or_default().trim();
        // A planar molecule in a file marked 3D is fine; only a 2D record is flagged.
        if !is_3d {
            warnings.push(format!("Record {k} is 2D (all z = 0)"));
        }
        if let Some(version) = label.strip_prefix("xtb:") {
            program.get_or_insert_with(|| "xtb".into());
            if !version.trim().is_empty() {
                program_version.get_or_insert_with(|| version.trim().into());
            }
        }
        // ReShiki's conformer export writes each record's energy and the
        // calculated state (which a typed charge can set apart from the
        // drawn formal charges) as data items.
        let item = |name: &str| {
            let mut data = record.split_at(molfile_end).1.lines();
            data.find(|line| line.starts_with('>') && line.contains(&format!("<{name}>")))
                .and_then(|_| data.next())
                .map(str::trim)
        };
        let energy_hartree = item("ENERGY_HARTREE")
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|e| e.is_finite());
        let charge = item("TOTAL_CHARGE")
            .and_then(|value| value.parse().ok())
            .unwrap_or(charge);
        let multiplicity = item("MULTIPLICITY")
            .and_then(|value| value.parse().ok())
            .unwrap_or(multiplicity);
        steps.push(Step {
            frame: frame(&atoms, positions, charge, multiplicity)?,
            energy_hartree,
            label: (!label.is_empty()).then(|| label.into()),
        });
    }

    if records > MAX_FRAMES {
        warnings.push(format!("Read the first {MAX_FRAMES} of {records} records"));
    }

    Ok(QcFile {
        format: Format::Sdf,
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
#[path = "tests/sdf.rs"]
mod tests;
