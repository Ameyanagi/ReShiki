//! Native Rust OPSIN adapter, isolated in the bounded same-executable worker.
//! The browser sends the same request to its naming Web Worker instead.
#[cfg(not(target_arch = "wasm32"))]
use super::client::{self, CancelOnDrop, POLL};
use super::{Error, Locants, Provenance, Record, canonical_smiles, worker};
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
    let smiles = reply.cxsmiles.ok_or_else(|| {
        Error::Rejected("The OPSIN Rust port returned no molecular structure".into())
    })?;
    if smiles.contains('|') {
        const MESSAGE: &str = "This name requires relative/racemic stereo, polymer or atom-label semantics unsupported by editable naming previews. No simplified structure was accepted";
        return Err(Error::Rejected(MESSAGE.into()));
    }
    let mut canonical_smiles = canonical_smiles(&smiles).map_err(Error::Rejected)?;
    let atom_count = crate::chemistry::smiles::parse(&smiles)
        .map_err(|error| Error::Worker(error.to_string()))?
        .graph
        .atoms
        .len();
    if reply.locants.len() != atom_count {
        return Err(Error::Worker(
            "Local parser returned locants for a different atom count".into(),
        ));
    }
    // Locants map by constitution, so they keep the parser's SMILES and atom
    // order even when salt components are reordered below.
    let locants = Locants {
        smiles: smiles.clone(),
        atoms: reply.locants,
    };
    let smiles = if reply.axes.is_empty() {
        smiles
    } else {
        let mut state = crate::chemistry::smiles::read(&smiles)
            .map_err(|error| Error::Rejected(error.to_string()))?
            .prepared
            .state;
        if state.graph.atoms.len() != atom_count {
            return Err(Error::Rejected(
                "Native import changed the atom count of an axial structure".into(),
            ));
        }
        let invalid = || Error::Worker("Local parser returned an invalid axis".into());
        let mut axes = Vec::new();
        for axis in &reply.axes {
            let [a, b] = axis.atoms;
            let index = state
                .graph
                .bonds
                .iter()
                .position(|bond| (bond.a, bond.b) == (a, b) || (bond.a, bond.b) == (b, a))
                .ok_or_else(invalid)?;
            let bond = state.graph.bonds.get(index).ok_or_else(invalid)?;
            let mut refs = axis.refs;
            if bond.a != a {
                refs.reverse();
            }
            // Native axial CIP and wedging take the lowest-index neighbour at
            // each end as carrier, so the winding flips once per other carrier.
            let mut clockwise = axis.clockwise;
            let mut carriers = vec![];
            for ((atom, other), reference) in
                [(bond.a, bond.b), (bond.b, bond.a)].into_iter().zip(refs)
            {
                let neighbours: Vec<_> = state
                    .graph
                    .bonds
                    .iter()
                    .filter_map(|bond| match (bond.a, bond.b) {
                        (x, y) | (y, x) if x == atom && y != other => Some(y),
                        _ => None,
                    })
                    .collect();
                let carrier = *neighbours.iter().min().ok_or_else(invalid)?;
                if neighbours.len() != 2 || !neighbours.contains(&reference) {
                    return Err(invalid());
                }
                clockwise ^= carrier != reference;
                carriers.push(carrier);
            }
            let metadata = state.metadata.bonds.get_mut(index).ok_or_else(invalid)?;
            if metadata.stereo != 0 {
                return Err(invalid());
            }
            metadata.stereo = if clockwise { 6 } else { 7 };
            metadata.stereo_atoms = carriers;
            axes.push((index, axis.descriptor.as_str()));
        }
        // Verify the requested descriptor after transferring references into
        // the SMILES atom order, since InChI cannot check the axis.
        let labelled = crate::chemistry::stereo::cip::label::assign(&state, &Default::default())
            .map_err(|error| Error::Rejected(error.to_string()))?;
        if axes.iter().any(|&(index, descriptor)| {
            labelled
                .state
                .properties
                .bond_codes
                .get(index)
                .and_then(Option::as_deref)
                != Some(descriptor)
        }) {
            return Err(Error::Rejected(
                "Native and OPSIN CIP assignments disagree on the biaryl axis".into(),
            ));
        }
        canonical_smiles = super::canonical_state(&state).map_err(Error::Rejected)?;
        canonical_smiles.clone()
    };
    if !generation
        && crate::chemistry::smiles::prepare(&canonical_smiles)
            .map_err(|error| Error::Rejected(error.to_string()))?
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
    let mut warnings = Vec::new();
    if let Some(signs) = signs {
        let descriptors = cip_descriptors(name);
        let imported = crate::chemistry::smiles::read(&smiles)
            .map_err(|error| Error::Rejected(error.to_string()))?;
        if descriptors.is_empty()
            || crate::chemistry::stereo::perception::has_unspecified_stereo(
                &imported.prepared.state,
            )
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
    let smiles = if smiles.contains('.') {
        let mut components = smiles
            .split('.')
            .map(|part| {
                let graph = crate::chemistry::smiles::parse(part)
                    .map_err(|error| Error::Rejected(error.to_string()))?
                    .graph;
                let charge: i32 = graph.atoms.iter().map(|atom| i32::from(atom.charge)).sum();
                Ok((charge <= 0, part))
            })
            .collect::<Result<Vec<_>, Error>>()?;
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
