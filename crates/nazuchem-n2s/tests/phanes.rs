// SPDX-License-Identifier: MIT
// Origin: adapted
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/PhaneAmplifier.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: ReShiki | crates/opsin/tests/phanes.rs | 0a3f60f3e4238b329a870c39d9e042d30822d3cf
// Source: OPSIN | opsin-core/src/test/resources/uk/ac/cam/ch/wwmm/opsin/uninterpretable.txt | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Copyright 2017 Daniel Lowe
// Changes: Rust graph/tree construction and regression harness around source-derived cases/assertions; local regressions retained.

//! Phane failures from upstream uninterpretable.txt, commits 836038e0..a37ddaf6.
//! Successful phanes are covered by the complete frozen structure oracle.
use nazuchem_n2s::{ParseOptions, Parser, Status};

#[test]
fn upstream_uninterpretable_phane_names_fail() {
    let parser = Parser::new().unwrap();
    let mut count = 0;
    for name in include_str!("fixtures/uninterpretable.txt")
        .lines()
        .filter(|name| name.contains("phane"))
    {
        let result = parser.parse(name, &ParseOptions::default());
        assert_eq!(result.status, Status::Failure, "{name}: {}", result.message);
        count += 1;
    }
    assert_eq!(count, 21);
}
