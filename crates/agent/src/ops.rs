//! Experimental operation API (OPERATION_API_VERSION 1); not wired to any
//! transport in this crate.
//!
//! # Layering
//!
//! From the transport inward:
//!
//! 1. [`host::ToolHost`] faces the transport: it lists the tool catalog,
//!    runs calls, cancels them and drains on shutdown.
//! 2. The executor ([`exec::Executor`]) owns request IDs, admission,
//!    permits, cooperative cancellation, deadlines and an explicit
//!    uncancellable effect phase.
//! 3. The documents store ([`store::Documents`], implemented over session
//!    documents by [`session::SessionStore`]) resolves handles and commits
//!    mutations, checking [`policy::check`] at resolve time and again inside
//!    the commit.
//! 4. The operation functions, shared by every host.
//!
//! [`headless::HeadlessHost`] puts the layers together for a process without
//! the app: the [`catalog`] tools over session documents.
//!
//! # Effect rule
//!
//! Every mutation is ONE atomic store commit. Under one lock it rechecks the
//! owner and policy, replays an idempotent receipt, checks the revision,
//! preflights quotas before any mutation, mutates infallibly and stores the
//! receipt. Nothing runs after the commit, so cancellation or a deadline that
//! passes after it can never lose a receipt or duplicate an edit.
//!
//! # Wire rules
//!
//! - Object IDs ([`wire::ObjectId`]) and revisions ([`wire::Revision`]) are
//!   decimal strings, never JSON numbers: IDs above 2^53 occur and would lose
//!   precision as numbers.
//! - Document handles ([`wire::DocHandle`]) are opaque strings of 1 to 64
//!   characters of `[A-Za-z0-9_-]`, checked against their owner on every call.
//! - Input schemas are hand-written JSON Schema 2020-12 built from the
//!   [`wire`] helpers. Every argument struct follows the required-nullable
//!   rule described in [`wire`], so serde and the schema's `required` agree.
//! - Results are a [`result::ToolResult`] whose `value` is always a JSON
//!   object, valid as `structuredContent` in every supported MCP revision.
//! - Errors are an [`error::OpError`]; only [`error::ErrorKind::UnknownTool`]
//!   is a protocol error, everything else is a tool execution error.
pub mod analyze;
pub mod budget;
pub mod catalog;
pub mod documents;
pub mod error;
pub mod exec;
pub mod export;
pub mod headless;
pub mod host;
mod ids;
pub mod import;
pub mod inspect;
pub mod policy;
pub mod progress;
pub mod render;
pub mod result;
pub mod session;
pub mod store;
pub mod wire;
