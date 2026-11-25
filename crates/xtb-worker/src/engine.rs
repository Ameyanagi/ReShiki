//! Warm-started calculations through the pinned axtb driver.

use crate::protocol::{
    self, ArrayHeader, ArrayName, Compute, Computed, Conventions, Engine, ErrorKind, GfnFfReport,
    GfnFfSettings, Hello, Method, Open, Opened, OrbitalOutput, ResponseError, RetryReport,
    ScfReport, SessionSettings, SolventModel, Units, VibrationsReport,
};
use axtb_driver::{Calculator, Request, Task};
use reshiki_qcdata::{
    AtomicCharges, BasisSet, Beta, BondOrder, ChargeScheme, OrbitalSet, Orbitals, Thermo,
};

pub(crate) const VERSION: &str = axtb_driver::VERSION;

pub(crate) fn hello() -> Hello {
    Hello {
        protocol: protocol::VERSION,
        engine: Some(Engine {
            name: "axtb".into(),
            version: VERSION.into(),
            license: crate::AXTB_LICENSE.into(),
            source: crate::AXTB_SOURCE.into(),
            rev: crate::AXTB_REV.into(),
        }),
        methods: Method::ALL.to_vec(),
        max_atoms: protocol::MAX_ATOMS,
        max_hessian_atoms: protocol::MAX_HESSIAN_ATOMS,
    }
}

pub(crate) struct Session {
    calculator: Calculator,
    open: Open,
    positions: Vec<[f64; 3]>,
    angstrom_to_bohr: f64,
    basis_functions: Option<u32>,
    settings: SessionSettings,
}

impl Session {
    pub(crate) fn open(open: Open, heap_bytes: usize) -> Result<(Self, Opened), ResponseError> {
        open.admit(None, None, protocol::MAX_ATOMS)?;
        reshiki_process_heap::begin(heap_bytes);
        let method = driver_method(open.method);
        let uhf = validate_electrons(&open, method)?;
        let conventions = driver_conventions(open.method, open.conventions);
        let angstrom_to_bohr = angstrom_to_bohr(method, conventions)?;
        let positions = to_bohr(open.positions.clone(), open.units, angstrom_to_bohr)?;
        let gfnff = if open.method == Method::Gfnff {
            Some(GfnFfSettings {
                reference_positions_bohr: to_bohr(
                    open.gfnff
                        .as_ref()
                        .and_then(|g| g.reference_positions.clone())
                        .unwrap_or_else(|| open.positions.clone()),
                    open.units,
                    angstrom_to_bohr,
                )?,
                fragment_charges: open
                    .gfnff
                    .as_ref()
                    .and_then(|g| g.fragment_charges.clone())
                    .unwrap_or_default(),
            })
        } else {
            None
        };
        let scf = open.scf.clone();
        let solvent = open.solvent.as_ref();
        let request = Request {
            numbers: open.numbers.iter().map(|&z| usize::from(z)).collect(),
            positions: positions.clone(),
            charge: f64::from(open.charge),
            uhf: Some(i64::from(uhf)),
            method: Some(method.name().into()),
            conventions: Some(conventions.into()),
            alpb: solvent
                .filter(|s| s.model == SolventModel::Alpb)
                .map(|s| s.name.clone()),
            gbsa: solvent
                .filter(|s| s.model == SolventModel::Gbsa)
                .map(|s| s.name.clone()),
            solvation_state: solvent
                .filter(|s| s.state != reshiki_qcdata::SolvationState::Gsolv)
                .map(|s| {
                    match s.state {
                        reshiki_qcdata::SolvationState::Gsolv => "gsolv",
                        reshiki_qcdata::SolvationState::Bar1mol => "bar1M",
                        reshiki_qcdata::SolvationState::Reference => "reference",
                    }
                    .into()
                }),
            solvation_correction: solvent.and_then(|s| s.correction).map(|_| "gfn2".into()),
            accuracy: scf.accuracy,
            etemp: scf.electronic_temperature_k,
            max_iter: scf.max_iterations.map(|n| n as usize),
            scf_retries: (!scf.retries.is_empty()).then(|| {
                scf.retries
                    .iter()
                    .map(|r| axtb_driver::ScfRetry {
                        etemp: r.electronic_temperature_k,
                        max_iter: r.max_iterations.map(|n| n as usize),
                    })
                    .collect()
            }),
            threads: Some(open.threads as usize),
            restart: (open.method != Method::Gfnff).then(|| "previous".into()),
            topology_positions: gfnff.as_ref().map(|g| g.reference_positions_bohr.clone()),
            fragment_charges: gfnff
                .as_ref()
                .filter(|g| !g.fragment_charges.is_empty())
                .map(|g| g.fragment_charges.iter().map(|&q| f64::from(q)).collect()),
            ..Request::default()
        };
        let calculator = Calculator::new(&request).map_err(engine_error)?;
        let basis_functions = if open.method == Method::Gfnff {
            None
        } else {
            let shells = axtb_driver::shell_layout(method, &request.numbers).ok_or_else(|| {
                ResponseError::new(
                    ErrorKind::UnsupportedElement,
                    "axtb cannot describe the basis for these elements",
                )
            })?;
            Some(shells.iter().map(|shell| (2 * shell.l + 1) as u32).sum())
        };
        let opened = Opened {
            atoms: request.numbers.len(),
            method: open.method,
            conventions: open.conventions,
            numerics: calculator.numerics().to_owned(),
            engine_conventions: calculator.conventions().into(),
            basis_functions,
        };
        let settings = SessionSettings {
            charge: open.charge,
            uhf,
            solvent: open.solvent.clone(),
            scf,
            gfnff,
            threads: open.threads,
        };
        Ok((
            Self {
                calculator,
                open,
                positions,
                angstrom_to_bohr,
                basis_functions,
                settings,
            },
            opened,
        ))
    }

