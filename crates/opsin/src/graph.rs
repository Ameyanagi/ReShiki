//! OPSIN's construction graph, rather than a sanitized molecule graph.
//!
//! Source: OPSIN 2.9.0, commit b91b610af5ab07560fedb20730d7aef46bb2bca0,
//! `Atom`, `Bond`, `Fragment`, `OutAtom`, `AtomParity`, and `BondStereo`.
//! Copyright Daniel Lowe and OPSIN contributors; distributed under the MIT
//! license retained in this crate. IDs are arena indices and never renumbered.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AtomId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BondId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FragmentId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutAtomId(pub usize);

macro_rules! elements {
    ($($symbol:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u8)]
        pub enum Element { $($symbol),+ }
        impl Element {
            pub const fn symbol(self) -> &'static str {
                match self { $(Self::$symbol => stringify!($symbol)),+ }
            }
            pub fn from_symbol(symbol: &str) -> Option<Self> {
                match symbol { $(stringify!($symbol) => Some(Self::$symbol)),+, _ => None }
            }
            pub const fn atomic_number(self) -> u8 { self as u8 }
            pub const fn is_chalcogen(self) -> bool { matches!(self, Self::O | Self::S | Self::Se | Self::Te) }
            pub const fn is_halogen(self) -> bool { matches!(self, Self::F | Self::Cl | Self::Br | Self::I) }
        }
    };
}
elements!(
    R, H, He, Li, Be, B, C, N, O, F, Ne, Na, Mg, Al, Si, P, S, Cl, Ar, K, Ca, Sc, Ti, V, Cr, Mn,
    Fe, Co, Ni, Cu, Zn, Ga, Ge, As, Se, Br, Kr, Rb, Sr, Y, Zr, Nb, Mo, Tc, Ru, Rh, Pd, Ag, Cd, In,
    Sn, Sb, Te, I, Xe, Cs, Ba, La, Ce, Pr, Nd, Pm, Sm, Eu, Gd, Tb, Dy, Ho, Er, Tm, Yb, Lu, Hf, Ta,
    W, Re, Os, Ir, Pt, Au, Hg, Tl, Pb, Bi, Po, At, Rn, Fr, Ra, Ac, Th, Pa, U, Np, Pu, Am, Cm, Bk,
    Cf, Es, Fm, Md, No, Lr, Rf, Db, Sg, Bh, Hs, Mt, Ds, Rg, Cn, Nh, Fl, Mc, Lv, Ts, Og
);

impl fmt::Display for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphError(pub String);
impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for GraphError {}

