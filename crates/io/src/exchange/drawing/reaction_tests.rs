use super::*;
use crate::{
    arrow_labels::{ArrowLabel, Side},
    arrows::{ArrowStyle, Body, Preset},
    chemistry::cdxml::schemes,
    document::{Annotation, Arrow},
    graphics::{Graphic, GraphicKind},
    reactions::{Participant, Reaction},
    scientific::SymbolKind,
};
use roxmltree::{Document as Xml, Node};

fn arrow(doc: &mut Document, x: f32, y: f32) -> u64 {
    let id = doc.next_id();
    doc.arrows.push(Arrow::new(
        id,
        Point::new(x, y),
        Point::new(x + 100., y),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    id
}

fn text(doc: &mut Document, value: &str, x: f32, y: f32) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        id,
        text: value.into(),
        position: Point::new(x, y),
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
        angle: 0.,
    });
    id
}

fn participant(atoms: Vec<u64>) -> Participant {
    Participant {
        atoms,
        coefficient: 1,
    }
}

fn xml(doc: &Document) -> String {
    write(doc, Options::default()).unwrap()
}

fn steps<'a>(tree: &'a Xml<'a>) -> Vec<Node<'a, 'a>> {
    tree.descendants()
        .filter(|n| n.has_tag_name("step"))
        .collect()
}

fn ids<'a>(node: Node<'a, '_>, name: &str) -> Vec<&'a str> {
    node.attribute(name)
        .unwrap_or_default()
        .split_whitespace()
        .collect()
}

fn target<'a>(tree: &'a Xml<'a>, id: &str) -> Node<'a, 'a> {
    tree.descendants()
        .find(|n| !n.has_tag_name("font") && n.attribute("id") == Some(id))
        .unwrap()
}

#[test]
fn detected_step_references_molecules_and_both_kinds_of_plusses() {
    let mut doc = Document::default();
    for (element, x) in [("C", 0.), ("O", 80.), ("N", 290.), ("F", 370.)] {
        doc.add_atom(element, Point::new(x, 0.));
    }
    arrow(&mut doc, 130., 0.);
    text(&mut doc, " + ", 36., -9.);
    text(&mut doc, "+", 36., 90.);
    text(&mut doc, "+", 240., -9.);
    doc.graphics.push(Graphic::dragged(
        doc.next_id(),
        GraphicKind::Symbol(SymbolKind::Plus),
        Point::new(330., 0.),
        Point::new(345., 0.),
        Default::default(),
        Default::default(),
        false,
    ));
    let xml = xml(&doc);
    let cdx_xml = crate::exchange::from_cdx(&crate::exchange::to_cdx(&xml).unwrap()).unwrap();
    for xml in [&xml, &cdx_xml] {
        assert!(schemes::validate(xml).unwrap());
        let tree = Xml::parse(xml).unwrap();
        let step = steps(&tree)[0];
        let reactants = ids(step, "ReactionStepReactants");
        let products = ids(step, "ReactionStepProducts");
        assert_eq!(reactants.len(), 2);
        assert_eq!(products.len(), 2);
        assert!(reactants.iter().chain(&products).all(|id| {
            let n = target(&tree, id);
            n.has_tag_name("fragment") && n.children().filter(|n| n.has_tag_name("n")).count() == 1
        }));
        let plus = ids(step, "ReactionStepPlusses");
        assert_eq!(plus.len(), 2);
        assert!(target(&tree, plus[0]).has_tag_name("t"));
        assert_eq!(target(&tree, plus[1]).attribute("SymbolType"), Some("Plus"));
        let arrows = ids(step, "ReactionStepArrows");
        assert_eq!(arrows.len(), 1);
        assert!(target(&tree, arrows[0]).has_tag_name("arrow"));
        assert_eq!(step.attributes().len(), 5);
    }
    let (clipboard, _) = write_clipboard(&doc).unwrap();
    assert!(schemes::validate(&clipboard).unwrap());
    let tree = Xml::parse(&clipboard).unwrap();
    assert_eq!(steps(&tree).len(), 1);
    assert_eq!(ids(steps(&tree)[0], "ReactionStepPlusses").len(), 2);
}

