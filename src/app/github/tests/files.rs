//! Every chemistry file in the picker, Insert, big folders, and the Tiles
//! and Tree views.
use super::*;
use iced::keyboard::{Key, key::Named};

const MOL: &str = include_str!("../../../../tests/fixtures/carbonyl-rotation/acetophenone.mol");
const BENZENE: &str = "benzene
  ReShiki test

  6  6  0  0  0  0  0  0  0  0999 V2000
    1.4000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.7000    1.2124    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.7000    1.2124    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -1.4000    0.0000    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
   -0.7000   -1.2124    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
    0.7000   -1.2124    0.0000 C   0  0  0  0  0  0  0  0  0  0  0  0
  1  2  1  0
  2  3  2  0
  3  4  1  0
  4  5  2  0
  5  6  1  0
  6  1  2  0
M  END
";

fn picker(app: &mut App, action: picker::Action) -> Task<Message> {
    app.update(Message::GitHub(Action::Picker(action)))
}

/// The picker on `repo()`, its top folder listed with `names` (a trailing
/// `/` is a folder), every file's last change answered: by `date` of its
/// path, or unknown.
fn files_at(app: &mut App, names: &[&str], date: impl Fn(&str) -> Option<String>) {
    app.github.repos = vec![repo()];
    let _ = picker(app, picker::Action::Show(Mode::Open));
    let _ = picker(app, picker::Action::PickRepo(repo().key));
    let entries = names.iter().map(|name| entry(name)).collect();
    let generation = picker_generation(app);
    let _ = picker(
        app,
        picker::Action::Listing(generation, Ok((sha('8'), entries))),
    );
    let asked: Vec<String> = the_picker(app).asked.iter().cloned().collect();
    for path in asked {
        let found = date(&path).map(|date| ("@alice".to_owned(), date));
        let _ = picker(app, picker::Action::Changed(generation, path, found));
    }
}

fn entry(name: &str) -> gh::Entry {
    match name.strip_suffix('/') {
        Some(folder) => gh::Entry {
            name: folder.into(),
            sha: sha('9'),
            folder: true,
        },
        None => gh::Entry {
            name: name.into(),
            sha: sha('7'),
            folder: false,
        },
    }
}

/// The picker on `repo()` in the tree, the repository read whole: `paths`,
/// a trailing `/` for a folder.
fn tree_at(app: &mut App, paths: &[&str], truncated: bool) {
    app.github.repos = vec![repo()];
    let _ = picker(app, picker::Action::Show(Mode::Open));
    let _ = picker(app, picker::Action::PickRepo(repo().key));
    let _ = picker(app, picker::Action::View(true));
    let generation = picker_generation(app);
    let _ = picker(
        app,
        picker::Action::Listing(generation, Ok((sha('8'), vec![]))),
    );
    let entries = paths.iter().map(|path| entry(path)).collect();
    let _ = picker(
        app,
        picker::Action::Whole(generation, Ok((entries, truncated))),
    );
}

/// The tree's rows, indented two spaces a level.
fn rows(app: &App) -> Vec<String> {
    (the_picker(app).rows().iter())
        .map(|row| {
            let name = row.entry.name.rsplit('/').next().unwrap();
            format!("{}{name}", "  ".repeat(row.depth))
        })
        .collect()
}

fn selected_row(app: &App) -> Option<String> {
    the_picker(app).row.clone()
}

#[test]
fn every_chemistry_file_lists_with_its_format() {
    for (name, label) in [
        ("a.mol", Some("MOL")),
        ("b.SDF", Some("SDF")),
        ("c.cdxml", Some("CDXML")),
        ("d.cdx", Some("CDX")),
        ("e.rxn", Some("RXN")),
        ("f.smi", Some("SMI")),
        ("ring.rsk", None),
        ("ring.svg", None),
        ("notes.md", None),
    ] {
        assert_eq!(gh::format_label(name), label, "{name}");
        assert_eq!(gh::listed(name), label.is_some() || name.ends_with(".rsk"));
    }
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let names = [
        "c.cdxml", "ring.rsk", "ring.svg", "notes.md", "a.mol", "b.sdf",
    ];
    files_at(&mut app, &names, |_| None);
    // Previews and other files are left out; the rest show at once.
    assert_eq!(
        picker_names(&app),
        ["a.mol", "b.sdf", "c.cdxml", "ring.rsk"]
    );
    assert_eq!(the_picker(&app).tiles().0, [0, 1, 2, 3]);
}

