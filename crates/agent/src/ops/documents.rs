//! The server and session-document tools: `info`, `document_new`,
//! `document_list` and `document_close`.
use super::{
    budget::Budgets,
    catalog::arguments,
    error::OpError,
    exec::Context,
    result::ToolResult,
    store::Documents,
    wire::{DocHandle, Principal, handle_schema, versions_json},
};
use crate::{
    document::Document,
    envelope::{OPERATION_API_VERSION, Versions},
    tool_spec::{Hints, ToolSpec},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::sync::Arc;

/// The `format` values the import operation accepts.
pub const IMPORT_FORMATS: &[&str] = &[
    "auto", "smiles", "mol", "rxn", "rsmi", "inchi", "cdxml", "cdx", "reshiki",
];

/// The `format` values the export operation accepts. EMF and CDX output are
/// not offered.
pub const EXPORT_FORMATS: &[&str] = &["svg", "pdf", "png", "cdxml", "mol", "smiles", "inchi"];

pub const INFO: ToolSpec = ToolSpec {
    name: "info",
    title: Some("Server info"),
    description: "Report ReShiki's versions, the limits this server enforces (budgets) and the import and export formats. Read-only.",
    input_schema: no_arguments_schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

pub const NEW: ToolSpec = ToolSpec {
    name: "document_new",
    title: Some("New document"),
    description: "Create an empty session document and return its handle and revision. Session documents live only in this server process: one expires after idle_ttl_seconds without use (60 minutes by default), and at most max_documents (16 by default) can be open at once; info reports both budgets. An expired handle gives unknown_document. document_list shows every live handle, so you can close the ones you no longer need with document_close.",
    input_schema: no_arguments_schema,
    hints: Some(Hints {
        read_only: false,
        destructive: false,
        idempotent: false,
        open_world: false,
    }),
};

pub const LIST: ToolSpec = ToolSpec {
    name: "document_list",
    title: Some("List documents"),
    description: "List your live session documents in creation order, each with its handle, revision and object count. Read-only.",
    input_schema: no_arguments_schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

pub const CLOSE: ToolSpec = ToolSpec {
    name: "document_close",
    title: Some("Close document"),
    description: "Close a session document, discarding the drawing and its undo history. The handle stops working; this cannot be undone.",
    input_schema: close_schema,
    hints: Some(Hints {
        read_only: false,
        destructive: true,
        idempotent: true,
        open_world: false,
    }),
};

fn no_arguments_schema() -> Value {
    json!({"type":"object","properties":{},"additionalProperties":false})
}

fn close_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"document": handle_schema()},
        "required": ["document"],
        "additionalProperties": false,
    })
}

/// The arguments of a tool without parameters: an empty object.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArguments {}

/// Accepts only `{}`.
pub(crate) fn decode_none(args: Value) -> Result<(), OpError> {
    arguments::<NoArguments>(args).map(|NoArguments {}| ())
}

/// The `document_close` arguments.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Close {
    document: DocHandle,
}

pub(crate) fn decode_close(args: Value) -> Result<Close, OpError> {
    arguments(args)
}

/// `fields` and `versions` as a successful result.
fn ok<const N: usize>(fields: [(&str, Value); N], versions: &Versions) -> ToolResult {
    let mut value: Map<String, Value> = fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    value.insert("versions".into(), versions_json(versions));
    ToolResult {
        value,
        images: Vec::new(),
        files: Vec::new(),
        is_error: false,
    }
}

/// `{operation_api, versions, budgets, formats: {import, export}}`.
pub(crate) fn info(versions: &Versions, budgets: &Budgets) -> ToolResult {
    ok(
        [
            ("operation_api", json!(OPERATION_API_VERSION)),
            ("budgets", budgets_json(budgets)),
            (
                "formats",
                json!({"import": IMPORT_FORMATS, "export": EXPORT_FORMATS}),
            ),
        ],
        versions,
    )
}

fn budgets_json(budgets: &Budgets) -> Value {
    let render = &budgets.render;
    json!({
        "max_request_bytes": budgets.max_request_bytes,
        "max_text_bytes": budgets.max_text_bytes,
        "max_cdx_base64": budgets.max_cdx_base64,
        "max_documents": budgets.max_documents,
        "max_objects": budgets.max_objects,
        "max_ids": budgets.max_ids,
        "max_session_weight": budgets.max_session_weight,
        "max_session_picture_bytes": budgets.max_session_picture_bytes,
        "history_depth": budgets.history_depth,
        "max_inspect_objects": budgets.max_inspect_objects,
        "render": {
            "default_width": render.default_width,
            "default_height": render.default_height,
            "min_side": render.min_side,
            "max_side": render.max_side,
            "max_pixels": render.max_pixels,
        },
        "export_png_pixels": budgets.export_png_pixels,
        "max_output_bytes": budgets.max_output_bytes,
        "concurrency": budgets.concurrency,
        "queue": budgets.queue,
        "op_deadline_seconds": budgets.op_deadline.as_secs(),
        "idle_ttl_seconds": budgets.idle_ttl.as_secs(),
        "idempotency_receipts": budgets.idempotency_receipts,
        "max_idempotency_key_bytes": budgets.max_idempotency_key_bytes,
    })
}

/// Creates an empty document, as the app's new tab does
/// (src/app/document_tab.rs). `{document, revision, versions}`.
pub(crate) async fn document_new(
    mut ctx: Context,
    store: Arc<dyn Documents>,
    who: Principal,
    versions: Versions,
) -> Result<ToolResult, OpError> {
    let created = ctx
        .effect(move || store.create(&who, Document::default()))
        .await?;
    Ok(ok(
        [
            ("document", Value::from(created.handle.as_str())),
            ("revision", Value::from(created.revision.to_string())),
        ],
        &versions,
    ))
}

/// `{documents: [{document, revision, objects}], versions}`.
pub(crate) async fn document_list(
    ctx: Context,
    store: Arc<dyn Documents>,
    who: Principal,
    versions: Versions,
) -> Result<ToolResult, OpError> {
    let listed = ctx.blocking(move || Ok(store.list(&who))).await?;
    let documents = listed
        .iter()
        .map(|listed| {
            json!({
                "document": listed.handle.as_str(),
                "revision": listed.revision.to_string(),
                "objects": listed.objects,
            })
        })
        .collect();
    Ok(ok([("documents", Value::Array(documents))], &versions))
}

/// `{document, closed: true, versions}`.
pub(crate) async fn document_close(
    mut ctx: Context,
    store: Arc<dyn Documents>,
    who: Principal,
    versions: Versions,
    args: Close,
) -> Result<ToolResult, OpError> {
    let handle = args.document.clone();
    ctx.effect(move || store.close(&who, &handle)).await?;
    Ok(ok(
        [
            ("document", Value::from(args.document.as_str())),
            ("closed", Value::Bool(true)),
        ],
        &versions,
    ))
}
