use super::*;
use iced::futures::StreamExt;
use reshiki::{
    document::Document,
    engine::LocalEngine,
    fischer::{self, Compound},
};

fn app(document: Document) -> App {
    let (mut app, _) = App::new();
    app.engine = LocalEngine::in_process();
    app.tab.busy = false;
    app.tab.doc = document;
    app.tab.history = Default::default();
    app.tab.selected.clear();
    app
}

async fn convert(app: &mut App, action: Action) {
    let task = app.update(Message::Cleanup(action));
    let mut stream = iced_runtime::task::into_stream(task).expect("conversion task");
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
            if !app.tab.busy {
                break;
            }
        }
    }
    assert!(!app.tab.busy);
}

#[tokio::test]
async fn fischer_wedge_command_and_reverse_each_commit_one_undo_step() {
    for compound in [Compound::LAlanine, Compound::DGlucose] {
        let mut app = app(fischer::document(compound, 42.).unwrap());
        let original = app.tab.doc.clone();
        // Empty selection targets Fischer objects throughout the drawing.
        convert(&mut app, Action::ToWedge).await;
        assert!(!app.error, "{}", app.status);
        assert!(app.tab.doc.bonds.iter().all(|bond| !bond.projection));
        assert!(
            app.tab
                .doc
                .bonds
                .iter()
                .any(|bond| { matches!(bond.display.as_str(), "wedge" | "hash") })
        );
        assert_eq!(app.tab.history.undo_frames().count(), 1);
        let wedge = app.tab.doc.clone();
        let identity = app.tab.analysis.as_ref().unwrap().inchikey.clone();
        app.step_history(false);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.history.can_undo());
        app.step_history(true);
        assert_eq!(app.tab.doc, wedge);

        app.tab.selected = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
        convert(&mut app, Action::ToFischer).await;
        assert!(!app.error, "{}", app.status);
        assert!(!fischer::recognize(&app.tab.doc).crosses.is_empty());
        assert_eq!(app.tab.analysis.as_ref().unwrap().inchikey, identity);
        assert_eq!(app.tab.history.undo_frames().count(), 2);
        app.step_history(false);
        assert_eq!(app.tab.doc, wedge);
    }
}

#[tokio::test]
async fn fischer_conversion_rejections_preserve_drawing_and_history() {
    let document = reshiki::haworth::sugar_document(
        reshiki::haworth::Sugar::Glucose,
        reshiki::haworth::Anomer::Alpha,
        42.,
    )
    .unwrap();
    let mut app = app(document.clone());
    app.tab.selected = app.tab.doc.atoms.iter().map(|atom| atom.id).collect();
    convert(&mut app, Action::ToFischer).await;
    assert!(app.error);
    assert_eq!(app.tab.doc, document);
    assert!(!app.tab.history.can_undo());
}
