use super::*;

#[test]
fn recent_files_keep_five_canonical_paths_and_reopen_in_use_order() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("recent-files.json");
    let mut recent = Recent::load_from(store.clone());
    let mut files = Vec::new();
    for index in 0..7 {
        let path = directory.path().join(format!("drawing-{index}.rsk"));
        std::fs::write(&path, b"{}").unwrap();
        recent.remember(&path);
        files.push(path.canonicalize().unwrap());
    }
    recent.remember(&files[3]);
    assert_eq!(recent.entries.len(), 5);
    assert_eq!(recent.entries[0].path, files[3]);
    assert!(recent.entries.iter().all(|entry| entry.path.is_absolute()));
    let reopened = Recent::load_from(store);
    assert_eq!(
        reopened
            .entries
            .iter()
            .map(|entry| &entry.path)
            .collect::<Vec<_>>(),
        recent
            .entries
            .iter()
            .map(|entry| &entry.path)
            .collect::<Vec<_>>()
    );
    assert!(!reopened.contains(&files[0]));
}

#[test]
fn recent_loading_rejects_newer_records_relative_names_and_oversized_data() {
    let directory = tempfile::tempdir().unwrap();
    let store = directory.path().join("recent-files.json");
    for (bytes, preserve) in [
        (
            br#"{"version":2,"paths":["/tmp/drawing.rsk"]}"#.to_vec(),
            true,
        ),
        (br#"{"version":1,"paths":["drawing.rsk"]}"#.to_vec(), false),
        (vec![b' '; STORE_LIMIT as usize + 1], true),
    ] {
        std::fs::write(&store, &bytes).unwrap();
        let mut recent = Recent::load_from(store.clone());
        assert!(recent.entries.is_empty());
        let drawing = directory.path().join("valid.rsk");
        std::fs::write(&drawing, b"{}").unwrap();
        recent.remember(&drawing);
        if preserve {
            assert!(
                std::fs::read(&store).unwrap() == bytes,
                "Unsupported records stay untouched"
            );
        } else {
            assert_eq!(
                Recent::load_from(store.clone()).entries[0].path,
                drawing.canonicalize().unwrap()
            );
        }
    }
}

#[test]
fn a_late_preview_cannot_replace_a_newer_preview_for_the_same_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("drawing.rsk");
    std::fs::write(&path, b"{}").unwrap();
    let mut recent = Recent::default();
    recent.remember(&path);
    let older = recent.entries[0].generation;
    recent.remember(&path);
    let newer = recent.entries[0].generation;
    let preview = image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]);
    recent.previewed(&path.canonicalize().unwrap(), older, Some(preview.clone()));
    assert!(recent.entries[0].preview.is_none());
    recent.previewed(&path.canonicalize().unwrap(), newer, Some(preview));
    assert!(recent.entries[0].preview.is_some());
}

#[tokio::test]
async fn recent_thumbnail_is_a_small_rendered_preview_and_refuses_large_sources() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("ethanol.rsk");
    std::fs::write(
        &file,
        include_bytes!("../../../../tests/fixtures/chemical-naming-rust/ethanol-rust-native.rsk"),
    )
    .unwrap();
    let engine = LocalEngine::in_process();
    assert!(preview(engine.clone(), file.clone()).await.is_some());
    std::fs::write(&file, vec![b' '; PREVIEW_LIMIT as usize + 1]).unwrap();
    assert!(preview(engine, file).await.is_none());
}
