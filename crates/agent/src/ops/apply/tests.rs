use super::*;
use crate::{
    attachments,
    document::{Arrow, Point},
    ops::{
        exec::{Barrier, Hooks},
        headless::HeadlessHost,
        host::{Call, ToolHost},
        policy::UNKNOWN_DOCUMENT,
        session::SessionStore,
        store::{Created, Listed, StepTarget},
        wire::{RequestId, versions_json},
    },
    reactions::{Participant, Reaction},
};
use std::{
    future::Future,
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;

/// A generous bound for every wait, so a bug fails instead of hanging.
const BOUND: Duration = Duration::from_secs(60);

const JOINS: &str =
    "This joins separate reaction participants. Clear their reaction roles before joining them.";

fn host() -> HeadlessHost {
    HeadlessHost::new("9.8.7", Budgets::default())
}

fn who() -> Principal {
    Principal::local()
}

fn request() -> RequestId {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    RequestId::Int(NEXT.fetch_add(1, Ordering::Relaxed))
}

fn call_as(id: RequestId, arguments: Value) -> Call {
    Call {
        principal: who(),
        request: id,
        tool: "apply".into(),
        arguments,
        progress: None,
    }
}

fn call(arguments: Value) -> Call {
    call_as(request(), arguments)
}

/// `elements` bonded in a chain along x from `x`, with IDs from 1.
fn chain(elements: &[&str], x: f32) -> Document {
    let mut doc = Document::default();
    let mut previous = None;
    for (i, element) in elements.iter().enumerate() {
        let id = doc.add_atom(element, Point::new(x + 42. * i as f32, 0.));
        if let Some(previous) = previous {
            doc.add_bond(previous, id, 1, "plain");
        }
        previous = Some(id);
    }
    doc
}

/// `doc` with the computed labels a refresh gives it.
fn labeled(mut doc: Document) -> Document {
    atom_labels::clear_computed(&mut doc);
    Refresh::calculate(&doc, &Refresh::default())
        .unwrap()
        .apply(&mut doc);
    doc
}

fn create(host: &HeadlessHost, doc: Document) -> DocHandle {
    host.store().create(&who(), doc).unwrap().handle
}

fn stored(host: &HeadlessHost, handle: &DocHandle) -> Arc<Document> {
    host.store()
        .snapshot(&who(), handle, Access::Read)
        .unwrap()
        .doc
}

fn revision(host: &HeadlessHost, handle: &DocHandle) -> u64 {
    host.store()
        .snapshot(&who(), handle, Access::Read)
        .unwrap()
        .revision
        .0
}

fn ids_json(ids: &[u64]) -> Value {
    ids.iter().map(|id| Value::from(id.to_string())).collect()
}

fn optional(value: Option<impl ToString>) -> Value {
    value.map_or(Value::Null, |value| Value::from(value.to_string()))
}

fn insert(
    document: &DocHandle,
    source: &DocHandle,
    ids: Option<&[u64]>,
    base: Option<u64>,
    key: Option<&str>,
) -> Value {
    json!({
        "document": document.as_str(),
        "edit": "insert",
        "source": source.as_str(),
        "ids": ids.map_or(Value::Null, ids_json),
        "base_revision": optional(base),
        "idempotency_key": key,
    })
}

fn delete(document: &DocHandle, ids: &[u64], base: u64, key: Option<&str>) -> Value {
    json!({
        "document": document.as_str(),
        "edit": "delete",
        "source": null,
        "ids": ids_json(ids),
        "base_revision": base.to_string(),
        "idempotency_key": key,
    })
}

fn step(document: &DocHandle, edit: &str, base: u64, key: Option<&str>) -> Value {
    json!({
        "document": document.as_str(),
        "edit": edit,
        "source": null,
        "ids": null,
        "base_revision": base.to_string(),
        "idempotency_key": key,
    })
}

/// The result value of a successful apply.
async fn apply_ok(host: &HeadlessHost, arguments: Value) -> Value {
    let result = host.call(call(arguments)).await.unwrap();
    assert!(!result.is_error, "{:?}", result.value);
    assert!(result.images.is_empty() && result.files.is_empty());
    Value::Object(result.value)
}

/// The error of a failed apply: `(code, message)`.
fn error(result: &ToolResult) -> (String, String) {
    assert!(result.is_error, "{:?}", result.value);
    let error = &result.value["error"];
    (
        error["code"].as_str().unwrap().to_owned(),
        error["message"].as_str().unwrap().to_owned(),
    )
}

async fn apply_err(host: &HeadlessHost, arguments: Value) -> (String, String) {
    error(&host.call(call(arguments)).await.unwrap())
}

/// Undoes until nothing is left and returns how many steps were undone.
async fn undo_all(host: &HeadlessHost, handle: &DocHandle) -> usize {
    let mut steps = 0;
    loop {
        let base = revision(host, handle);
        let undone = apply_ok(host, step(handle, "undo", base, None)).await;
        if undone["value"]["recorded"] == false {
            return steps;
        }
        steps += 1;
    }
}

async fn bounded<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(BOUND, future)
        .await
        .expect("timed out")
}

