#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]

#[cfg(test)]
pub(crate) use reshiki_process_heap::allocation_metrics;
#[cfg(test)]
#[global_allocator]
static ALLOCATOR: allocation_metrics::MeasuredAllocator<std::alloc::System> =
    allocation_metrics::MeasuredAllocator::new(std::alloc::System);

// Keep crate::document, crate::scene and the other model paths resolving
// inside this crate. A glob never leaves a cfg-specific import unused, the
// import stays private, and the local `chemistry` module shadows the model's.
use reshiki_model::*;

pub mod chemistry;
pub mod cleanup;
pub mod compatibility;
pub mod document_styles;
pub mod engine;
pub mod exchange;
pub mod export;
pub mod geometry;
#[cfg(windows)]
#[doc(hidden)]
pub mod native_windows;
pub mod nmr;
pub mod recovery;
pub mod template_library;

/// The repository root, from which the development reference resolves its
/// Python worker and fixtures; this crate lives at crates/io.
#[cfg(feature = "rdkit-reference")]
pub(crate) fn repository_root() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .unwrap_or(manifest)
        .to_path_buf()
}
