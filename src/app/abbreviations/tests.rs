use super::*;
use crate::canvas::Edit;
use reshiki::document::{Document, Point};

fn ready() -> (App, [u64; 3]) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let before = app.tab.doc.clone();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("C", Point::new(84., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    app.changed(before);
    (app, [a, b, c])
}

fn draft(app: &mut App) {
    let _ = app.update(Message::Abbreviations(Action::Label("QaMethoxy".into())));
    let _ = app.update(Message::Abbreviations(Action::ReverseLabel("MeOQa".into())));
}

#[test]
fn first_input_binds_its_context_and_normalized_selection_does_not_clear_it() {
    let (mut app, [a, b, _]) = ready();
    app.tab.selected = vec![a, b];
    draft(&mut app);
    assert_eq!(app.abbreviations.label, "QaMethoxy");
    assert_eq!(app.abbreviations.reverse_label, "MeOQa");
    app.tab.selected = vec![b, a, a];
    let _ = app.update(Message::Tick);
    assert_eq!(app.abbreviations.label, "QaMethoxy");
    assert_eq!(app.abbreviations.reverse_label, "MeOQa");
}

#[test]
fn changing_selection_clears_both_fields_without_reviving_a_previous_draft() {
    let (mut app, [a, b, c]) = ready();
    let _ = app.update(Message::Canvas(Edit::Select(vec![b, c])));
    draft(&mut app);
    let _ = app.update(Message::Canvas(Edit::Select(vec![a])));
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
    let _ = app.update(Message::Canvas(Edit::Select(vec![b, c])));
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
}

#[test]
fn refused_define_preserves_the_same_contexts_draft() {
    let (mut app, [_, b, _]) = ready();
    let _ = app.update(Message::Canvas(Edit::Select(vec![b])));
    draft(&mut app);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Abbreviations(Action::Define));
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.abbreviations.label, "QaMethoxy");
    assert_eq!(app.abbreviations.reverse_label, "MeOQa");
}

#[test]
fn a_tab_or_file_epoch_change_clears_only_the_fragment_draft() {
    let (mut app, [_, b, c]) = ready();
    app.tab.selected = vec![b, c];
    draft(&mut app);
    app.abbreviations.preset = "OBn".into();
    app.abbreviations.catalog_warning = Some("Catalog warning retained".into());
    let first = app.tab.id;
    let _ = app.update(Message::New);
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
    assert_eq!(app.abbreviations.preset, "OBn");
    assert_eq!(
        app.abbreviations.catalog_warning.as_deref(),
        Some("Catalog warning retained")
    );
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(first)));
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
    draft(&mut app);
    app.begin_file_epoch();
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
    draft(&mut app);
    app.show_opened(Document::default(), None);
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
}

#[test]
fn background_swaps_and_recursive_updates_do_not_clear_the_visible_draft() {
    let (mut app, [a, _, _]) = ready();
    let background = app.tab.id;
    let _ = app.update(Message::New);
    let front = app.tab.id;
    draft(&mut app);
    app.inspector_tab = InspectorTab::Reactions;
    let _ = app.in_tab(background, |app| {
        app.tab.selected = vec![a];
        app.begin_file_epoch();
        let _ = app.update(Message::Tick);
    });
    let _ = app.update(Message::Tick);
    assert_eq!(app.tab.id, front);
    assert_eq!(app.tabs.temporary_depth, 0);
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert_eq!(app.abbreviations.label, "QaMethoxy");
    assert_eq!(app.abbreviations.reverse_label, "MeOQa");
    assert_eq!(app.abbreviations.context.as_ref().unwrap().tab, front);
}

#[test]
fn restoring_into_a_reused_empty_tab_starts_a_fresh_panel_context() {
    use reshiki::recovery::{Candidate, Snapshot};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let (id, epoch) = (app.tab.id, app.tab.file_epoch);
    draft(&mut app);
    app.inspector_tab = InspectorTab::Reactions;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recovered.json");
    std::fs::write(&path, "draft").unwrap();
    let mut document = Document::default();
    document.add_atom("O", Point::default());
    app.recovered = vec![Candidate {
        path,
        snapshot: Snapshot {
            document,
            source: None,
            saved_at: 1,
        },
    }];
    let _ = app.update(Message::Restore);
    assert_eq!(app.tab.id, id);
    assert_ne!(app.tab.file_epoch, epoch);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
}

#[test]
fn defining_a_group_is_private_and_clears_the_draft_in_an_isolated_process() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("synthetic-account-private-data");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app::abbreviations::tests::define_group_child",
            "--ignored",
            "--nocapture",
        ])
        .env("RESHIKI_DATA_DIR", &data)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(data.join("abbreviations.json").is_file());
}

#[test]
#[ignore = "Runs only in the isolated Define test subprocess"]
fn define_group_child() {
    let data = std::env::var_os("RESHIKI_DATA_DIR").unwrap();
    let (mut app, [_, b, c]) = ready();
    app.tab.selected = vec![b, c];
    draft(&mut app);
    let _ = app.update(Message::Abbreviations(Action::Define));
    assert!(!app.error, "{}", app.status);
    assert_eq!(
        app.status,
        "Saved QaMethoxy to your abbreviations · Type it on any endpoint"
    );
    assert!(!app.status.contains(&data.to_string_lossy().to_string()));
    if let Some(home) = std::env::var_os("HOME") {
        assert!(!app.status.contains(&home.to_string_lossy().to_string()));
    }
    assert!(!app.status.contains("abbreviations.json"));
    assert!(app.abbreviations.label.is_empty() && app.abbreviations.reverse_label.is_empty());
    let group = app.tab.doc.abbreviation(b).unwrap();
    assert_eq!(group.label, "QaMethoxy");
    assert_eq!(group.reverse_label, "MeOQa");
    let text = std::fs::read_to_string(user_catalog_path().unwrap()).unwrap();
    assert!(text.contains("QaMethoxy") && text.contains("MeOQa"));
}
