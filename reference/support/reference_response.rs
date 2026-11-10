//! ReShiki-only analysis fields derived from the independent reference drawing.
use reshiki::{
    chemistry::document,
    engine::{Response, native_response::SMILES_AXIAL_NOTICE},
};
use serde_json::Value;

pub fn expected(mut response: Response) -> anyhow::Result<Value> {
    if let Some(analysis) = &mut response.analysis {
        // Match native_response::complete_analysis: an axis in the prepared
        // molecule loses information only when plain SMILES is available.
        // A drawing that cannot be prepared has no axis to lose.
        let axial = response
            .document
            .as_ref()
            .and_then(|document| document::prepare(document).ok())
            .is_some_and(|molecule| {
                molecule
                    .state
                    .metadata
                    .bonds
                    .iter()
                    .any(|bond| matches!(bond.stereo, 6 | 7))
            });
        analysis.smiles_notice =
            (axial && !analysis.smiles.is_empty()).then(|| SMILES_AXIAL_NOTICE.into());
    }
    Ok(serde_json::to_value(response)?)
}
