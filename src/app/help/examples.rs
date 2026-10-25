//! Before/after drawings for Help, made by running the shortcut's own edit
//! on a small fixture, so the preview cannot drift from the behavior.
use super::registry::Example;
use reshiki::{
    bonds::DoublePosition,
    document::{Document, Point},
    hotkeys,
    palette::{Color, Hue, Row},
    style::DEFAULT as STYLE,
};

/// Marks the atom or bond the key acts on, like the keyboard hotspot.
const TARGET: Color = Color::Palette(Hue::Amber, Row::Tint);

/// Zigzag butane, or but-2-ene, with ids in drawing order.
fn chain(double: bool) -> (Document, [u64; 4]) {
    let length = STYLE.bond_length_world;
    let (dx, dy) = (length * 0.866, length * 0.5);
    let mut doc = Document::default();
    let ids = [0., 1., 2., 3.].map(|i: f32| {
        let y = if i as u8 % 2 == 1 { -dy } else { 0. };
        doc.add_atom("C", Point::new(i * dx, y))
    });
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[1], ids[2], if double { 2 } else { 1 }, "plain");
    doc.add_bond(ids[2], ids[3], 1, "plain");
    (doc, ids)
}

/// The fixture with its target marked, and the result of the key.
pub(crate) fn render(example: Example) -> Result<(Document, Document), String> {
    let length = STYLE.bond_length_world;
    match example {
        Example::Atom(key) => {
            let (doc, ids) = chain(false);
            let target = ids[3];
            let after = match hotkeys::atom_edit(&doc, target, key, length) {
                Some(result) => result?.0,
                None => {
                    hotkeys::ring_edit(&doc, Some(target), None, key, length)
                        .ok_or("No edit for this key")??
                        .0
                }
            };
            let mut before = doc;
            if let Some(atom) = before.atom_mut(target) {
                atom.display.highlight = Some(TARGET);
            }
            Ok((before, after))
        }
        Example::Bond(key) => {
            let position = match key {
                "l" => Some(DoublePosition::Left),
                "c" => Some(DoublePosition::Center),
                "r" => Some(DoublePosition::Right),
                _ => None,
            };
            let (doc, a, b) = if matches!(
                key,
                "v" | "4" | "5" | "6" | "7" | "8" | "a" | "z" | "9" | "0"
            ) {
                let mut doc = reshiki::rings::Preset::Benzene.document(length, false);
                // Keep an eligible double edge vertical so the product fuses
                // horizontally and remains readable in the short preview pane.
                let (sin, cos) = std::f32::consts::FRAC_PI_3.sin_cos();
                for atom in &mut doc.atoms {
                    let p = atom.position;
                    atom.position = Point::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
                }
                let bond = doc.bonds.first().ok_or("Benzene fixture has no bond")?;
                let (a, b) = (bond.a, bond.b);
                (doc, a, b)
            } else {
                let (doc, ids) = chain(key == "1" || position.is_some());
                (doc, ids[1], ids[2])
            };
            let after = if let Some(position) = position {
                let mut after = doc.clone();
                for bond in &mut after.bonds {
                    if (bond.a, bond.b) == (a, b) {
                        bond.double_position = position;
                    }
                }
                after
            } else if let Some(preset) = hotkeys::bond_preset(key) {
                hotkeys::bond_edit(&doc, a, b, preset)?
            } else {
                hotkeys::ring_edit(&doc, None, Some((a, b)), key, length)
                    .ok_or("No edit for this key")??
                    .0
            };
            let mut before = doc;
            for bond in &mut before.bonds {
                if (bond.a, bond.b) == (a, b) {
                    bond.highlight = Some(TARGET);
                }
            }
            Ok((before, after))
        }
    }
}
