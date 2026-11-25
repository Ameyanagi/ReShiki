//! Version 1 of the ReShiki xTB worker wire format.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

pub const VERSION: u16 = 1;
pub const MAX_ATOMS: usize = 500;
pub const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
pub const HEADER_LEN: usize = 16;
/// CODATA 2022 Hartree energy in eV.
pub const HARTREE_TO_EV: f64 = 27.211386245981;
pub const NEAR_DEGENERATE_GAP_EV: f64 = 0.1;
const MAGIC: &[u8; 8] = b"RSHXTBWK";

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
pub(crate) fn is_near_degenerate(gap_ev: Option<f64>) -> bool {
    gap_ev.is_some_and(|gap| gap < NEAR_DEGENERATE_GAP_EV)
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("invalid xTB frame magic")]
    Magic,
    #[error("unsupported xTB protocol version")]
    Version,
    #[error("unsupported xTB frame kind")]
    Kind,
    #[error("xTB JSON frame exceeds 4 MiB")]
    Length,
    #[error("reading or writing xTB frame: {0}")]
    Io(#[from] io::Error),
    #[error("invalid xTB JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn decode_header(header: &[u8; HEADER_LEN]) -> Result<usize, FrameError> {
    if &header[..8] != MAGIC {
        return Err(FrameError::Magic);
    }
    if u16::from_le_bytes([header[8], header[9]]) != VERSION {
        return Err(FrameError::Version);
    }
    if u16::from_le_bytes([header[10], header[11]]) != 0 {
        return Err(FrameError::Kind);
    }
    let size = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;
    if size > MAX_JSON_BYTES {
        return Err(FrameError::Length);
    }
    Ok(size)
}

/// A clean EOF is possible only before the first byte of the header.
pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, FrameError> {
    let mut header = [0; HEADER_LEN];
    match reader.read_exact(&mut header[..1]) {
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        result => result?,
    }
    reader.read_exact(&mut header[1..])?;
    let mut body = vec![0; decode_header(&header)?];
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
    let length = (frame.0.len() - HEADER_LEN) as u32;
    frame.0[..8].copy_from_slice(MAGIC);
    frame.0[8..10].copy_from_slice(&VERSION.to_le_bytes());
    frame.0[12..HEADER_LEN].copy_from_slice(&length.to_le_bytes());
    Ok(frame.0)
}

struct FrameBuffer(Vec<u8>);

impl Write for FrameBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > HEADER_LEN + MAX_JSON_BYTES - self.0.len() {
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
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Conventions {
    #[default]
    Axtb,
    #[serde(rename = "xtb-6.8")]
    Xtb68,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Energy,
    Gradient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SolventModel {
    Alpb,
    Gbsa,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Solvent {
    pub model: SolventModel,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "one_thread")]
    pub threads: u32,
    pub heap_bytes: u64,
}

fn one_thread() -> u32 {
    1
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
impl Open {
    pub(crate) fn validate_geometry_and_threads(
        &self,
        max_atoms: usize,
    ) -> Result<(), ResponseError> {
        validate_atom_count(self.numbers.len(), max_atoms)?;
        validate_positions(&self.positions, self.numbers.len())?;
        if !(1..=64).contains(&self.threads) {
            return Err(ResponseError::new(
                ErrorKind::Input,
                "expected 1 to 64 threads",
            ));
        }
        Ok(())
    }

    #[cfg(any(feature = "engine", test))]
    pub(crate) fn validate(&self) -> Result<usize, ResponseError> {
        self.validate_geometry_and_threads(MAX_ATOMS)?;
        usize::try_from(self.heap_bytes)
            .ok()
            .filter(|&bytes| bytes > 0)
            .ok_or_else(|| ResponseError::new(ErrorKind::Limit, "invalid heap budget"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compute {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positions: Option<Vec<[f64; 3]>>,
    pub units: Units,
    pub gradient: bool,
}

#[cfg(any(not(target_arch = "wasm32"), feature = "engine", test))]
pub(crate) fn validate_atom_count(atoms: usize, max_atoms: usize) -> Result<(), ResponseError> {
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
pub(crate) fn validate_positions(
    positions: &[[f64; 3]],
    atoms: usize,
) -> Result<(), ResponseError> {
    if positions.len() != atoms || positions.iter().flatten().any(|x| !x.is_finite()) {
        return Err(ResponseError::new(
            ErrorKind::Input,
            "expected one finite position per atom",
        ));
    }
    Ok(())
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
    pub operations: Vec<Operation>,
    pub max_atoms: usize,
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
    pub conventions: Conventions,
    pub numerics: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Computed {
    pub method: Method,
    pub conventions: Conventions,
    pub numerics: String,
    pub energy: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Vec<[f64; 3]>>,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub homo_lumo_gap_ev: Option<f64>,
    pub near_degenerate: bool,
    pub scf_iterations: usize,
    pub seconds: f64,
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

#[cfg(test)]
mod tests;
