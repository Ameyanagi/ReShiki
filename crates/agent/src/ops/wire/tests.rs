use super::*;
use crate::{
    envelope::Warning,
    ops::{
        error::{ErrorKind, OpError},
        host::{Call, ToolHost},
        result::ToolResult,
    },
    tool_spec::ToolSpec,
};

fn versions() -> Versions {
    Versions::current("9.8.7")
}

fn versions_value() -> Value {
    json!({
        "app": "9.8.7",
        "operation_api": crate::envelope::OPERATION_API_VERSION,
        "engine_protocol": crate::engine::PROTOCOL,
        "document": crate::document::VERSION,
    })
}

fn error_text<T: std::fmt::Debug>(result: Result<T, serde_json::Error>) -> String {
    result.expect_err("expected a decode error").to_string()
}

#[test]
fn object_ids_above_2_pow_53_round_trip_as_strings() {
    let id = ObjectId(9_007_199_254_740_993);
    assert_eq!(serde_json::to_string(&id).unwrap(), r#""9007199254740993""#);
    assert_eq!(
        serde_json::from_str::<ObjectId>(r#""9007199254740993""#).unwrap(),
        id
    );
    assert_eq!(
        serde_json::from_value::<ObjectId>(serde_json::to_value(id).unwrap()).unwrap(),
        id
    );
    for (text, value) in [("1", 1), ("12", 12), ("18446744073709551614", u64::MAX - 1)] {
        assert_eq!(
            serde_json::from_value::<ObjectId>(json!(text)).unwrap(),
            ObjectId(value),
            "{text}"
        );
    }
}

#[test]
fn object_ids_reject_numbers_and_non_canonical_strings() {
    for number in [
        json!(12),
        json!(12.0),
        json!(9_007_199_254_740_993_u64),
        json!(-1),
    ] {
        let message = error_text(serde_json::from_value::<ObjectId>(number.clone()));
        assert!(message.contains(OBJECT_ID_MESSAGE), "{number}: {message}");
    }
    for text in [
        "012",
        "+1",
        " 1",
        "1 ",
        "0",
        "",
        "-1",
        "1e3",
        "0x1",
        "18446744073709551615",
        "18446744073709551616",
        "99999999999999999999",
        "100000000000000000000",
    ] {
        assert_eq!(
            error_text(serde_json::from_value::<ObjectId>(json!(text))),
            OBJECT_ID_MESSAGE,
            "{text:?}"
        );
    }
    for value in [json!(null), json!(true), json!(["1"]), json!({"id": "1"})] {
        assert!(
            serde_json::from_value::<ObjectId>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[test]
fn revisions_are_decimal_strings_including_zero() {
    assert_eq!(serde_json::to_value(Revision(0)).unwrap(), json!("0"));
    assert_eq!(
        serde_json::to_value(Revision(9_007_199_254_740_993)).unwrap(),
        json!("9007199254740993")
    );
    for (text, value) in [("0", 0), ("7", 7), ("18446744073709551615", u64::MAX)] {
        assert_eq!(
            serde_json::from_value::<Revision>(json!(text)).unwrap(),
            Revision(value)
        );
    }
    for text in ["00", "01", "+0", " 0", "", "18446744073709551616"] {
        assert_eq!(
            error_text(serde_json::from_value::<Revision>(json!(text))),
            REVISION_MESSAGE,
            "{text:?}"
        );
    }
    let message = error_text(serde_json::from_value::<Revision>(json!(0)));
    assert!(message.contains(REVISION_MESSAGE), "{message}");
}

#[test]
fn handles_are_1_to_64_characters_of_the_handle_charset() {
    let longest = "a".repeat(64);
    for text in [
        "a",
        "Z",
        "0",
        "_",
        "-",
        "doc_0123abcdef",
        "A-z_09",
        &longest,
    ] {
        let handle = DocHandle::new(text).unwrap();
        assert_eq!(handle.as_str(), text);
        assert_eq!(serde_json::to_value(&handle).unwrap(), json!(text));
        assert_eq!(
            serde_json::from_value::<DocHandle>(json!(text)).unwrap(),
            handle
        );
    }
    let too_long = "a".repeat(65);
    for text in [
        "", &too_long, "a b", "a/b", "a.b", "a:b", "é", "a\n", "\u{0}",
    ] {
        assert_eq!(DocHandle::new(text), None, "{text:?}");
        assert_eq!(
            error_text(serde_json::from_value::<DocHandle>(json!(text))),
            HANDLE_MESSAGE,
            "{text:?}"
        );
    }
    assert!(serde_json::from_value::<DocHandle>(json!(12)).is_err());
}

#[test]
fn schema_helpers_use_the_contract_patterns() {
    assert_eq!(
        object_id_schema(),
        json!({"type":"string","pattern":"^[1-9][0-9]{0,19}$"})
    );
    assert_eq!(
        revision_schema(),
        json!({"type":"string","pattern":"^(0|[1-9][0-9]{0,19})$"})
    );
    assert_eq!(
        handle_schema(),
        json!({"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"})
    );
    assert_eq!(
        ids_schema(),
        json!({"type":"array","maxItems":5000,"items":{"type":"string","pattern":"^[1-9][0-9]{0,19}$"}})
    );
    assert_eq!(
        nullable(handle_schema()),
        json!({"anyOf":[{"type":"null"},{"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"}]})
    );
}

/// A sample argument struct following the required-nullable rule.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    #[serde(deserialize_with = "Option::deserialize")]
    x: Option<ObjectId>,
}

#[test]
fn required_nullable_fields_must_be_present() {
    let message = error_text(serde_json::from_value::<Sample>(json!({})));
    assert!(message.contains("missing field `x`"), "{message}");
    assert_eq!(
        serde_json::from_value::<Sample>(json!({"x": null})).unwrap(),
        Sample { x: None }
    );
    assert_eq!(
        serde_json::from_value::<Sample>(json!({"x": "12"})).unwrap(),
        Sample {
            x: Some(ObjectId(12))
        }
    );
    assert!(serde_json::from_value::<Sample>(json!({"x": 12})).is_err());
    assert!(serde_json::from_value::<Sample>(json!({"x": null, "y": null})).is_err());
}

#[test]
fn request_ids_keep_integers_and_strings_apart() {
    assert_ne!(RequestId::Int(1), RequestId::Str("1".into()));
    assert_eq!(RequestId::Str("1".into()), RequestId::Str("1".into()));
    let ids = std::collections::HashSet::from([RequestId::Int(1), RequestId::Str("1".into())]);
    assert_eq!(ids.len(), 2);
    assert_eq!(Principal::local(), Principal::local());
    assert_eq!(Principal::local().as_str(), "local");
    assert_ne!(Principal::local(), Principal::new("client-2"));
}

#[derive(Serialize)]
struct Inserted {
    remap: IdRemapJson,
    receipt: ExportReceiptJson,
}

#[test]
fn envelope_json_has_the_documented_shape() {
    let envelope = Envelope {
        value: Inserted {
            remap: IdRemapJson::from(&IdRemap {
                pairs: vec![(3, 9_007_199_254_740_993), (4, 12)],
            }),
            receipt: ExportReceiptJson::from(&ExportReceipt {
                format: "png".into(),
                byte_len: 2048,
                detail: Some("Rendered at 2x".into()),
            }),
        },
        warnings: vec![Warning {
            message: "Check the stereo bonds".into(),
        }],
        validation: ValidationStatus::Valid,
        versions: versions(),
    };
    assert_eq!(
        Value::Object(envelope_json(&envelope)),
        json!({
            "value": {
                "remap": [
                    {"source": "3", "inserted": "9007199254740993"},
                    {"source": "4", "inserted": "12"},
                ],
                "receipt": {"format": "png", "byte_len": 2048, "detail": "Rendered at 2x"},
            },
            "warnings": [{"message": "Check the stereo bonds"}],
            "validation": {"status": "valid"},
            "versions": versions_value(),
        })
    );
    let receipt = ExportReceiptJson::from(&ExportReceipt {
        format: "svg".into(),
        byte_len: 0,
        detail: None,
    });
    assert_eq!(
        serde_json::to_value(receipt).unwrap(),
        json!({"format": "svg", "byte_len": 0, "detail": null})
    );
}

#[test]
fn envelope_json_reports_rejections_with_their_reason() {
    for (rejection, reason) in [
        (
            Rejection::Reactions("Keep reactions separate".into()),
            "reactions",
        ),
        (
            Rejection::Invalid("Duplicate or zero object ID".into()),
            "invalid",
        ),
    ] {
        let message = rejection.message().to_owned();
        let envelope = Envelope {
            value: (),
            warnings: Vec::new(),
            validation: ValidationStatus::Rejected(rejection),
            versions: versions(),
        };
        let json = envelope_json(&envelope);
        assert_eq!(
            json["validation"],
            json!({"status": "rejected", "reason": reason, "message": message})
        );
        assert_eq!(json["value"], Value::Null);
        assert_eq!(json["warnings"], json!([]));
    }
}

struct Unencodable;

impl Serialize for Unencodable {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("no JSON form"))
    }
}

#[test]
fn envelope_json_warns_when_the_value_cannot_be_encoded() {
    let envelope = Envelope {
        value: Unencodable,
        warnings: vec![Warning {
            message: "first".into(),
        }],
        validation: ValidationStatus::Valid,
        versions: versions(),
    };
    let json = envelope_json(&envelope);
    assert_eq!(json["value"], Value::Null);
    assert_eq!(
        json["warnings"],
        json!([
            {"message": "first"},
            {"message": "The result could not be encoded: no JSON form"},
        ])
    );
}

const KINDS: [ErrorKind; 12] = [
    ErrorKind::UnknownTool,
    ErrorKind::InvalidArguments,
    ErrorKind::UnknownDocument,
    ErrorKind::UnknownObject,
    ErrorKind::Stale,
    ErrorKind::Busy,
    ErrorKind::Budget,
    ErrorKind::Timeout,
    ErrorKind::Rejected,
    ErrorKind::Unsupported,
    ErrorKind::Failed,
    ErrorKind::Cancelled,
];

#[test]
fn only_unknown_tool_is_a_protocol_error() {
    let codes: Vec<_> = KINDS.iter().map(|kind| kind.code()).collect();
    assert_eq!(
        codes,
        [
            "unknown_tool",
            "invalid_arguments",
            "unknown_document",
            "unknown_object",
            "stale",
            "busy",
            "budget",
            "timeout",
            "rejected",
            "unsupported",
            "failed",
            "cancelled",
        ]
    );
    for kind in KINDS {
        assert_eq!(
            kind.is_protocol_error(),
            kind == ErrorKind::UnknownTool,
            "{kind:?}"
        );
    }
}

#[test]
fn tool_errors_become_is_error_results() {
    for kind in KINDS {
        let result = ToolResult::error(&OpError::new(kind, "It went wrong"), &versions());
        assert!(result.is_error);
        assert!(result.images.is_empty() && result.files.is_empty());
        assert_eq!(
            Value::Object(result.value),
            json!({
                "error": {"code": kind.code(), "message": "It went wrong"},
                "versions": versions_value(),
            })
        );
    }
}

#[test]
fn error_messages_keep_at_most_500_characters() {
    let long = "é".repeat(600);
    let error = OpError::new(ErrorKind::InvalidArguments, &long);
    assert_eq!(error.message.chars().count(), OpError::MAX_MESSAGE_CHARS);
    assert_eq!(error.message, "é".repeat(500));
    assert_eq!(error.message.len(), 1000);
    let mixed = format!("{}🧪{}", "a".repeat(499), "b".repeat(10));
    assert_eq!(
        OpError::new(ErrorKind::Failed, &mixed).message,
        format!("{}🧪", "a".repeat(499))
    );
    assert_eq!(OpError::new(ErrorKind::Busy, "short").message, "short");
    assert_eq!(OpError::new(ErrorKind::Busy, "short").to_string(), "short");
    let error = OpError::from(Rejection::Invalid("Invalid group ID".into()));
    assert_eq!(error.kind, ErrorKind::Rejected);
    assert_eq!(error.message, "Invalid group ID");
}

/// A host that follows the [`ToolHost::call`] contract for three tools.
struct FakeHost;

impl ToolHost for FakeHost {
    fn catalog(&self) -> &'static [ToolSpec] {
        &[]
    }

    async fn call(&self, call: Call) -> Result<ToolResult, OpError> {
        match call.tool.as_str() {
            "fails" => Ok(ToolResult::error(
                &OpError::new(ErrorKind::Failed, "It went wrong"),
                &versions(),
            )),
            "cancelled" => Err(OpError::new(ErrorKind::Cancelled, "")),
            tool => Err(OpError::new(
                ErrorKind::UnknownTool,
                format!("Unknown tool {tool}"),
            )),
        }
    }

    fn cancel(&self, _: &Principal, _: &RequestId) {}

    async fn drained(&self) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_host_futures_are_send_and_only_protocol_errors_or_cancels_are_err() {
    let host = std::sync::Arc::new(FakeHost);
    let call = |tool: &str| Call {
        principal: Principal::local(),
        request: RequestId::Int(1),
        tool: tool.into(),
        arguments: json!({}),
        progress: None,
    };
    let spawned = {
        let host = host.clone();
        let call = call("fails");
        tokio::spawn(async move { host.call(call).await })
    };
    let result = spawned.await.unwrap().unwrap();
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "failed");
    let unknown = host.call(call("missing")).await.unwrap_err();
    assert!(unknown.kind.is_protocol_error());
    let cancelled = host.call(call("cancelled")).await.unwrap_err();
    assert_eq!(cancelled.kind, ErrorKind::Cancelled);
    assert!(!cancelled.kind.is_protocol_error());
    host.cancel(&Principal::local(), &RequestId::Str("1".into()));
    tokio::spawn({
        let host = host.clone();
        async move { host.drained().await }
    })
    .await
    .unwrap();
    assert!(host.catalog().is_empty());
}
