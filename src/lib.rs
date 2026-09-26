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
pub mod atom_text;
pub mod attachments;
mod bond_joins;
pub mod bonds;
pub mod canvas_theme;
pub mod chains;
pub mod chemistry;
pub mod cleanup;
pub mod clipboard;
pub mod common_groups;
pub mod compatibility;
pub mod crossings;
pub mod document;
pub mod document_styles;
pub mod editing;
pub mod engine;
pub mod erasing;
pub mod exchange;
pub mod export;
pub mod graphics;
pub mod grouping;
pub mod haworth;
pub mod hotkeys;
pub mod joining;
pub mod ligands;
#[cfg(windows)]
mod native_windows;
pub mod pages;
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
pub mod typography;
pub mod updates;
