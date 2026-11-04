//! Application of OPSIN stereochemical descriptors to the completed graph.
//! Port of `StereochemistryHandler.java`, OPSIN 2.9.0,
//! b91b610af5ab07560fedb20730d7aef46bb2bca0; MIT, Daniel Lowe and contributors.
//! Compound locant stereobonds: upstream main, 42e87ac6.
//! Hydrogen neighbours must be physical atoms before this pass. A lone pair
//! is represented by the centre itself, as in the upstream StereoAnalyser.

use crate::WarningKind;
use crate::build_state::BuildState;
use crate::cip::{CipOrderingError, CipSequenceRules};
use crate::graph::{
    AtomId, AtomParity, AxialStereo, BondId, BondStereo, BondStereoValue, Element, FragmentId,
    Graph, GraphError, StereoGroup, StereoGroupType, StereoReference,
};
use crate::parse_tree::{Arena, NodeId};
use crate::stereo_analyser::{StereoBond, StereoCentre};
use crate::xml_declarations::*;
use reshiki_chemistry::stereo::{axis_ends_distinct, cip::label};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

/// Numbered [2.2.1] or [3.2.1] cage. Coordinates are only a
/// parity reference: bridgeheads lie on x, long bridges at +y/-y, short at +z.
struct BicyclicCage {
    atoms: Vec<AtomId>,
    points: Vec<[f64; 3]>,
    bridgehead: usize,
}

/// Template parities for the cage frame: both bridgeheads and every atom
/// carrying an endo/exo/syn/anti descriptor. Other cage atoms are independent
/// unless an explicit stereo group correlates them with the frame.
#[derive(Default)]
struct CageStereochemistry {
    atoms: Vec<AtomId>,
    parities: BTreeMap<AtomId, AtomParity>,
    group: Option<StereoGroupType>,
}

impl BicyclicCage {
    fn find(graph: &Graph, fragment: FragmentId) -> Option<Self> {
        for longest in [2, 3] {
            let count = longest + 5;
            let Some(atoms) = (1..=count)
                .map(|n| graph.atom_by_locant(fragment, &n.to_string()))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let bridgehead = longest + 1;
            let mut edges = BTreeSet::new();
            for i in 0..count - 2 {
                edges.insert((i, i + 1));
            }
            edges.insert((0, count - 2));
            edges.insert((0, count - 1));
            edges.insert((bridgehead, count - 1));
            let valid = (0..count).all(|i| {
                (i + 1..count).all(|j| {
                    let bond = graph.bond_between(atoms[i], atoms[j]);
                    if edges.contains(&(i, j)) {
                        bond.is_some()
                    } else {
                        bond.is_none()
                    }
                })
            });
            if !valid {
                continue;
            }
            // An extra path between cage atoms would make a different polycycle.
            let mut visited: BTreeSet<_> = atoms.iter().copied().collect();
            let mut extra_path = false;
            for &atom in &atoms {
                for neighbour in graph.neighbours(atom) {
                    if !visited.insert(neighbour) {
                        continue;
                    }
                    let mut stack = vec![neighbour];
                    let mut attachments = BTreeSet::new();
                    while let Some(current) = stack.pop() {
                        for next in graph.neighbours(current) {
                            if atoms.contains(&next) {
                                attachments.insert(next);
                            } else if visited.insert(next) {
                                stack.push(next);
                            }
                        }
                    }
                    if attachments.len() > 1 {
                        extra_path = true
                    }
                }
            }
            if extra_path {
                continue;
            }
            let mut points = vec![[0.; 3]; count];
            points[0] = [-1., 0., 0.];
            points[bridgehead] = [1., 0., 0.];
            for (i, point) in points.iter_mut().enumerate().take(bridgehead).skip(1) {
                let x = -1. + 2. * i as f64 / bridgehead as f64;
                *point = [x, 1. - x * x, 0.];
            }
            points[bridgehead + 1] = [1. / 3., -8. / 9., 0.];
            points[bridgehead + 2] = [-1. / 3., -8. / 9., 0.];
            points[count - 1] = [0., 0., 1.];
            return Some(Self {
                atoms,
                points,
                bridgehead,
            });
        }
        None
    }
    fn substituent(&self, graph: &Graph, index: usize) -> Option<AtomId> {
        if index == 0 || index == self.bridgehead {
            return None;
        }
        let external: Vec<_> = graph
            .neighbours(self.atoms[index])
            .into_iter()
            .filter(|a| !self.atoms.contains(a))
            .collect();
        if external.len() != 2 {
            return None;
        }
        let heavy: Vec<_> = external
            .into_iter()
            .filter(|a| graph.atom(*a).element != Element::H)
            .collect();
        (heavy.len() == 1).then(|| heavy[0])
    }
    fn parity(
        &self,
        graph: &Graph,
        index: usize,
        substituent: Option<(AtomId, f64)>,
    ) -> Result<AtomParity> {
        let atom = self.atoms[index];
        let neighbours = graph.neighbours(atom);
        let references: [AtomId; 4] = neighbours.clone().try_into().map_err(|_| {
            StereochemistryError::Uninterpretable(
                "Cage stereochemistry requires tetrahedral atoms".into(),
            )
        })?;
        let points: Vec<_> = neighbours
            .into_iter()
            .map(|neighbour| {
                if let Some(i) = self.atoms.iter().position(|&a| a == neighbour) {
                    return self.points[i];
                }
                let mut point = self.points[index];
                if let Some((target, direction)) = substituent {
                    let axis = if index == self.atoms.len() - 1 { 1 } else { 2 };
                    point[axis] += if neighbour == target {
                        direction
                    } else {
                        -direction
                    };
                } else {
                    point[0] += if index == 0 { -1. } else { 1. };
                }
                point
            })
            .collect();
        let delta =
            |i: usize| std::array::from_fn::<_, 3, _>(|axis| points[i][axis] - points[3][axis]);
        let [a, b, c] = [delta(0), delta(1), delta(2)];
        let determinant = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
        Ok(AtomParity::new(
            atom_references(references),
            if determinant > 0. { 1 } else { -1 },
        ))
    }
}

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

