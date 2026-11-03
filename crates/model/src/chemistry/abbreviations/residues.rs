//! Two-sided amino-acid residues. These definitions deliberately stay out of
//! the one-sided replacement and automatic-contraction catalog.
use super::{AttachmentPolicy, Error, Result, check_size, invalid, matcher, validate_with_policy};
use crate::{
    abbreviations::Abbreviation,
    chemistry::{RDKIT_VERSION, document, smiles},
    document::{Atom, Document},
};
use std::collections::{HashMap, HashSet};

/// A standard L residue. Mapped dummies identify the N side (1) and C side (2).
#[derive(Debug, Clone, Copy)]
pub struct Residue {
    pub symbol: &'static str,
    pub name: &'static str,
    pub smiles: &'static str,
}

pub const RESIDUES: &[Residue] = &[
    Residue {
        symbol: "Ala",
        name: "alanine",
        smiles: "[*:1]N[C@@H](C)C(=O)[*:2]",
    },
    Residue {
        symbol: "Arg",
        name: "arginine",
        smiles: "[*:1]N[C@@H](CCCNC(=N)N)C(=O)[*:2]",
    },
    Residue {
        symbol: "Asn",
        name: "asparagine",
        smiles: "[*:1]N[C@@H](CC(=O)N)C(=O)[*:2]",
    },
    Residue {
        symbol: "Asp",
        name: "aspartic acid",
        smiles: "[*:1]N[C@@H](CC(=O)O)C(=O)[*:2]",
    },
    Residue {
        symbol: "Cys",
        name: "cysteine",
        smiles: "[*:1]N[C@@H](CS)C(=O)[*:2]",
    },
    Residue {
        symbol: "Gln",
        name: "glutamine",
        smiles: "[*:1]N[C@@H](CCC(=O)N)C(=O)[*:2]",
    },
    Residue {
        symbol: "Glu",
        name: "glutamic acid",
        smiles: "[*:1]N[C@@H](CCC(=O)O)C(=O)[*:2]",
    },
    Residue {
        symbol: "Gly",
        name: "glycine",
        smiles: "[*:1]NCC(=O)[*:2]",
    },
    Residue {
        symbol: "His",
        name: "histidine",
        smiles: "[*:1]N[C@@H](Cc1c[nH]cn1)C(=O)[*:2]",
    },
    Residue {
        symbol: "Ile",
        name: "isoleucine",
        smiles: "[*:1]N[C@@H]([C@@H](C)CC)C(=O)[*:2]",
    },
    Residue {
        symbol: "Leu",
        name: "leucine",
        smiles: "[*:1]N[C@@H](CC(C)C)C(=O)[*:2]",
    },
    Residue {
        symbol: "Lys",
        name: "lysine",
        smiles: "[*:1]N[C@@H](CCCCN)C(=O)[*:2]",
    },
    Residue {
        symbol: "Met",
        name: "methionine",
        smiles: "[*:1]N[C@@H](CCSC)C(=O)[*:2]",
    },
    Residue {
        symbol: "Phe",
        name: "phenylalanine",
        smiles: "[*:1]N[C@@H](Cc1ccccc1)C(=O)[*:2]",
    },
    Residue {
        symbol: "Pro",
        name: "proline",
        smiles: "[*:1]N1[C@@H](CCC1)C(=O)[*:2]",
    },
    Residue {
        symbol: "Ser",
        name: "serine",
        smiles: "[*:1]N[C@@H](CO)C(=O)[*:2]",
    },
    Residue {
        symbol: "Thr",
        name: "threonine",
        smiles: "[*:1]N[C@@H]([C@H](O)C)C(=O)[*:2]",
    },
    Residue {
        symbol: "Trp",
        name: "tryptophan",
        smiles: "[*:1]N[C@@H](Cc1c[nH]c2ccccc12)C(=O)[*:2]",
    },
    Residue {
        symbol: "Tyr",
        name: "tyrosine",
        smiles: "[*:1]N[C@@H](Cc1ccc(O)cc1)C(=O)[*:2]",
    },
    Residue {
        symbol: "Val",
        name: "valine",
        smiles: "[*:1]N[C@@H](C(C)C)C(=O)[*:2]",
    },
];

/// A label-specific definition, with every stereocenter mirrored for D-Xaa.
#[derive(Debug, Clone)]
pub struct ResidueDefinition {
    pub label: String,
    pub name: &'static str,
    pub smiles: String,
}

