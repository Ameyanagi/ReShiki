//! Cartesian XYZ to quantum-chemistry input, without a drawing import.
use super::{
    args::{ExportInput, Output},
    host::CliError,
    io,
};
use reshiki_agent::ops::budget::Budgets;
use reshiki_io::{
    qc_input::{self, Geometry, Settings},
    qcdata::{self, CalcResult, Frame, Provenance, Source, Status, Task},
};
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
        Output::File(path) => file_stem(path, command.program),
        Output::Stdout => {
            qc_input::default_stem(&file_stem(&command.input, command.program), command.program)
        }
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
    let hessian = qc_input::read_orca_hess(&text, frame)
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

fn file_stem(path: &Path, program: qc_input::Program) -> String {
    let stem = stem(path);
    if program == qc_input::Program::Orca {
        return stem;
    }
    stem.chars()
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
    qc_input::parse_xyz(&text)
}