#[tokio::test]
async fn a_mol_opens_read_only_and_cmd_s_saves_a_copy_as_rsk() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    // A working copy of the same name is never what opens.
    let stray = workcopy::working_path(dir.path(), &repo().key, "phen.mol");
    std::fs::write(&stray, b"stray").unwrap();
    files_at(&mut app, &["phen.mol"], |_| None);
    let _ = picker(&mut app, picker::Action::Select(0));
    let _ = picker(&mut app, picker::Action::Open);
    assert!(app.github.dialog.is_some(), "{}", app.status);
    let request = app.github.seq;
    let download = api::Download {
        commit: sha('c'),
        blob: sha('b'),
        bytes: MOL.into(),
    };
    let downloaded =
        picker::Action::Downloaded(request, repo(), "phen.mol".into(), true, Ok(download));
    let task = picker(&mut app, downloaded);
    run(&mut app, task).await;
    let original = workcopy::old_path(dir.path(), &repo().key, &sha('c'), "phen.mol");
    assert_eq!(
        app.tab.path.as_deref(),
        Some(original.as_path()),
        "{}",
        app.status
    );
    assert_eq!(app.tab.doc.atoms.len(), 9);
    assert!(!app.edited(&app.tab));
    // Read only, not an old version: Send is Save a copy.
    let (label, detail, _) = app.github_send_row();
    assert_eq!(
        (label.as_str(), detail.as_str()),
        ("Save a copy…", "Read only")
    );
    let hint = app.github_tab(&app.tab, false).unwrap().hint;
    assert!(!hint.contains("Old version"));

    // Cmd+S: Save a copy, named as a drawing.
    let bytes = std::fs::read(&original).unwrap();
    let _ = app.update(Message::Save);
    let copy = the_picker(&app);
    assert!(copy.copy && copy.mode == Mode::SendTo);
    assert_eq!(copy.save_name, "phen");
    let generation = picker_generation(&app);
    let _ = picker(
        &mut app,
        picker::Action::Listing(generation, Ok((sha('8'), vec![]))),
    );
    let task = picker(&mut app, picker::Action::SendHere);
    run(&mut app, task).await;
    let saved = workcopy::working_path(dir.path(), &repo().key, "phen.rsk");
    assert_eq!(app.tab.path.as_deref(), Some(saved.as_path()));
    let saved = Document::from_native_file(&std::fs::read(&saved).unwrap()).unwrap();
    assert_eq!(saved.atoms.len(), 9);
    assert_eq!(
        std::fs::read(&original).unwrap(),
        bytes,
        "the original is untouched"
    );
}

#[test]
fn this_computer_suggests_a_drawing_name_for_a_file_save_cannot_write() {
    let (mut app, _) = App::new();
    for (path, name) in [
        ("/tmp/old/dppe.sdf", "dppe.rsk"),
        ("/tmp/old/salen.smi", "salen.rsk"),
        ("/tmp/caffeine.cdxml", "caffeine.cdxml"),
        ("/tmp/ring.rsk", "ring.rsk"),
    ] {
        app.tab.path = Some(path.into());
        assert_eq!(app.drawing_save_target(true).1, name);
    }
}

