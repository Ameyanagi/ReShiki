//! Native Rust OPSIN adapter, isolated in the bounded same-executable worker.
//! The browser sends the same request to its naming Web Worker instead.
#[cfg(not(target_arch = "wasm32"))]
use super::client::{self, CancelOnDrop, POLL};
use super::{Error, Locants, Provenance, Record, worker};
use std::borrow::Cow;
#[cfg(not(target_arch = "wasm32"))]
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const OUTPUT_LIMIT: usize = worker::OUTPUT_LIMIT;
#[cfg(not(target_arch = "wasm32"))]
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
#[cfg(not(target_arch = "wasm32"))]
impl Cancel {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(super) fn stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(super) async fn cancelled(&self) {
        while !self.stopped() {
            tokio::time::sleep(POLL).await;
        }
    }
}

/// One deterministic local parse. Dropping the future cancels the supervised
/// worker, which kills and waits for its child before releasing its directory.
#[cfg(not(target_arch = "wasm32"))]
pub async fn resolve_name(name: &str, cancel: Cancel) -> Result<Record, Error> {
    resolve_with(name, cancel, &SLOTS, run).await
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn resolve_generated(name: &str, cancel: Cancel) -> Result<Record, Error> {
    resolve_with(name, cancel, &SLOTS, |name, cancel| {
        let request = verification_request(name)?;
        let output =
            client::run_worker("reshiki-verify-name-", &request, cancel, worker::executable)?;
        verification_result(name, &output)
    })
    .await
}

#[cfg(not(target_arch = "wasm32"))]
async fn resolve_with(
    name: &str,
    cancel: Cancel,
    slots: &'static tokio::sync::Semaphore,
    run: impl FnOnce(&str, &Cancel) -> Result<Record, Error> + Send + 'static,
) -> Result<Record, Error> {
    let name = checked_name(name)?;
    let stop = CancelOnDrop(cancel.clone());
    let result = client::on_slot(
        name,
        &cancel,
        slots,
        "Local parser is unavailable",
        "Local name parsing was cancelled",
        "Local parser worker failed",
        run,
    )
    .await;
    drop(stop);
    result
}

/// The trimmed name, if the parser accepts its length and characters.
pub(super) fn checked_name(name: &str) -> Result<&str, Error> {
    const MESSAGE: &str = "Enter a chemical name of at most 2048 bytes without control characters";
    if name.len() > worker::MAX_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(Error::Rejected(MESSAGE.into()));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Rejected(MESSAGE.into()));
    }
    Ok(name)
}

#[cfg(not(target_arch = "wasm32"))]
fn run(name: &str, cancel: &Cancel) -> Result<Record, Error> {
    run_with_executable(name, cancel, &worker::executable().map_err(Error::Worker)?)
}
#[cfg(not(target_arch = "wasm32"))]
fn run_with_executable(name: &str, cancel: &Cancel, executable: &Path) -> Result<Record, Error> {
    if cancel.stopped() {
        return Err(Error::Cancelled("Local name parsing was cancelled".into()));
    }
    let request = parse_request(name)?;
    let output = client::run_worker("reshiki-native-name-", &request, cancel, || Ok(executable))?;
    parse_result(name, &output)
}

/// The worker request that parses `name`.
pub(super) fn parse_request(name: &str) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(&worker::Request::Parse {
        protocol: worker::PROTOCOL,
        name: parser_name(name)?.into_owned(),
    })
    .map_err(|error| Error::Worker(error.to_string()))
}

pub(super) fn verification_request(name: &str) -> Result<Vec<u8>, Error> {
    checked_name(name)?;
    serde_json::to_vec(&worker::Request::Verify {
        protocol: worker::PROTOCOL,
        name: name.into(),
    })
    .map_err(|error| Error::Worker(error.to_string()))
}

pub(super) fn verification_result(name: &str, output: &[u8]) -> Result<Record, Error> {
    parse_reply_with_mode(output, name, true)
}

/// The record for the worker's reply to `parse_request(name)`.
pub(super) fn parse_result(name: &str, output: &[u8]) -> Result<Record, Error> {
    certify_name(name, parse_reply(output, &parser_name(name)?)?)
}