pub fn residue(label: &str) -> Option<ResidueDefinition> {
    let (symbol, mirror) = label
        .strip_prefix("D-")
        .map_or((label, false), |s| (s, true));
    let entry = RESIDUES.iter().find(|entry| entry.symbol == symbol)?;
    if mirror && symbol == "Gly" {
        return None;
    }
    let smiles = if mirror {
        // The table uses tetrahedral @ tags only. A sentinel keeps @@ and @
        // from being inverted twice, including the side-chain centers.
        entry
            .smiles
            .replace("@@", "\u{1}")
            .replace('@', "@@")
            .replace('\u{1}', "@")
    } else {
        entry.smiles.into()
    };
    Some(ResidueDefinition {
        label: label.into(),
        name: entry.name,
        smiles,
    })
}

/// Explicit Find labels; these do not become endpoint insertion spellings.
pub fn residue_labels() -> impl Iterator<Item = String> {
    RESIDUES.iter().flat_map(|entry| {
        std::iter::once(entry.symbol.into())
            .chain((entry.symbol != "Gly").then(|| format!("D-{}", entry.symbol)))
    })
}

struct Candidate {
    anchor: u64,
    exit: u64,
    members: Vec<u64>,
    boundary: [usize; 2],
}

struct Target<'a> {
    doc: &'a Document,
    atoms: HashMap<u64, &'a Atom>,
    adjacent: HashMap<u64, Vec<(u64, usize)>>,
}

impl<'a> Target<'a> {
    fn new(doc: &'a Document) -> Result<Self> {
        check_size(doc)?;
        let atoms: HashMap<_, _> = doc.atoms.iter().map(|atom| (atom.id, atom)).collect();
        if atoms.len() != doc.atoms.len() {
            return Err(invalid("Duplicate drawing atom"));
        }
        let mut adjacent: HashMap<u64, Vec<(u64, usize)>> = HashMap::new();
        for (index, bond) in doc.bonds.iter().enumerate() {
            if !atoms.contains_key(&bond.a) || !atoms.contains_key(&bond.b) {
                return Err(invalid("Missing drawing atom"));
            }
            adjacent.entry(bond.a).or_default().push((bond.b, index));
            adjacent.entry(bond.b).or_default().push((bond.a, index));
        }
        Ok(Self {
            doc,
            atoms,
            adjacent,
        })
    }

