//! Application of OPSIN stereochemical descriptors to the completed graph.
//! Port of `StereochemistryHandler.java`, OPSIN 2.9.0,
//! b91b610af5ab07560fedb20730d7aef46bb2bca0; MIT, Daniel Lowe and contributors.
//! Hydrogen neighbours must be physical atoms before this pass. A lone pair
//! is represented by the centre itself, as in the upstream StereoAnalyser.

use crate::WarningKind;
use crate::build_state::BuildState;
use crate::cip::{CipOrderingError, CipSequenceRules};
use crate::graph::{
    AtomId, AtomParity, BondId, BondStereo, BondStereoValue, Element, FragmentId, Graph,
    GraphError, StereoGroup, StereoGroupType, StereoReference,
};
use crate::parse_tree::{Arena, NodeId};
use crate::stereo_analyser::{StereoBond, StereoCentre};
use crate::xml_declarations::*;
use std::collections::BTreeMap;
use std::fmt;

/// Upstream warning mode catches StereochemistryException (including CIP
/// errors), but not StructureBuildingException or an inconsistent graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StereochemistryError {
    Uninterpretable(String),
    StructureBuilding(String),
}
impl fmt::Display for StereochemistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uninterpretable(s) | Self::StructureBuilding(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for StereochemistryError {}
impl From<CipOrderingError> for StereochemistryError {
    fn from(e: CipOrderingError) -> Self {
        Self::Uninterpretable(e.to_string())
    }
}
impl From<GraphError> for StereochemistryError {
    fn from(e: GraphError) -> Self {
        Self::StructureBuilding(e.to_string())
    }
}
type Result<T = ()> = std::result::Result<T, StereochemistryError>;
fn structure<T>(s: impl Into<String>) -> Result<T> {
    Err(StereochemistryError::StructureBuilding(s.into()))
}
fn stereo<T>(s: impl Into<String>) -> Result<T> {
    Err(StereochemistryError::Uninterpretable(s.into()))
}

pub struct StereochemistryHandler<'a> {
    pub state: &'a mut BuildState,
    pub arena: &'a mut Arena,
    pub atom_stereo_centre_map: BTreeMap<AtomId, StereoCentre>,
    pub bond_stereo_bond_map: BTreeMap<BondId, StereoBond>,
    pub not_explicitly_defined_stereo_centre_map: BTreeMap<AtomId, StereoCentre>,
    pub not_explicitly_defined_stereo_bond_map: BTreeMap<BondId, StereoBond>,
}
impl<'a> StereochemistryHandler<'a> {
    pub fn new(
        state: &'a mut BuildState,
        arena: &'a mut Arena,
        centres: &[StereoCentre],
        bonds: &[StereoBond],
    ) -> Self {
        let atoms: BTreeMap<_, _> = centres.iter().map(|c| (c.atom, *c)).collect();
        let bonds: BTreeMap<_, _> = bonds.iter().map(|b| (b.bond, *b)).collect();
        Self {
            state,
            arena,
            not_explicitly_defined_stereo_centre_map: atoms.clone(),
            not_explicitly_defined_stereo_bond_map: bonds.clone(),
            atom_stereo_centre_map: atoms,
            bond_stereo_bond_map: bonds,
        }
    }
    fn attribute(&self, id: NodeId, key: &str) -> Option<String> {
        self.arena[id].attribute(key).map(str::to_string)
    }
    fn attr(&self, id: NodeId, key: &str) -> String {
        self.attribute(id, key).unwrap_or_default()
    }
    fn graph(&self) -> &Graph {
        self.state.graph()
    }
    fn fragment(&self, id: NodeId) -> Result<FragmentId> {
        self.arena[id].fragment.ok_or_else(|| {
            StereochemistryError::StructureBuilding(format!(
                "OPSIN bug: No fragment on {}",
                self.arena.to_xml(id)
            ))
        })
    }
    fn parent(&self, id: NodeId) -> Result<NodeId> {
        self.arena[id].parent.ok_or_else(|| {
            StereochemistryError::StructureBuilding(
                "OPSIN bug: Stereochemistry element has no parent".into(),
            )
        })
    }
    fn fragment_attribute(&self, fragment: FragmentId, key: &str) -> Option<String> {
        if let Some(token) = self.state.fragment_manager.token_for_fragment(fragment) {
            self.attribute(token, key)
        } else {
            self.graph()
                .fragment(fragment)
                .token_attributes
                .get(key)
                .cloned()
        }
    }
    fn possible_fragments(&self, id: NodeId) -> Result<Vec<FragmentId>> {
        let parent = self.parent(id)?;
        let mut fragments =
            crate::structure_building_methods::find_alternative_fragments(self.arena, parent)?;
        for group in self
            .arena
            .descendants_named(parent, GROUP_EL)
            .into_iter()
            .rev()
        {
            fragments.push(self.fragment(group)?)
        }
        Ok(fragments)
    }
    fn later_word_fragments(&self, id: NodeId, require_initial: bool) -> Result<Vec<FragmentId>> {
        let parent = self.parent(id)?;
        let Some(word) = self.arena[parent]
            .parent
            .filter(|&p| self.arena[p].name == WORD_EL)
        else {
            return Ok(Vec::new());
        };
        if require_initial && self.arena[word].children.first().copied() != Some(parent) {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        for w in self.arena.next_siblings_named(word, WORD_EL) {
            for group in self.arena.descendants_named(w, GROUP_EL).into_iter().rev() {
                result.push(self.fragment(group)?)
            }
        }
        Ok(result)
    }
    fn warning_or_error(&mut self, result: Result) -> Result {
        match result {
            Err(StereochemistryError::Uninterpretable(message))
                if self
                    .state
                    .options
                    .warn_rather_than_fail_on_uninterpretable_stereochemistry =>
            {
                self.state
                    .add_warning(WarningKind::StereochemistryIgnored, message);
                Ok(())
            }
            other => other,
        }
    }
    pub fn apply_stereochemical_elements(&mut self, elements: &[NodeId]) -> Result {
        let mut locanted = Vec::new();
        let mut unlocanted = Vec::new();
        let mut carbohydrate = Vec::new();
        let mut global = Vec::new();
        for &id in elements {
            if self.arena[id].attribute(LOCANT_ATR).is_some() {
                locanted.push(id)
            } else {
                match self.attr(id, TYPE_ATR).as_str() {
                    CARBOHYDRATECONFIGURATIONPREFIX_TYPE_VAL => carbohydrate.push(id),
                    RAC_TYPE_VAL | REL_TYPE_VAL => global.push(id),
                    _ => unlocanted.push(id),
                }
            }
        }
        for id in locanted {
            let result = self.match_stereochemistry(id);
            self.warning_or_error(result)?
        }
        if !carbohydrate.is_empty() {
            self.process_carbohydrate_stereochemistry(&carbohydrate)?
        }
        for id in unlocanted {
            let result = self.match_stereochemistry(id);
            self.warning_or_error(result)?
        }
        if global.len() > 1 {
            let message = "More than one global indicator of rac- or rel- was specified";
            if self
                .state
                .options
                .warn_rather_than_fail_on_uninterpretable_stereochemistry
            {
                self.state
                    .add_warning(WarningKind::StereochemistryIgnored, message)
            } else {
                return structure(message);
            }
        }
        for id in global {
            let result = self.match_stereochemistry(id);
            self.warning_or_error(result)?
        }
        Ok(())
    }
    pub fn remove_redundant_stereo_centres(&mut self, atoms: &[AtomId], bonds: &[BondId]) {
        for &atom in atoms {
            if !self.atom_stereo_centre_map.contains_key(&atom) {
                self.state.graph_mut().atom_mut(atom).parity = None
            }
        }
        for &bond in bonds {
            if !self.bond_stereo_bond_map.contains_key(&bond) {
                self.state.graph_mut().bond_mut(bond).stereo = None
            }
        }
    }
    fn match_stereochemistry(&mut self, id: NodeId) -> Result {
        let kind = self.attr(id, TYPE_ATR);
        match kind.as_str() {
            R_OR_S_TYPE_VAL=>self.assign_stereo_centre(id)?,E_OR_Z_TYPE_VAL=>self.assign_stereo_bond(id)?,
            CISORTRANS_TYPE_VAL=>{if !self.assign_cis_trans_on_ring(id)?{self.assign_stereo_bond(id)?}},
            ALPHA_OR_BETA_TYPE_VAL=>self.assign_alpha_beta_xi_stereochem(id)?,DLSTEREOCHEMISTRY_TYPE_VAL=>self.assign_dl_stereochem(id)?,
            RAC_TYPE_VAL=>self.apply_global_rac_or_rel(id,StereoGroupType::Racemic)?,REL_TYPE_VAL=>self.apply_global_rac_or_rel(id,StereoGroupType::Relative)?,
            ENDO_EXO_SYN_ANTI_TYPE_VAL|RELATIVECISTRANS_TYPE_VAL|AXIAL_TYPE_VAL=>return stereo(format!("{kind} stereochemistry is not currently interpretable by OPSIN")),
            OPTICALROTATION_TYPE_VAL=>self.state.add_warning(WarningKind::StereochemistryIgnored,format!("Optical rotation cannot be algorithmically used to assign stereochemistry. This term was ignored: {}",self.arena.value(id))),
            _=>return structure(format!("Unexpected stereochemistry type: {kind}"))
        }
        self.arena.detach(id);
        Ok(())
    }
    fn apply_global_rac_or_rel(&mut self, id: NodeId, kind: StereoGroupType) -> Result {
        let mut word = self.arena[id].parent;
        while word.is_some_and(|w| self.arena[w].name != WORD_EL) {
            word = self.arena[word.unwrap()].parent
        }
        let Some(word) = word else { return Ok(()) };
        let mut fragments = self.possible_fragments(id)?;
        for w in self.arena.next_siblings_named(word, WORD_EL) {
            for g in self.arena.descendants_named(w, GROUP_EL).into_iter().rev() {
                fragments.push(self.fragment(g)?)
            }
        }
        let mut undefined = Vec::new();
        let mut defined = Vec::new();
        for f in fragments {
            for &a in &self.graph().fragment(f).atoms {
                if self.graph().atom(a).parity.is_some() {
                    defined.push(a)
                } else if self
                    .not_explicitly_defined_stereo_centre_map
                    .contains_key(&a)
                {
                    undefined.push(a)
                }
            }
        }
        if !undefined.is_empty() {
            if undefined.len() > 1 {
                self.state.add_warning(
                    WarningKind::StereochemistryIgnored,
                    "More than one undefined stereocenter for rac- or rel- mixture",
                );
                return Ok(());
            }
            let atom = undefined[0];
            let centre = self.not_explicitly_defined_stereo_centre_map[&atom];
            match apply_stereo_chemistry_to_stereo_centre(self.state.graph_mut(), centre, "R") {
                Err(StereochemistryError::Uninterpretable(message)) => {
                    self.state.add_warning(
                        WarningKind::StereochemistryIgnored,
                        format!("Could not set rac- or rel- stereochemistry: {message}"),
                    );
                    return Ok(());
                }
                other => other?,
            }
            self.set_stereo_group(atom, kind, 1);
            self.not_explicitly_defined_stereo_centre_map.remove(&atom);
        } else {
            for atom in defined {
                self.set_stereo_group(atom, kind, 1)
            }
        }
        Ok(())
    }
    fn set_stereo_group(&mut self, atom: AtomId, kind: StereoGroupType, number: u32) {
        if let Some(parity) = &mut self.state.graph_mut().atom_mut(atom).parity {
            parity.stereo_group = StereoGroup { kind, number }
        }
    }
    fn assign_stereo_centre(&mut self, id: NodeId) -> Result {
        let locant = self.attribute(id, LOCANT_ATR);
        let value = self.attr(id, VALUE_ATR);
        let kind = match self.attribute(id, STEREOGROUP_ATR).as_deref() {
            Some("Abs") => StereoGroupType::Absolute,
            Some("Rac") => StereoGroupType::Racemic,
            Some("Rel") => StereoGroupType::Relative,
            None | Some("Unk") => StereoGroupType::Unknown,
            Some(s) => return structure(format!("Unexpected stereogroup type: {s}")),
        };
        for fragment in self.possible_fragments(id)? {
            if self.attempt_assign_stereo_centre(fragment, &value, locant.as_deref(), kind)? {
                return Ok(());
            }
        }
        for fragment in self.later_word_fragments(id, true)? {
            if self.attempt_assign_stereo_centre(fragment, &value, locant.as_deref(), kind)? {
                return Ok(());
            }
        }
        stereo(format!(
            "Could not find atom that: {} appeared to be referring to",
            self.arena.to_xml(id)
        ))
    }
    fn attempt_assign_stereo_centre(
        &mut self,
        fragment: FragmentId,
        value: &str,
        locant: Option<&str>,
        kind: StereoGroupType,
    ) -> Result<bool> {
        let atom = if let Some(locant) = locant {
            self.graph().atom_by_locant(fragment, locant).filter(|a| {
                self.not_explicitly_defined_stereo_centre_map
                    .contains_key(a)
            })
        } else {
            self.graph()
                .fragment(fragment)
                .atoms
                .iter()
                .copied()
                .find(|a| {
                    self.not_explicitly_defined_stereo_centre_map
                        .contains_key(a)
                })
        };
        if let Some(atom) = atom {
            let centre = self.not_explicitly_defined_stereo_centre_map[&atom];
            apply_stereo_chemistry_to_stereo_centre(self.state.graph_mut(), centre, value)?;
            self.set_stereo_group(atom, kind, 1);
            self.not_explicitly_defined_stereo_centre_map.remove(&atom);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn assign_stereo_bond(&mut self, id: NodeId) -> Result {
        let locant = self.attribute(id, LOCANT_ATR);
        let mut value = self.attr(id, VALUE_ATR);
        let cis_trans = self.attr(id, TYPE_ATR) == CISORTRANS_TYPE_VAL;
        if cis_trans {
            value = if value.eq_ignore_ascii_case("cis") {
                "Z".into()
            } else if value.eq_ignore_ascii_case("trans") {
                "E".into()
            } else {
                return structure(format!(
                    "Unexpected cis/trans stereochemistry type: {value}"
                ));
            }
        }
        for fragment in self.possible_fragments(id)? {
            if self.attempt_assign_stereo_bond(fragment, &value, locant.as_deref(), cis_trans)? {
                return Ok(());
            }
        }
        let parent = self.parent(id)?;
        if let Some(word) = self.arena[parent].parent.filter(|&w| {
            self.arena[w].name == WORD_EL
                && self.arena[w].attribute(TYPE_ATR) == Some("substituent")
        }) && let Some(outer) = self.arena[word].parent
        {
            for full in self
                .arena
                .children_with_attribute(outer, WORD_EL, TYPE_ATR, "full")
            {
                for g in self
                    .arena
                    .descendants_named(full, GROUP_EL)
                    .into_iter()
                    .rev()
                {
                    if self.attempt_assign_stereo_bond(
                        self.fragment(g)?,
                        &value,
                        locant.as_deref(),
                        cis_trans,
                    )? {
                        return Ok(());
                    }
                }
            }
        }
        stereo(format!(
            "Could not find bond that: {} {}",
            self.arena.to_xml(id),
            if cis_trans {
                "could refer unambiguously to"
            } else {
                "was referring to"
            }
        ))
    }
    fn attempt_assign_stereo_bond(
        &mut self,
        fragment: FragmentId,
        value: &str,
        locant: Option<&str>,
        cis_trans: bool,
    ) -> Result<bool> {
        let candidates = if let Some(locant) = locant {
            self.graph()
                .atom_by_locant(fragment, locant)
                .map(|a| self.graph().atom(a).bonds.clone())
                .unwrap_or_default()
        } else {
            let mut bonds = self.graph().fragment(fragment).bonds.clone();
            let mut inter = Vec::new();
            for &bond in self.state.fragment_manager.inter_fragment_bonds(fragment)? {
                if self.graph().atom(self.graph().bond(bond).from).fragment == fragment {
                    inter.insert(0, bond)
                } else {
                    inter.push(bond)
                }
            }
            bonds.extend(inter);
            bonds
        };
        for bond in candidates {
            if let Some(stereo_bond) = self
                .not_explicitly_defined_stereo_bond_map
                .get(&bond)
                .copied()
                && (!cis_trans || cis_trans_unambiguous_on_bond(self.graph(), bond))
            {
                apply_stereo_chemistry_to_stereo_bond(self.state.graph_mut(), stereo_bond, value)?;
                self.not_explicitly_defined_stereo_bond_map.remove(&bond);
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn assign_cis_trans_on_ring(&mut self, id: NodeId) -> Result<bool> {
        if self.arena[id].attribute(LOCANT_ATR).is_some() {
            return Ok(false);
        }
        for fragment in self.possible_fragments(id)? {
            if self.attempt_assign_cis_trans_ring(fragment, id)? {
                return Ok(true);
            }
        }
        for fragment in self.later_word_fragments(id, true)? {
            if self.attempt_assign_cis_trans_ring(fragment, id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn attempt_assign_cis_trans_ring(&mut self, fragment: FragmentId, id: NodeId) -> Result<bool> {
        let atoms = self.graph().fragment(fragment).atoms.clone();
        let mut chosen = Vec::new();
        let mut two_non_hydrogen = Vec::new();
        for &atom in &atoms {
            if self.graph().atom(atom).in_cycle {
                let neighbours = self.graph().neighbours(atom);
                if neighbours.len() == 4 {
                    let hydrogen = neighbours
                        .iter()
                        .filter(|&&n| self.graph().atom(n).element == Element::H)
                        .count();
                    let acyclic = neighbours
                        .iter()
                        .filter(|&&n| !self.graph().atom(n).in_cycle || !atoms.contains(&n))
                        .count();
                    if hydrogen == 1 || (hydrogen == 0 && acyclic == 1) {
                        chosen.push(atom)
                    } else if hydrogen == 0
                        && acyclic == 2
                        && self
                            .not_explicitly_defined_stereo_centre_map
                            .contains_key(&atom)
                    {
                        two_non_hydrogen.push(atom)
                    }
                }
            }
        }
        let mut by_cip = false;
        if chosen.len() < 2 && chosen.len() + two_non_hydrogen.len() == 2 {
            chosen.extend(two_non_hydrogen);
            by_cip = true
        }
        if chosen.len() != 2 {
            return Ok(false);
        }
        let a = chosen[0];
        let b = chosen[1];
        if self.graph().atom(a).parity.is_some() && self.graph().atom(b).parity.is_some() {
            return Ok(false);
        }
        let periphery = determine_periphery_bonds(self.graph(), fragment)?;
        let paths =
            crate::cycle_detector::paths_between_atoms_using_bonds(self.graph(), a, b, &periphery);
        if paths.len() != 2 {
            return Ok(false);
        }
        self.apply_cis_trans_ring(a, b, &paths, &atoms, &self.attr(id, VALUE_ATR), by_cip)?;
        self.not_explicitly_defined_stereo_centre_map.remove(&a);
        self.not_explicitly_defined_stereo_centre_map.remove(&b);
        if by_cip {
            self.state.add_is_ambiguous("Ring cis/trans applied to stereocenter where no hydrogen was present. Cahn-Ingold-Prelog rules used to determine which substituents are cis/trans, but other conventions may be in use")
        }
        Ok(true)
    }
    fn ring_atom_references(
        &self,
        atom: AtomId,
        other: AtomId,
        paths: &[Vec<AtomId>],
        fragment_atoms: &[AtomId],
        by_cip: bool,
        first: bool,
    ) -> Result<[AtomId; 4]> {
        let path_atom = |path: &Vec<AtomId>| {
            if first {
                path.first().copied().unwrap_or(other)
            } else {
                path.last().copied().unwrap_or(other)
            }
        };
        let p = path_atom(&paths[0]);
        let q = path_atom(&paths[1]);
        if p == q {
            return structure("OPSIN Bug: cannot assign cis/trans on ring stereochemistry");
        }
        let mut neighbours = self.graph().neighbours(atom);
        neighbours.retain(|&a| a != p && a != q);
        let chosen = if by_cip {
            CipSequenceRules::new(self.graph(), atom)
                .get_neighbouring_atoms_in_cip_order()
                .ok()
                .and_then(|ordered| ordered.into_iter().find(|a| neighbours.contains(a)))
        } else {
            neighbours
                .iter()
                .copied()
                .find(|&a| self.graph().atom(a).element == Element::H)
                .or_else(|| {
                    neighbours
                        .iter()
                        .copied()
                        .find(|&a| !self.graph().atom(a).in_cycle || !fragment_atoms.contains(&a))
                })
        };
        let Some(chosen) = chosen else {
            return structure("OPSIN Bug: cannot assign cis/trans on ring stereochemistry");
        };
        neighbours.retain(|&a| a != chosen);
        let Some(&remaining) = neighbours.first() else {
            return structure("OPSIN Bug: cannot assign cis/trans on ring stereochemistry");
        };
        Ok([remaining, chosen, p, q])
    }
    fn apply_cis_trans_ring(
        &mut self,
        a: AtomId,
        b: AtomId,
        paths: &[Vec<AtomId>],
        fragment_atoms: &[AtomId],
        value: &str,
        by_cip: bool,
    ) -> Result {
        let a_refs = self.ring_atom_references(a, b, paths, fragment_atoms, by_cip, true)?;
        let b_refs = self.ring_atom_references(b, a, paths, fragment_atoms, by_cip, false)?;
        let a_refs = atom_references(a_refs);
        let b_refs = atom_references(b_refs);
        let mut enantiomer = false;
        if let Some(parity) = &self.graph().atom(a).parity {
            if !check_equivalency_of_atom_refs_and_parity(
                &a_refs,
                1,
                &parity.atom_refs,
                parity.parity,
            ) {
                enantiomer = true
            }
        } else if let Some(parity) = &self.graph().atom(b).parity {
            let expected = match value {
                "cis" => Some(-1),
                "trans" => Some(1),
                _ => None,
            };
            if expected.is_some_and(|expected| {
                !check_equivalency_of_atom_refs_and_parity(
                    &b_refs,
                    expected,
                    &parity.atom_refs,
                    parity.parity,
                )
            }) {
                enantiomer = true
            }
        }
        match value {
            "cis" => {
                self.state.graph_mut().atom_mut(a).parity =
                    Some(AtomParity::new(a_refs, if enantiomer { -1 } else { 1 }));
                self.state.graph_mut().atom_mut(b).parity =
                    Some(AtomParity::new(b_refs, if enantiomer { 1 } else { -1 }))
            }
            "trans" => {
                let parity = if enantiomer { -1 } else { 1 };
                self.state.graph_mut().atom_mut(a).parity = Some(AtomParity::new(a_refs, parity));
                self.state.graph_mut().atom_mut(b).parity = Some(AtomParity::new(b_refs, parity))
            }
            _ => {} // Upstream grammar guarantees a lowercase cis/trans value.
        }
        Ok(())
    }
    fn assign_alpha_beta_xi_stereochem(&mut self, id: NodeId) -> Result {
        let parent = self.parent(id)?;
        let substituent = if self.arena[parent].name == SUBSTITUENT_EL {
            self.arena
                .first_child_named(parent, GROUP_EL)
                .map(|g| self.fragment(g))
                .transpose()?
        } else {
            None
        };
        let locant = self.attr(id, LOCANT_ATR);
        let value = self.attr(id, VALUE_ATR);
        for fragment in self.possible_fragments(id)? {
            if let Some(atom) = self
                .graph()
                .atom_by_locant(fragment, &locant)
                .filter(|a| self.atom_stereo_centre_map.contains_key(a))
            {
                if value == "xi" {
                    self.state.graph_mut().atom_mut(atom).parity = None
                } else {
                    let Some(order) =
                        self.fragment_attribute(fragment, ALPHABETACLOCKWISEATOMORDERING_ATR)
                    else {
                        return structure(
                            "Identified fragment is not known to be able to support alpha/beta stereochemistry",
                        );
                    };
                    self.apply_alpha_beta(atom, fragment, &order, &value, substituent)?;
                }
                self.not_explicitly_defined_stereo_centre_map.remove(&atom);
                return Ok(());
            }
        }
        structure(format!(
            "Could not find atom that: {} appeared to be referring to",
            self.arena.to_xml(id)
        ))
    }
    fn apply_alpha_beta(
        &mut self,
        atom: AtomId,
        fragment: FragmentId,
        order: &str,
        value: &str,
        substituent: Option<FragmentId>,
    ) -> Result {
        let order: Vec<_> = order.split('/').collect();
        let locant = self
            .graph()
            .atom(atom)
            .locants
            .first()
            .cloned()
            .unwrap_or_default();
        let position = order.iter().position(|&s| s == locant);
        if !self.graph().atom(atom).in_cycle || position.is_none() {
            return structure("Unsupported stereocentre type for alpha/beta stereochemistry");
        }
        let mut neighbours = self.graph().neighbours(atom);
        if neighbours.len() != 4 {
            return structure("Unsupported stereocentre type for alpha/beta stereochemistry");
        }
        let position = position.unwrap();
        let prev = if position == 0 {
            order.len() - 1
        } else {
            position - 1
        };
        let next = if position + 1 == order.len() {
            0
        } else {
            position + 1
        };
        let lookup = |loc: &str| {
            self.graph().atom_by_locant(fragment, loc).ok_or_else(|| {
                StereochemistryError::StructureBuilding(format!(
                    "Unable to find atom with locant {loc}"
                ))
            })
        };
        let previous = lookup(order[prev])?;
        let next = lookup(order[next])?;
        neighbours.retain(|&n| n != previous && n != next);
        if neighbours.len() != 2 {
            return structure("Unsupported stereocentre type for alpha/beta stereochemistry");
        }
        let a = neighbours[0];
        let b = neighbours[1];
        let in_order = |a: AtomId| {
            self.graph().fragment(fragment).atoms.contains(&a)
                && self
                    .graph()
                    .atom(a)
                    .locants
                    .first()
                    .is_some_and(|loc| order.contains(&loc.as_str()))
        };
        let (second, third) = if in_order(a) {
            (a, b)
        } else if in_order(b)
            || (self.graph().atom(a).element == Element::H
                && self.graph().atom(b).element != Element::H)
        {
            (b, a)
        } else if (self.graph().atom(b).element == Element::H
            && self.graph().atom(a).element != Element::H)
            || substituent
                .is_some_and(|s| s != fragment && self.graph().fragment(s).atoms.contains(&a))
        {
            (a, b)
        } else if substituent
            .is_some_and(|s| s != fragment && self.graph().fragment(s).atoms.contains(&b))
        {
            (b, a)
        } else {
            return structure(format!(
                "alpha/beta stereochemistry could not be determined at position {locant}"
            ));
        };
        let previous_parity = self.graph().atom(atom).parity.clone();
        let parity = match value {
            "alpha" => 1,
            "beta" => -1,
            _ => return structure("OPSIN Bug: malformed alpha/beta stereochemistry value"),
        };
        let new_parity = AtomParity::new(atom_references([previous, second, third, next]), parity);
        self.state.graph_mut().atom_mut(atom).parity = Some(new_parity.clone());
        if !self
            .not_explicitly_defined_stereo_centre_map
            .contains_key(&atom)
            && !previous_parity.as_ref().is_some_and(|old| {
                check_equivalency_of_atom_refs_and_parity(
                    &old.atom_refs,
                    old.parity,
                    &new_parity.atom_refs,
                    new_parity.parity,
                )
            })
        {
            return structure(format!(
                "contradictory alpha/beta stereochemistry at position {locant}"
            ));
        }
        Ok(())
    }
    fn process_carbohydrate_stereochemistry(&mut self, elements: &[NodeId]) -> Result {
        let mut groups: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        for &id in elements {
            let Some(group) = self.arena.next_sibling_named(id, GROUP_EL).filter(|&g| {
                [
                    SYSTEMATICCARBOHYDRATESTEMALDOSE_SUBTYPE_VAL,
                    SYSTEMATICCARBOHYDRATESTEMKETOSE_SUBTYPE_VAL,
                ]
                .contains(&self.attr(g, SUBTYPE_ATR).as_str())
            }) else {
                return structure(
                    "OPSIN bug: Could not find carbohydrate chain stem to apply stereochemistry to",
                );
            };
            groups.entry(group).or_default().push(id)
        }
        for (group, elements) in groups {
            self.assign_carbohydrate_prefix_stereochem(group, &elements)?
        }
        Ok(())
    }
    fn assign_carbohydrate_prefix_stereochem(
        &mut self,
        group: NodeId,
        elements: &[NodeId],
    ) -> Result {
        let fragment = self.fragment(group)?;
        let atoms = &self.graph().fragment(fragment).atoms;
        let mut centres = self
            .not_explicitly_defined_stereo_centre_map
            .keys()
            .copied()
            .filter(|a| atoms.contains(a) && !self.graph().atom(*a).properties.is_anomeric)
            .collect::<Vec<_>>();
        let configurations = elements
            .iter()
            .rev()
            .flat_map(|&id| {
                self.attr(id, VALUE_ATR)
                    .split('/')
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if centres.len() != configurations.len() {
            return structure(format!(
                "Disagreement between number of stereocentres on carbohydrate: {} and centres defined by configurational prefixes: {}",
                centres.len(),
                configurations.len()
            ));
        }
        centres
            .sort_by(|&a, &b| crate::fragment_tools::compare_atoms_by_locants(self.graph(), a, b));
        for (atom, configuration) in centres.into_iter().zip(configurations) {
            match configuration.as_str() {
                "r" | "l" => {
                    let Some(parity) = &mut self.state.graph_mut().atom_mut(atom).parity else {
                        return structure(
                            "OPSIN bug: stereochemistry was not defined on a carbohydrate stem, but it should been",
                        );
                    };
                    if configuration == "l" {
                        parity.parity = -parity.parity
                    }
                }
                "?" => self.state.graph_mut().atom_mut(atom).parity = None,
                _ => {
                    return structure(format!(
                        "OPSIN bug: unexpected carbohydrate stereochemistry configuration: {configuration}"
                    ));
                }
            }
            self.not_explicitly_defined_stereo_centre_map.remove(&atom);
        }
        Ok(())
    }
    fn assign_dl_stereochem(&mut self, id: NodeId) -> Result {
        let value = self.attr(id, VALUE_ATR);
        if let Some(g) = self
            .arena
            .next_sibling_ignoring(id, &[STEREOCHEMISTRY_EL])
            .filter(|&g| self.arena[g].name == GROUP_EL)
            && self.attempt_assign_dl_stereo(self.fragment(g)?, &value)?
        {
            return Ok(());
        }
        for fragment in self.possible_fragments(id)? {
            if self.attempt_assign_dl_stereo(fragment, &value)? {
                return Ok(());
            }
        }
        stereo(format!(
            "Could not find stereocentre to apply {} stereochemistry to",
            value.to_uppercase()
        ))
    }
    fn attempt_assign_dl_stereo(&mut self, fragment: FragmentId, value: &str) -> Result<bool> {
        let atoms = self.graph().fragment(fragment).atoms.clone();
        for atom in atoms {
            if self
                .not_explicitly_defined_stereo_centre_map
                .contains_key(&atom)
                && self.graph().atom(atom).bonds.len() == 4
            {
                let mut acid = None;
                let mut amine = None;
                let mut chain = None;
                let mut hydrogen = None;
                for neighbour in self.graph().neighbours(atom) {
                    match self.graph().atom(neighbour).element {
                        Element::H => hydrogen = Some(neighbour),
                        Element::C => {
                            if self
                                .graph()
                                .neighbours(neighbour)
                                .iter()
                                .any(|&a| self.graph().atom(a).element.is_chalcogen())
                            {
                                acid = Some(neighbour)
                            } else {
                                chain = Some(neighbour)
                            }
                        }
                        Element::O | Element::N => amine = Some(neighbour),
                        _ => {}
                    }
                }
                if let (Some(acid), Some(chain), Some(amine), Some(hydrogen)) =
                    (acid, chain, amine, hydrogen)
                {
                    let parity = match value {
                        "l" | "ls" => -1,
                        "d" | "ds" | "dl" => 1,
                        _ => {
                            return structure(format!(
                                "OPSIN bug: Unexpected value for D/L stereochemistry found: {value}"
                            ));
                        }
                    };
                    self.state.graph_mut().atom_mut(atom).parity = Some(AtomParity::new(
                        atom_references([acid, chain, amine, hydrogen]),
                        parity,
                    ));
                    if value == "dl" {
                        self.state.racemic_group_count += 1;
                        self.set_stereo_group(
                            atom,
                            StereoGroupType::Racemic,
                            self.state.racemic_group_count,
                        )
                    }
                    self.not_explicitly_defined_stereo_centre_map.remove(&atom);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

pub fn apply_stereo_chemistry_to_stereo_centre(
    graph: &mut Graph,
    centre: StereoCentre,
    value: &str,
) -> Result {
    let atoms = centre.get_cip_ordered_atoms(graph)?;
    if atoms.len() != 4 {
        return structure("Only tetrahedral chirality is currently supported");
    }
    let references = atom_references([atoms[3], atoms[0], atoms[1], atoms[2]]);
    let parity = match value {
        "R" => -1,
        "S" => 1,
        _ => return structure(format!("Unexpected stereochemistry type: {value}")),
    };
    graph.atom_mut(centre.atom).parity = Some(AtomParity::new(references, parity));
    Ok(())
}
pub fn apply_stereo_chemistry_to_stereo_bond(
    graph: &mut Graph,
    bond: StereoBond,
    value: &str,
) -> Result {
    let references = bond.get_ordered_stereo_atoms(graph)?;
    graph.bond_mut(bond.bond).stereo = match value {
        "E" => Some(BondStereo {
            atom_refs: references,
            value: BondStereoValue::Trans,
        }),
        "Z" => Some(BondStereo {
            atom_refs: references,
            value: BondStereoValue::Cis,
        }),
        "EZ" => None,
        _ => return structure(format!("Unexpected stereochemistry type: {value}")),
    };
    Ok(())
}
pub fn cis_trans_unambiguous_on_bond(graph: &Graph, bond: BondId) -> bool {
    let bond = graph.bond(bond);
    [bond.from, bond.to].iter().all(|&a| {
        graph
            .neighbours(a)
            .iter()
            .any(|&n| graph.atom(n).element == Element::H)
    })
}
fn atom_references(atoms: [AtomId; 4]) -> [Option<StereoReference>; 4] {
    atoms.map(|a| Some(StereoReference::Atom(a)))
}

pub fn swaps_required_to_sort(references: &[Option<StereoReference>; 4]) -> usize {
    let key = |a: Option<StereoReference>| match a {
        Some(StereoReference::Atom(id)) => id.0 + 1,
        _ => 0,
    };
    let mut swaps = 0;
    let mut copy = *references;
    for i in (0..copy.len()).rev() {
        let mut swapped = false;
        for j in 0..i {
            if key(copy[j]) > key(copy[j + 1]) {
                copy.swap(j, j + 1);
                swaps += 1;
                swapped = true
            }
        }
        if !swapped {
            break;
        }
    }
    swaps
}
pub fn check_equivalency_of_atom_refs_and_parity(
    a: &[Option<StereoReference>; 4],
    a_parity: i8,
    b: &[Option<StereoReference>; 4],
    b_parity: i8,
) -> bool {
    let mut a_swaps = swaps_required_to_sort(a);
    let b_swaps = swaps_required_to_sort(b);
    if (a_parity < 0 && b_parity > 0) || (a_parity > 0 && b_parity < 0) {
        a_swaps += 1
    }
    a_swaps % 2 == b_swaps % 2
}
fn determine_periphery_bonds(graph: &Graph, fragment: FragmentId) -> Result<Vec<BondId>> {
    let mut rings = crate::fused_ring_numberer::get_set_of_smallest_rings(graph, fragment)?;
    crate::fused_ring_numberer::setup_adjacent_fused_ring_properties(&mut rings);
    let mut bonds = Vec::new();
    for ring in &rings {
        for &bond in &ring.bonds {
            if !bonds.contains(&bond) {
                bonds.push(bond)
            }
        }
    }
    for ring in &rings {
        for &(bond, _) in &ring.neighbours {
            bonds.retain(|&b| b != bond)
        }
    }
    Ok(bonds)
}
