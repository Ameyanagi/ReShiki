//! Cartesian XYZ to quantum-chemistry input, without a drawing import.
use super::{
    args::{ExportInput, Output},
    host::CliError,
    io,
};
use reshiki_agent::ops::budget::Budgets;
use reshiki_io::{
    qc_input::{self, Geometry, Settings},
    qcdata::{self, CalcResult, Frame, MAX_FRAME_ATOMS, Provenance, Source, Status, Task},
};
use reshiki_model::chemistry::ELEMENTS;
use std::{fs, io::Write, path::Path};

pub(crate) fn run(
    command: ExportInput,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let limit = Budgets::default().max_text_bytes;
    let frame = Frame {
        charge: command.charge,
        multiplicity: command.multiplicity,
        ..read_xyz(&command.input, limit).map_err(CliError::Failed)?
    };
    let read_endpoint = |flag, path: Option<&Path>| {
        path.map(|path| {
            read_xyz(path, limit).map_err(|error| CliError::Failed(format!("{flag}: {error}")))
        })
        .transpose()
    };
    let reactant = read_endpoint("--reactant", command.reactant.as_deref())?;
    let product = read_endpoint("--product", command.product.as_deref())?;
    let result = command
        .hessian
        .as_deref()
        .map(|path| read_hessian(path, &frame, limit))
        .transpose()
        .map_err(CliError::Failed)?;
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
        Output::File(path) => file_stem(path),
        Output::Stdout => file_stem(&command.input),
    };
    let geometry = Geometry {
        frame: &frame,
        reactant: reactant.as_ref(),
        product: product.as_ref(),
        result: result.as_ref(),
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
        Output::File(path) => write_files(&path, &main.text, &companions).map_err(CliError::Failed),
    }
}

fn read_hessian(path: &Path, frame: &Frame, limit: usize) -> Result<CalcResult, String> {
    let bytes = io::read_input(Some(path), limit)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{}: the ORCA Hessian must be UTF-8", path.display()))?;
    let hessian = qc_input::read_orca_hess(&text, frame.numbers.len())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let file_name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned();
    Ok(CalcResult {
        frame: frame.hash(),
        input_frame: None,
        task: Task::Hessian,
        endpoints: None,
        settings: qcdata::Settings::Imported { keywords: vec![] },
        provenance: Provenance {
            program: "unknown".into(),
            program_version: None,
            method: Some(format!("the Hessian file {file_name}")),
            basis: None,
            source: Source::Imported {
                format: "orca-hess".into(),
                file_name,
                file_sha256: None,
                job: 0,
            },
        },
        status: Status::Converged,
        energy_hartree: None,
        energy_terms: Default::default(),
        gradient_hartree_per_bohr: vec![],
        hessian_hartree_per_bohr2: hessian,
        charges: vec![],
        dipole_e_bohr: None,
        bond_orders: vec![],
        orbitals: None,
        vibrations: None,
        thermo: None,
        trajectory: None,
        keep_full: false,
        omitted_bytes: None,
        run: None,
    })
}

pub(super) fn write_files(
    path: &Path,
    main: &str,
    companions: &[qc_input::File],
) -> Result<(), String> {
    for file in companions {
        let companion = path.with_file_name(&file.name);
        match fs::symlink_metadata(&companion) {
            Ok(_) => {
                return Err(format!(
                    "{} already exists; remove it or choose another --out name.",
                    file.name
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot open {}: {error}", companion.display())),
        }
    }
    io::write_output(path, main.as_bytes(), true)?;
    for file in companions {
        io::write_output(
            &path.with_file_name(&file.name),
            file.text.as_bytes(),
            false,
        )?;
    }
    Ok(())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .filter(|stem| !stem.is_empty())
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "molecule".into())
}

fn file_stem(path: &Path) -> String {
    stem(path)
        .chars()
        .map(|c| {
            if c.is_whitespace() || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn read_xyz(path: &Path, limit: usize) -> Result<Frame, String> {
    let bytes = io::read_input(Some(path), limit)?;
    let text = String::from_utf8(bytes).map_err(|_| "The XYZ input must be UTF-8.")?;
    parse_xyz(&text)
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
