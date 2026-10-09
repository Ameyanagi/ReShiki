//! One bounded native naming request, before GUI/runtime startup.
use super::rules;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
};

pub(super) const PROTOCOL: u32 = 2;
pub(super) const WIRE_LIMIT: usize = 32_768;
pub(super) const OUTPUT_LIMIT: usize = 65_536;
pub(super) const HEAP_BYTES: usize = 96 * 1024 * 1024;
pub(super) const BACKEND: &str = "opsin-rust";
pub(super) const PORT_VERSION: &str = opsin::PORT_VERSION;
pub(super) const UPSTREAM_VERSION: &str = opsin::UPSTREAM_VERSION;
pub(super) const UPSTREAM_COMMIT: &str = opsin::UPSTREAM_COMMIT;
pub(super) const RESOURCE_FINGERPRINT: &str = opsin::RESOURCE_FINGERPRINT;
pub(super) const OPTIONS: &str = "strict-semantic-cx1";

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    Parse {
        protocol: u32,
        name: String,
    },
    Generate {
        protocol: u32,
        profile: String,
        smiles: String,
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
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Generated {
    pub protocol: u32,
    pub profile: String,
    pub input: String,
    pub result: Result<String, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Response {
    Parse(Parsed),
    Generate(Generated),
}

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

fn parse(name: String) -> Result<Response, String> {
    if name.is_empty() || name.len() > 2_048 || name.chars().any(char::is_control) {
        return Err(
            "Enter a chemical name of at most 2048 bytes without control characters".into(),
        );
    }
    // Grammar initialization is inside the bounded native allocator, never a foreign runtime.
    let parser = opsin::Parser::new().map_err(|error| error.to_string())?;
    let result = parser.parse(&name, &opsin::ParseOptions::strict());
    let status = match result.status {
        opsin::Status::Success => ParserStatus::Success,
        opsin::Status::Warning => ParserStatus::Warning,
        opsin::Status::Failure => ParserStatus::Failure,
    };
    let warnings: Vec<ParserWarning> = result
        .warnings
        .into_iter()
        .map(|warning| ParserWarning {
            kind: match warning.kind {
                opsin::WarningKind::AppearsAmbiguous => "APPEARS_AMBIGUOUS",
                opsin::WarningKind::StereochemistryIgnored => "STEREOCHEMISTRY_IGNORED",
            }
            .into(),
            message: warning.message,
        })
        .collect();
    let serialized = result
        .structure
        .map(|structure| structure.semantic_cxsmiles())
        .transpose();
    let (status, message, cxsmiles) = match serialized {
        Ok(cxsmiles) => (status, result.message, cxsmiles),
        Err(error) => (
            ParserStatus::Failure,
            format!("{} Structure serialization failed: {error}", result.message),
            None,
        ),
    };
    Ok(Response::Parse(Parsed {
        protocol: PROTOCOL,
        backend: BACKEND.into(),
        port_version: PORT_VERSION.into(),
        upstream_version: UPSTREAM_VERSION.into(),
        upstream_commit: UPSTREAM_COMMIT.into(),
        resource_fingerprint: RESOURCE_FINGERPRINT.into(),
        options: OPTIONS.into(),
        name: result.input,
        status,
        message,
        warnings,
        cxsmiles,
    }))
}

fn operation() -> Result<Response, String> {
    io::stderr().flush().map_err(|e| e.to_string())?;
    reshiki_process_heap::begin(HEAP_BYTES);
    let mut bytes = vec![];
    io::stdin()
        .take((WIRE_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > WIRE_LIMIT {
        return Err("Local naming request exceeds its wire limit".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    match request {
        Request::Parse { protocol, name } if protocol == PROTOCOL => parse(name),
        Request::Generate {
            protocol,
            profile,
            smiles,
        } if protocol == PROTOCOL && profile == rules::PROFILE => {
            Ok(Response::Generate(Generated {
                protocol: PROTOCOL,
                profile: rules::PROFILE.into(),
                input: smiles.clone(),
                result: rules::generate(&smiles),
            }))
        }
        _ => Err("Incompatible native naming protocol/profile".into()),
    }
}
/// The executable installs BoundedHeap and enters before graphics or Tokio.
pub fn run() {
    match operation().and_then(|response| serde_json::to_vec(&response).map_err(|e| e.to_string()))
    {
        Ok(bytes) if bytes.len() <= OUTPUT_LIMIT => {
            if io::stdout().write_all(&bytes).is_err() {
                std::process::exit(1);
            }
        }
        Ok(_) => {
            eprintln!("Local naming response exceeds its wire limit");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
