//! Defined for the operation API (design Revision 1 items 14-15); not wired to any transport.
//!
//! # IDs
//!
//! - Object IDs are `u64` and share one space across atoms, annotations,
//!   arrows and graphics, in [`Document::object_ids`] order.
//! - Group IDs come from the same counter: [`Document::next_id`] is the
//!   largest object or group ID plus one, saturating.
//! - Bonds have no ID; they are addressed by their endpoint pair.
//! - Abbreviations are addressed by their [anchor atom], and reactions by
//!   their [arrow ID].
//! - The IDs of deleted objects can be reused once the maximum is gone.
//! - Transports must encode IDs as decimal strings.
//!
//! # Selection expansion
//!
//! - [`attachments::selection`] adds a selected point's target atoms, and the
//!   attachment points whose targets are all selected.
//! - [`Document::expand_abbreviation_selection`]: any selected member pulls in
//!   the whole abbreviation, and the result is in [`Document::all_ids`] order.
//! - Groups: [`Document::expand_groups`], [`Document::expand_integral_groups`]
//!   and [`Document::complete_selection`].
//! - [`editing::selection`] composes the attachment and abbreviation
//!   expansions.
//!
//! # ID remapping
//!
//! [`editing::append`] numbers the inserted objects from
//! `first = doc.next_id()`, mapping `source.all_ids()` and then the source
//! group IDs sequentially. It returns the inserted part's `all_ids()`, which is
//! the source `all_ids()` order remapped, or an empty vec on failure. The remap
//! is therefore `zip(source.all_ids(), returned)`; see [`IdRemap`].
//!
//! [`Document::object_ids`]: crate::document::Document::object_ids
//! [`Document::next_id`]: crate::document::Document::next_id
//! [anchor atom]: crate::abbreviations::Abbreviation::anchor
//! [arrow ID]: crate::reactions::Reaction::arrow
//! [`attachments::selection`]: crate::attachments::selection
//! [`Document::expand_abbreviation_selection`]: crate::document::Document::expand_abbreviation_selection
//! [`Document::all_ids`]: crate::document::Document::all_ids
//! [`Document::expand_groups`]: crate::document::Document::expand_groups
//! [`Document::expand_integral_groups`]: crate::document::Document::expand_integral_groups
//! [`Document::complete_selection`]: crate::document::Document::complete_selection
//! [`editing::selection`]: crate::editing::selection
//! [`editing::append`]: crate::editing::append
use crate::transaction::Rejection;

/// The operation API version, separate from the engine protocol and the
/// document version.
pub const OPERATION_API_VERSION: u32 = 1;

/// The four version numbers a result reports; each changes independently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versions {
    /// The application version.
    pub app: String,
    /// [`OPERATION_API_VERSION`].
    pub operation_api: u32,
    /// The internal engine request protocol, [`crate::engine::PROTOCOL`].
    pub engine_protocol: u32,
    /// The document format version, [`crate::document::VERSION`].
    pub document: u32,
}

impl Versions {
    /// This build's versions. Hosts pass `reshiki::updates::CURRENT_VERSION`
    /// as `app`.
    pub fn current(app: &str) -> Self {
        Self {
            app: app.to_owned(),
            operation_api: OPERATION_API_VERSION,
            engine_protocol: crate::engine::PROTOCOL,
            document: crate::document::VERSION,
        }
    }
}

/// A non-fatal message attached to a result.
///
/// Sources: engine [`Response::warnings`](crate::engine::Response::warnings),
/// the detail returned by [`figure_document`](crate::export::figure_document)
/// and cleanup preview warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub message: String,
}

/// Whether an edit was accepted or rolled back by
/// [`transaction::reconcile`](crate::transaction::reconcile).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationStatus {
    Valid,
    Rejected(Rejection),
}

/// What an export produced, mirroring [`Figure`](crate::export::Figure)
/// without its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReceipt {
    pub format: String,
    /// The length of [`Figure::bytes`](crate::export::Figure::bytes).
    pub byte_len: usize,
    /// [`Figure::detail`](crate::export::Figure::detail).
    pub detail: Option<String>,
}

/// How inserted objects were renumbered, as described under "ID remapping"
/// in the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdRemap {
    /// `(source ID, inserted ID)` pairs.
    pub pairs: Vec<(u64, u64)>,
}

/// A result with its warnings, validation status and versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope<T> {
    pub value: T,
    pub warnings: Vec<Warning>,
    pub validation: ValidationStatus,
    pub versions: Versions,
}

#[cfg(test)]
mod tests;
