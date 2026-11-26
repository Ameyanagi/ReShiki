//! Token parsing for the grammar in [`commands::COMMANDS`], described by
//! `reshiki --cli help`.
//!
//! Tokens stay [`OsString`]s, so paths need not be UTF-8. `--` ends the
//! options, and syntax errors are usage errors (exit 2) found before anything
//! is read or run.
use super::{
    commands::{self, CommandSpec},
    input_format, input_formats, output_formats, render_formats,
};
use reshiki_io::qc_input::{Job, Program, builtin_templates, nearest_gamess_basis};
use reshiki_xtb_worker::protocol::{
    Conventions, Method, ScfRetry, ScfSettings, SolvationState, Solvent, SolventCorrection,
    SolventModel,
};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

/// A parsed command line.
#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    Help {
        topic: Option<&'static CommandSpec>,
        json: bool,
    },
    Info,
    Version,
    Calc(Calc),
    Ts(Ts),
    Calculate(Calculate),
    ReadOutput(ReadOutput),
    Tools,
    Schema(Schema),
    Skill(Skill),
    Convert(Convert),
    Render(Render),
    Compose(Compose),
    Analyze(Input),
    PredictNmr(PredictNmr),
    ExportInput(ExportInput),
    Name(Name),
    PiecesCheck(Vec<PathBuf>),
    PiecesRender(PieceRender),
}

/// `calc`: the first XYZ frame and one xTB calculation.
#[derive(Debug, PartialEq)]
pub(crate) struct Calc {
    pub(crate) file: PathBuf,
    pub(crate) task: CalcTask,
    pub(crate) method: Method,
    pub(crate) gradient: bool,
    pub(crate) bond_orders: bool,
    pub(crate) charge: i32,
    pub(crate) uhf: Option<u32>,
    pub(crate) conventions: Conventions,
    pub(crate) solvent: Option<Solvent>,
    pub(crate) scf: Option<ScfSettings>,
    pub(crate) thermo_temperature_k: Option<f64>,
    pub(crate) threads: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CalcTask {
    Energy,
    Gradient,
    Hessian,
    Orbitals,
}

/// `ts`: mapped XYZ endpoints, or one XYZ with directed bond changes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Ts {
    pub(crate) reactant: Option<PathBuf>,
    pub(crate) product: Option<PathBuf>,
    /// Pairs are canonicalized and use one-based XYZ atom numbers.
    pub(crate) form: Vec<(usize, usize)>,
    pub(crate) break_: Vec<(usize, usize)>,
    pub(crate) charge: i32,
    pub(crate) uhf: Option<u32>,
    pub(crate) images: usize,
    pub(crate) workers: usize,
    pub(crate) remap_hydrogens: bool,
    pub(crate) keep_endpoints: bool,
    pub(crate) out_dir: Option<PathBuf>,
    pub(crate) json: Option<PathBuf>,
}

/// `calculate`: one XYZ file and the compact calculation tool's settings.
#[derive(Debug, PartialEq)]
pub(crate) struct Calculate {
    pub(crate) file: PathBuf,
    pub(crate) task: &'static str,
    pub(crate) method: &'static str,
    pub(crate) charge: i32,
    pub(crate) uhf: u32,
    pub(crate) solvent: Option<String>,
    pub(crate) conventions: &'static str,
    pub(crate) electronic_temperature_k: Option<f64>,
    pub(crate) accuracy: Option<f64>,
    pub(crate) max_iterations: Option<u32>,
    pub(crate) threads: Option<u32>,
    pub(crate) convergence: Option<&'static str>,
    pub(crate) fixed_atoms: Option<Vec<u32>>,
}

/// `read-output`: a quantum chemistry file and an optional zero-based job.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadOutput {
    pub(crate) file: PathBuf,
    pub(crate) job: Option<usize>,
}

/// A schema the CLI can print without a tool host.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Schema {
    Proposal,
}

/// Where the input comes from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Source {
    File(PathBuf),
    Stdin,
    Smiles(String),
}

/// The input and its import format, one of the ops import formats.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Input {
    pub(crate) source: Source,
    pub(crate) format: String,
}

/// `predict-nmr`: one molecule and the nucleus, `1H` or `13C`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PredictNmr {
    pub(crate) input: Input,
    pub(crate) nucleus: &'static str,
}

/// `export-input`: the first Cartesian XYZ frame and calculation settings.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ExportInput {
    pub(crate) input: PathBuf,
    pub(crate) program: Program,
    pub(crate) job: Job,
    pub(crate) reactant: Option<PathBuf>,
    pub(crate) product: Option<PathBuf>,
    pub(crate) hessian: Option<PathBuf>,
    pub(crate) method: String,
    pub(crate) basis: String,
    pub(crate) charge: i32,
    pub(crate) multiplicity: u32,
    pub(crate) cores: u32,
    pub(crate) memory_gb: u32,
    pub(crate) title: Option<String>,
    pub(crate) extra: String,
    pub(crate) output: Output,
}

/// `name`: one structure, with optional preference status in JSON.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Name {
    pub(crate) input: Input,
    pub(crate) json: bool,
    pub(crate) locants: bool,
    pub(crate) isotope_style: crate::naming::IsotopeStyle,
}

/// Where the output goes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Output {
    Stdout,
    File(PathBuf),
}

/// How SVG labels are written, passed to the tools as `svg_text`; other
/// output formats ignore it.
pub(crate) use reshiki_agent::ops::wire::SvgText;

/// The file convert and compose write: `format` is one of the ops export
/// formats.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Export {
    pub(crate) format: String,
    pub(crate) svg_text: SvgText,
    pub(crate) output: Output,
    pub(crate) pages: bool,
    pub(crate) force: bool,
    pub(crate) receipt: bool,
}

/// `convert`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Convert {
    pub(crate) input: Input,
    pub(crate) export: Export,
    pub(crate) labels: Option<Labels>,
}

