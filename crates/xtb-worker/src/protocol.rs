//! Version 2 of the ReShiki xTB worker wire format.

use reshiki_qcdata::{
    AtomicCharges, Beta, BondOrder, CalcResult, Error, Frame, FrameHash, GfnFfTopology, Orbitals,
    Provenance, Settings, Source, Status, Task, Thermo, Vibrations, XtbMethod, XtbSettings,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
};

pub use reshiki_qcdata::{
    ScfRetry, SolvationState, Solvent, SolventCorrection, SolventModel,
    XtbConventions as Conventions,
};

pub const VERSION: u16 = 2;
pub const MAX_ATOMS: usize = 500;
pub const MAX_HESSIAN_ATOMS: usize = reshiki_qcdata::MAX_HESSIAN_ATOMS;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const CHUNK_VALUES: usize = (MAX_FRAME_BYTES - 4) / 8;
pub const BOND_ORDER_MIN: f64 = 0.1;
pub const HEADER_LEN: usize = 16;
/// CODATA 2022 Hartree energy in eV.
pub const HARTREE_TO_EV: f64 = 27.211386245981;
pub const NEAR_DEGENERATE_GAP_EV: f64 = 0.1;
const MAGIC: &[u8; 8] = b"RSHXTBWK";

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
pub(crate) fn is_near_degenerate(gap_ev: Option<f64>) -> bool {
    gap_ev.is_some_and(|gap| gap < NEAR_DEGENERATE_GAP_EV)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum FrameKind {
    Json = 0,
    Chunk = 1,
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("invalid xTB frame magic")]
    Magic,
    #[error("unsupported xTB protocol version")]
    Version,
    #[error("unsupported xTB frame kind")]
    Kind,
    #[error("xTB frame exceeds 4 MiB")]
    Length,
    #[error("invalid xTB chunk: {0}")]
    Chunk(&'static str),
    #[error("reading or writing xTB frame: {0}")]
    Io(#[from] io::Error),
    #[error("invalid xTB JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn decode_header(header: &[u8; HEADER_LEN]) -> Result<(FrameKind, usize), FrameError> {
    if &header[..8] != MAGIC {
        return Err(FrameError::Magic);
    }
    if u16::from_le_bytes([header[8], header[9]]) != VERSION {
        return Err(FrameError::Version);
    }
    let kind = match u16::from_le_bytes([header[10], header[11]]) {
        0 => FrameKind::Json,
        1 => FrameKind::Chunk,
        _ => return Err(FrameError::Kind),
    };
    let size = u32::from_le_bytes(header[12..].try_into().unwrap()) as usize;
    if size > MAX_FRAME_BYTES {
        return Err(FrameError::Length);
    }
    Ok((kind, size))
}

fn frame_header(kind: FrameKind, size: usize) -> [u8; HEADER_LEN] {
    let mut header = [0; HEADER_LEN];
    header[..8].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&VERSION.to_le_bytes());
    header[10..12].copy_from_slice(&(kind as u16).to_le_bytes());
    header[12..].copy_from_slice(&(size as u32).to_le_bytes());
    header
}

/// A clean EOF is possible only before the first byte of the header.
pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, FrameError> {
    let mut header = [0; HEADER_LEN];
    match reader.read_exact(&mut header[..1]) {
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        result => result?,
    }
    reader.read_exact(&mut header[1..])?;
    let (kind, size) = decode_header(&header)?;
    if kind != FrameKind::Json {
        return Err(FrameError::Kind);
    }
    let mut body = vec![0; size];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

pub fn encode_frame(value: &impl Serialize) -> Result<Vec<u8>, FrameError> {
    let mut frame = FrameBuffer(vec![0; HEADER_LEN]);
    serde_json::to_writer(&mut frame, value).map_err(|error| {
        if error.is_io() {
            FrameError::Length
        } else {
            FrameError::Json(error)
        }
    })?;
    let header = frame_header(FrameKind::Json, frame.0.len() - HEADER_LEN);
    frame.0[..HEADER_LEN].copy_from_slice(&header);
    Ok(frame.0)
}

pub fn encode_chunk(index: u32, values: &[f64]) -> Vec<u8> {
    assert!((1..=CHUNK_VALUES).contains(&values.len()));
    let size = 4 + size_of_val(values);
    let mut frame = Vec::with_capacity(HEADER_LEN + size);
    frame.extend(frame_header(FrameKind::Chunk, size));
    frame.extend(index.to_le_bytes());
    for &value in values {
        frame.extend(value.to_le_bytes());
    }
    frame
}

#[cfg(any(not(target_arch = "wasm32"), test))]
pub(crate) fn decode_chunk(
    body: &[u8],
    index: u32,
    values: usize,
    out: &mut Vec<f64>,
) -> Result<(), FrameError> {
    if !(1..=CHUNK_VALUES).contains(&values) || body.len() != 4 + values * 8 {
        return Err(FrameError::Chunk("wrong value count"));
    }
    if body[..4] != index.to_le_bytes() {
        return Err(FrameError::Chunk("unexpected index"));
    }
    for bytes in body[4..].as_chunks::<8>().0 {
        let value = f64::from_le_bytes(*bytes);
        if !value.is_finite() {
            return Err(FrameError::Chunk("non-finite value"));
        }
        out.push(value);
    }
    Ok(())
}

pub fn chunk_count(values: usize) -> u32 {
    values.div_ceil(CHUNK_VALUES) as u32
}

#[cfg(any(feature = "engine", test))]
pub(crate) fn write_array(writer: &mut impl Write, values: &[f64]) -> io::Result<()> {
    for (index, chunk) in values.chunks(CHUNK_VALUES).enumerate() {
        writer.write_all(&encode_chunk(index as u32, chunk))?;
    }
    Ok(())
}

struct FrameBuffer(Vec<u8>);

impl Write for FrameBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > HEADER_LEN + MAX_FRAME_BYTES - self.0.len() {
            return Err(io::Error::other(FrameError::Length));
        }
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub op: RequestOp,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RequestOp {
    Hello {},
    Open(Open),
    Compute(Compute),
    Close {},
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    Angstrom,
    Bohr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Gfn2,
    Gfnff,
    Gxtb,
}

impl Method {
    pub const ALL: [Self; 3] = [Self::Gfn2, Self::Gfnff, Self::Gxtb];

    pub fn xtb_method(self) -> XtbMethod {
        match self {
            Self::Gfn2 => XtbMethod::Gfn2Xtb,
            Self::Gfnff => XtbMethod::GfnFf,
            Self::Gxtb => XtbMethod::GXtb,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScfSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub electronic_temperature_k: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retries: Vec<ScfRetry>,
}

impl ScfSettings {
    fn xtb_settings(
        &self,
        method: Method,
        conventions: Conventions,
        solvent: Option<Solvent>,
        gfnff: Option<GfnFfTopology>,
    ) -> XtbSettings {
        XtbSettings {
            method: method.xtb_method(),
            conventions,
            solvent,
            electronic_temperature_k: self.electronic_temperature_k,
            accuracy: self.accuracy,
            max_iterations: self.max_iterations,
            scf_retries: self.retries.clone(),
            gfnff,
            threads: None,
            convergence: Default::default(),
            fixed_atoms: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GfnFfOpen {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_positions: Option<Vec<[f64; 3]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_charges: Option<Vec<i32>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Open {
    pub numbers: Vec<u8>,
    pub positions: Vec<[f64; 3]>,
    pub units: Units,
    #[serde(default)]
    pub charge: i32,
    pub uhf: Option<u32>,
    pub method: Method,
    #[serde(default)]
    pub conventions: Conventions,
    pub solvent: Option<Solvent>,
    #[serde(default)]
    pub scf: ScfSettings,
    #[serde(default)]
    pub gfnff: Option<GfnFfOpen>,
    #[serde(default = "one_thread")]
    pub threads: u32,
    pub heap_bytes: u64,
}

fn one_thread() -> u32 {
    1
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
impl Open {
    pub(crate) fn admit(
        &self,
        compute: Option<&Compute>,
        basis_functions: Option<u32>,
        max_atoms: usize,
    ) -> Result<Frame, ResponseError> {
        use reshiki_qcdata::{OrbitalRequest, check_calculation};

        let atoms = self.numbers.len();
        validate_atom_count(atoms, max_atoms)?;
        if !(1..=64).contains(&self.threads) {
            return Err(ResponseError::new(
                ErrorKind::Input,
                "expected 1 to 64 threads",
            ));
        }

        let replacement = compute.and_then(|c| c.positions.as_deref().map(|p| (p, c.units)));
        let (positions, units) = replacement.unwrap_or((&self.positions, self.units));
        validate_positions(positions, atoms)?;
        // The qcdata frame describes all electrons; the engine resolves spin in its valence space.
        let electrons =
            self.numbers.iter().copied().map(i64::from).sum::<i64>() - i64::from(self.charge);
        let multiplicity = 1 + electrons.rem_euclid(2) as u32;
        let frame = make_frame(&self.numbers, positions, units, self.charge, multiplicity);
        let gfnff = if self.method == Method::Gfnff {
            if self.uhf.unwrap_or(0) > 0 {
                return Err(ResponseError::new(
                    ErrorKind::Unsupported,
                    "GFN-FF requires a singlet",
                ));
            }
            let reference = self
                .gfnff
                .as_ref()
                .and_then(|g| g.reference_positions.as_deref());
            let reference = if let Some(reference) =
                reference.or(replacement.map(|_| self.positions.as_slice()))
            {
                validate_positions(reference, atoms)?;
                make_frame(
                    &self.numbers,
                    reference,
                    self.units,
                    self.charge,
                    multiplicity,
                )
                .hash()
            } else {
                frame.hash()
            };
            let fragment_charges = self
                .gfnff
                .as_ref()
                .and_then(|g| g.fragment_charges.clone())
                .unwrap_or_default();
            Some(GfnFfTopology {
                reference,
                fragment_charges,
            })
        } else if self.gfnff.is_some() {
            return Err(ResponseError::new(
                ErrorKind::Input,
                "only GFN-FF accepts a reference topology",
            ));
        } else {
            None
        };

        let settings =
            self.scf
                .xtb_settings(self.method, self.conventions, self.solvent.clone(), gfnff);
        let mut task = Task::Energy;
        let mut orbital_request = OrbitalRequest::None;
        if let Some(compute) = compute {
            if compute.hessian.is_some() {
                task = Task::Hessian;
            } else if compute.gradient {
                task = Task::Gradient;
            }
            orbital_request = match &compute.orbitals {
                OrbitalOutput::None => OrbitalRequest::None,
                OrbitalOutput::Levels => OrbitalRequest::Levels,
                OrbitalOutput::All => {
                    let n_ao = basis_functions.unwrap_or_default();
                    OrbitalRequest::Rows { n_ao, rows: n_ao }
                }
                OrbitalOutput::Rows(rows) => {
                    if let Some(width) = basis_functions
                        && (rows.windows(2).any(|pair| pair[0] >= pair[1])
                            || rows.last().is_some_and(|&row| row >= width))
                    {
                        return Err(ResponseError::new(
                            ErrorKind::Input,
                            "orbital rows must be strictly increasing and below the basis function count",
                        ));
                    }
                    OrbitalRequest::Rows {
                        n_ao: basis_functions.unwrap_or_default(),
                        rows: rows.len() as u32,
                    }
                }
            };
        }
        check_calculation(task, &settings, &frame, orbital_request).map_err(admission_error)?;
        if let Some(compute) = compute {
            if let Some(hessian) = &compute.hessian {
                if hessian.masses_amu.as_ref().is_some_and(|masses| {
                    masses.len() != atoms || masses.iter().any(|&m| !m.is_finite() || m <= 0.0)
                }) {
                    return Err(ResponseError::new(
                        ErrorKind::Input,
                        "expected one positive finite mass per atom",
                    ));
                }
                let thermo = &hessian.thermo;
                if hessian.masses_amu.is_some() && thermo.symmetry_number.is_none() {
                    return Err(ResponseError::new(
                        ErrorKind::Input,
                        "give thermo.symmetry_number with explicit masses; automatic symmetry ignores isotopes",
                    ));
                }
                if [
                    thermo.temperature_k,
                    thermo.pressure_pa,
                    thermo.frequency_scale,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite() || value <= 0.0)
                    || thermo.symmetry_number == Some(0)
                    || [thermo.rotor_cutoff_cm1, thermo.imaginary_cutoff_cm1]
                        .into_iter()
                        .flatten()
                        .any(|value| !value.is_finite())
                {
                    return Err(ResponseError::new(
                        ErrorKind::Input,
                        "thermochemistry temperature, pressure, and frequency scale must be positive and finite, symmetry number at least 1, and cutoffs finite",
                    ));
                }
            }
            if compute.bond_orders && !settings.method.capabilities().bond_orders {
                return Err(ResponseError::new(
                    ErrorKind::Unsupported,
                    "GFN-FF does not provide bond orders",
                ));
            }
        }
        Ok(frame)
    }

    #[cfg(any(feature = "engine", test))]
    pub(crate) fn heap_budget(&self) -> Result<usize, ResponseError> {
        usize::try_from(self.heap_bytes)
            .ok()
            .filter(|&bytes| bytes > 0)
            .ok_or_else(|| ResponseError::new(ErrorKind::Limit, "invalid heap budget"))
    }
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
fn validate_atom_count(atoms: usize, max_atoms: usize) -> Result<(), ResponseError> {
    if atoms == 0 {
        return Err(ResponseError::new(
            ErrorKind::Input,
            "expected at least one atom",
        ));
    }
    if atoms > max_atoms {
        return Err(ResponseError::new(
            ErrorKind::Limit,
            format!("expected at most {max_atoms} atoms"),
        ));
    }
    Ok(())
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
fn validate_positions(positions: &[[f64; 3]], atoms: usize) -> Result<(), ResponseError> {
    if positions.len() != atoms || positions.iter().flatten().any(|x| !x.is_finite()) {
        return Err(ResponseError::new(
            ErrorKind::Input,
            "expected one finite position per atom",
        ));
    }
    Ok(())
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
fn make_frame(
    numbers: &[u8],
    positions: &[[f64; 3]],
    units: Units,
    charge: i32,
    multiplicity: u32,
) -> Frame {
    let positions_angstrom = match units {
        Units::Angstrom => positions.to_vec(),
        Units::Bohr => positions
            .iter()
            .map(|p| p.map(|x| x * reshiki_qcdata::ANGSTROM_PER_BOHR))
            .collect(),
    };
    Frame {
        numbers: numbers.to_vec(),
        positions_angstrom,
        isotopes: vec![],
        masses_amu: vec![],
        charge,
        multiplicity,
        drawing_atoms: vec![],
    }
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
fn admission_error(error: Error) -> ResponseError {
    let kind = match error {
        Error::TooLarge { .. } => ErrorKind::Limit,
        Error::Invalid(_) => ErrorKind::Input,
        Error::Unsupported(_) => ErrorKind::Unsupported,
    };
    ResponseError::new(kind, error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compute {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positions: Option<Vec<[f64; 3]>>,
    pub units: Units,
    pub gradient: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hessian: Option<HessianRequest>,
    #[serde(default)]
    pub bond_orders: bool,
    #[serde(default)]
    pub orbitals: OrbitalOutput,
}

impl Default for Compute {
    fn default() -> Self {
        Self {
            positions: None,
            units: Units::Bohr,
            gradient: false,
            hessian: None,
            bond_orders: false,
            orbitals: OrbitalOutput::None,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HessianRequest {
    #[serde(default)]
    pub thermo: ThermoRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub masses_amu: Option<Vec<f64>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermoRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_k: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_pa: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotor_cutoff_cm1: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imaginary_cutoff_cm1: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symmetry_number: Option<u32>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbitalOutput {
    #[default]
    None,
    Levels,
    All,
    Rows(Vec<u32>),
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine"))]
impl OrbitalOutput {
    pub(crate) fn coefficient_rows(&self, basis_functions: u32) -> Vec<u32> {
        match self {
            Self::None | Self::Levels => vec![],
            Self::All => (0..basis_functions).collect(),
            Self::Rows(rows) => rows.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<T> {
    pub id: u64,
    #[serde(flatten)]
    pub result: Outcome<T>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Outcome<T> {
    Ok { ok: T },
    Error { error: ResponseError },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub protocol: u16,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub engine: Option<Engine>,
    pub methods: Vec<Method>,
    pub max_atoms: usize,
    pub max_hessian_atoms: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    pub name: String,
    pub version: String,
    pub license: String,
    pub source: String,
    pub rev: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opened {
    pub atoms: usize,
    pub method: Method,
    pub uhf: u32,
    pub conventions: Conventions,
    pub numerics: String,
    pub engine_conventions: String,
    pub basis_functions: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Computed {
    pub method: Method,
    pub conventions: Conventions,
    pub engine_conventions: String,
    pub numerics: String,
    pub settings: SessionSettings,
    pub energy: f64,
    pub energy_terms: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Vec<[f64; 3]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charges: Option<AtomicCharges>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dipole_e_bohr: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quadrupole_e_bohr2: Option<[f64; 6]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_orders: Option<Vec<BondOrder>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "wire_orbitals"
    )]
    pub orbitals: Option<Orbitals>,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub homo_lumo_gap_ev: Option<f64>,
    pub near_degenerate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scf: Option<ScfReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite: Option<CompositeReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gfnff: Option<GfnFfReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hessian_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vibrations: Option<VibrationsReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermo: Option<Thermo>,
    pub arrays: Vec<ArrayHeader>,
    pub seconds: f64,
    #[serde(skip)]
    pub hessian: Vec<f64>,
    #[serde(skip)]
    pub dipole_derivatives: Vec<f64>,
    #[serde(skip)]
    pub normal_modes: Vec<f64>,
}

impl Computed {
    /// Values carried by the binary frames for an announced array.
    pub fn array_values(&self, name: ArrayName) -> &[f64] {
        match name {
            ArrayName::Hessian => &self.hessian,
            ArrayName::DipoleDerivatives => &self.dipole_derivatives,
            ArrayName::NormalModes => &self.normal_modes,
            ArrayName::CoefficientsAlpha => self
                .orbitals
                .as_ref()
                .map_or(&[], |orbitals| &orbitals.alpha.coefficients),
            ArrayName::CoefficientsBeta => {
                match self.orbitals.as_ref().map(|orbitals| &orbitals.beta) {
                    Some(Beta::Unrestricted(beta)) => &beta.coefficients,
                    _ => &[],
                }
            }
        }
    }

    /// Convert a complete response into a validated scientific result.
    pub fn to_calc_result(
        &self,
        frame: &Frame,
        task: Task,
        engine: &Engine,
        gfnff_reference: Option<FrameHash>,
    ) -> Result<CalcResult, Error> {
        if frame.charge != self.settings.charge {
            return Err(Error::Invalid(
                "frame charge differs from the xTB session".into(),
            ));
        }
        if self.settings.uhf.checked_add(1) != Some(frame.multiplicity) {
            return Err(Error::Unsupported(format!(
                "the xTB valence spin (uhf {}) differs from the frame multiplicity {}; this cannot be stored yet",
                self.settings.uhf, frame.multiplicity
            )));
        }
        if let Some(vibrations) = &self.vibrations {
            if frame.masses_amu.is_empty() {
                if frame.isotopes.iter().any(|&isotope| isotope != 0) {
                    return Err(Error::Unsupported(
                        "isotopes need explicit masses for the vibrational analysis".into(),
                    ));
                }
            } else if frame.masses_amu != vibrations.masses_amu {
                return Err(Error::Invalid(
                    "frame masses differ from the vibrational analysis masses".into(),
                ));
            }
        }
        let gfnff = if self.method == Method::Gfnff || self.settings.gfnff.is_some() {
            let reference = gfnff_reference.ok_or_else(|| {
                Error::Invalid("GFN-FF results require a reference frame hash".into())
            })?;
            self.settings.gfnff.as_ref().map(|gfnff| GfnFfTopology {
                reference,
                fragment_charges: gfnff.fragment_charges.clone(),
            })
        } else {
            None
        };
        let mut settings = self.settings.scf.xtb_settings(
            self.method,
            self.conventions,
            self.settings.solvent.clone(),
            gfnff,
        );
        settings.threads = Some(self.settings.threads);
        // A converged retry rung (outside the solvent composite, whose parts
        // climb their own ladders) is what produced the result: save it, not
        // the ladder, so recomputing reproduces these numbers.
        if let Some((scf, rung)) = self.scf.as_ref().and_then(|scf| {
            let rung = scf.retry.as_ref()?.rung.checked_sub(1)?;
            Some((scf, self.settings.scf.retries.get(rung)?))
        }) && self.composite.is_none()
        {
            settings.electronic_temperature_k = Some(scf.electronic_temperature_k);
            settings.max_iterations = rung.max_iterations.or(settings.max_iterations);
            settings.scf_retries.clear();
        }
        let mut energy_terms = self.energy_terms.clone();
        if let Some(composite) = &self.composite {
            energy_terms.extend(
                composite
                    .components
                    .iter()
                    .map(|(name, &energy)| (format!("composite.{name}"), energy)),
            );
        }
        let result = CalcResult {
            frame: frame.hash(),
            input_frame: None,
            task,
            endpoints: None,
            settings: Settings::Xtb(settings),
            provenance: Provenance {
                program: "axtb".into(),
                program_version: Some(engine.version.clone()),
                method: Some(self.method.xtb_method().label().into()),
                basis: None,
                source: Source::Calculated {
                    engine_revision: engine.rev.clone(),
                },
            },
            status: Status::Converged,
            energy_hartree: Some(self.energy),
            energy_terms,
            gradient_hartree_per_bohr: self.gradient.iter().flatten().flatten().copied().collect(),
            hessian_hartree_per_bohr2: self.hessian.clone(),
            charges: self.charges.iter().cloned().collect(),
            dipole_e_bohr: self.dipole_e_bohr,
            bond_orders: self.bond_orders.clone().unwrap_or_default(),
            orbitals: self.orbitals.clone(),
            vibrations: self.vibrations.as_ref().map(|v| Vibrations {
                frequencies_cm1: v.frequencies_cm1.clone(),
                ir_intensities_km_mol: v.ir_intensities_km_mol.clone().unwrap_or_default(),
                modes: self.normal_modes.clone(),
                reduced_masses_amu: v.reduced_masses_amu.clone(),
                masses_amu: v.masses_amu.clone(),
            }),
            thermo: self.thermo.clone(),
            trajectory: None,
            keep_full: false,
            omitted_bytes: None,
            run: None,
        };
        result.validate(frame)?;
        Ok(result)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSettings {
    pub charge: i32,
    pub uhf: u32,
    pub solvent: Option<Solvent>,
    pub scf: ScfSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gfnff: Option<GfnFfSettings>,
    pub threads: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GfnFfSettings {
    pub reference_positions_bohr: Vec<[f64; 3]>,
    pub fragment_charges: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScfReport {
    pub iterations: usize,
    pub start: String,
    pub electronic_temperature_k: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryReport>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryReport {
    pub rung: usize,
    pub of: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeReport {
    pub components: BTreeMap<String, f64>,
    pub parts: BTreeMap<String, CompositePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hessian_parts: Option<BTreeMap<String, CompositePart>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositePart {
    pub method: String,
    pub conventions: String,
    pub electronic_temperature_k: f64,
    pub max_iterations: usize,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub iterations: Option<usize>,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub retry: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GfnFfReport {
    pub fragments: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VibrationsReport {
    pub conventions: String,
    pub frequencies_cm1: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ir_intensities_km_mol: Option<Vec<f64>>,
    pub reduced_masses_amu: Vec<f64>,
    pub force_constants_mdyn_per_angstrom: Vec<f64>,
    pub masses_amu: Vec<f64>,
    pub linear: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrayHeader {
    pub name: ArrayName,
    pub shape: [u32; 2],
    pub chunks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrayName {
    Hessian,
    DipoleDerivatives,
    NormalModes,
    CoefficientsAlpha,
    CoefficientsBeta,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Closed {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseError {
    pub kind: ErrorKind,
    pub message: String,
}

impl ResponseError {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    #[error("input")]
    Input,
    #[error("unsupported")]
    Unsupported,
    #[error("unsupported_element")]
    UnsupportedElement,
    #[error("topology")]
    Topology,
    #[error("scf")]
    Scf,
    #[error("numerical")]
    Numerical,
    #[error("limit")]
    Limit,
    #[error("state")]
    State,
    #[error("unavailable")]
    Unavailable,
    #[error("protocol")]
    Protocol,
}

mod wire_orbitals;

#[cfg(test)]
mod tests;
