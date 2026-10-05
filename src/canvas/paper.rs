//! The drawing paper, layer by layer: background, the document as the active gesture would leave it, the document, then editing overlays.

mod draft;
mod placement;

use super::World;
use super::smart_guides::Guide;
use reshiki::document::Document;
use std::borrow::Cow;

/// The document as the in-progress gesture would leave it, plus its transient decorations.
pub(super) struct Draft<'d> {
    pub(super) preview: Cow<'d, Document>,
    pub(super) ring_selection: Option<Vec<u64>>,
    pub(super) chain_badge: Option<(World, String, bool)>,
    pub(super) template_notice: Option<(String, bool)>,
    pub(super) rejection: Option<reshiki::editing::RingRejection>,
    pub(super) translation: Option<World>,
    pub(super) smart: Vec<Guide>,
}

impl<'d> Draft<'d> {
    pub(super) fn new(base: &'d Document) -> Self {
        Self {
            preview: Cow::Borrowed(base),
            ring_selection: None,
            chain_badge: None,
            template_notice: None,
            rejection: None,
            translation: None,
            smart: Vec::new(),
        }
    }
}
