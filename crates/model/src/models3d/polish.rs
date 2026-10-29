//! Persistent generated chemistry, separate from the live source drawing.
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Labels {
    #[default]
    None,
    Heteroatoms,
    All,
}
impl Labels {
    pub const ALL: [Self; 3] = [Self::None, Self::Heteroatoms, Self::All];
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
    pub(super) fn shows(self, symbol: &str) -> bool {
        match self {
            Self::None => false,
            Self::Heteroatoms => !matches!(symbol, "C" | "H"),
            Self::All => true,
        }
    }
}

/// Frozen rendering chemistry in geometry order; bond endpoints are indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Generated {
    /// Live source atom for each geometry index; deleted atoms have no source.
    pub sources: Vec<Option<u64>>,
    pub elements: Vec<String>,
    pub bonds: Vec<[usize; 2]>,
}

pub(super) fn is_zero(value: &i32) -> bool {
    *value == 0
}

impl Model3d {
    /// Imported crystal and XYZ geometry has no generated-conformer lifecycle.
    pub fn is_file_derived(&self) -> bool {
        self.file_geometry || self.cell.is_some() || !self.sites.is_empty()
    }

    pub fn needs_polish_version(&self) -> bool {
        self.fingerprint.is_some()
            || self.generated.is_some()
            || self.linked_atoms != self.atoms.iter().map(|a| a.id).collect::<Vec<_>>()
            || self.seed != 0
            || self.labels != Labels::None
            || self.file_geometry
    }

    /// The model's live selection links, in drawing order.
    pub fn linked_ids(&self, doc: &Document) -> Vec<u64> {
        let linked: HashSet<_> = self.source_ids().into_iter().collect();
        doc.atoms
            .iter()
            .filter(|a| linked.contains(&a.id))
            .map(|a| a.id)
            .collect()
    }

    /// Whether source chemistry differs from the chemistry that produced this
    /// geometry. Legacy models acquire a baseline at their first transaction.
    pub fn is_stale(&self, doc: &Document) -> bool {
        let Some(baseline) = self
            .fingerprint
            .as_ref()
            .filter(|_| !self.is_file_derived())
        else {
            return false;
        };
        let ids = self.chemistry_ids(doc);
        let sources = self.geometry_sources();
        sources.iter().any(Option::is_none)
            || sources.iter().copied().flatten().collect::<HashSet<_>>()
                != ids.iter().copied().collect()
            || baseline != &fingerprint(doc, &ids)
    }

    /// Every current component touched by a surviving source link, including
    /// additions and all fragments after a split. This is the Update target.
    pub fn chemistry_ids(&self, doc: &Document) -> Vec<u64> {
        let mut component = HashSet::new();
        for id in &self.linked_atoms {
            if !component.contains(id) {
                component.extend(crate::molecule_names::component(doc, *id));
            }
        }
        let mut ids: Vec<_> = self
            .linked_atoms
            .iter()
            .copied()
            .filter(|id| component.remove(id))
            .collect();
        let mut added: Vec<_> = component.into_iter().collect();
        added.sort_unstable();
        ids.extend(added);
        ids
    }

    pub(super) fn source_ids(&self) -> Vec<u64> {
        self.linked_atoms.clone()
    }

    /// Live source identities in geometry order. Geometry-only atoms have no
    /// drawing identity and cannot alias an atom created after deletion.
    pub fn geometry_sources(&self) -> Vec<Option<u64>> {
        self.generated.as_ref().map_or_else(
            || self.atoms.iter().map(|a| Some(a.id)).collect(),
            |generated| generated.sources.clone(),
        )
    }

    pub(super) fn symbol<'a>(&'a self, doc: &'a Document, index: usize) -> Option<&'a str> {
        if let Some(generated) = &self.generated {
            generated.elements.get(index).map(String::as_str)
        } else {
            doc.atom(self.atoms.get(index)?.id)
                .map(|a| a.element.as_str())
        }
    }

