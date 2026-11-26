//! Quantum chemistry input files and Cartesian geometry exports.

use std::{borrow::Cow, fmt::Write};

use reshiki_model::chemistry::{ELEMENTS, Element};
use serde::{Deserialize, Serialize};

use crate::qcdata::{CalcResult, Frame};

mod gamess;
mod gaussian;
mod hess;
mod orca;
mod sdf;
mod xyz;

#[cfg(test)]
mod tests;

pub use hess::write_orca_hess;
pub use sdf::{SdfEntry, write_sdf};
pub use xyz::{parse_xyz, write_xyz_trajectory};

/// The zero-based species and first Cartesian coordinate columns in an XYZ file.
pub struct XyzColumns {
    pub species: usize,
    pub position: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Program {
    Gaussian,
    Orca,
    Gamess,
    Xyz,
}

impl Program {
    pub const ALL: [Self; 4] = [Self::Gaussian, Self::Orca, Self::Gamess, Self::Xyz];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gaussian => "Gaussian",
            Self::Orca => "ORCA",
            Self::Gamess => "GAMESS",
            Self::Xyz => "XYZ",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Gaussian => "gjf",
            Self::Orca | Self::Gamess => "inp",
            Self::Xyz => "xyz",
        }
    }

    pub fn jobs(self) -> &'static [Job] {
        use Job::*;
        match self {
            Self::Gaussian => &[
                SinglePoint,
                Optimize,
                OptFreq,
                TransitionState,
                Qst2,
                Qst3,
                Irc,
            ],
            Self::Orca => &[SinglePoint, Optimize, OptFreq, TransitionState, Irc, NebTs],
            Self::Gamess => &[SinglePoint, Optimize, OptFreq, TransitionState],
            Self::Xyz => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Job {
    SinglePoint,
    Optimize,
    OptFreq,
    TransitionState,
    Qst2,
    Qst3,
    Irc,
    NebTs,
}

impl Job {
    pub const ALL: [Self; 8] = [
        Self::SinglePoint,
        Self::Optimize,
        Self::OptFreq,
        Self::TransitionState,
        Self::Qst2,
        Self::Qst3,
        Self::Irc,
        Self::NebTs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SinglePoint => "Single point",
            Self::Optimize => "Optimize",
            Self::OptFreq => "Opt + freq",
            Self::TransitionState => "Transition state",
            Self::Qst2 => "QST2",
            Self::Qst3 => "QST3",
            Self::Irc => "IRC",
            Self::NebTs => "NEB-TS",
        }
    }

    pub fn is_transition_state(self) -> bool {
        matches!(
            self,
            Self::TransitionState | Self::Qst2 | Self::Qst3 | Self::Irc | Self::NebTs
        )
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "single-point" | "sp" => Some(Self::SinglePoint),
            "optimize" | "opt" => Some(Self::Optimize),
            "opt-freq" | "opt+freq" => Some(Self::OptFreq),
            "transition-state" | "ts" => Some(Self::TransitionState),
            "qst2" => Some(Self::Qst2),
            "qst3" => Some(Self::Qst3),
            "irc" => Some(Self::Irc),
            "neb-ts" => Some(Self::NebTs),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub method: String,
    pub basis: String,
    pub job: Job,
    pub cores: u32,
    pub memory_gb: u32,
    #[serde(default)]
    pub extra: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only: Option<Program>,
}

pub fn builtin_templates() -> [Template; 3] {
    [
        (
            "B3LYP-D3BJ/def2-SVP opt+freq",
            "B3LYP-D3BJ",
            "def2-SVP",
            Job::OptFreq,
            None,
        ),
        (
            "r2SCAN-3c opt+freq",
            "r2SCAN-3c",
            "",
            Job::OptFreq,
            Some(Program::Orca),
        ),
        (
            "HF/6-31G(d) single point",
            "HF",
            "6-31G(d)",
            Job::SinglePoint,
            None,
        ),
    ]
    .map(|(name, method, basis, job, only)| Template {
        name: name.into(),
        method: method.into(),
        basis: basis.into(),
        job,
        cores: 8,
        memory_gb: 16,
        extra: String::new(),
        only,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub program: Program,
    pub job: Job,
    pub method: String,
    pub basis: String,
    pub charge: i32,
    pub multiplicity: u32,
    pub cores: u32,
    pub memory_gb: u32,
    pub title: String,
    pub extra: String,
    pub irc_points: u32,
    pub neb_images: u32,
}

impl Settings {
    pub fn from_template(
        t: &Template,
        program: Program,
        charge: i32,
        multiplicity: u32,
        title: String,
    ) -> Self {
        Self {
            program,
            job: t.job,
            method: t.method.clone(),
            basis: t.basis.clone(),
            charge,
            multiplicity,
            cores: t.cores,
            memory_gb: t.memory_gb,
            title,
            extra: t.extra.clone(),
            irc_points: 30,
            neb_images: 12,
        }
    }
}

pub struct Geometry<'a> {
    pub frame: &'a Frame,
    pub reactant: Option<&'a Frame>,
    pub product: Option<&'a Frame>,
    pub result: Option<&'a CalcResult>,
}

impl Geometry<'_> {
    fn hessian(&self) -> Option<&[f64]> {
        self.result
            .map(|result| result.hessian_hartree_per_bohr2.as_slice())
            .filter(|hessian| !hessian.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub name: String,
    pub text: String,
}

#[derive(Debug)]
pub struct Output {
    /// The input file.
    pub main: File,
    /// Files the input names, written beside it.
    pub companions: Vec<File>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
}

pub fn write(
    settings: &Settings,
    geometry: &Geometry,
    stem: &str,
    generator: &str,
) -> Result<Output, Error> {
    let frame = geometry.frame;
    if let Some(result) = geometry.result {
        if result.frame != frame.hash() {
            return Err(Error::Invalid(
                "The calculation result belongs to another geometry.".into(),
            ));
        }
        if settings.charge != frame.charge || settings.multiplicity != frame.multiplicity {
            return Err(Error::Invalid(format!(
                "The calculation result is for charge {} and multiplicity {}.",
                frame.charge, frame.multiplicity
            )));
        }
    }
    let program = settings.program;
    let name = format!("{stem}.{}", program.extension());
    let title = clean_title(&settings.title);
    let mut frames = [Some(frame), geometry.reactant, geometry.product]
        .into_iter()
        .flatten();
    let mut drawing_atoms = frames
        .clone()
        .map(|frame| &frame.drawing_atoms)
        .filter(|ids| !ids.is_empty());
    let drawing_atoms_match = drawing_atoms
        .next()
        .is_none_or(|first| drawing_atoms.all(|ids| ids == first));
    let atom_order_error = || {
        Error::Invalid(
            "The reactant, product and TS guess must list the same atoms in the same order.".into(),
        )
    };
    let mut warnings = Vec::new();
    if program == Program::Xyz {
        check_atoms(frame)?;
        if !drawing_atoms_match {
            return Err(atom_order_error());
        }
        return Ok(Output {
            main: File {
                name,
                text: xyz::frame(frame, &title),
            },
            companions: Vec::new(),
            warnings,
        });
    }

    let job = settings.job;
    let method_name = single_line(settings.method.trim());
    if !program.jobs().contains(&job) {
        let jobs = program
            .jobs()
            .iter()
            .map(|job| job.label())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(Error::Invalid(format!(
            "{} offers {jobs}.",
            program.label()
        )));
    }
    if method_name.is_empty() {
        return Err(Error::Invalid("Enter a method.".into()));
    }
    if settings.multiplicity == 0 {
        return Err(Error::Invalid("Multiplicity must be at least 1.".into()));
    }
    if settings.cores == 0 || settings.memory_gb == 0 {
        return Err(Error::Invalid(
            "Cores and memory must be at least 1.".into(),
        ));
    }
    check_atoms(frame)?;
    let endpoints = if matches!(job, Job::Qst2 | Job::Qst3 | Job::NebTs) {
        Some(geometry.reactant.zip(geometry.product).ok_or_else(|| {
            Error::Invalid(format!(
                "{} needs the reactant and product of a transition-state search.",
                job.label()
            ))
        })?)
    } else {
        None
    };
    for endpoint in [geometry.reactant, geometry.product].into_iter().flatten() {
        check_atoms(endpoint)?;
    }
    if !drawing_atoms_match || frames.any(|endpoint| endpoint.numbers != frame.numbers) {
        return Err(atom_order_error());
    }
    let method = Method::parse(&method_name);
    if method.composite && program != Program::Orca {
        return Err(Error::Invalid(format!(
            "{method_name} is an ORCA composite method; {} does not offer it.",
            program.label()
        )));
    }
    let basis = settings.basis.trim();
    let gamess_basis = if program == Program::Gamess {
        gamess::basis_keywords(basis).ok_or_else(|| {
            Error::Invalid(
                "GAMESS exports use built-in bases only: 6-31G(d), 6-31G or STO-3G.".into(),
            )
        })?
    } else {
        ""
    };

    let charge = settings.charge;
    let multiplicity = settings.multiplicity;
    let electrons = frame.numbers.iter().map(|&z| i64::from(z)).sum::<i64>() - i64::from(charge);
    let unpaired = i64::from(multiplicity - 1);
    if unpaired > electrons || (electrons - unpaired) % 2 != 0 {
        warnings.push(format!(
            "Charge {charge} and multiplicity {multiplicity} do not fit {electrons} electrons."
        ));
    }
    if program != Program::Gaussian && frame.isotopes.iter().any(|&isotope| isotope != 0) {
        warnings.push(format!(
            "Isotopes are not written; set them in {} if you need them.",
            program.label()
        ));
    }
    let header = job
        .is_transition_state()
        .then(|| ts_header(geometry.result, generator));
    let mut companions = Vec::new();
    let text = match program {
        Program::Gaussian => {
            if basis.is_empty() {
                warnings.push("No basis set: Gaussian uses STO-3G.".into());
            }
            gaussian::write(
                settings,
                frame,
                endpoints,
                &method,
                &title,
                stem,
                header.as_deref(),
            )
        }
        Program::Orca => {
            if method.composite && !basis.is_empty() {
                warnings.push(format!(
                    "{method_name} brings its own basis; the Basis field is ignored."
                ));
            }
            if matches!(job, Job::TransitionState | Job::Irc) && geometry.hessian().is_none() {
                warnings.push("No xTB Hessian: ORCA computes the first Hessian itself.".into());
            }
            let maxcore = orca::maxcore(settings);
            if maxcore < 1000 {
                warnings.push(format!(
                    "ORCA gets only {maxcore} MB per core; raise Memory or lower Cores."
                ));
            }
            orca::warn_about_method(settings, &method, &method_name, &mut warnings);
            let (text, files) = orca::write(
                settings,
                geometry,
                endpoints,
                &method,
                &title,
                stem,
                header.as_deref(),
            )?;
            companions = files;
            text
        }
        Program::Gamess => {
            warnings.push(
                "GAMESS takes the number of cores when you start it (rungms), not from the file."
                    .into(),
            );
            if settings.cores > 1
                && method.base.eq_ignore_ascii_case("MP2")
                && !settings.extra.to_ascii_uppercase().contains("MEMDDI")
            {
                warnings.push("Parallel GAMESS MP2 needs MEMDDI: add a line such as  $SYSTEM MEMDDI=100 $END under Extra keywords.".into());
            }
            let text = gamess::write(
                settings,
                frame,
                &method,
                gamess_basis,
                &title,
                header.as_deref(),
            );
            if text.lines().any(|line| line.len() > 80) {
                warnings
                    .push("GAMESS reads 80 columns; shorten the title or Extra keywords.".into());
            }
            text
        }
        Program::Xyz => xyz::frame(frame, &title),
    };
    Ok(Output {
        main: File { name, text },
        companions,
        warnings,
    })
}

/// What every writer needs: atoms, each with a known element and a finite
/// position. Charge and spin are only warned about.
fn check_atoms(frame: &Frame) -> Result<(), Error> {
    if frame.numbers.is_empty() {
        return Err(Error::Invalid("The structure has no atoms.".into()));
    }
    if frame.positions_angstrom.len() != frame.numbers.len()
        || frame.numbers.iter().any(|z| !(1..=118).contains(z))
        || frame
            .positions_angstrom
            .iter()
            .flatten()
            .any(|x| !x.is_finite())
    {
        return Err(Error::Invalid(
            "The structure has an unknown element or a missing position.".into(),
        ));
    }
    Ok(())
}

enum Dispersion {
    D3Bj,
    D3,
}

struct Method<'a> {
    base: &'a str,
    dispersion: Option<Dispersion>,
    composite: bool,
}

impl<'a> Method<'a> {
    fn parse(method: &'a str) -> Self {
        let upper = method.to_ascii_uppercase();
        let (base, dispersion) = if let Some(base) = upper.strip_suffix("-D3BJ") {
            (base, Some(Dispersion::D3Bj))
        } else if let Some(base) = upper.strip_suffix("-D3") {
            (base, Some(Dispersion::D3))
        } else {
            (upper.as_str(), None)
        };
        Self {
            base: method.get(..base.len()).unwrap_or(method),
            dispersion,
            composite: base.ends_with("-3C"),
        }
    }
}

fn clean_title(title: &str) -> String {
    let title = single_line(title.trim());
    let title = title.trim_start_matches(|c: char| c.is_whitespace() || "@!%#$*".contains(c));
    if title.is_empty() {
        "molecule".into()
    } else {
        title.into()
    }
}

fn single_line(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

fn ts_header(result: Option<&CalcResult>, generator: &str) -> String {
    let source = result
        .and_then(|r| r.provenance.method.as_deref())
        .unwrap_or("the 3D model");
    let mut header = format!("TS guess from {} ({generator})", single_line(source));
    if let Some(frequency) = result
        .and_then(|r| r.vibrations.as_ref())
        .into_iter()
        .flat_map(|v| v.frequencies_cm1.iter().copied())
        .filter(|&f| f < 0.0)
        .min_by(f64::total_cmp)
    {
        let _ = write!(
            header,
            ", imaginary frequency -{:.0} cm-1",
            frequency.abs().round()
        );
    }
    header
}

fn atoms(frame: &Frame) -> impl Iterator<Item = (u8, &'static Element, [f64; 3])> + '_ {
    frame
        .numbers
        .iter()
        .copied()
        .zip(frame.positions_angstrom.iter().copied())
        .filter_map(|(z, pos)| {
            ELEMENTS
                .get(usize::from(z))
                .map(|element| (z, element, pos))
        })
}

fn coordinates_8(pos: [f64; 3]) -> [f64; 3] {
    pos.map(|x| if x.abs() < 0.5e-8 { 0.0 } else { x })
}

fn write_atoms(text: &mut String, frame: &Frame, program: Program) {
    let indent = match program {
        Program::Gaussian | Program::Gamess => " ",
        Program::Orca => "  ",
        Program::Xyz => "",
    };
    for (index, (_, element, pos)) in atoms(frame).enumerate() {
        let isotope = frame.isotopes.get(index).copied().unwrap_or_default();
        let sym = match (program, element.symbol, isotope) {
            (_, symbol, 0) => Cow::Borrowed(symbol),
            (Program::Gaussian, symbol, isotope) => format!("{symbol}(Iso={isotope})").into(),
            (Program::Xyz, "H", 2) => Cow::Borrowed("D"),
            (Program::Xyz, "H", 3) => Cow::Borrowed("T"),
            (Program::Xyz, symbol, isotope) => format!("{isotope}{symbol}").into(),
            (_, symbol, _) => Cow::Borrowed(symbol),
        };
        let [x, y, z] = coordinates_8(pos);
        let _ = writeln!(text, "{indent}{sym:<2} {x:>14.8} {y:>14.8} {z:>14.8}");
    }
}
