use moruno::{
    document::{Annotation, Arrow, Document, History, Point},
    editing::{self, Arrange},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle},
    grouping::Group,
    selection_region,
};

fn drawing() -> Document {
    let mut d = Document::default();
    let a = d.add_atom("C", Point::new(0., 0.));
    let b = d.add_atom("O", Point::new(42., 0.));
    d.add_bond(a, b, 1, "plain");
    d.annotations.push(Annotation {
        id: 3,
        position: Point::new(0., 60.),
        text: "substrate".into(),
        format: Default::default(),
    });
    d.arrows.push(Arrow {
        id: 4,
        start: Point::new(100., 0.),
        end: Point::new(180., 0.),
        kind: "forward".into(),
        control: None,
        style: None,
    });
    d.graphics.push(Graphic::dragged(
        5,
        GraphicKind::Rectangle,
        Point::new(200., 10.),
        Point::new(260., 90.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    ));
    d
}

#[test]
fn nested_groups_expand_keep_molecules_whole_and_ungroup_one_level() {
    let mut d = drawing();
    let original = d.clone();
    assert_eq!(d.group_selection(&[1, 3]).unwrap(), vec![1, 2, 3]);
    assert_eq!(d.expand_groups(&[2]), vec![1, 2, 3]);
    assert_eq!(d.expand_integral_groups(&[2]), vec![2]);
    d.groups[0].integral = true;
    assert_eq!(d.expand_integral_groups(&[2]), vec![1, 2, 3]);
    assert_eq!(d.group_selection(&[3, 4]).unwrap(), vec![1, 2, 3, 4]);
    assert_eq!(d.groups.len(), 2);
    d.validate().unwrap();
    let before = d.clone();
    let mut history = History::default();
    assert!(d.ungroup_selection(&[1, 2, 3, 4]));
    history.commit(before.clone(), &d);
    assert_eq!(d.groups.len(), 1);
    assert_eq!(d.expand_groups(&[4]), vec![4]);
    history.undo(&mut d);
    assert_eq!(d, before);
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), d);
    assert_eq!(d.atoms, original.atoms);
    assert_eq!(d.bonds, original.bonds);
}

#[test]
fn copying_nested_groups_remaps_ids_and_deleting_members_repairs_hierarchy() {
    let mut d = drawing();
    d.group_selection(&[1, 3]).unwrap();
    d.group_selection(&[1, 4]).unwrap();
    d.groups[1].integral = true;
    let source = editing::selection(&d, &[1, 2, 3, 4]);
    let selected = editing::append(&mut d, &source, Point::new(300., 0.));
    assert_eq!(selected.len(), 4);
    assert_eq!(d.groups.len(), 4);
    d.validate().unwrap();
    assert!(d.groups[2].members.iter().all(|id| selected.contains(id)));
    assert!(
        !d.groups[2]
            .members
            .iter()
            .any(|id| [1, 2, 3, 4].contains(id))
    );
    let partial = editing::selection(&d, &[1, 2]);
    assert!(partial.groups.is_empty());
    d.delete(&[3, 4]);
    d.validate().unwrap();
    assert_eq!(
        d.groups.iter().filter(|g| g.members.contains(&1)).count(),
        1
    );
    assert!(
        d.groups
            .iter()
            .find(|g| g.members.contains(&1))
            .unwrap()
            .integral
    );
    d.delete(&[1]);
    d.validate().unwrap();
    assert!(d.groups.iter().all(|g| !g.members.contains(&2)));
}

#[test]
fn group_validation_rejects_dangling_duplicate_and_crossing_memberships() {
    let mut d = drawing();
    for groups in [
        vec![Group {
            id: 8,
            members: vec![1, 99],
            integral: false,
        }],
        vec![Group {
            id: 1,
            members: vec![1, 2],
            integral: false,
        }],
        vec![Group {
            id: 8,
            members: vec![1, 1],
            integral: false,
        }],
        vec![
            Group {
                id: 8,
                members: vec![1, 2, 3],
                integral: false,
            },
            Group {
                id: 9,
                members: vec![2, 3, 4],
                integral: false,
            },
        ],
    ] {
        d.groups = groups;
        assert!(d.validate().is_err());
    }
}