struct UnknownStereoCentre {
    atom: AtomId,
    value: String,
    alpha_beta: bool,
}

pub struct StereochemistryHandler<'a> {
    pub state: &'a mut BuildState,
    pub arena: &'a mut Arena,
    pub atom_stereo_centre_map: BTreeMap<AtomId, StereoCentre>,
    pub bond_stereo_bond_map: BTreeMap<BondId, StereoBond>,
    pub not_explicitly_defined_stereo_centre_map: BTreeMap<AtomId, StereoCentre>,
    pub not_explicitly_defined_stereo_bond_map: BTreeMap<BondId, StereoBond>,
    pseudo_stereo_centres: Vec<(AtomId, String)>,
    preassigned_pseudo_stereo_centres: Vec<(AtomId, String)>,
    cage_stereo_groups: BTreeMap<AtomId, CageStereochemistry>,
    unknown_stereo_centres: Vec<UnknownStereoCentre>,
    unknown_stereo_reservations: BTreeSet<AtomId>,
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
            pseudo_stereo_centres: Vec::new(),
            preassigned_pseudo_stereo_centres: Vec::new(),
            cage_stereo_groups: BTreeMap::new(),
            unknown_stereo_centres: Vec::new(),
            unknown_stereo_reservations: BTreeSet::new(),
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
        // Descriptor-set prefixes, starred R/S and RS/SR have already been
        // normalized to Rel/Rac attributes; standalone qualifiers keep a type.
        let mut requested_group_kinds = BTreeSet::new();
        for &id in elements {
            let element = &self.arena[id];
            if element.attribute(TYPE_ATR) == Some(RAC_TYPE_VAL)
                || element.attribute(STEREOGROUP_ATR) == Some("Rac")
            {
                requested_group_kinds.insert(StereoGroupType::Racemic);
            }
            if element.attribute(TYPE_ATR) == Some(REL_TYPE_VAL)
                || element.attribute(STEREOGROUP_ATR) == Some("Rel")
            {
                requested_group_kinds.insert(StereoGroupType::Relative);
            }
        }
        let mut locanted = Vec::new();
        let mut unlocanted = Vec::new();
        let mut carbohydrate = Vec::new();
        let mut global = Vec::new();
        let mut pseudo = Vec::new();
        let mut cage = Vec::new();
        let mut unlocanted_unknown = Vec::new();
        for &id in elements {
            if self.attr(id, TYPE_ATR) == ENDO_EXO_SYN_ANTI_TYPE_VAL {
                cage.push(id)
            } else if self.attr(id, TYPE_ATR) == ALPHA_OR_BETA_TYPE_VAL
                && matches!(self.attr(id, VALUE_ATR).as_str(), "xi" | "Xi" | "ξ")
                && self.arena[id].attribute(LOCANT_ATR).is_none()
            {
                unlocanted_unknown.push(id)
            } else if self.attr(id, TYPE_ATR) == R_OR_S_TYPE_VAL
                && matches!(self.attr(id, VALUE_ATR).as_str(), "r" | "s")
            {
                pseudo.push(id)
            } else if self.arena[id].attribute(LOCANT_ATR).is_some() {
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
        let has_cage_descriptor = !cage.is_empty();
        for id in cage {
            let result = self.match_stereochemistry(id);
            self.warning_or_error(result)?
        }
        for (atom, mut cage) in std::mem::take(&mut self.cage_stereo_groups) {
            let result = self.resolve_cage_stereochemistry(&mut cage);
            if result.is_ok() {
                self.cage_stereo_groups.insert(atom, cage);
            }
            self.warning_or_error(result)?;
        }
        // Enantiomorphic ligand priorities depend on the true R/S centres and,
        // for achiral ring pairs, on the other lowercase descriptor. Seed all
        // lowercase centres before resolving any of them with full CIP.
        for id in pseudo {
            let result = self.match_stereochemistry(id);
            self.warning_or_error(result)?
        }
        let result = self.resolve_pseudo_stereo_centres();
        if result.is_err() {
            for (atom, _) in &self.pseudo_stereo_centres {
                self.state.graph_mut().atom_mut(*atom).parity = None;
            }
        }
        self.warning_or_error(result)?;
        self.apply_cage_stereo_groups()?;
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
        // The case of Unicode xi depends on the complete stereochemical
        // environment, including lowercase r/s descriptors.
        for id in unlocanted_unknown {
            let result = self.match_stereochemistry(id);
            self.warning_or_error(result)?
        }
        self.resolve_unknown_stereo_centres()?;
        if has_cage_descriptor
            && !requested_group_kinds.is_empty()
            && !self.cage_scope_is_single_group(&requested_group_kinds)
        {
            self.warning_or_error(stereo(
                "rel-/rac- outside an endo/exo cage's own scope is not supported",
            ))?;
        }
        self.cage_stereo_groups.clear();
        Ok(())
    }
    fn cage_scope_is_single_group(
        &self,
        requested_group_kinds: &BTreeSet<StereoGroupType>,
    ) -> bool {
        // Axial stereo changes under reflection; E/Z bond stereo does not.
        if requested_group_kinds.len() != 1
            || self.cage_stereo_groups.len() != 1
            || self
                .graph()
                .bonds
                .iter()
                .any(|bond| bond.active && bond.axial_stereo.is_some())
        {
            return false;
        }
        let cage = self.cage_stereo_groups.values().next().unwrap();
        let mut group = None;
        self.graph()
            .atoms
            .iter()
            .filter(|atom| atom.active)
            .all(|atom| {
                let Some(parity) = &atom.parity else {
                    return true;
                };
                if !cage.atoms.contains(&atom.id) {
                    return false;
                }
                requested_group_kinds.contains(&parity.stereo_group.kind)
                    && *group.get_or_insert(parity.stereo_group) == parity.stereo_group
            })
            && group.is_some()
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
            AXIAL_TYPE_VAL=>self.assign_axial_stereo(id)?,
            CISORTRANS_TYPE_VAL=>{if !self.assign_cis_trans_on_ring(id)?{self.assign_stereo_bond(id)?}},
            ALPHA_OR_BETA_TYPE_VAL=>self.assign_alpha_beta_xi_stereochem(id)?,DLSTEREOCHEMISTRY_TYPE_VAL=>self.assign_dl_stereochem(id)?,
            RAC_TYPE_VAL=>self.apply_global_rac_or_rel(id,StereoGroupType::Racemic)?,REL_TYPE_VAL=>self.apply_global_rac_or_rel(id,StereoGroupType::Relative)?,
            ENDO_EXO_SYN_ANTI_TYPE_VAL=>self.assign_cage_stereochemistry(id)?,
            RELATIVECISTRANS_TYPE_VAL=>return stereo(format!("{kind} stereochemistry is not currently interpretable by OPSIN")),
            OPTICALROTATION_TYPE_VAL=>self.state.add_warning(WarningKind::StereochemistryIgnored,format!("Optical rotation cannot be algorithmically used to assign stereochemistry. This term was ignored: {}",self.arena.value(id))),
            _=>return structure(format!("Unexpected stereochemistry type: {kind}"))
        }
        self.arena.detach(id);
        Ok(())
    }
    /// Collect all relative descriptors in a common template frame before
    /// existing parities on that frame choose its mirror.
    fn assign_cage_stereochemistry(&mut self, id: NodeId) -> Result {
        let value = self.attr(id, VALUE_ATR);
        let locant = self.attribute(id, LOCANT_ATR);
        let fragments = if locant.is_some() {
            self.cage_owning_fragment(id)?.into_iter().collect()
        } else {
            let mut fragments = self.possible_fragments(id)?;
            fragments.extend(self.later_word_fragments(id, true)?);
            fragments
        };
        for fragment in fragments {
            let Some(cage) = BicyclicCage::find(self.graph(), fragment) else {
                continue;
            };
            let target = if let Some(ref locant) = locant {
                self.graph()
                    .atom_by_locant(fragment, locant)
                    .and_then(|atom| cage.atoms.iter().position(|&a| a == atom))
            } else {
                let candidates: Vec<_> = (0..cage.atoms.len())
                    .filter(|&i| cage.substituent(self.graph(), i).is_some())
                    .collect();
                (candidates.len() == 1).then(|| candidates[0])
            };
            let Some(target) = target else { continue };
            let Some(substituent) = cage.substituent(self.graph(), target) else {
                return stereo("Cage descriptor requires one unambiguous substituent on a bridge");
            };
            let short_bridge = target == cage.atoms.len() - 1;
            let direction = match (short_bridge, value.as_str()) {
                (false, "exo") | (true, "syn") => 1.,
                (false, "endo") | (true, "anti") => -1.,
                _ => {
                    return stereo(
                        "Endo/exo applies to the longer bridges; syn/anti applies to the shortest bridge",
                    );
                }
            };
            let mut parities = Vec::new();
            for bridgehead in [0, cage.bridgehead] {
                parities.push((
                    cage.atoms[bridgehead],
                    cage.parity(self.graph(), bridgehead, None)?,
                ));
            }
            parities.push((
                cage.atoms[target],
                cage.parity(self.graph(), target, Some((substituent, direction)))?,
            ));
            let explicit_group = match self.attribute(id, STEREOGROUP_ATR).as_deref() {
                Some("Rac") => Some(StereoGroupType::Racemic),
                Some("Rel") => Some(StereoGroupType::Relative),
                _ => None,
            };
            let pending = self.cage_stereo_groups.entry(cage.atoms[0]).or_default();
            for (atom, proposed) in &parities {
                if let Some(previous) = pending.parities.get(atom)
                    && !check_equivalency_of_atom_refs_and_parity(
                        &previous.atom_refs,
                        previous.parity,
                        &proposed.atom_refs,
                        proposed.parity,
                    )
                {
                    return stereo("Contradictory cage stereochemistry");
                }
            }
            pending.parities.extend(parities);
            pending.atoms = cage.atoms;
            pending.group = explicit_group.or(pending.group);
            return Ok(());
        }
        stereo(
            "Endo/exo/syn/anti is supported only for substituted bicyclo[2.2.1]heptane and bicyclo[3.2.1]octane cages",
        )
    }
    fn resolve_cage_stereochemistry(&mut self, cage: &mut CageStereochemistry) -> Result {
        let mut mirror = None;
        let existing_group = cage
            .parities
            .keys()
            .filter_map(|&atom| self.graph().atom(atom).parity.as_ref())
            .map(|parity| parity.stereo_group.kind)
            .max_by_key(|kind| *kind == StereoGroupType::Absolute);
        for (&atom, proposed) in &cage.parities {
            if let Some(existing) = &self.graph().atom(atom).parity {
                let alignment = if check_equivalency_of_atom_refs_and_parity(
                    &existing.atom_refs,
                    existing.parity,
                    &proposed.atom_refs,
                    proposed.parity,
                ) {
                    1
                } else {
                    -1
                };
                if mirror.is_some_and(|previous| previous != alignment) {
                    return stereo("Contradictory cage stereochemistry");
                }
                mirror = Some(alignment);
            }
        }
        cage.group = if existing_group == Some(StereoGroupType::Absolute) {
            existing_group
        } else {
            cage.group.or(existing_group)
        };
        for (&atom, proposed) in &cage.parities {
            if self.graph().atom(atom).parity.is_none() {
                let mut parity = proposed.clone();
                parity.parity *= mirror.unwrap_or(1);
                self.state.graph_mut().atom_mut(atom).parity = Some(parity);
            }
            self.atom_stereo_centre_map
                .entry(atom)
                .or_insert(StereoCentre {
                    atom,
                    true_stereo_centre: false,
                });
            self.not_explicitly_defined_stereo_centre_map.remove(&atom);
        }
        Ok(())
    }
    fn cage_owning_fragment(&self, id: NodeId) -> Result<Option<FragmentId>> {
        let mut scope = self.parent(id)?;
        if self.arena[scope].attribute(LOCANT_ATR).is_none()
            && let Some(group) = self.arena.first_child_named(scope, GROUP_EL)
        {
            return self.fragment(group).map(Some);
        }
        while let Some(parent) = self.arena[scope].parent {
            let index = self.arena.index_of(parent, scope).unwrap();
            if let Some(mut target) = self.arena[parent].children[index + 1..]
                .iter()
                .rev()
                .copied()
                .find(|&node| {
                    matches!(
                        self.arena[node].name.as_str(),
                        ROOT_EL | SUBSTITUENT_EL | BRACKET_EL
                    )
                })
            {
                while self.arena[target].name == BRACKET_EL {
                    target = *self.arena[target]
                        .children
                        .iter()
                        .rev()
                        .find(|&&node| {
                            matches!(
                                self.arena[node].name.as_str(),
                                ROOT_EL | SUBSTITUENT_EL | BRACKET_EL
                            )
                        })
                        .unwrap();
                }
                let group = self.arena.first_child_named(target, GROUP_EL).unwrap();
                return self.fragment(group).map(Some);
            }
            if self.arena[parent].name == WORD_EL {
                break;
            }
            scope = parent;
        }
        Ok(None)
    }
    fn apply_cage_stereo_groups(&mut self) -> Result {
        let cages = std::mem::take(&mut self.cage_stereo_groups);
        let (mut racemic_number, mut relative_number) = (0, 0);
        for group in self
            .graph()
            .atoms
            .iter()
            .filter_map(|atom| atom.parity.as_ref().map(|parity| parity.stereo_group))
        {
            let number = match group.kind {
                StereoGroupType::Racemic => &mut racemic_number,
                StereoGroupType::Relative => &mut relative_number,
                _ => continue,
            };
            *number = (*number).max(group.number);
        }
        for cage in cages.values() {
            let mut atoms: Vec<_> = cage.parities.keys().copied().collect();
            let kind = if let Some(kind) = cage.group {
                kind
            } else {
                if !self.cage_has_distinct_mirror(&atoms)? {
                    continue;
                }
                StereoGroupType::Relative
            };
            let existing_groups: BTreeSet<_> = atoms
                .iter()
                .filter_map(|&atom| self.graph().atom(atom).parity.as_ref())
                .map(|parity| parity.stereo_group)
                .filter(|group| {
                    group.kind == kind
                        && matches!(kind, StereoGroupType::Relative | StereoGroupType::Racemic)
                })
                .collect();
            // Preserve explicit correlations across the geometric frame boundary.
            atoms.extend(cage.atoms.iter().copied().filter(|atom| {
                !cage.parities.contains_key(atom)
                    && self
                        .graph()
                        .atom(*atom)
                        .parity
                        .as_ref()
                        .is_some_and(|parity| existing_groups.contains(&parity.stereo_group))
            }));
            // Separately scoped descriptors initially reuse group number 1.
            // If it also occurs outside this cage, renumber the correlated set
            // together so independent cages and parent centres stay independent.
            let existing_number = existing_groups
                .iter()
                .next()
                .filter(|group| {
                    existing_groups.len() == 1
                        && !self.graph().atoms.iter().any(|atom| {
                            !cage.atoms.contains(&atom.id)
                                && atom
                                    .parity
                                    .as_ref()
                                    .is_some_and(|parity| parity.stereo_group == **group)
                        })
                })
                .map(|group| group.number);
            let number = existing_number.unwrap_or_else(|| match kind {
                StereoGroupType::Racemic => {
                    racemic_number += 1;
                    racemic_number
                }
                StereoGroupType::Relative => {
                    relative_number += 1;
                    relative_number
                }
                _ => 1,
            });
            for atom in atoms {
                self.set_stereo_group(atom, kind, number);
            }
        }
        self.cage_stereo_groups = cages;
        Ok(())
    }
    fn cage_has_distinct_mirror(&self, frame: &[AtomId]) -> Result<bool> {
        let mut graph = self.graph().clone();
        let fragment = graph.atom(frame[0]).fragment;
        for atom in &mut graph.atoms {
            if let Some(parity) = &mut atom.parity {
                parity.stereo_group = StereoGroup::default();
            }
        }
        let canonical = |graph: &Graph| -> Result<String> {
            let smiles = crate::smiles::write_semantic_cxsmiles(graph, fragment)
                .map_err(|error| StereochemistryError::Uninterpretable(error.to_string()))?;
            let imported = reshiki_chemistry::smiles::read(&smiles)
                .map_err(|error| StereochemistryError::Uninterpretable(error.to_string()))?;
            reshiki_chemistry::smiles::write::write(&imported.prepared.state, Default::default())
                .map(|output| output.text)
                .map_err(|error| StereochemistryError::Uninterpretable(error.to_string()))
        };
        let original = canonical(&graph)?;
        for &atom in frame {
            if let Some(parity) = &mut graph.atom_mut(atom).parity {
                parity.parity *= -1;
            }
        }
        Ok(original != canonical(&graph)?)
    }
    fn apply_global_rac_or_rel(&mut self, id: NodeId, kind: StereoGroupType) -> Result {
        // Stereo groups hold atom parities only; an absolute axis must not
        // silently stand in for a racemic or relative one.
        if self
            .graph()
            .bonds
            .iter()
            .any(|bond| bond.active && bond.axial_stereo.is_some())
        {
            return stereo("rac- or rel- axial stereochemistry is not currently interpretable");
        }
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
        let pseudo = matches!(value, "r" | "s");
        let available = |a: &AtomId| {
            self.not_explicitly_defined_stereo_centre_map
                .contains_key(a)
                || (pseudo
                    && locant.is_some()
                    && self
                        .cage_stereo_groups
                        .values()
                        .any(|cage| cage.parities.contains_key(a)))
                || (pseudo
                    && !self.atom_stereo_centre_map.contains_key(a)
                    && crate::stereo_analyser::is_possibly_stereogenic(self.graph(), *a))
        };
        let atom = if let Some(locant) = locant {
            self.graph()
                .atom_by_locant(fragment, locant)
                .filter(|a| available(a) || self.unknown_stereo_reservations.contains(a))
        } else {
            self.graph()
                .fragment(fragment)
                .atoms
                .iter()
                .copied()
                .find(available)
        };
        if let Some(atom) = atom {
            if pseudo {
                // Cage descriptors may already have fixed this centre. Check
                // its label after all lowercase centres are seeded, preserving
                // the existing parity even if the descriptor contradicts it.
                if self
                    .cage_stereo_groups
                    .values()
                    .any(|cage| cage.parities.contains_key(&atom))
                {
                    self.preassigned_pseudo_stereo_centres
                        .push((atom, value.into()));
                    return Ok(true);
                }
                let mut neighbours = self.graph().neighbours(atom);
                if neighbours.len() == 3 {
                    neighbours.push(atom);
                }
                let references: [AtomId; 4] = neighbours.try_into().map_err(|_| {
                    StereochemistryError::StructureBuilding(
                        "Only tetrahedral chirality is currently supported".into(),
                    )
                })?;
                self.state.graph_mut().atom_mut(atom).parity =
                    Some(AtomParity::new(atom_references(references), 1));
                self.atom_stereo_centre_map
                    .entry(atom)
                    .or_insert(StereoCentre {
                        atom,
                        true_stereo_centre: false,
                    });
                self.pseudo_stereo_centres.push((atom, value.into()));
            } else {
                let centre = self.atom_stereo_centre_map[&atom];
                apply_stereo_chemistry_to_stereo_centre(self.state.graph_mut(), centre, value)?;
            }
            self.set_stereo_group(atom, kind, 1);
            self.not_explicitly_defined_stereo_centre_map.remove(&atom);
            self.unknown_stereo_reservations.remove(&atom);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn resolve_pseudo_stereo_centres(&mut self) -> Result {
        for (atom, expected) in &self.pseudo_stereo_centres {
            let labels = crate::modern_cip::atom_labels(self.graph())?;
            match labels.get(atom).map(String::as_str) {
                Some(actual) if actual == expected => (),
                Some("r" | "s") => {
                    self.state
                        .graph_mut()
                        .atom_mut(*atom)
                        .parity
                        .as_mut()
                        .unwrap()
                        .parity *= -1;
                }
                _ => {
                    return stereo(format!(
                        "Could not assign pseudoasymmetric {expected} stereochemistry"
                    ));
                }
            }
        }
        if !self.pseudo_stereo_centres.is_empty()
            || !self.preassigned_pseudo_stereo_centres.is_empty()
        {
            let labels = crate::modern_cip::atom_labels(self.graph())?;
            for (atom, expected) in self
                .pseudo_stereo_centres
                .iter()
                .chain(&self.preassigned_pseudo_stereo_centres)
            {
                if labels.get(atom) != Some(expected) {
                    return stereo("Contradictory pseudoasymmetric stereochemistry");
                }
            }
        }
        Ok(())
    }
    fn assign_axial_stereo(&mut self, id: NodeId) -> Result {
        let value = self.attr(id, VALUE_ATR);
        let descriptor = match value.as_str() {
            "M" | "Ra" => "M",
            "P" | "Sa" => "P",
            _ => {
                return stereo(format!(
                    "{AXIAL_TYPE_VAL} stereochemistry is not currently interpretable by OPSIN"
                ));
            }
        };
        let Some(locant) = self.attribute(id, LOCANT_ATR) else {
            return stereo(format!(
                "{AXIAL_TYPE_VAL} stereochemistry is not currently interpretable by OPSIN"
            ));
        };
        // An axial locant belongs to the first component searched, including
        // when the descriptor is attached to an ester's substituent word.
        let Some(fragment) = self.possible_fragments(id)?.first().copied() else {
            return stereo(format!("Could not find a biaryl axis at locant {locant}"));
        };
        let Some(atom) = self.graph().atom_by_locant(fragment, &locant) else {
            return stereo(format!("Could not find a biaryl axis at locant {locant}"));
        };
        let (mut native, indices) = crate::modern_cip::state(self.graph())?;
        let rings = reshiki_chemistry::rings::perceive(
            &native.graph,
            reshiki_chemistry::rings::Options::default(),
        )
        .map_err(|error| StereochemistryError::Uninterpretable(error.to_string()))?;
        let aromatic = reshiki_chemistry::aromaticity::perceive(&native.graph, &rings.atoms)
            .map_err(StereochemistryError::Uninterpretable)?;
        let aryl_atoms: HashSet<_> = indices
            .iter()
            .filter_map(|(&atom, &index)| aromatic.graph.atoms[index].aromatic.then_some(atom))
            .collect();
        let candidates: Vec<_> = self
            .graph()
            .atom(atom)
            .bonds
            .iter()
            .copied()
            .filter(|&id| {
                let bond = self.graph().bond(id);
                bond.order == 1
                    && !crate::cycle_detector::is_bond_in_cycle(self.graph(), id)
                    && [bond.from, bond.to].into_iter().all(|end| {
                        aryl_atoms.contains(&end)
                            && self.graph().neighbours(end).len() == 3
                            && self
                                .graph()
                                .neighbours(end)
                                .into_iter()
                                .all(|other| self.graph().atom(other).element != Element::H)
                    })
            })
            .collect();
        let bond = match candidates.as_slice() {
            [] => return stereo(format!("Could not find a biaryl axis at locant {locant}")),
            [bond] => *bond,
            _ => return stereo(format!("More than one biaryl axis at locant {locant}")),
        };
        let axis = self.graph().bond(bond);
        if axis.axial_stereo.is_some() {
            return stereo(format!(
                "Duplicate or conflicting axial stereochemistry at locant {locant}"
            ));
        }
        // Use the same full CIP rules as native perception, including isotope
        // masses and resonance-averaged mancude duplicates. Its axial carriers
        // are the lowest-index non-axis neighbours, not the high-CIP ligands.
        let refs = [(axis.from, axis.to), (axis.to, axis.from)].map(|(end, partner)| {
            self.graph()
                .neighbours(end)
                .into_iter()
                .filter(|&atom| atom != partner)
                .min_by_key(|atom| indices[atom])
                .unwrap()
        });
        let native_bond = native
            .graph
            .bonds
            .iter()
            .position(|bond| bond.a == indices[&axis.from] && bond.b == indices[&axis.to])
            .unwrap();
        if !axis_ends_distinct(&native.graph, native_bond)
            .map_err(StereochemistryError::Uninterpretable)?
        {
            return stereo(format!("Axis at locant {locant} has equivalent ligands"));
        }
        native.metadata.bonds[native_bond].stereo = 6;
        let mut labelled = label::assign(&native, &Default::default())
            .map_err(|error| StereochemistryError::Uninterpretable(error.to_string()))?;
        if labelled.state.properties.bond_codes[native_bond].as_deref() != Some(descriptor) {
            native.metadata.bonds[native_bond].stereo = 7;
            labelled = label::assign(&native, &Default::default())
                .map_err(|error| StereochemistryError::Uninterpretable(error.to_string()))?;
        }
        if labelled.state.properties.bond_codes[native_bond].as_deref() != Some(descriptor) {
            return stereo(format!(
                "Could not assign {descriptor} to the axis at locant {locant}"
            ));
        }
        self.state.graph_mut().bond_mut(bond).axial_stereo = Some(AxialStereo {
            refs,
            clockwise: native.metadata.bonds[native_bond].stereo == 6,
            descriptor: descriptor.into(),
        });
        Ok(())
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
            let compound = crate::tree_tools::split_compound_locant(locant);
            let (locant, second_atom) = if let Some((ref base, compound)) = compound {
                let Some(second_atom) = self.graph().atom_by_locant(fragment, compound) else {
                    return Ok(false);
                };
                (base.as_str(), Some(second_atom))
            } else {
                (locant, None)
            };
            if let Some(first_atom) = self.graph().atom_by_locant(fragment, locant) {
                self.graph()
                    .atom(first_atom)
                    .bonds
                    .iter()
                    .copied()
                    .filter(|&bond| {
                        second_atom.is_none_or(|second| {
                            self.graph().bond(bond).other_atom(first_atom) == Some(second)
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
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
        self.unknown_stereo_reservations.remove(&a);
        self.unknown_stereo_reservations.remove(&b);
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
        // A centre configured by an earlier descriptor keeps its rel/rac group,
        // and its ring partner joins it: (1R*)-cis-2-methylcyclohexan-1-ol.
        let stereo_group = [a, b]
            .into_iter()
            .find_map(|atom| self.graph().atom(atom).parity.as_ref())
            .map_or_else(StereoGroup::default, |parity| parity.stereo_group);
        let new_parity = |atom_refs, parity| AtomParity {
            atom_refs,
            parity,
            stereo_group,
        };
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
                    Some(new_parity(a_refs, if enantiomer { -1 } else { 1 }));
                self.state.graph_mut().atom_mut(b).parity =
                    Some(new_parity(b_refs, if enantiomer { 1 } else { -1 }))
            }
            "trans" => {
                let parity = if enantiomer { -1 } else { 1 };
                self.state.graph_mut().atom_mut(a).parity = Some(new_parity(a_refs, parity));
                self.state.graph_mut().atom_mut(b).parity = Some(new_parity(b_refs, parity))
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
        let locant = self.attribute(id, LOCANT_ATR);
        let value = self.attr(id, VALUE_ATR);
        let unknown = matches!(value.as_str(), "xi" | "Xi" | "ξ");
        let fragments = self.possible_fragments(id)?;
        for &fragment in &fragments {
            let atom = if let Some(locant) = &locant {
                self.graph()
                    .atom_by_locant(fragment, locant)
                    .filter(|a| self.atom_stereo_centre_map.contains_key(a))
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
                let order = self.fragment_attribute(fragment, ALPHABETACLOCKWISEATOMORDERING_ATR);
                if unknown {
                    // Reserve the target now; clear its parity only after
                    // the other descriptors have established its environment.
                    // Steroid-style α/β positions also take lowercase ξ.
                    let alpha_beta = order.as_deref().is_some_and(|order| {
                        self.graph()
                            .atom(atom)
                            .locants
                            .first()
                            .is_some_and(|l| order.split('/').any(|o| o == l))
                    });
                    self.unknown_stereo_centres.push(UnknownStereoCentre {
                        atom,
                        value,
                        alpha_beta,
                    });
                } else {
                    let Some(order) = order else {
                        return structure(
                            "Identified fragment is not known to be able to support alpha/beta stereochemistry",
                        );
                    };
                    self.apply_alpha_beta(atom, fragment, &order, &value, substituent)?;
                }
                let available = self
                    .not_explicitly_defined_stereo_centre_map
                    .remove(&atom)
                    .is_some();
                if unknown && available {
                    self.unknown_stereo_reservations.insert(atom);
                } else if !unknown {
                    self.unknown_stereo_reservations.remove(&atom);
                }
                return Ok(());
            }
        }
        if matches!(value.as_str(), "Xi" | "xi") {
            for fragment in fragments {
                if self.attempt_assign_stereo_bond(fragment, "EZ", locant.as_deref(), false)? {
                    return Ok(());
                }
            }
        }
        if unknown {
            return stereo(format!(
                "Could not find stereocentre or bond that: {} appeared to be referring to",
                self.arena.to_xml(id)
            ));
        }
        structure(format!(
            "Could not find atom that: {} appeared to be referring to",
            self.arena.to_xml(id)
        ))
    }
    fn resolve_unknown_stereo_centres(&mut self) -> Result {
        self.unknown_stereo_reservations.clear();
        let mut centres = std::mem::take(&mut self.unknown_stereo_centres);
        let mut originals = BTreeMap::new();
        for centre in &centres {
            originals
                .entry(centre.atom)
                .or_insert_with(|| self.state.graph_mut().atom_mut(centre.atom).parity.take());
        }
        loop {
            let mut accepted = Vec::new();
            for centre in centres {
                match self.validate_unknown_stereo_centre(&centre) {
                    Ok(()) => accepted.push(centre),
                    Err(error) => self.warning_or_error(Err(error))?,
                }
            }
            let targets: BTreeSet<_> = accepted.iter().map(|centre| centre.atom).collect();
            let mut restored = false;
            originals.retain(|&atom, original| {
                if targets.contains(&atom) {
                    return true;
                }
                let original = original.take();
                restored |= original.is_some();
                self.state.graph_mut().atom_mut(atom).parity = original;
                false
            });
            if !restored {
                return Ok(());
            }
            // Restoring an ignored descriptor can change whether another
            // target is pseudoasymmetric. Recheck against that environment.
            centres = accepted;
        }
    }
    fn validate_unknown_stereo_centre(&mut self, centre: &UnknownStereoCentre) -> Result {
        let atom = centre.atom;
        if centre.value == "xi" || (centre.value == "ξ" && centre.alpha_beta) {
            return Ok(());
        }
        let mut neighbours = self.graph().neighbours(atom);
        if neighbours.len() == 3 {
            neighbours.push(atom);
        }
        let references: [AtomId; 4] = neighbours.try_into().map_err(|_| {
            StereochemistryError::StructureBuilding(
                "Only tetrahedral chirality is currently supported".into(),
            )
        })?;
        self.state.graph_mut().atom_mut(atom).parity =
            Some(AtomParity::new(atom_references(references), 1));
        let labels = crate::modern_cip::atom_labels(self.graph());
        self.state.graph_mut().atom_mut(atom).parity = None;
        labels.and_then(|labels| match labels.get(&atom).map(String::as_str) {
            Some("R" | "S") if centre.value == "ξ" => {
                stereo("Lowercase xi requires a pseudoasymmetric or alpha/beta stereocentre")
            }
            Some("r" | "s") if centre.value == "Xi" => {
                stereo("Uppercase Xi cannot specify a pseudoasymmetric stereocentre")
            }
            None => {
                // Only undefined stereocentres in tied ligands can
                // prevent CIP from distinguishing them.
                let neighbours = self.graph().neighbours(atom);
                let rules = CipSequenceRules::new(self.graph(), atom);
                let mut pending = Vec::new();
                for (i, &first) in neighbours.iter().enumerate() {
                    for &second in &neighbours[..i] {
                        match rules.compare_ligands(first, second) {
                            Err(CipOrderingError::UnresolvedTie) => {
                                pending.extend([first, second]);
                            }
                            Err(error) => return Err(error.into()),
                            Ok(_) => (),
                        }
                    }
                }
                let mut visited = BTreeSet::from([atom]);
                while let Some(other) = pending.pop() {
                    if !visited.insert(other) {
                        continue;
                    }
                    if self.atom_stereo_centre_map.contains_key(&other)
                        && self.graph().atom(other).parity.is_none()
                    {
                        return Ok(());
                    }
                    pending.extend(self.graph().neighbours(other));
                }
                stereo("Xi cannot specify a nonstereogenic atom")
            }
            _ => Ok(()),
        })
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
            && !self.unknown_stereo_reservations.contains(&atom)
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
                let mut acid_score = 0;
                let mut chain_score = 0;
                for neighbour in self.graph().neighbours(atom) {
                    match self.graph().atom(neighbour).element {
                        Element::H => hydrogen = Some(neighbour),
                        Element::C => {
                            let chalcogen_bond_order_sum: u32 = self
                                .graph()
                                .atom(neighbour)
                                .bonds
                                .iter()
                                .map(|&bond| self.graph().bond(bond))
                                .filter(|bond| {
                                    bond.other_atom(neighbour).is_some_and(|other| {
                                        other != atom
                                            && self.graph().atom(other).element.is_chalcogen()
                                    })
                                })
                                .map(|bond| u32::from(bond.order))
                                .sum();
                            if chalcogen_bond_order_sum > acid_score {
                                if acid.is_some() {
                                    chain = acid;
                                    chain_score = acid_score;
                                }
                                acid = Some(neighbour);
                                acid_score = chalcogen_bond_order_sum;
                            } else {
                                chain = Some(neighbour);
                                chain_score = chalcogen_bond_order_sum;
                            }
                        }
                        Element::O | Element::N => amine = Some(neighbour),
                        _ => {}
                    }
                }
                if let (Some(acid), Some(chain), Some(amine), Some(hydrogen)) =
                    (acid, chain, amine, hydrogen)
                    && acid_score > chain_score
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
