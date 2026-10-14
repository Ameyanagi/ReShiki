use super::*;
use crate::ops::{error::ErrorKind, wire::ObjectId};
use serde::Serializer;

fn versions() -> Versions {
    Versions {
        app: "9.8.7".into(),
        operation_api: 1,
        engine_protocol: 2,
        document: 3,
    }
}

#[derive(Serialize)]
struct Selected {
    ids: Vec<ObjectId>,
}

#[test]
fn valid_preserves_compact_envelope_bytes_and_warning_order() {
    const JSON: &str = r#"{"validation":{"status":"valid"},"value":{"ids":["9007199254740993"]},"versions":{"app":"9.8.7","document":3,"engine_protocol":2,"operation_api":1},"warnings":[{"message":"first"},{"message":"second"}]}"#;
    let result = ToolResult::valid(
        Selected {
            ids: vec![ObjectId(9_007_199_254_740_993)],
        },
        warnings(vec!["first".into(), "second".into()]),
        versions(),
    );
    assert!(!result.is_error);
    assert!(result.images.is_empty() && result.files.is_empty());
    assert_eq!(serde_json::to_string(&result.value).unwrap(), JSON,);
}

struct Unencodable;

impl Serialize for Unencodable {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("no JSON form"))
    }
}

#[test]
fn valid_keeps_the_existing_encoding_failure_warning() {
    let result = ToolResult::valid(Unencodable, warnings(vec!["first".into()]), versions());
    assert!(!result.is_error);
    assert_eq!(result.value["value"], Value::Null);
    assert_eq!(
        result.value["warnings"],
        json!([
            {"message": "first"},
            {"message": "The result could not be encoded: no JSON form"},
        ]),
    );
}

#[test]
fn rejected_preserves_compact_error_envelope_bytes() {
    const JSON: &str = r#"{"error":{"code":"rejected","message":"Bad IDs"},"validation":{"message":"Bad IDs","reason":"invalid","status":"rejected"},"value":null,"versions":{"app":"9.8.7","document":3,"engine_protocol":2,"operation_api":1},"warnings":[{"message":"first"},{"message":"second"}]}"#;
    let result = ToolResult::rejected(
        Rejection::Invalid("Bad IDs".into()),
        vec!["first".into(), "second".into()],
        &versions(),
    );
    assert!(result.is_error);
    assert!(result.images.is_empty() && result.files.is_empty());
    assert_eq!(serde_json::to_string(&result.value).unwrap(), JSON,);
    let message = "é".repeat(600);
    let result = ToolResult::rejected(
        Rejection::Reactions(message.clone()),
        Vec::new(),
        &versions(),
    );
    assert_eq!(result.value["validation"]["reason"], "reactions");
    assert_eq!(result.value["validation"]["message"], message);
    assert_eq!(result.value["error"]["message"], "é".repeat(500));
}

#[test]
fn plain_and_attachments_keep_the_object_and_error_flag() {
    const ERROR_JSON: &str = r#"{"error":{"code":"failed","message":"It went wrong"},"versions":{"app":"9.8.7","document":3,"engine_protocol":2,"operation_api":1}}"#;
    let value = Map::from_iter([("answer".into(), json!(42))]);
    let plain = ToolResult::plain(value.clone());
    assert_eq!(plain.value, value);
    assert!(!plain.is_error);
    assert!(plain.images.is_empty() && plain.files.is_empty());

    let error = ToolResult::error(&OpError::failed("It went wrong"), &versions());
    assert_eq!(serde_json::to_string(&error.value).unwrap(), ERROR_JSON,);
    let value = error.value.clone();
    let images = vec![Image {
        mime: "image/png",
        bytes: vec![1, 2],
    }];
    let files = vec![Blob {
        mime: "image/svg+xml",
        name: "drawing.svg".into(),
        bytes: vec![3, 4],
    }];
    let result = error.with_files(files.clone()).with_images(images.clone());
    assert_eq!(result.value, value);
    assert!(result.is_error);
    assert_eq!(result.images, images);
    assert_eq!(result.files, files);

    let invalid = OpError::invalid("é".repeat(600));
    assert_eq!(invalid.kind, ErrorKind::InvalidArguments);
    assert_eq!(invalid.message, "é".repeat(500));
}
