use super::*;
#[test]
fn drawable_id_order_excludes_groups_but_next_id_includes_them() {
    let mut doc = Document::default();
    doc.add_atom("C", Point::default());
    doc.add_atom("O", Point::new(42., 0.));
    doc.atoms[0].id = 10;
    doc.atoms[1].id = 2;
    doc.annotations.push(Annotation {
        id: 7,
        position: Point::default(),
        text: "caption".into(),
        format: Default::default(),
    });
    doc.arrows.push(Arrow {
        id: 6,
        start: Point::default(),
        end: Point::new(42., 0.),
        kind: "forward".into(),
        control: None,
        style: None,
    });
    doc.graphics.push(crate::graphics::Graphic::dragged(
        4,
        crate::graphics::GraphicKind::Rectangle,
        Point::default(),
        Point::new(42., 42.),
        Default::default(),
        Default::default(),
        false,
    ));
    doc.groups.push(crate::grouping::Group {
        id: 40,
        members: vec![10, 7],
        integral: false,
    });
    assert_eq!(doc.all_ids(), [10, 2, 7, 6, 4]);
    assert_eq!(doc.object_ids().collect::<Vec<_>>(), doc.all_ids());
    assert_eq!(doc.next_id(), 41);
    doc.validate().unwrap();
    doc.groups[0].id = u64::MAX;
    assert_eq!(doc.next_id(), u64::MAX);
    assert!(Document::default().all_ids().is_empty());
    assert_eq!(Document::default().next_id(), 1);
    doc.groups.clear();
    doc.atoms[1].id = 10;
    doc.atoms[0].position.x = f32::NAN;
    assert_eq!(doc.validate().unwrap_err(), "Duplicate or zero object ID");
}
#[test]
fn replacing_a_bond_keeps_paint_and_indicators_and_resets_chemical_fields() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(84., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    let bond = &mut doc.bonds[0];
    bond.highlight = Some(crate::palette::Color::Custom([10, 20, 30]));
    bond.z_order = -4;
    bond.indicator.show = Some(false);
    bond.indicator.offset = Some(Point::new(3., -2.));
    bond.indicator.style.bold = true;
    bond.double_position = crate::bonds::DoublePosition::Right;
    bond.color = crate::palette::Color::Custom([90, 80, 70]);
    bond.ring_arc = true;
    bond.projection = true;
    bond.cip_label = Some("E".into());
    bond.stereo = Some("E".into());
    bond.stereo_atoms = vec![a, c];
    bond.secondary_display = Some("Dash".into());
    let old = bond.clone();
    let untouched = doc.bonds[1].clone();
    doc.add_bond(b, a, 2, "bold");
    let replaced = &doc.bonds[0];
    assert_eq!((replaced.a, replaced.b, replaced.order), (b, a, 2));
    assert_eq!(replaced.display, "bold");
    assert_eq!(replaced.highlight, old.highlight);
    assert_eq!(replaced.z_order, old.z_order);
    assert_eq!(replaced.indicator, old.indicator);
    assert_eq!(replaced.double_position, old.double_position);
    assert_eq!(replaced.color, old.color);
    assert!(!replaced.ring_arc && !replaced.projection);
    assert_eq!(replaced.cip_label, None);
    assert_eq!(replaced.stereo, None);
    assert!(replaced.stereo_atoms.is_empty());
    assert_eq!(replaced.stereo_atoms.capacity(), 0);
    assert_eq!(replaced.secondary_display, None);
    assert_eq!(doc.bonds.len(), 2);
    assert_eq!(doc.bonds[1], untouched);
    let before = doc.clone();
    for (a, b) in [(a, a), (a, 999), (999, b)] {
        doc.add_bond(a, b, 1, "plain");
        assert_eq!(doc, before);
    }
}
#[test]
fn capped_history_keeps_chronology_noops_and_continuous_gestures() {
    let mut doc = Document::default();
    doc.add_atom("C", Point::default());
    let mut history = History::default();
    for value in 1..=101 {
        let before = doc.clone();
        doc.atoms[0].position.x = value as f32;
        assert!(history.commit(before, &doc));
    }
    for value in (1..101).rev() {
        assert!(history.undo(&mut doc));
        assert_eq!(doc.atoms[0].position.x, value as f32);
    }
    assert!(!history.undo(&mut doc));
    assert!(!history.commit(doc.clone(), &doc));
    assert!(history.can_redo());
    for value in 2..=101 {
        assert!(history.redo(&mut doc));
        assert_eq!(doc.atoms[0].position.x, value as f32);
    }
    assert!(!history.redo(&mut doc));
    assert!(history.undo(&mut doc));
    let before = doc.clone();
    doc.atoms[0].position.x = 200.;
    assert!(history.commit(before, &doc));
    assert!(!history.can_redo());
    let before = doc.clone();
    doc.atoms[0].position.x = 201.;
    assert!(history.commit_continuing(before, &doc, true));
    assert!(history.undo(&mut doc));
    assert_eq!(doc.atoms[0].position.x, 100.);
    let mut fresh = History::default();
    let before = doc.clone();
    doc.atoms[0].position.x = 300.;
    assert!(fresh.commit_continuing(before.clone(), &doc, true));
    assert!(fresh.undo(&mut doc));
    assert_eq!(doc, before);
}
#[test]
fn newer_drawings_are_reported_before_parsing_and_saves_use_the_current_version() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let saved = doc.file_json().unwrap();
    let reopened = Document::from_json(&saved).unwrap();
    assert_eq!(reopened.version, VERSION);
    assert_eq!(
        doc.version, 15,
        "saving leaves the drawing in memory unchanged"
    );
    assert_eq!(
        Document {
            version: 15,
            ..reopened
        },
        doc
    );
    // Earlier drawings keep loading.
    for (file, version) in [
        (
            &include_bytes!("../../../../tests/fixtures/legacy-drawing.moruno")[..],
            1,
        ),
        (
            include_bytes!("../../../../tests/fixtures/palette/legacy-light.rsk"),
            15,
        ),
    ] {
        assert_eq!(Document::from_json(file).unwrap().version, version);
    }
    for version in 1..=VERSION {
        let mut value = serde_json::to_value(&doc).unwrap();
        value["version"] = version.into();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(Document::from_json(&bytes).unwrap().version, version);
    }
    // A newer drawing with content this build cannot parse.
    let mut value = serde_json::to_value(&doc).unwrap();
    value["bonds"][0]["color"] = serde_json::json!({"gradient": ["red", "blue"]});
    for version in [u64::from(VERSION) + 1, u64::from(u32::MAX) + 1] {
        value["version"] = version.into();
        assert_eq!(
            Document::from_json(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
            format!(
                "This drawing was made with a newer version of ReShiki (document version {version}). Update ReShiki to open it."
            )
        );
    }
    doc.version = VERSION + 1;
    assert!(
        doc.validate()
            .unwrap_err()
            .contains("newer version of ReShiki")
    );
    doc.version = 0;
    assert!(doc.validate().is_err());
    let error = Document::from_json(b"{\"version\": 17, \"atoms\": 3}").unwrap_err();
    assert!(!error.contains("newer"), "{error}");
    assert!(Document::from_json(b"not a drawing").is_err());
}
#[test]
fn deleting_atom_removes_bonds_and_undo_restores_exact_document() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(40.0, 0.0));
    doc.add_bond(a, b, 1, "plain");
    let original = doc.clone();
    let mut history = History::default();
    doc.delete(&[a]);
    history.commit(original.clone(), &doc);
    assert!(doc.bonds.is_empty());
    assert!(doc.validate().is_ok());
    assert!(history.undo(&mut doc));
    assert_eq!(doc, original);
    assert!(history.redo(&mut doc));
    assert_eq!(doc.atoms.len(), 1);
}
#[test]
fn rejects_dangling_bond_and_duplicate_ids() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    doc.bonds.push(Bond {
        highlight: None,
        ring_arc: false,
        projection: false,
        stereo_authoritative: false,
        z_order: 0,
        indicator: Default::default(),
        cip_label: None,
        a,
        b: 50,
        order: 1,
        display: plain(),
        stereo: None,
        stereo_atoms: vec![],
        double_position: Default::default(),
        secondary_display: None,
        color: Default::default(),
    });
    assert!(doc.validate().is_err());
    doc.bonds.clear();
    doc.atoms.push(doc.atoms[0].clone());
    assert!(doc.validate().is_err());
}
