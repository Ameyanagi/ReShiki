//! The transport-facing side of the operation API.
use super::{
    error::OpError,
    progress,
    result::ToolResult,
    wire::{Principal, RequestId},
};
use crate::tool_spec::ToolSpec;
use serde_json::Value;
use std::future::Future;

/// One tool call, as the transport received it.
pub struct Call {
    /// Assigned by the server for the connection.
    pub principal: Principal,
    /// The JSON-RPC request ID, unique among the principal's in-flight calls.
    pub request: RequestId,
    pub tool: String,
    pub arguments: Value,
    pub progress: Option<progress::Sink>,
}

/// What a transport serves: a fixed tool catalog and the calls on it.
pub trait ToolHost: Send + Sync + 'static {
    /// The tools, in a fixed order.
    fn catalog(&self) -> &'static [ToolSpec];

    /// Runs one call.
    ///
    /// `Err` is returned ONLY for [`UnknownTool`](super::error::ErrorKind::UnknownTool),
    /// for which the transport sends a JSON-RPC error, and for
    /// [`Cancelled`](super::error::ErrorKind::Cancelled), for which the
    /// transport sends nothing. Every other outcome is `Ok`, with
    /// [`ToolResult::is_error`] set for a tool execution error.
    fn call(&self, call: Call) -> impl Future<Output = Result<ToolResult, OpError>> + Send;

    /// Cancels the principal's in-flight request `id`. Unknown and completed
    /// requests are ignored.
    fn cancel(&self, who: &Principal, id: &RequestId);

    /// Refuses new calls, cancels running ones and resolves once every
    /// admitted call has finished. The transport bounds the wait.
    fn drained(&self) -> impl Future<Output = ()> + Send;
}