#[test]
fn alignment_moves_grouped_caption_and_molecule_without_internal_distortion() {
    let mut d = drawing();
    d.group_selection(&[1, 3]).unwrap();
    let initial = d.clone();
    let ids = d.all_ids();
    editing::arrange(&mut d, &ids, Arrange::AlignLeft);
    let components = editing::groups(&d, &ids);
    assert_eq!(components.len(), 3);
    let first_bounds = moruno::scene::selection_bounds(&d, &[1, 2, 3]).unwrap();
    let (graphic_lo, _) = d.graphics[0].bounds();
    assert!((graphic_lo.x - d.arrows[0].bounds().0.x).abs() < 0.001);
    assert!((first_bounds.0.x - graphic_lo.x).abs() < 0.001);
    let delta = d.atoms[0].position.x - initial.atoms[0].position.x;
    assert!(
        (d.annotations[0].position.x - initial.annotations[0].position.x - delta).abs() < 0.001
    );
    assert_eq!(d.atoms[0].position.distance(d.atoms[1].position), 42.);
}

#[test]
fn concave_lasso_encloses_whole_objects_and_supports_add_subtract() {
    let mut d = Document::default();
    d.add_atom("C", Point::new(10., 10.));
    d.add_atom("O", Point::new(70., 70.));
    d.add_atom("N", Point::new(10., 70.));
    d.arrows.push(Arrow {
        id: 4,
        start: Point::new(10., 20.),
        end: Point::new(70., 70.),
        kind: "forward".into(),
        control: None,
        style: None,
    });
    let region = [
        Point::new(0., 0.),
        Point::new(100., 0.),
        Point::new(100., 30.),
        Point::new(30., 30.),
        Point::new(30., 100.),
        Point::new(0., 100.),
    ];
    assert_eq!(selection_region::objects(&d, &region), vec![1, 3]);
    assert_eq!(
        selection_region::combine(&[2], &[1, 3], true, false),
        vec![2, 1, 3]
    );
    assert_eq!(
        selection_region::combine(&[1, 2, 3], &[1, 3], false, true),
        vec![2]
    );
    // Endpoints can be inside while an edge crosses the concave notch.
    d.arrows[0].end = Point::new(90., 20.);
    d.arrows[0].start = Point::new(20., 90.);
    assert!(!selection_region::objects(&d, &region).contains(&4));
}

#[tokio::test]
async fn nested_mixed_groups_survive_chemistry_and_cdxml_roundtrip() {
    use moruno::engine::{ChemistryEngine, PythonEngine, Request};
    let engine = PythonEngine::default();
    let mut d = drawing();
    let a = d.add_atom("N", Point::new(300., 0.));
    d.group_selection(&[1, 3]).unwrap();
    d.groups[0].integral = true;
    d.group_selection(&[2, 4, 5]).unwrap();
    let group_sizes: Vec<_> = d.groups.iter().map(|g| g.members.len()).collect();
    for operation in ["analyze", "clean"] {
        let result = engine
            .execute(Request::molecule(operation, d.clone()))
            .await
            .unwrap();
        assert_eq!(result.document.unwrap().groups, d.groups);
    }
    let mut request = Request::molecule("export", d.clone());
    request.format = Some("cdxml".into());
    let xml = engine.execute(request).await.unwrap().output.unwrap();
    assert!(xml.contains("Integral=\"yes\""));
    let result = engine
        .execute(Request::import("cdxml", &xml))
        .await
        .unwrap();
    assert_eq!(result.analysis.unwrap().smiles, "CO.N");
    let r = result.document.unwrap();
    r.validate().unwrap();
    assert_eq!(
        r.groups.iter().map(|g| g.members.len()).collect::<Vec<_>>(),
        group_sizes
    );
    let nitrogen = r.atoms.iter().find(|atom| atom.element == "N").unwrap().id;
    assert!(r.groups.iter().all(|g| !g.members.contains(&nitrogen)));
    assert!(r.groups[0].integral);
    assert!(!r.groups[1].integral);
    assert!(d.groups.iter().all(|g| !g.members.contains(&a)));
}
