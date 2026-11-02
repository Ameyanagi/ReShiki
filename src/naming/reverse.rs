#[cfg(not(target_arch = "wasm32"))]
use super::{Cancel, client, client::CancelOnDrop};
use super::{Error, Identity, Provenance, Record, openclatura, worker};

#[cfg(not(target_arch = "wasm32"))]
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[cfg(not(target_arch = "wasm32"))]
fn generate_worker(smiles: &str, cancel: &Cancel) -> Result<openclatura::Name, Error> {
    if cancel.stopped() {
        return Err(Error::Cancelled(
            "Local name generation was cancelled".into(),
        ));
    }
    let request = generate_request(smiles)?;
    let bytes = client::run_worker(
        "reshiki-name-generate-",
        &request,
        cancel,
        worker::executable,
    )?;
    parse_response(&bytes, smiles)
}
/// The worker request that names the canonical `smiles` with the openclatura port.
pub(super) fn generate_request(smiles: &str) -> Result<Vec<u8>, Error> {
    let request = serde_json::to_vec(&worker::Request::Generate {
        protocol: worker::PROTOCOL,
        profile: openclatura::PROFILE.into(),
        smiles: smiles.into(),
    })
    .map_err(|e| Error::Worker(e.to_string()))?;
    if request.len() > worker::WIRE_LIMIT {
        return Err(Error::Worker(
            "Local naming request exceeds its wire limit".into(),
        ));
    }
    Ok(request)
}
pub(super) fn parse_response(bytes: &[u8], smiles: &str) -> Result<openclatura::Name, Error> {
    // Keep this wire cap narrower than local::parse_reply's output cap.
    if bytes.len() > worker::WIRE_LIMIT {
        return Err(Error::Worker(
            "Local name generation response exceeds its wire limit".into(),
        ));
    }
    let response: worker::Response = serde_json::from_slice(bytes).map_err(|_| {
        Error::Worker("Local name generation returned an invalid structured result".into())
    })?;
    let worker::Response::Generate(response) = response else {
        return Err(Error::Worker(
            "Local name generation returned a result for a different operation".into(),
        ));
    };
    if response.protocol != worker::PROTOCOL
        || response.profile != openclatura::PROFILE
        || response.input != smiles
    {
        return Err(Error::Worker(
            "Local name generation result has inconsistent profile or input identity".into(),
        ));
    }
    let name = response.result.map_err(Error::Rejected)?;
    worker::check_name(&name.text, "Local name generation returned an invalid name")
        .map_err(Error::Worker)?;
    for alternative in &name.alternatives {
        worker::check_name(
            &alternative.name,
            "Local name generation returned an invalid alternative",
        )
        .map_err(Error::Worker)?;
    }
    Ok(name)
}

/// A name is returned only after the pinned local decoder exactly reconstructs
/// the native graph. Decoder equivalence is separate from the rule tests.
#[cfg(not(target_arch = "wasm32"))]
pub async fn generate_name(identity: &Identity, cancel: Cancel) -> Result<Record, Error> {
    let expected = super::canonical_smiles(&identity.smiles).map_err(Error::Rejected)?;
    let stop = CancelOnDrop(cancel.clone());
    let input = identity.naming_smiles.as_deref().unwrap_or(&expected);
    let candidate = client::on_slot(
        input,
        &cancel,
        &SLOTS,
        "Local name generation is unavailable",
        "Local name generation was cancelled",
        "Local naming worker failed",
        generate_worker,
    )
    .await?;
    let record = reconstruct(identity, expected, candidate, &cancel).await;
    drop(stop);
    record
}

/// Check every name independently, then apply the Ξ/ξ preference.
/// The browser uses the same selection and reconstruction gate.
pub(super) async fn reconstruct(
    identity: &Identity,
    expected: String,
    mut candidate: openclatura::Name,
    cancel: &super::Cancel,
) -> Result<Record, Error> {
    let primary = reconstruct_name(&candidate.text, cancel).await?;
    let mut alternatives = vec![];
    for alternative in std::mem::take(&mut candidate.alternatives) {
        match reconstruct_name(&alternative.name, cancel).await {
            Ok(parsed) if super::verify_identity(&expected, &parsed.smiles).is_ok() => {
                alternatives.push(alternative);
            }
            Err(error @ Error::Cancelled(_)) => return Err(error),
            _ => {}
        }
    }
    // Only a verified Ξ/ξ name can replace the primary name, which has the
    // same identity as `primary`.
    if identity.write_unknown_stereo
        && let Some(alternative) = alternatives
            .iter_mut()
            .find(|alternative| alternative.name.contains(['Ξ', 'ξ']))
    {
        std::mem::swap(&mut candidate.text, &mut alternative.name);
        std::mem::swap(&mut candidate.status, &mut alternative.status);
        candidate.omitted_stereo.clear();
        candidate.omitted_units = 0;
    }
    let mut record = generated_record(identity, expected, candidate, &primary)?;
    record.alternatives = alternatives;
    Ok(record)
}

async fn reconstruct_name(name: &str, cancel: &super::Cancel) -> Result<Record, Error> {
    if cancel.stopped() {
        return Err(Error::Cancelled(
            "Local name generation was cancelled".into(),
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let decoder = super::Cancel::default();
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::select! {
            result = super::resolve_name(name, decoder.clone()) => result,
            () = cancel.cancelled() => {
                decoder.stop();
                Err(Error::Cancelled("Local name generation was cancelled".into()))
            },
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        super::resolve_name(name, cancel.clone()).await
    }
}

/// The generated name's record, once the parser's `reconstructed` record of
/// it has exactly the `expected` identity.
pub(super) fn generated_record(
    identity: &Identity,
    expected: String,
    candidate: openclatura::Name,
    reconstructed: &Record,
) -> Result<Record, Error> {
    super::verify_identity(&expected, &reconstructed.smiles)
        .map_err(|e| Error::Rejected(format!("Name reconstruction failed: {e}")))?;
    let mut warnings = identity.warnings.clone();
    warnings.push(format!(
        "Generated by {} and exactly reconstructed by the OPSIN Rust port.",
        openclatura::PROFILE
    ));
    Ok(Record {
        title: candidate.text.clone(),
        systematic_name: Some(candidate.text),
        name_status: Some(candidate.status),
        alternatives: vec![],
        omitted_stereo: candidate.omitted_stereo,
        omitted_units: candidate.omitted_units,
        smiles: expected.clone(),
        canonical_smiles: expected,
        locants: reconstructed.locants.clone(),
        synonyms: super::aliases_for_smiles(&reconstructed.smiles).map_err(Error::Worker)?,
        warnings,
        provenance: Provenance::Openclatura,
    })
}

#[cfg(test)]
mod tests;