/// `convert --nucleus NUCLEUS --labels FORMAT`: the predicted shifts to
/// write onto the atoms, and how.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Labels {
    pub(crate) nucleus: &'static str,
    pub(crate) format: String,
    pub(crate) decimals: u8,
}

/// The output formats that keep atom numbers, so `--labels` is allowed.
pub(crate) const LABEL_FORMATS: &[&str] = &["svg", "png", "pdf", "cdxml", "cdx"];

/// `render`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Render {
    pub(crate) input: Input,
    pub(crate) preview: Preview,
}

/// `pieces render`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PieceRender {
    pub(crate) file: PathBuf,
    pub(crate) dark: bool,
    pub(crate) preview: Preview,
}

/// The image to write. `width` and `height` are `None` for the tool's default.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Preview {
    pub(crate) format: String,
    pub(crate) svg_text: SvgText,
    pub(crate) output: Output,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) force: bool,
}

/// `compose`: the Proposal file, or `None` for standard input.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Compose {
    pub(crate) proposal: Option<PathBuf>,
    pub(crate) export: Export,
}

/// `skill`: an existing parent folder and the generated skill's identity.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Skill {
    pub(crate) folder: PathBuf,
    pub(crate) name: String,
    pub(crate) executable: Option<PathBuf>,
    pub(crate) force: bool,
    pub(crate) receipt: bool,
}

/// Parses `name` and the tokens after it; `Err` is the usage message.
pub(crate) fn parse(name: &OsStr, rest: &[OsString]) -> Result<Command, String> {
    let lossy = name.to_string_lossy();
    if matches!(lossy.as_ref(), "--help" | "-h") {
        return help(rest);
    }
    if lossy == "--version" {
        return if rest.is_empty() {
            Ok(Command::Version)
        } else {
            Err("--version takes no arguments".into())
        };
    }
    let spec = commands::find(&lossy).ok_or_else(|| unknown_command(&lossy))?;
    match spec.name {
        "help" => help(rest),
        "info" if rest.is_empty() => Ok(Command::Info),
        "info" => Err("info takes no arguments".into()),
        "tools" if rest.is_empty() => Ok(Command::Tools),
        "tools" => Err("tools takes no arguments".into()),
        "schema" => schema(&options(spec, rest)?),
        "skill" => skill(&options(spec, rest)?),
        "pieces" => pieces(spec, rest),
        "calc" => calc(&options(spec, rest)?),
        "ts" => ts(&options(spec, rest)?),
        "calculate" => calculate(&options(spec, rest)?),
        "read-output" => read_output(&options(spec, rest)?),
        "convert" => convert(&options(spec, rest)?),
        "render" => render(&options(spec, rest)?),
        "compose" => compose(&options(spec, rest)?),
        "analyze" => input(spec.name, &options(spec, rest)?).map(Command::Analyze),
        "predict-nmr" => {
            let options = options(spec, rest)?;
            let input = input(spec.name, &options)?;
            let nucleus = options
                .nucleus
                .as_deref()
                .ok_or("predict-nmr needs --nucleus 1H or 13C")?;
            Ok(Command::PredictNmr(PredictNmr {
                input,
                nucleus: nucleus_code(nucleus)?,
            }))
        }
        "export-input" => export_input(&options(spec, rest)?),
        "name" => {
            let options = options(spec, rest)?;
            Ok(Command::Name(Name {
                input: input(spec.name, &options)?,
                json: options.json,
                locants: options.locants,
                isotope_style: match options.isotope_style.as_deref().map(OsStr::to_str) {
                    None | Some(Some("iupac")) => crate::naming::IsotopeStyle::Iupac,
                    Some(Some("boughton")) => crate::naming::IsotopeStyle::Boughton,
                    _ => return Err("--isotope-style must be iupac or boughton".into()),
                },
            }))
        }
        _ => Err(unknown_command(&lossy)),
    }
}

