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
    abbreviations, aromatic, arrow_anchors, arrow_labels, arrows, atom_labels, atom_text,
    attachments, bonds, canvas_theme, chains, color_contrast, common_groups, crossings,
    depth_appearance, document, editing, erasing, generic, graphics, grouping, haworth, highlights,
    ionic, joining, ligands, models3d, molecule_names, name_markup, pages, palette, pictures,
    projection, reaction_mapping, reactions, rear_opacity, ring_arcs, ring_fills, rings, scene,
    scientific, selection_region, storage, style, templates, theme_files, theme_generator,
    transaction, typography,
};

#[cfg(not(target_arch = "wasm32"))]
pub use reshiki_agent::envelope;
#[cfg(windows)]
use reshiki_io::native_windows;
pub use reshiki_io::{
    chemistry, cleanup, compatibility, crystal, document_styles, engine, exchange, export, formats,
    geometry, nmr, recovery, task, template_library,
};

pub mod accessibility;
#[cfg(not(target_arch = "wasm32"))]
pub mod assistant;
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub mod cli;
pub mod clipboard;
pub mod hotkeys;
pub mod keyboard_drawing;
#[cfg(not(target_arch = "wasm32"))]
pub mod libreoffice;
pub mod metafile;
pub mod naming;
#[cfg(not(any(windows, target_arch = "wasm32")))]
pub(crate) mod native_process;
pub mod office_addin;
pub mod printing;
pub mod updates;
#[cfg(not(target_arch = "wasm32"))]
pub mod workers;
