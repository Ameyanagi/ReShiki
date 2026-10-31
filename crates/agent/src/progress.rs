//! Composition progress, independent of the chat transport that displays it.
use crate::document::Document;

/// A new preparation pass, which may publish several panel previews.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    Preview,
    Final,
}

/// A step reported while a proposal is prepared and previewed.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub enum Event {
    Pass(Pass),
    Proposal(Box<super::Proposal>),
    Structures { completed: usize, total: usize },
    Preview(Box<Document>),
}
