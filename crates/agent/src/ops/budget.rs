//! Resource budgets for the operation API and the document cost helpers they
//! are measured with.
use crate::document::Document;
use std::{collections::HashSet, time::Duration};

/// Object IDs per request, mirroring the `replace_ids` limit of
/// [`crate::Proposal::validate`].
pub const MAX_IDS: usize = 5_000;

const MIB: usize = 1024 * 1024;

/// Limits for one operation host. [`Budgets::default`] documents each value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budgets {
    /// 24 MiB. Published for the transport's request line cap.
    pub max_request_bytes: usize,
    /// `exchange::LIMIT` (16 MiB), for textual import formats only.
    pub max_text_bytes: usize,
    /// `exchange::LIMIT.div_ceil(3) * 4`, the base64 length of the largest
    /// CDX, and the only limit for cdx imports (as in native import).
    pub max_cdx_base64: usize,
    /// 16 documents per principal.
    pub max_documents: usize,
    /// 100,000 objects per document, the CDX exchange object limit.
    pub max_objects: usize,
    /// [`MAX_IDS`] object IDs per request.
    pub max_ids: usize,
    /// 2,000,000 per principal. The weight of a session is the sum of
    /// [`cost`] over each entry's current document and every undo and redo
    /// frame.
    pub max_session_weight: u64,
    /// 128 MiB per principal, measured by [`picture_memory`] over all of the
    /// principal's documents and history frames.
    pub max_session_picture_bytes: u64,
    /// 20 undo steps per document.
    pub history_depth: usize,
    /// 5,000 objects per inspect result.
    pub max_inspect_objects: usize,
    /// Preview render sizes.
    pub render: RenderBudget,
    /// 16,000,000 pixels for exported PNG files.
    pub export_png_pixels: u64,
    /// 16 MiB of raw output per result, 22,369,624 bytes (about 21.3 MiB)
    /// after base64.
    pub max_output_bytes: usize,
    /// 2 operations run at once.
    pub concurrency: usize,
    /// 8 calls may wait for a permit; more are refused as busy.
    pub queue: usize,
    /// 120 s cooperative deadline per operation.
    pub op_deadline: Duration,
    /// 60 min: an idle session document expires.
    pub idle_ttl: Duration,
    /// 32 idempotency receipts kept per document.
    pub idempotency_receipts: usize,
    /// 128 bytes per idempotency key.
    pub max_idempotency_key_bytes: usize,
}

/// Preview render limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderBudget {
    /// 1600, as `canvas_tools::image`.
    pub default_width: u32,
    /// 1000, as `canvas_tools::image`.
    pub default_height: u32,
    /// 1 pixel.
    pub min_side: u32,
    /// 8192 pixels.
    pub max_side: u32,
    /// 16,000,000 pixels, `pictures::MAX_PIXELS`.
    pub max_pixels: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        let text = reshiki_io::exchange::LIMIT;
        Self {
            max_request_bytes: 24 * MIB,
            max_text_bytes: text,
            max_cdx_base64: text.div_ceil(3) * 4,
            max_documents: 16,
            max_objects: 100_000,
            max_ids: MAX_IDS,
            max_session_weight: 2_000_000,
            max_session_picture_bytes: 128 * 1024 * 1024,
            history_depth: 20,
            max_inspect_objects: 5_000,
            render: RenderBudget {
                default_width: 1600,
                default_height: 1000,
                min_side: 1,
                max_side: 8192,
                max_pixels: crate::pictures::MAX_PIXELS,
            },
            export_png_pixels: 16_000_000,
            max_output_bytes: 16 * MIB,
            concurrency: 2,
            queue: 8,
            op_deadline: Duration::from_secs(120),
            idle_ttl: Duration::from_secs(60 * 60),
            idempotency_receipts: 32,
            max_idempotency_key_bytes: 128,
        }
    }
}

impl Budgets {
    /// Every limit is positive, every import argument fits in a request, and
    /// the default render size is within the render limits.
    pub fn validate(&self) -> Result<(), String> {
        let positive = [
            ("max_request_bytes", self.max_request_bytes),
            ("max_text_bytes", self.max_text_bytes),
            ("max_cdx_base64", self.max_cdx_base64),
            ("max_documents", self.max_documents),
            ("max_objects", self.max_objects),
            ("max_ids", self.max_ids),
            ("history_depth", self.history_depth),
            ("max_inspect_objects", self.max_inspect_objects),
            ("max_output_bytes", self.max_output_bytes),
            ("concurrency", self.concurrency),
            ("queue", self.queue),
            ("idempotency_receipts", self.idempotency_receipts),
            ("max_idempotency_key_bytes", self.max_idempotency_key_bytes),
        ];
        if let Some((name, _)) = positive.iter().find(|(_, value)| *value == 0) {
            return Err(format!("{name} must be positive"));
        }
        if self.max_session_weight == 0
            || self.max_session_picture_bytes == 0
            || self.export_png_pixels == 0
        {
            return Err("Session and export limits must be positive".into());
        }
        if self.op_deadline.is_zero() || self.idle_ttl.is_zero() {
            return Err("op_deadline and idle_ttl must be positive".into());
        }
        if self.max_text_bytes > self.max_request_bytes
            || self.max_cdx_base64 > self.max_request_bytes
        {
            return Err("Import limits must fit within max_request_bytes".into());
        }
        let render = &self.render;
        let sides = render.min_side..=render.max_side;
        if render.min_side == 0
            || !sides.contains(&render.default_width)
            || !sides.contains(&render.default_height)
            || u64::from(render.default_width) * u64::from(render.default_height)
                > render.max_pixels
        {
            return Err("The default render size must be within the render limits".into());
        }
        Ok(())
    }
}

/// Atoms, bonds, annotations, arrows, graphics and groups in `doc`.
pub fn objects(doc: &Document) -> usize {
    doc.atoms.len()
        + doc.bonds.len()
        + doc.annotations.len()
        + doc.arrows.len()
        + doc.graphics.len()
        + doc.groups.len()
}

/// The weight of one document or history frame: [`objects`] plus one, so an
/// empty document still counts.
pub fn cost(doc: &Document) -> u64 {
    (objects(doc) as u64).saturating_add(1)
}

/// Stored PNG and original EMF bytes in `docs`, counting each storage once.
///
/// Picture clones share one storage, so a picture held by many documents or
/// history frames counts once. Storages are keyed by the address of their
/// PNG bytes, which is only meaningful while `docs` are borrowed.
pub fn picture_memory<'a>(docs: impl IntoIterator<Item = &'a Document>) -> u64 {
    let mut seen = HashSet::new();
    docs.into_iter()
        .flat_map(|doc| &doc.graphics)
        .filter_map(|graphic| graphic.picture.as_ref())
        .filter(|picture| seen.insert(picture.png().as_ptr() as usize))
        .map(|picture| picture.stored_bytes() as u64)
        .fold(0, u64::saturating_add)
}

#[cfg(test)]
mod tests;
