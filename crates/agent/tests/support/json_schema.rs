//! A test-only JSON Schema 2020-12 validator for exactly the keywords the
//! hand-written agent schemas use. Any other keyword fails `check_keywords`,
//! so a schema can never silently rely on a keyword this subset ignores.
//!
//! Semantics follow the specification, not serde_json equality: `integer`
//! accepts 1.0, `enum` and `const` compare numbers mathematically (30 == 30.0),
//! and `minLength`/`maxLength` count Unicode code points.
use serde_json::{Map, Number, Value};
use std::{cmp::Ordering, fmt};

/// The supported keywords. `description` is accepted and ignored.
const KEYWORDS: &[&str] = &[
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "anyOf",
    "pattern",
    "description",
];

const TYPES: &[&str] = &[
    "null", "boolean", "object", "array", "number", "string", "integer",
];

type Matcher = fn(&str) -> bool;

/// The only `pattern` strings a schema may use, each with a hand-written
/// matcher (object IDs, revisions and document handles).
const PATTERNS: &[(&str, Matcher)] = &[
    ("^[1-9][0-9]{0,19}$", object_id),
    ("^(0|[1-9][0-9]{0,19})$", revision),
    ("^[A-Za-z0-9_-]{1,64}$", handle),
];

/// One problem, located by a JSON pointer: into the schema for
/// `check_keywords`, into the instance for `validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub pointer: String,
    pub message: String,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pointer = if self.pointer.is_empty() {
            "(root)"
        } else {
            &self.pointer
        };
        write!(f, "{pointer}: {}", self.message)
    }
}

fn error(pointer: &str, message: impl Into<String>) -> Error {
    Error {
        pointer: pointer.into(),
        message: message.into(),
    }
}

/// RFC 6901: `~` becomes `~0` and `/` becomes `~1`.
fn child(pointer: &str, token: &str) -> String {
    format!("{pointer}/{}", token.replace('~', "~0").replace('/', "~1"))
}

/// Every keyword-allowlist problem of `schema`. Empty when the schema uses
/// only the supported keywords with well-formed values.
pub fn check_keywords(schema: &Value) -> Vec<Error> {
    let mut errors = Vec::new();
    check_schema(schema, "", &mut errors);
    errors
}

fn check_schema(schema: &Value, at: &str, errors: &mut Vec<Error>) {
    let Some(schema) = schema.as_object() else {
        errors.push(error(at, "a subschema must be an object"));
        return;
    };
    for (keyword, value) in schema {
        let here = child(at, keyword);
        let bad = |message: &str| error(&here, message);
        match keyword.as_str() {
            "type" => {
                let valid = |v: &Value| v.as_str().is_some_and(|t| TYPES.contains(&t));
                let ok = match value {
                    Value::Array(types) => !types.is_empty() && types.iter().all(valid),
                    other => valid(other),
                };
                if !ok {
                    errors.push(bad("type must name JSON Schema types"));
                }
            }
            "enum" if !value.as_array().is_some_and(|v| !v.is_empty()) => {
                errors.push(bad("enum must be a non-empty array"));
            }
            "properties" => match value.as_object() {
                Some(properties) => {
                    for (name, sub) in properties {
                        check_schema(sub, &child(&here, name), errors);
                    }
                }
                None => errors.push(bad("properties must be an object")),
            },
            "required"
                if !value
                    .as_array()
                    .is_some_and(|v| v.iter().all(Value::is_string)) =>
            {
                errors.push(bad("required must be an array of strings"));
            }
            "additionalProperties" if value != &Value::Bool(false) => {
                errors.push(bad("only additionalProperties: false is supported"));
            }
            "items" => check_schema(value, &here, errors),
            "minItems" | "maxItems" | "minLength" | "maxLength" if !value.is_u64() => {
                errors.push(bad("must be a non-negative integer"));
            }
            "minimum" | "maximum" if !value.is_number() => {
                errors.push(bad("must be a number"));
            }
            "anyOf" => match value.as_array() {
                Some(branches) if !branches.is_empty() => {
                    for (i, sub) in branches.iter().enumerate() {
                        check_schema(sub, &child(&here, &i.to_string()), errors);
                    }
                }
                _ => errors.push(bad("anyOf must be a non-empty array")),
            },
            "pattern" if matcher(value).is_none() => {
                errors.push(bad("pattern is not in the allowlisted pattern table"));
            }
            "description" if !value.is_string() => {
                errors.push(bad("description must be a string"));
            }
            other if !KEYWORDS.contains(&other) => {
                errors.push(bad("unsupported keyword"));
            }
            _ => {}
        }
    }
}