/// Dummy references must remain distinct during construction and replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StereoReference {
    Atom(AtomId),
    ImplicitHydrogen,
    DeoxyHydrogen,
    RingOpening,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StereoGroupType {
    Absolute,
    Racemic,
    Relative,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StereoGroup {
    pub kind: StereoGroupType,
    pub number: u32,
}
impl Default for StereoGroup {
    fn default() -> Self {
        Self {
            kind: StereoGroupType::Absolute,
            number: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomParity {
    pub atom_refs: [Option<StereoReference>; 4],
    pub parity: i8,
    pub stereo_group: StereoGroup,
}
impl AtomParity {
    pub fn new(atom_refs: [Option<StereoReference>; 4], parity: i8) -> Self {
        Self {
            atom_refs,
            parity,
            stereo_group: StereoGroup::default(),
        }
    }
    pub fn add_reference(&mut self, reference: StereoReference) -> Result<usize, GraphError> {
        let index = self.atom_refs.iter().position(Option::is_none).ok_or_else(|| {
            GraphError("Tetrahedral stereocentre specified in SMILES appears to involve more than 4 atoms".into())
        })?;
        self.atom_refs[index] = Some(reference);
        Ok(index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondStereoValue {
    Cis,
    Trans,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondStereo {
    pub atom_refs: [AtomId; 4],
    pub value: BondStereoValue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondDirection {
    Slash,
    Backslash,
}
impl BondDirection {
    pub const fn flipped(self) -> Self {
        match self {
            Self::Slash => Self::Backslash,
            Self::Backslash => Self::Slash,
        }
    }
}

/// Typed counterparts of the upstream PropertyKey values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AtomProperties {
    pub ambiguous_element_assignment: Vec<AtomId>,
    /// Identity of the upstream mutable Set. Equal member lists can be
    /// independent after copyAndRelabelFragment.
    pub ambiguous_element_assignment_id: Option<usize>,
    pub atom_class: Option<u32>,
    pub homology_group: Option<String>,
    pub position_variation_bond: Option<Vec<AtomId>>,
    pub smiles_hydrogen_count: Option<u32>,
    pub oxidation_number: Option<i32>,
    pub is_aldehyde: bool,
    pub is_anomeric: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    pub id: AtomId,
    pub active: bool,
    pub element: Element,
    pub fragment: FragmentId,
    pub atom_type: String,
    pub locants: Vec<String>,
    pub charge: i32,
    pub isotope: Option<u32>,
    /// In upstream this is represented by explicit H atoms after finalization.
    pub explicit_hydrogens: u32,
    pub bonds: Vec<BondId>,
    pub spare_valency: bool,
    pub out_valency: i32,
    pub lambda_convention_valency: Option<i32>,
    pub minimum_valency: Option<i32>,
    pub implicit_hydrogen_allowed: bool,
    pub protons_explicitly_added_or_removed: i32,
    pub in_cycle: bool,
    pub parity: Option<AtomParity>,
    pub properties: AtomProperties,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bond {
    pub id: BondId,
    pub active: bool,
    pub from: AtomId,
    pub to: AtomId,
    pub order: u8,
    pub stereo: Option<BondStereo>,
    /// Transient reader/writer state, not the semantic cis/trans descriptor.
    pub smiles_direction: Option<BondDirection>,
}
impl Bond {
    pub fn other_atom(&self, atom: AtomId) -> Option<AtomId> {
        if self.from == atom {
            Some(self.to)
        } else if self.to == atom {
            Some(self.from)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutAtom {
    pub id: OutAtomId,
    pub atom: AtomId,
    pub valency: i32,
    pub explicitly_set: bool,
    pub locant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub id: FragmentId,
    pub active: bool,
    pub fragment_type: String,
    pub sub_type: String,
    pub token_attributes: BTreeMap<String, String>,
    /// These vectors correspond to LinkedHashMap / LinkedHashSet insertion order.
    pub atoms: Vec<AtomId>,
    pub bonds: Vec<BondId>,
    pub locants: HashMap<String, AtomId>,
    pub out_atoms: Vec<OutAtom>,
    pub functional_atoms: Vec<AtomId>,
    pub default_in_atom: Option<AtomId>,
    pub indicated_hydrogens: Vec<AtomId>,
    pub polymer_attachment_points: Option<Vec<AtomId>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    pub fragments: Vec<Fragment>,
    next_out_atom_id: usize,
    next_ambiguous_element_assignment_id: usize,
}

impl Graph {
    pub fn atom(&self, id: AtomId) -> &Atom {
        &self.atoms[id.0]
    }
    pub fn atom_mut(&mut self, id: AtomId) -> &mut Atom {
        &mut self.atoms[id.0]
    }
    pub fn bond(&self, id: BondId) -> &Bond {
        &self.bonds[id.0]
    }
    pub fn bond_mut(&mut self, id: BondId) -> &mut Bond {
        &mut self.bonds[id.0]
    }
    pub fn fragment(&self, id: FragmentId) -> &Fragment {
        &self.fragments[id.0]
    }
    pub fn fragment_mut(&mut self, id: FragmentId) -> &mut Fragment {
        &mut self.fragments[id.0]
    }

    pub fn add_fragment(&mut self, fragment_type: impl Into<String>) -> FragmentId {
        let id = FragmentId(self.fragments.len());
        self.fragments.push(Fragment {
            id,
            active: true,
            fragment_type: fragment_type.into(),
            sub_type: String::new(),
            token_attributes: BTreeMap::new(),
            atoms: Vec::new(),
            bonds: Vec::new(),
            locants: HashMap::new(),
            out_atoms: Vec::new(),
            functional_atoms: Vec::new(),
            default_in_atom: None,
            indicated_hydrogens: Vec::new(),
            polymer_attachment_points: None,
        });
        id
    }

    pub fn add_atom(&mut self, fragment: FragmentId, element: Element) -> AtomId {
        let id = AtomId(self.atoms.len());
        let atom_type = self.fragment(fragment).fragment_type.clone();
        self.atoms.push(Atom {
            id,
            active: true,
            element,
            fragment,
            atom_type,
            locants: Vec::new(),
            charge: 0,
            isotope: None,
            explicit_hydrogens: 0,
            bonds: Vec::new(),
            spare_valency: false,
            out_valency: 0,
            lambda_convention_valency: None,
            minimum_valency: None,
            implicit_hydrogen_allowed: true,
            protons_explicitly_added_or_removed: 0,
            in_cycle: false,
            parity: None,
            properties: AtomProperties::default(),
        });
        self.fragment_mut(fragment).atoms.push(id);
        id
    }

    /// Allocate a fresh Bond after source constructor and Atom.addBond checks.
    /// Bond.equals compares unordered endpoint identities, ignoring order;
    /// normal creation cannot duplicate a pair, even with a fresh Bond object.
    pub fn add_bond(&mut self, from: AtomId, to: AtomId, order: u8) -> Result<BondId, GraphError> {
        if from == to {
            return Err(GraphError("A bond cannot connect an atom to itself".into()));
        }
        if order == 0 || order > 3 {
            return Err(GraphError("Bond order must be 1, 2, or 3".into()));
        }
        if self.bond_between(from, to).is_some() || self.bond_between(to, from).is_some() {
            return Err(GraphError("Atom already has given bond (This is not allowed as this would give two bonds between the same atoms!)".into()));
        }
        let id = BondId(self.bonds.len());
        self.bonds.push(Bond {
            id,
            active: true,
            from,
            to,
            order,
            stereo: None,
            smiles_direction: None,
        });
        self.atom_mut(from).bonds.push(id);
        self.atom_mut(to).bonds.push(id);
        if self.atom(from).fragment == self.atom(to).fragment {
            let fragment = self.atom(from).fragment;
            self.fragment_mut(fragment).bonds.push(id);
        }
        Ok(id)
    }

    pub fn bond_between(&self, a: AtomId, b: AtomId) -> Option<BondId> {
        self.atom(a)
            .bonds
            .iter()
            .copied()
            .find(|id| self.bond(*id).active && self.bond(*id).other_atom(a) == Some(b))
    }
    pub fn incoming_valency(&self, atom: AtomId) -> i32 {
        self.atom(atom)
            .bonds
            .iter()
            .map(|id| i32::from(self.bond(*id).order))
            .sum::<i32>()
            + self.atom(atom).explicit_hydrogens as i32
    }
    pub fn neighbours(&self, atom: AtomId) -> Vec<AtomId> {
        self.atom(atom)
            .bonds
            .iter()
            .filter_map(|id| self.bond(*id).other_atom(atom))
            .collect()
    }

    pub fn add_locant(&mut self, atom: AtomId, locant: impl Into<String>) {
        let locant = locant.into();
        let fragment = self.atom(atom).fragment;
        self.atom_mut(atom).locants.push(locant.clone());
        self.fragment_mut(fragment).locants.insert(locant, atom);
    }
    pub fn clear_locants(&mut self, atom: AtomId) {
        let fragment = self.atom(atom).fragment;
        for locant in std::mem::take(&mut self.atom_mut(atom).locants) {
            self.fragment_mut(fragment).locants.remove(&locant);
        }
    }
    pub fn remove_locant(&mut self, atom: AtomId, locant: &str) {
        let fragment = self.atom(atom).fragment;
        self.atom_mut(atom).locants.retain(|value| value != locant);
        self.fragment_mut(fragment).locants.remove(locant);
    }
    pub fn set_out_atom_locant(
        &mut self,
        fragment: FragmentId,
        index: usize,
        locant: Option<String>,
    ) {
        let id = self.fragment(fragment).out_atoms[index].id;
        for view in &mut self.fragments {
            for out in &mut view.out_atoms {
                if out.id == id {
                    out.locant = locant.clone();
                }
            }
        }
    }
    /// Exact OutAtom.setAtom: the caller manages any out-valency transfer.
    pub fn set_out_atom_target(&mut self, fragment: FragmentId, index: usize, atom: AtomId) {
        let id = self.fragment(fragment).out_atoms[index].id;
        for view in &mut self.fragments {
            for out in &mut view.out_atoms {
                if out.id == id {
                    out.atom = atom;
                }
            }
        }
    }
    pub fn atom_by_locant(&self, fragment: FragmentId, locant: &str) -> Option<AtomId> {
        if let Some(atom) = self.fragment(fragment).locants.get(locant).copied() {
            return Some(atom);
        }
        let (element, primes, backbone) =
            crate::fragment_tools::parse_amino_acid_style_locant(locant)?;
        let backbone = self.fragment(fragment).locants.get(backbone).copied()?;
        crate::fragment_tools::get_atom_by_amino_acid_style_locant(self, backbone, element, primes)
    }

    pub fn add_out_atom(
        &mut self,
        fragment: FragmentId,
        atom: AtomId,
        valency: i32,
        explicitly_set: bool,
    ) {
        if explicitly_set {
            self.atom_mut(atom).out_valency += valency;
        }
        let id = OutAtomId(self.next_out_atom_id);
        self.next_out_atom_id += 1;
        self.fragment_mut(fragment).out_atoms.push(OutAtom {
            id,
            atom,
            valency,
            explicitly_set,
            locant: None,
        });
    }
    pub fn remove_out_atom(&mut self, fragment: FragmentId, index: usize) -> OutAtom {
        let out = self.fragment_mut(fragment).out_atoms.remove(index);
        if out.explicitly_set {
            self.atom_mut(out.atom).out_valency -= out.valency;
        }
        out
    }

    /// StructureBuilder.convertOutAtomsToAttachmentAtoms. Call after explicit
    /// hydrogen finalization when output_radicals_as_wildcard_atoms is enabled.
    /// Reverse order is intentional and affects insertion order/CX labels.
    pub fn convert_out_atoms_to_attachment_atoms(
        &mut self,
        fragment: FragmentId,
    ) -> Result<(), GraphError> {
        let backup = self.clone();
        let result = (|| {
            for index in (0..self.fragment(fragment).out_atoms.len()).rev() {
                let out = self.remove_out_atom(fragment, index);
                let order = u8::try_from(out.valency)
                    .ok()
                    .filter(|order| (1..=3).contains(order))
                    .ok_or_else(|| {
                        GraphError("Radical attachment bond order must be 1, 2, or 3".into())
                    })?;
                let wildcard = self.add_atom(fragment, Element::R);
                self.add_bond(out.atom, wildcard, order)?;
            }
            Ok(())
        })();
        if result.is_err() {
            *self = backup;
        }
        result
    }
    pub fn set_out_atom_valency(&mut self, fragment: FragmentId, index: usize, valency: i32) {
        let out = &self.fragment(fragment).out_atoms[index];
        let id = out.id;
        if out.explicitly_set {
            let atom = out.atom;
            let change = valency - out.valency;
            self.atom_mut(atom).out_valency += change;
        }
        for view in &mut self.fragments {
            for out in &mut view.out_atoms {
                if out.id == id {
                    out.valency = valency;
                }
            }
        }
    }
    pub fn set_out_atom_explicit(
        &mut self,
        fragment: FragmentId,
        index: usize,
        explicitly_set: bool,
    ) {
        let out = &self.fragment(fragment).out_atoms[index];
        let id = out.id;
        if out.explicitly_set != explicitly_set {
            let atom = out.atom;
            let change = if explicitly_set {
                out.valency
            } else {
                -out.valency
            };
            self.atom_mut(atom).out_valency += change;
        }
        for view in &mut self.fragments {
            for out in &mut view.out_atoms {
                if out.id == id {
                    out.explicitly_set = explicitly_set;
                }
            }
        }
    }

    /// Incorporation retains IDs and atom types; locant collision precedence is
    /// the source's order, as in Fragment.addAtom. Interfragment bonds become
    /// internal once both endpoints belong to the destination.
    pub fn incorporate_fragment(
        &mut self,
        source: FragmentId,
        destination: FragmentId,
    ) -> Result<(), GraphError> {
        if source == destination {
            return Err(GraphError(
                "Cannot incorporate a fragment into itself".into(),
            ));
        }
        if !self.fragment(source).active || !self.fragment(destination).active {
            return Err(GraphError("Cannot incorporate an inactive fragment".into()));
        }
        let source_copy = self.fragment(source).clone();
        for atom in source_copy.atoms {
            self.atom_mut(atom).fragment = destination;
            let locants = self.atom(atom).locants.clone();
            self.fragment_mut(destination).atoms.push(atom);
            for locant in locants {
                self.fragment_mut(destination).locants.insert(locant, atom);
            }
        }
        for bond in source_copy.bonds {
            if !self.fragment(destination).bonds.contains(&bond) {
                self.fragment_mut(destination).bonds.push(bond);
            }
        }
        for bond in &self.bonds {
            if bond.active
                && self.atom(bond.from).fragment == destination
                && self.atom(bond.to).fragment == destination
                && !self.fragment(destination).bonds.contains(&bond.id)
            {
                self.fragments[destination.0].bonds.push(bond.id);
            }
        }
        let dest = self.fragment_mut(destination);
        dest.out_atoms.extend(source_copy.out_atoms);
        dest.functional_atoms.extend(source_copy.functional_atoms);
        let src = self.fragment_mut(source);
        src.active = false;
        // FragmentManager.incorporateFragment leaves the source view intact.
        // Parse-tree elements retain this view even after registry removal.
        Ok(())
    }

    pub fn remove_bond(&mut self, bond: BondId) {
        let b = self.bond(bond).clone();
        self.bond_mut(bond).active = false;
        self.atom_mut(b.from).bonds.retain(|id| *id != bond);
        self.atom_mut(b.to).bonds.retain(|id| *id != bond);
        // Only the owning fragment loses membership. Incorporated source
        // views retain their original bond set, matching FragmentManager.
        self.fragment_mut(self.atom(b.from).fragment)
            .bonds
            .retain(|id| *id != bond);
    }

    pub fn remove_atom_and_associated_bonds(&mut self, atom: AtomId) {
        for bond in self.atom(atom).bonds.clone() {
            self.remove_bond(bond);
        }
        let fragment = self.atom(atom).fragment;
        let locants = self.atom(atom).locants.clone();
        let view = self.fragment_mut(fragment);
        view.atoms.retain(|id| *id != atom);
        for locant in locants {
            view.locants.remove(&locant);
        }
        if view.default_in_atom == Some(atom) {
            view.default_in_atom = None;
        }
        self.atom_mut(atom).active = false;
        self.remove_ambiguous_element_assignment_members(atom, &[atom]);
        let ambiguous = self
            .atom(atom)
            .properties
            .ambiguous_element_assignment
            .clone();
        if ambiguous.len() == 1 {
            let properties = &mut self.atom_mut(ambiguous[0]).properties;
            properties.ambiguous_element_assignment.clear();
            properties.ambiguous_element_assignment_id = None;
        }
    }

    /// Assign the same insertion-ordered mutable Set to all holders, as in
    /// FunctionalReplacement. Copying a fragment creates separate Sets.
    pub fn set_ambiguous_element_assignment(&mut self, holders: &[AtomId], members: Vec<AtomId>) {
        let id = self.next_ambiguous_element_assignment_id;
        self.next_ambiguous_element_assignment_id += 1;
        let mut unique = Vec::new();
        for member in members {
            if !unique.contains(&member) {
                unique.push(member);
            }
        }
        for &holder in holders {
            let properties = &mut self.atom_mut(holder).properties;
            properties.ambiguous_element_assignment = unique.clone();
            properties.ambiguous_element_assignment_id = Some(id);
        }
    }

    /// Mutate only the Set referenced by this holder. Untagged legacy member
    /// lists are independent; equality alone never establishes sharing.
    pub fn remove_ambiguous_element_assignment_members(
        &mut self,
        holder: AtomId,
        members: &[AtomId],
    ) {
        let id = self.atom(holder).properties.ambiguous_element_assignment_id;
        for atom in &mut self.atoms {
            if atom.id == holder
                || id.is_some() && atom.properties.ambiguous_element_assignment_id == id
            {
                atom.properties
                    .ambiguous_element_assignment
                    .retain(|member| !members.contains(member));
            }
        }
    }

    pub fn replace_ambiguous_element_assignment_member(
        &mut self,
        holder: AtomId,
        old: AtomId,
        new: AtomId,
    ) {
        let id = self.atom(holder).properties.ambiguous_element_assignment_id;
        for atom in &mut self.atoms {
            if atom.id == holder
                || id.is_some() && atom.properties.ambiguous_element_assignment_id == id
            {
                let members = &mut atom.properties.ambiguous_element_assignment;
                if let Some(index) = members.iter().position(|member| *member == old) {
                    members.remove(index);
                    if !members.contains(&new) {
                        members.push(new);
                    }
                }
            }
        }
    }

    /// FragmentManager.copyAndRelabelFragment. A standalone copy has a dummy
    /// token with type/subtype; cloneElement supplies its copied token later.
    pub fn copy_and_relabel_fragment(
        &mut self,
        original: FragmentId,
        primes_to_add: u32,
    ) -> Result<FragmentId, GraphError> {
        let backup = self.clone();
        let result = self.copy_and_relabel_fragment_inner(original, primes_to_add);
        if result.is_err() {
            *self = backup;
        }
        result
    }
    fn copy_and_relabel_fragment_inner(
        &mut self,
        original: FragmentId,
        primes_to_add: u32,
    ) -> Result<FragmentId, GraphError> {
        let source = self.fragment(original).clone();
        let destination = self.add_fragment(source.fragment_type.clone());
        self.fragment_mut(destination).sub_type = source.sub_type.clone();
        let mut remap = HashMap::new();
        for &atom in &source.atoms {
            let old = self.atom(atom).clone();
            let new_id = self.add_atom(destination, old.element);
            let mut new = old.clone();
            new.id = new_id;
            new.fragment = destination;
            new.active = true;
            new.bonds.clear();
            new.locants.clear();
            new.out_valency = 0;
            new.parity = None;
            self.atoms[new_id.0] = new;
            for locant in old.locants {
                let labelled = if primes_to_add == 0 {
                    locant
                } else {
                    let stem = locant.trim_end_matches('\'');
                    let current = locant.len() - stem.len();
                    let mut highest = current;
                    while self
                        .atom_by_locant(original, &format!("{stem}{}", "'".repeat(highest + 1)))
                        .is_some()
                    {
                        highest += 1;
                    }
                    format!(
                        "{stem}{}",
                        "'".repeat((highest + 1) * primes_to_add as usize + current)
                    )
                };
                self.add_locant(new_id, labelled);
            }
            remap.insert(atom, new_id);
        }
        let mapped = |atom: AtomId| {
            remap.get(&atom).copied().ok_or_else(|| {
                GraphError("Copied fragment contains an external atom reference".into())
            })
        };
        for &old_id in &source.atoms {
            let mut old = self.atom(old_id).clone();
            let new_id = remap[&old_id];
            if let Some(parity) = &mut old.parity {
                for reference in &mut parity.atom_refs {
                    if let Some(StereoReference::Atom(atom)) = reference {
                        *reference = remap.get(atom).copied().map(StereoReference::Atom);
                    }
                }
            }
            self.atom_mut(new_id).parity = old.parity;
            self.atom_mut(new_id)
                .properties
                .ambiguous_element_assignment = old
                .properties
                .ambiguous_element_assignment
                .into_iter()
                .map(mapped)
                .collect::<Result<_, _>>()?;
            let members = self
                .atom(new_id)
                .properties
                .ambiguous_element_assignment
                .clone();
            if old.properties.ambiguous_element_assignment_id.is_some() || !members.is_empty() {
                self.set_ambiguous_element_assignment(&[new_id], members);
            } else {
                self.atom_mut(new_id)
                    .properties
                    .ambiguous_element_assignment_id = None;
            }
            self.atom_mut(new_id).properties.position_variation_bond = old
                .properties
                .position_variation_bond
                .map(|atoms| atoms.into_iter().map(mapped).collect::<Result<Vec<_>, _>>())
                .transpose()?;
        }
        for out in source.out_atoms {
            self.add_out_atom(
                destination,
                mapped(out.atom)?,
                out.valency,
                out.explicitly_set,
            );
            let index = self.fragment(destination).out_atoms.len() - 1;
            self.set_out_atom_locant(
                destination,
                index,
                out.locant
                    .map(|locant| format!("{locant}{}", "'".repeat(primes_to_add as usize))),
            );
        }
        self.fragment_mut(destination).functional_atoms = source
            .functional_atoms
            .into_iter()
            .map(mapped)
            .collect::<Result<_, _>>()?;
        self.fragment_mut(destination).default_in_atom =
            source.default_in_atom.map(mapped).transpose()?;
        for bond in source.bonds {
            let b = self.bond(bond).clone();
            let new = self.add_bond(mapped(b.from)?, mapped(b.to)?, b.order)?;
            self.bond_mut(new).smiles_direction = b.smiles_direction;
            if let Some(stereo) = b.stereo {
                let refs = stereo
                    .atom_refs
                    .into_iter()
                    .map(mapped)
                    .collect::<Result<Vec<_>, _>>()?;
                self.bond_mut(new).stereo = Some(BondStereo {
                    atom_refs: refs.try_into().unwrap(),
                    value: stereo.value,
                });
            }
        }
        self.fragment_mut(destination).indicated_hydrogens = source
            .indicated_hydrogens
            .into_iter()
            .map(mapped)
            .collect::<Result<_, _>>()?;
        Ok(destination)
    }

    /// Port of Atom.determineValency; unknown elements preserve current valency.
    pub fn determine_valency(&self, atom: AtomId, consider_out_valency: bool) -> i32 {
        let a = self.atom(atom);
        let proton_delta = a.protons_explicitly_added_or_removed;
        if let Some(lambda) = a.lambda_convention_valency {
            return lambda + proton_delta;
        }
        let current = self.incoming_valency(atom)
            + if consider_out_valency {
                a.out_valency
            } else {
                0
            };
        let minimum = a.minimum_valency.map(|value| value + proton_delta);
        if (a.charge == 0 || proton_delta != 0)
            && let Some(default) =
                crate::valence::default_valency(a.element).map(|value| value + proton_delta)
            && current <= default
            && minimum.is_none_or(|value| default >= value)
        {
            return default;
        }
        if let Some(possible) = crate::valence::possible_valencies(a.element, a.charge) {
            if let Some(minimum) = minimum.filter(|value| *value >= current) {
                return minimum;
            }
            for &value in possible {
                if minimum.is_some_and(|minimum| value < minimum) {
                    continue;
                }
                if current <= value {
                    return value;
                }
            }
        }
        minimum.filter(|value| *value >= current).unwrap_or(current)
    }

    /// Assign ring membership without treating aromatic spare valency as bonds.
    pub fn assign_cycle_membership(&mut self, fragment: FragmentId) {
        crate::cycle_detector::assign_cycle_membership(self, fragment);
    }

    /// Explicit hydrogens are added only at finalization, after names have set
    /// lambda/minimum valency and used the fragment's out atoms.
    pub fn make_hydrogens_explicit(&mut self, fragment: FragmentId) -> Result<(), GraphError> {
        let backup = self.clone();
        let result = self.make_hydrogens_explicit_inner(fragment);
        if result.is_err() {
            *self = backup;
        }
        result
    }

    fn make_hydrogens_explicit_inner(&mut self, fragment: FragmentId) -> Result<(), GraphError> {
        let atoms = self.fragment(fragment).atoms.clone();
        for atom in atoms {
            if self.atom(atom).spare_valency {
                return Err(GraphError(
                    "Spare valency must be converted to double bonds before finalization".into(),
                ));
            }
            let count = crate::fragment_tools::calculate_substitutable_hydrogen_atoms(self, atom);
            let compact_count = self.atom(atom).explicit_hydrogens;
            self.atom_mut(atom).explicit_hydrogens = 0;
            for _ in 0..count as u32 + compact_count {
                let hydrogen = self.add_atom(fragment, Element::H);
                self.add_bond(atom, hydrogen, 1)?;
            }
            if let Some(parity) = self.atom(atom).parity.clone() {
                if count as u32 + compact_count > 1
                    || !crate::stereo_analyser::is_possibly_stereogenic(self, atom)
                {
                    // FragmentManager.makeHydrogensExplicit: deoxy processing
                    // can leave a now-achiral centre with two hydrogens.
                    self.atom_mut(atom).parity = None;
                    continue;
                }
                let placeholders = parity
                    .atom_refs
                    .iter()
                    .filter(|reference| {
                        matches!(
                            reference,
                            Some(
                                StereoReference::ImplicitHydrogen | StereoReference::DeoxyHydrogen
                            )
                        )
                    })
                    .count();
                if placeholders == 0 {
                    continue;
                }
                let candidates: Vec<_> = self
                    .neighbours(atom)
                    .into_iter()
                    .filter(|id| !parity.atom_refs.contains(&Some(StereoReference::Atom(*id))))
                    .collect();
                if placeholders == 2 && candidates.len() == 2 {
                    let higher_first = match crate::cip::CipSequenceRules::new(self, atom)
                        .get_neighbouring_atoms_in_cip_order()
                    {
                        Ok(ordered) => {
                            ordered.iter().position(|atom| *atom == candidates[0])
                                > ordered.iter().position(|atom| *atom == candidates[1])
                        }
                        Err(crate::cip::CipOrderingError::UnresolvedTie) => true,
                        Err(error) => return Err(GraphError(error.to_string())),
                    };
                    let (deoxy, hydrogen) = if higher_first {
                        (candidates[0], candidates[1])
                    } else {
                        (candidates[1], candidates[0])
                    };
                    let mut parity = parity;
                    for reference in &mut parity.atom_refs {
                        if *reference == Some(StereoReference::DeoxyHydrogen) {
                            *reference = Some(StereoReference::Atom(deoxy));
                        } else if *reference == Some(StereoReference::ImplicitHydrogen) {
                            *reference = Some(StereoReference::Atom(hydrogen));
                        }
                    }
                    self.atom_mut(atom).parity = Some(parity);
                    continue;
                }
                if placeholders != 1 || candidates.len() != 1 {
                    return Err(GraphError(
                        "Unable to determine which atom has substituted a hydrogen at stereocentre"
                            .into(),
                    ));
                }
                let mut parity = parity;
                for reference in &mut parity.atom_refs {
                    if matches!(
                        reference,
                        Some(StereoReference::ImplicitHydrogen | StereoReference::DeoxyHydrogen)
                    ) {
                        *reference = Some(StereoReference::Atom(candidates[0]));
                    }
                }
                self.atom_mut(atom).parity = Some(parity);
            }
        }
        Ok(())
    }
}