    pub(crate) fn atoms(&self) -> usize {
        self.positions.len()
    }

    pub(crate) fn compute(&mut self, compute: Compute) -> Result<Computed, ResponseError> {
        let admission;
        let current = if compute.positions.is_some() {
            &compute
        } else {
            admission = Compute {
                positions: Some(self.positions.clone()),
                units: Units::Bohr,
                ..compute.clone()
            };
            &admission
        };
        self.open
            .admit(Some(current), self.basis_functions, protocol::MAX_ATOMS)?;
        let positions = compute
            .positions
            .clone()
            .map(|positions| to_bohr(positions, compute.units, self.angstrom_to_bohr))
            .transpose()?;
        let thermo = compute.hessian.as_ref().map(|h| &h.thermo);
        let mut task = Task::from_request(&Request {
            gradient: compute.gradient,
            hessian: compute.hessian.is_some(),
            hessian_method: compute.hessian.as_ref().map(|_| "analytic".into()),
            bond_orders: compute.bond_orders,
            temperature: thermo
                .and_then(|t| t.temperature_k)
                .map(axtb_driver::Temperatures::One),
            pressure: thermo.and_then(|t| t.pressure_pa),
            rotor_cutoff: thermo.and_then(|t| t.rotor_cutoff_cm1),
            imaginary_cutoff: thermo.and_then(|t| t.imaginary_cutoff_cm1),
            frequency_scale: thermo.and_then(|t| t.frequency_scale),
            symmetry_number: thermo.and_then(|t| t.symmetry_number),
            ..Request::default()
        })
        .map_err(engine_error)?;
        let rows = self
            .basis_functions
            .map(|n| compute.orbitals.coefficient_rows(n))
            .unwrap_or_default();
        task.orbitals = !rows.is_empty();
        let response = self
            .calculator
            .compute(positions.as_deref().unwrap_or(&self.positions), &task)
            .map_err(engine_error)?;
        let energy = response.energy.ok_or_else(|| incomplete("energy"))?;
        let seconds = response.seconds.ok_or_else(|| incomplete("timing"))?;
        if self.open.method != Method::Gfnff && response.scf.is_none() {
            return Err(incomplete("SCF report"));
        }
        if (compute.gradient || compute.hessian.is_some()) && response.gradient.is_none() {
            return Err(incomplete("gradient"));
        }
        if compute.hessian.is_some()
            && (response.hessian.is_none() || response.vibrations.is_none())
        {
            return Err(incomplete("Hessian and vibrational analysis"));
        }
        let homo_lumo_gap_ev = response
            .scf
            .as_ref()
            .and_then(|s| s.homo_lumo_gap)
            .map(|gap| gap * protocol::HARTREE_TO_EV);
        if homo_lumo_gap_ev.is_some_and(|gap| !gap.is_finite() || gap < 0.0) {
            return Err(incomplete("valid HOMO-LUMO gap"));
        }
        let scf = response.scf.as_ref().map(|scf| ScfReport {
            iterations: scf.iterations,
            start: scf.guess.into(),
            electronic_temperature_k: scf.electronic_temperature,
            retry: response.scf_retry.as_ref().map(|r| RetryReport {
                rung: r.rung,
                of: r.of,
                label: r.label.clone(),
            }),
        });
        let orbitals = if compute.orbitals != OrbitalOutput::None {
            let scf = response
                .scf
                .as_ref()
                .ok_or_else(|| incomplete("orbital levels"))?;
            let core_electrons = scf.core_electrons.iter().map(|&n| n as u32).collect();
            let mut basis = None;
            let mut alpha_coefficients = Vec::new();
            let mut beta_coefficients = Vec::new();
            if !rows.is_empty() {
                let orbitals = response
                    .orbitals
                    .as_ref()
                    .ok_or_else(|| incomplete("orbitals"))?;
                let mut canonical = BasisSet { shells: Vec::new() };
                let mut norms = Vec::new();
                for (atom, shells) in orbitals.shells.iter().enumerate() {
                    for shell in shells {
                        let (shell, norm) = crate::ao::shell(
                            atom,
                            shell.l,
                            &shell.alpha[..shell.nprim],
                            &shell.coeff[..shell.nprim],
                        )?;
                        canonical.shells.push(shell);
                        norms.push(norm);
                    }
                }
                alpha_coefficients =
                    crate::ao::rows(&canonical, &norms, &orbitals.sets[0].coefficients, &rows)?;
                if self.settings.uhf > 0 && self.open.method == Method::Gxtb {
                    beta_coefficients =
                        crate::ao::rows(&canonical, &norms, &orbitals.sets[1].coefficients, &rows)?;
                }
                basis = Some(canonical);
            }
            let mut alpha = OrbitalSet {
                energies_hartree: scf.orbital_energies_alpha.clone(),
                occupations: scf.occupations_alpha.clone(),
                symmetries: Vec::new(),
                rows: rows.clone(),
                coefficients: alpha_coefficients,
            };
            let beta = if self.settings.uhf == 0 {
                alpha.occupations = scf
                    .occupations_alpha
                    .iter()
                    .zip(&scf.occupations_beta)
                    .map(|(a, b)| a + b)
                    .collect();
                Beta::Restricted
            } else if self.open.method == Method::Gfn2 {
                Beta::SameOrbitals {
                    occupations: scf.occupations_beta.clone(),
                }
            } else {
                Beta::Unrestricted(OrbitalSet {
                    energies_hartree: scf.orbital_energies_beta.clone(),
                    occupations: scf.occupations_beta.clone(),
                    symmetries: Vec::new(),
                    rows: rows.clone(),
                    coefficients: beta_coefficients,
                })
            };
            Some(Orbitals {
                basis,
                core_electrons,
                alpha,
                beta,
            })
        } else {
            None
        };
        let bond_orders = if compute.bond_orders {
            let matrix = response
                .scf
                .as_ref()
                .and_then(|s| s.bond_orders.as_ref())
                .ok_or_else(|| incomplete("bond orders"))?;
            let mut pairs = Vec::new();
            for a in 0..self.atoms() {
                for b in a + 1..self.atoms() {
                    let order = matrix.data[a * matrix.cols + b];
                    if order >= protocol::BOND_ORDER_MIN {
                        pairs.push(BondOrder {
                            a: a as u32,
                            b: b as u32,
                            order,
                        });
                    }
                }
            }
            Some(pairs)
        } else {
            None
        };
        let gfnff = response.gfnff.as_ref().map(|g| GfnFfReport {
            fragments: g.topology.fragments.iter().map(|&f| f as u32).collect(),
        });
        if let Some(gfnff) = &response.gfnff {
            self.settings
                .gfnff
                .as_mut()
                .ok_or_else(|| incomplete("GFN-FF settings"))?
                .fragment_charges = gfnff
                .topology
                .fragment_charges
                .iter()
                .map(|&charge| {
                    let rounded = charge.round() as i32;
                    if (charge - f64::from(rounded)).abs() <= 1e-9 {
                        Ok(rounded)
                    } else {
                        Err(ResponseError::new(
                            ErrorKind::Numerical,
                            "axtb returned a nonintegral GFN-FF fragment charge",
                        ))
                    }
                })
                .collect::<Result<_, _>>()?;
        }
        let mut hessian = Vec::new();
        let mut dipole_derivatives = Vec::new();
        let mut hessian_method = None;
        if let Some(part) = response.hessian {
            hessian = part.hessian.data;
            dipole_derivatives = part
                .dipole_derivatives
                .into_iter()
                .flatten()
                .flatten()
                .collect();
            hessian_method = Some(part.hessian_method.into());
        }
        let mut normal_modes = Vec::new();
        let mut thermo_result = None;
        let vibrations = response.vibrations.map(|v| {
            normal_modes = v.normal_modes.into_iter().flatten().flatten().collect();
            let point_group = v.thermo.as_ref().map(|t| t.point_group.clone());
            thermo_result = v.thermo.map(|t| Thermo {
                temperature_k: t.last.temperature,
                pressure_pa: t.pressure,
                zero_point_energy_hartree: t.zpve,
                enthalpy_correction_hartree: t.last.enthalpy,
                entropy_hartree_per_k: t.last.entropy,
                gibbs_correction_hartree: t.last.g_rrho,
                heat_capacity_hartree_per_k: Some(t.last.heat_capacity),
                frequency_scale: t.frequency_scale,
                rotor_cutoff_cm1: t.rotor_cutoff,
                imaginary_cutoff_cm1: t.imaginary_cutoff,
                symmetry_number: t.symmetry_number,
            });
            VibrationsReport {
                conventions: v.vibrational_conventions.into(),
                frequencies_cm1: v.frequencies,
                ir_intensities_km_mol: v.ir_intensities.unwrap_or_default(),
                reduced_masses_amu: v.reduced_masses,
                force_constants_mdyn_per_angstrom: v.force_constants,
                masses_amu: v.masses,
                linear: v.linear,
                point_group,
            }
        });
        let mut arrays = Vec::new();
        let axes = (3 * self.atoms()) as u32;
        let mut announce = |name, shape: [u32; 2], count| {
            if count > 0 {
                arrays.push(ArrayHeader {
                    name,
                    shape,
                    chunks: protocol::chunk_count(count),
                });
            }
        };
        announce(ArrayName::Hessian, [axes, axes], hessian.len());
        announce(
            ArrayName::DipoleDerivatives,
            [axes, 3],
            dipole_derivatives.len(),
        );
        announce(
            ArrayName::NormalModes,
            [
                vibrations.as_ref().map_or(0, |v| v.frequencies_cm1.len()) as u32,
                axes,
            ],
            normal_modes.len(),
        );
        if let Some(orbitals) = &orbitals {
            announce(
                ArrayName::CoefficientsAlpha,
                [rows.len() as u32, self.basis_functions.unwrap_or(0)],
                orbitals.alpha.coefficients.len(),
            );
            if let Beta::Unrestricted(beta) = &orbitals.beta {
                announce(
                    ArrayName::CoefficientsBeta,
                    [rows.len() as u32, self.basis_functions.unwrap_or(0)],
                    beta.coefficients.len(),
                );
            }
        }
        let computed = Computed {
            method: self.open.method,
            conventions: self.open.conventions,
            engine_conventions: response.conventions.into(),
            numerics: response.numerics,
            settings: self.settings.clone(),
            energy,
            energy_terms: response
                .terms
                .ok_or_else(|| incomplete("energy terms"))?
                .0
                .into_iter()
                .map(|(k, v)| (k.into(), v))
                .collect(),
            gradient: response.gradient,
            charges: Some(AtomicCharges {
                scheme: if self.open.method == Method::Gfnff {
                    ChargeScheme::Eeq
                } else {
                    ChargeScheme::Mulliken
                },
                values: response.charges.ok_or_else(|| incomplete("charges"))?,
            }),
            dipole_e_bohr: response.dipole,
            quadrupole_e_bohr2: response.scf.as_ref().map(|s| s.quadrupole_traceless),
            bond_orders,
            orbitals,
            homo_lumo_gap_ev,
            near_degenerate: protocol::is_near_degenerate(homo_lumo_gap_ev),
            scf,
            composite: response.composite.map(|c| {
                c.components
                    .0
                    .into_iter()
                    .map(|(k, v)| (k.into(), v))
                    .collect()
            }),
            gfnff,
            hessian_method,
            vibrations,
            thermo: thermo_result,
            arrays,
            hessian,
            dipole_derivatives,
            normal_modes,
            seconds,
        };
        if let Some(positions) = positions {
            self.positions = positions;
        }
        Ok(computed)
    }
}

