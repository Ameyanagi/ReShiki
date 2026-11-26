//! Cartesian XYZ to quantum-chemistry input, without a drawing import.
use super::{
    args::{ExportInput, Output},
    host::CliError,
    io,
};
use reshiki_agent::ops::budget::Budgets;
use reshiki_io::qc_input::{self, Geometry, Settings};
use std::{fs, io::Write, path::Path};

pub(crate) fn run(
    command: ExportInput,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), CliError> {
    let bytes = io::read_input(Some(&command.input), Budgets::default().max_text_bytes)
        .map_err(CliError::Failed)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| CliError::Failed("The XYZ input must be UTF-8.".into()))?;
    let frame = qc_input::parse_xyz(&text).map_err(CliError::Failed)?;
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
        Output::File(path) => write_files(&path, &main.text, &companions).map_err(CliError::Failed),
    }
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
