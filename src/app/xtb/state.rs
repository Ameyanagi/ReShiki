//! The Calculate card's settings, a run's progress and the finish decision,
//! kept free of iced and the worker so they can be tested alone.
use reshiki::qcdata::{self, XtbMethod};
use reshiki::science::StereoChange;

/// A task the Calculate card runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Task {
    Energy,
    #[default]
    Optimize,
}

impl Task {
    pub(crate) const ALL: [(Self, &'static str); 2] =
        [(Self::Energy, "Energy"), (Self::Optimize, "Optimize")];
}

/// Shown disabled, each with the reason, until their tabs arrive.
pub(crate) const LATER_TASKS: [(&str, &str); 3] = [
    ("Opt + freq", "Coming in the Vibrations tab"),
    ("Orbitals", "Coming in the Orbitals tab"),
    ("Conformers", "Coming with the conformer search"),
];
/// The methods in the card's order; GFN2-xTB is the default.
pub(crate) const METHODS: [XtbMethod; 3] = [XtbMethod::Gfn2Xtb, XtbMethod::GXtb, XtbMethod::GfnFf];
/// The electronic temperature "Retry at 1000 K" uses.
pub(crate) const RETRY_TEMPERATURE_K: f64 = 1000.;
/// ALPB solvents offered first; xtb also accepts these names.
pub(crate) const SOLVENTS: [&str; 9] = [
    "water",
    "methanol",
    "DMSO",
    "chloroform",
    "toluene",
    "acetone",
    "acetonitrile",
    "THF",
    "hexane",
];
/// Energy and optimization limit of the worker.
pub(crate) const MAX_ATOMS: usize = 500;
pub(crate) const NEAR_DEGENERATE: &str = "Near-degenerate electronic structure: results can differ between computers. Try a higher electronic temperature or more unpaired electrons.";

/// Whether `method` runs an SCF, and so has orbitals and SCF settings.
pub(crate) fn electronic(method: XtbMethod) -> bool {
    method.capabilities().orbitals
}

/// The one-line note under Method, if the method limits the card.
pub(crate) fn method_note(method: XtbMethod, solvent: bool) -> Option<&'static str> {
    match method {
        XtbMethod::GfnFf => {
            Some("No orbitals, bond orders or solvent with GFN-FF; closed shells only.")
        }
        XtbMethod::GXtb if solvent => Some(
            "g-xTB in solvent: the GFN2-xTB solvation energy (ALPB) is added to the gas-phase g-xTB energy.",
        ),
        _ => None,
    }
}

/// What one run calculates and how: from the card, a stored result's
/// Recalculate, or an entry point with the card's last settings.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Config {
    pub task: Task,
    pub method: XtbMethod,
    pub charge: i32,
    pub unpaired: u32,
    /// One of [`SOLVENTS`]; `None` for the gas phase and for GFN-FF.
    pub solvent: Option<&'static str>,
    /// "xtb 6.8 compatible" numerical conventions.
    pub xtb68: bool,
    /// SCF overrides; `None` is the method's default. Always `None` for GFN-FF.
    pub accuracy: Option<f64>,
    pub temperature_k: Option<f64>,
    pub max_iterations: Option<u32>,
    pub threads: u32,
    pub tight: bool,
    /// Drawing atoms the optimizer holds fixed.
    pub fixed: Vec<u64>,
}

