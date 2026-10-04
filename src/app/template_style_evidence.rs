//! Opt-in renderer snapshots through the real editor dispatch, usable on baseline too.
use super::{App, Edit, Message, document_styles, template_library};
use reshiki::{
    document::{Annotation, Document, Point},
    document_styles::Preset,
    rings,
    templates::{Anchor, Connection, LIBRARY},
};
use serde_json::{Value, json};
use std::path::Path;

fn select_builtin(app: &mut App, name: &str) {
    let index = LIBRARY.iter().position(|t| t.name == name).unwrap();
    let _ = app.update(Message::InsertTemplate(index));
    assert!(!app.error, "{}", app.status);
}

fn insert(app: &mut App, point: Point, direction: Option<Point>) {
    let _ = app.update(Message::Canvas(Edit::Template(point, direction)));
    assert!(!app.error, "{}", app.status);
}

fn journal(app: &mut App, preset: Preset) {
    let _ = app.update(Message::QuickDrawingStyle(
        document_styles::Choice::Journal(preset),
    ));
    assert!(!app.error, "{}", app.status);
}

fn snapshot(directory: &Path, case: &str, phase: &str, app: &App) -> anyhow::Result<Value> {
    let stem = format!("{case}-{phase}");
    let mut figures = Vec::new();
    for format in ["svg", "png"] {
        let figure = reshiki::export::figure(&app.tab.doc, format).map_err(anyhow::Error::msg)?;
        std::fs::write(directory.join(format!("{stem}.{format}")), figure.bytes)?;
        figures.push(json!({"format":format,"detail":figure.detail}));
    }
    std::fs::write(
        directory.join(format!("{stem}.reshiki")),
        serde_json::to_vec_pretty(&app.tab.doc)?,
    )?;
    let lengths: Vec<_> = app
        .tab
        .doc
        .bonds
        .iter()
        .map(|bond| {
            app.tab
                .doc
                .atom(bond.a)
                .unwrap()
                .position
                .distance(app.tab.doc.atom(bond.b).unwrap().position)
        })
        .collect();
    Ok(json!({
        "case":case,"phase":phase,"style":app.tab.doc.drawing_style,
        "atoms":app.tab.doc.atoms.len(),"bonds":app.tab.doc.bonds.len(),
        "bond_lengths_world":lengths,"selected_ids":app.tab.selected,
        "figures":figures,
        "svg":format!("{stem}.svg"),"png":format!("{stem}.png"),
        "document":format!("{stem}.reshiki")
    }))
}

#[test]
#[ignore = "Set RESHIKI_TEMPLATE_STYLE_EVIDENCE_DIR and run identically on baseline and candidate"]
fn capture_template_style_renderer_evidence() -> anyhow::Result<()> {
    let directory = std::env::var_os("RESHIKI_TEMPLATE_STYLE_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("Set RESHIKI_TEMPLATE_STYLE_EVIDENCE_DIR"))?;
    std::fs::create_dir_all(&directory)?;
    let mut snapshots = Vec::new();
    for case in [
        "nature_selected_before_style",
        "nature_selected_after_style",
        "acs_control",
        "personal_nature_control",
        "attached_nature_control",
        "fused_nature_control",
        "toolbar_nature_control",
    ] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        app.tab.selected.clear();
        app.tab.history = Default::default();
        app.tab.busy = false;
        app.templates.library = Default::default();
        app.sync_drawing_defaults();
        select_builtin(&mut app, "Benzene");
        insert(&mut app, Point::new(-160., 0.), None);
        snapshots.push(snapshot(&directory, case, "acs_reference", &app)?);

        if case == "nature_selected_before_style" {
            select_builtin(&mut app, "Cyclohexane");
        }
        if case != "acs_control" {
            journal(&mut app, Preset::Nature);
        }
        match case {
            "personal_nature_control" => {
                let mut saved = LIBRARY
                    .iter()
                    .find(|t| t.name == "Cyclohexane")
                    .unwrap()
                    .document
                    .clone();
                saved.annotations.push(Annotation {
                    id: saved.next_id(),
                    position: Point::new(-50., 65.),
                    text: "Saved at ACS".into(),
                    format: Default::default(),
                });
                let index = app
                    .templates
                    .library
                    .add("Saved ACS ring", "Evidence", saved, Anchor::Auto)
                    .map_err(anyhow::Error::msg)?;
                let _ = app.update(Message::InsertTemplate(index));
            }
            "toolbar_nature_control" => {
                let _ = app.update(Message::Tool(crate::canvas::Tool::RingPreset(
                    rings::Preset::ChairUp,
                )));
            }
            "nature_selected_before_style" => {}
            _ => select_builtin(&mut app, "Cyclohexane"),
        }
        if case == "attached_nature_control" {
            let _ = app.update(Message::Templates(template_library::Action::Connection(
                Connection::Connect,
            )));
        } else if case == "fused_nature_control" {
            let _ = app.update(Message::Templates(template_library::Action::Connection(
                Connection::FuseBond,
            )));
        }
        let before = app.tab.doc.clone();
        snapshots.push(snapshot(&directory, case, "before_insert", &app)?);
        match case {
            "attached_nature_control" => {
                let point = app.tab.doc.atoms[0].position;
                insert(&mut app, point, Some(point.offset(100., -80.)));
            }
            "fused_nature_control" => {
                let bond = app.tab.doc.bonds.iter().find(|b| b.order == 1).unwrap();
                let a = app.tab.doc.atom(bond.a).unwrap().position;
                let b = app.tab.doc.atom(bond.b).unwrap().position;
                let point = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
                insert(&mut app, point, Some(point.offset(80., 80.)));
            }
            "toolbar_nature_control" => {
                let _ = app.update(Message::Canvas(Edit::RingPreset(
                    rings::Preset::ChairUp,
                    Point::new(160., 0.),
                    None,
                    false,
                    false,
                )));
                assert!(!app.error, "{}", app.status);
            }
            _ => insert(&mut app, Point::new(160., 0.), Some(Point::new(195., 60.))),
        }
        assert!(
            app.tab.doc.atoms.len() > before.atoms.len(),
            "{case}: insertion did not run"
        );
        if case == "attached_nature_control" {
            assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 6);
            assert_eq!(app.tab.doc.bonds.len(), before.bonds.len() + 7);
        } else if case == "fused_nature_control" {
            assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 4);
            assert_eq!(app.tab.doc.bonds.len(), before.bonds.len() + 5);
        }
        snapshots.push(snapshot(&directory, case, "after_insert", &app)?);
        let placed = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before, "{case}: undo");
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed, "{case}: redo");
    }
    std::fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "dispatch":"App::update style/template selection and Canvas edit messages",
            "renderer":"reshiki::export::figure SVG and PNG, with native document snapshots",
            "snapshots":snapshots,
        }))?,
    )?;
    eprintln!("Template style renderer evidence: {}", directory.display());
    Ok(())
}
