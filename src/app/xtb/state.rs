//! The Calculate card's settings, a run's progress and the finish decision,
//! kept free of iced and the worker so they can be tested alone.
use reshiki::science::StereoChange;

/// A task the xTB worker (protocol v1) can run.
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

/// Shown disabled, each with the reason, until the worker supports them.
pub(crate) const LATER_TASKS: [(&str, &str); 3] = [
    ("Opt + freq", "Needs frequencies support in the xTB worker"),
    ("Orbitals", "Needs orbital output from the xTB worker"),
    ("Conformers", "Needs the conformer search in the xTB worker"),
];
pub(crate) const LATER_METHODS: [(&str, &str); 2] = [
    ("g-xTB", "Needs g-xTB in the xTB worker"),
    ("GFN-FF", "Needs GFN-FF in the xTB worker"),
];
/// Advanced settings the worker cannot take yet.
pub(crate) const LATER_SETTINGS: &str =
    "Coming later: accuracy, electronic temperature and maximum SCF iterations.";
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
/// Energy and optimization limit of the worker (protocol v1).
pub(crate) const MAX_ATOMS: usize = 500;
pub(crate) const NEAR_DEGENERATE: &str = "Near-degenerate electronic structure: results can differ between computers. Try a higher electronic temperature or more unpaired electrons.";

/// The card's choices. Charge and unpaired electrons hold what was typed;
/// an empty field means the value from the drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Settings {
    pub task: Task,
    pub charge: String,
    pub unpaired: String,
    /// One of [`SOLVENTS`]; `None` is the gas phase.
    pub solvent: Option<&'static str>,
    pub hold_fixed: bool,
    pub tight: bool,
    /// Zero is Auto.
    pub threads: u32,
    pub advanced: bool,
    /// The molecule the typed values belong to: tab, file epoch, first atom.
    pub molecule: Option<[u64; 3]>,
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
        let symbol = reshiki::editing::ELEMENTS
            .get(usize::from(z).wrapping_sub(1))
            .copied()
            .unwrap_or("?");
        *counts.entry(symbol).or_default() += 1;
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
            '-' => '⁻',
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
