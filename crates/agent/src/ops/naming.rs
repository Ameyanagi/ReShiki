//! Chemical naming for the operation tools.
//!
//! The name parser and generator live in the application, outside this
//! crate, so a host is given them as a [`Naming`] hook
//! ([`HeadlessHost::with_naming`](super::headless::HeadlessHost::with_naming)).
//! A host without one answers `import` format `name` and the `name` tool
//! with [`ErrorKind::Unsupported`].
use super::{
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    policy::Access,
    result::ToolResult,
    store::Documents,
    wire::{DocHandle, Principal, handle_schema, warnings},
};
use crate::{
    chemistry::smiles::Prepared,
    document::Document,
    envelope::Versions,
    tool_spec::{Hints, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{future::Future, pin::Pin, sync::Arc};

/// A naming call in flight. Dropping it cancels the work.
pub type NamingFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

/// Local, offline name and structure conversion. Errors are client-facing
/// messages.
pub trait Naming: Send + Sync + 'static {
    /// The structure of a chemical name.
    fn resolve(&self, name: String) -> NamingFuture<Resolved>;

    /// Checks that `doc`, the drawing imported from a [`Resolved`] structure,
    /// still has its `identity`.
    fn verify(&self, identity: &str, doc: &Document) -> Result<(), String>;

    /// The systematic name of a whole drawing, and the warnings about it.
    fn generate(&self, doc: Document) -> NamingFuture<(Named, Vec<String>)>;
}

/// The structure a name resolved to.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// Naming SMILES: private atom-map markers carry axial configuration, so
    /// it is read as the engine's `naming_smiles`, never as ordinary SMILES.
    pub smiles: String,
    /// The prepared graph, imported in place of `smiles` when there is one.
    pub prepared: Option<Prepared>,
    /// The canonical identity the imported drawing must keep.
    pub identity: String,
    /// The name parser's warnings.
    pub warnings: Vec<String>,
}

/// The host's naming hook, when it has one.
pub type Hook = Option<Arc<dyn Naming>>;

/// A generated name and how far it can be trusted, the `name` tool's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Named {
    pub name: String,
    /// `preferred`, `not_preferred` or `unverified`.
    pub status: String,
    /// Whether `status` is `preferred`.
    pub preferred: bool,
    /// The rule family behind a status other than `preferred`.
    pub reason: Option<String>,
    /// One sentence explaining the status.
    pub detail: String,
    /// Curated common names for the same structure.
    pub aliases: Vec<String>,
    pub alternatives: Vec<Alternative>,
}

/// Another valid name for the same structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Alternative {
    pub name: String,
    pub status: String,
    pub reason: Option<String>,
}

pub const NAME: ToolSpec = ToolSpec {
    name: "name",
    title: Some("Name structure"),
    description: "Generate the systematic IUPAC name of a session document's molecule, offline. status says how far to trust it: preferred, not_preferred or unverified, with a reason and a detail sentence; aliases are curated common names and alternatives other valid names. For the opposite direction, import with format name.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: true,
        destructive: false,
        idempotent: true,
        open_world: false,
    }),
};

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {"document": handle_schema()},
        "required": ["document"],
        "additionalProperties": false,
    })
}

/// The `name` arguments.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Name {
    document: DocHandle,
}

pub(crate) fn decode(args: Value) -> Result<Name, OpError> {
    arguments(args)
}

/// The hook, or the error of a host without naming.
pub(crate) fn hook(naming: &Hook) -> Result<&Arc<dyn Naming>, OpError> {
    naming.as_ref().ok_or_else(|| {
        OpError::new(
            ErrorKind::Unsupported,
            "Chemical naming is not available in this host",
        )
    })
}

/// Names the document's molecule with read access.
///
/// The successful result contract is in [`super::catalog::RESULTS`].
pub(crate) async fn name(
    ctx: Context,
    store: Arc<dyn Documents>,
    naming: Hook,
    who: Principal,
    versions: Versions,
    name: Name,
) -> Result<ToolResult, OpError> {
    let naming = Arc::clone(hook(&naming)?);
    let doc = ctx
        .blocking(move || {
            let snapshot = store.snapshot(&who, &name.document, Access::Read)?;
            Ok(Arc::unwrap_or_clone(snapshot.doc))
        })
        .await?;
    let named = naming.generate(doc).await.map_err(OpError::failed);
    ctx.checkpoint()?;
    let (named, messages) = named?;
    Ok(ToolResult::valid(named, warnings(messages), versions))
}
