//! Compare the worker's signed orbitals with axtb's Molden text from the same SCF.

mod grid;
mod open_shell;
mod reference;

use super::{Method, canonical_orbitals, driver_method};
use axtb_driver::{Calculator, MoldenTask, Request, Response, Task};
use reference::Molden;
use reshiki_qcdata::{Beta, Orbitals, Shell, contraction_overlap};

const WATER: &str = include_str!("../../../../tests/fixtures/xtb/water.xyz");
const CAFFEINE: &str = include_str!("../../../../tests/fixtures/xtb/caffeine.xyz");
const FERROCENE: &str = include_str!("../../../../tests/fixtures/xtb/ferrocene.xyz");

fn request(method: Method, xyz: &str, charge: i32, uhf: u32) -> Request {
    let method = driver_method(method);
    let (numbers, positions) = axtb_driver::read_xyz(xyz, method, None).unwrap();
    Request {
        numbers,
        positions,
        method: Some(method.name().into()),
        charge: f64::from(charge),
        uhf: Some(i64::from(uhf)),
        ..Request::default()
    }
}

fn compute_with_molden(request: &Request, spherical: bool) -> (Response, Molden) {
    let task = Task {
        orbitals: true,
        molden: Some(MoldenTask {
            // axtb-driver exposes MoldenTask; its functions enum implements FromStr.
            functions: if spherical { "spherical" } else { "cartesian" }
                .parse()
                .unwrap(),
            text: true,
            file: None,
        }),
        ..Task::default()
    };
    let response = Calculator::new(request)
        .unwrap()
        .compute(&request.positions, &task)
        .unwrap();
    let molden = reference::parse(response.molden.as_deref().unwrap(), spherical);
    (response, molden)
}

fn closed_shell_orbitals(request: &Request, method: Method, spherical: bool) -> (Orbitals, Molden) {
    let (response, molden) = compute_with_molden(request, spherical);
    let nao = response.orbitals.as_ref().unwrap().nao() as u32;
    let rows = (0..nao).collect::<Vec<_>>();
    let canonical = canonical_orbitals(&response, method, 0, &rows).unwrap();
    assert!(matches!(canonical.beta, Beta::Restricted));
    assert_eq!(canonical.alpha.rows, rows);
    assert_eq!(canonical.alpha.coefficients.len(), rows.len() * rows.len());
    for shell in &canonical.basis.as_ref().unwrap().shells {
        let overlap = contraction_overlap(shell);
        assert!(
            (overlap - 1.).abs() <= 1e-14,
            "atom {} l={} overlap {overlap}",
            shell.atom,
            shell.l
        );
    }
    assert_eq!(molden.alpha.len(), nao as usize);
    assert!(molden.beta.is_empty());
    (canonical, molden)
}

#[test]
fn ao_cartesian_grid_and_spherical_rebuild() {
    for (name, xyz, charge, method) in [
        ("water", WATER, 0, Method::Gfn2),
        ("caffeine", CAFFEINE, 0, Method::Gfn2),
        ("ferrocene", FERROCENE, 0, Method::Gfn2),
        ("water", WATER, 0, Method::Gxtb),
        ("ferrocene", FERROCENE, 0, Method::Gxtb),
        // The molecular fixtures go through d. Keep a real f shell in both comparisons.
        ("Ce(IV)", "1\ncerium ion\nCe 0 0 0\n", 4, Method::Gxtb),
    ] {
        let label = driver_method(method).name();
        let request = request(method, xyz, charge, 0);
        let (canonical, cartesian) = closed_shell_orbitals(&request, method, false);
        let basis = canonical.basis.as_ref().unwrap();
        let occupations = &canonical.alpha.occupations;
        let nmo = if name == "Ce(IV)" {
            assert!(basis.shells.iter().any(|s| s.l == 3));
            occupations.len() // Include every f orbital of the cerium ion.
        } else {
            let last_occupied = occupations.iter().rposition(|&n| n > 0.).unwrap();
            (last_occupied + 1 + 5).min(occupations.len())
        };
        let (points, errors) = grid::relative_errors(
            basis,
            &request.positions,
            &canonical.alpha.coefficients,
            &cartesian,
            nmo,
        );
        for (i, &error) in errors.iter().enumerate() {
            assert!(
                error <= 1e-10,
                "{label} {name} MO {i}: relative grid error {error:e}"
            );
        }
        let grid_max = errors.into_iter().fold(0., f64::max);

        let (canonical, spherical) = closed_shell_orbitals(&request, method, true);
        let report = reference::check_spherical(&canonical, &spherical, &canonical.alpha.rows);
        report.assert_matches();
        println!(
            "{label} {name}: grid MOs={nmo} points={points} max_rel={grid_max:.3e}; spherical {report}"
        );
    }
}

#[test]
fn ao_grid_rejects_native_p_order() {
    let request = request(Method::Gfn2, WATER, 0, 0);
    let (canonical, cartesian) = closed_shell_orbitals(&request, Method::Gfn2, false);
    let basis = canonical.basis.unwrap();
    let nao = basis.shells.iter().map(Shell::n_functions).sum();
    let mut wrong = canonical.alpha.coefficients;
    for row in wrong.chunks_exact_mut(nao) {
        let mut offset = 0;
        for shell in &basis.shells {
            if shell.l == 1 {
                row[offset..offset + 3].rotate_left(1); // x,y,z back to axtb's y,z,x
            }
            offset += shell.n_functions();
        }
    }
    let (_, errors) = grid::relative_errors(
        &basis,
        &request.positions,
        &wrong,
        &cartesian,
        cartesian.alpha.len(),
    );
    let max = errors.into_iter().fold(0., f64::max);
    assert!(max > 1e-3, "wrong p order escaped the grid check: {max:e}");
    println!("gfn2 water with deliberately wrong p order: max_rel={max:.3e}");
}
