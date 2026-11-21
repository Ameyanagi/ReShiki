use crate::app::{App, Message};
use crate::canvas::{Edit, MoleculeCanvas};
use reshiki::{
    cycles::{self, Role, Shape},
    document::{Document, Point},
};

fn native_bowed_cycle(shape: Shape) -> Document {
    let mut doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/cycles/bowed-insertion.rsk"
    ))
    .unwrap();
    let g = doc.graphics.iter_mut().find(|g| g.id == 191).unwrap();
    let cycle = g.cycle.as_mut().unwrap();
    cycle.shape = shape;
    cycle.steps[0].bow_pt = 180.;
    if shape != Shape::Circle {
        let (center, _) = g.cycle_circle();
        g.axis_x = Point::new(1230., 100.);
        g.axis_y = Point::new(-55., 690.);
        g.origin = center.offset(
            -(g.axis_x.x + g.axis_y.x) * 0.5,
            -(g.axis_x.y + g.axis_y.y) * 0.5,
        );
        g.style.corner_pt = Some(24.);
    }
    doc.validate().unwrap();
    doc
}

fn middle(doc: &Document, ids: &[u64]) -> Point {
    let (lo, hi) = reshiki::scene::selection_bounds(doc, ids).unwrap();
    lo.lerp(hi, 0.5)
}

fn on_painted_step(doc: &Document) -> Point {
    let g = doc.graphics.iter().find(|g| g.id == 191).unwrap();
    let drawn = cycles::drawn(doc, g);
    drawn
        .arrows
        .iter()
        .zip(&drawn.arcs)
        .find(|(_, step)| **step == Some(0))
        .map(|(arrow, _)| arrow.point(0.5))
        .unwrap()
}

fn captioned_ethane(doc: &mut Document, two_decimal_weight: bool) -> Vec<u64> {
    let a = doc.add_atom("C", Point::new(3000., -1500.));
    let b = doc.add_atom("C", Point::new(3042., -1500.));
    doc.add_bond(a, b, 1, "plain");
    let atoms = vec![a, b];
    reshiki::molecule_names::show(
        doc,
        &atoms,
        "ethane".into(),
        "CC".into(),
        Default::default(),
        false,
    )
    .unwrap();
    reshiki::linked_properties::toggle(
        doc,
        &atoms,
        reshiki::linked_properties::Property::Weight,
        Default::default(),
        two_decimal_weight,
    )
    .unwrap();
    atoms
}

#[test]
fn native_bowed_cycle_drag_previews_the_document_that_release_commits() {
    for shape in [
        Shape::Circle,
        Shape::Ellipse,
        Shape::RoundedRectangle,
        Shape::Rectangle,
    ] {
        for zoom in [0.5, 1.75] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.doc = native_bowed_cycle(shape);
            app.tab.camera.zoom = zoom;
            let before = app.tab.doc.clone();
            let moved: Vec<_> = (40..=85).collect();
            let from = middle(&before, &moved);
            let to = on_painted_step(&before);
            let (preview, target, edit) = MoleculeCanvas::select_drag_candidate(
                &before,
                &moved,
                from,
                to,
                zoom,
                app.appearance.two_decimal_weight,
            );
            let target = target.expect("the normal drag previews a painted step drop");
            assert_eq!(
                (target.cycle, target.role),
                (191, Role::Intermediate(1)),
                "{shape:?} at zoom {zoom}"
            );
            assert_eq!(
                cycles::role_of(&preview, moved[0]),
                Some((191, Role::Intermediate(1))),
                "the draft must contain the candidate cycle, not only its highlight"
            );
            assert!(matches!(&edit, Edit::Move(ids, ..) if ids == &moved));
            assert_eq!(
                app.tab.doc, before,
                "the drag keeps the live drawing intact"
            );
            let frames = app.tab.history.undo_frames().len();
            let _ = app.update(Message::Canvas(edit));
            assert!(!app.error, "{shape:?}: {}", app.status);
            assert_eq!(
                app.tab.doc, preview,
                "the candidate's entire geometry must survive release: {shape:?}, zoom {zoom}"
            );
            assert_eq!(app.tab.history.undo_frames().len(), frames + 1);
            assert_eq!(
                Document::from_json(&preview.file_json().unwrap()).unwrap(),
                preview
            );
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, preview);
        }
    }
}

