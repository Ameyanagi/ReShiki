use crate::document::{Document, Point};
use crate::generic::{
    Alternative, AlternativeSite, Crossing, LinkNode, RepeatCount, RepeatPattern, RepeatUnit,
};
use crate::graphics::{BracketSides, Graphic, GraphicKind};

pub(super) type InvalidCase = (&'static str, fn(&mut Document), &'static str);

pub(super) fn chain(elements: &[&str]) -> Document {
    let mut doc = Document::default();
    let mut previous = None;
    for (index, element) in elements.iter().enumerate() {
        let id = doc.add_atom(element, Point::new(index as f32 * 42., 0.));
        if let Some(previous) = previous {
            doc.add_bond(previous, id, 1, "plain");
        }
        previous = Some(id);
    }
    doc
}

pub(super) fn element(symbol: &str, isotope: u32) -> Alternative {
    Alternative::Element {
        symbol: symbol.into(),
        isotope,
    }
}

pub(super) fn bracket(doc: &mut Document) -> u64 {
    let id = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Brackets,
        Point::new(20., -30.),
        Point::new(104., 30.),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    id
}

pub(super) fn alternative_document() -> Document {
    let mut doc = chain(&["*", "C", "C"]);
    doc.generic.alternatives.push(AlternativeSite {
        atom: 1,
        choices: vec![element("Cl", 0), element("Br", 0)],
        excluded: false,
    });
    doc
}

pub(super) fn link_document() -> Document {
    let mut doc = chain(&["C", "C", "C"]);
    doc.generic.links.push(LinkNode {
        atom: 2,
        min: 1,
        max: 3,
    });
    doc.atom_mut(2).unwrap().display.variable = Some("(CH2)1-3".into());
    doc
}

pub(super) fn repeat_document() -> Document {
    let mut doc = chain(&["O", "C", "C", "N"]);
    let primary = bracket(&mut doc);
    let additional = bracket(&mut doc);
    doc.generic.repeats.push(RepeatUnit {
        bracket: Some(primary),
        bracket_objects: vec![additional],
        members: vec![2, 3],
        crossings: vec![
            Crossing {
                inside: 2,
                outside: 1,
            },
            Crossing {
                inside: 3,
                outside: 4,
            },
        ],
        crossing_brackets: vec![primary, additional],
        head_count: Some(1),
        usage: "SRU".into(),
        pattern: RepeatPattern::HeadToTail,
        count: RepeatCount::Finite { min: 1, max: 3 },
        label: "1-3".into(),
        flip: "Unspecified".into(),
    });
    doc
}
