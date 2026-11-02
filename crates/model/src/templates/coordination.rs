//! Depiction-only coordination frames; projected wedges do not assign R/S.
use super::{Anchor, Template};
use crate::document::{Bond, Document, Point};

pub(super) fn templates() -> Result<Vec<Template>, String> {
    let x = 3_f32.sqrt() / 2.;
    let square = [
        (x, 0.5, "wedge"),
        (-x, 0.5, "wedge"),
        (x, -0.5, "hash"),
        (-x, -0.5, "hash"),
    ];
    let trigonal = [
        (0., -1., "plain"),
        (0., 1., "plain"),
        (-1., 0., "plain"),
        (x, 0.5, "wedge"),
        (x, -0.5, "hash"),
    ];
    let mut octahedral = square.to_vec();
    octahedral.extend([(0., -1., "plain"), (0., 1., "plain")]);
    [
        (
            "Square planar ML₄ (SP-4)",
            "Pt",
            "C[Pt](C)(C)C",
            ["square planar", "SP-4"],
            square.as_slice(),
        ),
        (
            "Trigonal bipyramidal ML₅ (TBPY-5)",
            "Fe",
            "C[Fe](C)(C)(C)C",
            ["trigonal bipyramidal", "TBPY-5"],
            trigonal.as_slice(),
        ),
        (
            "Octahedral ML₆ (OC-6)",
            "Co",
            "C[Co](C)(C)(C)(C)C",
            ["octahedral", "OC-6"],
            octahedral.as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, element, smiles, keywords, rays)| {
        let mut document = Document::default();
        let metal = document.add_atom(element, Point::default());
        let length = crate::style::DEFAULT.bond_length_world;
        for &(x, y, display) in rays {
            let ligand = document.add_atom("C", Point::new(x * length, y * length));
            let mut bond = Bond::plain(metal, ligand, 1, display);
            bond.projection = display != "plain";
            document.bonds.push(bond);
        }
        document.validate()?;
        Ok(Template {
            id: String::new(),
            group: "Coordination geometry".into(),
            name: name.into(),
            smiles: smiles.into(),
            keywords: keywords
                .into_iter()
                .chain(["coordination", "metal", "frame"])
                .map(str::to_owned)
                .collect(),
            note: "Depiction-only frame: wedges show projection and assign no tetrahedral R/S. Share an atom onto a metal to apply it; relabel the ligand ends.".into(),
            document,
            anchor: Anchor::Atom(metal),
        })
    })
    .collect()
}

#[cfg(test)]
mod tests;
