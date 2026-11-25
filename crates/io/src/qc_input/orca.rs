use std::fmt::Write;

use super::{
    Dispersion, Error, File, Frame, Geometry, Job, Method, Settings, single_line, write_atoms,
    write_orca_hess, xyz,
};

pub(super) fn maxcore(settings: &Settings) -> u64 {
    u64::from(settings.memory_gb) * 768 / u64::from(settings.cores)
}

pub(super) fn write(
    settings: &Settings,
    geometry: &Geometry,
    endpoints: Option<(&Frame, &Frame)>,
    method: &Method,
    title: &str,
    stem: &str,
    header: Option<&str>,
) -> Result<(String, Vec<File>), Error> {
    let mut text = String::new();
    if let Some(header) = header {
        let _ = writeln!(text, "# {header}");
    }
    let _ = writeln!(text, "# {title}");
    text.push_str("! ");
    let base = method.base;
    let upper = base.to_ascii_uppercase();
    let numerical_frequencies = ["MP2", "RI-MP2", "DLPNO", "CCSD"]
        .iter()
        .any(|prefix| upper.starts_with(prefix));
    if settings.multiplicity > 1 {
        let wave_function = upper == "HF" || numerical_frequencies;
        text.push_str(if wave_function { "UHF " } else { "UKS " });
    }
    text.push_str(base);
    text.push_str(match method.dispersion {
        Some(Dispersion::D3Bj) => " D3BJ",
        Some(Dispersion::D3) => " D3ZERO",
        None => "",
    });
    let basis = single_line(settings.basis.trim());
    if !method.composite && !basis.is_empty() {
        let _ = write!(text, " {basis}");
    }
    text.push_str(match settings.job {
        Job::SinglePoint => " SP",
        Job::Optimize | Job::OptFreq => " Opt",
        Job::TransitionState => " OptTS",
        Job::Irc => " IRC",
        Job::NebTs => " NEB-TS",
        Job::Qst2 | Job::Qst3 => "", // Rejected by write before dispatch.
    });
    if matches!(
        settings.job,
        Job::OptFreq | Job::TransitionState | Job::NebTs
    ) {
        text.push_str(if numerical_frequencies {
            " NumFreq"
        } else {
            " Freq"
        });
    }
    let extra = single_line(settings.extra.trim());
    if !extra.is_empty() {
        let _ = write!(text, " {extra}");
    }
    text.push('\n');

    let cores = settings.cores;
    if cores > 1 {
        let _ = writeln!(text, "%pal nprocs {cores} end");
    }
    let _ = writeln!(text, "%maxcore {}", maxcore(settings));

    let mut files = Vec::new();
    let frame = geometry.frame;
    if matches!(settings.job, Job::TransitionState | Job::Irc) {
        let hess_name = if let Some(hessian) = geometry.hessian() {
            // ORCA writes its own `{stem}.hess` after Freq; keep the xTB one apart.
            let name = format!("{stem}_xtb.hess");
            files.push(File {
                name: name.clone(),
                text: write_orca_hess(frame, hessian)?,
            });
            Some(name)
        } else {
            None
        };
        if settings.job == Job::TransitionState {
            text.push_str("%geom\n");
            if let Some(name) = hess_name {
                let _ = writeln!(text, "  InHess Read\n  InHessName \"{name}\"");
            } else {
                text.push_str("  Calc_Hess true\n");
            }
        } else {
            let _ = writeln!(text, "%irc\n  MaxIter {}", settings.irc_points);
            if let Some(name) = hess_name {
                let _ = writeln!(text, "  InitHess read\n  Hess_Filename \"{name}\"");
            }
        }
        text.push_str("end\n");
    }

    let inline = if let Some((reactant, product)) = endpoints {
        let product_name = format!("{stem}_product.xyz");
        let ts_name = format!("{stem}_ts.xyz");
        let _ = writeln!(
            text,
            "%neb\n  NEB_End_XYZFile \"{product_name}\"\n  NEB_TS_XYZFile \"{ts_name}\"\n  NImages {}\nend",
            settings.neb_images
        );
        files.push(File {
            name: product_name,
            text: xyz::frame(product, &format!("{title} product")),
        });
        files.push(File {
            name: ts_name,
            text: xyz::frame(frame, &format!("{title} TS guess")),
        });
        reactant
    } else {
        frame
    };
    let _ = writeln!(
        text,
        "\n* xyz {} {}",
        settings.charge, settings.multiplicity
    );
    write_atoms(&mut text, inline, "  ");
    text.push_str("*\n");
    Ok((text, files))
}
