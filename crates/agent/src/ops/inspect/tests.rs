use super::*;
use crate::{
    attachments::Kind,
    document::Arrow,
    ops::{
        headless::HeadlessHost,
        host::{Call, ToolHost},
        wire::RequestId,
    },
    pictures::Picture,
    scientific::SymbolKind,
};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicI64, Ordering},
};

/// A 2 × 1 RGBA PNG.
const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8AAQv8BD/kD/YURmXYAAAAASUVORK5CYII=";

/// The JSON keys a summary may hold numbers under, besides `counts`.
const NUMERIC_KEYS: [&str; 10] = [
    "x",
    "y",
    "charge",
    "isotope",
    "explicit_h",
    "radical_electrons",
    "order",
    "coefficient",
    "width_pixels",
    "height_pixels",
];

fn host_with(budgets: Budgets) -> HeadlessHost {
    HeadlessHost::new("9.8.7", budgets)
}

fn call(tool: &str, arguments: Value) -> Call {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    Call {
        principal: Principal::local(),
        request: RequestId::Int(NEXT.fetch_add(1, Ordering::Relaxed)),
        tool: tool.into(),
        arguments,
        progress: None,
    }
}

/// The IDs of [`everything`]'s objects.
struct Ids {
    ring: [u64; 3],
    point: u64,
    metal: u64,
    oxide: u64,
    variable: u64,
    methyl: u64,
    caption: u64,
    arrow: u64,
    picture: u64,
    symbol: u64,
    group: u64,
}

/// Every object kind: a three-carbon ring under a multi-center attachment
/// point bonded to iron, a charged isotopic oxygen radical, a variable atom,
/// an abbreviation, a caption, an arrow, a picture, a symbol, a reaction and
/// a group.
fn everything() -> (Document, Ids) {
    let mut doc = Document::default();
    let ring = [
        doc.add_atom("C", Point::new(0., 0.)),
        doc.add_atom("C", Point::new(30., 0.)),
        doc.add_atom("C", Point::new(15., 26.)),
    ];
    doc.add_bond(ring[0], ring[1], 1, "plain");
    doc.add_bond(ring[1], ring[2], 2, "plain");
    doc.add_bond(ring[2], ring[0], 1, "wedge");
    let point = attachments::add(&mut doc, &ring, Kind::MultiCenter).unwrap();
    let metal = doc.add_atom("Fe", Point::new(15., 60.));
    doc.add_bond(point, metal, 1, "plain");
    let oxide = doc.add_atom("O", Point::new(200., 0.));
    let atom = doc.atom_mut(oxide).unwrap();
    atom.charge = -1;
    atom.isotope = 18;
    atom.explicit_h = 1;
    atom.radical_electrons = 1;
    let variable = doc.add_atom("*", Point::new(240., 0.));
    doc.atom_mut(variable).unwrap().display.variable = Some("R".into());
    let methyl = doc.add_atom("C", Point::new(270., 0.));
    doc.abbreviations.push(
        serde_json::from_value(json!({"label": "Me", "anchor": methyl, "members": [methyl]}))
            .unwrap(),
    );
    let caption = doc.next_id();
    doc.annotations.push(crate::document::Annotation {
        id: caption,
        position: Point::new(100., -20.),
        text: "heat".into(),
        format: Default::default(),
    });
    let arrow = doc.next_id();
    doc.arrows.push(Arrow {
        start_anchor: None,
        end_anchor: None,
        id: arrow,
        start: Point::new(80., 0.),
        end: Point::new(160., 0.),
        kind: "forward".into(),
        control: None,
        cubic: None,
        style: None,
    });
    let png: Picture = serde_json::from_value(json!(PNG_BASE64)).unwrap();
    let picture = doc.next_id();
    doc.graphics
        .push(png.graphic(picture, Point::new(0., 100.)));
    let symbol = doc.next_id();
    let mut plus = png.graphic(symbol, Point::new(40., 100.));
    plus.picture = None;
    plus.kind = GraphicKind::Symbol(SymbolKind::Plus);
    doc.graphics.push(plus);
    doc.reactions.push(
        serde_json::from_value(json!({
            "arrow": arrow,
            "reactants": [{"atoms": [ring[0], ring[1], ring[2], point, metal], "coefficient": 2}],
            "products": [{"atoms": [oxide]}],
            "annotations": [caption],
        }))
        .unwrap(),
    );
    let group = doc.next_id();
    doc.groups.push(
        serde_json::from_value(
            json!({"id": group, "members": [oxide, variable], "integral": true}),
        )
        .unwrap(),
    );
    let ids = Ids {
        ring,
        point,
        metal,
        oxide,
        variable,
        methyl,
        caption,
        arrow,
        picture,
        symbol,
        group,
    };
    (doc, ids)
}