    pub(super) fn geometry_bonds(&self, doc: &Document) -> Vec<[usize; 2]> {
        if let Some(generated) = &self.generated {
            return generated.bonds.clone();
        }
        let index: HashMap<_, _> = self
            .atoms
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id, i))
            .collect();
        doc.bonds
            .iter()
            .filter_map(|b| Some([*index.get(&b.a)?, *index.get(&b.b)?]))
            .collect()
    }

    pub(super) fn capture_generated(&mut self, doc: &Document) {
        if self.is_file_derived() {
            return;
        }
        if self.generated.is_some() {
            self.fingerprint = Some(fingerprint(doc, &self.chemistry_ids(doc)));
            return;
        }
        let elements = self
            .atoms
            .iter()
            .map(|a| doc.atom(a.id).map(|a| a.element.clone()))
            .collect::<Option<Vec<_>>>();
        if let Some(elements) = elements {
            let bonds = self.geometry_bonds(doc);
            let sources = self.atoms.iter().map(|a| Some(a.id)).collect();
            self.generated = Some(Generated {
                sources,
                elements,
                bonds,
            });
            self.fingerprint = Some(fingerprint(doc, &self.chemistry_ids(doc)));
            self.index_geometry();
        }
    }

    /// Generated geometry uses only local indices, including hydrogen parents.
    fn index_geometry(&mut self) {
        let indices: HashMap<_, _> = self
            .atoms
            .iter()
            .enumerate()
            .map(|(index, atom)| (atom.id, index as u64))
            .collect();
        for hydrogen in &mut self.hydrogens {
            if let Some(index) = indices.get(&hydrogen.parent) {
                hydrogen.parent = *index;
            }
        }
        for (index, atom) in self.atoms.iter_mut().enumerate() {
            atom.id = index as u64;
        }
    }
}

/// Upgrade absent legacy source lists at the persistence boundary, rather
/// than giving an empty list a second meaning throughout the model lifecycle.
pub(crate) fn deserialize_models<'de, D>(deserializer: D) -> Result<Vec<Model3d>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut models = Vec::<Model3d>::deserialize(deserializer)?;
    for model in &mut models {
        if model.linked_atoms.is_empty() && model.generated.is_none() {
            model.linked_atoms = model.atoms.iter().map(|a| a.id).collect();
        }
    }
    Ok(models)
}

#[derive(Serialize)]
struct ChemicalAtom<'a> {
    element: &'a str,
    charge: i32,
    isotope: u32,
    radicals: u8,
    explicit_hydrogens: u32,
    no_implicit: bool,
    aromatic: bool,
}

#[derive(Serialize)]
struct ChemicalBond {
    a: usize,
    b: usize,
    order: u8,
}

#[derive(Serialize)]
struct PerceivedChemistry {
    valences: Vec<(u32, u32)>,
    atoms: Vec<(u8, Option<u32>)>,
    bonds: Vec<(u8, Vec<usize>, bool)>,
}

