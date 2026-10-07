use super::*;
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
    assert_eq!(resaved.version, VERSION);
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
