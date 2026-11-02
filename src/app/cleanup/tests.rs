use super::*;
use crate::{
    app::{InspectorTab, atom_labels},
    canvas::Edit,
};
use iced::futures::StreamExt;
use reshiki::{
    document::{Document, Point},
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

async fn dispatch(app: &mut App, message: Message) {
    let mut tasks = vec![app.update(message)];
    while let Some(task) = tasks.pop() {
        if let Some(mut stream) = iced_runtime::task::into_stream(task) {
            while let Some(action) = stream.next().await {
                if let iced_runtime::Action::Output(message) = action {
                    tasks.push(app.update(message));
                }
            }
        }
    }
}

async fn placed_fischer_converts_to_three_arm_centers(name: &str, count: usize) -> Document {
    let mut app = app(Document::default());
    let index = app
        .templates
        .library
        .iter()
        .position(|template| template.name == name)
        .unwrap();
    dispatch(&mut app, Message::InsertTemplate(index)).await;
    dispatch(
        &mut app,
        Message::Canvas(Edit::Template(Point::new(180., 120.), None)),
    )
    .await;
    assert!(!app.error, "{name}: {}", app.status);
    app.tab.doc.atom_labels.stereo = true;
    dispatch(&mut app, Message::SelectAll).await;
    // Opening Labels for the selection makes its checkbox set per-object overrides.
    dispatch(&mut app, Message::Inspector(InspectorTab::Labels)).await;
    assert_eq!(app.tab.labels.scope, atom_labels::Scope::Selection);
    dispatch(&mut app, Message::Labels(atom_labels::Action::Stereo(true))).await;
    dispatch(&mut app, Message::Analyze).await;
    assert!(!app.error, "{name}: {}", app.status);
    assert!(app.tab.doc.atom_labels.stereo);
    let centers = fischer::marked_centers(&app.tab.doc);
    assert_eq!(centers.len(), count, "{name}");
    assert!(
        centers
            .iter()
            .all(|id| app.tab.doc.atom(*id).unwrap().cip_label.is_some())
    );
    let identity = app.tab.analysis.as_ref().unwrap().inchikey.clone();
    dispatch(&mut app, Message::Cleanup(Action::ToWedge)).await;
    assert!(!app.error, "{name}: {}", app.status);
    assert_eq!(
        app.tab.analysis.as_ref().unwrap().inchikey,
        identity,
        "{name}"
    );
    for center in centers {
        let atom = app.tab.doc.atom(center).unwrap();
        let vectors: Vec<_> = app
            .tab
            .doc
            .bonds
            .iter()
            .filter_map(|bond| {
                if bond.a == center {
                    Some(bond.b)
                } else if bond.b == center {
                    Some(bond.a)
                } else {
                    None
                }
            })
            .map(|id| {
                let p = app.tab.doc.atom(id).unwrap().position;
                (
                    f64::from(p.x - atom.position.x),
                    f64::from(p.y - atom.position.y),
                )
            })
            .collect();
        assert_eq!(vectors.len(), 3, "{name} centre {center}");
        for (index, &(ax, ay)) in vectors.iter().enumerate() {
            for &(bx, by) in &vectors[index + 1..] {
                let cosine = (ax * bx + ay * by) / (ax.hypot(ay) * bx.hypot(by));
                assert!(
                    cosine >= 165_f64.to_radians().cos(),
                    "{name} centre {center} retains an angle of {} degrees",
                    cosine.clamp(-1., 1.).acos().to_degrees()
                );
            }
        }
    }
    app.tab.doc
}

#[tokio::test]
async fn placed_alanine_fischer_converts_to_three_arm_centers() {
    let doc = placed_fischer_converts_to_three_arm_centers("L-alanine · Fischer", 1).await;
    let methyl = doc.abbreviations.iter().find(|g| g.label == "CH3").unwrap();
    let carbon = doc.atom(methyl.anchor).unwrap();
    let bond = doc
        .bonds
        .iter()
        .find(|b| b.a == carbon.id || b.b == carbon.id)
        .unwrap();
    let center = doc
        .atom(if bond.a == carbon.id { bond.b } else { bond.a })
        .unwrap();
    assert!(center.position.x > carbon.position.x);
    assert_eq!(methyl.text(&doc), "H3C");
    let runs = reshiki::scene::primitives(&doc);
    let hydrogen = runs
        .iter()
        .filter_map(|run| match run {
            reshiki::scene::Primitive::Text { position, text, .. } if text == "H" => Some(position),
            _ => None,
        })
        .min_by(|a, b| {
            a.distance(carbon.position)
                .total_cmp(&b.distance(carbon.position))
        })
        .unwrap();
    assert!(hydrogen.x < carbon.position.x);
    let labels: Vec<_> = doc
        .atoms
        .iter()
        .filter_map(|atom| {
            reshiki::scene::atom_label_bounds(atom, &doc).map(|bounds| (atom.id, bounds))
        })
        .collect();
    for (index, &(id, (lo, hi))) in labels.iter().enumerate() {
        for &(other, (a, b)) in &labels[index + 1..] {
            let dx = (lo.x - b.x).max(a.x - hi.x).max(0.);
            let dy = (lo.y - b.y).max(a.y - hi.y).max(0.);
            assert!(dx.hypot(dy) >= 2., "labels {id} and {other} nearly touch");
        }
    }
}

#[tokio::test]
async fn placed_glucose_fischer_converts_to_three_arm_centers() {
    placed_fischer_converts_to_three_arm_centers("aldehydo-D-glucose · Fischer", 4).await;
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
