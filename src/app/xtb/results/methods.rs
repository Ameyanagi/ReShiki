//! The methods paragraph Copy methods text writes for a stored result, citing
//! the BibTeX keys of docs/references/xtb.bib for what actually ran.
use super::{OPTIMIZER_SEPARATOR, energies, solvent_model};
use reshiki::qcdata::{
    self, CalcResult, Convergence, Frame, Settings, Source, Status, Task, XtbConventions,
};

/// A methods paragraph with the citation keys in ReShiki's xTB bibliography.
pub(crate) fn methods_text(result: &CalcResult, frame: &Frame) -> String {
    let provenance = &result.provenance;
    let program = match &provenance.program_version {
        Some(version) => format!("{} {version}", provenance.program),
        None => provenance.program.clone(),
    };
    let Settings::Xtb(settings) = &result.settings else {
        return imported_methods(result, &program);
    };
    let converged_at_start = result.task == Task::Optimize
        && result.status == Status::Converged
        && energies(result).len() == 1;

    let mut text = String::new();
    let start_minimizer = provenance
        .method
        .as_deref()
        .and_then(|method| method.split_once(" ETKDG "))
        .map(|(field, _)| field);
    let xtb_starts = matches!(start_minimizer, Some("GFN-FF" | "GFN2-xTB"));
    let field = match start_minimizer {
        Some("MMFF94s") => Some(("MMFF94s", "Halgren1999MMFF94s")),
        Some("MMFF94") => Some(("MMFF94", "Halgren1996MMFF")),
        Some("UFF") => Some(("UFF", "Rappe1992UFF")),
        _ => None,
    };
    if result.task == Task::Conformers {
        text.push_str("Conformers were generated with ETKDG [Riniker2015ETKDG; Wang2020ETKDGv3]");
        if let Some((field, citation)) = field {
            text.push_str(&format!(
                ", with {field} [{citation}] recorded as the initial minimizer"
            ));
        }
        if !xtb_starts {
            text.push_str(", and clustered by heavy-atom RMSD [Butina1999Clustering]");
        }
        text.push_str(". ");
    }
    let thermochemistry = result.thermo.as_ref().map(|thermo| {
        let (model, citations) = if thermo.rotor_cutoff_cm1 > 0. {
            ("quasi-RRHO", "Grimme2012QRRHO; FernandezRamos2007Symmetry")
        } else {
            ("RRHO", "FernandezRamos2007Symmetry")
        };
        format!(
            "{model} thermochemistry at {:.2} K [{citations}]",
            thermo.temperature_k
        )
    });
    text.push_str(match result.task {
        Task::Optimize if converged_at_start => "The start geometry was evaluated",
        Task::Optimize => "Geometries were optimized",
        Task::Hessian if thermochemistry.is_some() => "Harmonic frequencies and",
        Task::Hessian => "Harmonic frequencies were calculated",
        Task::Gradient => "Energy gradients were computed",
        Task::Conformers if field.is_some() => "Cluster representatives were re-optimized",
        Task::Conformers if xtb_starts => "Conformer starts were optimized",
        Task::Conformers => "Conformer geometries were optimized",
        Task::TransitionState => "Transition states were searched for",
        Task::Energy => "Energies were computed",
    });
    if result.task == Task::Hessian
        && let Some(thermo) = &thermochemistry
    {
        text.push_str(&format!(" {thermo} were calculated"));
    }
    let method = settings.method;
    let citation = method_citation(method);
    let extension = if method == qcdata::XtbMethod::GfnFf
        && frame
            .numbers
            .iter()
            .any(|z| matches!(z, 57..=71 | 89..=103))
    {
        "; Rose2024GFNFFLnAn"
    } else {
        ""
    };
    text.push_str(&format!(
        " with {} [{citation}{extension}] as implemented in ReShiki [ReShiki] ({program} [Axtb])",
        method.label(),
    ));
    if let Some(solvent) = &settings.solvent {
        let model = solvent_model(solvent.model);
        let correction = if solvent.correction.is_some() {
            " with a GFN2-xTB solvation correction [Bannwarth2019GFN2]"
        } else {
            ""
        };
        text.push_str(&format!(
            " in {} using {model} [Ehlert2021ALPB]{correction}",
            solvent.name
        ));
    } else {
        text.push_str(" in the gas phase");
    }
    if matches!(result.task, Task::Optimize | Task::Conformers) {
        text.push_str(&optimization_details(
            result,
            settings.convergence,
            converged_at_start,
        ));
    }
    if !settings.fixed_atoms.is_empty() {
        let count = settings.fixed_atoms.len();
        let unit = if count == 1 { "atom" } else { "atoms" };
        text.push_str(&format!(" with {count} {unit} held fixed"));
    }
    if settings.conventions == XtbConventions::Xtb68 {
        text.push_str(", using xtb 6.8 compatible conventions [Bannwarth2021Xtb]");
    }
    if let Some(kelvin) = settings.electronic_temperature_k {
        text.push_str(&format!(", at an electronic temperature of {kelvin} K"));
    }
    text.push('.');
    if result.task == Task::Conformers && xtb_starts {
        text.push_str(
            " The optimized starts were clustered by heavy-atom RMSD [Butina1999Clustering].",
        );
    }
    if result.task != Task::Hessian {
        if let Some(thermo) = thermochemistry {
            text.push_str(&format!(
                " Harmonic frequencies and {thermo} were calculated."
            ));
        } else if result.vibrations.is_some() {
            text.push_str(" Harmonic frequencies were also calculated.");
        }
    }
    text
}