fn incomplete(part: &str) -> ResponseError {
    ResponseError::new(ErrorKind::Numerical, format!("axtb returned no {part}"))
}

fn driver_method(method: Method) -> axtb_driver::Method {
    match method {
        Method::Gfn2 => axtb_driver::Method::Gfn2,
        Method::Gfnff => axtb_driver::Method::Gfnff,
        Method::Gxtb => axtb_driver::Method::GxtbV2_0_1,
    }
}

fn driver_conventions(method: Method, conventions: Conventions) -> &'static str {
    match (method, conventions) {
        (_, Conventions::Axtb) => "axtb",
        (Method::Gxtb, Conventions::Xtb68) => "gxtb",
        (_, Conventions::Xtb68) => "xtb",
    }
}

fn validate_electrons(open: &Open, method: axtb_driver::Method) -> Result<u32, ResponseError> {
    if open.method == Method::Gfnff {
        return Ok(0);
    }
    let neutral_electrons = open
        .numbers
        .iter()
        .map(|&number| axtb_driver::valence_electrons(method, usize::from(number)))
        .sum::<Option<f64>>();
    let Some(neutral_electrons) = neutral_electrons else {
        // Calculator construction reports invalid or unsupported atomic numbers.
        return Ok(open.uhf.unwrap_or(0));
    };
    // g-xTB's tabulated reference occupations sum to whole valence counts
    // within rounding error. Its SCF also resolves spin parity from that integer.
    let electrons = neutral_electrons.round() - f64::from(open.charge);
    if electrons < 0.0 {
        return Err(ResponseError::new(
            ErrorKind::Input,
            "charge leaves a negative valence electron count",
        ));
    }
    let uhf = open.uhf.unwrap_or((electrons as u32) % 2);
    if f64::from(uhf) > electrons || f64::from(uhf) % 2.0 != electrons % 2.0 {
        return Err(ResponseError::new(
            ErrorKind::Input,
            "uhf must have the same parity as the valence electron count and cannot exceed it",
        ));
    }
    Ok(uhf)
}

