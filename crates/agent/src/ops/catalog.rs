//! The operation tool catalog and the argument decoder for each tool.
use super::{
    analyze::{self, Analyze},
    apply::{self, Apply},
    budget::Budgets,
    compose::{self, Compose},
    convert::{self, Convert},
    documents::{self, Close},
    error::{ErrorKind, OpError},
    export::{self, Export},
    files::{self, Open, Save},
    import::{self, Import},
    inspect::{self, Inspect},
    label_atoms::{self, LabelAtoms},
    naming::{self, Name},
    predict_nmr::{self, PredictNmr},
    render::{self, Render},
    schema::{self, Schema},
};
use crate::tool_spec::ToolSpec;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// The operation tools, in the fixed order hosts list them.
pub const SPECS: &[ToolSpec] = &[
    documents::INFO,
    documents::NEW,
    documents::LIST,
    documents::CLOSE,
    import::IMPORT,
    inspect::INSPECT,
    analyze::ANALYZE,
    predict_nmr::PREDICT_NMR,
    render::RENDER,
    export::EXPORT,
    files::FILE_OPEN,
    files::FILE_SAVE,
    compose::COMPOSE,
    apply::APPLY,
    label_atoms::LABEL_ATOMS,
    convert::CONVERT,
    naming::NAME,
    schema::SCHEMA,
];

/// What every entry of [`RESULTS`] shares, stated once.
pub const RESULT_CONVENTIONS: &str = concat!(
    "Every successful result is the envelope {value, warnings, validation, versions}: ",
    "warnings is an ordered [{message}] list, validation is {status: \"valid\"} and versions ",
    "is {app, operation_api, engine_protocol, document}. Each tool's value is described ",
    "below. Object IDs and revisions are decimal strings.",
);