fn calc(options: &Options) -> Result<Command, String> {
    let file = options
        .input
        .as_ref()
        .filter(|file| !file.is_empty() && *file != "-")
        .map(PathBuf::from)
        .ok_or("calc needs an XYZ file: FILE.xyz")?;
    if !file
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xyz"))
    {
        return Err("calc needs an XYZ file: FILE.xyz".into());
    }
    let charge = charge(options.charge.as_deref())?;
    let uhf = uhf(options.uhf.as_deref())?;
    let conventions = match options.conventions.as_deref().map(OsStr::to_str) {
        None | Some(Some("axtb")) => Conventions::Axtb,
        Some(Some("xtb-6.8")) => Conventions::Xtb68,
        _ => return Err("--conventions must be axtb or xtb-6.8".into()),
    };
    let task = match options.task.as_deref().map(OsStr::to_str) {
        None | Some(Some("energy")) => CalcTask::Energy,
        Some(Some("gradient")) => CalcTask::Gradient,
        Some(Some("hessian")) => CalcTask::Hessian,
        Some(Some("orbitals")) => CalcTask::Orbitals,
        _ => return Err("--task must be energy, gradient, hessian or orbitals".into()),
    };
    if task != CalcTask::Hessian && options.thermo_temperature.is_some() {
        return Err("--thermo-temperature needs --task hessian".into());
    }
    let method = match options.method.as_deref().map(OsStr::to_str) {
        None | Some(Some("gfn2")) => Method::Gfn2,
        Some(Some("gfnff")) => Method::Gfnff,
        Some(Some("gxtb")) => Method::Gxtb,
        _ => return Err("--method must be gfn2, gfnff or gxtb".into()),
    };
    let correction = match options.solvent_correction.as_deref().map(OsStr::to_str) {
        None => None,
        Some(Some("gfn2")) => Some(SolventCorrection::Gfn2),
        _ => return Err("--solvent-correction must be gfn2".into()),
    };
    let solvent = match (&options.alpb, &options.gbsa) {
        (Some(_), Some(_)) => return Err("--alpb and --gbsa cannot be combined".into()),
        (None, None) => {
            if correction.is_some() {
                return Err("--solvent-correction needs --alpb or --gbsa".into());
            }
            None
        }
        (alpb, gbsa) => {
            let (model, name) = match (alpb, gbsa) {
                (Some(name), _) => (SolventModel::Alpb, name),
                (_, Some(name)) => (SolventModel::Gbsa, name),
                _ => return Err("a solvent name is required".into()),
            };
            let name = name
                .to_str()
                .filter(|name| !name.trim().is_empty())
                .ok_or("the solvent name must be nonempty UTF-8 text")?;
            Some(Solvent {
                model,
                name: name.into(),
                state: SolvationState::Gsolv,
                correction,
            })
        }
    };
    let temperature = |name, value: &OsStr| {
        value
            .to_str()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| format!("{name} must be finite and at least 0 K"))
    };
    let iterations = |name, value: &OsStr| {
        value
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{name} must be a whole number from 1 to 4294967295"))
    };
    let electronic_temperature_k = options
        .etemp
        .as_deref()
        .map(|value| temperature("--etemp", value))
        .transpose()?;
    let thermo_temperature_k = options
        .thermo_temperature
        .as_deref()
        .map(|value| {
            value
                .to_str()
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| *value > 0.0 && *value <= 10_000.0)
                .ok_or("--thermo-temperature must be greater than 0 K and at most 10000 K")
        })
        .transpose()?;
    let accuracy = options
        .accuracy
        .as_deref()
        .map(|value| {
            value
                .to_str()
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value > 0.0)
                .ok_or("--accuracy must be finite and positive")
        })
        .transpose()?;
    let max_iterations = options
        .max_iter
        .as_deref()
        .map(|value| iterations("--max-iter", value))
        .transpose()?;
    let mut retries = Vec::with_capacity(options.scf_retry.len());
    for retry in &options.scf_retry {
        let text = retry.to_str().ok_or("--scf-retry must be K or K:N")?;
        let (kelvin, count) = text
            .split_once(':')
            .map_or((text, None), |(kelvin, count)| (kelvin, Some(count)));
        let kelvin = temperature("--scf-retry temperature", OsStr::new(kelvin))?;
        if electronic_temperature_k.is_some_and(|initial| kelvin < initial) {
            return Err("--scf-retry temperature must be at least the explicit --etemp".into());
        }
        retries.push(ScfRetry {
            electronic_temperature_k: Some(kelvin),
            max_iterations: count
                .map(|value| iterations("--scf-retry iterations", OsStr::new(value)))
                .transpose()?,
        });
    }
    let scf = if electronic_temperature_k.is_some()
        || accuracy.is_some()
        || max_iterations.is_some()
        || !retries.is_empty()
    {
        Some(ScfSettings {
            accuracy,
            electronic_temperature_k,
            max_iterations,
            retries,
        })
    } else {
        None
    };
    let threads = options
        .threads
        .as_deref()
        .map(|value| {
            value
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| (1..=64).contains(value))
                .ok_or("--threads must be a whole number from 1 to 64")
        })
        .transpose()?
        .unwrap_or(1);
    Ok(Command::Calc(Calc {
        file,
        task,
        method,
        gradient: options.gradient,
        bond_orders: options.wbo,
        charge,
        uhf,
        conventions,
        solvent,
        scf,
        thermo_temperature_k,
        threads,
    }))
}

fn charge(value: Option<&OsStr>) -> Result<i32, &'static str> {
    optional_integer("--charge", value)
        .map(|charge| charge.unwrap_or_default())
        .map_err(|_| "--charge must be a whole number in the signed 32-bit range")
}

fn uhf(value: Option<&OsStr>) -> Result<Option<u32>, &'static str> {
    optional_integer("--uhf", value)
        .map_err(|_| "--uhf must be a nonnegative whole number in the unsigned 32-bit range")
}

fn ts(options: &Options) -> Result<Command, String> {
    if options.input.is_some() {
        return Err("ts takes no positional input; use --reactant or --product".into());
    }
    let path = |flag, value: Option<&OsStr>| {
        value
            .map(|value| {
                if value.is_empty() || value == "-" {
                    Err(format!("{flag} needs a file or directory path"))
                } else {
                    Ok(PathBuf::from(value))
                }
            })
            .transpose()
    };
    let reactant = path("--reactant", options.reactant.as_deref())?;
    let product = path("--product", options.product.as_deref())?;
    let has_edits = !options.form.is_empty() || !options.break_.is_empty();
    if options.remap_hydrogens && has_edits {
        return Err(
            "--remap-hydrogens requires two XYZ endpoints without --form or --break".into(),
        );
    }
    match (&reactant, &product, has_edits) {
        (None, None, _) => return Err("ts needs --reactant or --product".into()),
        (Some(_), Some(_), true) => {
            return Err("--form and --break require exactly one XYZ endpoint".into());
        }
        (Some(_), None, false) | (None, Some(_), false) => {
            return Err("one XYZ endpoint requires at least one --form or --break".into());
        }
        _ => {}
    }
    let form = ts_pairs("--form", &options.form)?;
    let break_ = ts_pairs("--break", &options.break_)?;
    if let Some(&(i, j)) = form.iter().find(|pair| break_.contains(pair)) {
        return Err(format!(
            "atom pair {i}-{j} cannot be both formed and broken"
        ));
    }
    let count = |value: Option<&OsStr>, flag, min, max| {
        value
            .map(|value| {
                value
                    .to_str()
                    .and_then(|text| text.parse::<usize>().ok())
                    .filter(|value| (min..=max).contains(value))
                    .ok_or_else(|| format!("{flag} must be a whole number from {min} to {max}"))
            })
            .transpose()
    };
    let images = count(options.images.as_deref(), "--images", 5, 24)?
        .unwrap_or_else(|| reshiki_optimize::ts::TsOptions::default().images);
    let workers = count(options.workers.as_deref(), "--workers", 1, 8)?.unwrap_or_else(|| {
        let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        4.min(cpus).min(images - 2)
    });
    Ok(Command::Ts(Ts {
        reactant,
        product,
        form,
        break_,
        charge: charge(options.charge.as_deref())?,
        uhf: uhf(options.uhf.as_deref())?,
        images,
        workers,
        remap_hydrogens: options.remap_hydrogens,
        keep_endpoints: options.keep_endpoints,
        out_dir: path("--out-dir", options.out_dir.as_deref())?,
        json: path("--json", options.json_file.as_deref())?,
    }))
}

