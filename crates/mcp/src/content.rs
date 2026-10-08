//! Maps an operation [`ToolResult`] to an MCP `CallToolResult`.
//!
//! - `structuredContent` is the result's value, always a JSON object, as
//!   2025-06-18 and 2025-11-25 require.
//! - `content` starts with a text copy of it, the backward-compatible form
//!   the specification recommends, followed by each image as base64 image
//!   content and each file as an embedded resource at
//!   `reshiki:result/{name}`: text for textual formats that are UTF-8, a
//!   base64 blob otherwise. The resources capability stays undeclared; the
//!   contents travel inline.
//! - `isError` is always present.
//!
//! A result whose serialized size exceeds the cap is replaced by a `budget`
//! tool error.
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki_agent::{
    envelope::Versions,
    ops::{
        error::{ErrorKind, OpError},
        result::{Blob, Image, ToolResult},
    },
};
use rmcp::model::{CallToolResult, ContentBlock, ResourceContents};
use serde_json::Value;
use std::io::{self, Write};

#[cfg(test)]
mod tests;

/// File formats sent as text resources when their bytes are UTF-8.
const TEXT_MIMES: [&str; 5] = [
    "image/svg+xml",
    "chemical/x-cdxml",
    "chemical/x-mdl-molfile",
    "chemical/x-daylight-smiles",
    "chemical/x-inchi",
];

/// `r` as MCP content, or a `budget` tool error when the serialized result
/// would exceed `cap` bytes.
pub(crate) fn to_mcp(r: ToolResult, versions: &Versions, cap: usize) -> CallToolResult {
    let result = build(r);
    if fits(&result, cap) {
        return result;
    }
    let error = OpError::new(
        ErrorKind::Budget,
        format!("Result exceeds {cap} bytes; request a smaller render or fewer objects"),
    );
    build(ToolResult::error(&error, versions))
}

fn build(r: ToolResult) -> CallToolResult {
    let ToolResult {
        value,
        images,
        files,
        is_error,
    } = r;
    let structured = Value::Object(value);
    // Serializing a Value never fails: its map keys are strings.
    let text = serde_json::to_string(&structured).unwrap_or_default();
    let mut content = Vec::with_capacity(1 + images.len() + files.len());
    content.push(ContentBlock::text(text));
    content.extend(images.into_iter().map(image));
    content.extend(files.into_iter().map(file));
    let mut result = if is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    };
    result.structured_content = Some(structured);
    result
}

fn image(Image { mime, bytes }: Image) -> ContentBlock {
    ContentBlock::image(STANDARD.encode(bytes), mime)
}

fn file(Blob { mime, name, bytes }: Blob) -> ContentBlock {
    let uri = format!("reshiki:result/{name}");
    let contents = if TEXT_MIMES.contains(&mime) {
        match String::from_utf8(bytes) {
            Ok(text) => ResourceContents::TextResourceContents {
                uri,
                mime_type: Some(mime.to_owned()),
                text,
                meta: None,
            },
            Err(error) => blob(uri, mime, &error.into_bytes()),
        }
    } else {
        blob(uri, mime, &bytes)
    };
    ContentBlock::resource(contents)
}

fn blob(uri: String, mime: &str, bytes: &[u8]) -> ResourceContents {
    ResourceContents::BlobResourceContents {
        uri,
        mime_type: Some(mime.to_owned()),
        blob: STANDARD.encode(bytes),
        meta: None,
    }
}

/// Whether `result` serializes to at most `cap` bytes, counted without
/// keeping the output.
fn fits(result: &CallToolResult, cap: usize) -> bool {
    let mut counter = Counter { n: 0, cap };
    serde_json::to_writer(&mut counter, result).is_ok()
}

/// Counts bytes written and fails once more than `cap` were.
struct Counter {
    n: usize,
    cap: usize,
}

impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.n.checked_add(bytes.len()) {
            Some(n) if n <= self.cap => {
                self.n = n;
                Ok(bytes.len())
            }
            _ => Err(io::Error::other("result too large")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
