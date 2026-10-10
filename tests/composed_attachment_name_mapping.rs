//! UNRUN private proposal: exercise the composed native model without a worker or GUI.
use reshiki::{
    arrow_anchors::{self, Target},
    atom_labels::{self, Number, Owner},
    document::{Document, History, Point, VERSION},
    editing, molecule_names,
    palette::Color,
    transaction,
    typography::{TextFormat, TextStyle},
};
use std::collections::HashMap;

const CAPTURED: &[u8] =
    include_bytes!("fixtures/mechanism-attachments-92/desktop/methanol-attached-arrow-desktop.rsk");

fn near(actual: Point, expected: Point) {
    assert!(
        (actual.x - expected.x).abs() < 0.0002,
        "x: {actual:?} != {expected:?}"
    );
    assert!(
        (actual.y - expected.y).abs() < 0.0002,
        "y: {actual:?} != {expected:?}"
    );
}

#[test]
fn composed_attached_methanol_caption_and_maps_remap_without_reassigning_local_mark_ids() {
    // These are authentic v21 bytes. The name, depth and map presentation below
    // are declared in-memory extensions rather than a claimed desktop capture.
    let mut source = Document::from_native_file(CAPTURED).unwrap();
    assert_eq!(source.version, 21);
    assert_eq!(
        (source.atoms.len(), source.bonds.len(), source.arrows.len()),
        (2, 1, 1)
    );
    let carbon = source.atoms.iter().find(|a| a.element == "C").unwrap().id;
    let oxygen = source.atoms.iter().find(|a| a.element == "O").unwrap().id;
    let mark = source.atom(oxygen).unwrap().marks[0].clone();
    let mark_id = mark.id.expect("captured mark has stable local ID");
    assert_eq!(
        mark_id, carbon,
        "the original local mark ID deliberately aliases an atom ID"
    );
    assert!(
        mark.offset.y.abs() > 20.,
        "use the saved manually displaced mark"
    );
    let captured_arrow = source.arrows[0].clone();
    assert!(
        matches!(captured_arrow.start_anchor.as_ref().unwrap().target,
        Target::LonePair { atom, mark: id } if atom == oxygen && id == mark_id)
    );
    assert!(matches!(captured_arrow.end_anchor.as_ref().unwrap().target,
        Target::Atom { atom } if atom == carbon));
    assert_ne!(captured_arrow.cubic.unwrap()[0], captured_arrow.start);
    assert_ne!(captured_arrow.cubic.unwrap()[1], captured_arrow.end);

    source.atom_labels.maps = false;
    source.atom_mut(carbon).unwrap().depth = 2.25;
    source.atom_mut(oxygen).unwrap().depth = -1.5;
    source.atom_mut(carbon).unwrap().map_num = 141;
    let mapping = &mut source.atom_mut(carbon).unwrap().display.mapping;
    mapping.show = Some(true);
    mapping.offset = Some(Point::new(45., 15.));
    mapping.style = TextStyle {
        size_pt: 9.,
        italic: true,
        color: Color::Custom([23, 91, 167]),
        ..Default::default()
    };
    source.atom_mut(oxygen).unwrap().map_num = 7;
    source.atom_mut(oxygen).unwrap().display.number = Some(Number {
        text: "oxygen-site".into(),
        offset: Some(Point::new(15., 45.)),
        style: TextStyle {
            bold: true,
            ..Default::default()
        },
    });
    let atoms = molecule_names::component(&source, carbon);
    let caption = molecule_names::show(
        &mut source,
        &atoms,
        "methanol".into(),
        "CO".into(),
        TextFormat::default(),
    )
    .unwrap();
    arrow_anchors::reconcile(&mut source);
    assert_eq!(source.atom(oxygen).unwrap().marks[0], mark);
    source.validate().unwrap();
    let original_arrow = source.arrows[0].clone();

    // Omitting the caption ID must still include its valid chemical association.
    let selected = editing::selection(&source, &[carbon, oxygen, original_arrow.id]);
    assert_eq!(
        (
            selected.atoms.len(),
            selected.annotations.len(),
            selected.molecule_names.len()
        ),
        (2, 1, 1)
    );
    assert_eq!(selected.annotations[0].id, caption);
    let clipboard = Document::from_native_file(&selected.file_json().unwrap()).unwrap();
    assert_eq!(clipboard.version, VERSION);
    let mut target = Document {
        drawing_style: clipboard.drawing_style.clone(),
        ..Default::default()
    };
    for (element, x) in [("He", -500.), ("Ne", -450.), ("Ar", -400.)] {
        target.add_atom(element, Point::new(x, -200.));
    }
    assert_ne!(target.atom_labels.maps, clipboard.atom_labels.maps);
    let before = target.clone();
    let offset = Point::new(180., 90.);
    let added = editing::append(&mut target, &clipboard, offset);
    assert_eq!(added.len(), clipboard.all_ids().len());
    let remap: HashMap<_, _> = clipboard.all_ids().into_iter().zip(added).collect();
    assert!(clipboard.all_ids().iter().all(|id| remap[id] != *id));
    let mut history = History::default();
    assert!(
        transaction::apply(&mut target, &mut history, before.clone(), false)
            .unwrap()
            .recorded
    );
    let actual_arrow = &target.arrows[0];
    assert_eq!(actual_arrow.id, remap[&original_arrow.id]);
    assert!(matches!(actual_arrow.start_anchor.as_ref().unwrap().target,
        Target::LonePair { atom, mark: id } if atom == remap[&oxygen] && id == mark_id));
    assert!(matches!(actual_arrow.end_anchor.as_ref().unwrap().target,
        Target::Atom { atom } if atom == remap[&carbon]));
    assert_ne!(
        mark_id, remap[&carbon],
        "a local mark is not a document object ID"
    );
    assert_eq!(target.atom(remap[&oxygen]).unwrap().marks[0], mark);
    near(
        actual_arrow.start,
        original_arrow.start.offset(offset.x, offset.y),
    );
    near(
        actual_arrow.end,
        original_arrow.end.offset(offset.x, offset.y),
    );
    for (actual, original) in actual_arrow
        .cubic
        .unwrap()
        .into_iter()
        .zip(original_arrow.cubic.unwrap())
    {
        near(actual, original.offset(offset.x, offset.y));
    }
    let name = &target.molecule_names[0];
    assert_eq!(name.annotation, remap[&caption]);
    assert_eq!(
        name.atoms,
        atoms.iter().map(|id| remap[id]).collect::<Vec<_>>()
    );
    assert_eq!((&name.text[..], &name.smiles[..]), ("methanol", "CO"));
    for id in &atoms {
        let original = clipboard.atom(*id).unwrap();
        let actual = target.atom(remap[id]).unwrap();
        assert_eq!(
            (actual.depth, actual.map_num),
            (original.depth, original.map_num)
        );
        assert_eq!(actual.display.number, original.display.number);
        assert_eq!(actual.display.mapping.style, original.display.mapping.style);
        assert_eq!(
            actual.display.mapping.offset,
            original.display.mapping.offset
        );
    }
    let labels = atom_labels::indicators(&target);
    assert!(
        labels
            .iter()
            .any(|v| v.owner == Owner::Mapping(remap[&carbon]) && v.text == "141")
    );
    assert!(
        !labels
            .iter()
            .any(|v| v.owner == Owner::Mapping(remap[&oxygen]))
    );
    assert!(
        labels
            .iter()
            .any(|v| v.owner == Owner::Number(remap[&oxygen]) && v.text == "oxygen-site")
    );
    let committed = target.clone();
    assert!(history.undo(&mut target));
    assert_eq!(target, before);
    assert!(history.redo(&mut target));
    assert_eq!(target, committed);
    let once = target.clone();
    arrow_anchors::reconcile(&mut target);
    assert_eq!(
        target, once,
        "a reconciled paste must not translate controls again"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("attached-name-maps.rsk");
    std::fs::write(&path, target.file_json().unwrap()).unwrap();
    let reopened = Document::from_native_file(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(reopened.version, VERSION);
    assert_eq!(reopened, target.current());

    // Arrow-only clipboard data retains its curve but cannot retain external owners.
    let only = editing::selection(&source, &[original_arrow.id]);
    assert!(only.atoms.is_empty() && only.molecule_names.is_empty());
    assert_eq!(
        (
            only.arrows[0].start,
            only.arrows[0].end,
            only.arrows[0].cubic
        ),
        (
            original_arrow.start,
            original_arrow.end,
            original_arrow.cubic
        )
    );
    assert!(only.arrows[0].start_anchor.is_none() && only.arrows[0].end_anchor.is_none());
    only.validate().unwrap();

    // Chemical maps are excluded from name identity; a real isotope change is not.
    let before_map = target.clone();
    target.atom_mut(remap[&carbon]).unwrap().map_num = 142;
    transaction::apply(&mut target, &mut history, before_map, false).unwrap();
    assert_eq!(target.molecule_names.len(), 1);
    assert_eq!(target.molecule_names[0].annotation, remap[&caption]);
    let before_isotope = target.clone();
    target.atom_mut(remap[&carbon]).unwrap().isotope = 13;
    transaction::apply(&mut target, &mut history, before_isotope.clone(), false).unwrap();
    assert!(target.molecule_names.is_empty());
    assert!(!target.annotations.iter().any(|a| a.id == remap[&caption]));
    assert!(history.undo(&mut target));
    assert_eq!(target, before_isotope);
}
