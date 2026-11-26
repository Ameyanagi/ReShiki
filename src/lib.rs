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
    attachments, bonds, canvas_theme, chains, color_contrast, common_groups, compare,
    compound_numbers, crossings, cycles, depiction, depth_appearance, document, editing, equipment,
    erasing, fischer, generic, graphics, grouping, haworth, highlights, ionic, joining, layouts,
    leaders, ligands, line_icons, linked_properties, models3d, molecule_names, pages, palette,
    pictures, projection, purpose, reaction_cleanup, reaction_colors, reaction_mapping, reactions,
    rear_opacity, ring_arcs, ring_fills, rings, scene, science, scientific, selection_region,
    shape_text, stereo_groups, stereochemistry, storage, style, symbols, templates, theme_files,
    theme_generator, transaction, typography,
};

#[cfg(not(target_arch = "wasm32"))]
pub use reshiki_agent::envelope;
#[cfg(windows)]
use reshiki_io::native_windows;
pub use reshiki_io::{
    chemistry, cleanup, compatibility, crystal, document_styles, engine, exchange, export, formats,
    geometry, nmr, payload, piece_library, qc_input, qcdata, qcread, recovery, svg_drawing, task,
    template_library,
};

pub mod accessibility;
#[cfg(not(target_arch = "wasm32"))]
pub mod assistant;
#[doc(hidden)]
pub mod browser_render;
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod browser_worker;
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub mod cli;
pub mod clipboard;
#[cfg(not(target_arch = "wasm32"))]
pub mod github;
pub mod hotkeys;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod http;
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
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
pub mod xtb_agent;
#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
#[doc(hidden)]
pub mod xtb_pool;

pub use reshiki_xtb_jobs::XTB_NEAR_DEGENERATE;
