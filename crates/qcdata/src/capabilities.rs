//! Method × task capabilities and calculation admission checks.
//!
//! Full references: docs/references/xtb.md.

use serde::{Deserialize, Serialize};

use crate::{
    Error, Frame, MAX_BASIS_FUNCTIONS, MAX_CALC_ATOMS, MAX_COEFFICIENTS, MAX_HESSIAN_ATOMS,
    MAX_TS_ATOMS, SolventCorrection, Task, XtbSettings,
};

/// An xTB engine method.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XtbMethod {
    /// GFN2-xTB (Bannwarth et al. 2019) \[Bannwarth2019GFN2\].
    #[default]
    Gfn2Xtb,
    /// GFN-FF (Spicher & Grimme 2020) \[Spicher2020GFNFF\].
    GfnFf,
    /// g-xTB 2.0.1 (Froitzheim et al. 2025) \[Froitzheim2025gxTB\].
    GXtb,
}

/// Tasks and outputs a method supports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Total energy.
    pub energy: bool,
    /// Energy gradient.
    pub gradient: bool,
    /// Geometry optimization.
    pub optimize: bool,
    /// Analytic Hessian.
    pub hessian: bool,
    /// Conformer search with re-ranking.
    pub conformers: bool,
    /// Transition-state search (path, saddle refinement and IRC).
    pub transition_state: bool,
    /// Orbital energies, occupations, and coefficients.
    pub orbitals: bool,
    /// Wiberg–Mayer bond orders.
    pub bond_orders: bool,
    /// Open-shell spin states.
    pub open_shell: bool,
    /// Implicit solvent treatment.
    pub solvent: SolventSupport,
}

/// Available implicit solvent treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolventSupport {
    /// The method computes the solvent effect directly.
    Native,
    /// Compute the solvent effect as a GFN2-xTB energy difference.
    Gfn2Correction,
    /// Solvents are unsupported.
    None,
}

impl XtbMethod {
    /// Return the conventional method name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Gfn2Xtb => "GFN2-xTB",
            Self::GfnFf => "GFN-FF",
            Self::GXtb => "g-xTB",
        }
    }

    /// Return the supported tasks, outputs, and solvent treatment.
    pub const fn capabilities(self) -> Capabilities {
        let electronic = !matches!(self, Self::GfnFf);
        Capabilities {
            energy: true,
            gradient: true,
            optimize: true,
            hessian: true,
            conformers: true,
            transition_state: electronic,
            orbitals: electronic,
            bond_orders: electronic,
            open_shell: electronic,
            solvent: match self {
                Self::Gfn2Xtb => SolventSupport::Native,
                Self::GfnFf => SolventSupport::None,
                Self::GXtb => SolventSupport::Gfn2Correction,
            },
        }
    }
}

/// The amount of orbital data requested from a calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbitalRequest {
    /// No orbital data.
    None,
    /// Energies and occupations only.
    Levels,
    /// Coefficient rows with a known basis width.
    Rows {
        /// Atomic orbital count.
        n_ao: u32,
        /// Requested molecular orbital row count.
        rows: u32,
    },
}