#[tokio::test]
async fn insert_puts_a_files_structures_in_the_drawing_as_one_undo_step() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.tab.busy = false;
    let sdf = format!("{MOL}>  <Name>\nacetophenone\n\n$$$$\n{BENZENE}$$$$\n");
    for (name, bytes, atoms) in [("PPh3.mol", MOL.to_owned(), 9), ("dppe.sdf", sdf, 15)] {
        files_at(&mut app, &[name], |_| None);
        // Right-click selects the file and offers Insert into drawing.
        let _ = picker(&mut app, picker::Action::FileMenu(Some(name.into())));
        assert_eq!(the_picker(&app).selected, Some(0));
        let before = app.tab.doc.clone();
        let _ = picker(&mut app, picker::Action::Insert);
        assert!(app.github.dialog.is_none(), "Insert closes the picker");
        let fetched = picker::Action::Fetched(app.tab.id, name.into(), Ok(bytes.into_bytes()));
        let task = picker(&mut app, fetched);
        run(&mut app, task).await;
        let after = app.tab.doc.clone();
        assert_eq!(
            after.atoms.len(),
            before.atoms.len() + atoms,
            "{}",
            app.status
        );
        // What was there stays where it was; what came in is selected, as a
        // paste is.
        assert!(before.atoms.iter().all(|atom| {
            after
                .atom(atom.id)
                .is_some_and(|kept| kept.position == atom.position && kept.element == atom.element)
        }));
        let new: Vec<u64> = (after.atoms.iter())
            .filter(|atom| before.atom(atom.id).is_none())
            .map(|atom| atom.id)
            .collect();
        assert!(new.iter().all(|id| app.tab.selected.contains(id)));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before, "one Undo step");
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
    }
}

#[test]
fn a_big_folder_shows_200_newest_first_and_the_search_finds_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let names: Vec<String> = (0..240).map(|n| format!("hit-{n:03}.mol")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    // The tiles on screen are asked about; later ones are newer.
    files_at(&mut app, &names, |path| {
        let n: u32 = path[4..7].parse().unwrap();
        Some(format!("2026-11-10T00:{:02}:00Z", n % 60))
    });
    let asked = the_picker(&app).asked.len();
    assert!(asked > 0 && asked < 30, "only the tiles on screen: {asked}");
    let shown = |app: &App| -> Vec<String> {
        let picker = the_picker(app);
        let (tiles, matching) = picker.tiles();
        let entries = picker.entries.as_ref().unwrap();
        let mut names: Vec<String> = tiles.iter().map(|at| entries[*at].name.clone()).collect();
        names.push(matching.to_string());
        names
    };
    let all = shown(&app);
    assert_eq!(all.len(), 201);
    assert_eq!(all[200], "240");
    // Newest first.
    assert_eq!(all[0], format!("hit-{:03}.mol", asked - 1));
    let _ = picker(&mut app, picker::Action::Search("HIT-23".into()));
    let found = shown(&app);
    assert_eq!(found.len(), 11);
    assert_eq!(found[10], "10");
    assert!(found[..10].iter().all(|name| name.starts_with("hit-23")));
    // The keyboard moves among the matches only.
    let _ = picker(&mut app, picker::Action::Move(1));
    let selected = the_picker(&app).selected.unwrap();
    assert!(picker_names(&app)[selected].starts_with("hit-23"));
    let _ = picker(&mut app, picker::Action::Search("nothing".into()));
    assert_eq!(shown(&app), ["0"]);
}

