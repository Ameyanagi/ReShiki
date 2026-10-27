use super::*;

fn abandoned(directory: &Path, name: &str, saved_at: u64) -> Candidate {
    let mut document = Document::default();
    document.add_atom("O", Default::default());
    let snapshot = Snapshot {
        document,
        source: Some(PathBuf::from(name)),
        saved_at,
    };
    let path = directory.join(format!("4294967294-{name}-{saved_at}.json"));
    std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    Candidate { path, snapshot }
}

#[test]
fn discarding_valid_abandoned_drafts_moves_them_out_of_recovery() {
    let directory = tempfile::Builder::new()
        .prefix("reshiki-recovery-discard-test-")
        .tempdir()
        .unwrap();
    let store = Recovery::in_directory(directory.path()).unwrap();
    let first = abandoned(directory.path(), "ethanol.rsk", 100);
    let second = abandoned(directory.path(), "aspirin.rsk", 200);
    let candidates = store.candidates();
    assert_eq!(candidates.len(), 2, "Seeded files were actually scanned");
    for candidate in candidates {
        discard(&candidate).unwrap();
    }
    assert!(!first.path.exists() && !second.path.exists());
    assert!(
        store.candidates().is_empty(),
        "A fresh scan must stay empty"
    );
}

#[test]
fn discard_rechecks_live_owners_and_changed_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(directory.path()).unwrap();
    let mut candidate = abandoned(directory.path(), "drawing.rsk", 100);
    assert_eq!(store.candidates().len(), 1);
    let live = directory
        .path()
        .join(format!("{}-live.json", std::process::id()));
    std::fs::rename(&candidate.path, &live).unwrap();
    candidate.path = live.clone();
    assert!(!candidate.available());
    assert!(store.candidates().is_empty());
    assert!(
        discard(&candidate).is_err(),
        "Even a stale/forged offer is protected"
    );
    assert!(live.exists());
    let candidate = abandoned(directory.path(), "changed.rsk", 100);
    let mut changed = candidate.snapshot.clone();
    changed.document.add_atom("N", Default::default());
    std::fs::write(&candidate.path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(discard(&candidate).is_err());
    assert!(candidate.path.exists());
}

#[cfg(unix)]
#[test]
fn another_running_process_is_never_offered_or_discarded() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let child = Child(
        std::process::Command::new("/bin/sleep")
            .arg("60")
            .spawn()
            .unwrap(),
    );
    let directory = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(directory.path()).unwrap();
    let mut candidate = abandoned(directory.path(), "running.rsk", 100);
    let path = directory
        .path()
        .join(format!("{}-running.json", child.0.id()));
    std::fs::rename(&candidate.path, &path).unwrap();
    candidate.path = path;
    assert!(store.candidates().is_empty());
    assert!(discard(&candidate).is_err());
    assert!(candidate.path.exists());
}

#[cfg(unix)]
#[test]
fn recovery_scan_and_discard_do_not_follow_symbolic_links() {
    let directory = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(directory.path()).unwrap();
    let source = abandoned(directory.path(), "source.rsk", 100);
    let link = directory.path().join("4294967294-link.json");
    std::os::unix::fs::symlink(&source.path, &link).unwrap();
    assert_eq!(store.candidates().len(), 1);
    let candidate = Candidate {
        path: link,
        snapshot: source.snapshot,
    };
    assert!(discard(&candidate).is_err());
    assert!(source.path.exists());
}
#[cfg(windows)]
#[test]
fn windows_does_not_recover_a_live_editors_draft() {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    let mut child = Command::new(
        std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/ping.exe"),
    )
    .args(["-n", "30", "127.0.0.1"])
    .creation_flags(0x08000000)
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(dir.path()).unwrap();
    let mut doc = Document::default();
    doc.add_atom("O", Default::default());
    store.save(&doc, None).unwrap();
    std::fs::rename(
        &store.session,
        dir.path().join(format!("{}-other.json", child.id())),
    )
    .unwrap();
    let while_running = store.candidates();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(while_running.is_empty());
    assert_eq!(store.candidates().len(), 1);
}
#[test]
fn old_dark_drafts_convert_their_colors_once() {
    let bytes = include_bytes!("../../../../tests/fixtures/palette/legacy-dark.rsk");
    let old: Document = serde_json::from_slice(bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(dir.path()).unwrap();
    let draft = dir.path().join("4294967294-old.json");
    let snapshot = Snapshot {
        document: old.clone(),
        source: None,
        saved_at: 1,
    };
    std::fs::write(&draft, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let restored = store.candidates().remove(0).snapshot.document;
    assert_eq!(restored, Document::from_json(bytes).unwrap());
    assert_ne!(restored.bonds, old.bonds);
    // This build's draft is marked current and reads back unchanged.
    store.save(&restored, None).unwrap();
    std::fs::rename(&store.session, &draft).unwrap();
    let resaved = store.candidates().remove(0).snapshot.document;
    assert_eq!(resaved.version, crate::document::DISTANCE_BONDS);
    assert_eq!(resaved, restored.current());
}
#[test]
fn draft_is_durable_and_corrupt_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(dir.path()).unwrap();
    let mut doc = Document::default();
    doc.add_atom("O", Default::default());
    store.save(&doc, None).unwrap();
    std::fs::rename(&store.session, dir.path().join("4294967294-test.json")).unwrap();
    std::fs::write(dir.path().join("broken.json"), b"partial json").unwrap();
    let candidates = store.candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].snapshot.document, doc.current());
    store.save(&candidates[0].snapshot.document, None).unwrap();
    remove(&candidates[0].path).unwrap();
    assert!(store.session.exists());
    store.clear().unwrap();
    assert!(!store.session.exists());
}

