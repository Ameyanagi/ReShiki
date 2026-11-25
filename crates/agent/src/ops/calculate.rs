//! The `calculate` tool: xTB calculations through the host's worker hook.
use super::{
    budget::Budgets,
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    export_input::xyz_frame,
    progress::Monotonic,
    result::ToolResult,
    wire::warnings,
};
use crate::{
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
};
use reshiki_io::qcdata::{
    self, Convergence, Frame, GfnFfTopology, OrbitalRequest, Solvent, SolventCorrection,
    SolventModel, SolventSupport, Task, XtbConventions, XtbMethod, XtbSettings,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{future::Future, pin::Pin, sync::Arc, time::Instant};

/// The timeout message for the default calculation budget.
pub const DEADLINE_EXCEEDED: &str = "the calculation exceeded its 30-minute deadline";

/// One calculation in flight. Dropping it cancels the work.
pub type CalcFuture = Pin<Box<dyn Future<Output = Result<Calculated, String>> + Send>>;

pub trait Calculations: Send + Sync + 'static {
    /// The engine, as the worker's hello reports it.
    fn engine(&self) -> Engine;

    /// Runs one calculation. Dropping the future cancels it (the host kills its worker).
    fn calculate(&self, request: CalcRequest, progress: Option<Arc<Monotonic>>) -> CalcFuture;
}

/// The engine's identification and attribution from its hello.
#[derive(Debug, Clone, Serialize)]
pub struct Engine {
    pub name: String,
    pub version: String,
    pub license: String,
    pub source: String,
    pub rev: String,
}

/// The validated geometry and settings for the host.
#[derive(Debug)]
pub struct CalcRequest {
    pub task: CalcTask,
    pub frame: Frame,
    pub settings: XtbSettings,
    /// The cooperative deadline set by the executor before calling the host.
    pub deadline: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalcTask {
    Energy,
    Optimize,
    Frequencies,
    #[serde(rename = "opt+freq")]
    OptFreq,
}

/// The hook's result and client-facing warnings.
#[derive(Debug)]
pub struct Calculated {
    pub value: CalcValue,
    pub warnings: Vec<String>,
}

/// A successful calculation. Energies are unrounded. Charges are rounded to
/// four decimal places and frequencies to two.
#[derive(Debug, Serialize)]
pub struct CalcValue {
    pub task: CalcTask,
    /// The method's wire name, as returned by [`method_name`].
    pub method: String,
    pub engine: EngineIdentity,
    pub atoms: usize,
    pub charge: i32,
    pub multiplicity: u32,
    pub energy_hartree: f64,
    pub homo_lumo_gap_ev: Option<f64>,
    pub dipole_debye: Option<f64>,
    pub charges: Option<Vec<f64>>,
    pub optimization: Option<Optimization>,
    pub frequencies: Option<Frequencies>,
    pub seconds: f64,
}

/// The engine identification in a calculation result.
#[derive(Debug, Serialize)]
pub struct EngineIdentity {
    pub name: String,
    pub version: String,
    pub rev: String,
}

#[derive(Debug, Serialize)]
pub struct Optimization {
    pub converged: bool,
    pub steps: usize,
    pub rmsd_angstrom: f64,
    pub xyz: String,
}

#[derive(Debug, Serialize)]
pub struct Frequencies {
    pub frequencies_cm1: Vec<f64>,
    pub ir_intensities_km_mol: Vec<f64>,
    pub imaginary: usize,
    pub temperature_k: Option<f64>,
    pub zero_point_energy_hartree: Option<f64>,
    pub enthalpy_hartree: Option<f64>,
    pub gibbs_hartree: Option<f64>,
}

const METHODS: [XtbMethod; 3] = [XtbMethod::Gfn2Xtb, XtbMethod::GXtb, XtbMethod::GfnFf];
const MAX_THREADS: u32 = 64;

/// The operation API's wire name for an xTB method.
pub const fn method_name(method: XtbMethod) -> &'static str {
    match method {
        XtbMethod::Gfn2Xtb => "gfn2",
        XtbMethod::GXtb => "gxtb",
        XtbMethod::GfnFf => "gfnff",
    }
}

pub const CALCULATE: ToolSpec = ToolSpec {
    name: "calculate",
    title: Some("Run xTB calculation"),
    description: "Calculate energy, optimize or compute frequencies from the first XYZ frame (Å). Only in builds with xTB.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "xyz": {"type": "string"},
            "task": {"type": "string", "enum": ["energy", "optimize", "frequencies", "opt+freq"]},
            "method": {"type": "string", "enum": METHODS.map(method_name)},
            "charge": {"type": "integer"},
            "uhf": {"type": "integer", "minimum": 0, "description": "Unpaired electrons."},
            "solvent": {"type": ["string", "null"], "description": "ALPB solvent name."},
            "conventions": {"type": "string", "enum": ["axtb", "xtb-6.8"]},
            "electronic_temperature_k": {"type": ["number", "null"], "minimum": 0},
            "accuracy": {"type": ["number", "null"], "minimum": 0},
            "max_iterations": {"type": ["integer", "null"], "minimum": 1},
            "threads": {"type": ["integer", "null"], "minimum": 1, "maximum": MAX_THREADS},
            "convergence": {"type": ["string", "null"], "enum": ["normal", "tight", null]},
            "fixed_atoms": {"type": ["array", "null"], "items": {"type": "integer", "minimum": 1},
                            "description": "Unique XYZ atom numbers for optimization."},
        },
        "required": ["xyz", "task", "method", "charge", "uhf", "solvent", "conventions",
                     "electronic_temperature_k", "accuracy", "max_iterations", "threads",
                     "convergence", "fixed_atoms"],
        "additionalProperties": false,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    xyz: String,
    task: CalcTask,
    #[serde(default = "default_method")]
    method: String,
    #[serde(default)]
    charge: i32,
    #[serde(default)]
    uhf: u32,
    #[serde(default)]
    solvent: Option<String>,
    #[serde(default)]
    conventions: XtbConventions,
    #[serde(default)]
    electronic_temperature_k: Option<f64>,
    #[serde(default)]
    accuracy: Option<f64>,
    #[serde(default)]
    max_iterations: Option<u32>,
    #[serde(default)]
    threads: Option<u32>,
    #[serde(default)]
    convergence: Option<Convergence>,
    #[serde(default)]
    fixed_atoms: Option<Vec<u32>>,
}