#[test]
fn explicit_roles_keep_conditions_and_unique_atom_map_pairs() {
    let mut doc = Document::default();
    let mut molecule = |x: f32, y: f32| {
        let atoms = [0., 40., 80.].map(|dx| doc.add_atom("C", Point::new(x + dx, y)));
        for pair in atoms.windows(2) {
            doc.add_bond(pair[0], pair[1], 1, "plain");
        }
        atoms
    };
    // Explicit roles reverse what the geometry would assign.
    let reactants = molecule(300., 0.);
    let products = molecule(0., 0.);
    let catalyst = doc.add_atom("Pd", Point::new(190., -55.));
    let agent = doc.add_atom("O", Point::new(190., 55.));
    let arrow = arrow(&mut doc, 140., 0.);
    let plus = text(&mut doc, "+", -120., 95.);
    let reflux = text(&mut doc, "reflux", 160., -53.);
    let heat = text(&mut doc, "heat", 170., -90.);
    doc.arrow_labels.push(ArrowLabel {
        arrow,
        annotation: reflux,
        side: Side::Below,
        gap: None,
    });
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(participant(reactants.to_vec()));
    reaction.products.push(participant(products.to_vec()));
    reaction.agents = vec![participant(vec![catalyst]), participant(vec![agent])];
    reaction.annotations = vec![plus, heat];
    doc.reactions.push(reaction);
    for (atom, number) in reactants
        .into_iter()
        .zip([8, 4, 4])
        .chain(products.into_iter().zip([8, 4, 5]))
    {
        doc.atom_mut(atom).unwrap().map_num = number;
    }
    let xml = xml(&doc);
    assert!(schemes::validate(&xml).unwrap());
    let tree = Xml::parse(&xml).unwrap();
    let step = steps(&tree)[0];
    assert_eq!(ids(step, "ReactionStepReactants"), ["2"]);
    assert_eq!(ids(step, "ReactionStepAtomMap"), ["3", "6"]);
    assert_eq!(ids(step, "ReactionStepPlusses").len(), 1);
    for (attr, element, condition) in [
        ("ReactionStepObjectsAboveArrow", "46", "heat"),
        ("ReactionStepObjectsBelowArrow", "8", "reflux"),
    ] {
        let objects = ids(step, attr);
        assert_eq!(objects.len(), 2);
        let molecule = target(&tree, objects[0]);
        assert_eq!(
            molecule.first_element_child().unwrap().attribute("Element"),
            Some(element)
        );
        let label = target(&tree, objects[1]);
        assert_eq!(label.first_element_child().unwrap().text(), Some(condition));
    }
}

#[test]
fn shared_intermediate_groups_steps_in_arrow_order() {
    let mut doc = Document::default();
    for (element, x, y) in [
        ("C", 0., 0.),
        ("N", 220., 0.),
        ("O", 440., 0.),
        ("Br", 0., 270.),
        ("Cl", 220., 270.),
    ] {
        doc.add_atom(element, Point::new(x, y));
    }
    arrow(&mut doc, 60., 0.);
    arrow(&mut doc, 60., 270.);
    arrow(&mut doc, 280., 0.);
    let xml = xml(&doc);
    assert!(schemes::validate(&xml).unwrap());
    let tree = Xml::parse(&xml).unwrap();
    let schemes: Vec<_> = tree
        .descendants()
        .filter(|n| n.has_tag_name("scheme"))
        .collect();
    assert_eq!(schemes.len(), 2);
    let linear: Vec<_> = schemes[0].children().filter(Node::is_element).collect();
    assert_eq!(linear.len(), 2);
    assert_eq!(
        ids(linear[0], "ReactionStepProducts"),
        ids(linear[1], "ReactionStepReactants")
    );
    let arrows: Vec<_> = tree
        .descendants()
        .filter(|n| n.has_tag_name("arrow"))
        .map(|n| n.attribute("id").unwrap())
        .collect();
    assert_eq!(ids(linear[0], "ReactionStepArrows"), [arrows[0]]);
    assert_eq!(ids(linear[1], "ReactionStepArrows"), [arrows[2]]);
    let single: Vec<_> = schemes[1].children().filter(Node::is_element).collect();
    assert_eq!(single.len(), 1);
    assert_eq!(ids(single[0], "ReactionStepArrows"), [arrows[1]]);
}

#[test]
fn an_empty_arrow_and_a_lowered_arrow_have_no_scheme() {
    let mut doc = Document::default();
    let id = arrow(&mut doc, 60., 0.);
    assert!(!schemes::validate(&xml(&doc)).unwrap());
    doc.add_atom("N", Point::new(0., 0.));
    doc.add_atom("O", Point::new(220., 0.));
    doc.arrows[0].style = Some(ArrowStyle {
        body: Body::Chevron,
        ..ArrowStyle::preset(Preset::Forward)
    });
    assert!(crate::reactions::preview(&doc, id).is_ok());
    assert!(!schemes::validate(&xml(&doc)).unwrap());
}

#[test]
fn a_shared_reactant_and_product_fragment_skips_the_step() {
    let mut doc = Document::default();
    let donor = doc.add_atom("N", Point::new(0., 0.));
    let a = doc.add_atom("H", Point::new(42., 0.));
    let b = doc.add_atom("O", Point::new(220., 0.));
    // The zero-order bond joins a drawing fragment but keeps two reaction molecules.
    doc.add_bond(donor, a, 1, "plain");
    doc.add_bond(a, b, 0, "dotted");
    doc.add_atom("F", Point::new(0., 270.));
    let arrow = arrow(&mut doc, 60., 0.);
    assert!(crate::reactions::preview(&doc, arrow).is_ok());
    let xml = xml(&doc);
    assert!(!schemes::validate(&xml).unwrap());
    let tree = Xml::parse(&xml).unwrap();
    assert_eq!(
        tree.descendants()
            .filter(|n| n.has_tag_name("fragment"))
            .count(),
        1
    );
}