fn matcher(pattern: &Value) -> Option<Matcher> {
    let pattern = pattern.as_str()?;
    PATTERNS
        .iter()
        .find(|(known, _)| *known == pattern)
        .map(|(_, matches)| *matches)
}

/// `^[1-9][0-9]{0,19}$`
fn object_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    matches!(bytes.first(), Some(b'1'..=b'9'))
        && bytes.len() <= 20
        && bytes.iter().all(u8::is_ascii_digit)
}

/// `^(0|[1-9][0-9]{0,19})$`
fn revision(text: &str) -> bool {
    text == "0" || object_id(text)
}

/// `^[A-Za-z0-9_-]{1,64}$`
fn handle(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Every reason `instance` fails `schema`, each at its instance pointer.
/// The schema must pass `check_keywords`; anything else panics.
pub fn validate(schema: &Value, instance: &Value) -> Vec<Error> {
    let mut errors = Vec::new();
    validate_at(schema, instance, "", &mut errors);
    errors
}

fn validate_at(schema: &Value, instance: &Value, at: &str, errors: &mut Vec<Error>) {
    let schema = schema
        .as_object()
        .unwrap_or_else(|| panic!("{at}: unsupported subschema {schema}"));
    for (keyword, value) in schema {
        match keyword.as_str() {
            "type" => {
                let names: Vec<&str> = match value {
                    Value::Array(types) => types.iter().filter_map(Value::as_str).collect(),
                    other => other.as_str().into_iter().collect(),
                };
                if !names.iter().any(|name| has_type(name, instance)) {
                    errors.push(error(at, format!("expected type {value}, got {instance}")));
                }
            }
            "enum" => {
                let options = value.as_array().map(Vec::as_slice).unwrap_or_default();
                if !options.iter().any(|option| equal(option, instance)) {
                    errors.push(error(at, format!("{instance} is not one of {value}")));
                }
            }
            "const" if !equal(value, instance) => {
                errors.push(error(at, format!("{instance} is not {value}")));
            }
            "properties" | "additionalProperties" | "required" => {
                if let Some(object) = instance.as_object() {
                    object_keyword(keyword, value, schema, object, at, errors);
                }
            }
            "items" => {
                for (i, item) in instance.as_array().into_iter().flatten().enumerate() {
                    validate_at(value, item, &child(at, &i.to_string()), errors);
                }
            }
            "minItems" | "maxItems" => {
                if let Some(items) = instance.as_array() {
                    bound(keyword, value, items.len(), "items", at, errors);
                }
            }
            "minLength" | "maxLength" => {
                if let Some(text) = instance.as_str() {
                    let points = text.chars().count();
                    bound(keyword, value, points, "code points", at, errors);
                }
            }
            "minimum" | "maximum" => {
                if let (Value::Number(n), Value::Number(limit)) = (instance, value) {
                    let order = compare(n, limit);
                    if (keyword == "minimum" && order == Ordering::Less)
                        || (keyword == "maximum" && order == Ordering::Greater)
                    {
                        errors.push(error(at, format!("{n} violates {keyword} {limit}")));
                    }
                }
            }
            "anyOf" => {
                let branches = value.as_array().map(Vec::as_slice).unwrap_or_default();
                let failures: Vec<Vec<Error>> = branches
                    .iter()
                    .map(|branch| {
                        let mut found = Vec::new();
                        validate_at(branch, instance, at, &mut found);
                        found
                    })
                    .collect();
                if failures.iter().all(|f| !f.is_empty()) {
                    let detail: Vec<String> = failures
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            let all: Vec<String> = f.iter().map(ToString::to_string).collect();
                            format!("branch {i}: {}", all.join(", "))
                        })
                        .collect();
                    errors.push(error(
                        at,
                        format!("matches no anyOf branch ({})", detail.join("; ")),
                    ));
                }
            }
            "pattern" => {
                let matches = matcher(value)
                    .unwrap_or_else(|| panic!("{at}: pattern {value} is not allowlisted"));
                if let Some(text) = instance.as_str()
                    && !matches(text)
                {
                    errors.push(error(at, format!("{instance} does not match {value}")));
                }
            }
            "const" | "description" => {}
            other => panic!("{at}: unsupported keyword {other}"),
        }
    }
}

