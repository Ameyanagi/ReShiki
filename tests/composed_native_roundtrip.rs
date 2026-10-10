//! UNRUN proposal: apply only after the complete model feature union exists.
use reshiki::{
    arrows::{ArrowStyle, Preset},
    atom_labels::{self, Number, Owner},
    document::{Arrow, Document, Point, VERSION},
    editing, molecule_names,
    palette::Color,
    rear_opacity,
    recovery::Recovery,
    typography::{TextFormat, TextStyle},
};
use std::collections::{BTreeMap, HashMap};

const CAGE: &[u8] = include_bytes!("fixtures/rear-opacity/c60-rear-opacity-25.rsk");
const METAFILE: &[u8] = include_bytes!(
    "../docs/changes/fixtures/emf-import/windows-native-20261010/desktop-import-20261010.rsk"
);
const EMF: &[u8] =
    include_bytes!("../docs/changes/fixtures/emf-import/windows-native-20261010/source.emf");

fn composed() -> Document {
    // Authentic fixture versions are left intact; only new save output uses VERSION.
    let mut doc = Document::from_native_file(CAGE).unwrap();
    assert_eq!(doc.version, 22);
    assert_eq!((doc.atoms.len(), doc.bonds.len()), (60, 90));
    // Model a valid legacy frozen scope whose captured classification is in
    // weights. Partial selection must use that fallback, not discard it.
    doc.depth_appearance[0].automatic = false;
    assert!(doc.depth_appearance[0].rear_weights.is_empty());
    doc.atom_labels.maps = false;
    let a = doc.add_atom("C", Point::new(400., 300.));
    let b = doc.add_atom("C", Point::new(442., 300.));
    let c = doc.add_atom("O", Point::new(484., 300.));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    for (id, map) in [(a, i32::MAX as u32), (b, 7), (c, 11)] {
        doc.atom_mut(id).unwrap().map_num = map;
    }
    let mapping = &mut doc.atom_mut(a).unwrap().display.mapping;
    mapping.show = Some(true);
    mapping.offset = Some(Point::new(3., -28.));
    mapping.style = TextStyle {
        size_pt: 9.,
        italic: true,
        color: Color::Custom([23, 91, 167]),
        ..Default::default()
    };
    doc.atom_mut(b).unwrap().display.number = Some(Number {
        text: "site-B".into(),
        offset: Some(Point::new(4., 23.)),
        style: TextStyle {
            bold: true,
            ..Default::default()
        },
    });
    doc.atom_mut(c).unwrap().display.mapping.show = Some(false);
    molecule_names::show(
        &mut doc,
        &[a, b, c],
        "ethan-1-ol".into(),
        "CCO".into(),
        TextFormat::default(),
    )
    .unwrap();
    let mut arrow = Arrow::new(
        doc.next_id(),
        Point::new(300., 180.),
        Point::new(520., 180.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    arrow.cubic = Some([Point::new(340., 110.), Point::new(480., 240.)]);
    doc.arrows.push(arrow);
    let picture = Document::from_native_file(METAFILE).unwrap();
    assert_eq!(picture.version, 20);
    assert_eq!(
        editing::append(&mut doc, &picture, Point::new(650., 0.)).len(),
        1
    );
    // This valid unrelated atom is deliberately excluded from selected copy.
    doc.add_atom("F", Point::new(900., 500.));
    doc.validate().unwrap();
    doc
}

fn assert_dual_picture(doc: &Document) {
    let actual = doc.graphics[0].picture.as_ref().unwrap();
    let source = Document::from_native_file(METAFILE).unwrap();
    let expected = source.graphics[0].picture.as_ref().unwrap();
    assert_eq!(actual.emf(), Some(EMF));
    assert_eq!(actual.png(), expected.png());
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height())
    );
}

fn assert_independent_labels(doc: &Document, named: &[u64]) {
    let labels = atom_labels::indicators(doc);
    let shown = labels
        .iter()
        .find(|v| v.owner == Owner::Mapping(named[0]))
        .unwrap();
    assert_eq!(shown.text, i32::MAX.to_string());
    assert_eq!(
        shown.style,
        doc.atom(named[0]).unwrap().display.mapping.style
    );
    assert!(
        !labels
            .iter()
            .any(|v| matches!(v.owner, Owner::Mapping(id) if named[1..].contains(&id)))
    );
    assert!(
        labels
            .iter()
            .any(|v| v.owner == Owner::Number(named[1]) && v.text == "site-B")
    );
}

