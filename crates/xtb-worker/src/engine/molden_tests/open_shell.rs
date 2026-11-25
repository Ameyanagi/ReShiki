//! Compare both spin channels to Molden from the same open-shell SCF.

use super::{
    Beta, Method, Orbitals, Response, canonical_orbitals, compute_with_molden, driver_method,
    reference::check_spherical, request,
};
use crate::protocol::OrbitalOutput;

const OH: &str = "2\nOH radical\nO 0 0 0\nH 0 0 0.97\n";
const O2: &str = "2\ntriplet oxygen\nO 0 0 0\nO 0 0 1.21\n";

fn convert(response: &Response, method: Method, uhf: u32, output: &OrbitalOutput) -> Orbitals {
    let nao = response.orbitals.as_ref().unwrap().nao() as u32;
    let rows = output.coefficient_rows(nao);
    canonical_orbitals(response, method, uhf, &rows).unwrap()
}

#[test]
fn spin_channels_and_sparse_rows_match_molden() {
    // Each spin's Aufbau HOMO is its electron count minus one, even with smearing.
    for (name, xyz, n_alpha, n_beta) in [("OH", OH, 4, 3), ("O2", O2, 7, 5)] {
        let uhf = n_alpha - n_beta;
        for method in [Method::Gfn2, Method::Gxtb] {
            let label = driver_method(method).name();
            let (response, molden) = compute_with_molden(&request(method, xyz, 0, uhf), true);
            let all_rows = (0..molden.alpha.len() as u32).collect::<Vec<_>>();
            for (selection, output) in [
                ("all", OrbitalOutput::All),
                ("sparse", OrbitalOutput::Rows(vec![1, 3])),
                ("alpha H/L", OrbitalOutput::Rows(vec![n_alpha - 1, n_alpha])),
                ("beta H/L", OrbitalOutput::Rows(vec![n_beta - 1, n_beta])),
            ] {
                let rows = match &output {
                    OrbitalOutput::All => &all_rows,
                    OrbitalOutput::Rows(rows) => rows,
                    _ => unreachable!(),
                };
                let canonical = convert(&response, method, uhf, &output);
                let beta_occupations = match (method, &canonical.beta) {
                    (Method::Gfn2, Beta::SameOrbitals { occupations }) => occupations,
                    (Method::Gxtb, Beta::Unrestricted(beta)) => &beta.occupations,
                    _ => panic!("unexpected beta representation for {label} {name}"),
                };
                for (spin, occupations, expected) in [
                    ("alpha", &canonical.alpha.occupations, n_alpha),
                    ("beta", beta_occupations, n_beta),
                ] {
                    let electrons = occupations.iter().sum::<f64>();
                    assert!(
                        (electrons - f64::from(expected)).abs() < 1e-6,
                        "{label} {name} {spin}: expected {expected} electrons, got {electrons}"
                    );
                }
                let report = check_spherical(&canonical, &molden, rows);
                report.assert_matches();
                println!(
                    "{label} {name} {selection} rows={rows:?} MOs/spin={}/{} occupations={} {report}",
                    rows.len(),
                    all_rows.len(),
                    molden.alpha.len() + molden.beta.len()
                );
            }
        }
    }
}

#[test]
fn unrestricted_beta_rejects_copied_alpha_coefficients() {
    let method = Method::Gxtb;
    let (response, molden) = compute_with_molden(&request(method, O2, 0, 2), true);
    let rows = [1, 3];
    let mut canonical = convert(&response, method, 2, &OrbitalOutput::Rows(rows.to_vec()));
    check_spherical(&canonical, &molden, &rows).assert_matches();

    let Beta::Unrestricted(beta) = &mut canonical.beta else {
        panic!("expected unrestricted g-xTB triplet");
    };
    beta.coefficients.clone_from(&canonical.alpha.coefficients);
    let wrong = check_spherical(&canonical, &molden, &rows);
    assert!(
        wrong.max_relative > 1e-3,
        "copied alpha coefficients escaped detection: {wrong}"
    );
    println!(
        "gxtb-v2.0.1 O2 copied alpha into beta rows={rows:?} MOs/spin={}/{} {wrong}",
        rows.len(),
        molden.beta.len()
    );
}
