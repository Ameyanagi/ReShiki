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

pub use reshiki_model::{
    abbreviations, aromatic, arrows, atom_labels, atom_text, attachments, bonds, canvas_theme,
    chains, color_contrast, common_groups, crossings, depth_appearance, document, editing, erasing,
    graphics, grouping, haworth, highlights, joining, ligands, pages, palette, pictures,
    projection, reactions, ring_arcs, ring_fills, rings, scene, scientific, selection_region,
    storage, style, templates, theme_files, theme_generator, typography,
};

pub mod accessibility;
pub mod assistant;
pub mod chemistry;
pub mod cleanup;
pub mod clipboard;
pub mod compatibility;
pub mod document_styles;
pub mod engine;
pub mod exchange;
pub mod export;
pub mod geometry;
pub mod hotkeys;
pub mod keyboard_drawing;
pub mod libreoffice;
#[cfg(not(windows))]
pub(crate) mod native_process;
#[cfg(windows)]
mod native_windows;
pub mod office_addin;
pub mod printing;
pub mod recovery;
pub mod template_library;
pub mod updates;
