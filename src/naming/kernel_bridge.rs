//! Temporary plain-data boundary to identity helpers that stay in the app.
//! Removed in step 3 of docs/plans/naming-split.md, when `canonical_identity`,
//! `apply_axis_markers` and `is_pseudoasymmetric` move into nazuchem-kernel.
//!
//! Prepared bare-SMILES states cross as JSON bytes to preserve source indices,
//! valences and stereo caches. Rewriting SMILES here would reorder atoms and
//! lose annotations. The skipped `invalid_unknown` flag is always false on
//! these paths: CX properties are rejected before bare-SMILES preparation.
use reshiki_chemistry::stereo::perception::State;

fn state(bytes: &[u8]) -> Result<State, String> {
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}

pub(super) fn apply_axis_markers(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let state = reshiki_model::molecule_names::apply_axis_markers(&state(bytes)?)
        .map_err(|error| format!("{error:?}"))?;
    serde_json::to_vec(&state).map_err(|error| error.to_string())
}

pub(super) fn canonical_identity(bytes: &[u8]) -> Result<String, String> {
    reshiki_model::molecule_names::canonical_identity(&state(bytes)?)
        .map_err(|error| format!("{error:?}"))
}

pub(super) fn is_pseudoasymmetric(bytes: &[u8], atom: usize) -> Result<bool, String> {
    Ok(reshiki_model::chemistry::stereo_lint::is_pseudoasymmetric(
        &state(bytes)?,
        atom,
    ))
}

#[cfg(test)]
pub(super) fn smiles_from_inchi(inchi: &str) -> Result<String, String> {
    let imported = reshiki_chemistry::inchi::generator::read_in_process(inchi)
        .map_err(|error| error.to_string())?;
    let state = imported.state.ok_or(imported.message)?;
    reshiki_chemistry::smiles::write::write(&state, Default::default())
        .map(|output| output.text)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
pub(super) fn inchi_from_smiles(smiles: &str) -> String {
    use reshiki_chemistry::inchi::{generator, kernel};
    let state = reshiki_chemistry::smiles::prepare(smiles).unwrap().state;
    let input = kernel::Molecule::prepare(&state, None).unwrap();
    let output = generator::generate_in_process(&input).unwrap();
    assert!(output.status.is_success(), "{smiles}: {}", output.message);
    output.inchi
}