fn ts_pairs(flag: &str, values: &[OsString]) -> Result<Vec<(usize, usize)>, String> {
    fn atom(text: &str) -> Option<usize> {
        if !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        text.parse::<usize>().ok().filter(|&value| value > 0)
    }
    values
        .iter()
        .map(|value| {
            let pair = value.to_str().and_then(|text| {
                let (i, j) = text.split_once('-')?;
                Some((atom(i)?, atom(j)?))
            });
            match pair {
                Some((i, j)) if i != j => Ok((i.min(j), i.max(j))),
                _ => Err(format!(
                    "{flag} needs two distinct positive atom numbers as I-J, got `{}`",
                    value.to_string_lossy()
                )),
            }
        })
        .collect()
}

fn calculate(options: &Options) -> Result<Command, String> {
    let file = options
        .input
        .as_ref()
        .filter(|file| !file.is_empty() && *file != "-")
        .map(PathBuf::from)
        .filter(|file| {
            file.extension()
                .and_then(OsStr::to_str)
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xyz"))
        })
        .ok_or("calculate needs an XYZ file: FILE.xyz")?;
    let task = match option_text("--task", options.task.as_deref())? {
        None | Some("energy") => "energy",
        Some("optimize") => "optimize",
        Some("frequencies") => "frequencies",
        Some("opt+freq") => "opt+freq",
        _ => return Err("--task must be energy, optimize, frequencies or opt+freq".into()),
    };
    let method = match option_text("--method", options.method.as_deref())? {
        None | Some("gfn2") => "gfn2",
        Some("gxtb") => "gxtb",
        Some("gfnff") => "gfnff",
        _ => return Err("--method must be gfn2, gxtb or gfnff".into()),
    };
    let conventions = match option_text("--conventions", options.conventions.as_deref())? {
        None | Some("axtb") => "axtb",
        Some("xtb-6.8") => "xtb-6.8",
        _ => return Err("--conventions must be axtb or xtb-6.8".into()),
    };
    let uhf = integer("--uhf", options.uhf.as_deref(), 0_u32)?;
    if uhf == u32::MAX {
        return Err("--uhf is too large for a spin multiplicity".into());
    }
    let solvent = option_text("--solvent", options.solvent.as_deref())?
        .map(|text| {
            if text.trim().is_empty() {
                Err("--solvent needs a nonempty name")
            } else {
                Ok(text.to_owned())
            }
        })
        .transpose()?;
    let electronic_temperature_k = option_text("--etemp", options.etemp.as_deref())?
        .map(|text| {
            text.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or("--etemp must be finite and at least 0 K")
        })
        .transpose()?;
    let accuracy = option_text("--accuracy", options.accuracy.as_deref())?
        .map(|text| {
            text.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
                .ok_or("--accuracy must be finite and positive")
        })
        .transpose()?;
    let max_iterations = option_text("--max-iter", options.max_iter.as_deref())?
        .map(|text| {
            text.parse::<u32>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or("--max-iter must be a whole number from 1 to 4294967295")
        })
        .transpose()?;
    let threads = option_text("--threads", options.threads.as_deref())?
        .map(|text| {
            text.parse::<u32>()
                .ok()
                .filter(|value| (1..=64).contains(value))
                .ok_or("--threads must be a whole number from 1 to 64")
        })
        .transpose()?;
    let convergence = match option_text("--convergence", options.convergence.as_deref())? {
        None => None,
        Some("normal") => Some("normal"),
        Some("tight") => Some("tight"),
        _ => return Err("--convergence must be normal or tight".into()),
    };
    let fixed_atoms = option_text("--fix", options.fix.as_deref())?
        .map(|text| {
            let mut atoms = Vec::new();
            for atom in text.split(',') {
                let atom = atom
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .filter(|atom| *atom > 0)
                    .ok_or("--fix must list atom numbers starting at 1, separated by commas")?;
                if atoms.contains(&atom) {
                    return Err("--fix atom numbers must be unique");
                }
                atoms.push(atom);
            }
            Ok(atoms)
        })
        .transpose()?;
    if !matches!(task, "optimize" | "opt+freq") {
        if convergence.is_some() {
            return Err("--convergence requires --task optimize or opt+freq".into());
        }
        if fixed_atoms.is_some() {
            return Err("--fix requires --task optimize or opt+freq".into());
        }
    }
    Ok(Command::Calculate(Calculate {
        file,
        task,
        method,
        charge: integer("--charge", options.charge.as_deref(), 0)?,
        uhf,
        solvent,
        conventions,
        electronic_temperature_k,
        accuracy,
        max_iterations,
        threads,
        convergence,
        fixed_atoms,
    }))
}

fn read_output(options: &Options) -> Result<Command, String> {
    let file = options
        .input
        .as_ref()
        .filter(|file| !file.is_empty() && *file != "-")
        .map(PathBuf::from)
        .ok_or("read-output needs a file: FILE")?;
    let job = option_text("--job", options.job.as_deref())?
        .map(|text| {
            text.parse::<usize>()
                .map_err(|_| "--job must be a nonnegative whole number")
        })
        .transpose()?;
    Ok(Command::ReadOutput(ReadOutput { file, job }))
}