/// A host holding `doc` as a session document, and its handle.
fn hosting(doc: Document, budgets: Budgets) -> (HeadlessHost, String) {
    let host = host_with(budgets);
    let created = host.store().create(&Principal::local(), doc).unwrap();
    (host, created.handle.as_str().to_owned())
}

async fn inspect_call(host: &HeadlessHost, handle: &str, ids: Value) -> ToolResult {
    host.call(call("inspect", json!({"document": handle, "ids": ids})))
        .await
        .unwrap()
}

async fn inspect_ok(host: &HeadlessHost, handle: &str, ids: Value) -> Value {
    let result = inspect_call(host, handle, ids).await;
    assert!(!result.is_error, "{:?}", result.value);
    assert!(result.images.is_empty() && result.files.is_empty());
    Value::Object(result.value)
}

fn strings(ids: &[u64]) -> Value {
    ids.iter().map(|id| Value::from(id.to_string())).collect()
}

fn listed_ids(summary: &Value, list: &str) -> Value {
    summary[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].clone())
        .collect()
}

/// The keys numbers appear under, outside the top-level `counts`.
fn numeric_keys(value: &Value, key: &str, found: &mut BTreeSet<String>) {
    match value {
        Value::Number(_) => {
            found.insert(key.to_owned());
        }
        Value::Array(items) => {
            for item in items {
                numeric_keys(item, key, found);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                numeric_keys(value, key, found);
            }
        }
        _ => {}
    }
}

