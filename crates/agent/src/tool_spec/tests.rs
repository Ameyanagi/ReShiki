use super::valid_name;
use crate::canvas_tools::SPECS;

#[test]
fn canvas_tool_names_are_valid_and_unique() {
    for (index, spec) in SPECS.iter().enumerate() {
        assert!(valid_name(spec.name), "{}", spec.name);
        assert!(
            SPECS[..index].iter().all(|other| other.name != spec.name),
            "duplicate {}",
            spec.name
        );
    }
}

#[test]
fn canvas_tool_input_schemas_are_objects() {
    for spec in SPECS {
        assert_eq!((spec.input_schema)()["type"], "object", "{}", spec.name);
    }
}

#[test]
fn valid_name_follows_the_mcp_rule() {
    assert!(valid_name("a"));
    assert!(valid_name("A.z_0-9"));
    assert!(valid_name(&"x".repeat(128)));
    assert!(!valid_name(""));
    assert!(!valid_name(&"x".repeat(129)));
    for name in ["a b", "a/b", "a:b", "é"] {
        assert!(!valid_name(name), "{name}");
    }
}