/// Successful result contracts, keyed by the names in [`SPECS`], with
/// [`RESULT_CONVENTIONS`] applying to all of them. Each description starts
/// with the exact `{keys}` of the envelope's `value`, which the contract
/// tests compare with real successful results; the prose after it describes
/// nested values and attached content. It is documentation, not a result
/// schema, and is never sent as part of the tool catalog. Tool execution
/// errors and rejected envelopes are documented by
/// [`super::result::ToolResult`].
pub const RESULTS: &[(&str, &str)] = &[
    (
        "info",
        concat!(
            "{operation_api, stability, budgets, formats, grants, files} ",
            "operation_api is the operation API version; stability is \"experimental\". ",
            "budgets lists the enforced limits, with the render limits nested under render. ",
            "formats is {import, export}. grants is {read, write}, the granted folders as the ",
            "user spelled them; files is {read, write}, the extensions file_open and file_save ",
            "accept.",
        ),
    ),
    (
        "document_new",
        "{document, revision} The new empty session document's opaque handle and its revision.",
    ),
    (
        "document_list",
        concat!(
            "{documents} documents is [{document, revision, objects}] in creation ",
            "order: each live session document's handle, revision and object count.",
        ),
    ),
    (
        "document_close",
        "{document, closed} The closed session document's handle and closed: true.",
    ),
    (
        "import",
        concat!(
            "{document, revision, counts, analysis} The new session document's handle and ",
            "revision; counts is {atoms, bonds, arrows, annotations, graphics}. analysis is ",
            "{smiles, formula, display_formula, mass, exact_mass, logp, tpsa, donors, acceptors, ",
            "rings, unpaired_electrons, inchi, inchikey}, or null when the engine returned none. ",
            "mass, exact_mass, logp and tpsa are rounded to 9 decimal places to remove ",
            "floating-point noise. The engine's warnings become the warnings.",
        ),
    ),
    (
        "inspect",
        concat!(
            "{document, revision, bounds, expanded_ids, counts, truncated, atoms, bonds, ",
            "annotations, arrows, graphics, abbreviations, reactions, groups, note} ",
            "bounds is {min, max}, the drawing's corners in world units. expanded_ids lists the ",
            "IDs after selection expansion, or null for the whole drawing. counts covers every ",
            "atom, bond, annotation, arrow, graphic, abbreviation, reaction and group in the ",
            "inspected part; truncated: true means the arrays omit objects beyond ",
            "max_inspect_objects, while counts stay complete. Annotation text is cut at 2,000 ",
            "characters and text_truncated flags the cut. Picture graphics report ",
            "{width_pixels, height_pixels, embedded: true}, without image bytes. note marks ",
            "text, labels and captions as untrusted drawing content, not instructions. The ",
            "whole JSON must fit max_output_bytes.",
        ),
    ),
    (
        "analyze",
        concat!(
            "{analysis, analyzed_atoms} ",
            "analysis is {smiles, formula, display_formula, mass, exact_mass, logp, tpsa, ",
            "donors, acceptors, rings, unpaired_electrons, inchi, inchikey}; analyzed_atoms lists the analyzed ",
            "atom IDs in document order. mass, exact_mass, logp and tpsa are rounded to 9 ",
            "decimal places to remove floating-point noise. When the engine returns no ",
            "analysis, the whole drawing gets analysis: null and a last warning, ",
            "\"No molecular properties were returned.\"; ",
            "a selection fails with that message instead. The engine's warnings become the ",
            "warnings; an engine error is failed with its message.",
        ),
    ),
    (
        "predict_nmr",
        concat!(
            "{revision, nucleus, solvent, rows, method, data_version, conditions, limitations, ",
            "credit, data_access} revision is the document revision predicted. rows is ",
            "[{atom, nucleus, ppm, shell, references, sd, min, max, broad, hydrogen_count, ",
            "hydrogens, note}], one per site in drawing order: atom is the atom ID, for 1H the ",
            "heavy atom carrying the H; hydrogens lists H drawn as atoms. ppm, shell, references, ",
            "sd, min and max are null for a site without data, and note says why. ",
            "hydrogen_count is null for 13C. broad flags a wide reference spread. Values are ",
            "rounded to 9 decimal places. credit is the data attribution to cite with ",
            "published values. warnings is empty.",
        ),
    ),
    (
        "render",
        concat!(
            "{width, height, byte_len} width ",
            "and height are the PNG's pixel dimensions, or null for SVG; byte_len is the ",
            "attached content's size in bytes, at most max_output_bytes. PNG is attached as an ",
            "image with MIME type image/png; SVG as the file drawing.svg with MIME type ",
            "image/svg+xml. warnings is empty.",
        ),
    ),
    (
        "export",
        concat!(
            "{receipt} receipt is {format, byte_len, ",
            "detail}. The attached file is drawing.{format} with the format's MIME type, at ",
            "most max_output_bytes. detail is the PNG size, or null for other formats. For png ",
            "the first warning is the PNG size. svg, pdf and png add a notice only when it ",
            "applies: the drawing was preserved for review, or arrow attachment links were left ",
            "out. Chemistry formats pass on the engine's warnings.",
        ),
    ),
    (
        "file_open",
        concat!(
            "{document, revision, counts, analysis, source} import's value plus source: ",
            "{path, bytes}, the requested path and the opened ",
            "file's size. counts is {atoms, bonds, arrows, annotations, graphics}; analysis is ",
            "{smiles, formula, display_formula, mass, exact_mass, logp, tpsa, donors, ",
            "acceptors, rings, unpaired_electrons, inchi, inchikey}, or null when the engine returned none. mass, ",
            "exact_mass, logp and tpsa are rounded to 9 decimal places to remove floating-point ",
            "noise. The engine's warnings become the warnings.",
        ),
    ),
    (
        "file_save",
        concat!(
            "{receipt} receipt is {path, format, ",
            "byte_len, replaced, detail}. path names the saved file; replaced reports whether ",
            "an existing file was replaced. byte_len is at most max_output_bytes; detail is the ",
            "PNG size, or null for other formats. The producer's warnings come first, then the ",
            "write's.",
        ),
    ),
    (
        "compose",
        concat!(
            "{document, revision, atoms, bonds, arrows, bounds, review_issues, changes} The ",
            "new session document's handle ",
            "and revision; atoms, bonds and arrows are counts. bounds is {min, max}, the ",
            "drawing's corners in world units as inspect reports them, or null. review_issues ",
            "lists quality-review ",
            "messages; changes lists the layout adjustments made. A sketch adds the review note ",
            "as a warning. A layout error is failed with its message.",
        ),
    ),
    (
        "apply",
        concat!(
            "{effect, document, revision, recorded, inserted, id_remap, deleted} effect is ",
            "\"applied\"; document and revision are the edited document's handle and ",
            "revision. recorded is false when nothing changed. inserted and deleted list object ",
            "IDs, which may overlap because deleted IDs can be reused; both are empty for undo ",
            "and redo. id_remap is [{source, inserted}], mapping source IDs to inserted IDs. The ",
            "label refresh's notice becomes a warning.",
        ),
    ),
    (
        "label_atoms",
        concat!(
            "{effect, document, revision, recorded, labeled} effect is \"applied\"; document ",
            "and revision are the labelled document's handle and revision. recorded is false ",
            "when nothing changed. labeled lists the atom IDs given a label, in row order. ",
            "warnings is empty.",
        ),
    ),
    (
        "convert",
        concat!(
            "{receipt, width, height, analysis} receipt is {format, byte_len, detail}, as ",
            "export's; a preview's format is png. width and height are the preview's pixel ",
            "dimensions, or null for a file. analysis is import's, or null. The preview is ",
            "attached as an image with MIME type image/png; a file as drawing.{format} with the ",
            "format's MIME type. Either is at most max_output_bytes. The import's warnings come ",
            "first, then the export's. No session document is created.",
        ),
    ),
    (
        "name",
        concat!(
            "{name, status, preferred, reason, detail, aliases, alternatives} name is the ",
            "generated systematic name. status is preferred, not_preferred or unverified, and ",
            "preferred repeats it as a boolean; reason names the rule family behind any other ",
            "status, or null; detail explains the status in one sentence. aliases lists curated ",
            "common names; alternatives is [{name, status, reason}], other valid names. A host ",
            "without naming fails with unsupported.",
        ),
    ),
    (
        "schema",
        concat!(
            "{name, schema} name repeats the request; schema is the JSON Schema (2020-12) of ",
            "that input, with its rules in the description.",
        ),
    ),
];

