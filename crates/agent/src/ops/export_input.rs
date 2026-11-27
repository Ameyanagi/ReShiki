//! The `export_input` tool: a quantum-chemistry input from Cartesian XYZ.
use super::{
    budget::Budgets,
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    result::ToolResult,
    wire::warnings,
};
use crate::{
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
};
use reshiki_io::{
    qc_input::{self, Geometry, Settings},
    qcdata::Frame,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const EXPORT_INPUT: ToolSpec = ToolSpec {
    name: "export_input",
    title: Some("Export calculation input"),
    description: "Write Gaussian, ORCA or GAMESS input from the first XYZ frame (Å). Null settings use CLI defaults.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    let text = json!({"type": ["string", "null"]});
    let positive = json!({"type": ["integer", "null"], "minimum": 1});
    json!({
        "type": "object",
        "properties": {
            "xyz": {"type": "string"},
            "program": {"type": "string", "enum": ["gaussian", "orca", "gamess"]},
            "job": {"type": "string", "enum": ["single_point", "optimize", "opt_freq"]},
            "method": text, "basis": text, "title": text, "extra": text,
            "charge": {"type": ["integer", "null"]},
            "multiplicity": positive, "cores": positive, "memory_gb": positive,
        },
        "required": ["xyz", "program", "job", "method", "basis", "charge",
                     "multiplicity", "title", "cores", "memory_gb", "extra"],
        "additionalProperties": false,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Program {
    Gaussian,
    Orca,
    Gamess,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Job {
    SinglePoint,
    Optimize,
    OptFreq,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    xyz: String,
    program: Program,
    job: Job,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    basis: Option<String>,
    #[serde(default)]
    charge: Option<i32>,
    #[serde(default)]
    multiplicity: Option<u32>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    cores: Option<u32>,
    #[serde(default)]
    memory_gb: Option<u32>,
    #[serde(default)]
    extra: Option<String>,
}

pub(crate) struct ExportInput {
    frame: Frame,
    settings: Settings,
}

const STEM: &str = "molecule";

pub(crate) fn xyz_frame(text: &str, budgets: &Budgets) -> Result<Frame, OpError> {
    let limit = budgets.max_text_bytes;
    if text.len() > limit {
        return Err(OpError::new(
            ErrorKind::Budget,
            format!(
                "The XYZ text is {} bytes; max_text_bytes allows at most {limit}",
                text.len()
            ),
        )
        .param("xyz"));
    }
    qc_input::parse_xyz(text).map_err(|error| OpError::invalid(error).param("xyz"))
}

pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<ExportInput, OpError> {
    let args: Arguments = arguments(args)?;
    for (name, value) in [
        ("multiplicity", args.multiplicity),
        ("cores", args.cores),
        ("memory_gb", args.memory_gb),
    ] {
        if value == Some(0) {
            return Err(OpError::invalid(format!("{name} must be at least 1")).param(name));
        }
    }
    let frame = xyz_frame(&args.xyz, budgets)?;
    let program = match args.program {
        Program::Gaussian => qc_input::Program::Gaussian,
        Program::Orca => qc_input::Program::Orca,
        Program::Gamess => qc_input::Program::Gamess,
    };
    let [template, ..] = qc_input::builtin_templates();
    let mut settings = Settings::from_template(
        &template,
        program,
        args.charge.unwrap_or(0),
        args.multiplicity.unwrap_or(1),
        args.title.unwrap_or_else(|| STEM.into()),
    );
    settings.job = match args.job {
        Job::SinglePoint => qc_input::Job::SinglePoint,
        Job::Optimize => qc_input::Job::Optimize,
        Job::OptFreq => qc_input::Job::OptFreq,
    };
    settings.method = args.method.unwrap_or(settings.method);
    if let Some(basis) = args.basis {
        settings.basis = basis;
    } else if program == qc_input::Program::Gamess
        && let Some(basis) = qc_input::nearest_gamess_basis(&settings.basis)
    {
        settings.basis = basis.into();
    }
    settings.cores = args.cores.unwrap_or(settings.cores);
    settings.memory_gb = args.memory_gb.unwrap_or(settings.memory_gb);
    settings.extra = args.extra.unwrap_or(settings.extra);
    Ok(ExportInput { frame, settings })
}

#[derive(Serialize)]
struct InputFile {
    file_name: String,
    text: String,
}

impl From<qc_input::File> for InputFile {
    fn from(file: qc_input::File) -> Self {
        Self {
            file_name: file.name,
            text: file.text,
        }
    }
}

#[derive(Serialize)]
struct ExportedInput {
    #[serde(flatten)]
    main: InputFile,
    companions: Vec<InputFile>,
}

/// Writes the files inline, bounded by the sum of their text bytes.
pub(crate) async fn export_input(
    ctx: Context,
    versions: Versions,
    budgets: Budgets,
    args: ExportInput,
) -> Result<ToolResult, OpError> {
    ctx.blocking(move || {
        let geometry = Geometry {
            frame: &args.frame,
            reactant: None,
            product: None,
            result: None,
        };
        let generator = format!("ReShiki {}", versions.app);
        let stem = qc_input::default_stem(STEM, args.settings.program);
        let output = qc_input::write(&args.settings, &geometry, &stem, &generator)
            .map_err(|error| OpError::failed(error.to_string()))?;
        let bytes = output.main.text.len()
            + output
                .companions
                .iter()
                .map(|file| file.text.len())
                .sum::<usize>();
        if bytes > budgets.max_output_bytes {
            return Err(OpError::new(
                ErrorKind::Budget,
                format!(
                    "The input files total {bytes} bytes; max_output_bytes allows at most {}",
                    budgets.max_output_bytes
                ),
            ));
        }
        Ok(ToolResult::valid(
            ExportedInput {
                main: output.main.into(),
                companions: output.companions.into_iter().map(InputFile::from).collect(),
            },
            warnings(output.warnings),
            versions,
        ))
    })
    .await
}