fn default_method() -> String {
    method_name(XtbMethod::Gfn2Xtb).into()
}

pub(crate) fn decode(args: Value, budgets: &Budgets) -> Result<CalcRequest, OpError> {
    let args: Arguments = arguments(args)?;
    let method = METHODS
        .into_iter()
        .find(|method| method_name(*method) == args.method)
        .ok_or_else(|| OpError::invalid("method must be gfn2, gxtb or gfnff").param("method"))?;
    if args
        .threads
        .is_some_and(|n| !(1..=MAX_THREADS).contains(&n))
    {
        return Err(
            OpError::invalid(format!("threads must be from 1 to {MAX_THREADS}")).param("threads"),
        );
    }
    let tasks: &[Task] = match args.task {
        CalcTask::Energy => &[Task::Energy],
        CalcTask::Optimize => &[Task::Optimize],
        CalcTask::Frequencies => &[Task::Hessian],
        CalcTask::OptFreq => &[Task::Optimize, Task::Hessian],
    };
    if !tasks.contains(&Task::Optimize) {
        if args.convergence.is_some() {
            return Err(OpError::invalid("convergence requires optimization").param("convergence"));
        }
        if args.fixed_atoms.is_some() {
            return Err(OpError::invalid("fixed_atoms requires optimization").param("fixed_atoms"));
        }
    }
    let mut frame = xyz_frame(&args.xyz, budgets)?;
    frame.charge = args.charge;
    frame.multiplicity = args
        .uhf
        .checked_add(1)
        .ok_or_else(|| OpError::invalid("uhf is too large for a spin multiplicity").param("uhf"))?;
    let mut fixed_atoms = args.fixed_atoms.unwrap_or_default();
    for atom in &mut fixed_atoms {
        *atom = atom
            .checked_sub(1)
            .ok_or_else(|| OpError::invalid("fixed_atoms starts at 1").param("fixed_atoms"))?;
    }
    fixed_atoms.sort_unstable();
    let settings = XtbSettings {
        method,
        conventions: args.conventions,
        solvent: args.solvent.map(|name| Solvent {
            model: SolventModel::Alpb,
            name: name.to_lowercase(),
            state: Default::default(),
            correction: (method == XtbMethod::GXtb).then_some(SolventCorrection::Gfn2),
        }),
        electronic_temperature_k: args.electronic_temperature_k,
        accuracy: args.accuracy,
        max_iterations: args.max_iterations,
        scf_retries: Vec::new(),
        gfnff: (method == XtbMethod::GfnFf).then(|| GfnFfTopology {
            reference: frame.hash(),
            fragment_charges: Vec::new(),
        }),
        threads: args.threads,
        convergence: tasks
            .contains(&Task::Optimize)
            .then(|| args.convergence.unwrap_or(Convergence::Normal)),
        fixed_atoms,
    };
    for &task in tasks {
        qcdata::check_calculation(task, &settings, &frame, OrbitalRequest::None).map_err(
            |error| {
                let kind = match error {
                    qcdata::Error::Unsupported(_) => ErrorKind::Unsupported,
                    _ => ErrorKind::InvalidArguments,
                };
                OpError::new(kind, error.to_string())
            },
        )?;
    }
    Ok(CalcRequest {
        task: args.task,
        frame,
        settings,
        deadline: None,
    })
}

pub(crate) async fn calculate(
    ctx: Context,
    hook: Arc<dyn Calculations>,
    versions: Versions,
    mut request: CalcRequest,
) -> Result<ToolResult, OpError> {
    ctx.checkpoint()?;
    request.deadline = ctx.deadline();
    let calculated = ctx
        .cancellable(hook.calculate(request, ctx.progress()))
        .await
        .map_err(|error| {
            if error.kind == ErrorKind::Timeout {
                OpError::new(ErrorKind::Timeout, DEADLINE_EXCEEDED)
            } else {
                error
            }
        })?
        .map_err(OpError::failed)?;
    Ok(ToolResult::valid(
        calculated.value,
        warnings(calculated.warnings),
        versions,
    ))
}

pub(crate) fn info(hook: &dyn Calculations) -> Value {
    let methods = METHODS.map(|method| {
        let caps = method.capabilities();
        let solvent = match caps.solvent {
            SolventSupport::Native => Some("native"),
            SolventSupport::Gfn2Correction => Some("gfn2_correction"),
            SolventSupport::None => None,
        };
        json!({
            "method": method_name(method),
            "label": method.label(),
            "energy": caps.energy,
            "optimize": caps.optimize,
            "frequencies": caps.hessian,
            "open_shell": caps.open_shell,
            "solvent": solvent,
        })
    });
    json!({
        "engine": hook.engine(),
        "methods": methods,
        "max_atoms": qcdata::MAX_CALC_ATOMS,
        "max_frequency_atoms": qcdata::MAX_HESSIAN_ATOMS,
    })
}
