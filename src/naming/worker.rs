//! One bounded native naming request, before GUI/runtime startup. The
//! browser's naming Web Worker answers the same requests with `respond`.
use super::openclatura;
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    io::{self, Read, Write},
    path::PathBuf,
};

pub(super) const PROTOCOL: u32 = 6;
pub(super) const WIRE_LIMIT: usize = 32_768;
pub(super) const OUTPUT_LIMIT: usize = 65_536;
pub(super) const MAX_NAME_BYTES: usize = 2_048;
pub const HEAP_BYTES: usize = 96 * 1024 * 1024;
pub(super) const BACKEND: &str = "opsin-rust";
pub(super) const PORT_VERSION: &str = nazuchem_n2s::PORT_VERSION;
pub(super) const UPSTREAM_VERSION: &str = nazuchem_n2s::UPSTREAM_VERSION;
pub(super) const UPSTREAM_COMMIT: &str = nazuchem_n2s::UPSTREAM_COMMIT;
pub(super) const RESOURCE_FINGERPRINT: &str = nazuchem_n2s::RESOURCE_FINGERPRINT;
pub(super) const OPTIONS: &str = "strict-semantic-cx1";
pub(super) const GENERATION_OPTIONS: &str = "generation-radicals-semantic-cx1";

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    Parse {
        protocol: u32,
        name: String,
    },
    Verify {
        protocol: u32,
        name: String,
    },
    Generate {
        protocol: u32,
        profile: String,
        smiles: String,
        isotope_style: super::IsotopeStyle,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum ParserStatus {
    Success,
    Warning,
    Failure,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ParserWarning {
    pub kind: String,
    pub message: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parsed {
    pub protocol: u32,
    pub backend: String,
    pub port_version: String,
    pub upstream_version: String,
    pub upstream_commit: String,
    pub resource_fingerprint: String,
    pub options: String,
    pub name: String,
    pub status: ParserStatus,
    pub message: String,
    pub warnings: Vec<ParserWarning>,
    pub cxsmiles: Option<String>,
    pub locants: Vec<Option<String>>,
    pub axes: Vec<Axis>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Axis {
    pub atoms: [usize; 2],
    pub refs: [usize; 2],
    pub clockwise: bool,
    pub descriptor: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Generated {
    pub protocol: u32,
    pub profile: String,
    pub input: String,
    pub result: Result<openclatura::Name, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Response {
    Parse(Parsed),
    Generate(Generated),
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn executable() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("RESHIKI_NAMING_HELPER") {
        let path = PathBuf::from(path);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err(
            "RESHIKI_NAMING_HELPER must be an absolute path to the ReShiki executable".into(),
        );
    }
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    if current
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "deps")
    {
        return Err("Naming integration tests require RESHIKI_NAMING_HELPER pointing to this exact source's built ReShiki executable".into());
    }
    Ok(current)
}

pub(super) fn check_name(name: &str, error: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(error.into());
    }
    Ok(())
}

fn parse(name: String, generation: bool) -> Result<Response, String> {
    check_name(
        &name,
        "Enter a chemical name of at most 2048 bytes without control characters",
    )?;
    // Grammar initialization is inside the bounded native allocator, never a foreign runtime.
    let parser = nazuchem_n2s::Parser::with_budget(crate::naming::kernel_budget())
        .map_err(|error| error.to_string())?;
    let mut options = nazuchem_n2s::ParseOptions::strict();
    options.allow_radicals = generation;
    let result = parser.parse(&name, &options);
    let status = match result.status {
        nazuchem_n2s::Status::Success => ParserStatus::Success,
        nazuchem_n2s::Status::Warning => ParserStatus::Warning,
        nazuchem_n2s::Status::Failure => ParserStatus::Failure,
    };
    let warnings: Vec<ParserWarning> = result
        .warnings
        .into_iter()
        .map(|warning| ParserWarning {
            kind: match warning.kind {
                nazuchem_n2s::WarningKind::AppearsAmbiguous => "APPEARS_AMBIGUOUS",
                nazuchem_n2s::WarningKind::StereochemistryIgnored => "STEREOCHEMISTRY_IGNORED",
            }
            .into(),
            message: warning.message,
        })
        .collect();
    let serialized = result
        .structure
        .map(|structure| structure.semantic_cxsmiles_with_locants_and_axes())
        .transpose();
    let (status, message, cxsmiles, locants, axes) = match serialized {
        Ok(Some((cxsmiles, locants, axes))) => {
            (status, result.message, Some(cxsmiles), locants, axes)
        }
        Ok(None) => (status, result.message, None, vec![], vec![]),
        Err(error) => (
            ParserStatus::Failure,
            format!("{} Structure serialization failed: {error}", result.message),
            None,
            vec![],
            vec![],
        ),
    };
    Ok(Response::Parse(Parsed {
        protocol: PROTOCOL,
        backend: BACKEND.into(),
        port_version: PORT_VERSION.into(),
        upstream_version: UPSTREAM_VERSION.into(),
        upstream_commit: UPSTREAM_COMMIT.into(),
        resource_fingerprint: RESOURCE_FINGERPRINT.into(),
        options: if generation {
            GENERATION_OPTIONS
        } else {
            OPTIONS
        }
        .into(),
        name: result.input,
        status,
        message,
        warnings,
        cxsmiles,
        locants,
        axes: axes
            .into_iter()
            .map(|axis| Axis {
                atoms: axis.atoms,
                refs: axis.refs,
                clockwise: axis.clockwise,
                descriptor: axis.descriptor,
            })
            .collect(),
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn operation() -> Result<Vec<u8>, String> {
    io::stderr().flush().map_err(|e| e.to_string())?;
    reshiki_process_heap::begin(HEAP_BYTES);
    let mut bytes = vec![];
    io::stdin()
        .take((WIRE_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    respond(&bytes)
}

/// The serialized response to one serialized request, within the wire limits.
pub fn respond(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() > WIRE_LIMIT {
        return Err("Local naming request exceeds its wire limit".into());
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let response = match request {
        Request::Parse { protocol, name } if protocol == PROTOCOL => parse(name, false),
        Request::Verify { protocol, name } if protocol == PROTOCOL => parse(name, true),
        Request::Generate {
            protocol,
            profile,
            smiles,
            isotope_style,
        } if protocol == PROTOCOL && profile == openclatura::PROFILE => {
            let result = openclatura::name_smiles_with_style(
                &smiles,
                isotope_style,
                &crate::naming::kernel_budget(),
            )
            .and_then(|name| {
                check_name(&name.text, "the name exceeds the supported length")?;
                Ok(name)
            })
            .map_err(|reason| format!("This structure can't be named locally yet: {reason}"));
            Ok(Response::Generate(Generated {
                protocol: PROTOCOL,
                profile: openclatura::PROFILE.into(),
                input: smiles,
                result,
            }))
        }
        _ => Err("Incompatible native naming protocol/profile".into()),
    }?;
    let bytes = serde_json::to_vec(&response).map_err(|e| e.to_string())?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err("Local naming response exceeds its wire limit".into());
    }
    Ok(bytes)
}
/// The executable installs BoundedHeap and enters before graphics or Tokio.
#[cfg(not(target_arch = "wasm32"))]
pub fn run() {
    match operation() {
        Ok(bytes) => {
            if io::stdout().write_all(&bytes).is_err() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
