//! Phane failures from upstream uninterpretable.txt, commits 836038e0..a37ddaf6.
//! Successful phanes are covered by the complete frozen structure oracle.
use opsin::{ParseOptions, Parser, Status};

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
