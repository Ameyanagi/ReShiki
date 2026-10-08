//! The `compose` tool: lay out a Proposal as a new session document, as the
//! in-app assistant composes its drafts.
use super::{
    budget::{Budgets, objects},
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    import::warnings,
    policy::Access,
    progress,
    result::ToolResult,
    store::Documents,
    wire::{DocHandle, Principal, Revision, envelope_json, handle_schema, nullable},
};
use crate::{
    DrawingSettings, Proposal, composition,
    document::{Document, Point},
    engine::LocalEngine,
    envelope::{Envelope, ValidationStatus, Versions},
    progress::Event,
    render_progress, review, scene, sketch,
    tool_spec::{Hints, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::mpsc::{self, Sender};

/// A Proposal with replacement IDs; compose only creates documents.
pub const REPLACE_IDS: &str =
    "compose creates a new document; pass replacement IDs to apply as decimal strings";

/// A Proposal without molecules, reactions or a sketch.
pub const NO_DRAWING: &str = "A drawing needs at least one molecule, reaction or sketch";

pub const COMPOSE: ToolSpec = ToolSpec {
    name: "compose",
    title: Some("Compose scheme"),
    description: "Lay out a Proposal as a new session document, as ReShiki's assistant composes its drafts: molecules and reactions from SMILES, or one editable sketch diagram, never both. proposal follows the assistant's Proposal schema; replace_ids must be empty, because compose always creates a new document. style_document is a session document whose drawing style (bond length, font, line width) and atom label settings to use, or null for the default style. Returns the document's handle and revision, its atom, bond and arrow counts, its bounds, layout review issues and the layout changes made. Use render to look at it. Every call creates another document; close the ones you no longer need with document_close.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: false,
        destructive: false,
        idempotent: false,
        open_world: false,
    }),
};

/// The Proposal schema is nested verbatim, as the assistant's
/// `canvas_preview` publishes it.
fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "proposal": crate::schema(),
            "style_document": nullable(handle_schema()),
        },
        "required": ["proposal", "style_document"],
    })
}

/// The `compose` arguments as sent. The proposal is decoded on its own so
/// its errors read exactly as on the assistant's path.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    proposal: Value,
    #[serde(deserialize_with = "Option::deserialize")]
    style_document: Option<DocHandle>,
}

/// The decoded `compose` arguments.
#[derive(Debug)]
pub(crate) struct Compose {
    proposal: Proposal,
    style_document: Option<DocHandle>,
}

fn invalid(message: impl AsRef<str>) -> OpError {
    OpError::new(ErrorKind::InvalidArguments, message)
}

/// Decodes the arguments before any work runs.
///
/// The proposal is decoded and validated exactly as the assistant's Codex
/// Proposal turn does (src/assistant/codex.rs), with the same error text.
/// Then compose's own rules: no replacement IDs ([`REPLACE_IDS`]) and at
/// least one molecule, reaction or sketch ([`NO_DRAWING`]).
pub(crate) fn decode(args: Value) -> Result<Compose, OpError> {
    let Arguments {
        proposal,
        style_document,
    } = arguments(args)?;
    let proposal: Proposal =
        serde_json::from_value(proposal).map_err(|error| invalid(error.to_string()))?;
    proposal.validate().map_err(invalid)?;
    if !proposal.replace_ids.is_empty() {
        return Err(invalid(REPLACE_IDS));
    }
    if !proposal.has_drawing() {
        return Err(invalid(NO_DRAWING));
    }
    Ok(Compose {
        proposal,
        style_document,
    })
}

/// What a compose reports about the stored drawing, as the assistant's
/// `canvas_preview` describes a draft (canvas_tools.rs).
#[derive(Debug, Serialize)]
struct Report {
    atoms: usize,
    bonds: usize,
    arrows: usize,
    bounds: Option<(Point, Point)>,
    review_issues: Vec<String>,
    changes: Vec<String>,
}

/// The envelope value of a compose.
#[derive(Debug, Serialize)]
struct Composed {
    document: DocHandle,
    revision: Revision,
    #[serde(flatten)]
    report: Report,
}

fn failed(message: impl AsRef<str>) -> OpError {
    OpError::new(ErrorKind::Failed, message)
}