#[test]
fn the_tree_opens_and_closes_folders_from_the_keyboard() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let paths = [
        "salen.smi",
        "bipyridines/",
        "bipyridines/phen.mol",
        "bipyridines/bpy.cdxml",
        "bipyridines/bpy.svg",
        "phosphines/",
        "phosphines/dppe.sdf",
        "index.rsk",
    ];
    tree_at(&mut app, &paths, false);
    assert_eq!(
        rows(&app),
        ["bipyridines", "phosphines", "index.rsk", "salen.smi"]
    );
    // → and ← go to the tree while the search field is empty.
    let keys = app.github_keys();
    let right = keys(&Key::Named(Named::ArrowRight), Modifiers::empty());
    assert!(matches!(
        right,
        Some(Message::GitHub(Action::Picker(picker::Action::Expand(
            true
        ))))
    ));
    let insert = keys(&Key::Named(Named::Enter), Modifiers::SHIFT);
    assert!(matches!(
        insert,
        Some(Message::GitHub(Action::Picker(picker::Action::Insert)))
    ));
    let _ = picker(&mut app, picker::Action::Move(1));
    assert_eq!(selected_row(&app).as_deref(), Some("bipyridines"));
    let _ = picker(&mut app, picker::Action::Expand(true));
    assert_eq!(
        rows(&app),
        [
            "bipyridines",
            "  bpy.cdxml",
            "  phen.mol",
            "phosphines",
            "index.rsk",
            "salen.smi"
        ]
    );
    let _ = picker(&mut app, picker::Action::Expand(true));
    assert_eq!(selected_row(&app).as_deref(), Some("bipyridines/bpy.cdxml"));
    let _ = picker(&mut app, picker::Action::Move(1));
    assert_eq!(selected_row(&app).as_deref(), Some("bipyridines/phen.mol"));
    let _ = picker(&mut app, picker::Action::Expand(false));
    assert_eq!(selected_row(&app).as_deref(), Some("bipyridines"));
    let _ = picker(&mut app, picker::Action::Expand(false));
    assert_eq!(
        rows(&app),
        ["bipyridines", "phosphines", "index.rsk", "salen.smi"]
    );
    // A click opens and closes a folder; Enter on a folder too.
    let _ = picker(&mut app, picker::Action::Row("phosphines".into()));
    assert_eq!(rows(&app)[2], "  dppe.sdf");
    let _ = picker(&mut app, picker::Action::Enter);
    assert_eq!(rows(&app).len(), 4);
    // Enter on a file opens it.
    let _ = picker(&mut app, picker::Action::Row("salen.smi".into()));
    let requests = app.github.seq;
    let _ = picker(&mut app, picker::Action::Enter);
    assert_eq!(app.github.seq, requests + 1, "a download was asked for");
    // Typing, → and ← move the cursor in the search field instead.
    let _ = picker(&mut app, picker::Action::Search("s".into()));
    let keys = app.github_keys();
    assert!(keys(&Key::Named(Named::ArrowRight), Modifiers::empty()).is_none());
}

#[test]
fn tree_search_covers_the_whole_repository_with_folders_opened() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let paths = [
        "bipyridines/",
        "bipyridines/phen.mol",
        "bipyridines/deep/",
        "bipyridines/deep/phenanthroline.cdxml",
        "phosphines/",
        "phosphines/dppe.sdf",
        "phenol.rsk",
    ];
    tree_at(&mut app, &paths, false);
    // Nothing opened by hand: the matches' folders open.
    let _ = picker(&mut app, picker::Action::Search("PHEN".into()));
    assert_eq!(
        rows(&app),
        [
            "bipyridines",
            "  deep",
            "    phenanthroline.cdxml",
            "  phen.mol",
            "phenol.rsk"
        ]
    );
    let _ = picker(&mut app, picker::Action::Search("zzz".into()));
    assert!(rows(&app).is_empty());
    let _ = picker(&mut app, picker::Action::Search(String::new()));
    assert_eq!(rows(&app), ["bipyridines", "phosphines", "phenol.rsk"]);
}

#[test]
fn a_tree_listed_in_part_says_so_and_makes_nothing_new() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    // GitHub left the folders out; their files say where they are.
    tree_at(&mut app, &["ir/ethanol.mol", "nmr/2026/ethanol.mol"], true);
    assert_eq!(rows(&app), ["ir", "nmr"]);
    let picker_now = the_picker(&app);
    assert_eq!(picker_now.whole.as_ref().map(|(_, part)| *part), Some(true));
    // Its names are not all known, so nothing new is made there.
    assert!(!picker_now.can_create());
    tree_at(&mut app, &["ir/ethanol.mol"], false);
    assert!(the_picker(&app).can_create());
}