/// Yields until `condition` holds.
async fn until(mut condition: impl FnMut() -> bool) {
    bounded(async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await;
}

/// Waits on a barrier without blocking a runtime worker.
async fn wait(barrier: &Arc<Barrier>) {
    let barrier = barrier.clone();
    tokio::task::spawn_blocking(move || barrier.wait())
        .await
        .unwrap();
}

fn spawn(host: &Arc<HeadlessHost>, call: Call) -> JoinHandle<Result<ToolResult, OpError>> {
    let host = host.clone();
    tokio::spawn(async move { host.call(call).await })
}

/// Both calls' results, once each has parked before its effect.
async fn race(host: &Arc<HeadlessHost>, first: Value, second: Value) -> [ToolResult; 2] {
    host.exec().set_hooks(Hooks {
        before_effect: Some(Arc::new(Barrier::new(2))),
        ..Hooks::default()
    });
    let calls = [spawn(host, call(first)), spawn(host, call(second))];
    let mut results = Vec::new();
    for call in calls {
        results.push(bounded(call).await.unwrap().unwrap());
    }
    host.exec().set_hooks(Hooks::default());
    results.try_into().unwrap()
}

/// A [`SessionStore`] that runs `hook` after every read of a document: the
/// insert's source snapshot and the step target, both inside the stage
/// before the effect.
struct Hooked {
    inner: Arc<SessionStore>,
    hook: Box<dyn Fn() + Send + Sync>,
}

impl Documents for Hooked {
    fn create(&self, who: &Principal, doc: Document) -> Result<Created, OpError> {
        self.inner.create(who, doc)
    }

    fn snapshot(
        &self,
        who: &Principal,
        handle: &DocHandle,
        access: Access,
    ) -> Result<Snapshot, OpError> {
        let snapshot = self.inner.snapshot(who, handle, access);
        if access == Access::Read {
            (self.hook)();
        }
        snapshot
    }

    fn step_target(
        &self,
        who: &Principal,
        handle: &DocHandle,
        redo: bool,
    ) -> Result<StepTarget, OpError> {
        let target = self.inner.step_target(who, handle, redo);
        (self.hook)();
        target
    }

    fn receipt(
        &self,
        who: &Principal,
        handle: &DocHandle,
        key: &IdempotencyKey,
    ) -> Result<Option<Arc<ToolResult>>, OpError> {
        self.inner.receipt(who, handle, key)
    }

    fn commit(
        &self,
        who: &Principal,
        handle: &DocHandle,
        change: Change,
    ) -> Result<Arc<ToolResult>, OpError> {
        self.inner.commit(who, handle, change)
    }

    fn close(&self, who: &Principal, handle: &DocHandle) -> Result<(), OpError> {
        self.inner.close(who, handle)
    }

    fn list(&self, who: &Principal) -> Vec<Listed> {
        self.inner.list(who)
    }
}

/// Runs `arguments` as request `id` on the host's executor over a [`Hooked`]
/// view of its store.
fn spawn_hooked(
    host: &HeadlessHost,
    id: RequestId,
    arguments: Value,
    hook: impl Fn() + Send + Sync + 'static,
) -> JoinHandle<Result<ToolResult, OpError>> {
    let decoded = decode(arguments, &Budgets::default()).unwrap();
    let store: Arc<dyn Documents> = Arc::new(Hooked {
        inner: host.store().clone(),
        hook: Box::new(hook),
    });
    let exec = host.exec().clone();
    tokio::spawn(async move {
        exec.run(who(), id, None, move |ctx| {
            apply(ctx, store, who(), Versions::current("9.8.7"), decoded)
        })
        .await
    })
}

#[test]
fn decode_checks_the_rules_between_fields() {
    let budgets = Budgets::default();
    let doc = "doc_1";
    let decoded = |arguments: Value| decode(arguments, &budgets).map_err(|e| e.message);
    let args = |edit: &str, source: Value, ids: Value, base: Value| json!({"document": doc, "edit": edit, "source": source, "ids": ids, "base_revision": base, "idempotency_key": null});
    let null = Value::Null;
    for (arguments, expected) in [
        (
            args("insert", null.clone(), null.clone(), null.clone()),
            NO_SOURCE,
        ),
        (
            args("delete", json!("doc_2"), json!(["1"]), json!("0")),
            DELETE_SOURCE,
        ),
        (
            args("delete", null.clone(), null.clone(), json!("0")),
            NO_IDS,
        ),
        (args("delete", null.clone(), json!([]), json!("0")), NO_IDS),
        (
            args("undo", json!("doc_2"), null.clone(), json!("0")),
            STEP_ARGUMENTS,
        ),
        (
            args("redo", null.clone(), json!([]), json!("0")),
            STEP_ARGUMENTS,
        ),
        (
            args("insert", json!("doc_2"), json!(["1"]), null.clone()),
            NO_BASE,
        ),
        (
            args("delete", null.clone(), json!(["1"]), null.clone()),
            NO_BASE,
        ),
        (
            args("undo", null.clone(), null.clone(), null.clone()),
            NO_BASE,
        ),
        (
            args("redo", null.clone(), null.clone(), null.clone()),
            NO_BASE,
        ),
    ] {
        assert_eq!(
            decoded(arguments.clone()).unwrap_err(),
            expected,
            "{arguments}"
        );
    }
    // An addition needs no base revision; a guarded edit with one decodes.
    for arguments in [
        args("insert", json!("doc_2"), null.clone(), null.clone()),
        args("insert", json!("doc_2"), json!([]), null.clone()),
        args("insert", json!("doc_2"), json!(["1"]), json!("3")),
        args("delete", null.clone(), json!(["1"]), json!("3")),
        args("undo", null.clone(), null.clone(), json!("3")),
        args("redo", null.clone(), null.clone(), json!("0")),
    ] {
        assert!(decoded(arguments.clone()).is_ok(), "{arguments}");
    }
    let long = json!({"document": doc, "edit": "undo", "source": null, "ids": null, "base_revision": "0", "idempotency_key": "é".repeat(65)});
    assert_eq!(
        decoded(long).unwrap_err(),
        "idempotency_key is 130 bytes; at most 128 are allowed"
    );
}

#[test]
fn the_digest_covers_the_canonical_arguments() {
    let budgets = Budgets::default();
    let key = |arguments: Value| decode(arguments, &budgets).unwrap().key;
    let undo = |base: &str| json!({"document": "doc_1", "edit": "undo", "source": null, "ids": null, "base_revision": base, "idempotency_key": "k"});
    let first = key(undo("1")).unwrap();
    assert_eq!(first.key, "k");
    // serde_json sorts object keys, so the order they arrive in is irrelevant.
    let reordered = serde_json::from_str::<Value>(
        r#"{"idempotency_key":"k","base_revision":"1","ids":null,"source":null,"edit":"undo","document":"doc_1"}"#,
    )
    .unwrap();
    assert_eq!(key(reordered), Some(first.clone()));
    assert_ne!(key(undo("2")).unwrap().digest, first.digest);
    let mut unkeyed = undo("1");
    unkeyed["idempotency_key"] = Value::Null;
    assert_eq!(key(unkeyed), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_insert_then_an_undo_restores_the_exact_previous_document() {
    let host = host();
    let base = labeled(chain(&["C", "C"], 0.));
    let document = create(&host, base.clone());
    let source = create(&host, chain(&["C", "O"], 0.));
    let applied = apply_ok(&host, insert(&document, &source, None, None, None)).await;
    let versions = versions_json(&Versions::current("9.8.7"));
    assert_eq!(
        applied,
        json!({
            "value": {
                "effect": "applied",
                "document": document.as_str(),
                "revision": "1",
                "recorded": true,
                "inserted": ["3", "4"],
                "id_remap": [{"source": "1", "inserted": "3"}, {"source": "2", "inserted": "4"}],
                "deleted": [],
            },
            "warnings": [],
            "validation": {"status": "valid"},
            "versions": versions,
        })
    );
    let inserted = stored(&host, &document);
    assert_eq!(inserted.atoms.len(), 4);
    // The source is unchanged.
    assert_eq!(*stored(&host, &source), chain(&["C", "O"], 0.));

    let undone = apply_ok(&host, step(&document, "undo", 1, None)).await;
    assert_eq!(undone["value"]["revision"], "2");
    assert_eq!(undone["value"]["recorded"], true);
    assert_eq!(undone["value"]["inserted"], json!([]));
    assert_eq!(undone["value"]["deleted"], json!([]));
    assert_eq!(*stored(&host, &document), base);
    // The insert was one history step.
    let nothing = apply_ok(&host, step(&document, "undo", 2, None)).await;
    assert_eq!(nothing["value"]["recorded"], false);
    assert_eq!(nothing["value"]["revision"], "2");
    assert_eq!(revision(&host, &document), 2);

    let redone = apply_ok(&host, step(&document, "redo", 2, None)).await;
    assert_eq!(redone["value"]["revision"], "3");
    assert_eq!(stored(&host, &document), inserted);
    let nothing = apply_ok(&host, step(&document, "redo", 3, None)).await;
    assert_eq!(nothing["value"]["recorded"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_insert_refreshes_the_computed_labels() {
    let host = host();
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C", "O"], 0.));
    assert_eq!(stored(&host, &source).atoms[1].label_h, 0);
    apply_ok(&host, insert(&document, &source, None, None, None)).await;
    let doc = stored(&host, &document);
    assert_eq!(doc.atoms[1].element, "O");
    assert_eq!(doc.atoms[1].label_h, 1);
    assert_eq!(*doc, labeled((*doc).clone()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_base_revision_changes_nothing() {
    let host = host();
    let document = create(&host, chain(&["C", "C", "C"], 0.));
    let source = create(&host, chain(&["N"], 0.));
    apply_ok(&host, delete(&document, &[3], 0, None)).await;
    let before = stored(&host, &document);
    for arguments in [
        insert(&document, &source, Some(&[1]), Some(0), None),
        delete(&document, &[1], 0, Some("k")),
        step(&document, "undo", 0, None),
        step(&document, "redo", 2, None),
    ] {
        let (code, message) = apply_err(&host, arguments).await;
        assert_eq!(code, "stale");
        assert!(
            message.contains("is not the current revision 1"),
            "{message}"
        );
    }
    assert_eq!(revision(&host, &document), 1);
    assert_eq!(stored(&host, &document), before);
    // The stale keyed delete stored no receipt.
    apply_ok(&host, delete(&document, &[1], 1, Some("k"))).await;
    assert_eq!(revision(&host, &document), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ids_and_handles_are_checked_before_any_change() {
    let host = host();
    let document = create(&host, chain(&["C", "C"], 0.));
    let source = create(&host, chain(&["N"], 0.));
    let empty = create(&host, Document::default());
    let foreign = host
        .store()
        .create(&Principal::new("other"), chain(&["O"], 0.))
        .unwrap()
        .handle;
    let (code, message) = apply_err(&host, delete(&document, &[2, 9], 0, None)).await;
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("unknown_object", "Object 9 is not in the drawing")
    );
    let (code, _) = apply_err(&host, insert(&document, &source, Some(&[7]), Some(0), None)).await;
    assert_eq!(code, "unknown_object");
    for (target, source) in [(&document, &foreign), (&foreign, &source)] {
        let (code, message) = apply_err(&host, insert(target, source, None, None, None)).await;
        assert_eq!(
            (code.as_str(), message.as_str()),
            ("unknown_document", UNKNOWN_DOCUMENT)
        );
    }
    let (code, message) = apply_err(&host, insert(&document, &empty, None, None, None)).await;
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("invalid_arguments", "There are no drawing objects to apply")
    );
    assert_eq!(revision(&host, &document), 0);
    assert_eq!(*stored(&host, &document), chain(&["C", "C"], 0.));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deleting_an_abbreviation_member_deletes_the_whole_abbreviation() {
    let host = host();
    // C-O-C with O-C contracted to OMe; the last carbon is the hidden member.
    let mut doc = chain(&["C", "O", "C"], 0.);
    doc.contract(&[2, 3], "OMe", "MeO").unwrap();
    assert_eq!(doc.abbreviations[0].members, [2, 3]);
    let document = create(&host, doc);
    let deleted = apply_ok(&host, delete(&document, &[3], 0, None)).await;
    assert_eq!(deleted["value"]["deleted"], json!(["2", "3"]));
    assert_eq!(deleted["value"]["inserted"], json!([]));
    let doc = stored(&host, &document);
    assert_eq!(doc.all_ids(), [1]);
    assert!(doc.abbreviations.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replacing_the_only_or_the_largest_object_reports_the_reused_ids_as_deleted() {
    let host = host();
    let source = create(&host, chain(&["C", "O"], 0.));
    // The only object: the insert starts again from ID 1.
    let only = create(&host, chain(&["N"], 0.));
    let applied = apply_ok(&host, insert(&only, &source, Some(&[1]), Some(0), None)).await;
    assert_eq!(applied["value"]["deleted"], json!(["1"]));
    assert_eq!(applied["value"]["inserted"], json!(["1", "2"]));
    let doc = stored(&host, &only);
    assert_eq!(doc.all_ids(), [1, 2]);
    assert_eq!(doc.atoms[0].element, "C");
    // The largest ID: the insert reuses it.
    let largest = create(&host, chain(&["N", "N", "N"], 0.));
    let applied = apply_ok(&host, insert(&largest, &source, Some(&[3]), Some(0), None)).await;
    assert_eq!(applied["value"]["deleted"], json!(["3"]));
    assert_eq!(applied["value"]["inserted"], json!(["3", "4"]));
    assert_eq!(
        applied["value"]["id_remap"],
        json!([{"source": "1", "inserted": "3"}, {"source": "2", "inserted": "4"}])
    );
    let doc = stored(&host, &largest);
    assert_eq!(doc.all_ids(), [1, 2, 3, 4]);
    assert_eq!(doc.atom(3).unwrap().element, "C");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_key_and_arguments_apply_once() {
    let host = host();
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C", "O"], 0.));
    let arguments = insert(&document, &source, None, None, Some("once"));
    let first = apply_ok(&host, arguments.clone()).await;
    let again = apply_ok(&host, arguments).await;
    assert_eq!(first, again);
    assert_eq!(revision(&host, &document), 1);
    assert_eq!(stored(&host, &document).atoms.len(), 2);
    assert_eq!(undo_all(&host, &document).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_applies_with_one_key_commit_once_and_return_equal_results() {
    let host = Arc::new(host());
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C", "O"], 0.));
    let arguments = insert(&document, &source, None, None, Some("once"));
    let [first, second] = race(&host, arguments.clone(), arguments).await;
    assert!(!first.is_error, "{:?}", first.value);
    assert_eq!(first, second);
    assert_eq!(first.value["value"]["revision"], "1");
    assert_eq!(revision(&host, &document), 1);
    assert_eq!(undo_all(&host, &document).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_key_reused_with_other_arguments_is_invalid_and_changes_nothing() {
    const KEY_REUSED: &str = "idempotency_key was already used with different arguments";
    let host = Arc::new(host());
    let carbon = create(&host, chain(&["C"], 0.));
    let nitrogen = create(&host, chain(&["N"], 0.));
    // In sequence.
    let document = create(&host, Document::default());
    apply_ok(&host, insert(&document, &carbon, None, None, Some("k"))).await;
    let (code, message) =
        apply_err(&host, insert(&document, &nitrogen, None, None, Some("k"))).await;
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("invalid_arguments", KEY_REUSED)
    );
    assert_eq!(revision(&host, &document), 1);
    assert_eq!(stored(&host, &document).atoms[0].element, "C");
    // At once: whichever commits second finds the first's receipt.
    let document = create(&host, Document::default());
    let results = race(
        &host,
        insert(&document, &carbon, None, None, Some("k")),
        insert(&document, &nitrogen, None, None, Some("k")),
    )
    .await;
    let failed: Vec<_> = results.iter().filter(|result| result.is_error).collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(
        error(failed[0]),
        ("invalid_arguments".into(), KEY_REUSED.into())
    );
    assert_eq!(revision(&host, &document), 1);
    assert_eq!(stored(&host, &document).atoms.len(), 1);
    assert_eq!(undo_all(&host, &document).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_unkeyed_inserts_from_one_base_give_exactly_one_stale() {
    let host = Arc::new(host());
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C"], 0.));
    let arguments = insert(&document, &source, None, None, None);
    let results = race(&host, arguments.clone(), arguments).await;
    let codes: Vec<_> = results
        .iter()
        .filter(|result| result.is_error)
        .map(|result| error(result).0)
        .collect();
    assert_eq!(codes, ["stale"]);
    assert_eq!(revision(&host, &document), 1);
    assert_eq!(stored(&host, &document).atoms.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_guarded_undos_from_one_base_give_exactly_one_stale() {
    let host = Arc::new(host());
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C"], 0.));
    for _ in 0..2 {
        apply_ok(&host, insert(&document, &source, None, None, None)).await;
    }
    let undo = step(&document, "undo", 2, None);
    let results = race(&host, undo.clone(), undo).await;
    let codes: Vec<_> = results
        .iter()
        .filter(|result| result.is_error)
        .map(|result| error(result).0)
        .collect();
    assert_eq!(codes, ["stale"]);
    assert_eq!(revision(&host, &document), 3);
    assert_eq!(stored(&host, &document).atoms.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_redo_overtaken_by_an_insert_is_stale() {
    let host = host();
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C"], 0.));
    apply_ok(&host, insert(&document, &source, None, None, None)).await;
    apply_ok(&host, step(&document, "undo", 1, None)).await;
    // The redo reads its target, then waits while the insert commits.
    let (read, resume) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
    let redo = spawn_hooked(&host, request(), step(&document, "redo", 2, None), {
        let (read, resume) = (read.clone(), resume.clone());
        move || {
            read.wait();
            resume.wait();
        }
    });
    wait(&read).await;
    apply_ok(&host, insert(&document, &source, None, None, None)).await;
    wait(&resume).await;
    let error = bounded(redo).await.unwrap().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Stale);
    assert_eq!(revision(&host, &document), 3);
    assert_eq!(stored(&host, &document).atoms.len(), 1);
}

/// A ring, an attachment point over it bonded to iron, and a reaction with
/// the ring as reactant and the point and iron as product. Validation passes,
/// since no bond crosses a participant, but reconciliation follows the point
/// to its ring and finds the two participants joined.
fn latent_join() -> Document {
    let mut doc = Document::default();
    let ring = [
        doc.add_atom("C", Point::new(0., 0.)),
        doc.add_atom("C", Point::new(30., 0.)),
        doc.add_atom("C", Point::new(15., 26.)),
    ];
    doc.add_bond(ring[0], ring[1], 1, "plain");
    doc.add_bond(ring[1], ring[2], 1, "plain");
    doc.add_bond(ring[2], ring[0], 1, "plain");
    let point = attachments::add(&mut doc, &ring, attachments::Kind::MultiCenter).unwrap();
    let iron = doc.add_atom("Fe", Point::new(15., 60.));
    doc.add_bond(point, iron, 1, "plain");
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(100., 0.),
        Point::new(240., 0.),
        Default::default(),
        Default::default(),
    ));
    let participant = |atoms: Vec<u64>| Participant {
        atoms,
        coefficient: 1,
    };
    doc.reactions.push(Reaction {
        arrow,
        reactants: vec![participant(ring.to_vec())],
        products: vec![participant(vec![point, iron])],
        agents: Vec::new(),
        annotations: Vec::new(),
    });
    doc.validate().unwrap();
    doc
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reaction_joining_edit_is_rejected_and_changes_nothing() {
    let host = host();
    let base = latent_join();
    let document = create(&host, base.clone());
    let source = create(&host, chain(&["C"], 0.));
    let result = host
        .call(call(insert(&document, &source, None, None, Some("k"))))
        .await
        .unwrap();
    assert_eq!(error(&result), ("rejected".into(), JOINS.into()));
    assert_eq!(
        result.value["validation"],
        json!({"status": "rejected", "reason": "reactions", "message": JOINS})
    );
    assert_eq!(result.value["value"], Value::Null);
    assert_eq!(revision(&host, &document), 0);
    assert_eq!(*stored(&host, &document), base);
    // A rejection stores no receipt: the same key runs (and fails) again.
    let again = host
        .call(call(insert(&document, &source, None, None, Some("k"))))
        .await
        .unwrap();
    assert_eq!(again, result);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_before_the_effect_changes_nothing() {
    let host = host();
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C", "O"], 0.));
    let arguments = insert(&document, &source, None, None, Some("k"));
    let id = request();
    // The cancel lands while the edit and its labels are being prepared.
    let cancelled = spawn_hooked(&host, id.clone(), arguments.clone(), {
        let (exec, id) = (host.exec().clone(), id.clone());
        move || exec.cancel(&who(), &id)
    });
    let error = bounded(cancelled).await.unwrap().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert_eq!(revision(&host, &document), 0);
    assert!(stored(&host, &document).atoms.is_empty());
    // No receipt was stored, so the retry applies.
    let applied = apply_ok(&host, arguments).await;
    assert_eq!(applied["value"]["revision"], "1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_parked_after_the_effect_keeps_the_change_and_replays_its_receipt() {
    let host = Arc::new(host());
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C", "O"], 0.));
    let arguments = insert(&document, &source, None, None, Some("k"));
    let (before, after) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
    host.exec().set_hooks(Hooks {
        before_effect: Some(before.clone()),
        after_effect: Some(after.clone()),
        ..Hooks::default()
    });
    let id = request();
    let applying = spawn(&host, call_as(id.clone(), arguments.clone()));
    wait(&before).await;
    until(|| revision(&host, &document) == 1).await;
    host.cancel(&who(), &id);
    wait(&after).await;
    let error = bounded(applying).await.unwrap().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Cancelled);
    host.exec().set_hooks(Hooks::default());
    // The change stands, and the same key replays it.
    assert_eq!(stored(&host, &document).atoms.len(), 2);
    let replayed = apply_ok(&host, arguments).await;
    assert_eq!(replayed["value"]["revision"], "1");
    assert_eq!(replayed["value"]["recorded"], true);
    assert_eq!(replayed["value"]["inserted"], json!(["1", "2"]));
    assert_eq!(revision(&host, &document), 1);
    assert_eq!(undo_all(&host, &document).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deadline_that_passes_after_the_effect_still_returns_the_result() {
    // Long enough for the stage before the effect, which takes milliseconds.
    let deadline = Duration::from_secs(2);
    let host = Arc::new(HeadlessHost::new(
        "9.8.7",
        Budgets {
            op_deadline: deadline,
            ..Budgets::default()
        },
    ));
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C"], 0.));
    let (before, after) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
    host.exec().set_hooks(Hooks {
        before_effect: Some(before.clone()),
        after_effect: Some(after.clone()),
        ..Hooks::default()
    });
    let applying = spawn(&host, call(insert(&document, &source, None, None, None)));
    wait(&before).await;
    // The call was admitted before it reached the effect, so its deadline
    // has passed once this wait ends; it is parked after the effect by then.
    let committed = Instant::now();
    tokio::time::sleep(deadline).await;
    assert!(committed.elapsed() >= deadline);
    wait(&after).await;
    let result = bounded(applying).await.unwrap().unwrap();
    assert!(!result.is_error, "{:?}", result.value);
    assert_eq!(result.value["value"]["revision"], "1");
    assert_eq!(revision(&host, &document), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_history_keeps_history_depth_undo_steps() {
    let host = HeadlessHost::new(
        "9.8.7",
        Budgets {
            history_depth: 2,
            ..Budgets::default()
        },
    );
    let document = create(&host, Document::default());
    let source = create(&host, chain(&["C"], 0.));
    for _ in 0..4 {
        apply_ok(&host, insert(&document, &source, None, None, None)).await;
    }
    assert_eq!(undo_all(&host, &document).await, 2);
    assert_eq!(stored(&host, &document).atoms.len(), 2);
}