impl Config {
    /// A stored result's settings, to calculate it again; what the file
    /// does not keep comes from `info` (this session's run), else defaults.
    /// `None` when the card cannot reproduce it: imported results, other
    /// tasks, GBSA or a solvent the card does not list.
    pub(crate) fn from_result(
        result: &qcdata::CalcResult,
        frame: &qcdata::Frame,
        info: Option<&super::results::RunInfo>,
        threads: u32,
    ) -> Option<Self> {
        let qcdata::Settings::Xtb(settings) = &result.settings else {
            return None;
        };
        let task = match result.task {
            qcdata::Task::Optimize => Task::Optimize,
            qcdata::Task::Energy => Task::Energy,
            _ => return None,
        };
        // Settings the card cannot send: a retry ladder, a GFN-FF
        // topology with fragment charges, another solvation state.
        let topology = settings
            .gfnff
            .as_ref()
            .is_some_and(|gfnff| !gfnff.fragment_charges.is_empty());
        if !settings.scf_retries.is_empty() || topology {
            return None;
        }
        let solvent = match &settings.solvent {
            None => None,
            Some(s) if s.model == qcdata::SolventModel::Alpb && s.state == Default::default() => {
                Some(
                    SOLVENTS
                        .into_iter()
                        .find(|name| name.eq_ignore_ascii_case(&s.name))?,
                )
            }
            Some(_) => return None,
        };
        Some(Self {
            task,
            method: settings.method,
            charge: frame.charge,
            unpaired: frame.multiplicity.saturating_sub(1),
            solvent,
            xtb68: settings.conventions == qcdata::XtbConventions::Xtb68,
            accuracy: settings.accuracy,
            temperature_k: settings.electronic_temperature_k,
            max_iterations: settings.max_iterations,
            threads: info.map_or(threads, |info| info.threads),
            tight: info.is_some_and(|info| info.tight),
            fixed: info.map_or_else(Vec::new, |info| info.fixed.clone()),
        })
    }

    /// The same calculation at the retry electronic temperature, unless it
    /// already runs that hot (or has no SCF).
    pub(crate) fn hotter(&self) -> Option<Self> {
        (electronic(self.method)
            && self
                .temperature_k
                .is_none_or(|kelvin| kelvin < RETRY_TEMPERATURE_K))
        .then(|| Self {
            temperature_k: Some(RETRY_TEMPERATURE_K),
            ..self.clone()
        })
    }
}

/// Why a run failed, as the error card says it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Failure {
    /// The SCF did not converge; the worker's message.
    Scf(String),
    /// `by` (the method that said so, if known) has no parameters for `z`.
    Element {
        z: u8,
        by: Option<XtbMethod>,
    },
    TooMany {
        count: usize,
        limit: usize,
    },
    /// The worker stopped: exited, timed out or broke the protocol.
    Crashed(String),
    /// The method's valence model needs `valence` unpaired electrons.
    Spin {
        valence: u32,
    },
    Other(String),
}