fn pieces(spec: &CommandSpec, rest: &[OsString]) -> Result<Command, String> {
    let Some((sub, tokens)) = rest.split_first() else {
        return Err("pieces needs a subcommand: check or render".into());
    };
    match sub.to_str() {
        Some("check") => pieces_check(tokens),
        Some("render") => {
            let mut options = parse_options("pieces render", spec, tokens)?;
            let file = options.input.take().ok_or("pieces render needs a FILE")?;
            options.to.get_or_insert_with(|| "svg".into());
            Ok(Command::PiecesRender(PieceRender {
                file: file.into(),
                dark: options.dark,
                preview: preview(&options)?,
            }))
        }
        _ => Err(format!(
            "unknown pieces subcommand `{}`; the subcommands are check and render",
            sub.to_string_lossy()
        )),
    }
}

/// `pieces check FILE…`: one or more files.
fn pieces_check(files: &[OsString]) -> Result<Command, String> {
    let mut paths = Vec::new();
    let mut options_ended = false;
    for token in files {
        if !options_ended && token == "--" {
            options_ended = true;
        } else if !options_ended && token.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option `{}`", token.to_string_lossy()));
        } else {
            paths.push(PathBuf::from(token));
        }
    }
    if paths.is_empty() {
        return Err("pieces check needs at least one FILE".into());
    }
    Ok(Command::PiecesCheck(paths))
}

/// The usage message for a command name that is not in the table.
fn unknown_command(name: &str) -> String {
    format!("unknown command `{name}`; run `reshiki --cli help`")
}

/// The usage message for an option `command` does not take: a file command's
/// flag names the command; any other flag, such as help's --json, stays an
/// unknown option there, as it was before another command took it. `name`
/// is excluded from FILE_COMMANDS to preserve these errors for --json.
fn foreign_option(command: &str, token: &OsStr) -> String {
    match commands::FILE_COMMANDS
        .iter()
        .flat_map(|command| command.options)
        .find(|option| token == option.flag)
    {
        Some(known) => format!("{command} does not take {}", known.flag),
        None => format!("unknown option `{}`", token.to_string_lossy()),
    }
}

/// `help [--json] [COMMAND]`; without `--json`, keep the original grammar.
fn help(rest: &[OsString]) -> Result<Command, String> {
    let mut json = false;
    let mut topics = Vec::new();
    for token in rest {
        if token == "--json" {
            if json {
                return Err("--json given more than once".into());
            }
            json = true;
        } else {
            topics.push(token);
        }
    }
    let [topic] = topics.as_slice() else {
        return if topics.is_empty() {
            Ok(Command::Help { topic: None, json })
        } else {
            Err("help takes at most one command".into())
        };
    };
    let name = topic.to_string_lossy();
    let topic = commands::find(&name).ok_or_else(|| unknown_command(&name))?;
    Ok(Command::Help {
        topic: Some(topic),
        json,
    })
}

/// `schema NAME`; reuse the standard option and positional-input errors.
fn schema(options: &Options) -> Result<Command, String> {
    let name = options
        .input
        .as_ref()
        .ok_or("schema needs a name: proposal")?
        .to_string_lossy();
    match name.as_ref() {
        "proposal" => Ok(Command::Schema(Schema::Proposal)),
        _ => Err(format!("unknown schema `{name}`; supported: proposal")),
    }
}

/// The tokens after a command.
#[derive(Default)]
struct Options {
    /// INPUT, PROPOSAL, FILE or `-`.
    input: Option<OsString>,
    smiles: Option<OsString>,
    from: Option<OsString>,
    to: Option<OsString>,
    output: Option<OsString>,
    width: Option<OsString>,
    height: Option<OsString>,
    name: Option<OsString>,
    executable: Option<OsString>,
    isotope_style: Option<OsString>,
    nucleus: Option<OsString>,
    labels: Option<OsString>,
    decimals: Option<OsString>,
    program: Option<OsString>,
    job: Option<OsString>,
    template: Option<OsString>,
    method: Option<OsString>,
    basis: Option<OsString>,
    charge: Option<OsString>,
    multiplicity: Option<OsString>,
    cores: Option<OsString>,
    memory_gb: Option<OsString>,
    title: Option<OsString>,
    extra: Option<OsString>,
    uhf: Option<OsString>,
    conventions: Option<OsString>,
    task: Option<OsString>,
    alpb: Option<OsString>,
    gbsa: Option<OsString>,
    solvent_correction: Option<OsString>,
    etemp: Option<OsString>,
    thermo_temperature: Option<OsString>,
    accuracy: Option<OsString>,
    max_iter: Option<OsString>,
    scf_retry: Vec<OsString>,
    threads: Option<OsString>,
    reactant: Option<OsString>,
    product: Option<OsString>,
    hessian: Option<OsString>,
    form: Vec<OsString>,
    break_: Vec<OsString>,
    images: Option<OsString>,
    workers: Option<OsString>,
    remap_hydrogens: bool,
    keep_endpoints: bool,
    out_dir: Option<OsString>,
    json_file: Option<OsString>,
    solvent: Option<OsString>,
    convergence: Option<OsString>,
    fix: Option<OsString>,
    svg_text: SvgText,
    pages: bool,
    dark: bool,
    force: bool,
    receipt: bool,
    json: bool,
    locants: bool,
    gradient: bool,
    wbo: bool,
}

/// Splits `tokens` into the command's options and one positional input.
fn options(spec: &CommandSpec, tokens: &[OsString]) -> Result<Options, String> {
    parse_options(spec.name, spec, tokens)
}