// A curated retained-name equivalence, not a rule from optical rotation to R/S.
// CHEBI:422 explicitly lists (+)-lactic acid and its (2S) graph:
// https://www.ebi.ac.uk/chebi/CHEBI:422 (entry modified 2024-04-18).
// Only this complete name is mapped to S; other rotation signs never assign CIP.
const PLUS_LACTIC_NAME: &str = "(+)-lactic acid";
const PLUS_LACTIC_PARSER_NAME: &str = "(2S)-2-hydroxypropanoic acid";
const PLUS_LACTIC_GRAPH: &str = "C[C@H](O)C(=O)O";

/// The text OPSIN parses: the retained-name mapping, an expansion of
/// biochemical symbol notation, or the name itself.
fn parser_name(name: &str) -> Result<Cow<'_, str>, Error> {
    if name.eq_ignore_ascii_case(PLUS_LACTIC_NAME) {
        return Ok(Cow::Borrowed(PLUS_LACTIC_PARSER_NAME));
    }
    Ok(super::symbols::expand(name)?.map_or(Cow::Borrowed(name), |e| Cow::Owned(e.name)))
}

fn certify_name(name: &str, mut record: Record) -> Result<Record, Error> {
    if !name.eq_ignore_ascii_case(PLUS_LACTIC_NAME) {
        if let Some(expansion) = super::symbols::expand(name)? {
            record.title = name.into();
            record.provenance = Provenance::OpsinRustSymbols {
                name: expansion.name,
            };
            record.warnings.extend(expansion.warnings);
        }
        return Ok(record);
    }
    // The strict adapter already checked warnings, semantic layers, provenance,
    // bounds and the interpreted input. Independently require the specific S
    // graph before exposing the user name to preview, insertion or captions.
    const MESSAGE: &str =
        "The retained (+)-lactic acid name did not resolve to its verified (S) structure";
    super::verify_identity(PLUS_LACTIC_GRAPH, &record.smiles)
        .map_err(|error| Error::Rejected(format!("{MESSAGE}: {error}")))?;
    record.title = name.into();
    record.provenance = Provenance::OpsinRustRetainedName {
        source: "ChEBI:422",
    };
    Ok(record)
}

fn parse_reply(bytes: &[u8], name: &str) -> Result<Record, Error> {
    parse_reply_with_mode(bytes, name, false)
}

