use super::*;
use crate::chemistry::smiles;

#[test]
#[ignore = "isolated requested-Rust-allocation and callback refresh timing measurement"]
fn measure_toolkit_refresh() {
    use crate::allocation_metrics;
    use std::{hint::black_box, time::Instant};
    let state = smiles::prepare(&"C".repeat(1024)).unwrap().state;
    let molecule = to_adapter(&state, vec![], &[]).unwrap();
    for cached in [false, true] {
        let mut toolkit = Toolkit {
            state: cached.then(|| state.clone()),
            unspecified: vec![],
        };
        let baseline = allocation_metrics::reset();
        let start = Instant::now();
        for _ in 0..100 {
            black_box(toolkit.refresh(&molecule).unwrap());
        }
        let elapsed = start.elapsed();
        let snapshot = allocation_metrics::snapshot();
        println!(
            "cached={cached} atoms=1024 count=100 elapsed={elapsed:?} allocations={} allocated={} peak_extra={} retained_extra={}",
            snapshot.allocation_count,
            snapshot.allocated_bytes,
            snapshot.peak_bytes.saturating_sub(baseline),
            snapshot.live_bytes.saturating_sub(baseline)
        );
    }
}

#[test]
fn rust_kernel_generates_methane_without_reference_dependencies() {
    let imported = read("InChI=1S/CH4/h1H4", output::Options::default()).unwrap();
    let molecule = Molecule::prepare(&imported.state.unwrap(), None).unwrap();
    let result = generate(&molecule).unwrap();
    assert_eq!(result.inchi, "InChI=1S/CH4/h1H4");
    assert_eq!(result.status, 0);
}

#[test]
fn unchanged_graph_preserves_prepared_caches() {
    let state = smiles::prepare("C1CC1.C").unwrap().state;
    let molecule = to_adapter(&state, vec![], &[]).unwrap();
    let mut toolkit = Toolkit {
        state: Some(state.clone()),
        unspecified: vec![],
    };
    assert_eq!(
        serde_json::to_value(toolkit.refresh(&molecule).unwrap()).unwrap(),
        serde_json::to_value(state).unwrap()
    );
}

#[test]
fn cleanup_graph_edits_with_unchanged_counts_invalidate_computed_caches() {
    // Rewire a cycle into a chain, change bond orders, and reorder unlike atoms.
    // Each replacement keeps the sizes checked by the former reuse condition.
    for (before, after) in [("C1CC1.C", "CCCC"), ("C=CC", "CCC"), ("CCO", "OCC")] {
        let previous = smiles::prepare(before).unwrap().state;
        let replacement = smiles::prepare(after).unwrap().state;
        assert_eq!(previous.graph.atoms.len(), replacement.graph.atoms.len());
        assert_eq!(previous.graph.bonds.len(), replacement.graph.bonds.len());
        let mut molecule = to_adapter(&replacement, vec![], &[]).unwrap();
        let mut toolkit = Toolkit {
            state: Some(previous),
            unspecified: vec![],
        };
        toolkit.synchronize_after_cleanup(&molecule).unwrap();
        let refreshed = toolkit.state.as_ref().unwrap();
        assert_eq!(refreshed.rings.kind, RingKind::None, "{before} -> {after}");
        assert!(refreshed.rings.atoms.is_empty());
        assert!(refreshed.properties.done.is_none());
        assert!(refreshed.conjugated.iter().all(|value| !value));
        assert!(
            refreshed
                .hybridizations
                .iter()
                .all(|value| *value == Hybridization::Unspecified)
        );

        // A later callback must perceive the replacement graph's rings. This
        // checks the adapter contract, not the current kernel's callback order.
        toolkit.kekulize(&mut molecule, false).unwrap();
        let updated = toolkit.state.as_ref().unwrap();
        assert_eq!(updated.rings.kind, RingKind::Basis);
        assert!(updated.rings.atoms.is_empty());
    }
}