#[test]
fn the_picker_opens_in_the_view_used_last() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.repos = vec![repo()];
    let _ = picker(&mut app, picker::Action::Show(Mode::Open));
    assert!(!the_picker(&app).tree, "Tiles the very first time");
    let _ = picker(&mut app, picker::Action::View(true));
    assert!(app.github.prefs.tree);
    github(&mut app, Action::Close);
    let _ = picker(&mut app, picker::Action::Show(Mode::Open));
    assert!(the_picker(&app).tree);
    // Send chooses a folder as tiles.
    github(&mut app, Action::Close);
    let _ = picker(&mut app, picker::Action::Show(Mode::SendTo));
    assert!(!the_picker(&app).tree);
    let _ = picker(&mut app, picker::Action::View(true));
    assert!(!the_picker(&app).tree);
    github(&mut app, Action::Close);
    // Kept with the repository colors; Tiles is not written.
    let kept = serde_json::to_string(&app.github.prefs).unwrap();
    assert!(kept.contains("\"tree\":true"));
    app.github.prefs.tree = false;
    let kept = serde_json::to_string(&app.github.prefs).unwrap();
    assert!(!kept.contains("tree"));
}

#[test]
fn new_rows_need_push_and_the_default_branch() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    tree_at(&mut app, &["a.mol"], false);
    assert!(the_picker(&app).can_create());
    // Someone else's repository, read only.
    let Some(Dialog::Picker(open)) = &mut app.github.dialog else {
        panic!("no picker");
    };
    open.repo.as_mut().unwrap().push = false;
    assert!(!the_picker(&app).can_create());
    // An old version.
    let Some(Dialog::Picker(open)) = &mut app.github.dialog else {
        panic!("no picker");
    };
    open.repo.as_mut().unwrap().push = true;
    open.reference = Some("draft".into());
    assert!(!the_picker(&app).can_create());
    open_tiles_creating_nothing(&mut app);
}

/// Tiles in a read-only repository offer no `+ New` either.
fn open_tiles_creating_nothing(app: &mut App) {
    let _ = picker(app, picker::Action::View(false));
    let Some(Dialog::Picker(open)) = &mut app.github.dialog else {
        panic!("no picker");
    };
    open.reference = None;
    open.repo.as_mut().unwrap().push = false;
    let _ = picker(app, picker::Action::NewItem(false));
    assert!(the_picker(app).new_item.is_none());
}

#[test]
fn signed_out_the_tree_shows_names_only() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.client = None;
    app.github.account = Account::SignedOut(None);
    tree_at(
        &mut app,
        &["bipyridines/", "bipyridines/phen.mol", "index.rsk"],
        false,
    );
    let _ = picker(&mut app, picker::Action::Row("bipyridines".into()));
    let _ = picker(&mut app, picker::Action::Row("bipyridines/phen.mol".into()));
    let _ = picker(&mut app, picker::Action::TreeScrolled(0., 400.));
    let shown = the_picker(&app);
    assert!(shown.signed_out);
    assert_eq!(rows(&app), ["bipyridines", "  phen.mol", "index.rsk"]);
    // Nobody is asked who changed a file, nor for a preview.
    assert!(shown.asked.is_empty());
    assert!(shown.thumbs.is_empty());
}

#[test]
fn signed_in_the_tree_asks_only_about_the_rows_on_screen() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let names: Vec<String> = (0..100).map(|n| format!("hits/hit-{n:03}.mol")).collect();
    let mut paths: Vec<&str> = vec!["hits/"];
    paths.extend(names.iter().map(String::as_str));
    tree_at(&mut app, &paths, false);
    let _ = picker(&mut app, picker::Action::Row("hits".into()));
    let asked = the_picker(&app).asked.len();
    assert!(asked > 0 && asked < 25, "{asked}");
    // The selected file's preview is asked for, right-clicked too.
    let _ = picker(&mut app, picker::Action::Row("hits/hit-000.mol".into()));
    assert_eq!(the_picker(&app).thumbs.len(), 1);
    let _ = picker(
        &mut app,
        picker::Action::FileMenu(Some("hits/hit-001.mol".into())),
    );
    assert_eq!(the_picker(&app).thumbs.len(), 2);
    // Scrolled down, the rows now on screen.
    let _ = picker(&mut app, picker::Action::TreeScrolled(28. * 80., 400.));
    assert!(the_picker(&app).asked.contains("hits/hit-085.mol"));
}