fn parse_options(
    command: &str,
    spec: &CommandSpec,
    tokens: &[OsString],
) -> Result<Options, String> {
    let mut options = Options::default();
    let mut seen: Vec<&str> = Vec::new();
    let mut ended = false;
    let mut tokens = tokens.iter();
    while let Some(token) = tokens.next() {
        if !ended && token == "--" {
            ended = true;
            continue;
        }
        if ended || token == "-" || !token.as_encoded_bytes().starts_with(b"-") {
            if options.input.is_some() {
                return Err(format!(
                    "{command} takes one input; unexpected `{}`",
                    token.to_string_lossy()
                ));
            }
            options.input = Some(token.clone());
            continue;
        }
        let Some(option) = spec.options.iter().find(|option| token == option.flag) else {
            return Err(foreign_option(command, token));
        };
        let name = option.flag;
        if !matches!(name, "--scf-retry" | "--form" | "--break") && seen.contains(&name) {
            return Err(format!("{name} given more than once"));
        }
        seen.push(name);
        if option.value.is_some() {
            // The next token is the value, whatever it looks like.
            let value = tokens
                .next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))?;
            match name {
                "--smiles" => options.smiles = Some(value),
                "--from" => options.from = Some(value),
                "--to" => options.to = Some(value),
                "--width" => options.width = Some(value),
                "--height" => options.height = Some(value),
                "--name" => options.name = Some(value),
                "--executable" => options.executable = Some(value),
                "--isotope-style" => options.isotope_style = Some(value),
                "--nucleus" => options.nucleus = Some(value),
                "--labels" => options.labels = Some(value),
                "--decimals" => options.decimals = Some(value),
                "--program" => options.program = Some(value),
                "--job" => options.job = Some(value),
                "--template" => options.template = Some(value),
                "--method" => options.method = Some(value),
                "--basis" => options.basis = Some(value),
                "--charge" => options.charge = Some(value),
                "--multiplicity" => options.multiplicity = Some(value),
                "--cores" => options.cores = Some(value),
                "--memory-gb" => options.memory_gb = Some(value),
                "--title" => options.title = Some(value),
                "--extra" => options.extra = Some(value),
                "--uhf" => options.uhf = Some(value),
                "--conventions" => options.conventions = Some(value),
                "--task" => options.task = Some(value),
                "--alpb" => options.alpb = Some(value),
                "--gbsa" => options.gbsa = Some(value),
                "--solvent-correction" => options.solvent_correction = Some(value),
                "--etemp" => options.etemp = Some(value),
                "--thermo-temperature" => options.thermo_temperature = Some(value),
                "--accuracy" => options.accuracy = Some(value),
                "--max-iter" => options.max_iter = Some(value),
                "--scf-retry" => options.scf_retry.push(value),
                "--threads" => options.threads = Some(value),
                "--reactant" => options.reactant = Some(value),
                "--product" => options.product = Some(value),
                "--hessian" => options.hessian = Some(value),
                "--form" => options.form.push(value),
                "--break" => options.break_.push(value),
                "--images" => options.images = Some(value),
                "--workers" => options.workers = Some(value),
                "--out-dir" => options.out_dir = Some(value),
                "--json" => options.json_file = Some(value),
                "--solvent" => options.solvent = Some(value),
                "--convergence" => options.convergence = Some(value),
                "--fix" => options.fix = Some(value),
                "--svg-text" => {
                    options.svg_text = match value.to_str() {
                        Some("live") => SvgText::Live,
                        Some("outline") => SvgText::Outline,
                        _ => return Err("--svg-text must be live or outline".into()),
                    };
                }
                _ => options.output = Some(value),
            }
        } else {
            match name {
                "--pages" => options.pages = true,
                "--dark" => options.dark = true,
                "--force" => options.force = true,
                "--receipt" => options.receipt = true,
                "--json" => options.json = true,
                "--locants" => options.locants = true,
                "--gradient" => options.gradient = true,
                "--wbo" => options.wbo = true,
                "--remap-hydrogens" => options.remap_hydrogens = true,
                "--keep-endpoints" => options.keep_endpoints = true,
                _ => {}
            }
        }
    }
    Ok(options)
}

/// A portable Agent Skill name, also used as exactly one directory component.
pub(super) fn valid_skill_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && !name.contains("claude")
        && !name.contains("anthropic")
}

/// `skill`: the folder that receives NAME, the name and the executable path.
fn skill(options: &Options) -> Result<Command, String> {
    if let Some(input) = &options.input {
        return Err(format!(
            "skill takes no input; unexpected `{}`",
            input.to_string_lossy()
        ));
    }
    let folder = options
        .output
        .as_ref()
        .filter(|path| !path.is_empty() && *path != "-")
        .ok_or("skill writes a folder; pass -o FOLDER")?;
    let name = options
        .name
        .as_deref()
        .unwrap_or(OsStr::new(commands::DEFAULT_SKILL_NAME))
        .to_str()
        .ok_or("--name must be UTF-8")?;
    if !valid_skill_name(name) {
        let message = "--name must be 1-64 lowercase letters, digits or hyphens, with no leading, \
trailing or double hyphen, and cannot contain claude or anthropic";
        return Err(message.into());
    }
    let executable = options
        .executable
        .as_ref()
        .map(|path| {
            let text = path.to_str().ok_or("--executable must be UTF-8")?;
            if text.chars().any(char::is_control) {
                return Err("--executable must not contain control characters");
            }
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err("--executable must be an absolute path");
            }
            Ok(path)
        })
        .transpose()?;
    Ok(Command::Skill(Skill {
        folder: PathBuf::from(folder),
        name: name.into(),
        executable,
        force: options.force,
        receipt: options.receipt,
    }))
}

/// The one input source and its format.
fn input(command: &str, options: &Options) -> Result<Input, String> {
    let source = match (&options.input, &options.smiles) {
        (Some(_), Some(_)) => {
            return Err(format!(
                "{command} takes one input: INPUT, - or --smiles, not both"
            ));
        }
        (None, None) => {
            return Err(format!(
                "{command} needs an input: a file, - for standard input, or --smiles TEXT"
            ));
        }
        (Some(path), None) if path == "-" => Source::Stdin,
        (Some(path), None) => Source::File(PathBuf::from(path)),
        (None, Some(text)) => Source::Smiles(
            text.to_str()
                .ok_or("--smiles text must be UTF-8")?
                .to_owned(),
        ),
    };
    // `auto` detects supported molecular formats from their contents. It must
    // not turn a known crystal path into the SMILES fallback. An explicit
    // supported format still overrides a misleading filename.
    if let Source::File(path) = &source
        && options.from.as_deref().is_none_or(|from| from == "auto")
        && let Some(format) = crate::compatibility::file_format(path)
        && crate::crystal::is_crystal_format(format)
    {
        return Err(unsupported_crystal_input(command, format));
    }
    let format = match (&source, &options.from) {
        (Source::Smiles(_), Some(_)) => {
            return Err("--smiles cannot be combined with --from".into());
        }
        (Source::Smiles(_), None) => "smiles".to_owned(),
        (Source::Stdin, None) => {
            return Err("reading standard input (-) needs --from FORMAT".into());
        }
        (Source::File(path), None) => input_format(&extension(path)).to_owned(),
        (_, Some(from)) => {
            let from = from.to_string_lossy();
            let formats = input_formats();
            if !formats.contains(&from.as_ref()) {
                if let Some(format) = crate::crystal::extension_format(&from) {
                    return Err(unsupported_crystal_input(command, format));
                }
                return Err(format!(
                    "unknown input format `{from}`; supported: {}",
                    formats.join(", ")
                ));
            }
            from.into_owned()
        }
    };
    Ok(Input { source, format })
}

