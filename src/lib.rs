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

pub mod abbreviations;
pub mod aromatic;
pub mod arrows;
pub mod assistant;
pub mod atom_labels;
pub mod bonds;
pub mod chains;
pub mod cleanup;
pub mod clipboard;
pub mod crossings;
pub mod document;
pub mod editing;
pub mod engine;
pub mod export;
pub mod graphics;
pub mod grouping;
pub mod joining;
pub mod pages;
pub mod recovery;
pub mod rings;
pub mod scene;
pub mod scientific;
pub mod selection_region;
pub mod storage;
pub mod style;
pub mod template_library;
pub mod templates;
pub mod typography;