/// FNV-1a over dedicated chemistry records in persistent geometry/source order.
/// Presentation fields and nondirectional endpoint order never enter the digest.
fn fingerprint(doc: &Document, ids: &[u64]) -> String {
    let mapping: HashMap<_, _> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let index = |id| mapping.get(&id).copied().unwrap_or_default();
    let mut ordered = crate::editing::analysis_document(doc, ids);
    ordered.models.clear();
    ordered.atoms.sort_by_key(|a| index(a.id));
    // Legacy stereo priorities include atom maps before comparing substituent
    // graphs. They are annotations, so omit them from this detached perception
    // input just as analysis_document omits cached CIP labels.
    for atom in &mut ordered.atoms {
        atom.map_num = 0;
    }
    // Sort by chemical endpoints while retaining the direction of stereo
    // markers for perception. Ordinary and aromatic bonds may be reversed by
    // a presentation tool without changing either their graph or stereo.
    for bond in &mut ordered.bonds {
        let directed = matches!(bond.order, 0 | 5)
            || !bond.projection && bond.order == 1 && crate::bonds::is_stereo_marker(&bond.display);
        if !directed && index(bond.a) > index(bond.b) {
            bond.reverse();
        }
    }
    ordered.bonds.sort_by_key(|b| {
        let (a, b) = (index(b.a), index(b.b));
        (a.min(b), a.max(b))
    });
    let atoms: Vec<_> = ordered
        .atoms
        .iter()
        .map(|a| ChemicalAtom {
            element: &a.element,
            charge: a.charge,
            isotope: a.isotope,
            radicals: a.radical_electrons,
            explicit_hydrogens: a.explicit_h,
            no_implicit: a.no_implicit,
            aromatic: a.aromatic,
        })
        .collect();
    let bonds: Vec<_> = ordered
        .bonds
        .iter()
        .map(|bond| {
            let (mut a, mut b) = (index(bond.a), index(bond.b));
            if !matches!(bond.order, 0 | 5) && a > b {
                std::mem::swap(&mut a, &mut b);
            }
            ChemicalBond {
                a,
                b,
                order: bond.order,
            }
        })
        .collect();
    let perceived = crate::chemistry::document::prepare(&ordered)
        .ok()
        .map(|molecule| {
            let state = molecule.state;
            PerceivedChemistry {
                valences: state
                    .valences
                    .iter()
                    .map(|v| (v.explicit_valence, v.implicit_hydrogens))
                    .collect(),
                atoms: state
                    .metadata
                    .atoms
                    .iter()
                    .map(|a| (a.chiral_tag, a.chiral_permutation))
                    .collect(),
                bonds: state
                    .metadata
                    .bonds
                    .into_iter()
                    .map(|b| (b.stereo, b.stereo_atoms, b.unknown_stereo))
                    .collect(),
            }
        });
    let bytes = serde_json::to_vec(&(2u8, atoms, bonds, perceived)).unwrap_or_default();
    let hash = bytes.into_iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

/// Capture current chemical stereo before an old layout moves its atoms.
pub(super) fn current_stereo(doc: &Document, model: &Model3d) -> Drawn {
    let mut captured = Drawn::of(doc, model);
    let mut part = crate::editing::analysis_document(doc, &model.chemistry_ids(doc));
    part.models.clear();
    let Ok(molecule) = crate::chemistry::document::prepare(&part) else {
        return captured;
    };
    let index: HashMap<_, _> = molecule
        .ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let mut neighbors: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut bonds = HashMap::new();
    for (index, bond) in molecule.state.graph.bonds.iter().enumerate() {
        let Some((&a, &b)) = molecule.ids.get(bond.a).zip(molecule.ids.get(bond.b)) else {
            continue;
        };
        neighbors.entry(a).or_default().push(b);
        neighbors.entry(b).or_default().push(a);
        bonds.insert((a.min(b), a.max(b)), (index, a, bond.order));
    }
    for (id, _, _, stereo) in &mut captured.atoms {
        let Some(metadata) = index
            .get(id)
            .and_then(|index| molecule.state.metadata.atoms.get(*index))
        else {
            continue;
        };
        *stereo = match metadata.chiral_tag {
            0 => None,
            tag => Some(AtomStereo {
                winding: if tag == 1 { "cw" } else { "ccw" }.into(),
                neighbors: neighbors.get(id).cloned().unwrap_or_default(),
            }),
        };
    }
    for (a, b, stereo, atoms, authoritative) in &mut captured.bonds {
        let Some((index, start, order)) = bonds.get(&((*a).min(*b), (*a).max(*b))) else {
            continue;
        };
        let Some(metadata) = molecule.state.metadata.bonds.get(*index) else {
            continue;
        };
        let Some(mut refs) = metadata
            .stereo_atoms
            .iter()
            .map(|index| molecule.ids.get(*index).copied())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        if a != start {
            refs.reverse();
        }
        *stereo = match metadata.stereo {
            1 => Some("any"),
            2 => Some("z"),
            3 => Some("e"),
            4 => Some("cis"),
            5 => Some("trans"),
            _ => None,
        }
        .map(String::from);
        *atoms = refs;
        *authoritative |= *order == 2;
    }
    captured
}

#[cfg(test)]
mod tests;
