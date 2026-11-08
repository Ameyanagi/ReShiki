use super::*;
use crate::{chemistry::kekulize::Direction, document::Point};

// RDKit's AtropManyChirals.cxsmi (#1056) with its N-vinyl axis N8-C9 drawn
// by an N8 wedge and C11 wedged away from axis end C9, at the given atom
// positions (world units).
fn molecule(positions: [(f32, f32); 20]) -> anyhow::Result<molecular::Molecule> {
    let mut doc = Document::default();
    for (element, (x, y)) in [
        "O", "C", "C", "C", "N", "C", "O", "N", "C", "C", "C", "C", "C", "Br", "C", "F", "C", "C",
        "Cl", "C",
    ]
    .into_iter()
    .zip(positions)
    {
        doc.add_atom(element, Point::new(x, y));
    }
    for (a, b, order, display) in [
        (1, 2, 2, "plain"),
        (2, 3, 1, "plain"),
        (3, 4, 2, "plain"),
        (4, 5, 1, "plain"),
        (5, 6, 1, "plain"),
        (6, 7, 2, "plain"),
        (6, 8, 1, "plain"),
        (8, 9, 1, "plain"),
        (9, 10, 2, "plain"),
        (11, 9, 1, "plain"),
        (11, 12, 1, "wedge"),
        (12, 13, 1, "hash"),
        (12, 14, 1, "plain"),
        (11, 15, 1, "plain"),
        (15, 16, 1, "plain"),
        (15, 17, 1, "wedge"),
        (11, 18, 1, "plain"),
        (18, 19, 1, "plain"),
        (18, 20, 1, "wedge"),
        (8, 2, 1, "wedge"),
    ] {
        doc.add_bond(a, b, order, display);
    }
    Ok(molecular::prepare(&doc)?)
}

const MACOS: [(f32, f32); 20] = [
    (-61.08, -75.62),
    (-69.01, -34.38),
    (-108.70, -20.63),
    (-116.63, 20.62),
    (-84.88, 48.11),
    (-45.20, 34.36),
    (-29.66, 50.39),
    (-37.26, -6.88),
    (1.07, -24.04),
    (1.72, -66.04),
    (37.12, -2.48),
    (58.68, -38.53),
    (38.24, -75.22),
    (100.67, -37.88),
    (73.16, 19.08),
    (109.86, -1.36),
    (72.51, 61.07),
    (18.78, 35.30),
    (-1.74, 44.07),
    (42.34, 70.07),
];

#[test]
fn fresh_wedges_that_hide_an_axis_are_refused() -> anyhow::Result<()> {
    let drawn = molecule(MACOS)?;
    assert_eq!(axes(&drawn)?.len(), 1);
    // A fresh import has no wedges yet. In the macOS/Windows layout the
    // reference wedges C11 towards C9, which hides the axis.
    let mut fresh = drawn.clone();
    fresh.state.directions.fill(Direction::None);
    let labels = vec![None; fresh.ids.len()];
    let error = finish_molecule(fresh.clone(), &labels, Default::default())
        .err()
        .ok_or_else(|| anyhow::anyhow!("Hidden axis accepted"))?;
    assert!(error.to_string().contains("atropisomer axis"), "{error}");
    // In the Linux layout the reference wedges keep it.
    fresh.positions = molecule([
        (73.36, -69.67),
        (73.36, -27.67),
        (109.73, -6.67),
        (109.73, 35.33),
        (73.36, 56.33),
        (36.99, 35.33),
        (0.61, 56.33),
        (36.99, -6.67),
        (0.61, -27.67),
        (0.61, -69.67),
        (-35.76, -6.67),
        (-14.76, 29.70),
        (14.95, 37.11),
        (-35.76, 66.08),
        (-72.13, 14.33),
        (-72.13, 56.33),
        (-108.50, -6.67),
        (-56.76, -43.04),
        (-35.76, -79.42),
        (-98.76, -43.04),
    ])?
    .positions;
    let prepared = finish_molecule(fresh, &labels, Default::default())?;
    assert_eq!(
        axes(&molecular::prepare(&prepared.document)?)?,
        axes(&drawn)?
    );
    Ok(())
}
