//! The root package's integration tests, built as one test binary. Each file
//! under `tests/` is a module here, so the application library links once and
//! not once per file. To run one file's tests: `cargo test --test it cli::`.
//! Three files stay separate targets in Cargo.toml: `metafile_worker` has its
//! own `main`, `picture_memory` its own global allocator, and release.yml
//! builds `agent_api_windows_pipes` alone in the release profile.

#[path = "common/fixture.rs"]
mod fixture;
#[path = "common/headless.rs"]
mod headless;
#[path = "../reference/support/ring_basis.rs"]
mod ring_basis;

mod abbreviation_style_exchange;
mod abbreviation_styles;
mod abbreviations;
mod adjustable_arcs;
mod agent_api_cli_files;
mod agent_api_grants;
mod agent_api_labels;
mod agent_api_malformed;
mod agent_api_runtime;
mod aromatic;
mod aromatic_bond_styles;
mod aromatic_fusion;
mod arrows;
mod assistant;
mod assistant_attachments;
mod assistant_contract;
mod assistant_ligand_review;
mod assistant_ligands;
mod atom_labels;
mod attachment_movement;
mod attachments;
mod bonds;
mod bridge;
mod canvas_theme;
mod chains;
mod chemdraw_arrows;
mod chemdraw_captions;
mod cli;
mod clipboard_exchange;
mod common_groups_shortcuts;
mod composed_attachment_name_mapping;
mod composed_map_rear_visibility;
mod composed_native_roundtrip;
mod crossings;
mod depth_appearance;
mod drawing_transfer;
mod dummy_exports;
mod engine_backend;
mod explicit_labels;
mod figure_exports;
mod font_export_103;
mod graphics;
mod grouping;
mod haworth;
mod haworth_interchange;
mod headless_launch;
mod headless_parity;
mod highlight_exchange;
mod highlight_rendering;
mod highlights;
mod hydrogen_placement;
mod import_batch_native_roundtrip;
mod inchi_input;
mod inchi_output;
mod inchi_rust;
mod internal_groups;
mod io_stage_baseline;
mod joining;
mod ligand_groups;
mod mcp_shutdown;
mod mcp_stdio;
mod mcp_tools;
mod native_engine;
mod pages;
mod pi_ligand_exchange;
mod pictures;
mod plane_growth;
mod porphine_core;
mod print_worker;
mod printing;
mod projection_crossings;
mod projection_exchange;
mod reaction_mapping;
mod reactions;
mod rear_opacity;
mod reference_geometry;
mod regular_ring_safety;
mod ring_arc_rendering;
mod ring_curve_crossings;
mod ring_fills;
mod rings;
mod rotation_pivot;
mod runtime_safety;
mod scientific;
mod selection_resize;
mod shortcut_gallery;
mod stated_ez_wedges;
mod steroids;
mod style_units;
mod tbdps_abbreviations;
mod template_library;
mod template_style;
mod templates;
mod theme_files;
mod theme_generator;
mod theme_reference;
mod tunable_pen_lines;
mod typography;
mod windows_native;
mod windows_version_metadata;

/// Cargo no longer finds test files by itself (`autotests = false`), so a file
/// that is neither a module above nor a target in Cargo.toml would never be
/// compiled.
#[test]
fn every_test_file_is_a_module_of_this_binary() {
    let declared = |name: &String| {
        include_str!("it.rs").contains(&format!("\nmod {name};\n"))
            || include_str!("../Cargo.toml").contains(&format!("\"tests/{name}.rs\""))
    };
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let missing: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".rs").map(str::to_owned))
        .filter(|name| !declared(name))
        .collect();
    assert!(
        missing.is_empty(),
        "add as a module to tests/it.rs: {missing:?}"
    );
}
