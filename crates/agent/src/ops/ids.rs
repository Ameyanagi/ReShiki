//! Object IDs from a request: the decode-time count cap and the check
//! against a document.
use super::{
    budget::Budgets,
    error::{ErrorKind, OpError},
    wire::ObjectId,
};
use crate::document::Document;
use std::collections::HashSet;

/// Fails with [`ErrorKind::Budget`] when a request names more than
/// [`Budgets::max_ids`] object IDs. Decoders call it before any work runs.
pub(crate) fn check_count(ids: &[ObjectId], budgets: &Budgets) -> Result<(), OpError> {
    if ids.len() > budgets.max_ids {
        return Err(OpError::new(
            ErrorKind::Budget,
            format!(
                "{} object IDs were given; at most {} are allowed per request",
                ids.len(),
                budgets.max_ids
            ),
        ));
    }
    Ok(())
}

/// The IDs as `u64`, in the request's order, when every one is in
/// [`Document::object_ids`]: an atom, annotation, arrow or graphic. Bonds have
/// no ID and groups are not objects.
///
/// Otherwise [`ErrorKind::UnknownObject`] names the first missing ID.
/// Operations call this before any selection expansion.
pub(crate) fn resolve(doc: &Document, ids: &[ObjectId]) -> Result<Vec<u64>, OpError> {
    let known: HashSet<u64> = doc.object_ids().collect();
    if let Some(missing) = ids.iter().find(|id| !known.contains(&id.0)) {
        return Err(OpError::new(
            ErrorKind::UnknownObject,
            format!("Object {missing} is not in the drawing"),
        ));
    }
    Ok(ids.iter().map(|id| id.0).collect())
}