fn assert_nontrivial_rear_ink(doc: &Document) {
    let paint = rear_opacity::Paint::new(doc);
    let alphas: Vec<_> = doc
        .bonds
        .iter()
        .flat_map(|bond| [0.25, 0.5, 0.75].map(|t| paint.bond(bond, t)))
        .collect();
    assert!(
        alphas.contains(&0.25),
        "rear state became completely opaque"
    );
    assert!(alphas.contains(&1.), "exposed ink stopped being opaque");
}

#[test]
fn composed_native_file_and_selected_append_preserve_independent_feature_data() {
    let doc = composed();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("composed.rsk");
    std::fs::write(&file, doc.file_json().unwrap()).unwrap();
    let reopened = Document::from_native_file(&std::fs::read(&file).unwrap()).unwrap();
    let mut expected = doc.current();
    atom_labels::clear_computed(&mut expected);
    assert_eq!(reopened.version, VERSION);
    assert_eq!(reopened, expected);
    assert_dual_picture(&reopened);
    assert_independent_labels(&reopened, &reopened.molecule_names[0].atoms);
    assert_nontrivial_rear_ink(&reopened);

    // Selecting all actual molecules, the arrow and EMF implicitly includes
    // the linked caption; the unselected F must not sneak into clipboard data.
    let mut ids = reopened.depth_appearance[0].atoms.clone();
    ids.extend(&reopened.molecule_names[0].atoms);
    ids.push(reopened.arrows[0].id);
    ids.push(reopened.graphics[0].id);
    let selected = editing::selection(&reopened, &ids);
    assert_eq!(
        (
            selected.atoms.len(),
            selected.annotations.len(),
            selected.molecule_names.len()
        ),
        (63, 1, 1)
    );
    let caption = &selected.annotations[0];
    assert_eq!(selected.molecule_names[0].annotation, caption.id);
    let clipboard = Document::from_native_file(&selected.file_json().unwrap()).unwrap();
    let mut target = Document::default(); // Maps visible by default, unlike source.
    target.add_atom("He", Point::new(-200., -200.));
    assert_ne!(target.atom_labels.maps, clipboard.atom_labels.maps);
    let offset = Point::new(240., 80.);
    let added = editing::append(&mut target, &clipboard, offset);
    assert_eq!(added.len(), clipboard.all_ids().len());
    let remap: HashMap<_, _> = clipboard.all_ids().into_iter().zip(added).collect();
    assert!(clipboard.all_ids().iter().all(|id| remap[id] != *id));
    let source_name = &clipboard.molecule_names[0];
    let name = &target.molecule_names[0];
    assert_eq!(
        name.atoms,
        source_name
            .atoms
            .iter()
            .map(|id| remap[id])
            .collect::<Vec<_>>()
    );
    assert_eq!(name.annotation, remap[&source_name.annotation]);
    assert_eq!(
        (&name.smiles, &name.text),
        (&source_name.smiles, &source_name.text)
    );
    let mut wanted_caption = caption.clone();
    wanted_caption.id = remap[&caption.id];
    wanted_caption.position = caption.position.offset(offset.x, offset.y);
    assert_eq!(target.annotations[0], wanted_caption);
    for source in &clipboard.atoms {
        let actual = target.atom(remap[&source.id]).unwrap();
        assert_eq!(actual.position, source.position.offset(offset.x, offset.y));
        assert_eq!(
            (actual.depth, actual.map_num),
            (source.depth, source.map_num)
        );
        assert_eq!(actual.display.number, source.display.number);
        assert_eq!(actual.display.mapping.style, source.display.mapping.style);
        assert_eq!(actual.display.mapping.offset, source.display.mapping.offset);
    }
    assert_independent_labels(&target, &name.atoms);
    let original = &clipboard.arrows[0];
    let arrow = &target.arrows[0];
    assert_eq!(arrow.id, remap[&original.id]);
    assert_eq!(
        (arrow.start, arrow.end),
        (
            original.start.offset(offset.x, offset.y),
            original.end.offset(offset.x, offset.y)
        )
    );
    assert_eq!(
        arrow.cubic,
        original
            .cubic
            .map(|points| points.map(|p| p.offset(offset.x, offset.y)))
    );
    assert_eq!(arrow.control, None);
    let scope = &target.depth_appearance[0];
    let source_scope = &clipboard.depth_appearance[0];
    assert_eq!(
        scope.atoms,
        source_scope
            .atoms
            .iter()
            .map(|id| remap[id])
            .collect::<Vec<_>>()
    );
    assert_eq!(
        scope.weights,
        source_scope
            .weights
            .iter()
            .map(|(id, w)| (remap[id], *w))
            .collect::<BTreeMap<_, _>>()
    );
    assert_eq!(
        (scope.automatic, scope.rear_opacity, &scope.rear_weights),
        (false, 0.25, &BTreeMap::<u64, f32>::new())
    );
    assert_dual_picture(&target);
    assert_nontrivial_rear_ink(&target);
    target.validate().unwrap();
    let mut expected = target.current();
    atom_labels::clear_computed(&mut expected);
    assert_eq!(
        Document::from_native_file(&target.file_json().unwrap()).unwrap(),
        expected
    );

    // A partial cage exercises the legacy empty-rear_weights fallback; the
    // complete named molecule still retains its linked caption alongside it.
    let mut partial_ids = source_scope.atoms[..3].to_vec();
    partial_ids.extend(&source_name.atoms);
    let partial = editing::selection(&clipboard, &partial_ids);
    let wanted: BTreeMap<_, _> = source_scope
        .weights
        .iter()
        .filter(|(id, _)| partial_ids.contains(id))
        .map(|(id, w)| (*id, *w))
        .collect();
    assert_eq!(partial.depth_appearance[0].rear_weights, wanted);
    assert_eq!(partial.molecule_names, clipboard.molecule_names);
    partial.validate().unwrap();
    let mut pasted_partial = Document::default();
    pasted_partial.add_atom("He", Point::default());
    let added = editing::append(&mut pasted_partial, &partial, offset);
    assert_eq!(added.len(), partial.all_ids().len());
    let ids: HashMap<_, _> = partial.all_ids().into_iter().zip(added).collect();
    assert_eq!(
        pasted_partial.depth_appearance[0].rear_weights,
        wanted
            .iter()
            .map(|(id, w)| (ids[id], *w))
            .collect::<BTreeMap<_, _>>()
    );
    pasted_partial.validate().unwrap();

    // This union must retain map validation at the actual public-loader gate.
    let mut excessive = reopened;
    excessive.atom_mut(source_name.atoms[0]).unwrap().map_num = i32::MAX as u32 + 1;
    assert!(
        Document::from_native_file(&excessive.file_json().unwrap())
            .unwrap_err()
            .contains("Atom maps")
    );
}

#[test]
fn composed_recovery_discovery_preserves_the_same_native_fields() {
    let doc = composed();
    let dir = tempfile::tempdir().unwrap();
    let store = Recovery::in_directory(dir.path()).unwrap();
    let source = dir.path().join("original.rsk");
    store.save(&doc, Some(source.clone())).unwrap();
    // A nonnumeric crash filename avoids depending on another OS process/PID.
    // candidates() still runs its actual deserialization, validate and migrate.
    std::fs::rename(&store.session, dir.path().join("composed-crash.json")).unwrap();
    let mut candidates = store.candidates();
    assert_eq!(candidates.len(), 1);
    let snapshot = candidates.remove(0).snapshot;
    assert_eq!(snapshot.source, Some(source));
    assert_eq!(snapshot.document.version, VERSION);
    assert_eq!(snapshot.document, doc.current());
    assert_dual_picture(&snapshot.document);
    assert_independent_labels(
        &snapshot.document,
        &snapshot.document.molecule_names[0].atoms,
    );
    assert_nontrivial_rear_ink(&snapshot.document);
}