/// Lays out `proposal`, forwarding structure progress when the client asked
/// for it. Without a progress sink no channel exists, so the layout clones
/// and composes no preview documents.
async fn layout(
    ctx: &Context,
    engine: &LocalEngine,
    proposal: &Proposal,
    settings: &DrawingSettings,
) -> Result<Document, String> {
    let Some(sink) = ctx.progress() else {
        return render_progress(engine, proposal, settings, None::<&Sender<Event>>).await;
    };
    let (tx, rx) = mpsc::channel(8);
    let forwarder = tokio::spawn(progress::forward(rx, sink));
    let rendered = render_progress(engine, proposal, settings, Some(&tx)).await;
    // The forwarder ends once the last sender is gone; waiting for it
    // delivers every queued report before the call ends.
    drop(tx);
    let _ = forwarder.await;
    rendered
}

/// Composes the proposal into a new session document.
///
/// - Settings: with `style_document`, [`DrawingSettings::for_document`] of
///   a read snapshot of it, as the app derives a tab's settings; with
///   `null`, [`DrawingSettings::default`], as the assistant composes
///   without a canvas.
/// - Layout: [`render_progress`], then on the blocking pool
///   [`composition::finish_layout`] and [`review::quality`], as the
///   assistant finishes a draft (src/assistant/codex.rs). A sketch adds
///   [`sketch::REVIEW_NOTE`] as a warning.
/// - The store's create is the effect. It is not idempotent: a cancel that
///   arrives after it leaves a listed document the caller can close.
///
/// Cancellation and the deadline are honored between these stages, not
/// within the layout, which [`Proposal::validate`] bounds (at most 32
/// molecules).
///
/// `{value: {document, revision, atoms, bonds, arrows, bounds, review_issues,
/// changes}, warnings, validation, versions}`. `bounds` is the drawing's
/// `[min, max]` corners in world units, or `null`. A layout error is
/// [`ErrorKind::Failed`] with its message.
pub(crate) async fn compose(
    mut ctx: Context,
    store: Arc<dyn Documents>,
    engine: LocalEngine,
    who: Principal,
    versions: Versions,
    budgets: Budgets,
    compose: Compose,
) -> Result<ToolResult, OpError> {
    let Compose {
        proposal,
        style_document,
    } = compose;
    let settings = match style_document {
        None => DrawingSettings::default(),
        Some(handle) => {
            let store = Arc::clone(&store);
            let who = who.clone();
            ctx.blocking(move || {
                let snapshot = store.snapshot(&who, &handle, Access::Read)?;
                Ok(DrawingSettings::for_document(&snapshot.doc))
            })
            .await?
        }
    };
    ctx.checkpoint()?;
    let rendered = layout(&ctx, &engine, &proposal, &settings).await;
    ctx.checkpoint()?;
    let mut doc = rendered.map_err(failed)?;
    let max_objects = budgets.max_objects;
    let (doc, report, messages) = ctx
        .blocking(move || {
            let changes = composition::finish_layout(&proposal, &mut doc)
                .into_iter()
                .collect();
            let review_issues = review::quality(&doc, &proposal.composition);
            let warnings = if proposal.sketch.is_some() {
                vec![sketch::REVIEW_NOTE.to_owned()]
            } else {
                Vec::new()
            };
            let count = objects(&doc);
            if count > max_objects {
                return Err(OpError::new(
                    ErrorKind::Budget,
                    format!("The drawing has {count} objects; the limit is {max_objects}"),
                ));
            }
            let report = Report {
                atoms: doc.atoms.len(),
                bonds: doc.bonds.len(),
                arrows: doc.arrows.len(),
                bounds: scene::selection_bounds(&doc, &doc.all_ids()),
                review_issues,
                changes,
            };
            Ok((doc, report, warnings))
        })
        .await?;
    let created = ctx.effect(move || store.create(&who, doc)).await?;
    let value = envelope_json(&Envelope {
        value: Composed {
            document: created.handle,
            revision: created.revision,
            report,
        },
        warnings: warnings(messages),
        validation: ValidationStatus::Valid,
        versions,
    });
    Ok(ToolResult {
        value,
        images: Vec::new(),
        files: Vec::new(),
        is_error: false,
    })
}

#[cfg(test)]
mod tests;
