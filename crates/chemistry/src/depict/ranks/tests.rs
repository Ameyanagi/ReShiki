use super::*;
#[test]
fn string_unsigned_conversion_preserves_native_right_trim_and_wrapping() {
    for (text, expected) in [
        ("0", 0),
        ("+1", 1),
        ("-1", u32::MAX),
        ("4294967295", u32::MAX),
        ("-4294967295", 1),
        ("01", 1),
        ("1 \t\r\n\x0b\x0c", 1),
        ("-0", 0),
    ] {
        assert_eq!(
            Property::from_bytes(text.as_bytes()),
            Property::Value(expected),
            "{text:?}"
        );
    }
    for text in [
        "",
        " ",
        " 1",
        "1.0",
        "1e0",
        "4294967296",
        "-4294967296",
        "++1",
        "1\0",
        "1\u{a0}",
    ] {
        assert_eq!(
            Property::from_bytes(text.as_bytes()),
            Property::Invalid,
            "{text:?}"
        );
    }
}