    fn neighbors(&self, atom: u64) -> impl Iterator<Item = (u64, usize)> + '_ {
        self.adjacent.get(&atom).into_iter().flatten().copied()
    }

    fn candidates(&self, work: &mut matcher::Work) -> Result<Vec<Candidate>> {
        let mut result = Vec::new();
        for alpha in &self.doc.atoms {
            work.spend(1)?;
            if alpha.element != "C" || alpha.aromatic {
                continue;
            }
            for (nitrogen, n_bond) in self.neighbors(alpha.id) {
                if self
                    .atoms
                    .get(&nitrogen)
                    .is_none_or(|atom| atom.element != "N")
                    || self
                        .doc
                        .bonds
                        .get(n_bond)
                        .is_none_or(|bond| bond.order != 1)
                {
                    continue;
                }
                for (carbonyl, c_bond) in self.neighbors(alpha.id) {
                    work.spend(1)?;
                    if self
                        .atoms
                        .get(&carbonyl)
                        .is_none_or(|atom| atom.element != "C")
                        || self
                            .doc
                            .bonds
                            .get(c_bond)
                            .is_none_or(|bond| bond.order != 1)
                    {
                        continue;
                    }
                    let oxygens: Vec<_> = self
                        .neighbors(carbonyl)
                        .filter(|&(other, bond)| {
                            self.atoms
                                .get(&other)
                                .is_some_and(|atom| atom.element == "O")
                                && self.doc.bonds.get(bond).is_some_and(|bond| bond.order == 2)
                        })
                        .collect();
                    let [(oxygen, _)] = oxygens.as_slice() else {
                        continue;
                    };
                    // The side chain is the component reached from C-alpha,
                    // stopping at the backbone N/C (also handles Pro's ring).
                    let mut members = HashSet::from([nitrogen, carbonyl, *oxygen, alpha.id]);
                    let mut pending = vec![alpha.id];
                    while let Some(id) = pending.pop() {
                        for (other, _) in self.neighbors(id) {
                            work.spend(1)?;
                            if members.insert(other) {
                                pending.push(other);
                            }
                        }
                    }
                    // An explicitly drawn amino hydrogen is part of the residue.
                    for (other, _) in self.neighbors(nitrogen) {
                        if self
                            .atoms
                            .get(&other)
                            .is_some_and(|atom| atom.element == "H")
                        {
                            members.insert(other);
                        }
                    }
                    let mut boundary = Vec::new();
                    for &id in &members {
                        for (other, bond) in self.neighbors(id) {
                            work.spend(1)?;
                            if !members.contains(&other) {
                                boundary.push((id, bond));
                            }
                        }
                    }
                    boundary.sort_unstable_by_key(|&(id, _)| id != nitrogen);
                    let [(n_id, n_bond), (c_id, c_bond)] = boundary.as_slice() else {
                        continue;
                    };
                    if *n_id != nitrogen
                        || *c_id != carbonyl
                        || boundary
                            .iter()
                            .any(|&(_, i)| self.doc.bonds.get(i).is_none_or(|bond| bond.order != 1))
                    {
                        continue;
                    }
                    let mut members: Vec<_> = members.into_iter().collect();
                    members.sort_unstable();
                    result.push(Candidate {
                        anchor: nitrogen,
                        exit: carbonyl,
                        members,
                        boundary: [*n_bond, *c_bond],
                    });
                }
            }
        }
        Ok(result)
    }

    fn canonical(&self, candidate: &Candidate) -> Result<String> {
        let members: HashSet<_> = candidate.members.iter().copied().collect();
        let mut part = Document {
            atoms: candidate
                .members
                .iter()
                .map(|id| {
                    self.atoms
                        .get(id)
                        .map(|atom| (**atom).clone())
                        .ok_or_else(|| invalid("Missing residue atom"))
                })
                .collect::<Result<Vec<_>>>()?,
            ..Document::default()
        };
        let mut bonds = HashSet::new();
        for &id in &candidate.members {
            bonds.extend(
                self.adjacent
                    .get(&id)
                    .into_iter()
                    .flatten()
                    .map(|&(_, index)| index),
            );
        }
        let mut bonds: Vec<_> = bonds.into_iter().collect();
        bonds.sort_unstable();
        part.bonds = bonds
            .into_iter()
            .filter_map(|i| self.doc.bonds.get(i))
            .filter(|bond| members.contains(&bond.a) && members.contains(&bond.b))
            .cloned()
            .collect();
        let mut next = part.object_ids().max().unwrap_or(0);
        for (side, (&boundary, attachment)) in candidate
            .boundary
            .iter()
            .zip([candidate.anchor, candidate.exit])
            .enumerate()
        {
            next = next
                .checked_add(1)
                .ok_or_else(|| invalid("Object ID limit exceeded"))?;
            let mut bond = super::at(&self.doc.bonds, boundary)?.clone();
            let outside = if bond.a == attachment { bond.b } else { bond.a };
            let position = self
                .atoms
                .get(&outside)
                .ok_or_else(|| invalid("Missing outside atom"))?
                .position;
            let dummy = part.add_atom("*", position);
            let atom = part
                .atom_mut(dummy)
                .ok_or_else(|| invalid("Missing dummy atom"))?;
            atom.id = next;
            atom.map_num = (side + 1) as u32;
            if bond.a == outside {
                bond.a = next;
            } else {
                bond.b = next;
            }
            bond.display = "plain".into();
            bond.stereo = None;
            bond.stereo_atoms.clear();
            if let Some(stereo) = part
                .atom_mut(attachment)
                .and_then(|atom| atom.stereo.as_mut())
            {
                for neighbor in &mut stereo.neighbors {
                    if *neighbor == outside {
                        *neighbor = next;
                    }
                }
            }
            part.bonds.push(bond);
        }
        let prepared = document::prepare(&part).map_err(|error| invalid(error.to_string()))?;
        canonical(&prepared.state)
    }
}