fn object_keyword(
    keyword: &str,
    value: &Value,
    schema: &Map<String, Value>,
    object: &Map<String, Value>,
    at: &str,
    errors: &mut Vec<Error>,
) {
    let properties = schema.get("properties").and_then(Value::as_object);
    match keyword {
        "properties" => {
            for (name, sub) in properties.into_iter().flatten() {
                if let Some(member) = object.get(name) {
                    validate_at(sub, member, &child(at, name), errors);
                }
            }
        }
        "required" => {
            for name in value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !object.contains_key(name) {
                    errors.push(error(at, format!("missing required property {name:?}")));
                }
            }
        }
        _ => {
            for name in object.keys() {
                if properties.is_none_or(|p| !p.contains_key(name)) {
                    errors.push(error(&child(at, name), "additional property not allowed"));
                }
            }
        }
    }
}

fn bound(
    keyword: &str,
    limit: &Value,
    actual: usize,
    unit: &str,
    at: &str,
    errors: &mut Vec<Error>,
) {
    let limit = limit
        .as_u64()
        .and_then(|l| usize::try_from(l).ok())
        .unwrap_or(usize::MAX);
    let ok = if keyword.starts_with("min") {
        actual >= limit
    } else {
        actual <= limit
    };
    if !ok {
        errors.push(error(
            at,
            format!("{actual} {unit} violates {keyword} {limit}"),
        ));
    }
}

fn has_type(name: &str, instance: &Value) -> bool {
    match (name, instance) {
        ("null", Value::Null)
        | ("boolean", Value::Bool(_))
        | ("object", Value::Object(_))
        | ("array", Value::Array(_))
        | ("number", Value::Number(_))
        | ("string", Value::String(_)) => true,
        ("integer", Value::Number(n)) => match exact(n) {
            Exact::Integer(_) => true,
            Exact::Float(f) => f.fract() == 0.,
        },
        _ => false,
    }
}

/// JSON value equality with mathematical number comparison.
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => compare(a, b) == Ordering::Equal,
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}

enum Exact {
    Integer(i128),
    Float(f64),
}

/// serde_json stores u64, i64 or a finite f64; integers stay exact in i128.
fn exact(n: &Number) -> Exact {
    if let Some(u) = n.as_u64() {
        Exact::Integer(i128::from(u))
    } else if let Some(i) = n.as_i64() {
        Exact::Integer(i128::from(i))
    } else {
        Exact::Float(n.as_f64().expect("serde_json numbers are finite"))
    }
}

/// Exact numeric order, also between integers and floats beyond 2^53.
fn compare(a: &Number, b: &Number) -> Ordering {
    match (exact(a), exact(b)) {
        (Exact::Integer(a), Exact::Integer(b)) => a.cmp(&b),
        // `total_cmp` orders -0.0 below 0.0; mathematically they are equal.
        (Exact::Float(a), Exact::Float(b)) if a == b => Ordering::Equal,
        (Exact::Float(a), Exact::Float(b)) => a.total_cmp(&b),
        (Exact::Integer(a), Exact::Float(b)) => integer_vs_float(a, b),
        (Exact::Float(a), Exact::Integer(b)) => integer_vs_float(b, a).reverse(),
    }
}

