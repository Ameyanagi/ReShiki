//! Native aromatic ligand presentation, verified against desktop CDX output.
use super::*;
use crate::document::Atom;

pub(super) fn hidden_charge(atom: &Atom) -> bool {
    atom.display.hide_charge
        && atom.charge.unsigned_abs() == 1
        && atom.element == "C"
        && atom.aromatic
        && atom.marks.is_empty()
}

// Explicit NumHydrogens forces an atom label in external readers. These
// ordinary aromatic CH vertices already carry their H through the ring bonds.
pub(super) fn implicit_carbon(atom: &Atom, doc: &Document) -> bool {
    if atom.element != "C"
        || !atom.aromatic
        || atom.explicit_h != 1
        || atom.isotope != 0
        || atom.radical_electrons != 0
        || !atom.marks.is_empty()
    {
        return false;
    }
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| b.a == atom.id || b.b == atom.id)
        .collect();
    bonds.len() == 2 && bonds.iter().all(|b| b.order == 4)
}

impl Writer<'_> {
    pub(super) fn hidden_charges(&mut self) -> Result<()> {
        for atom in self.doc.atoms.iter().filter(|a| hidden_charge(a)) {
            // Native symbol association depends on proximity as well as the
            // explicit owner reference. Keep the invisible mark beside its atom.
            let unit = real(self.doc.drawing_style.font_size_pt) / self.scale;
            let p = P::from(atom.position);
            let ring = self
                .doc
                .atoms
                .iter()
                .find(|a| a.centroid.contains(&atom.id));
            let away = ring.map(|ring| {
                self.doc
                    .bonds
                    .iter()
                    .find_map(|bond| {
                        let other = if bond.a == ring.id {
                            bond.b
                        } else if bond.b == ring.id {
                            bond.a
                        } else {
                            return None;
                        };
                        self.doc
                            .atom(other)
                            .filter(|a| !ring.centroid.contains(&a.id))
                            .map(|a| a.position)
                    })
                    .unwrap_or(ring.position)
            });
            let offset = away
                .map(|origin| {
                    let dx = p.x - real(origin.x);
                    let dy = p.y - real(origin.y);
                    let length = dx.hypot(dy).max(0.001);
                    (dx / length * unit * 0.3, dy / length * unit * 0.3)
                })
                .unwrap_or((0., -unit * 0.3));
            let center = p.add(offset.0, offset.1);
            let id = self.id()?;
            let mark = self.tree.add(
                Some(self.fragment),
                "graphic",
                [
                    ("id", id),
                    ("GraphicType", "Symbol".into()),
                    (
                        "SymbolType",
                        if atom.charge > 0 { "Plus" } else { "Minus" }.into(),
                    ),
                    ("Visible", "no".into()),
                    (
                        "BoundingBox",
                        format!(
                            "{} {}",
                            self.position(center.add(unit * 0.25, 0.)),
                            self.position(center.add(-unit * 0.25, 0.))
                        ),
                    ),
                ],
            )?;
            self.tree.add(
                Some(mark),
                "represent",
                [
                    ("attribute", "Charge".into()),
                    ("object", self.atom_xml(atom.id)?),
                ],
            )?;
        }
        Ok(())
    }

    pub(super) fn projected_circle(&mut self, circle: &crate::aromatic::Circle) -> Result<()> {
        for (points, closed) in super::objects::curve_points(&circle.graphic().commands())? {
            self.curve(
                self.fragment,
                &points,
                closed,
                circle.color,
                false,
                real(circle.width_pt),
                false,
                1,
            )?;
        }
        Ok(())
    }
}