fn canonical(state: &crate::chemistry::stereo::perception::State) -> Result<String> {
    let written = smiles::write::write(state, Default::default())
        .map(|output| output.text)
        .map_err(|error| invalid(error.to_string()))?;
    // Drawing graphs retain explicit H atoms. The SMILES preparation path
    // removes ordinary explicit H without changing tetrahedral parity, so a
    // drawn N-H or C-alpha-H has the same identity as its implicit-H definition.
    if state
        .graph
        .atoms
        .iter()
        .any(|atom| atom.atomic_number == 1 && atom.isotope == 0)
    {
        let prepared = smiles::prepare(&written).map_err(|error| invalid(error.to_string()))?;
        return smiles::write::write(&prepared.state, Default::default())
            .map(|output| output.text)
            .map_err(|error| invalid(error.to_string()));
    }
    Ok(written)
}

fn definition_identity(definition: &ResidueDefinition) -> Result<String> {
    let prepared =
        smiles::prepare(&definition.smiles).map_err(|error| invalid(error.to_string()))?;
    canonical(&prepared.state)
}

/// Whether exactly the selected fragment is the labeled residue, including
/// stereochemistry and both outside single bonds. Unknown labels return false.
pub fn residue_matches_selection(
    doc: &Document,
    selection: &[u64],
    label: &str,
) -> std::result::Result<bool, String> {
    residue_selection_attachments(doc, selection, label).map(|matched| matched.is_some())
}

/// N and carbonyl-C attachments when exactly the selected fragment matches.
pub(crate) fn residue_selection_attachments(
    doc: &Document,
    selection: &[u64],
    label: &str,
) -> std::result::Result<Option<(u64, u64)>, String> {
    let Some(definition) = residue(label) else {
        return Ok(None);
    };
    let run = || -> Result<Option<(u64, u64)>> {
        let target = Target::new(doc)?;
        let selected: HashSet<_> = doc
            .expand_abbreviation_selection(selection)
            .into_iter()
            .filter(|id| target.atoms.contains_key(id))
            .collect();
        let identity = definition_identity(&definition)?;
        for candidate in target.candidates(&mut matcher::Work::default())? {
            if candidate.members.len() == selected.len()
                && candidate.members.iter().all(|id| selected.contains(id))
                && target.canonical(&candidate)? == identity
            {
                return Ok(Some((candidate.anchor, candidate.exit)));
            }
        }
        Ok(None)
    };
    run().map_err(|error| error.to_string())
}

pub(super) fn find(
    doc: &Document,
    molecule: &document::Molecule,
    selection: &[u64],
    definition: ResidueDefinition,
    policy: AttachmentPolicy,
) -> Result<Document> {
    if policy == AttachmentPolicy::Terminal {
        return Err(Error::Attachment);
    }
    let target = Target::new(doc)?;
    if molecule.rdkit_version != RDKIT_VERSION
        || molecule.ids.len() != molecule.state.graph.atoms.len()
        || molecule.ids.len() != target.atoms.len()
        || molecule.ids.iter().copied().collect::<HashSet<_>>().len() != target.atoms.len()
        || molecule.ids.iter().any(|id| !target.atoms.contains_key(id))
    {
        return Err(invalid("Prepared molecule identities changed"));
    }
    if selection.len() > 100_000 {
        return Err(invalid("Selection size limit exceeded"));
    }
    let selected: HashSet<_> = selection.iter().copied().collect();
    let identity = definition_identity(&definition)?;
    let mut used: HashSet<_> = doc
        .abbreviations
        .iter()
        .flat_map(|group| group.members.iter().copied())
        .collect();
    let mut result = doc.clone();
    for candidate in target.candidates(&mut matcher::Work::default())? {
        if candidate
            .members
            .iter()
            .any(|id| used.contains(id) || (!selected.is_empty() && !selected.contains(id)))
        {
            continue;
        }
        if candidate.members.iter().any(|id| {
            target.atoms.get(id).is_none_or(|atom| {
                atom.isotope != 0
                    || atom.map_num != 0
                    || !atom.marks.is_empty()
                    || atom.display.number.is_some()
            })
        }) {
            continue;
        }
        if target.canonical(&candidate)? != identity {
            continue;
        }
        used.extend(&candidate.members);
        result.abbreviations.push(Abbreviation {
            label: definition.label.clone(),
            reverse_label: String::new(),
            anchor: candidate.anchor,
            exit: Some(candidate.exit),
            members: candidate.members,
            alignment: Default::default(),
            label_style: None,
            label_color_override: false,
            highlight: None,
        });
    }
    if result.abbreviations.len() > doc.abbreviations.len() {
        result.require_version(crate::document::RESIDUE_GROUPS);
    }
    validate_with_policy(&result, policy)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
