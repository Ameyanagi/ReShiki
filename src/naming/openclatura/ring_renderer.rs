// SPDX-License-Identifier: MIT AND (MIT OR Apache-2.0)
// Origin: mixed
// Source: openclatura | src/openclatura/ring_renderer.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: ReShiki | src/naming/openclatura/ring_renderer.rs | 5f559fd482e237307a589dfcb7ea333cee0a6e13
// Source: openclatura | tests/unit/test_analysis.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Source: openclatura | tests/integration/test_human_descriptor.py | 90dc5bcb4910c196122ec0405d06a32929a8f308
// Copyright (c) 2026 lamalab-org
// Copyright (c) 2026 Ameyanagi and ReShiki contributors
// Changes: Adapted openclatura ring/von-Baeyer descriptor rendering and cycle-count decoding with local Rust regression tests that include adapted upstream cases.
// Local contributions: MIT OR Apache-2.0; see NOTICE and PROVENANCE-nazuchem.toml.

//! Data-backed P-23/P-24 ring descriptor rendering.
use super::nomenclature::section;
use super::rules::multipliers;

#[allow(
    clippy::expect_used,
    reason = "Ring descriptor kinds come from the checked-in registry."
)]
// Provenance: fn render_ring_descriptor | adapted
// Source: openclatura | src/openclatura/ring_renderer.py::render_ring_descriptor
pub fn render_ring_descriptor(kind: &str, descriptor_numbers: &[usize]) -> String {
    let template = section("ring_descriptor_templates")[kind]
        .as_str()
        .expect("checked-in ring descriptor template");
    let numbers = descriptor_numbers
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".");
    template.replace("{numbers}", &numbers)
}

/// The bridge count is one less than the number of independent cycles.
// Provenance: fn render_von_baeyer_descriptor | adapted
// Source: openclatura | src/openclatura/ring_renderer.py::render_von_baeyer_descriptor
pub fn render_von_baeyer_descriptor(bridge_count: usize, descriptor_body: &str) -> String {
    format!("{}{descriptor_body}", von_baeyer_kind(bridge_count + 1))
}

// Provenance: fn von_baeyer_kind | adapted
// Source: openclatura | src/openclatura/ring_renderer.py::von_baeyer_kind
pub fn von_baeyer_kind(cycle_count: usize) -> String {
    section("polycycle_prefixes")[cycle_count.saturating_sub(1).to_string()]
        .as_str()
        .map_or_else(
            || format!("{}cyclo", multipliers::basic(cycle_count)),
            str::to_owned,
        )
}

// Provenance: fn von_baeyer_cycle_count | adapted
// Source: openclatura | src/openclatura/ring_renderer.py::von_baeyer_cycle_count
pub fn von_baeyer_cycle_count(descriptor: Option<&str>) -> Option<usize> {
    let prefix = descriptor?.split_once("cyclo[")?.0;
    let rendered_prefix = format!("{prefix}cyclo");
    section("polycycle_prefixes")
        .as_object()
        .and_then(|prefixes| {
            prefixes.iter().find_map(|(count, rendered)| {
                if rendered.as_str()? != rendered_prefix {
                    return None;
                }
                Some(count.parse::<usize>().ok()? + 1)
            })
        })
        .or_else(|| (2..=1000).find(|&count| multipliers::basic(count) == prefix))
}

// Provenance: fn is_von_baeyer_descriptor | adapted
// Source: openclatura | src/openclatura/ring_renderer.py::is_von_baeyer_descriptor
pub fn is_von_baeyer_descriptor(descriptor: Option<&str>) -> bool {
    von_baeyer_cycle_count(descriptor).is_some_and(|count| count >= 3)
}

#[cfg(test)]
// Provenance: mod tests | original
// Source: ReShiki | src/naming/openclatura/ring_renderer.rs::tests
mod tests {
    use super::*;

    #[test]
    // Provenance: fn tests::descriptor_kinds_keep_their_distinct_naming_systems | adapted
    // Source: ReShiki | src/naming/openclatura/ring_renderer.rs::tests::descriptor_kinds_keep_their_distinct_naming_systems
    // Source: openclatura | tests/unit/test_analysis.py::test_ring_descriptor_rendering_is_registry_backed
    // Source: openclatura | tests/integration/test_human_descriptor.py::test_systematic_bicyclic_parent_keeps_its_selected_descriptor
    // Test provenance: upstream inputs/expected names with local assertions and extensions;
    // exact matches are conservatively adapted; independent ancestry is not established.
    fn descriptor_kinds_keep_their_distinct_naming_systems() {
        assert_eq!(render_ring_descriptor("spiro", &[3, 5]), "spiro[3.5]");
        assert_eq!(
            render_ring_descriptor("dispiro", &[3, 1, 3, 1]),
            "dispiro[3.1.3.1]"
        );
        assert_eq!(von_baeyer_kind(2), "bicyclo");
        assert_eq!(von_baeyer_cycle_count(Some("bicyclo[2.2.1]")), Some(2));
        assert_eq!(
            render_von_baeyer_descriptor(2, "[3.2.2.0^{1,5}]"),
            "tricyclo[3.2.2.0^{1,5}]"
        );
        assert!(is_von_baeyer_descriptor(Some("undecacyclo[1.1.1]")));
    }
}
