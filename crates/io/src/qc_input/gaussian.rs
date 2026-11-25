use std::{borrow::Cow, fmt::Write};

use super::{Dispersion, Frame, Job, Method, Settings, single_line, write_atoms};

pub(super) fn write(
    settings: &Settings,
    frame: &Frame,
    endpoints: Option<(&Frame, &Frame)>,
    method: &Method,
    title: &str,
    stem: &str,
    header: Option<&str>,
) -> String {
    let mut text = String::new();
    if let Some(header) = header {
        let _ = writeln!(text, "! {header}");
    }
    let cores = settings.cores;
    let memory = settings.memory_gb;
    let _ = writeln!(
        text,
        "%nprocshared={cores}\n%mem={memory}GB\n%chk={stem}.chk"
    );

    text.push_str("#p ");
    let base = method.base;
    if settings.multiplicity > 1 {
        let upper = base.to_ascii_uppercase();
        if !upper.starts_with('U') && !upper.starts_with("RO") {
            text.push('U');
        }
    }
    text.push_str(base);
    let basis = single_line(settings.basis.trim());
    let basis = basis_name(&basis);
    if !basis.is_empty() {
        let _ = write!(text, "/{basis}");
    }
    text.push_str(match method.dispersion {
        Some(Dispersion::D3Bj) => " EmpiricalDispersion=GD3BJ",
        Some(Dispersion::D3) => " EmpiricalDispersion=GD3",
        None => "",
    });
    match settings.job {
        Job::SinglePoint => text.push_str(" SP"),
        Job::Optimize => text.push_str(" Opt"),
        Job::OptFreq => text.push_str(" Opt Freq"),
        Job::TransitionState => text.push_str(" Opt=(TS,CalcFC,NoEigenTest) Freq"),
        Job::Qst2 => text.push_str(" Opt=QST2 Freq"),
        Job::Qst3 => text.push_str(" Opt=QST3 Freq"),
        Job::Irc => {
            let _ = write!(text, " IRC=(CalcFC,MaxPoints={})", settings.irc_points);
        }
        Job::NebTs => {} // Rejected by write before dispatch.
    }
    let extra = single_line(settings.extra.trim());
    if !extra.is_empty() {
        let _ = write!(text, " {extra}");
    }
    text.push_str("\n\n");

    if let Some((reactant, product)) = endpoints {
        section(&mut text, settings, &format!("{title} reactant"), reactant);
        section(&mut text, settings, &format!("{title} product"), product);
        if settings.job == Job::Qst3 {
            section(&mut text, settings, &format!("{title} TS guess"), frame);
        }
    } else {
        section(&mut text, settings, title, frame);
    }
    text
}

fn basis_name(basis: &str) -> Cow<'_, str> {
    let upper = basis.to_ascii_uppercase();
    match upper.strip_prefix("DEF2-") {
        Some("SV(P)") => "Def2SVPP".into(),
        Some(suffix) => format!("Def2{suffix}").into(),
        None => basis.into(),
    }
}

fn section(text: &mut String, settings: &Settings, title: &str, frame: &Frame) {
    let _ = writeln!(
        text,
        "{title}\n\n{} {}",
        settings.charge, settings.multiplicity
    );
    write_atoms(text, frame, " ");
    text.push('\n');
}
