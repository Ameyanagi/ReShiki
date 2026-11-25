//! Cartesian XYZ to quantum-chemistry input, without a drawing import.
use super::{
    args::{ExportInput, Output},
    host::CliError,
    io,
};
use reshiki_agent::ops::budget::Budgets;
use reshiki_io::{
    qc_input::{self, Geometry, Settings},
    qcdata::{Frame, MAX_FRAME_ATOMS},
};
use reshiki_model::chemistry::ELEMENTS;
use std::{io::Write, path::Path};

pub(crate) fn run(
    command: ExportInput,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let bytes = io::read_input(Some(&command.input), Budgets::default().max_text_bytes)
        .map_err(CliError::Failed)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| CliError::Failed("The XYZ input must be UTF-8.".into()))?;
    let frame = parse_xyz(&text).map_err(CliError::Failed)?;
    let title = command.title.unwrap_or_else(|| stem(&command.input));
    let settings = Settings {
        program: command.program,
        job: command.job,
        method: command.method,
        basis: command.basis,
        charge: command.charge,
        multiplicity: command.multiplicity,
        cores: command.cores,
        memory_gb: command.memory_gb,
        title,
        extra: command.extra,
        irc_points: 30,
        neb_images: 12,
    };
    let file_stem = match &command.output {
        Output::File(path) => stem(path),
        Output::Stdout => stem(&command.input),
    };
    let geometry = Geometry {
        frame: &frame,
        reactant: None,
        product: None,
        result: None,
    };
    let generator = format!("ReShiki {}", crate::updates::CURRENT_VERSION);
    let output = qc_input::write(&settings, &geometry, &file_stem, &generator)
        .map_err(|error| CliError::Failed(error.to_string()))?;
    for warning in output.warnings {
        let _ = writeln!(err, "warning: {warning}");
    }
    let (main, companions) = (output.main, output.companions);
    match command.output {
        Output::Stdout => out
            .write_all(main.text.as_bytes())
            .map_err(CliError::Stdout),
        Output::File(path) => {
            io::write_output(&path, main.text.as_bytes(), true).map_err(CliError::Failed)?;
            for file in companions {
                io::write_output(&path.with_file_name(&file.name), file.text.as_bytes(), true)
                    .map_err(CliError::Failed)?;
            }
            Ok(())
        }
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .filter(|stem| !stem.is_empty())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "molecule".into())
}

/// Read only the first frame; later frames may contain a different structure.
fn parse_xyz(text: &str) -> Result<Frame, String> {
    let mut lines = text.lines();
    let atoms = lines
        .next()
        .and_then(|line| line.trim().parse::<usize>().ok())
        .ok_or("The XYZ input needs an atom count on the first line.")?;
    if atoms > MAX_FRAME_ATOMS {
        return Err(format!("The XYZ input exceeds {MAX_FRAME_ATOMS} atoms."));
    }
    let comment = lines.next().ok_or("The XYZ input needs a comment line.")?;
    let columns = qc_input::XyzColumns::from_comment(comment)
        .ok_or("XYZ Properties needs species:S:1 and pos:R:3 columns.")?;
    let mut numbers = Vec::with_capacity(atoms);
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
        let number = ELEMENTS
            .iter()
            .position(|element| element.symbol == symbol && element.symbol != "*")
            .and_then(|number| u8::try_from(number).ok())
            .ok_or_else(invalid)?;
        let mut position = [0.0; 3];
        for (offset, coordinate) in position.iter_mut().enumerate() {
            *coordinate = fields
                .get(columns.position + offset)
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .ok_or_else(invalid)?;
        }
        numbers.push(number);
        positions_angstrom.push(position);
    }
    Ok(Frame {
        numbers,
        positions_angstrom,
        isotopes: Vec::new(),
        masses_amu: Vec::new(),
        charge: 0,
        multiplicity: 1,
        drawing_atoms: Vec::new(),
    })
}
