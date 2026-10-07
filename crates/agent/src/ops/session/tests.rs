use super::*;
use crate::{
    document::{Annotation, Atom, Point},
    ops::store::Receipt,
    pictures::Picture,
    transaction,
};
use serde_json::{Map, Value, json};
use std::{cell::Cell, sync::Barrier, thread, time::Duration};

thread_local! {
    /// This thread's fake time, set on first use.
    static CLOCK: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The fake clock of stores built by [`fake_clock_store`].
fn clock() -> Instant {
    CLOCK.with(|clock| {
        let now = clock.get().unwrap_or_else(Instant::now);
        clock.set(Some(now));
        now
    })
}

fn advance(by: Duration) {
    let now = clock() + by;
    CLOCK.with(|clock| clock.set(Some(now)));
}

fn fake_clock_store(budgets: Budgets) -> SessionStore {
    SessionStore::with_clock(budgets, clock)
}

fn store() -> SessionStore {
    SessionStore::new(Budgets::default())
}

fn alice() -> Principal {
    Principal::local()
}

fn bob() -> Principal {
    Principal::new("bob")
}

fn key(key: &str, digest: u64) -> Option<IdempotencyKey> {
    Some(IdempotencyKey {
        key: key.into(),
        digest,
    })
}

/// `n` unbonded carbon atoms with IDs 1 to `n`.
fn carbons(n: u64) -> Vec<Atom> {
    (1..=n)
        .map(|id| {
            serde_json::from_value(json!({
                "id": id,
                "element": "C",
                "position": {"x": id as f64 * 20.0, "y": 0.0},
            }))
            .unwrap()
        })
        .collect()
}

fn with_carbons(n: u64) -> Document {
    Document {
        atoms: carbons(n),
        ..Document::default()
    }
}

/// A 2 × 1 RGBA PNG; each call returns a picture with its own storage.
fn picture() -> Picture {
    serde_json::from_value(json!("iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8AAQv8BD/kD/YURmXYAAAAASUVORK5CYII=")).unwrap()
}

fn with_picture(picture: &Picture) -> Document {
    let mut doc = Document::default();
    doc.graphics.push(picture.graphic(1, Point::new(0., 0.)));
    doc
}

/// `{revision, recorded, chemistry_changed, label_notice}`.
fn receipt() -> Receipt {
    Box::new(|applied: &Applied| ToolResult {
        value: Map::from_iter([
            ("revision".to_owned(), json!(applied.revision.to_string())),
            ("recorded".to_owned(), json!(applied.recorded)),
            (
                "chemistry_changed".to_owned(),
                json!(applied.chemistry_changed),
            ),
            ("label_notice".to_owned(), json!(applied.label_notice)),
        ]),
        images: Vec::new(),
        files: Vec::new(),
        is_error: false,
    })
}

/// A reconciled replacement of the current document, edited by `edit`.
fn replace(
    store: &SessionStore,
    handle: &DocHandle,
    edit: impl FnOnce(&mut Document),
    key: Option<IdempotencyKey>,
) -> Change {
    let snapshot = store.snapshot(&alice(), handle, Access::Edit).unwrap();
    let before = (*snapshot.doc).clone();
    let mut doc = before.clone();
    edit(&mut doc);
    let reconciled = transaction::reconcile(&mut doc, before).unwrap();
    Change {
        base: snapshot.revision,
        edit: Edit::Replace { doc, reconciled },
        key,
        labels: None,
        receipt: receipt(),
    }
}

/// A replacement whose document holds `n` carbons.
fn carbons_change(
    store: &SessionStore,
    handle: &DocHandle,
    n: u64,
    key: Option<IdempotencyKey>,
) -> Change {
    replace(store, handle, |doc| doc.atoms = carbons(n), key)
}

fn step(base: u64, redo: bool, key: Option<IdempotencyKey>) -> Change {
    Change {
        base: Revision(base),
        edit: Edit::Step {
            redo,
            chemistry_changed: true,
        },
        key,
        labels: None,
        receipt: receipt(),
    }
}

fn create(store: &SessionStore, who: &Principal) -> DocHandle {
    store.create(who, Document::default()).unwrap().handle
}

fn listed(store: &SessionStore, who: &Principal) -> Vec<DocHandle> {
    store
        .list(who)
        .into_iter()
        .map(|listed| listed.handle)
        .collect()
}

/// Reads the entry behind `handle`.
fn entry<R>(store: &SessionStore, handle: &DocHandle, read: impl FnOnce(&Entry) -> R) -> R {
    read(store.lock().entries.get(handle).unwrap())
}

/// Undo and redo frames.
fn frames(store: &SessionStore, handle: &DocHandle) -> (usize, usize) {
    entry(store, handle, |entry| {
        (
            entry.history.undo_frames().len(),
            entry.history.redo_frames().len(),
        )
    })
}

fn weight(store: &SessionStore, handle: &DocHandle) -> u64 {
    entry(store, handle, |entry| entry.weight)
}

/// The weight counted from scratch.
fn recount(store: &SessionStore, handle: &DocHandle) -> u64 {
    entry(store, handle, |entry| {
        cost(&entry.doc)
            + entry.history.undo_frames().map(cost).sum::<u64>()
            + entry.history.redo_frames().map(cost).sum::<u64>()
    })
}

fn revision(store: &SessionStore, handle: &DocHandle) -> Revision {
    store
        .snapshot(&alice(), handle, Access::Read)
        .unwrap()
        .revision
}

#[test]
fn another_principals_handle_is_an_unknown_document() {
    let store = store();
    let handle = create(&store, &alice());
    let unknown = unknown();
    assert_eq!(unknown.message, UNKNOWN_DOCUMENT);
    let attempts = [
        store.snapshot(&bob(), &handle, Access::Read).map(drop),
        store.snapshot(&bob(), &handle, Access::Edit).map(drop),
        store.step_target(&bob(), &handle, false).map(drop),
        store
            .receipt(&bob(), &handle, &key("k", 1).unwrap())
            .map(drop),
        store
            .commit(&bob(), &handle, step(0, false, None))
            .map(drop),
        store.close(&bob(), &handle),
    ];
    for attempt in attempts {
        assert_eq!(attempt, Err(unknown.clone()));
    }
    assert!(store.list(&bob()).is_empty());
    let missing = DocHandle::new("doc_0").unwrap();
    assert_eq!(
        store
            .snapshot(&alice(), &missing, Access::Read)
            .unwrap_err(),
        unknown
    );
    assert_eq!(listed(&store, &alice()), std::slice::from_ref(&handle));
    let snapshot = store.snapshot(&alice(), &handle, Access::Edit).unwrap();
    assert_eq!(snapshot.kind, HandleKind::Session);
    assert_eq!(snapshot.revision, Revision(0));
    assert_eq!(*snapshot.doc, Document::default());
}

#[test]
fn idle_documents_expire_and_only_the_owner_keeps_them_alive() {
    let ttl = Budgets::default().idle_ttl;
    let store = fake_clock_store(Budgets::default());
    let kept = create(&store, &alice());
    let idle = create(&store, &alice());
    advance(ttl);
    // Idle for exactly idle_ttl: both are still live.
    store.snapshot(&alice(), &kept, Access::Read).unwrap();
    advance(Duration::from_nanos(1));
    assert_eq!(
        store.snapshot(&alice(), &idle, Access::Read).unwrap_err(),
        unknown()
    );
    assert_eq!(listed(&store, &alice()), std::slice::from_ref(&kept));
    // Another principal's attempts do not count as use.
    advance(ttl - Duration::from_nanos(1));
    assert!(store.snapshot(&bob(), &kept, Access::Read).is_err());
    advance(Duration::from_nanos(1));
    assert_eq!(
        store.close(&alice(), &kept),
        Err(unknown()),
        "expired after idle_ttl without its owner"
    );
    let inner = store.lock();
    assert!(inner.entries.is_empty() && inner.order.is_empty());
}

#[test]
fn each_principal_has_a_document_cap_and_a_creation_ordered_list() {
    let store = SessionStore::new(Budgets {
        max_documents: 3,
        ..Budgets::default()
    });
    let first: Vec<_> = (0..3).map(|_| create(&store, &alice())).collect();
    let error = store.create(&alice(), Document::default()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    assert_eq!(
        error.message,
        "At most 3 session documents can be open; close one with document_close"
    );
    let theirs = create(&store, &bob());
    store.close(&alice(), &first[1]).unwrap();
    let fourth = create(&store, &alice());
    assert_eq!(
        listed(&store, &alice()),
        [first[0].clone(), first[2].clone(), fourth.clone()]
    );
    assert_eq!(listed(&store, &bob()), [theirs]);
    let counts: Vec<_> = store
        .list(&alice())
        .into_iter()
        .map(|listed| (listed.revision, listed.objects))
        .collect();
    assert_eq!(counts, [(Revision(0), 0); 3]);
}

#[test]
fn create_checks_the_object_limit() {
    let store = SessionStore::new(Budgets {
        max_objects: 2,
        ..Budgets::default()
    });
    let created = store.create(&alice(), with_carbons(2)).unwrap();
    assert_eq!(created.revision, Revision(0));
    assert_eq!(store.list(&alice())[0].objects, 2);
    let error = store.create(&alice(), with_carbons(3)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    assert_eq!(error.message, "The drawing has 3 objects; the limit is 2");
}

#[test]
fn handles_are_doc_and_32_lowercase_hex_digits() {
    let store = store();
    let handles: Vec<_> = (0..16).map(|_| create(&store, &alice())).collect();
    for handle in &handles {
        let hex = handle.as_str().strip_prefix("doc_").unwrap();
        assert_eq!(hex.len(), 32, "{handle}");
        assert!(
            hex.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "{handle}"
        );
    }
    let unique: std::collections::HashSet<_> = handles.iter().collect();
    assert_eq!(unique.len(), handles.len());
    // Another store has its own keys.
    assert_ne!(
        create(&SessionStore::new(Budgets::default()), &alice()),
        handles[0]
    );
}

#[test]
fn deleting_everything_keeps_the_old_drawing_in_the_weight() {
    let store = store();
    let handle = create(&store, &alice());
    store
        .commit(
            &alice(),
            &handle,
            carbons_change(&store, &handle, 1000, None),
        )
        .unwrap();
    let old = store.snapshot(&alice(), &handle, Access::Read).unwrap().doc;
    assert_eq!(cost(&old), 1001);
    store
        .commit(
            &alice(),
            &handle,
            replace(&store, &handle, |doc| doc.atoms.clear(), None),
        )
        .unwrap();
    assert_eq!(
        objects(&store.snapshot(&alice(), &handle, Access::Read).unwrap().doc),
        0
    );
    assert!(weight(&store, &handle) >= cost(&old));
    assert_eq!(weight(&store, &handle), 1 + 1001 + 1);
    assert_eq!(weight(&store, &handle), recount(&store, &handle));
}

#[test]
fn repeated_replacements_keep_history_depth_frames_and_an_exact_weight() {
    let store = SessionStore::new(Budgets {
        history_depth: 3,
        ..Budgets::default()
    });
    let handle = create(&store, &alice());
    assert_eq!(weight(&store, &handle), 1);
    for n in 1..=6 {
        store
            .commit(&alice(), &handle, carbons_change(&store, &handle, n, None))
            .unwrap();
        let (undo, redo) = frames(&store, &handle);
        assert_eq!((undo, redo), (n.min(3) as usize, 0));
        assert_eq!(weight(&store, &handle), recount(&store, &handle));
    }
    // The frames hold 3, 4 and 5 carbons around the 6-carbon document.
    assert_eq!(weight(&store, &handle), 7 + 4 + 5 + 6);
    store
        .commit(&alice(), &handle, step(6, false, None))
        .unwrap();
    assert_eq!(frames(&store, &handle), (2, 1));
    assert_eq!(weight(&store, &handle), recount(&store, &handle));
    assert_eq!(weight(&store, &handle), 7 + 4 + 5 + 6);
}

#[test]
fn a_shared_picture_counts_once_and_until_its_last_holder_closes() {
    let shared = picture();
    let size = shared.png().len() as u64;
    let store = SessionStore::new(Budgets {
        max_session_picture_bytes: size,
        ..Budgets::default()
    });
    let first = store
        .create(&alice(), with_picture(&shared))
        .unwrap()
        .handle;
    // A clone shares the storage, so it adds nothing.
    let second = store
        .create(&alice(), with_picture(&shared.clone()))
        .unwrap()
        .handle;
    let separate = picture();
    assert_eq!(separate.png().len() as u64, size);
    store.close(&alice(), &first).unwrap();
    // The second document still holds the shared picture.
    let error = store.create(&alice(), with_picture(&separate)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Budget);
    assert_eq!(
        error.message,
        format!(
            "Embedded pictures would exceed the session limit of {size} bytes; close documents you no longer need"
        )
    );
    // Every principal has its own budget.
    store.create(&bob(), with_picture(&separate)).unwrap();
    store.close(&alice(), &second).unwrap();
    store.create(&alice(), with_picture(&separate)).unwrap();
}

#[test]
fn a_rejected_preflight_changes_nothing() {
    let shared = picture();
    let pictures = shared.png().len() as u64 - 1;
    let store = SessionStore::new(Budgets {
        max_objects: 10,
        max_session_weight: 20,
        max_session_picture_bytes: pictures,
        ..Budgets::default()
    });
    let handle = create(&store, &alice());
    store
        .commit(&alice(), &handle, carbons_change(&store, &handle, 2, None))
        .unwrap();
    store
        .commit(&alice(), &handle, carbons_change(&store, &handle, 4, None))
        .unwrap();
    store
        .commit(&alice(), &handle, step(2, false, None))
        .unwrap();
    // A second document weighs 9, so the session weighs 3 + 1 + 5 + 9 = 18.
    let other = store.create(&alice(), with_carbons(8)).unwrap();
    assert_eq!(weight(&store, &other.handle), 9);
    let before = store.snapshot(&alice(), &handle, Access::Read).unwrap();
    let state = || {
        (
            revision(&store, &handle),
            frames(&store, &handle),
            weight(&store, &handle),
        )
    };
    let unchanged = (Revision(3), (1, 1), 9);
    assert_eq!(state(), unchanged);
    let rejections = [
        // 11 objects.
        (
            carbons_change(&store, &handle, 11, None),
            "The drawing has 11 objects; the limit is 10".to_owned(),
        ),
        // 10 objects: 11 + 3 + 1 + 9 = 24.
        (
            carbons_change(&store, &handle, 10, None),
            "Session documents and their undo history would exceed the session weight limit of 20; close documents you no longer need".to_owned(),
        ),
        (
            replace(
                &store,
                &handle,
                |doc| doc.graphics.push(shared.graphic(9, Point::new(0., 0.))),
                None,
            ),
            format!("Embedded pictures would exceed the session limit of {pictures} bytes; close documents you no longer need"),
        ),
    ];
    for (change, message) in rejections {
        let error = store.commit(&alice(), &handle, change).unwrap_err();
        assert_eq!((error.kind, error.message), (ErrorKind::Budget, message));
        let after = store.snapshot(&alice(), &handle, Access::Read).unwrap();
        assert!(Arc::ptr_eq(&before.doc, &after.doc));
        assert_eq!(state(), unchanged);
    }
    // The redo frame survived.
    store
        .commit(&alice(), &handle, step(3, true, None))
        .unwrap();
    assert_eq!(
        objects(&store.snapshot(&alice(), &handle, Access::Read).unwrap().doc),
        4
    );
}

#[test]
fn concurrent_replacements_from_one_base_give_exactly_one_stale() {
    let store = Arc::new(store());
    let handle = create(&store, &alice());
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (1..=2)
        .map(|n| {
            let (store, handle, barrier) = (store.clone(), handle.clone(), barrier.clone());
            thread::spawn(move || {
                let change = carbons_change(&store, &handle, n, None);
                barrier.wait();
                store.commit(&alice(), &handle, change)
            })
        })
        .collect();
    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let stale = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .unwrap();
    assert_eq!(stale.kind, ErrorKind::Stale);
    assert_eq!(stale.message, STALE);
    assert_eq!(revision(&store, &handle), Revision(1));
    assert_eq!(frames(&store, &handle), (1, 0));
}

#[test]
fn a_repeated_key_replays_its_receipt_and_a_stale_commit_stores_none() {
    let store = store();
    let handle = create(&store, &alice());
    let mut stale = carbons_change(&store, &handle, 1, key("k", 1));
    stale.base = Revision(7);
    assert_eq!(
        store.commit(&alice(), &handle, stale).unwrap_err().kind,
        ErrorKind::Stale
    );
    assert_eq!(
        store.receipt(&alice(), &handle, &key("k", 1).unwrap()),
        Ok(None)
    );
    let replay = carbons_change(&store, &handle, 2, key("k", 1));
    let first = store
        .commit(
            &alice(),
            &handle,
            carbons_change(&store, &handle, 1, key("k", 1)),
        )
        .unwrap();
    assert_eq!(first.value["revision"], "1");
    // The replay's base is now stale, and its edit differs: neither matters.
    let again = store.commit(&alice(), &handle, replay).unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    let fast = store
        .receipt(&alice(), &handle, &key("k", 1).unwrap())
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&first, &fast));
    assert_eq!(frames(&store, &handle), (1, 0));
    assert_eq!(revision(&store, &handle), Revision(1));
    let reused = OpError::new(ErrorKind::InvalidArguments, KEY_REUSED);
    assert_eq!(
        store
            .commit(
                &alice(),
                &handle,
                carbons_change(&store, &handle, 3, key("k", 2))
            )
            .unwrap_err(),
        reused
    );
    assert_eq!(
        store.receipt(&alice(), &handle, &key("k", 2).unwrap()),
        Err(reused)
    );
    assert_eq!(frames(&store, &handle), (1, 0));
    assert_eq!(revision(&store, &handle), Revision(1));
}

#[test]
fn receipts_keep_only_the_newest_keys() {
    let store = SessionStore::new(Budgets {
        idempotency_receipts: 2,
        ..Budgets::default()
    });
    let handle = create(&store, &alice());
    for (n, name) in (1..).zip(["a", "b", "c"]) {
        store
            .commit(
                &alice(),
                &handle,
                carbons_change(&store, &handle, n, key(name, n)),
            )
            .unwrap();
    }
    let stored = |name: &str, digest| {
        store
            .receipt(&alice(), &handle, &key(name, digest).unwrap())
            .unwrap()
            .is_some()
    };
    assert!(!stored("a", 1));
    assert!(stored("b", 2) && stored("c", 3));
}

#[test]
fn steps_check_the_revision_and_replay_by_key() {
    let store = Arc::new(store());
    let handle = create(&store, &alice());
    for n in 1..=2 {
        store
            .commit(&alice(), &handle, carbons_change(&store, &handle, n, None))
            .unwrap();
    }
    let stale = store
        .commit(&alice(), &handle, step(1, false, None))
        .unwrap_err();
    assert_eq!(
        (stale.kind, stale.message.as_str()),
        (ErrorKind::Stale, STALE)
    );
    assert_eq!(frames(&store, &handle), (2, 0));

    // Two undos from one base: one applies, the other is stale.
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let (store, handle, barrier) = (store.clone(), handle.clone(), barrier.clone());
            thread::spawn(move || {
                barrier.wait();
                store.commit(&alice(), &handle, step(2, false, None))
            })
        })
        .collect();
    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(results.iter().any(|result| {
        result
            .as_ref()
            .is_err_and(|error| error.kind == ErrorKind::Stale)
    }));
    assert_eq!(frames(&store, &handle), (1, 1));
    let target = store.step_target(&alice(), &handle, false).unwrap();
    assert_eq!(target.revision, Revision(3));
    assert_eq!(objects(&target.current), 1);
    assert_eq!(target.target.as_ref().map(objects), Some(0));

    // The same key twice: one undo, then its receipt.
    let first = store
        .commit(&alice(), &handle, step(3, false, key("undo", 9)))
        .unwrap();
    let again = store
        .commit(&alice(), &handle, step(3, false, key("undo", 9)))
        .unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    assert_eq!(first.value["revision"], "4");
    assert_eq!(frames(&store, &handle), (0, 2));
    let snapshot = store.snapshot(&alice(), &handle, Access::Read).unwrap();
    assert_eq!(*snapshot.doc, Document::default());

    // Nothing left to undo: no change and no new revision.
    assert!(
        store
            .step_target(&alice(), &handle, false)
            .unwrap()
            .target
            .is_none()
    );
    let noop = store
        .commit(&alice(), &handle, step(4, false, None))
        .unwrap();
    assert_eq!(
        (
            noop.value["revision"].clone(),
            noop.value["recorded"].clone()
        ),
        (json!("4"), json!(false))
    );
    let redo = store
        .commit(&alice(), &handle, step(4, true, None))
        .unwrap();
    assert_eq!(redo.value["revision"], "5");
    assert_eq!(frames(&store, &handle), (1, 1));
}

