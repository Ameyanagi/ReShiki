use super::*;
use crate::equipment::builtin;

/// A lab's copy of a built-in flask.
fn lab(id: &str, revision: u32) -> Definition {
    let mut d = (*builtin::find("reshiki/rbf-100-15").unwrap()).clone();
    d.id = id.into();
    d.revision = revision;
    d
}

fn about() -> About {
    About {
        id: "tanaka-lab".into(),
        names: [("en".to_owned(), "Tanaka lab".to_owned())].into(),
        ..About::default()
    }
}

#[test]
fn identity_survives_a_round_trip_through_a_file() {
    let d = lab("lab/flask", 2);
    let back = read(&piece_bytes(&d)).unwrap();
    assert_eq!(back, vec![d.clone()]);
    assert_eq!(key(&back[0]), key(&d));
    // The hash ignores key order and layout.
    let mut value: serde_json::Value = serde_json::from_slice(&piece_bytes(&d)).unwrap();
    let names = value["names"].take();
    value.as_object_mut().unwrap().insert("names".into(), names);
    let compact = serde_json::to_vec(&value).unwrap();
    assert_eq!(key(&read(&compact).unwrap()[0]), key(&d));
}

#[test]
fn a_collection_reads_back_every_piece() {
    let pieces = [Arc::new(lab("lab/a", 1)), Arc::new(lab("lab/b", 3))];
    let back = read(&collection_bytes(&about(), &pieces)).unwrap();
    assert_eq!(back, vec![lab("lab/a", 1), lab("lab/b", 3)]);
}

#[test]
fn one_bad_piece_refuses_the_whole_collection() {
    let mut bad = lab("lab/b", 1);
    bad.names.clear();
    let pieces = [Arc::new(lab("lab/a", 1)), Arc::new(bad)];
    let error = read(&collection_bytes(&about(), &pieces)).unwrap_err();
    assert!(error.starts_with("Piece 2: "), "{error}");
}

#[test]
fn library_files_cannot_claim_the_built_in_namespace() {
    let d = (*builtin::find("reshiki/stopper-15").unwrap()).clone();
    let error = read(&piece_bytes(&d)).unwrap_err();
    assert!(error.contains("kept for built-in pieces"), "{error}");
}

#[test]
fn size_count_and_version_limits_refuse_the_file() {
    let mut padded = piece_bytes(&lab("lab/a", 1));
    padded.extend(vec![b' '; crate::equipment::validate::MAX_FILE_BYTES]);
    assert_eq!(
        read(&padded).unwrap_err(),
        "The piece file is larger than 1 MB."
    );
    let many = serde_json::json!({
        "format": COLLECTION_FORMAT, "format_version": 0, "library": {"id": "x"},
        "pieces": vec![serde_json::Value::Null; MAX_PIECES + 1],
    });
    let error = read(&serde_json::to_vec(&many).unwrap()).unwrap_err();
    assert!(error.contains("the limit is 2048"), "{error}");
    let newer = serde_json::json!({
        "format": COLLECTION_FORMAT, "format_version": 9, "library": {"id": "x"}, "pieces": [],
    });
    let error = read(&serde_json::to_vec(&newer).unwrap()).unwrap_err();
    assert!(error.contains("newer version"), "{error}");
}

#[test]
fn importing_twice_adds_nothing_the_second_time() {
    let dir = tempfile::tempdir().unwrap();
    let pieces = [lab("lab/a", 1), lab("lab/b", 1)];
    assert_eq!(add(dir.path(), &pieces).unwrap().added.len(), 2);
    assert!(add(dir.path(), &pieces).unwrap().is_empty());
    assert_eq!(scan(dir.path()).pieces.len(), 2);
}

#[test]
fn same_id_and_revision_with_other_content_both_stay() {
    let dir = tempfile::tempdir().unwrap();
    let a = lab("lab/flask", 2);
    let mut b = a.clone();
    b.keywords.push("edited".into());
    add(dir.path(), std::slice::from_ref(&a)).unwrap();
    add(dir.path(), std::slice::from_ref(&b)).unwrap();
    let mut keys: Vec<_> = scan(dir.path()).pieces.iter().map(|d| key(d)).collect();
    keys.sort();
    let mut want = vec![key(&a), key(&b)];
    want.sort();
    assert_eq!(keys, want);
}

#[test]
fn removing_archives_the_file_and_undo_brings_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let d = lab("lab/flask", 1);
    let added = add(dir.path(), &[d.clone(), lab("lab/other", 1)]).unwrap();
    let removed = remove(dir.path(), &key(&d)).unwrap();
    assert_eq!(removed.archived.len(), 1);
    assert!(
        removed.archived[0]
            .1
            .starts_with(dir.path().join(".deleted"))
    );
    assert_eq!(scan(dir.path()).pieces.len(), 1);
    removed.undo(dir.path()).unwrap();
    assert_eq!(scan(dir.path()).pieces.len(), 2);
    // Undoing the import archives what it added.
    added.undo(dir.path()).unwrap();
    assert!(scan(dir.path()).pieces.is_empty());
}