#[test]
fn drafts_use_the_required_file_version_after_custom_arrow_gap_returns_to_auto() {
    use crate::{
        arrow_labels::{self, ArrowLabel, Side},
        arrows::{ArrowStyle, Preset},
        document::{ARROW_LABEL_GAPS, Annotation, Arrow, DISTANCE_BONDS, Point},
        typography::TextFormat,
    };

    fn saved(store: &Recovery, doc: &Document, version: u32) -> serde_json::Value {
        store.save(doc, None).unwrap();
        let bytes = std::fs::read(&store.session).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["document"]["version"], version);
        let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(snapshot.document, doc.current());
        raw
    }

    let dir = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(dir.path()).unwrap();
    let mut doc = Document::default();
    doc.add_atom("O", Point::new(-100., 0.));
    saved(&store, &doc, DISTANCE_BONDS);
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::default(),
        Point::new(120., 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    let annotation = doc.next_id();
    doc.annotations.push(Annotation {
        id: annotation,
        position: Point::new(40., -30.),
        text: "heat".into(),
        format: TextFormat::default(),
    });
    doc.arrow_labels.push(ArrowLabel {
        arrow,
        annotation,
        side: Side::Above,
        gap: None,
    });
    // Also exercise an already-current ordinary document's borrowed fast path.
    doc = doc.current();
    for gap in [None, Some(24.), None] {
        let before = doc.clone();
        arrow_labels::set_gap(&mut doc, &[annotation], gap).unwrap();
        let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
        let expected = if gap.is_some() {
            ARROW_LABEL_GAPS
        } else {
            DISTANCE_BONDS
        };
        let raw = saved(&store, &doc, expected);
        if let Some(gap) = gap {
            assert_eq!(raw["document"]["arrow_labels"][0]["gap"], gap);
        } else {
            assert!(raw["document"]["arrow_labels"][0].get("gap").is_none());
        }
        std::fs::rename(&store.session, dir.path().join("abandoned-gap.json")).unwrap();
        let candidates = store.candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].snapshot.document, doc.current());
    }
    assert_eq!(doc.version, ARROW_LABEL_GAPS);
}

#[test]
fn drafts_keep_3d_models() {
    use crate::models3d::{Display, ForceField, Model3d, Placed};
    let dir = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(dir.path()).unwrap();
    let mut doc = Document::default();
    let a = doc.add_atom("C", Default::default());
    let b = doc.add_atom("O", crate::document::Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let placed = vec![
        Placed {
            id: a,
            xyz: [-0.7, 0., 0.],
        },
        Placed {
            id: b,
            xyz: [0.7, 0.1, 0.],
        },
    ];
    doc.models
        .push(Model3d::new(&doc, placed, vec![], ForceField::Uff));
    doc.require_version(crate::document::MODELS_3D);
    crate::models3d::set_display(&mut doc, 0, Display::ThreeD);
    store.save(&doc, None).unwrap();
    std::fs::rename(&store.session, dir.path().join("4294967294-test.json")).unwrap();
    let candidates = store.candidates();
    assert_eq!(candidates[0].snapshot.document, doc.current());
    assert_eq!(candidates[0].snapshot.document.models, doc.models);
}