#[test]
fn labels_apply_only_when_the_chemistry_changed() {
    let store = store();
    let handle = create(&store, &alice());
    let mut change = carbons_change(&store, &handle, 2, None);
    change.labels = Some(Err("Labels could not be computed".into()));
    let applied = store.commit(&alice(), &handle, change).unwrap();
    assert_eq!(applied.value["chemistry_changed"], true);
    assert_eq!(
        applied.value["label_notice"],
        "Labels could not be computed"
    );

    // An annotation is not chemistry, so the labels are not applied.
    let annotation: Annotation =
        serde_json::from_value(json!({"id": 9, "position": {"x": 0, "y": 0}, "text": "note"}))
            .unwrap();
    let mut change = replace(
        &store,
        &handle,
        |doc| doc.annotations.push(annotation),
        None,
    );
    change.labels = Some(Err("unused".into()));
    let applied = store.commit(&alice(), &handle, change).unwrap();
    assert_eq!(applied.value["recorded"], true);
    assert_eq!(applied.value["chemistry_changed"], false);
    assert_eq!(applied.value["label_notice"], Value::Null);

    // A replacement that changes nothing records nothing.
    let applied = store
        .commit(&alice(), &handle, replace(&store, &handle, |_| {}, None))
        .unwrap();
    assert_eq!(applied.value["recorded"], false);
    assert_eq!(applied.value["revision"], "2");
    assert_eq!(frames(&store, &handle), (2, 0));
}
