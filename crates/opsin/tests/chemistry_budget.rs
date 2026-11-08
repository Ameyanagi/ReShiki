// SPDX-License-Identifier: MIT
// Origin: original nazuchem/ReShiki code; source history and separately attributed data/API context follow.
//   ReShiki, crates/opsin/tests/chemistry_budget.rs @ 777cdd3f98dfb955e165b17e352d3ce6da768b0c.

use opsin::{Budget, ParseOptions, Parser, Status};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn supplied_budget_reaches_axial_and_cage_perception() {
    let unbounded = Parser::new().unwrap();
    for name in [
        "(1M)-[1,1'-binaphthalene]-2,2'-diol",
        "bicyclo[2.2.1]heptan-2-exo-ol",
    ] {
        let expected = unbounded.parse(name, &ParseOptions::strict());
        assert_eq!(
            expected.status,
            Status::Success,
            "{name}: {}",
            expected.message
        );

        let reservations = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&reservations);
        let budget = Budget::new(usize::MAX, usize::MAX, None, usize::MAX, move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            None
        });
        // Parser clones must preserve the host's reservation policy.
        let parser = Parser::with_budget(budget).unwrap().clone();
        let result = parser.parse(name, &ParseOptions::strict());
        assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
        assert!(reservations.load(Ordering::SeqCst) > 0, "{name}");
        assert!(
            result.message.contains("resources are busy"),
            "{name}: {}",
            result.message
        );
    }
}
