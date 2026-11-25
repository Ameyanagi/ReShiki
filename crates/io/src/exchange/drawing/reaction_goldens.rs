//! Reproducible editable reaction drawings for before/after reader checks.
//!
//! Keep this module on APIs that also exist on `origin/main`: copying it there
//! and setting `RESHIKI_UPDATE_REACTION_GOLDENS=1` produces the baseline files.
use crate::{
    arrow_labels::{ArrowLabel, Side},
    arrows::{ArrowStyle, Preset},
    document::{Annotation, Arrow, Document, Point},
    reactions::{self, Participant, Reaction},
    typography::TextFormat,
};
use std::{fs, path::PathBuf};

const UPDATE: &str = "RESHIKI_UPDATE_REACTION_GOLDENS";
// A 42-unit bond at 30 degrees, matching the default drawing bond length.
const DX: f32 = 36.373_066;

fn molecule(
    document: &mut Document,
    origin: Point,
    atoms: &[(&str, f32, f32)],
    bonds: &[(usize, usize, u8)],
) -> Vec<u64> {
    let ids: Vec<_> = atoms
        .iter()
        .map(|&(element, x, y)| document.add_atom(element, origin.offset(x, y)))
        .collect();
    for &(a, b, order) in bonds {
        document.add_bond(ids[a], ids[b], order, "plain");
    }
    ids
}

fn ethanol(document: &mut Document, x: f32) -> Vec<u64> {
    molecule(
        document,
        Point::new(x, 0.),
        &[("C", 0., 0.), ("C", DX, -21.), ("O", 2. * DX, 0.)],
        &[(0, 1, 1), (1, 2, 1)],
    )
}

fn acetic_acid(document: &mut Document, x: f32) -> Vec<u64> {
    molecule(
        document,
        Point::new(x, 0.),
        &[
            ("C", 0., 0.),
            ("C", DX, -21.),
            ("O", DX, -63.),
            ("O", 2. * DX, 0.),
        ],
        &[(0, 1, 1), (1, 2, 2), (1, 3, 1)],
    )
}

fn ethyl_acetate(document: &mut Document, x: f32) -> Vec<u64> {
    molecule(
        document,
        Point::new(x, 0.),
        &[
            ("C", 0., 0.),
            ("C", DX, -21.),
            ("O", DX, -63.),
            ("O", 2. * DX, 0.),
            ("C", 3. * DX, -21.),
            ("C", 4. * DX, 0.),
        ],
        &[(0, 1, 1), (1, 2, 2), (1, 3, 1), (3, 4, 1), (4, 5, 1)],
    )
}

