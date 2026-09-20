use moruno::{
    document::{Annotation, Document, Point},
    editing,
    engine::{ChemistryEngine, PythonEngine, Request},
    template_library::Library,
    templates::{Anchor, place_anchored},
};

fn ethanol() -> Document {
    let mut d = Document::default();
    let a = d.add_atom("C", Point::new(0., 0.));
    let b = d.add_atom("C", Point::new(42., 0.));
    let c = d.add_atom("O", Point::new(63., 36.373));
    d.add_bond(a, b, 1, "plain");
    d.add_bond(b, c, 1, "plain");
    d
}
#[tokio::test]
async fn exact_source_atom_changes_regiochemistry_without_substitution_or_fallback() {
    let part = ethanol();
    let mut host = Document::default();
    let c = host.add_atom("C", Point::default());
    let f = host.add_atom("F", Point::new(-42., 0.));
    host.add_bond(c, f, 1, "plain");
    let engine = PythonEngine::default();
    for (source, expected) in [(1, "FCCO"), (2, "CC(O)F")] {
        let (doc, ids) = place_anchored(
            &host,
            &part,
            Point::default(),
            None,
            5.,
            Anchor::Atom(source),
        )
        .unwrap();
        doc.validate().unwrap();
        assert_eq!(ids[source as usize - 1], c);
        assert_eq!(doc.atoms.len(), 4);
        assert_eq!(doc.atom(f), host.atom(f));
        let result = engine
            .execute(Request::molecule("analyze", doc))
            .await
            .unwrap()
            .analysis
            .unwrap();
        let expected = engine
            .execute(Request::import_smiles(expected))
            .await
            .unwrap()
            .analysis
            .unwrap();
        assert_eq!(result.smiles, expected.smiles);
    }
    assert!(place_anchored(&host, &part, Point::default(), None, 5., Anchor::Atom(3)).is_err());
    assert!(place_anchored(&host, &part, Point::default(), None, 5., Anchor::Atom(999)).is_err());
    assert!(place_anchored(&host, &part, Point::default(), None, 5., Anchor::Bond(1, 2)).is_err());
}
#[test]
fn source_bond_is_exact_and_mixed_objects_transform_and_remap_together() {
    let mut part = ethanol();
    let id = part.next_id();
    part.annotations.push(Annotation {
        id,
        position: Point::new(21., -30.),
        text: "ethanol".into(),
        format: Default::default(),
    });
    let ids = part.all_ids();
    part.group_selection(&ids).unwrap();
    let mut host = Document::default();
    let a = host.add_atom("C", Point::new(0., 0.));
    let b = host.add_atom("C", Point::new(0., 84.));
    host.add_bond(a, b, 1, "plain");
    let (result, placed) = place_anchored(
        &host,
        &part,
        Point::new(0., 42.),
        Some(Point::new(-100., 42.)),
        5.,
        Anchor::Bond(1, 2),
    )
    .unwrap();
    result.validate().unwrap();
    assert_eq!(result.atoms.len(), 3);
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].members.len(), 4);
    assert!(result.groups[0].members.contains(&a));
    assert!(result.groups[0].members.contains(&b));
    // Source label is 30 units from the shared bond midpoint; attachment scales by two.
    assert!((result.annotations[0].position.distance(Point::new(0., 42.)) - 60.).abs() < 0.01);
    assert_eq!(placed.len(), 4);
    assert!(
        place_anchored(
            &host,
            &part,
            Point::new(0., 42.),
            None,
            5.,
            Anchor::Bond(2, 3)
        )
        .is_err()
    );
    let (free, _) = place_anchored(
        &Document::default(),
        &part,
        Point::new(100., 100.),
        Some(Point::new(100., 200.)),
        5.,
        Anchor::Atom(1),
    )
    .unwrap();
    assert_eq!(free.atoms[0].position, Point::new(100., 100.));
    assert!((free.atoms[1].position.y - 142.).abs() < 0.01);
    assert!((free.annotations[0].position.x - 130.).abs() < 0.01);
    free.validate().unwrap();
}
#[test]
fn collections_persist_styles_groups_anchors_and_favorites_and_reject_invalid_updates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("templates.json");
    let mut library = Library::default();
    let doc = ethanol();
    let index = library
        .add("  Ethanol fragment  ", " Reagents ", doc, Anchor::Atom(2))
        .unwrap();
    let id = library.get(index).unwrap().id.clone();
    library.favorites.push(id);
    library.save(&path).unwrap();
    assert_eq!(Library::load(&path).unwrap(), library);
    let before = std::fs::read(&path).unwrap();
    let mut invalid = library.clone();
    invalid.templates[0].anchor = Anchor::Atom(999);
    assert!(invalid.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(Library::from_bytes(b"{ partial").is_err());
    let mut imported = Library::default();
    assert_eq!(imported.merge(library.clone()).unwrap(), 1);
    assert_eq!(imported.merge(library.clone()).unwrap(), 0);
    let mut revised = library.clone();
    revised.templates[0].name = "Revised ethanol".into();
    assert_eq!(imported.merge(revised.clone()).unwrap(), 1);
    assert_eq!(imported.merge(revised).unwrap(), 0);
    assert_eq!(imported.templates.len(), 2);
    assert_ne!(imported.templates[0].id, imported.templates[1].id);
    assert_eq!(imported.favorites.len(), 2);
    assert_eq!(library.get(index).unwrap().name, "Ethanol fragment");
    let ids = library.templates[0].document.all_ids();
    let copied = editing::selection(&library.templates[0].document, &ids);
    assert_eq!(copied, library.templates[0].document);
}

#[test]
fn concurrent_library_updates_and_corrupt_files_cannot_overwrite_saved_templates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("templates.json");
    let original = Library::default();
    let mut first = original.clone();
    first
        .add("Ethanol", "Reagents", ethanol(), Anchor::Auto)
        .unwrap();
    first.save_checked(&path, &original).unwrap();
    let mut stale = original.clone();
    stale
        .add("Another fragment", "Reagents", ethanol(), Anchor::Atom(2))
        .unwrap();
    assert!(stale.save_checked(&path, &original).is_err());
    assert_eq!(Library::load(&path).unwrap(), first);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.with_extension("json.lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(stale.save_checked(&path, &first).is_err());
    drop(lock);
    stale.save_checked(&path, &first).unwrap();
    assert_eq!(Library::load(&path).unwrap(), stale);
    std::fs::write(&path, b"broken library").unwrap();
    assert!(first.save_checked(&path, &stale).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"broken library");
}