impl Failure {
    /// The error card's plain sentence and its grey second line. `numbers`
    /// are the frame's atomic numbers, when the run got that far.
    pub(crate) fn explain(&self, method: XtbMethod, numbers: &[u8]) -> (String, String) {
        match self {
            Self::Scf(message) => (
                "The SCF did not converge.".into(),
                format!(
                    "{}. Nothing was applied to the drawing.",
                    capitalized(message.trim_end_matches('.'))
                ),
            ),
            Self::Element { z, by } => {
                let symbol = symbol(*z);
                let by = by.unwrap_or(method);
                let title = match numbers.iter().position(|n| n == z) {
                    Some(index) => format!(
                        "Atom {} is {symbol} (Z = {z}). {} has no parameters for it.",
                        index + 1,
                        by.label()
                    ),
                    None => format!("{} has no parameters for {symbol} (Z = {z}).", by.label()),
                };
                let detail = match (method, by) {
                    (XtbMethod::GXtb, XtbMethod::Gfn2Xtb) => {
                        "g-xTB in solvent uses GFN2-xTB for the solvation energy; gas-phase g-xTB covers this element."
                    }
                    (_, XtbMethod::Gfn2Xtb) => {
                        "GFN2-xTB covers elements up to radon (Z = 86); g-xTB and GFN-FF go further."
                    }
                    _ => "Change or remove that atom to calculate.",
                };
                (title, detail.into())
            }
            Self::TooMany { count, limit } => (
                format!("This molecule has {count} atoms. The limit is {limit} atoms."),
                "Split the structure or calculate a fragment.".into(),
            ),
            Self::Crashed(_) => (
                "The calculation stopped unexpectedly. Your drawing is safe.".into(),
                "Run again, or copy the details for a bug report.".into(),
            ),
            Self::Spin { valence } => (
                format!(
                    "xTB's valence model gives {valence} unpaired electron{} for this molecule, which its full electron count cannot have.",
                    if *valence == 1 { "" } else { "s" }
                ),
                "GFN2-xTB keeps the lanthanides' f electrons in the core. Use g-xTB, or GFN-FF for a closed shell.".into(),
            ),
            Self::Other(message) => (capitalized(message), String::new()),
        }
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// "U" for 92.
pub(crate) fn symbol(z: u8) -> &'static str {
    reshiki::editing::ELEMENTS
        .get(usize::from(z).wrapping_sub(1))
        .copied()
        .unwrap_or("?")
}

/// Typed SCF overrides: accuracy, electronic temperature (K), maximum iterations.
pub(crate) type Scf = (Option<f64>, Option<f64>, Option<u32>);

/// The card's choices. Charge, unpaired electrons and the SCF fields hold
/// what was typed; an empty field means the drawing's value or the
/// method's default.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub task: Task,
    pub method: XtbMethod,
    pub charge: String,
    pub unpaired: String,
    /// One of [`SOLVENTS`]; `None` is the gas phase.
    pub solvent: Option<&'static str>,
    pub hold_fixed: bool,
    pub tight: bool,
    /// Zero is Auto.
    pub threads: u32,
    pub advanced: bool,
    pub accuracy: String,
    pub temperature: String,
    pub iterations: String,
    pub xtb68: bool,
    /// The molecule the typed values belong to: tab, file epoch, first atom.
    pub molecule: Option<[u64; 3]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            task: Task::default(),
            method: XtbMethod::Gfn2Xtb,
            charge: String::new(),
            unpaired: String::new(),
            solvent: None,
            hold_fixed: false,
            tight: false,
            threads: 0,
            advanced: false,
            accuracy: String::new(),
            temperature: String::new(),
            iterations: String::new(),
            xtb68: false,
            molecule: None,
        }
    }
}

impl Settings {
    /// Typed charge and unpaired electrons belong to one molecule; another
    /// target starts again from its drawing.
    pub(crate) fn retarget(&mut self, molecule: Option<[u64; 3]>) {
        if self.molecule != molecule {
            self.molecule = molecule;
            self.charge.clear();
            self.unpaired.clear();
        }
    }

