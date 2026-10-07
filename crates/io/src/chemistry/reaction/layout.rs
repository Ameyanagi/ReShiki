//! Stage reaction participants, laying out only those without input coordinates.
use super::{Drawing, Error, Imported, SmilesReaction, document, invalid, molfile};
use crate::{
    chemistry::{RDKIT_VERSION, stereo::Point3},
    reactions::Role,
};

struct Part {
    source: molfile::Imported,
    role: Role,
    pending: bool,
    attachments: Vec<bool>,
    properties: Vec<Vec<(Vec<u8>, Vec<u8>)>>,
}

pub(crate) struct Layout {
    parts: Vec<Part>,
}

#[derive(serde::Serialize)]
pub(crate) struct Request<'a> {
    molecule: &'a document::Molecule,
    file: &'a molfile::FileAnnotations,
    atom_properties: &'a [Vec<(Vec<u8>, Vec<u8>)>],
}
impl<'a> Request<'a> {
    pub(crate) fn molecule(&self) -> &'a document::Molecule {
        self.molecule
    }
    pub(crate) fn atom_properties(&self) -> &'a [Vec<(Vec<u8>, Vec<u8>)>] {
        self.atom_properties
    }
}

impl SmilesReaction {
    pub(crate) fn layout(self) -> Result<Layout, Error> {
        let mut parts = Vec::new();
        let mut next_id = 1u64;
        for (role, row) in [
            (Role::Reactant, self.reactants),
            (Role::Agent, self.agents),
            (Role::Product, self.products),
        ] {
            for part in row {
                let n = part.prepared.state.graph.atoms.len();
                let end = next_id
                    .checked_add(u64::try_from(n).map_err(|_| Error::Limit)?)
                    .ok_or(Error::Limit)?;
                let conformer = part.conformers.into_iter().next();
                let pending = conformer.is_none();
                let is_3d = conformer.as_ref().is_some_and(|c| c.is_3d);
                let source = molfile::Imported {
                    molecule: document::Molecule {
                        rdkit_version: RDKIT_VERSION,
                        ids: (next_id..end).collect(),
                        positions: conformer
                            .map_or_else(|| vec![Point3::default(); n], |c| c.positions),
                        state: part.prepared.state,
                    },
                    annotations: molfile::FileAnnotations {
                        chemdraw_directions: Vec::new(),
                        attachments: Vec::new(),
                        is_3d,
                        attachment_points: vec![None; n],
                        dummy_labels: part.prepared.dummy_labels,
                    },
                };
                document::validate_molecule(&source.molecule)?;
                next_id = end;
                parts.push(Part {
                    source,
                    role,
                    pending,
                    attachments: part.reaction_attachments,
                    properties: part.reaction_properties,
                });
            }
        }
        Ok(Layout { parts })
    }
}
impl Layout {
    pub(crate) fn requests(&self) -> impl Iterator<Item = Request<'_>> {
        self.parts.iter().filter(|p| p.pending).map(|p| Request {
            molecule: &p.source.molecule,
            file: &p.source.annotations,
            atom_properties: &p.properties,
        })
    }

    pub(crate) fn finish(self, positions: Vec<Vec<Point3>>) -> Result<Drawing, Error> {
        let mut coordinates = positions.into_iter();
        let mut source = Imported {
            reactants: Vec::new(),
            products: Vec::new(),
            agents: Vec::new(),
        };
        let mut attachments = Vec::with_capacity(self.parts.len());
        for mut part in self.parts {
            if part.pending {
                let positions = coordinates
                    .next()
                    .ok_or_else(|| invalid("Missing reaction layout"))?;
                if positions.len() != part.source.molecule.ids.len() {
                    return Err(invalid("Reaction layout atom count changed"));
                }
                part.source.molecule.positions = positions;
            }
            let row = match part.role {
                Role::Reactant => &mut source.reactants,
                Role::Product => &mut source.products,
                Role::Agent => &mut source.agents,
            };
            row.push(part.source);
            attachments.push(part.attachments);
        }
        if coordinates.next().is_some() {
            return Err(invalid("Unexpected reaction layout"));
        }
        source.drawing_with_attachments(Some(&attachments))
    }
}

#[cfg(test)]
mod tests;
