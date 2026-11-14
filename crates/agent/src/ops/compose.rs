//! The `compose` tool: lay out a Proposal as a new session document, as the
//! in-app assistant composes its drafts.
use super::{
    budget::{Budgets, objects},
    catalog::arguments,
    error::{ErrorKind, OpError},
    exec::Context,
    policy::Access,
    progress,
    result::ToolResult,
    store::Documents,
    wire::{DocHandle, Principal, Revision, described, handle_schema, nullable, warnings},
};
use crate::{
    DrawingSettings, Proposal, composition,
    document::{Document, Point},
    engine::LocalEngine,
    envelope::Versions,
    progress::Event,
    render_progress, review, scene, sketch,
    tool_spec::{Hints, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
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
    description: "Lay out a Proposal as a new session document, as ReShiki's assistant composes its drafts: molecules and reactions from SMILES, or one editable sketch diagram, never both. Returns the document's handle and revision, its atom, bond and arrow counts, its bounds, layout review issues and the layout changes made. Use render to look at it. Every call creates another document; close the ones you no longer need with document_close.",
    input_schema: schema,
    hints: Some(Hints {
        read_only: false,
        destructive: false,
        idempotent: false,
        open_world: false,
    }),
};

/// Proposal subschemas that occur more than once, published once under
/// `$defs`: each name and where its first copy is.
const SHARED: &[(&str, &str)] = &[
    ("molecules", "/properties/molecules"),
    (
        "point",
        "/properties/sketch/anyOf/1/properties/arrows/items/properties/start",
    ),
    (
        "color",
        "/properties/sketch/anyOf/1/properties/captions/items/properties/color",
    ),
];

/// Sentences of the sketch description that repeat its first sentences or
/// the descriptions of `centroids.kind` and `bonds.ring_arc`.
const REPEATED: &[&str] = &[
    " Use centroids with kind multi_center for haptic contacts (all target atoms), kind variable for alternative attachment positions, and null only for nonchemical drawing anchors.",
    " Use ring_arc on consecutive ring bonds for partial inner curves, preserving their underlying bond orders.",
    " Leave molecules and reactions empty.",
];

/// The Proposal schema is derived from the assistant's [`crate::schema`],
/// which stays as `canvas_preview` publishes it, and made smaller:
///
/// - `explanation` and `replace_ids` are left out, because compose always
///   creates a new document; [`decode`] fills them in and still accepts them;
/// - each [`SHARED`] subschema is stated once under `$defs`;
/// - the [`REPEATED`] sentences are removed.
fn schema() -> Value {
    let mut proposal = crate::schema();
    for name in ["explanation", "replace_ids"] {
        if let Some(properties) = proposal
            .pointer_mut("/properties")
            .and_then(Value::as_object_mut)
        {
            properties.remove(name);
        }
        if let Some(required) = proposal
            .pointer_mut("/required")
            .and_then(Value::as_array_mut)
        {
            required.retain(|required| required != name);
        }
    }
    if let Some(Value::String(text)) =
        proposal.pointer_mut("/properties/sketch/anyOf/1/description")
    {
        for sentence in REPEATED {
            *text = text.replace(sentence, "");
        }
    }
    let mut defs = Map::new();
    for (name, pointer) in SHARED {
        if let Some(def) = proposal.pointer(pointer).cloned() {
            share(
                &mut proposal,
                &def,
                &json!({"$ref": format!("#/$defs/{name}")}),
            );
            defs.insert((*name).to_owned(), def);
        }
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "proposal": proposal,
            "style_document": described(nullable(handle_schema()), "A session document whose drawing style (bond length, font, line width) and atom label settings to use, or null for the default style."),
        },
        "required": ["proposal", "style_document"],
        "$defs": defs,
    })
}

/// Replaces every subschema of `schema` equal to `def` with `reference`.
fn share(schema: &mut Value, def: &Value, reference: &Value) {
    if schema == def {
        *schema = reference.clone();
        return;
    }
    match schema {
        Value::Object(members) => members
            .values_mut()
            .for_each(|member| share(member, def, reference)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| share(item, def, reference)),
        _ => {}
    }
}

/// The `compose` arguments as sent. The proposal is decoded on its own so
/// its errors read exactly as on the assistant's path.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    proposal: Value,
    #[serde(default)]
    style_document: Option<DocHandle>,
}

/// The decoded `compose` arguments.
#[derive(Debug)]
pub(crate) struct Compose {
    proposal: Proposal,
    style_document: Option<DocHandle>,
}

/// Decodes the arguments before any work runs.
///
/// A proposal without `explanation` gets an empty one; `replace_ids`
/// already defaults to none. The proposal is then decoded and validated
/// exactly as the assistant's Codex Proposal turn does
/// (src/assistant/codex.rs), with the same error text. Then compose's own
/// rules: no replacement IDs ([`REPLACE_IDS`]) and at least one molecule,
/// reaction or sketch ([`NO_DRAWING`]).
pub(crate) fn decode(args: Value) -> Result<Compose, OpError> {
    let Arguments {
        mut proposal,
        style_document,
    } = arguments(args)?;
    if let Some(fields) = proposal.as_object_mut() {
        fields
            .entry("explanation")
            .or_insert_with(|| Value::String(String::new()));
    }
    let proposal: Proposal =
        serde_json::from_value(proposal).map_err(|error| OpError::invalid(error.to_string()))?;
    proposal.validate().map_err(OpError::invalid)?;
    if !proposal.replace_ids.is_empty() {
        return Err(OpError::invalid(REPLACE_IDS));
    }
    if !proposal.has_drawing() {
        return Err(OpError::invalid(NO_DRAWING));
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
/// The successful result contract is in [`super::catalog::RESULTS`].
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
    let mut doc = rendered.map_err(OpError::failed)?;
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
    Ok(ToolResult::valid(
        Composed {
            document: created.handle,
            revision: created.revision,
            report,
        },
        warnings(messages),
        versions,
    ))
}

#[cfg(test)]
mod tests;