    /// The charge and unpaired electrons to use: typed values over the drawn ones.
    pub(crate) fn state(&self, drawn: (i32, u32)) -> Result<(i32, u32), &'static str> {
        let charge = match self.charge.trim() {
            "" => drawn.0,
            typed => typed
                .replace('−', "-")
                .parse()
                .map_err(|_| "Charge must be a whole number")?,
        };
        let unpaired = match self.unpaired.trim() {
            "" => drawn.1,
            typed => typed
                .parse()
                .map_err(|_| "Unpaired electrons must be a whole number from 0")?,
        };
        Ok((charge, unpaired))
    }

    /// The run these settings describe for a molecule drawn with `drawn`
    /// (charge, unpaired electrons); `typed` says whether the typed charge
    /// and unpaired electrons belong to it.
    pub(crate) fn config(
        &self,
        drawn: (i32, u32),
        typed: bool,
        fixed: Vec<u64>,
    ) -> Result<Config, &'static str> {
        let (charge, unpaired) = if typed { self.state(drawn)? } else { drawn };
        let electronic = electronic(self.method);
        if !electronic && unpaired > 0 {
            return Err("GFN-FF calculates closed shells only: set Unpaired electrons to 0");
        }
        let (accuracy, temperature_k, max_iterations) = if electronic {
            self.scf()?
        } else {
            (None, None, None)
        };
        let solvents = self.method.capabilities().solvent != qcdata::SolventSupport::None;
        Ok(Config {
            task: self.task,
            method: self.method,
            charge,
            unpaired,
            solvent: self.solvent.filter(|_| solvents),
            xtb68: self.xtb68,
            accuracy,
            temperature_k,
            max_iterations,
            threads: self.threads(),
            tight: self.tight,
            fixed: if self.task == Task::Optimize {
                fixed
            } else {
                vec![]
            },
        })
    }

    /// The typed SCF overrides: accuracy, electronic temperature (K) and
    /// maximum iterations. Empty fields are the method's defaults.
    pub(crate) fn scf(&self) -> Result<Scf, &'static str> {
        fn typed<T: std::str::FromStr + PartialOrd>(
            text: &str,
            range: std::ops::RangeInclusive<T>,
            error: &'static str,
        ) -> Result<Option<T>, &'static str> {
            match text.trim() {
                "" => Ok(None),
                text => text
                    .replace('−', "-")
                    .parse()
                    .ok()
                    .filter(|value| range.contains(value))
                    .map(Some)
                    .ok_or(error),
            }
        }
        Ok((
            typed(
                &self.accuracy,
                1e-4..=1e3,
                "Accuracy must be a number from 0.0001 to 1000",
            )?,
            typed(
                &self.temperature,
                0.0..=1e5,
                "Electronic temperature must be from 0 to 100000 K",
            )?,
            typed(
                &self.iterations,
                1..=10_000,
                "Max SCF iterations must be a whole number from 1 to 10000",
            )?,
        ))
    }

    pub(crate) fn threads(&self) -> u32 {
        if self.threads > 0 {
            return self.threads;
        }
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8) as u32)
    }
}

/// The live check under Charge: "94 electrons · multiplicity 1", or why the
/// numbers cannot describe a molecule with `protons`.
pub(crate) fn electrons(protons: u32, charge: i32, unpaired: u32) -> Result<String, String> {
    let count = i64::from(protons) - i64::from(charge);
    if count < 0 {
        return Err("The charge leaves fewer than zero electrons".into());
    }
    if i64::from(unpaired) > count {
        return Err(format!("Unpaired electrons must be between 0 and {count}"));
    }
    if (count - i64::from(unpaired)) % 2 != 0 {
        return Err(if count % 2 != 0 {
            "Odd electron count needs an odd number of unpaired electrons".into()
        } else {
            "Even electron count needs an even number of unpaired electrons".into()
        });
    }
    Ok(format!(
        "{count} electron{} · multiplicity {}",
        if count == 1 { "" } else { "s" },
        u64::from(unpaired) + 1
    ))
}

/// Hill formula of atomic numbers, e.g. "C8H10N4O2".
pub(crate) fn formula(numbers: &[u8]) -> String {
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for &z in numbers {
        *counts.entry(symbol(z)).or_default() += 1;
    }
    let carbon = counts.contains_key("C");
    let mut text = String::new();
    let mut push = |symbol: &str, count: usize| {
        text.push_str(symbol);
        if count > 1 {
            text.push_str(&count.to_string());
        }
    };
    if carbon {
        for symbol in ["C", "H"] {
            if let Some(count) = counts.remove(symbol) {
                push(symbol, count);
            }
        }
    }
    for (symbol, count) in counts {
        push(symbol, count);
    }
    text
}

/// "−2.1 × 10⁻⁵", with a real minus sign.
pub(crate) fn scientific(value: f64) -> String {
    if value == 0.0 || !value.is_finite() {
        return "0".into();
    }
    let exponent = value.abs().log10().floor() as i32;
    let mut mantissa = value / 10f64.powi(exponent);
    let mut exponent = exponent;
    if (mantissa.abs() * 10.).round() >= 100. {
        mantissa /= 10.;
        exponent += 1;
    }
    let superscript: String = exponent
        .to_string()
        .chars()
        .map(|c| match c {
            // Superscript minus. The browser build, whose fonts lack it, never
            // shows the card (see `unavailable`).
            '-' => '\u{207B}',
            digit => digit
                .to_digit(10)
                .and_then(|d| "⁰¹²³⁴⁵⁶⁷⁸⁹".chars().nth(d as usize))
                .unwrap_or(digit),
        })
        .collect();
    format!("{:.1} × 10{superscript}", mantissa).replace('-', "−")
}