#[test]
fn a_normal_drag_that_misses_the_cycle_previews_and_commits_the_same_loose_part() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = native_bowed_cycle(Shape::Ellipse);
    let before = app.tab.doc.clone();
    let moved: Vec<_> = (40..=85).collect();
    let from = middle(&before, &moved);
    let (preview, target, edit) = MoleculeCanvas::select_drag_candidate(
        &before,
        &moved,
        from,
        Point::new(3000., 2500.),
        1.,
        app.appearance.two_decimal_weight,
    );
    assert!(target.is_none());
    assert!(cycles::role_of(&preview, moved[0]).is_none());
    let _ = app.update(Message::Canvas(edit));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn a_cycle_drag_keeps_linked_captions_and_uses_the_current_precision_in_its_snapshot() {
    for two_decimal_weight in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.appearance.two_decimal_weight = two_decimal_weight;
        app.tab.doc = native_bowed_cycle(Shape::Ellipse);
        // The open file can carry the opposite precision from the current UI.
        let atoms = captioned_ethane(&mut app.tab.doc, !two_decimal_weight);
        let before = app.tab.doc.clone();
        let moved = reshiki::molecule_names::include_annotations(&before, &atoms);
        let from = middle(&before, &atoms);
        let to = on_painted_step(&before);
        let (preview, target, edit) = MoleculeCanvas::select_drag_candidate(
            &before,
            &moved,
            from,
            to,
            1.75,
            two_decimal_weight,
        );
        assert!(target.is_some());
        assert_eq!(preview.molecule_names, before.molecule_names);
        assert_eq!(preview.property_captions.len(), 1);
        let caption = &preview.property_captions[0];
        assert_eq!(caption.two_decimal_weight, two_decimal_weight);
        assert_eq!(
            caption.text,
            if two_decimal_weight {
                "Mol. Wt.: 30.07"
            } else {
                "Mol. Wt.: 30.070"
            }
        );
        let _ = app.update(Message::Canvas(edit));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc, preview);
        assert_eq!(app.tab.history.undo_frames().len(), 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, preview);
    }
}

#[test]
fn suzuki_catalyst_drop_preserves_its_pose_through_preview_release_reopen_move_and_undo() {
    let mut doc = Document::from_native_file(include_bytes!(
        "../../../tests/fixtures/cycles/suzuki-rectangle-bow180-target.rsk"
    ))
    .unwrap();
    let target = middle(&doc, &(137..=143).collect::<Vec<_>>());
    // Retain the aryl–Pd–Br graph from the original oxidative-addition
    // intermediate, with its spectator phosphine ligands omitted.
    let catalyst = reshiki::editing::selection(&doc, &(40..=47).collect::<Vec<_>>());
    assert_eq!(catalyst.atoms.len(), 8);
    assert!(catalyst.atoms.iter().any(|atom| atom.element == "Pd"));
    assert!(catalyst.atoms.iter().any(|atom| atom.element == "Br"));
    doc.delete(&(137..=143).collect::<Vec<_>>());
    let moved = reshiki::editing::append_preserving_layout(&mut doc, &catalyst, Point::default());
    let from = middle(&doc, &moved);
    let loose_at = target.offset(600., -500.);
    doc.translate(&moved, loose_at.x - from.x, loose_at.y - from.y);
    doc.validate().unwrap();
    let before = doc;
    let from = middle(&before, &moved);
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = before.clone();
    app.tab.camera.zoom = 0.74;
    let (preview, drop, edit) = MoleculeCanvas::select_drag_candidate(
        &before,
        &moved,
        from,
        target,
        app.tab.camera.zoom,
        app.appearance.two_decimal_weight,
    );
    let drop = drop.expect("the actual bowed Suzuki shaft accepts the catalyst intermediate");
    assert_eq!((drop.cycle, drop.role), (191, Role::Intermediate(2)));
    for atom in before.atoms.iter().filter(|atom| moved.contains(&atom.id)) {
        assert!(
            preview
                .atom(atom.id)
                .unwrap()
                .position
                .distance(atom.position.offset(target.x - from.x, target.y - from.y))
                < 0.001,
            "insertion must preserve the exact pose shown under the pointer"
        );
    }
    assert_eq!(preview.bonds, before.bonds);
    let _ = app.update(Message::Canvas(edit));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    let reopened = Document::from_native_file(&preview.file_json().unwrap()).unwrap();
    assert_eq!(reopened, preview);
    let capture = reopened.graphics[0]
        .cycle
        .as_ref()
        .unwrap()
        .split_runs
        .clone();
    assert_eq!(capture.len(), 1);
    let mut reopened_app = App::new().0;
    reopened_app.tab.busy = false;
    reopened_app.tab.doc = reopened;
    let all = reopened_app.tab.doc.all_ids();
    let _ = reopened_app.update(Message::Canvas(Edit::Move(all, 20., 10.)));
    assert!(!reopened_app.error, "{}", reopened_app.status);
    assert_eq!(
        reopened_app.tab.doc.graphics[0]
            .cycle
            .as_ref()
            .unwrap()
            .split_runs,
        capture,
        "a whole-cycle move preserves the captured source stations"
    );
    let _ = reopened_app.update(Message::Undo);
    assert_eq!(reopened_app.tab.doc, preview);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
    if let Some(dir) = std::env::var_os("RESHIKI_CYCLE_CATALYST_QA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, doc) in [("before", &before), ("after", &preview)] {
            std::fs::write(dir.join(format!("{name}.rsk")), doc.file_json().unwrap()).unwrap();
        }
        std::fs::write(
            dir.join("drag.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "cycle": 191,
                "part": moved,
                "from": from,
                "to": target,
                "zoom": 0.74,
                "intermediate": "aryl–Pd–Br; spectator phosphine ligands omitted"
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