fn angstrom_to_bohr(method: axtb_driver::Method, conventions: &str) -> Result<f64, ResponseError> {
    let (_, positions) =
        axtb_driver::read_xyz("1\n\nH 1 0 0\n", method, Some(conventions)).map_err(engine_error)?;
    Ok(positions[0][0])
}

fn to_bohr(
    mut positions: Vec<[f64; 3]>,
    units: Units,
    angstrom_to_bohr: f64,
) -> Result<Vec<[f64; 3]>, ResponseError> {
    if units == Units::Angstrom {
        for coordinate in positions.iter_mut().flatten() {
            *coordinate *= angstrom_to_bohr;
            if !coordinate.is_finite() {
                return Err(ResponseError::new(
                    ErrorKind::Input,
                    "position is out of range in bohr",
                ));
            }
        }
    }
    Ok(positions)
}

fn engine_error(error: axtb_driver::Error) -> ResponseError {
    let kind = match error.kind() {
        "unsupported_element" => ErrorKind::UnsupportedElement,
        "topology" => ErrorKind::Topology,
        "scf_not_converged" => ErrorKind::Scf,
        "numerical" => ErrorKind::Numerical,
        "not_applicable" => ErrorKind::Unsupported,
        _ => ErrorKind::Input,
    };
    ResponseError::new(kind, error.to_string())
}

#[cfg(test)]
mod molden_tests;
#[cfg(test)]
mod tests;
