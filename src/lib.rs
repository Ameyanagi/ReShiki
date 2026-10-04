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

pub mod abbreviations;
pub mod accessibility;
pub mod aromatic;
pub mod arrows;
pub mod assistant;
pub mod atom_labels;
pub mod atom_text;
pub mod attachments;
mod bond_joins;
pub mod bonds;
pub mod canvas_theme;
pub mod chains;
pub mod chemistry;
pub mod cleanup;
pub mod clipboard;
pub mod color_contrast;
pub mod common_groups;
pub mod compatibility;
pub mod crossings;
pub mod depth_appearance;
pub mod document;
pub mod document_styles;
pub mod editing;
pub mod engine;
pub mod erasing;
pub mod exchange;
pub mod export;
pub mod geometry;
pub mod graphics;
pub mod grouping;
pub mod haworth;
pub mod highlights;
pub mod hotkeys;
pub mod joining;
pub mod keyboard_drawing;
pub mod libreoffice;
pub mod ligands;
#[cfg(not(windows))]
pub(crate) mod native_process;
#[cfg(windows)]
mod native_windows;
pub mod office_addin;
pub mod pages;
pub mod palette;
pub mod pictures;
pub mod printing;
pub mod projection;
pub mod reactions;
pub mod recovery;
pub mod ring_arcs;
pub mod ring_fills;
pub mod rings;
pub mod scene;
pub mod scientific;
pub mod selection_region;
pub mod storage;
pub mod style;
pub mod template_library;
pub mod templates;
pub mod theme_files;
pub mod theme_generator;
pub mod typography;
pub mod updates;
