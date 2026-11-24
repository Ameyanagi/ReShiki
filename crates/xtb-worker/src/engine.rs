//! Warm-started GFN2-xTB calculations through the axtb driver.

use crate::protocol::{
    self, Compute, Computed, Conventions, Engine, ErrorKind, Hello, Method, Open, Opened,
    Operation, ResponseError, SolventModel, Units,
};
use axtb_driver::{Calculator, Request, Task};

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
        methods: vec![Method::Gfn2],
        operations: vec![Operation::Energy, Operation::Gradient],
        max_atoms: protocol::MAX_ATOMS,
    }
}

pub(crate) struct Session {
    calculator: Calculator,
    positions: Vec<[f64; 3]>,
    angstrom_to_bohr: f64,
    method: Method,
    conventions: Conventions,
}

impl Session {
    pub(crate) fn open(open: Open, heap_bytes: usize) -> Result<(Self, Opened), ResponseError> {
        reshiki_process_heap::begin(heap_bytes);
        let method = match open.method {
            Method::Gfn2 => axtb_driver::Method::Gfn2,
        };
        let conventions = match open.conventions {
            Conventions::Axtb => "axtb",
            Conventions::Xtb68 => "xtb",
        };
        let angstrom_to_bohr = angstrom_to_bohr(method, conventions)?;
        let positions = to_bohr(open.positions, open.units, angstrom_to_bohr)?;
        let (alpb, gbsa) = match open.solvent {
            Some(solvent) if solvent.model == SolventModel::Alpb => (Some(solvent.name), None),
            Some(solvent) => (None, Some(solvent.name)),
            None => (None, None),
        };
        let request = Request {
            numbers: open.numbers.into_iter().map(usize::from).collect(),
            positions,
            charge: f64::from(open.charge),
            uhf: open.uhf.map(i64::from),
            method: Some(method.name().into()),
            conventions: Some(conventions.into()),
            alpb,
            gbsa,
            threads: Some(open.threads as usize),
            restart: Some("previous".into()),
            ..Request::default()
        };
        let calculator = Calculator::new(&request).map_err(engine_error)?;
        let opened = Opened {
            atoms: request.numbers.len(),
            method: open.method,
            conventions: open.conventions,
            numerics: calculator.numerics().to_owned(),
        };
        Ok((
            Self {
                calculator,
                positions: request.positions,
                angstrom_to_bohr,
                method: open.method,
                conventions: open.conventions,
            },
            opened,
        ))
    }

    pub(crate) fn atoms(&self) -> usize {
        self.positions.len()
    }

    pub(crate) fn compute(&mut self, compute: Compute) -> Result<Computed, ResponseError> {
        let positions = compute
            .positions
            .map(|positions| to_bohr(positions, compute.units, self.angstrom_to_bohr))
            .transpose()?;
        let task = Task::from_request(&Request {
            gradient: compute.gradient,
            ..Request::default()
        })
        .map_err(engine_error)?;
        let response = self
            .calculator
            .compute(positions.as_deref().unwrap_or(&self.positions), &task)
            .map_err(engine_error)?;
        let (Some(energy), Some(scf), Some(seconds)) =
            (response.energy, response.scf, response.seconds)
        else {
            return Err(ResponseError::new(
                ErrorKind::Numerical,
                "axtb returned an incomplete GFN2 result",
            ));
        };
        let homo_lumo_gap_ev = scf.homo_lumo_gap.map(|gap| gap * protocol::HARTREE_TO_EV);
        if homo_lumo_gap_ev.is_some_and(|gap| !gap.is_finite() || gap < 0.0) {
            return Err(ResponseError::new(
                ErrorKind::Numerical,
                "axtb returned an invalid HOMO-LUMO gap",
            ));
        }
        if let Some(positions) = positions {
            self.positions = positions;
        }
        Ok(Computed {
            method: self.method,
            conventions: self.conventions,
            numerics: response.numerics,
            energy,
            gradient: response.gradient,
            homo_lumo_gap_ev,
            near_degenerate: protocol::is_near_degenerate(homo_lumo_gap_ev),
            scf_iterations: scf.iterations,
            seconds,
        })
    }
}

fn angstrom_to_bohr(method: axtb_driver::Method, conventions: &str) -> Result<f64, ResponseError> {
    // Ask the driver's XYZ reader for its scale at 1 Å in the selected conventions.
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
        _ => ErrorKind::Input,
    };
    ResponseError::new(kind, error.to_string())
}

#[cfg(test)]
mod tests;
