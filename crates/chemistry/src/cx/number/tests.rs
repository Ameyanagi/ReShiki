use super::{Reader, parse_for, valid_for};

#[test]
fn linux_coordinate_syntax_and_underflow_match_native_reader() {
    for (text, bits) in [
        ("1e-9999", 0),
        ("-1e-9999", 1u64 << 63),
        ("1e-308", 2_024_022_533_073_106),
        ("5e-324", 1),
    ] {
        assert_eq!(parse_for(text, Reader::Linux).map(f64::to_bits), Some(bits));
    }
    for text in ["0x0", "0x1p0", "-0x1p-1074", "1e309"] {
        assert!(!valid_for(text, Reader::Linux), "{text}");
    }
}

#[test]
fn windows_hexadecimal_boundary_matches_observed_native_bits() {
    for (text, bits) in [
        ("0x0.fffffffffffff8p-1022", 13_510_798_882_111_488),
        ("0x1.fffffffffffff7p-1023", 18_014_398_509_481_984),
        ("0x1.fffffffffffff8p-1023", 18_014_398_509_481_984),
        ("0x1.ffffffffffffffffp-1023", 18_014_398_509_481_984),
    ] {
        assert_eq!(
            parse_for(text, Reader::Windows).map(f64::to_bits),
            Some(bits)
        );
        assert_eq!(
            parse_for(text, Reader::Mac).map(f64::to_bits),
            Some(f64::MIN_POSITIVE.to_bits())
        );
    }
}

#[test]
fn native_underflow_rules_distinguish_windows_and_mac() {
    for input in [
        "1e-308",
        "5e-324",
        "0x1.1p-1075",
        "0x1.8p-1074",
        "0x1.123456789abcdefp-1023",
    ] {
        assert!(valid_for(input, Reader::Windows), "Windows: {input}");
        assert!(!valid_for(input, Reader::Mac), "macOS: {input}");
    }
    for input in [
        "0x1p-1075",
        "0x1p-1076",
        "1e-9999",
        "1e309",
        "0x1p1024",
        "0x1.fffffffffffff8p1023",
        "bad",
    ] {
        assert!(!valid_for(input, Reader::Windows), "Windows: {input}");
        assert!(!valid_for(input, Reader::Mac), "macOS: {input}");
    }
    for input in [
        "0x1p-1074",
        "0x0.fffffffffffff8p-1022",
        "0x1p-1022",
        "0x1.fffffffffffffp1023",
        "0e-9999",
    ] {
        assert!(valid_for(input, Reader::Windows), "Windows: {input}");
        assert!(valid_for(input, Reader::Mac), "macOS: {input}");
    }
}