#[test]
fn a_linked_folder_lists_a_broken_file_and_reads_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("glass/flasks");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join("a.reshiki-piece"),
        piece_bytes(&lab("lab/a", 1)),
    )
    .unwrap();
    std::fs::write(dir.path().join("bad.reshiki-piece"), b"{").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"not a piece").unwrap();
    let pieces = [Arc::new(lab("lab/b", 1)), Arc::new(lab("lab/c", 1))];
    std::fs::write(
        dir.path().join("set.reshiki-pieces"),
        collection_bytes(&about(), &pieces),
    )
    .unwrap();
    // Hidden folders, such as the archive, are not read.
    std::fs::create_dir_all(dir.path().join(".deleted/1")).unwrap();
    std::fs::write(
        dir.path().join(".deleted/1/d.reshiki-piece"),
        piece_bytes(&lab("lab/d", 1)),
    )
    .unwrap();
    let folder = scan(dir.path());
    let mut ids: Vec<_> = folder.pieces.iter().map(|d| d.id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, ["lab/a", "lab/b", "lab/c"]);
    assert_eq!(folder.problems.len(), 1);
    assert!(folder.problems[0].path.ends_with("bad.reshiki-piece"));
}

#[test]
fn a_folder_deeper_than_the_limit_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let mut deep = dir.path().to_path_buf();
    for level in 0..=MAX_DEPTH {
        deep = deep.join(level.to_string());
    }
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("a.reshiki-piece"), piece_bytes(&lab("lab/a", 1))).unwrap();
    std::fs::write(
        deep.parent().unwrap().join("b.reshiki-piece"),
        piece_bytes(&lab("lab/b", 1)),
    )
    .unwrap();
    let ids: Vec<_> = scan(dir.path())
        .pieces
        .iter()
        .map(|d| d.id.clone())
        .collect();
    assert_eq!(ids, ["lab/b"]);
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_not_followed() {
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("a.reshiki-piece"),
        piece_bytes(&lab("lab/a", 1)),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("folder")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("a.reshiki-piece"),
        dir.path().join("file.reshiki-piece"),
    )
    .unwrap();
    let folder = scan(dir.path());
    assert!(folder.pieces.is_empty() && folder.problems.is_empty());
}

#[test]
fn a_missing_folder_is_a_problem_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let folder = scan(&dir.path().join("gone"));
    assert!(folder.pieces.is_empty());
    assert_eq!(folder.problems.len(), 1);
}

#[test]
fn two_writers_saving_the_same_piece_leave_one_good_file() {
    let dir = tempfile::tempdir().unwrap();
    let pieces = [lab("lab/a", 1)];
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| add(dir.path(), &pieces).unwrap());
        }
    });
    let folder = scan(dir.path());
    assert_eq!(folder.pieces.len(), 1);
    assert!(folder.problems.is_empty());
}

#[test]
fn a_duplicate_field_inside_a_collection_is_refused() {
    let pieces = [Arc::new(lab("lab/a", 1))];
    let text = String::from_utf8(collection_bytes(&about(), &pieces)).unwrap();
    let doubled = text.replacen("\"revision\": 1,", "\"revision\": 1,\n\"revision\": 2,", 1);
    assert_ne!(doubled, text);
    let error = read(doubled.as_bytes()).unwrap_err();
    assert!(error.contains("duplicate field"), "{error}");
}

#[test]
fn a_piece_too_big_for_its_own_file_is_refused_inside_a_collection() {
    let mut d = lab("lab/a", 1);
    for i in 0..6000 {
        d.names.insert(format!("x-{i}"), "n".repeat(180));
    }
    let error = read(&collection_bytes(&about(), &[Arc::new(d)])).unwrap_err();
    assert!(error.contains("larger than 1 MB"), "{error}");
}

#[test]
fn removing_a_piece_saved_by_someone_else_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let d = lab("lab/a", 1);
    std::fs::write(
        dir.path().join("set.reshiki-pieces"),
        collection_bytes(&about(), &[Arc::new(d.clone())]),
    )
    .unwrap();
    assert_eq!(scan(dir.path()).pieces.len(), 1);
    assert!(remove(dir.path(), &key(&d)).is_err());
}

#[test]
fn a_failed_import_leaves_nothing_added() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (lab("lab/a", 1), lab("lab/b", 1));
    // A folder where the second piece's file goes makes its write fail.
    std::fs::create_dir(dir.path().join(file_name(&b))).unwrap();
    assert!(add(dir.path(), &[a, b]).is_err());
    assert!(scan(dir.path()).pieces.is_empty());
}