fn imported_methods(result: &CalcResult, program: &str) -> String {
    let provenance = &result.provenance;
    let method = match (&provenance.method, &provenance.basis) {
        (Some(method), Some(basis)) => format!(" using {method}/{basis}"),
        (Some(method), None) => format!(" using {method}"),
        (None, Some(basis)) => format!(" using the {basis} basis"),
        (None, None) => String::new(),
    };
    if let Source::Imported {
        format, file_name, ..
    } = &provenance.source
    {
        let name = provenance.program.trim();
        let is_format = |label: &str| format.eq_ignore_ascii_case(label);
        let is_program = |label: &str| name.eq_ignore_ascii_case(label);
        // These readers identify the calculation program in its job output.
        // Gaussian is also the format default for fchk, even without a version record.
        let gaussian_fchk = is_format("Gaussian fchk") && is_program("Gaussian");
        let calculator_known = (is_format("ORCA output") && is_program("ORCA"))
            || (is_format("Gaussian log") && is_program("Gaussian"))
            || (gaussian_fchk && provenance.program_version.is_some());
        if !calculator_known {
            let writer = if name.is_empty()
                || is_program("unknown")
                || is_program(format)
                || is_program(&format!("{format} file"))
                || gaussian_fchk
            {
                String::new()
            } else {
                format!(", written by {program}")
            };
            let citation = if format.eq_ignore_ascii_case("Molden") {
                "; MoldenFormat; Schaftenaar2000Molden"
            } else {
                ""
            };
            let energies = if result.energy_hartree.is_some()
                || result
                    .trajectory
                    .as_ref()
                    .is_some_and(|trajectory| !trajectory.energies_hartree.is_empty())
            {
                " and energies"
            } else {
                ""
            };
            return format!(
                "Geometries{energies} were read from {file_name} ({format} format{writer}){method} and imported into ReShiki [ReShiki{citation}]."
            );
        }
    }
    let citation = if provenance.program.eq_ignore_ascii_case("ORCA")
        && provenance
            .program_version
            .as_deref()
            .is_some_and(|version| version.split('.').next() == Some("6"))
    {
        " [Neese2025ORCA6]"
    } else if provenance.program.eq_ignore_ascii_case("PySCF") {
        " [Sun2020PySCF]"
    } else {
        ""
    };
    format!(
        "Results computed with {program}{citation}{method} were imported into ReShiki [ReShiki]."
    )
}

fn optimization_details(
    result: &CalcResult,
    convergence: Option<Convergence>,
    converged_at_start: bool,
) -> String {
    let criteria = match convergence {
        Some(Convergence::Normal) => "the normal convergence criteria",
        Some(Convergence::Tight) => "the tight convergence criteria",
        None => "convergence (criteria were not recorded)",
    };
    if converged_at_start {
        return format!(", and the start geometry already met {criteria}");
    }
    // Every retained conformer has a converged optimization report, even if
    // the search was canceled before all representatives could be processed.
    let mut text = if result.task == Task::Conformers || result.status == Status::Converged {
        if convergence.is_some() {
            format!(" to {criteria}")
        } else {
            " (convergence criteria were not recorded)".into()
        }
    } else {
        let steps = result
            .trajectory
            .as_ref()
            .and_then(|trajectory| trajectory.energies_hartree.len().checked_sub(1))
            .map_or_else(String::new, |steps| {
                let unit = if steps == 1 { "step" } else { "steps" };
                format!(" for {steps} {unit}")
            });
        format!("{steps} without reaching {criteria}")
    };
    let recorded = result
        .provenance
        .method
        .as_deref()
        .and_then(|method| method.split_once(OPTIMIZER_SEPARATOR))
        .map(|(_, methods)| methods);
    let mut names = Vec::new();
    let mut citations = Vec::new();
    for name in recorded.into_iter().flat_map(|methods| methods.split(", ")) {
        let references: &[&str] = match name {
            "TRIC" => &[
                "Wang2016TRIC",
                "Baker1996DLC",
                "Lindh1995Hessian",
                "Banerjee1985PRFO",
            ],
            "ANC" => &["Grimme2017ANCopt", "Lindh1995Hessian", "Banerjee1985PRFO"],
            "L-BFGS" => &["Liu1989LBFGS", "Armijo1966LineSearch"],
            _ => continue,
        };
        if !names.contains(&name) {
            names.push(name);
        }
        for &citation in references {
            if !citations.contains(&citation) {
                citations.push(citation);
            }
        }
    }
    if names.is_empty() {
        text.push_str(" (optimizer algorithms were not recorded; ReShiki’s default optimizer uses TRIC [Wang2016TRIC])");
    } else {
        let algorithms = if names.len() == 1 {
            "optimizer"
        } else {
            "optimizer algorithms"
        };
        text.push_str(&format!(
            " using the {} {algorithms} [{}]",
            names.join(", "),
            citations.join("; ")
        ));
    }
    text
}

fn method_citation(method: qcdata::XtbMethod) -> &'static str {
    match method {
        qcdata::XtbMethod::Gfn2Xtb => "Bannwarth2019GFN2",
        qcdata::XtbMethod::GfnFf => "Spicher2020GFNFF",
        qcdata::XtbMethod::GXtb => "Froitzheim2025gxTB",
    }
}
