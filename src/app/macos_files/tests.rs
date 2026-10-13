use super::*;
use reshiki::document::{Document, Point};
use std::path::PathBuf;

fn drawing(element: &str) -> Vec<u8> {
    let mut doc = Document::default();
    doc.add_atom(element, Point::default());
    serde_json::to_vec(&doc).unwrap()
}

#[test]
fn finder_files_open_as_tabs_in_order_and_keep_the_edited_drawing() {
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("C", Point::default());
    app.changed(before);
    let edited = app.tab.doc.clone();
    let paths = [
        PathBuf::from("/tmp/日本語 first.rsk"),
        "/tmp/second.rsk".into(),
    ];
    let task = app.update(Message::MacFiles(Action::Open(Ok(paths.to_vec()))));
    assert!(task.units() > 0, "Reading runs off the event loop");
    assert_eq!(app.tabs.background.len(), 0);
    for (path, element) in paths.iter().zip(["N", "O"]) {
        super::super::files::finish_dispatched_open(&mut app, path.clone(), Ok(drawing(element)));
    }
    let names: Vec<_> = app.strip().map(|tab| tab.name()).collect();
    assert_eq!(names, ["Untitled", "日本語 first.rsk", "second.rsk"]);
    assert_eq!(app.tab.path.as_ref(), paths.get(1));
    assert_eq!(app.tab.doc.atoms[0].element, "O");
    assert_eq!(app.tabs.background[0].doc, edited);
    assert!(!app.error);
}

#[test]
fn invalid_native_files_report_errors_without_adding_a_tab() {
    let (mut app, _) = App::new();
    let original = app.tab.doc.clone();
    let path = PathBuf::from("/tmp/bad.rsk");
    let _ = app.update(Message::MacFiles(Action::Open(Ok(vec![path.clone()]))));
    super::super::files::finish_dispatched_open(&mut app, path, Ok(b"not json".to_vec()));
    assert_eq!(app.tab.doc, original);
    assert!(app.error);
    assert!(app.tab.path.is_none());
    assert!(app.tabs.background.is_empty());
}

#[tokio::test]
async fn finder_read_task_keeps_its_original_tab_tag_after_the_front_tab_changes() {
    use iced::futures::StreamExt;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.rsk");
    let (mut app, _) = App::new();
    let original = app.tab.id;
    let task = app.update(Message::MacFiles(Action::Open(Ok(vec![path.clone()]))));
    app.add_tab();
    let front = app.tab.id;
    assert_ne!(front, original);

    let mut stream = iced_runtime::task::into_stream(task).expect("Finder starts a read task");
    loop {
        let action = stream
            .next()
            .await
            .expect("The read returns its completion");
        let iced_runtime::Action::Output(message) = action else {
            continue;
        };
        let Message::Tab(id, message) = message else {
            panic!("Finder read completions must carry their originating tab tag");
        };
        assert_eq!(id, original);
        let Message::FilePrepared(Some((opened, result))) = *message else {
            panic!("The read returns FilePrepared");
        };
        assert_eq!(opened, path);
        assert!(result.is_err());
        break;
    }
    assert_eq!(app.tab.id, front);
}
