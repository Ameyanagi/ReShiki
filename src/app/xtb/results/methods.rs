use super::solvent_model;
use reshiki::qcdata::{self, CalcResult, Convergence, Frame, Settings, Task, XtbConventions};

/// A methods paragraph with the citation keys in ReShiki's xTB bibliography.
pub(crate) fn methods_text(result: &CalcResult, frame: &Frame) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let provenance = &result.provenance;
    let program = match &provenance.program_version {
        Some(version) => format!("{} {version}", provenance.program),
        None => provenance.program.clone(),
    };
    let Settings::Xtb(settings) = &result.settings else {
        let method = provenance
            .method
            .as_ref()
            .map_or_else(String::new, |method| format!(" using {method}"));
        return format!(
            "Results computed with {program}{method} were imported into ReShiki {version} [ReShiki]."
        );
    };

    let mut text = String::new();
    if result.task == Task::Conformers {
        text.push_str("Conformers were generated with ETKDG [Riniker2015ETKDG; Wang2020ETKDGv3]");
        let start = provenance
            .method
            .as_deref()
            .and_then(|method| method.split_once(" ETKDG "));
        if let Some((field, _)) = start {
            let citation = match field {
                "MMFF94s" => "Halgren1999MMFF94s",
                "MMFF94" => "Halgren1996MMFF",
                "UFF" => "Rappe1992UFF",
                _ => method_citation(settings.method),
            };
            text.push_str(&format!(
                ", with {field} [{citation}] selected for minimization"
            ));
        }
        text.push_str(", and clustered by heavy-atom RMSD [Butina1999Clustering]. ");
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
        Task::Optimize => "Geometries were optimized",
        Task::Hessian if thermochemistry.is_some() => "Harmonic frequencies and",
        Task::Hessian => "Harmonic frequencies were calculated",
        Task::Gradient => "Energy gradients were computed",
        Task::Conformers => "Conformer energies were computed",
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
        " with {} [{citation}{extension}] as implemented in ReShiki {version} [ReShiki] ({program} [Axtb])",
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
    if result.task == Task::Optimize {
        text.push_str(match settings.convergence {
            Some(Convergence::Normal) => " with normal convergence",
            Some(Convergence::Tight) => " with tight convergence",
            None => " using the TRIC optimizer",
        });
        text.push_str(" [Wang2016TRIC; Baker1996DLC; Lindh1995Hessian; Banerjee1985PRFO]");
    }
    if settings.conventions == XtbConventions::Xtb68 {
        text.push_str(", using xtb 6.8 compatible conventions [Bannwarth2021Xtb]");
    }
    if let Some(kelvin) = settings.electronic_temperature_k {
        text.push_str(&format!(", at an electronic temperature of {kelvin} K"));
    }
    text.push('.');
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

fn method_citation(method: qcdata::XtbMethod) -> &'static str {
    match method {
        qcdata::XtbMethod::Gfn2Xtb => "Bannwarth2019GFN2",
        qcdata::XtbMethod::GfnFf => "Spicher2020GFNFF",
        qcdata::XtbMethod::GXtb => "Froitzheim2025gxTB",
    }
}
