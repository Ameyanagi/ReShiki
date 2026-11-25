//! Work off the UI thread: an xTB result's orbitals recomputed by the
//! worker (light results keep no coefficients), and isosurface meshes.
use reshiki::models3d::surface::orbital;
use reshiki::qcdata::{self, BasisSet, CalcResult, Frame, Settings};
use std::sync::Arc;

/// Orbitals `rows` of `result` at its own frame, from the worker, in the
/// canonical contract. The engine is pinned, so the levels match the
/// stored ones; a mismatch is reported rather than shown.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn orbitals(
    result: &CalcResult,
    frame: &Frame,
    rows: Vec<u32>,
    cancel: &reshiki_xtb_worker::client::Cancel,
) -> Result<qcdata::Orbitals, String> {
    use reshiki_xtb_worker::client::{Limits, Session};
    use reshiki_xtb_worker::protocol::{Compute, Method, Open, OrbitalOutput, ScfSettings, Units};

    let Settings::Xtb(settings) = &result.settings else {
        return Err("Imported orbitals cannot be recomputed".into());
    };
    let method = match settings.method {
        qcdata::XtbMethod::Gfn2Xtb => Method::Gfn2,
        qcdata::XtbMethod::GXtb => Method::Gxtb,
        qcdata::XtbMethod::GfnFf => return Err("GFN-FF has no orbitals".into()),
    };
    let limits = Limits::default();
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let (mut session, _) =
        Session::start(&exe, limits, cancel).map_err(|error| error.to_string())?;
    let open = Open {
        numbers: frame.numbers.clone(),
        positions: frame.positions_angstrom.clone(),
        units: Units::Angstrom,
        charge: frame.charge,
        uhf: Some(frame.multiplicity.saturating_sub(1)),
        method,
        conventions: settings.conventions,
        solvent: settings.solvent.clone(),
        scf: ScfSettings {
            accuracy: settings.accuracy,
            electronic_temperature_k: settings.electronic_temperature_k,
            max_iterations: settings.max_iterations,
            retries: settings.scf_retries.clone(),
        },
        gfnff: None,
        threads: 1,
        heap_bytes: limits.heap_bytes,
    };
    session
        .open(&open, cancel)
        .map_err(|error| error.to_string())?;
    let computed = session
        .compute(
            &Compute {
                orbitals: OrbitalOutput::Rows(rows),
                ..Compute::default()
            },
            cancel,
        )
        .map_err(|error| error.to_string())?;
    let _ = session.close(cancel);
    let orbitals = computed
        .orbitals
        .ok_or("The xTB worker returned no orbitals")?;
    // Same build, same levels: anything else would draw another orbital
    // than the one listed.
    let stored = result.orbitals.as_ref().map(|o| &o.alpha.energies_hartree);
    let same = stored.is_none_or(|stored| {
        stored.len() == orbitals.alpha.energies_hartree.len()
            && stored
                .iter()
                .zip(&orbitals.alpha.energies_hartree)
                .all(|(a, b)| (a - b).abs() < 1e-5)
    });
    if !same {
        return Err("The recomputed orbitals differ from the stored levels".into());
    }
    Ok(orbitals)
}

/// The coefficient row of orbital `index` of `spin`, if `orbitals` keeps it.
pub(crate) fn row(
    orbitals: &qcdata::Orbitals,
    spin: qcdata::Spin,
    index: usize,
) -> Option<(&BasisSet, &[f64])> {
    let basis = orbitals.basis.as_ref()?;
    let set = match (&orbitals.beta, spin) {
        (qcdata::Beta::Unrestricted(beta), qcdata::Spin::Beta) => beta,
        _ => &orbitals.alpha,
    };
    let at = set.rows.binary_search(&u32::try_from(index).ok()?).ok()?;
    let n = basis
        .shells
        .iter()
        .map(qcdata::Shell::n_functions)
        .sum::<usize>();
    Some((basis, set.coefficients.get(at * n..(at + 1) * n)?))
}

/// The isosurface meshes of one orbital around the model's `points` (Å),
/// or why there are none.
pub(super) fn meshes(
    basis: &BasisSet,
    row: &[f64],
    points: &[[f32; 3]],
    iso: f32,
) -> Result<super::Meshes, String> {
    let centres: Vec<[f64; 3]> = points.iter().map(|p| p.map(f64::from)).collect();
    let field = orbital::OrbitalField::new(basis, row, &centres)?;
    orbital::orbital_meshes(&field, points, iso)
        .map(|meshes| super::Meshes(Arc::new(meshes)))
        .ok_or_else(|| "The surface reaches past the grid at this isovalue; raise it".into())
}
