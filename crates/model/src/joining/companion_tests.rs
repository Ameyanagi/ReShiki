use super::*;
use crate::{
    arrow_labels::{ArrowLabel, Side},
    arrows::{ArrowStyle, Preset},
    depth_appearance::Scope,
    document::{Annotation, Arrow},
    molecule_names::MoleculeName,
    typography::TextFormat,
};

fn caption(doc: &mut Document, text: &str, at: Point) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        position: at,
        text: text.into(),
        format: TextFormat::default(),
    });
    id
}

/// The moving C-O.
fn fixture() -> (Document, [u64; 2]) {
    let mut doc = Document::default();
    let source = doc.add_atom("C", Point::new(180., 0.));
    let source_end = doc.add_atom("O", Point::new(222., 0.));
    doc.add_bond(source, source_end, 1, "plain");
    (doc, [source, source_end])
}

/// Added last, so the moved objects do not get their own IDs back by chance.
fn host(doc: &mut Document) {
    let host = doc.add_atom("C", Point::default());
    let host_end = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(host, host_end, 1, "plain");
}

fn join(doc: &Document, selected: &[u64], source: u64) -> Document {
    place(doc, selected, source, Connection::Connect)
}

fn place(doc: &Document, selected: &[u64], source: u64, mode: Connection) -> Document {
    let prepared = Prepared::new(doc, selected).unwrap();
    let (placed, _) = prepared
        .place(Point::default(), None, 5., Anchor::Atom(source), mode)
        .unwrap();
    placed
}

#[test]
fn move_and_attach_keeps_a_named_molecule_caption() {
    let (mut doc, atoms) = fixture();
    let annotation = caption(&mut doc, "methanol", Point::new(180., 30.));
    doc.molecule_names.push(MoleculeName {
        annotation,
        atoms: atoms.to_vec(),
        smiles: "CO".into(),
        text: "methanol".into(),
    });
    host(&mut doc);
    doc.validate().unwrap();
    let joined = join(&doc, &atoms, atoms[0]);
    assert_eq!(joined.molecule_names, doc.molecule_names);
}

#[test]
fn move_and_attach_keeps_an_arrow_condition_label() {
    let (mut doc, atoms) = fixture();
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(150., 80.),
        Point::new(270., 80.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    let annotation = caption(&mut doc, "heat", Point::new(200., 60.));
    doc.arrow_labels.push(ArrowLabel {
        arrow,
        annotation,
        side: Side::Above,
        gap: None,
    });
    host(&mut doc);
    doc.validate().unwrap();
    let joined = join(&doc, &[atoms[0], atoms[1], arrow], atoms[0]);
    assert_eq!(joined.arrow_labels, doc.arrow_labels);
}

#[test]
fn move_and_attach_keeps_depth_appearance_on_the_moved_atoms() {
    let (mut doc, atoms) = fixture();
    let scope = Scope::alpha_only(&doc, atoms.to_vec(), 0.5);
    doc.depth_appearance.push(scope);
    host(&mut doc);
    doc.validate().unwrap();
    let joined = join(&doc, &atoms, atoms[0]);
    assert_eq!(joined.depth_appearance.len(), 1);
    assert_eq!(joined.depth_appearance[0].atoms, atoms);
}

#[test]
fn move_and_attach_keeps_a_model_linked_to_the_moved_atoms() {
    use crate::models3d::{ForceField, Model3d, Placed};
    let (mut doc, atoms) = fixture();
    let placed = atoms
        .iter()
        .zip([-1., 1.])
        .map(|(id, x)| Placed {
            id: *id,
            xyz: [x, 0., 0.],
        })
        .collect();
    let model = Model3d::new(&doc, placed, vec![], ForceField::Mmff94s);
    doc.models.push(model);
    crate::models3d::require_version(&mut doc);
    host(&mut doc);
    doc.validate().unwrap();
    let joined = join(&doc, &atoms, atoms[0]);
    assert_eq!(joined.models.len(), 1);
    assert_eq!(joined.models[0].linked_ids(&joined), atoms);
}

/// The shared carbon is the host's, so everything that named it follows.
#[test]
fn sharing_an_atom_keeps_the_name_and_depth_scope_of_the_moved_molecule() {
    let (mut doc, atoms) = fixture();
    let annotation = caption(&mut doc, "methanol", Point::new(180., 30.));
    doc.molecule_names.push(MoleculeName {
        annotation,
        atoms: atoms.to_vec(),
        smiles: "CO".into(),
        text: "methanol".into(),
    });
    let scope = Scope::alpha_only(&doc, atoms.to_vec(), 0.5);
    doc.depth_appearance.push(scope);
    host(&mut doc);
    doc.validate().unwrap();
    let shared = doc.atoms[2].id;
    let joined = place(&doc, &atoms, atoms[0], Connection::ShareAtom);
    assert!(joined.atom(atoms[0]).is_none());
    assert_eq!(joined.molecule_names[0].annotation, annotation);
    assert_eq!(joined.molecule_names[0].atoms, [shared, atoms[1]]);
    assert_eq!(joined.depth_appearance[0].atoms, [shared, atoms[1]]);
}
