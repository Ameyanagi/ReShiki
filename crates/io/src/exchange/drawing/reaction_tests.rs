use super::*;
use crate::{
    arrow_labels::{ArrowLabel, Side},
    arrows::{ArrowStyle, Body, Preset},
    chemistry::cdxml::schemes,
    document::{Annotation, Arrow},
    graphics::{Graphic, GraphicKind, LinePattern},
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
    let reflux = text(&mut doc, "reflux", 160., 80.);
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
fn disconnected_salt_participants_reference_every_fragment_once() {
    let mut doc = Document::default();
    let sodium = doc.add_atom("Na", Point::new(0., 0.));
    let chloride = doc.add_atom("Cl", Point::new(40., 0.));
    let product = doc.add_atom("O", Point::new(300., 0.));
    doc.atom_mut(sodium).unwrap().charge = 1;
    doc.atom_mut(chloride).unwrap().charge = -1;
    let arrow = arrow(&mut doc, 120., 0.);
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = vec![participant(vec![sodium, chloride])];
    reaction.products = vec![participant(vec![product])];
    doc.reactions.push(reaction);
    let xml = xml(&doc);
    let tree = Xml::parse(&xml).unwrap();
    let step = steps(&tree)[0];
    let reactants = ids(step, "ReactionStepReactants");
    assert_eq!(reactants.len(), 2);
    let elements: Vec<_> = reactants
        .iter()
        .map(|id| {
            target(&tree, id)
                .first_element_child()
                .unwrap()
                .attribute("Element")
                .unwrap()
        })
        .collect();
    assert_eq!(elements, ["11", "17"]);
    assert_eq!(ids(step, "ReactionStepProducts").len(), 1);
}

#[test]
fn a_fragment_in_two_assigned_roles_has_no_step() {
    for pair in [(0, 1), (0, 2), (1, 2)] {
        let mut doc = Document::default();
        let atoms = [("N", 0., 0.), ("O", 300., 0.), ("N", 170., -30.)]
            .map(|(element, x, y)| doc.add_atom(element, Point::new(x, y)));
        let hydrogen = doc.add_atom(
            "H",
            doc.atom(atoms[pair.0]).unwrap().position.offset(30., 0.),
        );
        doc.add_bond(atoms[pair.0], hydrogen, 1, "plain");
        doc.add_bond(hydrogen, atoms[pair.1], 0, "dotted");
        let mut roles = atoms.map(|atom| participant(vec![atom]));
        roles[pair.0].atoms.push(hydrogen);
        let [reactant, product, agent] = roles;
        let arrow = arrow(&mut doc, 120., 0.);
        let mut reaction = Reaction::new(arrow);
        reaction.reactants = vec![reactant];
        reaction.products = vec![product];
        reaction.agents = vec![agent];
        doc.reactions.push(reaction);
        let xml = xml(&doc);
        assert!(
            steps(&Xml::parse(&xml).unwrap()).is_empty(),
            "roles {pair:?}"
        );
    }
}

#[test]
fn a_fragment_with_an_unassigned_atom_omits_only_its_step() {
    let mut doc = Document::default();
    let nitrogen = doc.add_atom("N", Point::new(0., 0.));
    let hydrogen = doc.add_atom("H", Point::new(42., 0.));
    let oxygen = doc.add_atom("O", Point::new(42., 60.));
    let fluorine = doc.add_atom("F", Point::new(300., 0.));
    let chlorine = doc.add_atom("Cl", Point::new(600., 0.));
    doc.add_bond(nitrogen, hydrogen, 1, "plain");
    for (x, reactants, product) in [
        (120., vec![nitrogen, hydrogen], fluorine),
        (420., vec![fluorine], chlorine),
    ] {
        let mut reaction = Reaction::new(arrow(&mut doc, x, 0.));
        reaction.reactants = vec![participant(reactants)];
        reaction.products = vec![participant(vec![product])];
        doc.reactions.push(reaction);
    }
    assert!(!has_unwritten_reactions(&doc));
    assert_eq!(steps(&Xml::parse(&xml(&doc)).unwrap()).len(), 2);

    doc.add_bond(hydrogen, oxygen, 0, "dotted");
    assert!(has_unwritten_reactions(&doc));
    let xml = xml(&doc);
    let tree = Xml::parse(&xml).unwrap();
    let steps = steps(&tree);
    assert_eq!(steps.len(), 1);
    let product = target(&tree, ids(steps[0], "ReactionStepProducts")[0]);
    assert_eq!(
        product.first_element_child().unwrap().attribute("Element"),
        Some("17")
    );
}

#[test]
fn an_explicit_draft_with_an_empty_side_has_no_step() {
    for empty_reactants in [false, true] {
        let mut doc = Document::default();
        let atom = doc.add_atom("C", Point::new(0., 0.));
        let arrow = arrow(&mut doc, 100., 0.);
        let mut reaction = Reaction::new(arrow);
        if empty_reactants {
            reaction.products.push(participant(vec![atom]));
        } else {
            reaction.reactants.push(participant(vec![atom]));
        }
        doc.reactions.push(reaction);
        let xml = xml(&doc);
        assert!(steps(&Xml::parse(&xml).unwrap()).is_empty());
    }
}

#[test]
fn left_arrow_conditions_use_visible_agent_geometry_and_drawn_sides() {
    let mut doc = Document::default();
    let reactant = doc.add_atom("C", Point::new(300., 0.));
    let product = doc.add_atom("O", Point::new(0., 0.));
    let catalyst = [(-25., 170.), (120., 170.), (180., 210.)]
        .map(|(y, x)| doc.add_atom("C", Point::new(x, y)));
    doc.add_bond(catalyst[0], catalyst[1], 1, "plain");
    doc.add_bond(catalyst[1], catalyst[2], 1, "plain");
    doc.contract(&catalyst, "Cat", "").unwrap();
    let arrow_id = arrow(&mut doc, 120., 0.);
    let arrow = doc.arrows.last_mut().unwrap();
    std::mem::swap(&mut arrow.start, &mut arrow.end);
    let heat = text(&mut doc, "heat", 150., -50.);
    let linked = text(&mut doc, "linked", 150., -80.);
    doc.arrow_labels.push(ArrowLabel {
        arrow: arrow_id,
        annotation: linked,
        side: Side::Below,
        gap: None,
    });
    let mut reaction = Reaction::new(arrow_id);
    reaction.reactants = vec![participant(vec![reactant])];
    reaction.products = vec![participant(vec![product])];
    reaction.agents = vec![participant(catalyst.to_vec())];
    reaction.annotations = vec![heat];
    doc.reactions.push(reaction);
    let xml = xml(&doc);
    let tree = Xml::parse(&xml).unwrap();
    let step = steps(&tree)[0];
    // A label linked below but drawn above counts as drawn, as a reopened
    // file reads it.
    let above = ids(step, "ReactionStepObjectsAboveArrow");
    assert_eq!(above.len(), 3);
    assert!(target(&tree, above[0]).has_tag_name("fragment"));
    for (id, condition) in above[1..].iter().zip(["heat", "linked"]) {
        assert_eq!(
            target(&tree, id).first_element_child().unwrap().text(),
            Some(condition)
        );
    }
    assert!(ids(step, "ReactionStepObjectsBelowArrow").is_empty());
}

#[test]
fn an_agent_fragment_is_listed_once_on_the_first_assigned_side() {
    let mut doc = Document::default();
    let reactant = doc.add_atom("C", Point::new(0., 0.));
    let product = doc.add_atom("O", Point::new(300., 0.));
    let above = doc.add_atom("N", Point::new(170., -25.));
    let below = doc.add_atom("N", Point::new(170., 25.));
    let hydrogen = doc.add_atom("H", Point::new(200., -25.));
    doc.add_bond(above, hydrogen, 1, "plain");
    doc.add_bond(hydrogen, below, 0, "dotted");
    let arrow = arrow(&mut doc, 120., 0.);
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = vec![participant(vec![reactant])];
    reaction.products = vec![participant(vec![product])];
    reaction.agents = vec![participant(vec![above, hydrogen]), participant(vec![below])];
    doc.reactions.push(reaction);
    let xml = xml(&doc);
    let tree = Xml::parse(&xml).unwrap();
    let step = steps(&tree)[0];
    assert_eq!(ids(step, "ReactionStepObjectsAboveArrow").len(), 1);
    assert!(ids(step, "ReactionStepObjectsBelowArrow").is_empty());
}

#[test]
fn explicit_roles_keep_graphic_plusses_that_native_symbols_can_preserve() {
    for (axis_x, axis_y, pattern, native) in [
        ((20., 0.), (0., 20.), LinePattern::Solid, true),
        ((20., 0.), (0., 40.), LinePattern::Solid, false),
        ((20., 0.), (12., 16.), LinePattern::Solid, false),
        ((12., 16.), (-16., 12.), LinePattern::Solid, false),
        ((-20., 0.), (0., -20.), LinePattern::Solid, false),
        ((20., 0.), (0., 20.), LinePattern::Dashed, false),
    ] {
        let mut doc = Document::default();
        let first = doc.add_atom("C", Point::new(0., 0.));
        let second = doc.add_atom("O", Point::new(80., 0.));
        let product = doc.add_atom("N", Point::new(300., 0.));
        let arrow = arrow(&mut doc, 140., 0.);
        let mut plus = Graphic::dragged(
            doc.next_id(),
            GraphicKind::Symbol(SymbolKind::Plus),
            Point::new(40., 0.),
            Point::new(60., 0.),
            Default::default(),
            Default::default(),
            false,
        );
        plus.axis_x = Point::new(axis_x.0, axis_x.1);
        plus.axis_y = Point::new(axis_y.0, axis_y.1);
        plus.style.pattern = pattern;
        let parts = HashMap::from([(plus.id, plus.parts())]);
        doc.graphics.push(plus);
        let options = || Options {
            graphic_parts: Some(&parts),
            ..Default::default()
        };
        let inferred = write(&doc, options()).unwrap();
        let mut reaction = Reaction::new(arrow);
        reaction.reactants = vec![participant(vec![first]), participant(vec![second])];
        reaction.products = vec![participant(vec![product])];
        doc.reactions.push(reaction);
        let explicit = write(&doc, options()).unwrap();
        let tree = Xml::parse(&explicit).unwrap();
        let step = steps(&tree)[0];
        if native {
            let plusses = ids(step, "ReactionStepPlusses");
            assert_eq!(plusses.len(), 1);
            assert_eq!(
                target(&tree, plusses[0]).attribute("SymbolType"),
                Some("Plus")
            );
        } else {
            assert!(ids(step, "ReactionStepPlusses").is_empty());
            assert!(
                !tree
                    .descendants()
                    .any(|n| n.attribute("SymbolType") == Some("Plus"))
            );
            let curves = |xml: &str| {
                Xml::parse(xml)
                    .unwrap()
                    .descendants()
                    .filter(|n| n.has_tag_name("curve"))
                    .map(|n| {
                        (
                            n.attribute("CurvePoints").unwrap().to_string(),
                            n.attribute("CurveType").unwrap().to_string(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert!(!curves(&explicit).is_empty());
            assert_eq!(curves(&explicit), curves(&inferred));
        }
    }
}

#[test]
fn one_reaction_on_a_sheet_with_eight_thousand_reference_atoms_exports() {
    let mut doc = Document::default();
    doc.add_atom("C", Point::new(0., 0.));
    doc.add_atom("O", Point::new(300., 0.));
    let mut reference = doc.atoms[0].clone();
    reference.purpose = crate::purpose::Purpose::Reference;
    for index in 0..8_000 {
        reference.id = index + 3;
        reference.position = Point::new(
            (index % 100) as f32 * 30.,
            1000. + (index / 100) as f32 * 30.,
        );
        doc.atoms.push(reference.clone());
    }
    arrow(&mut doc, 100., 0.);
    let xml = xml(&doc);
    let tree = Xml::parse(&xml).unwrap();
    let step = steps(&tree)[0];
    assert_eq!(ids(step, "ReactionStepReactants").len(), 1);
    assert_eq!(ids(step, "ReactionStepProducts").len(), 1);
    assert_eq!(
        tree.descendants()
            .filter(|n| n.has_tag_name("fragment"))
            .count(),
        8_002
    );
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