fn unsupported_crystal_input(command: &str, format: &str) -> String {
    format!(
        "{command} does not support crystal input ({format}); supported input formats: {}",
        input_formats().join(", ")
    )
}

/// The extension as the app reads it: empty unless it is UTF-8.
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_owned()
}

/// `convert`: the input, then the file to write and the labels.
fn convert(options: &Options) -> Result<Command, String> {
    let input = input("convert", options)?;
    let export = export(options)?;
    if options.decimals.is_some() && options.labels.is_none() {
        return Err("--decimals needs --labels value, numbers, ppm or both".into());
    }
    let labels = match (&options.nucleus, &options.labels) {
        (None, None) => None,
        (_, None) => return Err("--nucleus needs --labels value, numbers, ppm or both".into()),
        (None, _) => return Err("--labels needs --nucleus 1H or 13C".into()),
        (Some(nucleus), Some(format)) => {
            let format = format
                .to_str()
                .filter(|format| crate::nmr::LabelFormat::from_code(format).is_some())
                .ok_or("--labels must be value, numbers, ppm or both")?;
            if !LABEL_FORMATS.contains(&export.format.as_str()) {
                return Err(format!(
                    "--labels needs an output that keeps atom numbers: {}",
                    LABEL_FORMATS.join(", ")
                ));
            }
            let nucleus = nucleus_code(nucleus)?;
            let decimals = match &options.decimals {
                None => crate::nmr::default_decimals(crate::nmr::Nucleus::from_code(nucleus)),
                Some(value) => value
                    .to_str()
                    .and_then(|text| text.parse::<u8>().ok())
                    .filter(|decimals| *decimals <= crate::nmr::MAX_DECIMALS)
                    .ok_or("--decimals must be a whole number from 0 to 4")?,
            };
            Some(Labels {
                nucleus,
                format: format.to_owned(),
                decimals,
            })
        }
    };
    Ok(Command::Convert(Convert {
        input,
        export,
        labels,
    }))
}

/// A `--nucleus` value: `1H` or `13C`, never Rust's `H1` or `C13`.
fn nucleus_code(nucleus: &OsStr) -> Result<&'static str, String> {
    nucleus
        .to_str()
        .and_then(crate::nmr::Nucleus::from_code)
        .map(crate::nmr::Nucleus::code)
        .ok_or_else(|| "--nucleus must be 1H or 13C".into())
}

fn export_input(options: &Options) -> Result<Command, String> {
    let input = options
        .input
        .as_ref()
        .ok_or("export-input needs INPUT.xyz")?;
    if input == "-" {
        return Err("export-input needs an XYZ file path".into());
    }
    let program = match options.program.as_deref().and_then(OsStr::to_str) {
        Some("gaussian") => Program::Gaussian,
        Some("orca") => Program::Orca,
        Some("gamess") => Program::Gamess,
        Some("xyz") => Program::Xyz,
        _ => return Err("--program must be gaussian, orca, gamess or xyz".into()),
    };
    if options.template.is_some() && (options.method.is_some() || options.basis.is_some()) {
        return Err("--template cannot be combined with --method or --basis".into());
    }
    if options.method.is_some() != options.basis.is_some() {
        return Err("--method and --basis must be given together".into());
    }
    let templates = builtin_templates();
    let template = integer("--template", options.template.as_deref(), 1_usize)?;
    let template = template
        .checked_sub(1)
        .and_then(|index| templates.get(index))
        .ok_or_else(|| format!("--template must be from 1 to {}", templates.len()))?;
    if template.only.is_some_and(|only| only != program) {
        return Err(format!("{} is available only for ORCA", template.name));
    }
    let job = match option_text("--job", options.job.as_deref())? {
        Some(text) => Job::parse(text).ok_or(
            "--job must be single-point, optimize, opt-freq, transition-state (ts), qst2, qst3, irc or neb-ts",
        )?,
        None => template.job,
    };
    let path = |flag, value: Option<&OsStr>| {
        value
            .map(|value| {
                if value.is_empty() || value == "-" {
                    Err(format!("{flag} needs a file path"))
                } else {
                    Ok(PathBuf::from(value))
                }
            })
            .transpose()
    };
    let reactant = path("--reactant", options.reactant.as_deref())?;
    let product = path("--product", options.product.as_deref())?;
    let hessian = path("--hessian", options.hessian.as_deref())?;
    if matches!(
        (program, job),
        (Program::Gaussian, Job::Qst2 | Job::Qst3) | (Program::Orca, Job::NebTs)
    ) {
        let missing = match (&reactant, &product) {
            (None, None) => Some("--reactant and --product"),
            (None, Some(_)) => Some("--reactant"),
            (Some(_), None) => Some("--product"),
            (Some(_), Some(_)) => None,
        };
        if let Some(missing) = missing {
            return Err(format!("{} requires {missing}", job.label()));
        }
    } else if reactant.is_some() || product.is_some() {
        return Err("--reactant and --product require Gaussian qst2/qst3 or ORCA neb-ts".into());
    }
    if hessian.is_some()
        && !(program == Program::Orca && matches!(job, Job::TransitionState | Job::Irc))
    {
        return Err("--hessian requires --program orca with --job ts or irc".into());
    }
    let output = match &options.output {
        None => Output::Stdout,
        Some(path) if path == "-" => Output::Stdout,
        Some(path) if path.is_empty() => return Err("--out needs a file path".into()),
        Some(path) => Output::File(path.into()),
    };
    if output == Output::Stdout
        && (hessian.is_some() || (program == Program::Orca && job == Job::NebTs))
    {
        return Err("use --out PATH to write the main input and its companion files".into());
    }
    let method = option_text("--method", options.method.as_deref())?.unwrap_or(&template.method);
    let basis = match option_text("--basis", options.basis.as_deref())? {
        Some(basis) => basis,
        None if program == Program::Gamess => {
            nearest_gamess_basis(&template.basis).unwrap_or(&template.basis)
        }
        None => &template.basis,
    };
    Ok(Command::ExportInput(ExportInput {
        input: input.into(),
        program,
        job,
        reactant,
        product,
        hessian,
        method: method.into(),
        basis: basis.into(),
        charge: integer("--charge", options.charge.as_deref(), 0)?,
        multiplicity: integer("--multiplicity", options.multiplicity.as_deref(), 1)?,
        cores: integer("--cores", options.cores.as_deref(), template.cores)?,
        memory_gb: integer(
            "--memory-gb",
            options.memory_gb.as_deref(),
            template.memory_gb,
        )?,
        title: option_text("--title", options.title.as_deref())?.map(str::to_owned),
        extra: option_text("--extra", options.extra.as_deref())?
            .unwrap_or(&template.extra)
            .into(),
        output,
    }))
}