fn forward_arrow(document: &mut Document, start: f32, end: f32) -> u64 {
    let id = document.next_id();
    document.arrows.push(Arrow::new(
        id,
        Point::new(start, 0.),
        Point::new(end, 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    id
}

fn text(document: &mut Document, position: Point, value: &str) -> u64 {
    let id = document.next_id();
    document.annotations.push(Annotation {
        id,
        position,
        text: value.into(),
        format: TextFormat {
            style: document.drawing_style.text_style(),
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

fn simple_document() -> Document {
    let mut document = Document::default();
    for (x, aromatic) in [(42., true), (350., false)] {
        let atoms: Vec<_> = [
            (0., -42.),
            (DX, -21.),
            (DX, 21.),
            (0., 42.),
            (-DX, 21.),
            (-DX, -21.),
        ]
        .into_iter()
        .map(|(dx, y)| document.add_atom("C", Point::new(x + dx, y)))
        .collect();
        for index in 0..6 {
            let order = if aromatic && index % 2 == 0 { 2 } else { 1 };
            document.add_bond(atoms[index], atoms[(index + 1) % 6], order, "plain");
        }
    }
    forward_arrow(&mut document, 130., 260.);
    document
}

/// Explicit ethanol + acetic acid -> ethyl acetate, with a visible plus sign.
fn esterification_document() -> Document {
    let mut document = Document::default();
    let alcohol = ethanol(&mut document, 0.);
    let acid = acetic_acid(&mut document, 160.);
    let ester = ethyl_acetate(&mut document, 510.);
    let arrow = forward_arrow(&mut document, 290., 450.);
    let plus = text(&mut document, Point::new(113., -16.), "+");
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = vec![participant(alcohol), participant(acid)];
    reaction.products = vec![participant(ester)];
    reaction.annotations.push(plus);
    document.reactions.push(reaction);
    document
}

fn multi_step_document() -> Document {
    let mut document = Document::default();
    ethanol(&mut document, 0.);
    molecule(
        &mut document,
        Point::new(310., 0.),
        &[("C", 0., 0.), ("C", DX, -21.), ("O", 2. * DX, 0.)],
        &[(0, 1, 1), (1, 2, 2)],
    );
    acetic_acid(&mut document, 620.);
    forward_arrow(&mut document, 130., 250.);
    forward_arrow(&mut document, 440., 560.);
    document
}

fn conditions_document() -> Document {
    let mut document = esterification_document();
    let catalyst = molecule(
        &mut document,
        Point::new(370., -100.),
        &[
            ("S", 0., 0.),
            ("O", 0., -42.),
            ("O", 0., 42.),
            ("O", -42., 0.),
            ("O", 42., 0.),
        ],
        &[(0, 1, 2), (0, 2, 2), (0, 3, 1), (0, 4, 1)],
    );
    document.reactions[0].agents.push(participant(catalyst));
    let annotation = text(&mut document, Point::new(334., 30.), "reflux");
    document.arrow_labels.push(ArrowLabel {
        arrow: document.reactions[0].arrow,
        annotation,
        side: Side::Below,
        gap: None,
    });
    document
}

fn mapped_document() -> Document {
    let mut document = esterification_document();
    // Ethanol: maps 1,2,3. Acetic acid: maps 4,5,6,7. The ester retains
    // ethanol's oxygen (3); acid hydroxyl oxygen (7) leaves in omitted water.
    let reaction = &document.reactions[0];
    let assignments: Vec<_> = reaction.reactants[0]
        .atoms
        .iter()
        .copied()
        .zip([1, 2, 3])
        .chain(
            reaction.reactants[1]
                .atoms
                .iter()
                .copied()
                .zip([4, 5, 6, 7]),
        )
        .chain(
            reaction.products[0]
                .atoms
                .iter()
                .copied()
                .zip([4, 5, 6, 3, 2, 1]),
        )
        .collect();
    for (id, map) in assignments {
        document.atom_mut(id).unwrap().map_num = map;
    }
    document
}

fn golden(name: &str, document: Document, expected_steps: &[(usize, usize, usize, usize)]) {
    document.validate().unwrap();
    assert_eq!(document.arrows.len(), expected_steps.len(), "{name}");
    for (arrow, &(reactants, products, agents, conditions)) in
        document.arrows.iter().zip(expected_steps)
    {
        let reaction = reactions::preview(&document, arrow.id).unwrap();
        assert_eq!(
            (
                reaction.reactants.len(),
                reaction.products.len(),
                reaction.agents.len(),
                reaction.annotations.len(),
            ),
            (reactants, products, agents, conditions),
            "{name}: source reaction participants"
        );
    }
    let xml = crate::exchange::drawing::write(&document, Default::default()).unwrap();
    let cdx = crate::exchange::to_cdx(&xml).unwrap();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reaction_steps");
    let update = std::env::var(UPDATE).as_deref() == Ok("1");
    if update {
        fs::create_dir_all(&directory).unwrap();
    }
    for (extension, actual) in [("cdxml", xml.as_bytes()), ("cdx", cdx.as_slice())] {
        let path = directory.join(format!("{name}.{extension}"));
        if update {
            fs::write(&path, actual).unwrap();
        }
        let expected = fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "Cannot read {}: {error}; regenerate with {UPDATE}=1 cargo test -p reshiki-io reaction_goldens",
                path.display()
            )
        });
        let first_difference = actual
            .iter()
            .zip(&expected)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or(actual.len().min(expected.len()));
        assert!(
            actual == expected,
            "{name}.{extension} differs at byte {first_difference} (actual {} bytes, golden {} bytes); regenerate with {UPDATE}=1 cargo test -p reshiki-io reaction_goldens",
            actual.len(),
            expected.len()
        );
    }
}

#[test]
fn simple() {
    golden("simple", simple_document(), &[(1, 1, 0, 0)]);
}

#[test]
fn multi_reactant() {
    golden("multi_reactant", esterification_document(), &[(2, 1, 0, 1)]);
}

#[test]
fn multi_step() {
    golden(
        "multi_step",
        multi_step_document(),
        &[(1, 1, 0, 0), (1, 1, 0, 0)],
    );
}

#[test]
fn conditions() {
    golden("conditions", conditions_document(), &[(2, 1, 1, 2)]);
}

#[test]
fn mapped() {
    golden("mapped", mapped_document(), &[(2, 1, 0, 1)]);
}