fn integer_vs_float(integer: i128, float: f64) -> Ordering {
    // Beyond ±2^127 every i128 lies on one side.
    const LIMIT: f64 = 1.7014118346046923e38;
    if float >= LIMIT {
        return Ordering::Less;
    }
    if float < -LIMIT {
        return Ordering::Greater;
    }
    let floor = float.floor();
    // `floor` is integral and within i128, so the cast is exact.
    match integer.cmp(&(floor as i128)) {
        Ordering::Equal if float > floor => Ordering::Less,
        order => order,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn accepts(schema: Value, instance: Value) -> bool {
        assert_eq!(check_keywords(&schema), [], "{schema}");
        validate(&schema, &instance).is_empty()
    }

    #[test]
    fn integer_accepts_whole_floats_only() {
        let integer = json!({"type":"integer"});
        assert!(accepts(integer.clone(), json!(1)));
        assert!(accepts(integer.clone(), json!(1.0)));
        assert!(accepts(integer.clone(), json!(-3.0)));
        assert!(accepts(integer.clone(), json!(1e20)));
        assert!(!accepts(integer.clone(), json!(1.5)));
        assert!(!accepts(integer, json!("1")));
        assert!(accepts(json!({"type":"number"}), json!(1)));
        assert!(accepts(json!({"type":["number","null"]}), json!(null)));
        assert!(!accepts(json!({"type":["number","null"]}), json!(false)));
    }

    #[test]
    fn enum_and_const_compare_numbers_mathematically() {
        let rotation = json!({"type":"number","enum":[0,30,-30]});
        assert!(accepts(rotation.clone(), json!(30)));
        assert!(accepts(rotation.clone(), json!(30.0)));
        assert!(accepts(rotation.clone(), json!(-30.0)));
        assert!(!accepts(rotation.clone(), json!(30.0004)));
        assert!(!accepts(rotation, json!("30")));
        assert!(accepts(
            json!({"const":[1,{"a":2}]}),
            json!([1.0,{"a":2.0}])
        ));
        assert!(!accepts(json!({"const":{"a":2}}), json!({"a":2,"b":1})));
        assert!(accepts(json!({"enum":["a",null]}), json!(null)));
    }

    #[test]
    fn signed_zeros_are_equal() {
        let negative: Value = serde_json::from_str("-0").expect("valid JSON");
        assert_eq!(negative, json!(-0.0), "serde_json parses -0 as a float");
        for zero in [json!(0), json!(0.0)] {
            for (schema, instance) in [(zero.clone(), negative.clone()), (negative.clone(), zero)] {
                assert!(accepts(json!({"const":schema}), instance.clone()));
                assert!(accepts(json!({"enum":[schema]}), instance.clone()));
                assert!(accepts(json!({"minimum":schema}), instance.clone()));
                assert!(accepts(json!({"maximum":schema}), instance));
            }
        }
    }

    #[test]
    fn numbers_compare_exactly_across_representations() {
        let ordered = [
            json!(-1e300),
            json!(i64::MIN),
            json!(-1.5),
            json!(-1),
            json!(0),
            json!(0.5),
            json!(1),
            json!(9007199254740993_u64),
            json!(9007199254740994.0),
            json!(u64::MAX),
            json!(1e300),
        ];
        for (i, a) in ordered.iter().enumerate() {
            for (j, b) in ordered.iter().enumerate() {
                let (Value::Number(a), Value::Number(b)) = (a, b) else {
                    unreachable!()
                };
                assert_eq!(compare(a, b), i.cmp(&j), "{a} vs {b}");
            }
        }
    }

    #[test]
    fn minimum_and_maximum_are_inclusive() {
        let range = json!({"type":"number","minimum":-50,"maximum":50});
        assert!(accepts(range.clone(), json!(50)));
        assert!(accepts(range.clone(), json!(-50.0)));
        assert!(!accepts(range.clone(), json!(50.000001)));
        assert!(!accepts(range.clone(), json!(-51)));
        let bounds = json!({"minimum":-50,"maximum":50});
        assert!(accepts(bounds, json!("50000")), "bounds ignore non-numbers");
        let id = json!({"type":"integer","minimum":1});
        assert!(accepts(id.clone(), json!(u64::MAX)));
        assert!(!accepts(id, json!(0)));
    }

    #[test]
    fn lengths_count_code_points() {
        let short = json!({"type":"string","minLength":2,"maxLength":3});
        assert!(accepts(short.clone(), json!("ééé")));
        assert!(accepts(short.clone(), json!("日本")));
        assert!(!accepts(short.clone(), json!("éééé")));
        assert!(!accepts(short.clone(), json!("é")));
        let lengths = json!({"minLength":2,"maxLength":3});
        assert!(accepts(lengths, json!(12345)), "lengths ignore non-strings");
        let items = json!({"type":"array","minItems":1,"maxItems":2});
        assert!(accepts(items.clone(), json!([1, 2])));
        assert!(!accepts(items.clone(), json!([])));
        assert!(!accepts(items, json!([1, 2, 3])));
    }

    #[test]
    fn objects_check_properties_required_and_extras() {
        let point = json!({"type":"object","additionalProperties":false,"properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]});
        assert!(accepts(point.clone(), json!({"x":1,"y":2})));
        let errors = validate(&point, &json!({"x":"1","z/~":0}));
        let found: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert_eq!(
            found,
            [
                "/z~1~0: additional property not allowed",
                "/x: expected type \"number\", got \"1\"",
                "(root): missing required property \"y\"",
            ]
        );
    }

    #[test]
    fn items_and_any_of_report_every_error_with_pointers() {
        let schema = json!({"type":"array","items":{"anyOf":[{"type":"null"},{"type":"integer","minimum":0}]}});
        assert!(accepts(schema.clone(), json!([null, 0, 7])));
        let errors = validate(&schema, &json!([null, -1, "a"]));
        let pointers: Vec<&str> = errors.iter().map(|e| e.pointer.as_str()).collect();
        assert_eq!(pointers, ["/1", "/2"]);
        assert_eq!(
            errors[0].message,
            r#"matches no anyOf branch (branch 0: /1: expected type "null", got -1; branch 1: /1: -1 violates minimum 0)"#
        );
        let pair = json!({"anyOf":[{"type":"null"},{"properties":{"a":{"type":"string"},"b":{"type":"string"}}}]});
        let errors = validate(&pair, &json!({"a":0,"b":0}));
        let found: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert_eq!(
            found,
            [
                r#"(root): matches no anyOf branch (branch 0: (root): expected type "null", got {"a":0,"b":0}; branch 1: /a: expected type "string", got 0, /b: expected type "string", got 0)"#
            ]
        );
    }

    #[test]
    fn pattern_table_matchers_follow_their_regexes() {
        let id = json!({"type":"string","pattern":"^[1-9][0-9]{0,19}$"});
        for (text, ok) in [
            ("1", true),
            ("18446744073709551615", true),
            ("99999999999999999999", true),
            ("100000000000000000000", false),
            ("0", false),
            ("01", false),
            ("", false),
            ("1a", false),
            ("1\n", false),
            ("１", false),
        ] {
            assert_eq!(accepts(id.clone(), json!(text)), ok, "object id {text:?}");
        }
        let revision = json!({"type":"string","pattern":"^(0|[1-9][0-9]{0,19})$"});
        for (text, ok) in [("0", true), ("7", true), ("00", false), ("-1", false)] {
            assert_eq!(
                accepts(revision.clone(), json!(text)),
                ok,
                "revision {text:?}"
            );
        }
        let handle = json!({"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"});
        for (text, ok) in [
            ("a", true),
            ("Doc_1-x", true),
            (&"z".repeat(64), true),
            (&"z".repeat(65), false),
            ("", false),
            ("a b", false),
            ("a.b", false),
            ("é", false),
        ] {
            assert_eq!(accepts(handle.clone(), json!(text)), ok, "handle {text:?}");
        }
        let pattern = json!({"pattern":"^[A-Za-z0-9_-]{1,64}$"});
        assert!(accepts(pattern, json!(42)), "pattern ignores non-strings");
    }

    #[test]
    fn the_keyword_allowlist_rejects_everything_else() {
        assert_eq!(
            check_keywords(
                &json!({"type":"object","description":"d","properties":{"a":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"}}})
            ),
            []
        );
        let rejected = [
            json!({"type":"string","pattern":"^[a-z]+$"}),
            json!({"type":"object","additionalProperties":true}),
            json!({"type":"object","additionalProperties":{"type":"string"}}),
            json!({"type":"array","uniqueItems":true}),
            json!({"oneOf":[{"type":"null"}]}),
            json!({"$schema":"https://json-schema.org/draft/2020-12/schema"}),
            json!({"type":"float"}),
            json!({"type":"array","items":true}),
            json!({"properties":{"a":{"type":"number","exclusiveMinimum":0}}}),
            json!({"anyOf":[]}),
            json!({"minLength":-1}),
        ];
        for schema in rejected {
            assert_ne!(check_keywords(&schema), [], "{schema}");
        }
        let nested = check_keywords(&json!({"properties":{"a/b":{"format":"uri"}}}));
        assert_eq!(nested[0].pointer, "/properties/a~1b/format");
    }
}