#[test]
fn ids_over_the_budget_are_refused_at_decode_time() {
    let budgets = Budgets::default();
    let ids = |n: usize| vec![json!("1"); n];
    let decoded = |n| decode(json!({"document": "doc_1", "ids": ids(n)}), &budgets).map(drop);
    assert_eq!(decoded(budgets.max_ids), Ok(()));
    let error = decoded(budgets.max_ids + 1).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    assert_eq!(
        error.message,
        "5001 object IDs were given; at most 5000 are allowed per request"
    );
    let omitted = decode(json!({"document": "doc_1"}), &budgets).unwrap_err();
    assert_eq!(omitted.kind, ErrorKind::InvalidArguments);
    assert_eq!(omitted.message, "missing field `ids`");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_whole_drawing_is_summarized_with_string_ids() {
    let (doc, ids) = everything();
    let (host, handle) = hosting(doc.clone(), Budgets::default());
    let summary = inspect_ok(&host, &handle, Value::Null).await;
    assert_eq!(summary["document"], handle.as_str());
    assert_eq!(summary["revision"], "0");
    assert_eq!(summary["expanded_ids"], Value::Null);
    assert_eq!(summary["truncated"], false);
    assert_eq!(summary["note"], NOTE);
    assert_eq!(
        summary["versions"],
        versions_json(&Versions::current("9.8.7"))
    );
    let (min, max) = doc.bounds();
    assert_eq!(
        summary["bounds"],
        json!({"min": {"x": min.x, "y": min.y}, "max": {"x": max.x, "y": max.y}})
    );
    assert_eq!(
        summary["counts"],
        json!({"atoms": 8, "bonds": 4, "annotations": 1, "arrows": 1, "graphics": 2, "abbreviations": 1, "reactions": 1, "groups": 1})
    );
    let atom_ids: Vec<u64> = doc.atoms.iter().map(|atom| atom.id).collect();
    assert_eq!(listed_ids(&summary, "atoms"), strings(&atom_ids));
    let atom = |id: u64| {
        summary["atoms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|atom| atom["id"].as_str().and_then(|text| text.parse().ok()) == Some(id))
            .unwrap()
            .clone()
    };
    assert_eq!(atom(ids.point)["centroid"], strings(&ids.ring));
    assert_eq!(atom(ids.point)["element"], "*");
    assert_eq!(atom(ids.variable)["variable"], "R");
    assert_eq!(atom(ids.metal)["variable"], Value::Null);
    let oxide = atom(ids.oxide);
    assert_eq!(
        (
            &oxide["charge"],
            &oxide["isotope"],
            &oxide["explicit_h"],
            &oxide["radical_electrons"],
            &oxide["aromatic"],
        ),
        (&json!(-1), &json!(18), &json!(1), &json!(1), &json!(false))
    );
    assert_eq!(
        summary["bonds"][2],
        json!({"a": ids.ring[2].to_string(), "b": ids.ring[0].to_string(), "order": 1, "display": "wedge", "stereo": null})
    );
    assert_eq!(
        summary["annotations"],
        json!([{"id": ids.caption.to_string(), "text": "heat", "text_truncated": false, "position": {"x": 100.0, "y": -20.0}}])
    );
    assert_eq!(
        summary["arrows"],
        json!([{"id": ids.arrow.to_string(), "kind": "forward", "start": {"x": 80.0, "y": 0.0}, "end": {"x": 160.0, "y": 0.0}}])
    );
    assert_eq!(
        summary["graphics"],
        json!([
            {"id": ids.picture.to_string(), "kind": "picture", "picture": {"width_pixels": 2, "height_pixels": 1, "embedded": true}},
            {"id": ids.symbol.to_string(), "kind": {"symbol": "plus"}, "picture": null},
        ])
    );
    assert_eq!(
        summary["abbreviations"],
        json!([{"label": "Me", "anchor": ids.methyl.to_string(), "members": [ids.methyl.to_string()]}])
    );
    let mut reactants = ids.ring.to_vec();
    reactants.extend([ids.point, ids.metal]);
    assert_eq!(
        summary["reactions"],
        json!([{
            "arrow": ids.arrow.to_string(),
            "reactants": [{"atoms": strings(&reactants), "coefficient": 2}],
            "products": [{"atoms": [ids.oxide.to_string()], "coefficient": 1}],
            "agents": [],
            "annotations": [ids.caption.to_string()],
        }])
    );
    assert_eq!(
        summary["groups"],
        json!([{"id": ids.group.to_string(), "members": strings(&[ids.oxide, ids.variable]), "integral": true}])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn numbers_appear_only_under_allowlisted_keys() {
    let (doc, _) = everything();
    let (host, handle) = hosting(doc, Budgets::default());
    let mut summary = inspect_ok(&host, &handle, Value::Null).await;
    let object = summary.as_object_mut().unwrap();
    // versions are the result's, not the drawing's.
    object.remove("versions").unwrap();
    let counts = object.remove("counts").unwrap();
    assert!(counts.as_object().unwrap().values().all(Value::is_u64));
    let mut found = BTreeSet::new();
    numeric_keys(&summary, "", &mut found);
    // The fixture reaches every allowlisted key, and nothing else is numeric.
    assert_eq!(found, NUMERIC_KEYS.map(str::to_owned).into());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_ids_name_the_first_missing_one() {
    let (doc, ids) = everything();
    let (host, handle) = hosting(doc, Budgets::default());
    let missing = ids.group + 100;
    for (selection, first) in [
        (json!([missing.to_string()]), missing),
        (
            json!([
                ids.oxide.to_string(),
                (missing + 1).to_string(),
                missing.to_string()
            ]),
            missing + 1,
        ),
        // Groups share the ID counter but are not objects.
        (json!([ids.group.to_string()]), ids.group),
    ] {
        let result = inspect_call(&host, &handle, selection).await;
        assert!(result.is_error);
        assert_eq!(
            result.value["error"],
            json!({"code": "unknown_object", "message": format!("Object {first} is not in the drawing")})
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_annotation_only_selection_lists_just_the_annotation() {
    let (doc, ids) = everything();
    let (host, handle) = hosting(doc, Budgets::default());
    let summary = inspect_ok(&host, &handle, strings(&[ids.caption])).await;
    assert_eq!(summary["expanded_ids"], strings(&[ids.caption]));
    assert_eq!(
        summary["counts"],
        json!({"atoms": 0, "bonds": 0, "annotations": 1, "arrows": 0, "graphics": 0, "abbreviations": 0, "reactions": 0, "groups": 0})
    );
    assert_eq!(listed_ids(&summary, "annotations"), strings(&[ids.caption]));
    for list in [
        "atoms",
        "bonds",
        "arrows",
        "graphics",
        "abbreviations",
        "reactions",
        "groups",
    ] {
        assert_eq!(summary[list], json!([]), "{list}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selections_expand_to_attachment_points_and_whole_abbreviations() {
    let (doc, ids) = everything();
    let (host, handle) = hosting(doc, Budgets::default());
    let mut expected = ids.ring.to_vec();
    expected.push(ids.point);
    // A selected attachment point pulls in its targets.
    let summary = inspect_ok(&host, &handle, strings(&[ids.point])).await;
    assert_eq!(summary["expanded_ids"], strings(&expected));
    assert_eq!(listed_ids(&summary, "atoms"), strings(&expected));
    assert_eq!(summary["counts"]["bonds"], 3);
    // Complete targets carry their attachment point, but not the iron the
    // point is bonded to.
    let summary = inspect_ok(&host, &handle, strings(&ids.ring)).await;
    assert_eq!(summary["expanded_ids"], strings(&expected));
    // A selected member pulls in its whole abbreviation; the reaction and
    // group stay out until all their members are selected.
    let summary = inspect_ok(
        &host,
        &handle,
        strings(&[ids.methyl, ids.oxide, ids.variable]),
    )
    .await;
    assert_eq!(
        summary["expanded_ids"],
        strings(&[ids.oxide, ids.variable, ids.methyl])
    );
    assert_eq!(summary["counts"]["abbreviations"], 1);
    assert_eq!(summary["counts"]["groups"], 1);
    assert_eq!(summary["counts"]["reactions"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listed_objects_are_capped_with_full_counts() {
    let (doc, _) = everything();
    let (host, handle) = hosting(
        doc.clone(),
        Budgets {
            max_inspect_objects: 10,
            ..Budgets::default()
        },
    );
    let summary = inspect_ok(&host, &handle, Value::Null).await;
    assert_eq!(summary["truncated"], true);
    assert_eq!(summary["counts"]["atoms"], 8);
    assert_eq!(summary["counts"]["bonds"], 4);
    assert_eq!(summary["atoms"].as_array().unwrap().len(), 8);
    assert_eq!(summary["bonds"].as_array().unwrap().len(), 2);
    for list in [
        "annotations",
        "arrows",
        "graphics",
        "abbreviations",
        "reactions",
        "groups",
    ] {
        assert_eq!(summary[list], json!([]), "{list}");
    }
    let total = doc.atoms.len()
        + doc.bonds.len()
        + doc.annotations.len()
        + doc.arrows.len()
        + doc.graphics.len()
        + doc.abbreviations.len()
        + doc.reactions.len()
        + doc.groups.len();
    let (host, handle) = hosting(
        doc,
        Budgets {
            max_inspect_objects: total,
            ..Budgets::default()
        },
    );
    let summary = inspect_ok(&host, &handle, Value::Null).await;
    assert_eq!(summary["truncated"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn long_annotation_text_is_cut_by_characters_and_flagged() {
    let (mut doc, ids) = everything();
    let at_limit = doc.next_id();
    let mut caption = doc.annotations[0].clone();
    caption.id = at_limit;
    caption.text = "é".repeat(MAX_TEXT_CHARS);
    doc.annotations.push(caption);
    doc.annotations[0].text = "é".repeat(MAX_TEXT_CHARS + 1);
    let (host, handle) = hosting(doc, Budgets::default());
    let summary = inspect_ok(&host, &handle, strings(&[ids.caption, at_limit])).await;
    for (annotation, truncated) in summary["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .zip([true, false])
    {
        assert_eq!(annotation["text"], "é".repeat(MAX_TEXT_CHARS));
        assert_eq!(annotation["text_truncated"], truncated);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_summary_must_fit_the_output_budget() {
    let (doc, _) = everything();
    let (host, handle) = hosting(
        doc,
        Budgets {
            max_output_bytes: 1_000,
            ..Budgets::default()
        },
    );
    let result = inspect_call(&host, &handle, Value::Null).await;
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "budget");
    let message = result.value["error"]["message"].as_str().unwrap();
    assert!(
        message.ends_with(
            "max_output_bytes allows at most 1000. Pass ids to inspect part of the drawing"
        ),
        "{message}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_handle_is_an_unknown_document() {
    let host = host_with(Budgets::default());
    let result = inspect_call(&host, "doc_0123456789abcdef0123456789abcdef", Value::Null).await;
    assert!(result.is_error);
    assert_eq!(result.value["error"]["code"], "unknown_document");
}
