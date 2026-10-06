//! Composition progress, independent of the chat transport that displays it.
use crate::document::Document;

/// A step reported while a proposal is prepared and previewed.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub enum Event {
    Proposal(Box<super::Proposal>),
    Structures { completed: usize, total: usize },
    Preview(Box<Document>),
}
