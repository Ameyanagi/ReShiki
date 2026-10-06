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
#[global_allocator]
static ALLOCATOR: reshiki_process_heap::allocation_metrics::MeasuredAllocator<std::alloc::System> =
    reshiki_process_heap::allocation_metrics::MeasuredAllocator::new(std::alloc::System);

pub mod abbreviations;
pub mod aromatic;
pub mod arrows;
pub mod atom_labels;
pub mod atom_text;
pub mod attachments;
mod bond_joins;
pub mod bonds;
pub mod canvas_theme;
pub mod chains;
pub mod chemistry;
pub mod color_contrast;
pub mod common_groups;
pub mod crossings;
pub mod depth_appearance;
pub mod document;
pub mod editing;
pub mod erasing;
pub mod graphics;
pub mod grouping;
pub mod haworth;
pub mod highlights;
pub mod joining;
pub mod ligands;
pub mod pages;
pub mod palette;
pub mod pictures;
pub mod projection;
pub mod reactions;
pub mod ring_arcs;
pub mod ring_fills;
pub mod rings;
pub mod scene;
pub mod scientific;
pub mod selection_region;
pub mod storage;
pub mod style;
pub mod templates;
pub mod theme_files;
pub mod theme_generator;
pub mod transaction;
pub mod typography;

#[cfg(test)]
mod parity_tests;