/// Check a frame, task, settings, and orbital request against the method's limits.
pub fn check_calculation(
    task: Task,
    settings: &XtbSettings,
    frame: &Frame,
    orbitals: OrbitalRequest,
) -> Result<(), Error> {
    frame.validate()?;
    let capabilities = settings.method.capabilities();
    let (supported, max) = match task {
        Task::Energy => (capabilities.energy, MAX_CALC_ATOMS),
        Task::Gradient => (capabilities.gradient, MAX_CALC_ATOMS),
        Task::Optimize => (capabilities.optimize, MAX_CALC_ATOMS),
        Task::Hessian => (capabilities.hessian, MAX_HESSIAN_ATOMS),
        Task::Conformers => (capabilities.conformers, MAX_HESSIAN_ATOMS),
        Task::TransitionState => (capabilities.transition_state, MAX_TS_ATOMS),
    };
    if !supported {
        return Err(Error::Unsupported(format!(
            "{} does not support {task:?}",
            settings.method.label()
        )));
    }
    let count = frame.numbers.len();
    if settings.threads == Some(0) {
        return Err(Error::Invalid("worker threads must be at least 1".into()));
    }
    if settings
        .fixed_atoms
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || settings
            .fixed_atoms
            .iter()
            .any(|&atom| atom as usize >= count)
    {
        return Err(Error::Invalid(
            "fixed atom indices must be strictly ascending and within the frame".into(),
        ));
    }
    if count > max {
        return Err(Error::TooLarge {
            what: "calculation atoms",
            count,
            max,
        });
    }
    if !capabilities.open_shell && frame.multiplicity != 1 {
        return Err(Error::Unsupported("GFN-FF requires a singlet".into()));
    }
    if let Some(solvent) = &settings.solvent {
        match (capabilities.solvent, solvent.correction) {
            (SolventSupport::Native, None)
            | (SolventSupport::Gfn2Correction, Some(SolventCorrection::Gfn2)) => {}
            (SolventSupport::Native, Some(_)) => {
                return Err(Error::Invalid(
                    "GFN2-xTB takes no solvent correction".into(),
                ));
            }
            (SolventSupport::Gfn2Correction, None) => {
                return Err(Error::Invalid(
                    "g-xTB solvent requires a GFN2-xTB correction".into(),
                ));
            }
            (SolventSupport::None, _) => {
                return Err(Error::Unsupported("GFN-FF does not support solvent".into()));
            }
        }
    }
    if settings.method == XtbMethod::GfnFf
        && (settings.electronic_temperature_k.is_some()
            || settings.accuracy.is_some()
            || settings.max_iterations.is_some()
            || !settings.scf_retries.is_empty())
    {
        return Err(Error::Unsupported("GFN-FF takes no SCF settings".into()));
    }
    // 0 K is valid (g-xTB's default); an accuracy is a positive multiplier.
    if settings
        .electronic_temperature_k
        .is_some_and(|t| !(t.is_finite() && t >= 0.0))
        || settings
            .accuracy
            .is_some_and(|a| !(a.is_finite() && a > 0.0))
        || settings.max_iterations == Some(0)
    {
        return Err(Error::Invalid(
            "electronic temperature must be finite and at least 0 K, accuracy positive, and iterations at least 1"
                .into(),
        ));
    }
    if settings.scf_retries.iter().any(|retry| {
        (retry.electronic_temperature_k.is_none() && retry.max_iterations.is_none())
            || retry
                .electronic_temperature_k
                .is_some_and(|t| !(t.is_finite() && t >= 0.0))
            || retry.max_iterations == Some(0)
    }) {
        return Err(Error::Invalid(
            "each SCF retry needs a finite temperature of at least 0 K or an iteration limit of at least 1".into(),
        ));
    }
    if let Some(base) = settings.electronic_temperature_k
        && settings.scf_retries.iter().any(|retry| {
            retry
                .electronic_temperature_k
                .is_some_and(|temperature| temperature < base)
        })
    {
        return Err(Error::Invalid(
            "SCF retry temperature must be at least the session temperature".into(),
        ));
    }
    if settings.method == XtbMethod::GfnFf {
        let topology = settings
            .gfnff
            .as_ref()
            .ok_or_else(|| Error::Invalid("GFN-FF requires a reference topology".into()))?;
        if topology.fragment_charges.len() > count {
            return Err(Error::Invalid(
                "GFN-FF has more fragment charges than atoms".into(),
            ));
        }
        if !topology.fragment_charges.is_empty()
            && topology
                .fragment_charges
                .iter()
                .copied()
                .map(i64::from)
                .sum::<i64>()
                != i64::from(frame.charge)
        {
            return Err(Error::Invalid(
                "GFN-FF fragment charges must sum to the frame charge".into(),
            ));
        }
    } else if settings.gfnff.is_some() {
        return Err(Error::Invalid(
            "only GFN-FF accepts a reference topology".into(),
        ));
    }
    if orbitals != OrbitalRequest::None && !capabilities.orbitals {
        return Err(Error::Unsupported(
            "GFN-FF does not provide orbitals".into(),
        ));
    }
    if let OrbitalRequest::Rows { n_ao, rows } = orbitals {
        if n_ao == 0 {
            return Err(Error::Invalid(
                "orbital rows require a nonempty basis".into(),
            ));
        }
        if n_ao as usize > MAX_BASIS_FUNCTIONS {
            return Err(Error::TooLarge {
                what: "basis functions",
                count: n_ao as usize,
                max: MAX_BASIS_FUNCTIONS,
            });
        }
        let count = (n_ao as usize).saturating_mul(rows as usize);
        if count > MAX_COEFFICIENTS {
            return Err(Error::TooLarge {
                what: "orbital coefficients",
                count,
                max: MAX_COEFFICIENTS,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GfnFfTopology, ScfRetry, SolvationState, Solvent, SolventModel};

    fn frame(n_atoms: usize) -> Frame {
        Frame {
            numbers: vec![2; n_atoms],
            positions_angstrom: vec![[0.0; 3]; n_atoms],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }
    }

    fn settings(method: XtbMethod) -> XtbSettings {
        XtbSettings {
            method,
            conventions: Default::default(),
            solvent: None,
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            scf_retries: vec![],
            gfnff: None,
            threads: None,
            convergence: Default::default(),
            fixed_atoms: vec![],
        }
    }

    #[test]
    fn scf_retry_settings_are_checked() {
        let frame = frame(1);
        let mut settings = settings(XtbMethod::Gfn2Xtb);
        let check = |settings: &XtbSettings| {
            check_calculation(Task::Energy, settings, &frame, OrbitalRequest::None)
        };
        for (electronic_temperature_k, max_iterations) in [
            (None, None),
            (Some(-1.0), Some(1)),
            (Some(f64::NAN), None),
            (Some(f64::INFINITY), None),
            (Some(1000.0), Some(0)),
        ] {
            settings.scf_retries = vec![ScfRetry {
                electronic_temperature_k,
                max_iterations,
            }];
            assert!(matches!(check(&settings), Err(Error::Invalid(_))));
        }
        settings.scf_retries = vec![
            ScfRetry {
                electronic_temperature_k: Some(0.0),
                max_iterations: None,
            },
            ScfRetry {
                electronic_temperature_k: None,
                max_iterations: Some(1),
            },
        ];
        check(&settings).unwrap();
        settings.electronic_temperature_k = Some(300.0);
        assert!(matches!(check(&settings), Err(Error::Invalid(_))));
        settings.electronic_temperature_k = None;
        settings.method = XtbMethod::GfnFf;
        assert!(matches!(
            check(&settings),
            Err(Error::Unsupported(message)) if message == "GFN-FF takes no SCF settings"
        ));
    }

    fn solvent(correction: Option<SolventCorrection>) -> Solvent {
        Solvent {
            model: SolventModel::Alpb,
            name: "water".into(),
            state: SolvationState::Gsolv,
            correction,
        }
    }

    #[test]
    fn capability_table_and_labels() {
        for (method, label, orbitals, bond_orders, open_shell, solvent) in [
            (
                XtbMethod::Gfn2Xtb,
                "GFN2-xTB",
                true,
                true,
                true,
                SolventSupport::Native,
            ),
            (
                XtbMethod::GfnFf,
                "GFN-FF",
                false,
                false,
                false,
                SolventSupport::None,
            ),
            (
                XtbMethod::GXtb,
                "g-xTB",
                true,
                true,
                true,
                SolventSupport::Gfn2Correction,
            ),
        ] {
            assert_eq!(method.label(), label);
            assert_eq!(
                method.capabilities(),
                Capabilities {
                    energy: true,
                    gradient: true,
                    optimize: true,
                    hessian: true,
                    conformers: true,
                    transition_state: method != XtbMethod::GfnFf,
                    orbitals,
                    bond_orders,
                    open_shell,
                    solvent,
                }
            );
        }
    }

    #[test]
    fn task_atom_boundaries() {
        let settings = settings(XtbMethod::Gfn2Xtb);
        for (task, limit) in [
            (Task::Energy, 500),
            (Task::Gradient, 500),
            (Task::Optimize, 500),
            (Task::Hessian, 200),
            (Task::Conformers, 200),
            (Task::TransitionState, 120),
        ] {
            check_calculation(task, &settings, &frame(limit), OrbitalRequest::None).unwrap();
            assert!(matches!(
                check_calculation(task, &settings, &frame(limit + 1), OrbitalRequest::None),
                Err(Error::TooLarge { count, max, .. }) if count == limit + 1 && max == limit
            ));
        }
    }

    #[test]
    fn transition_states_accept_both_electronic_methods() {
        for method in [XtbMethod::Gfn2Xtb, XtbMethod::GXtb, XtbMethod::GfnFf] {
            let admitted = check_calculation(
                Task::TransitionState,
                &settings(method),
                &frame(3),
                OrbitalRequest::None,
            );
            assert_eq!(admitted.is_ok(), method != XtbMethod::GfnFf);
        }
    }

    #[test]
    fn scf_settings_have_physical_ranges() {
        let base = settings(XtbMethod::Gfn2Xtb);
        let check = |settings: &XtbSettings| {
            check_calculation(Task::Energy, settings, &frame(3), OrbitalRequest::None)
        };
        for (temperature, accuracy, iterations, ok) in [
            (Some(0.0), Some(1e-4), Some(1), true),
            (Some(-300.0), None, None, false),
            (None, Some(0.0), None, false),
            (None, Some(-1.0), None, false),
            (None, None, Some(0), false),
        ] {
            let settings = XtbSettings {
                electronic_temperature_k: temperature,
                accuracy,
                max_iterations: iterations,
                ..base.clone()
            };
            assert_eq!(check(&settings).is_ok(), ok, "{settings:?}");
        }
    }

    #[test]
    fn gfnff_requires_topology_and_restricts_settings() {
        let frame = frame(3);
        let mut settings = settings(XtbMethod::GfnFf);
        let check = |settings: &XtbSettings, frame: &Frame| {
            check_calculation(Task::Energy, settings, frame, OrbitalRequest::None)
        };
        assert!(check(&settings, &frame).is_err());

        settings.gfnff = Some(GfnFfTopology {
            reference: frame.hash(),
            fragment_charges: vec![],
        });
        check(&settings, &frame).unwrap();
        let mut open = frame.clone();
        open.multiplicity = 3;
        assert!(check(&settings, &open).is_err());
        settings.gfnff.as_mut().unwrap().fragment_charges = vec![-1, 1];
        check(&settings, &frame).unwrap();
        settings.gfnff.as_mut().unwrap().fragment_charges = vec![-1, 2];
        assert!(check(&settings, &frame).is_err());
        settings.gfnff.as_mut().unwrap().fragment_charges.clear();

        for invalid in [
            XtbSettings {
                solvent: Some(solvent(None)),
                ..settings.clone()
            },
            XtbSettings {
                electronic_temperature_k: Some(300.0),
                ..settings.clone()
            },
            XtbSettings {
                accuracy: Some(0.05),
                ..settings.clone()
            },
            XtbSettings {
                max_iterations: Some(12),
                ..settings.clone()
            },
            XtbSettings {
                method: XtbMethod::Gfn2Xtb,
                ..settings.clone()
            },
        ] {
            assert!(check(&invalid, &frame).is_err());
        }
        for request in [
            OrbitalRequest::Levels,
            OrbitalRequest::Rows { n_ao: 3, rows: 1 },
        ] {
            assert!(check_calculation(Task::Energy, &settings, &frame, request).is_err());
        }
    }

    #[test]
    fn gfnff_fragment_count_cannot_exceed_atom_count() {
        let frame = frame(2);
        let mut settings = settings(XtbMethod::GfnFf);
        settings.gfnff = Some(GfnFfTopology {
            reference: frame.hash(),
            fragment_charges: vec![-1, 1],
        });
        check_calculation(Task::Energy, &settings, &frame, OrbitalRequest::None).unwrap();
        settings.gfnff.as_mut().unwrap().fragment_charges.push(0);
        assert!(check_calculation(Task::Energy, &settings, &frame, OrbitalRequest::None).is_err());
    }

    #[test]
    fn solvent_rules() {
        let frame = frame(3);
        let mut settings = settings(XtbMethod::GXtb);
        let check = |settings: &XtbSettings| {
            check_calculation(Task::Hessian, settings, &frame, OrbitalRequest::Levels)
        };
        settings.solvent = Some(solvent(None));
        assert!(check(&settings).is_err());
        settings.solvent = Some(solvent(Some(SolventCorrection::Gfn2)));
        check(&settings).unwrap();
        settings.method = XtbMethod::Gfn2Xtb;
        assert!(check(&settings).is_err());
        settings.solvent = Some(solvent(None));
        check(&settings).unwrap();
    }

    #[test]
    fn orbital_limits_are_independent_of_calculation_limits() {
        let settings = settings(XtbMethod::Gfn2Xtb);
        let frame = frame(500);
        let check = |request| check_calculation(Task::Optimize, &settings, &frame, request);
        check(OrbitalRequest::None).unwrap();
        check(OrbitalRequest::Levels).unwrap();
        check(OrbitalRequest::Rows {
            n_ao: 2048,
            rows: 2048,
        })
        .unwrap();
        assert!(matches!(
            check(OrbitalRequest::Rows {
                n_ao: 2048,
                rows: 2049
            }),
            Err(Error::TooLarge {
                max: MAX_COEFFICIENTS,
                ..
            })
        ));
        assert!(matches!(
            check(OrbitalRequest::Rows {
                n_ao: 10_001,
                rows: 1
            }),
            Err(Error::TooLarge {
                max: MAX_BASIS_FUNCTIONS,
                ..
            })
        ));
    }

    #[test]
    fn requested_orbital_rows_need_a_nonzero_basis_width() {
        let settings = settings(XtbMethod::Gfn2Xtb);
        let frame = frame(2);
        let check = |n_ao, rows| {
            check_calculation(
                Task::Energy,
                &settings,
                &frame,
                OrbitalRequest::Rows { n_ao, rows },
            )
        };
        assert!(matches!(check(0, 0), Err(Error::Invalid(_))));
        assert!(matches!(check(0, 1), Err(Error::Invalid(_))));
        check(1, 0).unwrap();
    }
}