/// Signed decimal with a real minus sign.
pub(crate) fn signed(value: f64, places: usize) -> String {
    format!("{value:.places$}").replace('-', "−")
}

/// What has arrived from a running job.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Progress {
    /// Energy of each accepted optimizer step, from step 0.
    pub energies: Vec<f64>,
}

impl Progress {
    /// Add `energies` starting at step `first`; a repeated step replaces the old one.
    pub(crate) fn push(&mut self, first: usize, energies: &[f64]) {
        self.energies.truncate(first);
        self.energies.extend_from_slice(energies);
    }

    pub(crate) fn step(&self) -> usize {
        self.energies.len().saturating_sub(1)
    }

    /// Energy change of each step after the first.
    pub(crate) fn deltas(&self) -> impl Iterator<Item = f64> + '_ {
        self.energies
            .iter()
            .zip(self.energies.iter().skip(1))
            .map(|(before, after)| after - before)
    }

    /// "Optimizing · step 12 · ΔE −2.1 × 10⁻⁵ Eh · 3.2 s".
    pub(crate) fn line(&self, task: Task, seconds: f64) -> String {
        let time = format!("{seconds:.1} s");
        match task {
            Task::Energy => format!("Computing energy · {time}"),
            Task::Optimize => match self.deltas().last() {
                Some(delta) => format!(
                    "Optimizing · step {} · ΔE {} Eh · {time}",
                    self.step(),
                    scientific(delta)
                ),
                None if self.energies.is_empty() => format!("Starting xTB · {time}"),
                None => format!("Optimizing · step 0 · {time}"),
            },
        }
    }
}

/// The finished result in one line: "E −5.070176 Eh · gap 14.01 eV · converged in 7 steps".
pub(crate) fn summary(
    energy: f64,
    gap_ev: Option<f64>,
    optimized: Option<(bool, usize)>,
) -> String {
    let mut line = format!("E {} Eh", signed(energy, 6));
    if let Some(gap) = gap_ev {
        line.push_str(&format!(" · gap {gap:.2} eV"));
    }
    match optimized {
        Some((true, steps)) => line.push_str(&format!(
            " · converged in {steps} step{}",
            if steps == 1 { "" } else { "s" }
        )),
        Some((false, steps)) => line.push_str(&format!(" · not converged after {steps} steps")),
        None => {}
    }
    line
}

/// How a stereo change reads in the warning card. `name` gives an atom's
/// label ("C3") and the drawing's CIP code, if any.
pub(crate) fn describe(
    change: &StereoChange,
    name: impl Fn(u64) -> (String, Option<String>),
) -> String {
    match change {
        StereoChange::Centre { atom, inverted } => {
            let (label, cip) = name(*atom);
            let after = match (cip.as_deref(), inverted) {
                (_, false) => "flat".to_owned(),
                (Some("R"), true) => "S".into(),
                (Some("S"), true) => "R".into(),
                (_, true) => "inverted".into(),
            };
            match cip {
                Some(cip) => {
                    format!("Optimization changed the configuration at {label} ({cip} → {after}).")
                }
                None => format!("Optimization changed the configuration at {label} ({after})."),
            }
        }
        StereoChange::DoubleBond { atoms } => {
            let (a, _) = name(atoms[0]);
            let (b, _) = name(atoms[1]);
            format!("Optimization changed the E/Z configuration of the {a}={b} double bond.")
        }
    }
}

#[cfg(test)]
mod tests;