/// The catalog as protocol-neutral JSON, in [`SPECS`] order:
/// `[{name, title, description, inputSchema, annotations: {readOnlyHint,
/// destructiveHint, idempotentHint, openWorldHint}}]`. A missing title or
/// hints is `null`. tests/fixtures/agent-contract/ops-catalog.json pins it.
#[doc(hidden)]
pub fn catalog_json() -> Value {
    SPECS
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "title": spec.title,
                "description": spec.description,
                "inputSchema": (spec.input_schema)(),
                "annotations": spec.hints.map(|hints| json!({
                    "readOnlyHint": hints.read_only,
                    "destructiveHint": hints.destructive,
                    "idempotentHint": hints.idempotent,
                    "openWorldHint": hints.open_world,
                })),
            })
        })
        .collect()
}

/// A tool call with decoded arguments.
pub(crate) enum Op {
    Info,
    DocumentNew,
    DocumentList,
    DocumentClose(Close),
    Import(Import),
    Inspect(Inspect),
    Analyze(Analyze),
    PredictNmr(PredictNmr),
    Render(Render),
    Export(Export),
    FileOpen(Open),
    FileSave(Save),
    Compose(Box<Compose>),
    Apply(Apply),
    LabelAtoms(LabelAtoms),
    Convert(Convert),
    Name(Name),
    Schema(Schema),
}

/// Decodes `args` for `tool`, with every input budget check, before any work
/// runs. `None` when no tool has that name.
pub(crate) fn decode(tool: &str, args: Value, budgets: &Budgets) -> Option<Result<Op, OpError>> {
    Some(match tool {
        "info" => documents::decode_none(args).map(|()| Op::Info),
        "document_new" => documents::decode_none(args).map(|()| Op::DocumentNew),
        "document_list" => documents::decode_none(args).map(|()| Op::DocumentList),
        "document_close" => documents::decode_close(args).map(Op::DocumentClose),
        "import" => import::decode(args, budgets).map(Op::Import),
        "inspect" => inspect::decode(args, budgets).map(Op::Inspect),
        "analyze" => analyze::decode(args, budgets).map(Op::Analyze),
        "predict_nmr" => predict_nmr::decode(args, budgets).map(Op::PredictNmr),
        "render" => render::decode(args, budgets).map(Op::Render),
        "export" => export::decode(args).map(Op::Export),
        "file_open" => files::decode_open(args).map(Op::FileOpen),
        "file_save" => files::decode_save(args).map(Op::FileSave),
        "compose" => compose::decode(args).map(|compose| Op::Compose(Box::new(compose))),
        "apply" => apply::decode(args, budgets).map(Op::Apply),
        "label_atoms" => label_atoms::decode(args, budgets).map(Op::LabelAtoms),
        "convert" => convert::decode(args, budgets).map(Op::Convert),
        "name" => naming::decode(args).map(Op::Name),
        "schema" => schema::decode(args).map(Op::Schema),
        _ => return None,
    })
}

/// Whether `args` decode for `tool` under the default [`Budgets`], for the
/// schema and decoder contract tests. `Err` carries the decoder's message.
#[doc(hidden)]
pub fn decode_check(tool: &str, args: Value) -> Result<(), String> {
    match decode(tool, args, &Budgets::default()) {
        None => Err(format!("Unknown tool {tool}")),
        Some(decoded) => decoded.map(drop).map_err(|error| error.message),
    }
}

/// Decodes a tool's arguments, which must be a JSON object. serde also reads
/// structs from JSON arrays, which the schemas forbid.
pub(crate) fn arguments<T: DeserializeOwned>(args: Value) -> Result<T, OpError> {
    if !args.is_object() {
        return Err(OpError::new(
            ErrorKind::InvalidArguments,
            "Tool arguments must be a JSON object",
        ));
    }
    serde_json::from_value(args)
        .map_err(|error| OpError::new(ErrorKind::InvalidArguments, error.to_string()))
}