fn parse_reply_with_mode(bytes: &[u8], name: &str, generation: bool) -> Result<Record, Error> {
    if bytes.len() > OUTPUT_LIMIT {
        return Err(Error::Worker(
            "Local parser response exceeds the supported limit".into(),
        ));
    }
    let response: worker::Response = serde_json::from_slice(bytes).map_err(|_| {
        Error::Worker("Local parser returned an invalid structured response".into())
    })?;
    let worker::Response::Parse(reply) = response else {
        return Err(Error::Worker(
            "Local parser returned a result for a different operation".into(),
        ));
    };
    if reply.protocol != worker::PROTOCOL
        || reply.backend != worker::BACKEND
        || reply.port_version != worker::PORT_VERSION
        || reply.upstream_version != worker::UPSTREAM_VERSION
        || reply.upstream_commit != worker::UPSTREAM_COMMIT
        || reply.resource_fingerprint != worker::RESOURCE_FINGERPRINT
        || reply.options
            != if generation {
                worker::GENERATION_OPTIONS
            } else {
                worker::OPTIONS
            }
        || reply.name != name
    {
        return Err(Error::Worker(
            "Local parser result has inconsistent version, options or input identity".into(),
        ));
    }
    const ROTATION_PREFIX: &str = "Optical rotation cannot be algorithmically used to assign stereochemistry. This term was ignored: ";
    // Optical-rotation terms are the only warnings a name may carry; they are
    // checked against CIP descriptors and complete stereo below.
    let signs: Option<Vec<_>> = reply
        .warnings
        .iter()
        .map(|w| {
            (w.kind == "STEREOCHEMISTRY_IGNORED")
                .then(|| w.message.strip_prefix(ROTATION_PREFIX))
                .flatten()
                .map(|sign| sign.trim_end_matches('-'))
        })
        .collect();
    let signs = signs.filter(|signs| !signs.is_empty());
    if matches!(reply.status, worker::ParserStatus::Failure)
        || (signs.is_none()
            && (!matches!(reply.status, worker::ParserStatus::Success)
                || !reply.warnings.is_empty()))
    {
        const MESSAGE: &str = "The OPSIN Rust port could not interpret this name unambiguously with complete stereochemistry";
        let details = reply
            .warnings
            .iter()
            .take(16)
            .map(|w| format!("{}: {}", w.kind, w.message))
            .chain(std::iter::once(reply.message))
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(Error::Rejected(format!(
            "{MESSAGE}: {}",
            details.chars().take(2048).collect::<String>()
        )));
    }
    let molecule = reply.molecule.ok_or_else(|| {
        Error::Rejected("The OPSIN Rust port returned no molecular structure".into())
    })?;
    const SEMANTICS: &str = "ReShiki drawings cannot yet store relative configuration (rel, rac, or endo/exo/syn/anti on a chiral ring system), polymer or atom-label semantics. No simplified structure or single enantiomer was drawn";
    // Verification alone accepts the single OR group used by endo/exo names.
    // Import retains its existing fail-closed policy for relative configurations.
    let relative = match molecule.stereo_groups.as_slice() {
        [] => None,
        [group]
            if generation
                && matches!(group.kind, nazuchem_ir::StereoGroupKind::Or(_))
                && group.stereo.iter().all(|&index| {
                    matches!(
                        molecule.stereo.get(index as usize),
                        Some(nazuchem_ir::Stereo::Tetrahedral { .. })
                    )
                }) =>
        {
            Some(group)
        }
        _ => return Err(Error::Rejected(SEMANTICS.into())),
    };
    let prepared = crate::molecule_names::from_ir(&molecule).map_err(Error::Rejected)?;
    let (prepared, ir_atoms) =
        crate::molecule_names::name_import_projection(&prepared).map_err(Error::Rejected)?;
    if prepared.state.graph.atoms.len() > crate::molecule_names::MAX_NAMING_ATOMS {
        return Err(Error::Rejected(
            "Naming supports at most 512 input atom tokens; no oversized graph was imported".into(),
        ));
    }
    let mut identity_state = prepared.state.clone();
    identity_state.metadata.groups.clear();
    let canonical_smiles = super::canonical_state(&identity_state).map_err(Error::Rejected)?;
    if !generation
        && prepared
            .state
            .graph
            .atoms
            .iter()
            .any(|atom| atom.radical_electrons != 0)
    {
        return Err(Error::Rejected(
            "Importing radical names is not supported by the strict parser".into(),
        ));
    }
    // SMILES remains an editable display/export view. Chemistry and layout use
    // the prepared IR graph, and the writer's traversal maps display locants.
    let display_state = crate::molecule_names::encode_axis_markers(&identity_state)
        .map_err(|_| Error::Rejected("Native import could not encode the biaryl axis".into()))?;
    let written = crate::chemistry::smiles::write::write(
        &display_state,
        crate::chemistry::smiles::write::Options {
            canonical: false,
            ..Default::default()
        },
    )
    .map_err(|error| Error::Rejected(error.to_string()))?;
    let mut smiles = written.text;
    if let Some(group) = relative {
        let nazuchem_ir::StereoGroupKind::Or(number) = group.kind else {
            return Err(Error::Rejected(SEMANTICS.into()));
        };
        let mut centers = group
            .stereo
            .iter()
            .filter_map(|&index| match molecule.stereo.get(index as usize) {
                Some(nazuchem_ir::Stereo::Tetrahedral { center, .. }) => written
                    .atom_order
                    .iter()
                    .position(|&atom| ir_atoms.get(atom) == Some(&(*center as usize))),
                _ => None,
            })
            .collect::<Vec<_>>();
        centers.sort_unstable();
        smiles.push_str(&format!(
            " |o{number}:{}|",
            centers
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    let projected = molecule.displayed_locants();
    // Locants compare constitution against the drawing. Private axial maps are
    // only for isomeric identity and must not enter this unmarked source view.
    let locant_source = crate::chemistry::smiles::write::write(
        &identity_state,
        crate::chemistry::smiles::write::Options {
            canonical: false,
            ..Default::default()
        },
    )
    .map_err(|error| Error::Rejected(error.to_string()))?;
    let locants = Locants {
        smiles: if relative.is_some() {
            smiles.clone()
        } else {
            locant_source.text
        },
        atoms: locant_source
            .atom_order
            .iter()
            .map(|&atom| {
                ir_atoms
                    .get(atom)
                    .and_then(|&atom| projected.get(atom))
                    .cloned()
                    .flatten()
            })
            .collect(),
    };
    let mut warnings = Vec::new();
    if let Some(signs) = signs {
        let descriptors = cip_descriptors(name);
        if descriptors.is_empty()
            || crate::chemistry::stereo::perception::has_unspecified_stereo(&identity_state)
                .map_err(Error::Rejected)?
        {
            return Err(Error::Rejected("A rotation sign is accepted only beside R/S descriptors that specify every stereocentre and double bond; the sign itself is never used to assign configuration".into()));
        }
        for sign in signs {
            warnings.push(format!(
                "Rotation sign {sign} ignored; structure from {}",
                descriptors.join(", ")
            ));
        }
    }
    // Stable sorting moves positive components to the front without changing
    // the parser's atom order (and therefore stereo) within each component.
    // Relative CX groups must retain the parser's atom indices.
    let smiles = if relative.is_none() && smiles.contains('.') {
        let mut offset = 0;
        let mut components = smiles
            .split('.')
            .map(|part| {
                let first = written.atom_order.get(offset).copied();
                let members = first
                    .map(|first| component_atoms(&prepared.state, first))
                    .unwrap_or_default();
                let count = members.len();
                let charge: i32 = written
                    .atom_order
                    .iter()
                    .skip(offset)
                    .take(count)
                    .filter_map(|&atom| prepared.state.graph.atoms.get(atom))
                    .map(|atom| i32::from(atom.charge))
                    .sum();
                offset += count;
                (charge <= 0, part)
            })
            .collect::<Vec<_>>();
        components.sort_by_key(|(nonpositive, _)| *nonpositive);
        components
            .into_iter()
            .map(|(_, part)| part)
            .collect::<Vec<_>>()
            .join(".")
    } else {
        smiles
    };
    let synonyms = if smiles.contains('.') {
        vec![]
    } else {
        super::aliases_for_smiles(&canonical_smiles).map_err(Error::Worker)?
    };
    Ok(Record {
        prepared: Some(prepared),
        title: name.into(),
        systematic_name: None,
        name_status: None,
        alternatives: vec![],
        omitted_stereo: vec![],
        omitted_units: 0,
        locants: Some(locants),
        smiles,
        canonical_smiles,
        synonyms,
        warnings,
        provenance: Provenance::OpsinRust,
    })
}

/// Connected-component size in the source graph, used to align writer fragments.
fn component_atoms(
    state: &crate::chemistry::stereo::perception::State,
    first: usize,
) -> std::collections::HashSet<usize> {
    let mut seen = std::collections::HashSet::from([first]);
    let mut pending = vec![first];
    while let Some(atom) = pending.pop() {
        for bond in &state.graph.bonds {
            let other = if bond.a == atom {
                Some(bond.b)
            } else if bond.b == atom {
                Some(bond.a)
            } else {
                None
            };
            if let Some(other) = other
                && seen.insert(other)
            {
                pending.push(other);
            }
        }
    }
    seen
}

/// Check only the syntax of a single relative CX group in a generation reply.
/// Reverse verification checks its parsed membership before accepting identity.
#[cfg(test)]
pub(super) fn relative_stereo_smiles(smiles: &str) -> Option<&str> {
    let (graph, group) = smiles.split_once(" |o")?;
    let (number, atoms) = group.strip_suffix('|')?.split_once(':')?;
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    (digits(number) && atoms.split(',').all(digits)).then_some(graph)
}

fn cip_descriptors(name: &str) -> Vec<&str> {
    name.match_indices('(')
        .filter_map(|(start, _)| {
            let end = start + name[start..].find(')')? + 1;
            let text = &name[start + 1..end - 1];
            let mut cip = false;
            for term in text.split(',') {
                let term = term.trim();
                let descriptor = term.chars().last()?;
                if !matches!(descriptor, 'R' | 'S' | 'r' | 's' | 'M' | 'P' | 'E' | 'Z') {
                    return None;
                }
                let locant = &term[..term.len() - 1];
                if !locant.is_empty()
                    && (!locant.starts_with(|c: char| c.is_ascii_digit())
                        || !locant.chars().all(|c| {
                            c.is_ascii_digit() || c.is_ascii_lowercase() || matches!(c, '\'' | '′')
                        }))
                {
                    return None;
                }
                cip |= matches!(descriptor, 'R' | 'S' | 'r' | 's' | 'M' | 'P');
            }
            cip.then_some(&name[start..end])
        })
        .collect()
}

#[cfg(test)]
mod tests;
