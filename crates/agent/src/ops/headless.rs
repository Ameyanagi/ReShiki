//! The tool host for a process without the app: session documents only.
use super::{
    analyze, apply,
    budget::Budgets,
    catalog::{self, Op},
    compose, documents,
    error::{ErrorKind, OpError},
    exec::{Context, Executor},
    export, files,
    host::{Call, ToolHost},
    import, inspect, render,
    result::ToolResult,
    session::SessionStore,
    store::Documents,
    wire::{Principal, RequestId},
};
use crate::{access::Grants, engine::LocalEngine, envelope::Versions, tool_spec::ToolSpec};
use std::sync::Arc;

/// Serves the [`catalog`] tools over a [`SessionStore`], running every call
/// on a bounded [`Executor`].
pub struct HeadlessHost {
    exec: Executor,
    store: Arc<SessionStore>,
    engine: LocalEngine,
    budgets: Budgets,
    versions: Versions,
    grants: Arc<Grants>,
}

impl HeadlessHost {
    /// A host whose results report `app_version`; the app passes
    /// `reshiki::updates::CURRENT_VERSION`. It starts with
    /// [`Grants::none`], so file_open and file_save refuse every path.
    pub fn new(app_version: &'static str, budgets: Budgets) -> Self {
        Self {
            exec: Executor::new(budgets.clone()),
            store: Arc::new(SessionStore::new(budgets.clone())),
            engine: LocalEngine::default(),
            budgets,
            versions: Versions::current(app_version),
            grants: Arc::new(Grants::none()),
        }
    }

    /// The folders file_open and file_save may use, opened once at startup.
    pub fn with_grants(self, grants: Arc<Grants>) -> Self {
        Self { grants, ..self }
    }

    #[cfg(test)]
    pub(crate) fn exec(&self) -> &Executor {
        &self.exec
    }

    /// The host's session documents, for this crate's tests and the app's
    /// parity tests, which read the stored drawing back.
    #[doc(hidden)]
    pub fn store(&self) -> &Arc<SessionStore> {
        &self.store
    }
}

/// Runs one decoded call on the executor.
#[allow(clippy::too_many_arguments)]
async fn run(
    op: Op,
    ctx: Context,
    who: Principal,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    versions: Versions,
    budgets: Budgets,
    grants: Arc<Grants>,
) -> Result<ToolResult, OpError> {
    match op {
        // The other tools check through `blocking` or `effect`.
        Op::Info => ctx
            .checkpoint()
            .map(|()| documents::info(&versions, &budgets, &grants)),
        Op::DocumentNew => documents::document_new(ctx, store, who, versions).await,
        Op::DocumentList => documents::document_list(ctx, store, who, versions).await,
        Op::DocumentClose(args) => documents::document_close(ctx, store, who, versions, args).await,
        Op::Import(args) => import::import(ctx, store, engine, who, versions, args).await,
        Op::Inspect(args) => inspect::inspect(ctx, store, who, versions, budgets, args).await,
        Op::Analyze(args) => analyze::analyze(ctx, store, engine, who, versions, args).await,
        Op::Render(args) => render::render(ctx, store, who, versions, budgets, args).await,
        Op::Export(args) => export::export(ctx, store, engine, who, versions, budgets, args).await,
        Op::FileOpen(args) => {
            files::file_open(ctx, store, engine, grants, who, versions, budgets, args).await
        }
        Op::FileSave(args) => {
            files::file_save(ctx, store, engine, grants, who, versions, budgets, args).await
        }
        Op::Compose(args) => {
            compose::compose(ctx, store, engine, who, versions, budgets, *args).await
        }
        Op::Apply(args) => apply::apply(ctx, store, who, versions, args).await,
    }
}

impl ToolHost for HeadlessHost {
    fn catalog(&self) -> &'static [ToolSpec] {
        catalog::SPECS
    }

    /// Decodes the arguments first, so invalid arguments and input budgets
    /// never reach the executor. Every error except an unknown tool and a
    /// cancellation becomes an `is_error` result.
    async fn call(&self, call: Call) -> Result<ToolResult, OpError> {
        let Call {
            principal,
            request,
            tool,
            arguments,
            progress,
        } = call;
        let op = match catalog::decode(&tool, arguments, &self.budgets) {
            None => {
                return Err(OpError::new(
                    ErrorKind::UnknownTool,
                    format!("Unknown tool {tool}"),
                ));
            }
            Some(Err(error)) => return Ok(ToolResult::error(&error, &self.versions)),
            Some(Ok(op)) => op,
        };
        let who = principal.clone();
        let store: Arc<dyn Documents> = self.store.clone();
        let engine = self.engine.clone();
        let versions = self.versions.clone();
        let budgets = self.budgets.clone();
        let grants = Arc::clone(&self.grants);
        let outcome = self
            .exec
            .run(principal, request, progress, move |ctx| {
                run(op, ctx, who, store, engine, versions, budgets, grants)
            })
            .await;
        match outcome {
            Ok(result) => Ok(result),
            Err(error) if error.kind == ErrorKind::Cancelled => Err(error),
            Err(error) => Ok(ToolResult::error(&error, &self.versions)),
        }
    }

    fn cancel(&self, who: &Principal, id: &RequestId) {
        self.exec.cancel(who, id);
    }

    async fn drained(&self) {
        self.exec.drained().await;
    }
}

#[cfg(test)]
mod tests;
