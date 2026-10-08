use super::*;
use crate::fake_host::SPECS;
use serde_json::json;

fn names(catalog: &Catalog) -> Vec<&str> {
    catalog
        .tools()
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect()
}

#[test]
fn tools_keep_the_host_order_and_schemas() {
    let catalog = Catalog::new(SPECS).unwrap();
    let expected: Vec<&str> = SPECS.iter().map(|spec| spec.name).collect();
    assert_eq!(names(&catalog), expected);
    for (tool, spec) in catalog.tools().iter().zip(SPECS) {
        assert_eq!(
            Value::Object(tool.input_schema.as_ref().clone()),
            (spec.input_schema)(),
            "{}",
            spec.name
        );
        assert_eq!(tool.description.as_deref(), Some(spec.description));
        assert_eq!(tool.title.as_deref(), spec.title);
        assert!(catalog.contains(spec.name));
    }
    assert!(!catalog.contains("no_such_tool"));
    assert!(!catalog.contains(""));
}

#[test]
fn a_tool_serializes_with_every_hint_and_no_output_schema() {
    let catalog = Catalog::new(SPECS).unwrap();
    let echo = serde_json::to_value(&catalog.tools()[0]).unwrap();
    assert_eq!(
        echo,
        json!({
            "name": "echo",
            "title": "Echo",
            "description": "Returns its arguments.",
            "inputSchema": {"type": "object"},
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false,
            },
        })
    );
}

#[test]
fn annotations_follow_the_hints() {
    let catalog = Catalog::new(SPECS).unwrap();
    let annotations: Vec<(&str, Value)> = catalog
        .tools()
        .iter()
        .map(|tool| {
            let json = serde_json::to_value(tool).unwrap();
            (tool.name.as_ref(), json["annotations"].clone())
        })
        .collect();
    let hinted = |read_only, destructive, idempotent, open_world| {
        json!({
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "idempotentHint": idempotent,
            "openWorldHint": open_world,
        })
    };
    for (name, expected) in [
        ("echo", hinted(true, false, true, false)),
        ("png", hinted(false, true, false, true)),
        ("pdf", hinted(false, false, true, false)),
        ("svg", hinted(true, true, false, true)),
        ("slow", Value::Null),
        ("fail_busy", Value::Null),
    ] {
        let found = annotations.iter().find(|(tool, _)| *tool == name).unwrap();
        assert_eq!(found.1, expected, "{name}");
    }
}

/// The operation catalog itself is valid, maps every hint and publishes no
/// output schema.
#[test]
fn the_operation_catalog_is_served_as_specified() {
    let specs = reshiki_agent::ops::catalog::SPECS;
    let catalog = Catalog::new(specs).unwrap();
    let expected: Vec<&str> = specs.iter().map(|spec| spec.name).collect();
    assert_eq!(names(&catalog), expected);
    for (tool, spec) in catalog.tools().iter().zip(specs) {
        let json = serde_json::to_value(tool).unwrap();
        assert!(json.get("outputSchema").is_none(), "{}", spec.name);
        assert_eq!(json["inputSchema"], (spec.input_schema)(), "{}", spec.name);
        let hints = spec.hints.unwrap();
        assert_eq!(
            json["annotations"],
            json!({
                "readOnlyHint": hints.read_only,
                "destructiveHint": hints.destructive,
                "idempotentHint": hints.idempotent,
                "openWorldHint": hints.open_world,
            }),
            "{}",
            spec.name
        );
    }
}

#[test]
fn no_tool_has_an_output_schema() {
    let catalog = Catalog::new(SPECS).unwrap();
    for tool in catalog.tools() {
        assert_eq!(tool.output_schema, None, "{}", tool.name);
        let json = serde_json::to_value(tool).unwrap();
        assert!(json.get("outputSchema").is_none(), "{json}");
    }
}

fn object() -> Value {
    json!({"type": "object"})
}

fn string_schema() -> Value {
    json!({"type": "string"})
}

fn untyped() -> Value {
    json!({"properties": {}})
}

fn array() -> Value {
    json!([{"type": "object"}])
}

const fn spec(name: &'static str, input_schema: fn() -> Value) -> ToolSpec {
    ToolSpec {
        name,
        title: None,
        description: "",
        input_schema,
        hints: None,
    }
}

static DUPLICATE: [ToolSpec; 3] = [spec("a", object), spec("b", object), spec("a", object)];
static EMPTY_NAME: [ToolSpec; 2] = [spec("a", object), spec("", object)];
static BAD_CHARACTER: [ToolSpec; 1] = [spec("a b", object)];
static SLASH: [ToolSpec; 1] = [spec("tools/a", object)];
static STRING_SCHEMA: [ToolSpec; 2] = [spec("a", object), spec("b", string_schema)];
static UNTYPED_SCHEMA: [ToolSpec; 1] = [spec("a", untyped)];
static ARRAY_SCHEMA: [ToolSpec; 1] = [spec("a", array)];

#[test]
fn invalid_catalogs_are_refused() {
    let long: &'static str = Box::leak("t".repeat(129).into_boxed_str());
    let too_long: &'static [ToolSpec] = Box::leak(Box::new([spec(long, object)]));
    let longest: &'static str = Box::leak("t".repeat(128).into_boxed_str());
    let fits: &'static [ToolSpec] = Box::leak(Box::new([spec(longest, object)]));
    for (specs, expected) in [
        (&DUPLICATE[..], CatalogError::Duplicate(2)),
        (&EMPTY_NAME, CatalogError::InvalidName(1)),
        (&BAD_CHARACTER, CatalogError::InvalidName(0)),
        (&SLASH, CatalogError::InvalidName(0)),
        (too_long, CatalogError::InvalidName(0)),
        (&STRING_SCHEMA, CatalogError::Schema(1)),
        (&UNTYPED_SCHEMA, CatalogError::Schema(0)),
        (&ARRAY_SCHEMA, CatalogError::Schema(0)),
    ] {
        assert_eq!(Catalog::new(specs).err(), Some(expected));
        assert_eq!(expected.index(), Catalog::new(specs).err().unwrap().index());
    }
    assert!(Catalog::new(fits).is_ok());
    assert!(Catalog::new(&[]).unwrap().tools().is_empty());
}
