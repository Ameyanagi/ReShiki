//! The tool host for a process without the app: session documents only.
use super::{
    budget::Budgets,
    catalog::{self, Op},
    documents,
    error::{ErrorKind, OpError},
    exec::{Context, Executor},
    host::{Call, ToolHost},
    result::ToolResult,
    session::SessionStore,
    store::Documents,
    wire::{Principal, RequestId},
};
use crate::{engine::LocalEngine, envelope::Versions, tool_spec::ToolSpec};
use std::sync::Arc;

/// Serves the [`catalog`] tools over a [`SessionStore`], running every call
/// on a bounded [`Executor`].
pub struct HeadlessHost {
    exec: Executor,
    store: Arc<SessionStore>,
    #[expect(
        dead_code,
        reason = "the import, analyze and export operations of later steps use it"
    )]
    engine: LocalEngine,
    budgets: Budgets,
    versions: Versions,
}

impl HeadlessHost {
    /// A host whose results report `app_version`; the app passes
    /// `reshiki::updates::CURRENT_VERSION`.
    pub fn new(app_version: &'static str, budgets: Budgets) -> Self {
        Self {
            exec: Executor::new(budgets.clone()),
            store: Arc::new(SessionStore::new(budgets.clone())),
            engine: LocalEngine::default(),
            budgets,
            versions: Versions::current(app_version),
        }
    }

    #[cfg(test)]
    pub(crate) fn exec(&self) -> &Executor {
        &self.exec
    }

    #[cfg(test)]
    pub(crate) fn store(&self) -> &Arc<SessionStore> {
        &self.store
    }
}

/// Runs one decoded call on the executor.
async fn run(
    op: Op,
    ctx: Context,
    who: Principal,
    store: Arc<dyn Documents>,
    versions: Versions,
    budgets: Budgets,
) -> Result<ToolResult, OpError> {
    match op {
        Op::Info => Ok(documents::info(&versions, &budgets)),
        Op::DocumentNew => documents::document_new(ctx, store, who, versions).await,
        Op::DocumentList => documents::document_list(ctx, store, who, versions).await,
        Op::DocumentClose(args) => documents::document_close(ctx, store, who, versions, args).await,
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
        let op = match catalog::decode(&tool, arguments) {
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
        let versions = self.versions.clone();
        let budgets = self.budgets.clone();
        let outcome = self
            .exec
            .run(principal, request, progress, move |ctx| {
                run(op, ctx, who, store, versions, budgets)
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
