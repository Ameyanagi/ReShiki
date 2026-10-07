//! rmcp's [`Transport`] over one framed connection.
//!
//! The framing threads own the input and output; this side only parses
//! admitted lines into rmcp messages and serializes rmcp's messages into
//! [`Outbound`] lines. Every admitted request gets exactly one outbound line
//! carrying its key, so the writer always completes its slot: rmcp's
//! response, or an error written here when rmcp never sees the request.
use crate::{
    framing::{Inbound, Key, Outbound, Status, Tracker},
    log::{Level, Log},
};
use rmcp::{
    RoleServer,
    model::{ClientJsonRpcMessage, ClientRequest, JsonRpcMessage, NumberOrString},
    service::TxJsonRpcMessage,
    transport::Transport,
};
use serde::Serialize;
use std::{fmt, future::Future, sync::Arc};
use tokio::sync::mpsc::{Receiver, Sender, error::SendError};

#[cfg(test)]
mod tests;

const INVALID_REQUEST: i64 = -32600;
const INTERNAL_ERROR: i64 = -32603;

/// Room for the JSON-RPC envelope around a result of
/// [`crate::framing::Limits::max_result_bytes`].
pub(crate) const ENVELOPE_BYTES: usize = 64 * 1024;

/// One connection's half of the framing, as rmcp sees it.
pub(crate) struct Stdio {
    pub(crate) inbound: Receiver<Inbound>,
    pub(crate) outbound: Sender<Outbound>,
    pub(crate) tracker: Arc<Tracker>,
    pub(crate) status: Arc<Status>,
    pub(crate) log: Log,
    /// The longest line sent; a longer response is replaced by an error.
    pub(crate) max_line: usize,
    /// Whether rmcp received an `initialize` request it could parse. Until
    /// it has, `notifications/initialized` is dropped: rmcp ends the
    /// connection on a notification before its lifecycle starts.
    pub(crate) initialize_delivered: bool,
    /// The error reply to an admitted request rmcp never sees, held until
    /// the outbound queue has room. rmcp drops [`Transport::receive`]
    /// whenever another event wins its select, so the reply must outlive
    /// the call that dequeued its request.
    pub(crate) pending: Option<Outbound>,
}

/// The writer is gone, so nothing more can be sent.
#[derive(Debug)]
pub(crate) struct Error;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MCP output closed")
    }
}

impl std::error::Error for Error {}

impl Transport<RoleServer> for Stdio {
    type Error = Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let key = match &item {
            JsonRpcMessage::Response(response) => Some(key_of(&response.id)),
            JsonRpcMessage::Error(error) => error.id.as_ref().map(key_of),
            JsonRpcMessage::Request(_) | JsonRpcMessage::Notification(_) => None,
        };
        let line = match serde_json::to_vec(&item) {
            Ok(line) if line.len() <= self.max_line || key.is_none() => Some(line),
            Ok(line) => {
                let bytes = u64::try_from(line.len()).unwrap_or(u64::MAX);
                self.log
                    .event(Level::Warn, "response too large", &[("bytes", bytes)]);
                Some(error_line(
                    key.as_ref(),
                    INTERNAL_ERROR,
                    "Response too large",
                ))
            }
            Err(_) => {
                self.log
                    .event(Level::Error, "response serialization failed", &[]);
                key.as_ref()
                    .map(|key| error_line(Some(key), INTERNAL_ERROR, "Internal error"))
            }
        };
        let sender = self.outbound.clone();
        let tracker = Arc::clone(&self.tracker);
        async move {
            let Some(line) = line else {
                return Ok(());
            };
            match sender.send(Outbound { key, line }).await {
                Ok(()) => Ok(()),
                Err(SendError(Outbound { key, .. })) => {
                    // The writer is gone and will never complete this slot.
                    if let Some(key) = &key {
                        tracker.complete(key);
                    }
                    Err(Error)
                }
            }
        }
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            // Cancel-safe: the reply leaves `pending` only once a permit is
            // in hand, and sending on a permit never waits.
            if self.pending.is_some() {
                let Ok(permit) = self.outbound.reserve().await else {
                    // The writer is gone and will never complete it.
                    if let Some(Outbound { key: Some(key), .. }) = self.pending.take() {
                        self.tracker.complete(&key);
                    }
                    return None;
                };
                if let Some(reply) = self.pending.take() {
                    permit.send(reply);
                }
            }
            let inbound = tokio::select! {
                inbound = self.inbound.recv() => inbound?,
                () = writer_failed(&self.status) => return None,
            };
            match inbound {
                Inbound::Request { key, line } => match parse(&line) {
                    Ok(JsonRpcMessage::Request(request)) => {
                        if matches!(request.request, ClientRequest::InitializeRequest(_)) {
                            self.initialize_delivered = true;
                        }
                        return Some(JsonRpcMessage::Request(request));
                    }
                    // The framing admitted it, so answer it here; the
                    // next turn of the loop queues the reply.
                    Ok(_) | Err(_) => {
                        self.log.event(Level::Debug, "invalid request", &[]);
                        let line = error_line(Some(&key), INVALID_REQUEST, "Invalid Request");
                        self.pending = Some(Outbound {
                            key: Some(key),
                            line,
                        });
                    }
                },
                Inbound::Initialized { line } => match parse(&line) {
                    Ok(message) if self.initialize_delivered => return Some(message),
                    Ok(_) | Err(_) => {
                        self.log.event(Level::Debug, "ignored notification", &[]);
                    }
                },
            }
        }
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        std::future::ready(Ok(()))
    }
}

impl Drop for Stdio {
    /// rmcp dropped the transport with a reply still pending: queue it if
    /// there is room, else free its slot, as the writer never will.
    fn drop(&mut self) {
        if let Some(reply) = self.pending.take()
            && let Err(refused) = self.outbound.try_send(reply)
            && let Some(key) = refused.into_inner().key
        {
            self.tracker.complete(&key);
        }
    }
}

/// Resolves once the writer failed. After EOF it never resolves: the
/// inbound channel closes once its queued lines are read.
async fn writer_failed(status: &Status) {
    status.closed().await;
    if !status.writer_failed() {
        std::future::pending::<()>().await;
    }
}

fn parse(line: &[u8]) -> serde_json::Result<ClientJsonRpcMessage> {
    serde_json::from_slice(line)
}

fn key_of(id: &NumberOrString) -> Key {
    match id {
        NumberOrString::Number(value) => Key::Int(*value),
        NumberOrString::String(text) => Key::Str(Arc::clone(text)),
    }
}

/// `{"jsonrpc":"2.0","id":…,"error":{"code":…,"message":…}}`, with `id`
/// omitted when there is none.
fn error_line(id: Option<&Key>, code: i64, message: &str) -> Vec<u8> {
    #[derive(Serialize)]
    struct ErrorLine<'a> {
        jsonrpc: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<&'a Key>,
        error: ErrorBody<'a>,
    }
    #[derive(Serialize)]
    struct ErrorBody<'a> {
        code: i64,
        message: &'a str,
    }
    // Serializing strings and integers cannot fail; the fallback only keeps
    // the slot's completion unconditional.
    serde_json::to_vec(&ErrorLine {
        jsonrpc: "2.0",
        id,
        error: ErrorBody { code, message },
    })
    .unwrap_or_else(|_| {
        br#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"}}"#.to_vec()
    })
}
