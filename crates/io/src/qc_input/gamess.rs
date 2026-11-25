//! GAMESS-US input: $CONTRL, $SYSTEM, $BASIS, $DFT, $STATPT and $DATA groups.
use std::fmt::Write;

use super::{Dispersion, Frame, Job, Method, Settings, atoms, coordinates_8};

pub(super) fn basis_keywords(basis: &str) -> Option<&'static str> {
    match basis.to_ascii_uppercase().as_str() {
        "6-31G(D)" | "6-31G*" => Some("GBASIS=N31 NGAUSS=6 NDFUNC=1"),
        "6-31G" => Some("GBASIS=N31 NGAUSS=6"),
        "STO-3G" => Some("GBASIS=STO NGAUSS=3"),
        _ => None,
    }
}

pub(super) fn write(
    settings: &Settings,
    frame: &Frame,
    method: &Method,
    basis: &str,
    title: &str,
    header: Option<&str>,
) -> String {
    let mut text = String::new();
    if let Some(header) = header {
        let _ = writeln!(text, "! {header}");
    }
    let _ = writeln!(text, "! {title}");

    let scf = if settings.multiplicity > 1 {
        "UHF"
    } else {
        "RHF"
    };
    let run = match settings.job {
        Job::SinglePoint => "ENERGY",
        Job::Optimize | Job::OptFreq => "OPTIMIZE",
        Job::TransitionState => "SADPOINT",
        _ => "", // Rejected by write before dispatch.
    };
    let _ = write!(text, " $CONTRL SCFTYP={scf} RUNTYP={run}");
    match method.base.to_ascii_uppercase().as_str() {
        "HF" => {}
        "MP2" => text.push_str(" MPLEVL=2"),
        base => {
            let _ = write!(text, " DFTTYP={base}");
        }
    }
    let _ = writeln!(
        text,
        "\n         ICHARG={} MULT={} UNITS=ANGS $END",
        settings.charge, settings.multiplicity
    );
    let mwords = (u64::from(settings.memory_gb) * 125 / u64::from(settings.cores)).max(1);
    let _ = writeln!(text, " $SYSTEM MWORDS={mwords} $END\n $BASIS {basis} $END");
    if let Some(dispersion) = &method.dispersion {
        let version = match dispersion {
            Dispersion::D3Bj => 4,
            Dispersion::D3 => 3,
        };
        let _ = writeln!(text, " $DFT DC=.T. IDCVER={version} $END");
    }
    text.push_str(match settings.job {
        Job::Optimize => " $STATPT NSTEP=100 $END\n",
        Job::OptFreq => " $STATPT NSTEP=100 HSSEND=.T. $END\n",
        Job::TransitionState => " $STATPT HESS=CALC NSTEP=100 HSSEND=.T. $END\n",
        _ => "",
    });
    for line in settings
        .extra
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let _ = writeln!(text, " {line}");
    }

    let _ = writeln!(text, " $DATA\n{title}\nC1");
    for (number, element, pos) in atoms(frame) {
        let sym = element.symbol;
        let nuclear_charge = f64::from(number);
        let [x, y, z] = coordinates_8(pos);
        let _ = writeln!(
            text,
            "{sym:<2} {nuclear_charge:>6.1} {x:>14.8} {y:>14.8} {z:>14.8}"
        );
    }
    text.push_str(" $END\n");
    text
}