fn option_text<'a>(flag: &str, value: Option<&'a OsStr>) -> Result<Option<&'a str>, String> {
    value
        .map(|value| {
            value
                .to_str()
                .ok_or_else(|| format!("{flag} must be UTF-8"))
        })
        .transpose()
}

fn integer<T: std::str::FromStr>(
    flag: &str,
    value: Option<&OsStr>,
    default: T,
) -> Result<T, String> {
    optional_integer(flag, value).map(|value| value.unwrap_or(default))
}

fn optional_integer<T: std::str::FromStr>(
    flag: &str,
    value: Option<&OsStr>,
) -> Result<Option<T>, String> {
    option_text(flag, value)?
        .map(|text| {
            text.parse()
                .map_err(|_| format!("{flag} must be a whole number"))
        })
        .transpose()
}

/// `render`: the input, the image format and destination, then the size.
fn render(options: &Options) -> Result<Command, String> {
    Ok(Command::Render(Render {
        input: input("render", options)?,
        preview: preview(options)?,
    }))
}

fn preview(options: &Options) -> Result<Preview, String> {
    let (format, output) = destination(options, render_formats())?;
    Ok(Preview {
        format,
        svg_text: options.svg_text,
        output,
        width: side("--width", options.width.as_deref())?,
        height: side("--height", options.height.as_deref())?,
        force: options.force,
    })
}

/// `compose`: the Proposal file or `-`, then the file to write.
fn compose(options: &Options) -> Result<Command, String> {
    let proposal = match &options.input {
        None => {
            return Err("compose needs a proposal: a JSON file, or - for standard input".into());
        }
        Some(path) if path == "-" => None,
        Some(path) => Some(PathBuf::from(path)),
    };
    let export = export(options)?;
    Ok(Command::Compose(Compose { proposal, export }))
}

/// A `--width` or `--height` value: a `u32`, which the render tool then
/// checks against its limits.
fn side(name: &str, value: Option<&OsStr>) -> Result<Option<u32>, String> {
    let parse = |value: &OsStr| {
        let number = value.to_str().and_then(|text| text.parse().ok());
        number.ok_or_else(|| {
            format!(
                "{name} must be a whole number of pixels, not `{}`",
                value.to_string_lossy()
            )
        })
    };
    value.map(parse).transpose()
}

/// The file convert and compose write: the format and destination, then
/// `--pages` for pdf only and `--receipt` only with `-o FILE`.
fn export(options: &Options) -> Result<Export, String> {
    let (format, output) = destination(options, output_formats())?;
    if options.pages && format != "pdf" {
        return Err("--pages is valid only with pdf output".into());
    }
    if options.receipt && output == Output::Stdout {
        return Err("--receipt needs -o FILE".into());
    }
    Ok(Export {
        format,
        svg_text: options.svg_text,
        output,
        pages: options.pages,
        force: options.force,
        receipt: options.receipt,
    })
}

/// Where the output goes and its format, one of `formats`: `--to`, or the
/// extension of `-o FILE` (`.smi` is smiles).
fn destination(options: &Options, formats: &[&str]) -> Result<(String, Output), String> {
    let output = match &options.output {
        Some(path) if path == "-" => Output::Stdout,
        Some(path) if path.is_empty() => return Err("-o needs a file name".into()),
        Some(path) => Output::File(PathBuf::from(path)),
        None => Output::Stdout,
    };
    let format = match (&options.to, &output) {
        (Some(to), _) => to.to_string_lossy().into_owned(),
        (None, Output::File(path)) => match extension_lossy(path) {
            Some(extension) if extension == "smi" => "smiles".to_owned(),
            Some(extension) => extension,
            None => {
                return Err(format!(
                    "cannot tell the output format of `{}`; pass --to FORMAT",
                    path.display()
                ));
            }
        },
        (None, Output::Stdout) => {
            return Err("writing to standard output needs --to FORMAT".into());
        }
    };
    if !formats.contains(&format.as_str()) {
        return Err(format!(
            "unsupported output format `{format}`; supported: {}",
            formats.join(", ")
        ));
    }
    Ok((format, output))
}

/// The lowercased extension of an output path, if it has one.
fn extension_lossy(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    (!extension.is_empty()).then_some(extension)
}