#[test]
fn files_in_a_folder_show_once_their_dates_are_known() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    files_at(&mut app, &["sub/"], |_| None);
    let _ = picker(&mut app, picker::Action::Folder("sub".into()));
    let generation = picker_generation(&app);
    let entries = vec![entry("phen.mol"), entry("ring.rsk")];
    let _ = picker(
        &mut app,
        picker::Action::Listing(generation, Ok((sha('6'), entries))),
    );
    let asked: Vec<String> = the_picker(&app).asked.iter().cloned().collect();
    assert!(
        asked.iter().all(|path| path.starts_with("sub/")),
        "{asked:?}"
    );
    assert!(
        the_picker(&app).tiles().0.is_empty(),
        "waiting for the dates"
    );
    for path in asked {
        let _ = picker(&mut app, picker::Action::Changed(generation, path, None));
    }
    assert_eq!(the_picker(&app).tiles().0.len(), 2);
}

#[test]
fn a_whole_listing_is_kept_only_for_its_repository_and_trusted_only_when_read() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    let other = Repo {
        id: 9,
        key: RepoKey {
            owner: "octo".into(),
            repo: "other".into(),
        },
        ..repo()
    };
    tree_at(&mut app, &["a.mol"], false);
    let first = the_picker(&app).generation;
    app.github.repos = vec![repo(), other.clone()];
    let _ = picker(&mut app, picker::Action::PickRepo(other.key));
    // The first repository's listing, late: dropped.
    let late = picker::Action::Whole(first, Ok((vec![entry("a.mol")], false)));
    let _ = picker(&mut app, late);
    assert!(the_picker(&app).whole.is_none());
    // A listing that failed makes nothing new.
    let generation = picker_generation(&app);
    let _ = picker(
        &mut app,
        picker::Action::Listing(generation, Ok((sha('8'), vec![]))),
    );
    let failed = picker::Action::Whole(generation, Err(gh::Error::Offline));
    let _ = picker(&mut app, failed);
    assert!(!the_picker(&app).can_create());
    assert!(the_picker(&app).error.is_some());
}

#[tokio::test]
async fn insert_lands_in_the_drawing_it_was_chosen_for() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.tab.busy = false;
    let (chosen, atoms) = (app.tab.id, app.tab.doc.atoms.len());
    // Another tab is in front when the file arrives.
    open_other(&mut app, dir.path());
    let other = app.tab.doc.atoms.len();
    let fetched = picker::Action::Fetched(chosen, "PPh3.mol".into(), Ok(MOL.into()));
    let task = picker(&mut app, fetched);
    run(&mut app, task).await;
    assert_eq!(
        app.tab.doc.atoms.len(),
        other,
        "the tab in front is left alone"
    );
    let _ = app.update(Message::Tabs(tabs::Action::Select(chosen)));
    assert_eq!(app.tab.id, chosen);
    assert_eq!(app.tab.doc.atoms.len(), atoms + 9, "{}", app.status);
}

#[test]
fn tree_keys_leave_a_name_field_its_arrows_and_search_hides_a_selection() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    tree_at(&mut app, &["a.mol", "b.mol"], false);
    let _ = picker(&mut app, picker::Action::NewItem(false));
    let keys = app.github_keys();
    assert!(keys(&Key::Named(Named::ArrowLeft), Modifiers::empty()).is_none());
    let _ = picker(&mut app, picker::Action::Row("a.mol".into()));
    // Clicking a row lets the name go; the arrows are the tree's again.
    let keys = app.github_keys();
    assert!(keys(&Key::Named(Named::ArrowLeft), Modifiers::empty()).is_some());
    let _ = picker(&mut app, picker::Action::Search("b".into()));
    assert!(selected_row(&app).is_none());
    // Open on a folder opens it, as Enter does.
    tree_at(&mut app, &["sub/", "sub/a.mol"], false);
    let _ = picker(&mut app, picker::Action::Move(1));
    let _ = picker(&mut app, picker::Action::Open);
    assert_eq!(rows(&app), ["sub", "  a.mol"]);
}
