//! Completed-system fused-parent numbering and non-cumulative bond assignments.
use super::super::{
    chains::canonical_ranks,
    locants::{parse_system_locant, retained_locant_sort_key},
};
use super::super::{
    molecule::{AtomSet, Molecule},
    retained_graph_model::RetainedGraphTemplate,
    rules::elements,
};
use super::model::*;
use super::{
    citation_numbering::numbered_parent_graphs_agree,
    faces::{BoundedFaceModel, typed_face_model},
    layout::preferred_intrinsic_layouts,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type NumberingScore = ComparisonKey;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletedNumbering {
    pub perimeter: Vec<usize>,
    pub atom_to_locant: Vec<(usize, String)>,
    pub score: NumberingScore,
    pub layout_index: Option<usize>,
    pub start_face_id: Option<usize>,
    pub start_atom: Option<usize>,
}

impl CompletedNumbering {
    pub fn string_map(&self) -> BTreeMap<usize, String> {
        self.atom_to_locant.iter().cloned().collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompletedNumberingSelection {
    pub accepted: Vec<CompletedNumbering>,
    pub rejected: Vec<RejectedNumbering>,
}

#[cfg(test)]
pub fn completed_system_numberings(
    mol: &Molecule,
    faces: &BoundedFaceModel,
    face_model: Option<&FaceModel>,
    layouts: &[FusedLayout],
) -> Result<Vec<CompletedNumbering>, String> {
    Ok(completed_system_numbering_selection(mol, faces, face_model, layouts, false)?.accepted)
}

/// Keep all tied maps after the ordered locant criteria and construction proof.
pub fn completed_system_numbering_selection(
    mol: &Molecule,
    faces: &BoundedFaceModel,
    face_model: Option<&FaceModel>,
    layouts: &[FusedLayout],
    defer_indicated_hydrogen: bool,
) -> Result<CompletedNumberingSelection, String> {
    if layouts.is_empty() {
        return Ok(CompletedNumberingSelection {
            accepted: graph_numbering_candidates(mol, faces)?,
            rejected: Vec::new(),
        });
    }
    let face_model = face_model.ok_or_else(|| {
        "layout-derived numbering requires the corresponding typed face model".to_string()
    })?;
    let mut candidates = Vec::new();
    let mut rejected = Vec::new();
    let fusion_atoms = faces.fusion_atoms();
    let mut perimeter_cache = BTreeMap::new();
    for (layout_index, layout) in layouts.iter().enumerate() {
        let Some(mut derived) = numbering_from_layout(
            mol,
            faces,
            face_model,
            layout,
            layout_index,
            &fusion_atoms,
            &mut perimeter_cache,
        ) else {
            rejected.push(RejectedNumbering {
                orientation_score: layout.orientation_score.clone().into(),
                reason: format!(
                    "layout {layout_index} has no valid uppermost/rightmost clockwise perimeter"
                ),
            });
            continue;
        };
        if defer_indicated_hydrogen && let ComparisonKey::Tuple(parts) = &mut derived.score {
            parts.pop();
        }
        candidates.push(derived);
    }
    let Some(best_score) = candidates.iter().map(|c| &c.score).min().cloned() else {
        return Ok(CompletedNumberingSelection {
            accepted: Vec::new(),
            rejected,
        });
    };
    let mut keys = BTreeSet::new();
    let mut tied = Vec::new();
    for candidate in candidates {
        if candidate.score != best_score {
            rejected.push(RejectedNumbering {
                orientation_score: ComparisonKey::Tuple(vec![
                    layouts[candidate.layout_index.unwrap_or(0)].orientation_score.clone().into(),
                    candidate.score.clone(),
                ]),
                reason: format!(
                    "layout {} start at face {}, atom {} loses the ordered completed-system locant criteria",
                    optional_number(candidate.layout_index), optional_number(candidate.start_face_id),
                    optional_number(candidate.start_atom),
                ),
            });
            continue;
        }
        let key: Vec<_> = candidate.string_map().into_iter().collect();
        if keys.insert(key) {
            tied.push(candidate);
        }
    }
    let selected = construction_ordered_numberings(mol, layouts, &tied);
    if selected != tied {
        for candidate in &tied {
            if !selected.contains(candidate) {
                rejected.push(RejectedNumbering {
                    orientation_score: candidate.score.clone(),
                    reason: "stable construction enumeration distinguishes locant-labelled graphs"
                        .into(),
                });
            }
        }
    }
    Ok(CompletedNumberingSelection {
        accepted: selected,
        rejected,
    })
}

fn optional_number(value: Option<usize>) -> String {
    value.map_or_else(|| "None".into(), |v| v.to_string())
}

fn construction_ordered_numberings(
    mol: &Molecule,
    layouts: &[FusedLayout],
    candidates: &[CompletedNumbering],
) -> Vec<CompletedNumbering> {
    if candidates.len() < 2 {
        return candidates.to_vec();
    }
    let mut ranked = Vec::new();
    for candidate in candidates {
        let Some(layout_index) = candidate.layout_index else {
            return candidates.to_vec();
        };
        let layout = &layouts[layout_index];
        if layout.kind != LayoutKind::OpsinConstruction {
            return candidates.to_vec();
        }
        if let Some(priority) = &layout.construction_numbering_priority {
            ranked.push((priority, candidate));
        }
    }
    if ranked.is_empty() {
        return candidates.to_vec();
    }
    let maps: Vec<_> = candidates
        .iter()
        .map(CompletedNumbering::string_map)
        .collect();
    if numbered_parent_graphs_agree(mol, maps.iter()) {
        return candidates.to_vec();
    }
    let best = ranked
        .iter()
        .skip(1)
        .fold(ranked[0].0, |best, (priority, _)| best.min(*priority));
    let first: Vec<_> = ranked
        .iter()
        .filter(|(priority, _)| *priority == best)
        .map(|(_, candidate)| candidate.string_map())
        .collect();
    if !numbered_parent_graphs_agree(mol, first.iter()) {
        return candidates.to_vec();
    }
    candidates
        .iter()
        .filter(|candidate| {
            let mapping = candidate.string_map();
            numbered_parent_graphs_agree(mol, [&first[0], &mapping])
        })
        .cloned()
        .collect()
}

fn graph_numbering_candidates(
    mol: &Molecule,
    faces: &BoundedFaceModel,
) -> Result<Vec<CompletedNumbering>, String> {
    if !faces.interior_atoms().is_empty() || faces.cycle_rank < 2 {
        return Ok(Vec::new());
    }
    let face_model = typed_face_model(mol, faces);
    let layouts = preferred_intrinsic_layouts(&face_model)?;
    if layouts.is_empty() {
        return Ok(Vec::new());
    }
    let mut selection =
        completed_system_numbering_selection(mol, faces, Some(&face_model), &layouts, false)?;
    for candidate in &mut selection.accepted {
        candidate.layout_index = None;
        candidate.start_face_id = None;
        candidate.start_atom = None;
    }
    Ok(selection.accepted)
}

type PerimeterCache = BTreeMap<(Vec<usize>, Option<Vec<usize>>), Option<CompletedNumbering>>;

fn numbering_from_layout(
    mol: &Molecule,
    faces: &BoundedFaceModel,
    face_model: &FaceModel,
    layout: &FusedLayout,
    layout_index: usize,
    fusion_atoms: &AtomSet,
    perimeter_cache: &mut PerimeterCache,
) -> Option<CompletedNumbering> {
    let positions: BTreeMap<_, _> = layout
        .atom_positions
        .iter()
        .map(|&(a, x, y)| (a, (x, y)))
        .collect();
    let centers: BTreeMap<_, _> = layout
        .face_positions
        .iter()
        .map(|&(f, x, y)| (f, (x, y)))
        .collect();
    let face_by_id: BTreeMap<_, _> = face_model
        .faces
        .iter()
        .map(|face| (face.id, face))
        .collect();
    if positions.keys().copied().collect::<AtomSet>() != faces.atom_ids
        || centers.keys().collect::<BTreeSet<_>>() != face_by_id.keys().collect::<BTreeSet<_>>()
    {
        return None;
    }
    let face_order = clockwise_face_order(&centers);
    let mut start_face_id = face_order.into_iter().find(|face_id| {
        face_by_id[face_id]
            .atom_cycle
            .iter()
            .any(|atom| !fusion_atoms.contains(atom) && faces.outer_boundary.atoms.contains(atom))
    })?;
    let clockwise = clockwise_boundary(&faces.outer_boundary.atoms, &positions)?;
    let mut start_atom =
        counterclockwise_nonfusion_start(face_by_id[&start_face_id], &clockwise, fusion_atoms)?;
    let offset = clockwise.iter().position(|&atom| atom == start_atom)?;
    let mut perimeter: Vec<_> = clockwise[offset..]
        .iter()
        .chain(&clockwise[..offset])
        .copied()
        .collect();
    if layout.kind == LayoutKind::OpsinEntry {
        perimeter = layout.entry_perimeter.as_ref()?.clone();
        let outer_edges: EdgeSet = cycle_edges(&faces.outer_boundary.atoms).collect();
        if perimeter.iter().copied().collect::<AtomSet>()
            != faces.outer_boundary.atoms.iter().copied().collect()
            || cycle_edges(&perimeter).any(|edge| !outer_edges.contains(&edge))
        {
            return None;
        }
        start_face_id = layout.entry_face_id?;
        start_atom = *perimeter.first()?;
        if !face_by_id
            .get(&start_face_id)?
            .atom_cycle
            .contains(&start_atom)
            || fusion_atoms.contains(&start_atom)
        {
            return None;
        }
    }
    let construction_order = if layout.kind == LayoutKind::Intrinsic {
        None
    } else {
        layout.construction_atom_order.as_deref()
    };
    let key = (perimeter.clone(), construction_order.map(<[usize]>::to_vec));
    let mut numbered = if let Some(cached) = perimeter_cache.get(&key) {
        cached.clone()
    } else {
        let locant_map =
            number_completed_system(mol, faces, &perimeter, fusion_atoms, construction_order);
        let numbered = locant_map.map(|locants| CompletedNumbering {
            perimeter: perimeter.clone(),
            atom_to_locant: ordered_locant_items(&locants),
            score: numbering_score(mol, &locants, fusion_atoms),
            layout_index: None,
            start_face_id: None,
            start_atom: None,
        });
        perimeter_cache.insert(key, numbered.clone());
        numbered
    }?;
    numbered.layout_index = Some(layout_index);
    numbered.start_face_id = Some(start_face_id);
    numbered.start_atom = Some(start_atom);
    Some(numbered)
}

fn cycle_edges(cycle: &[usize]) -> impl Iterator<Item = Edge> + '_ {
    cycle
        .iter()
        .copied()
        .zip(cycle.iter().copied().cycle().skip(1))
        .take(cycle.len())
        .map(|(u, v)| normalize_edge(u, v))
}

fn clockwise_face_order(centers: &BTreeMap<usize, (i64, i64)>) -> Vec<usize> {
    if centers.is_empty() || centers.values().collect::<BTreeSet<_>>().len() != centers.len() {
        return Vec::new();
    }
    let mut ordered: Vec<_> = centers.keys().copied().collect();
    let first = ordered.iter().skip(1).fold(ordered[0], |best, &id| {
        if (centers[&id].1, centers[&id].0) > (centers[&best].1, centers[&best].0) {
            id
        } else {
            best
        }
    });
    let count = centers.len() as i128;
    let sum_x: i128 = centers.values().map(|p| i128::from(p.0)).sum();
    let sum_y: i128 = centers.values().map(|p| i128::from(p.1)).sum();
    let vector = |id: &usize| {
        (
            i128::from(centers[id].0) * count - sum_x,
            i128::from(centers[id].1) * count - sum_y,
        )
    };
    let half = |(x, y): (i128, i128)| if x > 0 || x == 0 && y >= 0 { 0 } else { 1 };
    ordered.sort_by(|left, right| {
        let l = vector(left);
        let r = vector(right);
        half(l)
            .cmp(&half(r))
            .then_with(|| (l.0 * r.1 - l.1 * r.0).cmp(&0))
    });
    let offset = ordered.iter().take_while(|&&id| id != first).count();
    ordered.rotate_left(offset);
    ordered
}

fn clockwise_boundary(
    boundary: &[usize],
    positions: &BTreeMap<usize, (i64, i64)>,
) -> Option<Vec<usize>> {
    let mut area = 0i128;
    for (&u, &v) in boundary
        .iter()
        .zip(boundary.iter().cycle().skip(1))
        .take(boundary.len())
    {
        let (ux, uy) = *positions.get(&u)?;
        let (vx, vy) = *positions.get(&v)?;
        area += i128::from(ux) * i128::from(vy) - i128::from(vx) * i128::from(uy);
    }
    if area == 0 {
        None
    } else if area < 0 {
        Some(boundary.to_vec())
    } else {
        Some(boundary.iter().rev().copied().collect())
    }
}

fn counterclockwise_nonfusion_start(
    face: &Face,
    boundary: &[usize],
    fusion_atoms: &AtomSet,
) -> Option<usize> {
    let mut candidates = Vec::new();
    for (index, &atom) in boundary.iter().enumerate() {
        if !face.atom_cycle.contains(&atom) || fusion_atoms.contains(&atom) {
            continue;
        }
        let previous = boundary[(index + boundary.len() - 1) % boundary.len()];
        if face.atom_cycle.contains(&previous) && fusion_atoms.contains(&previous) {
            candidates.push(atom);
        }
    }
    if candidates.len() == 1 {
        candidates.first().copied()
    } else {
        None
    }
}

/// Compare actual parent bonds with complete allowed Kekule assignments.
pub fn observed_parent_matches_bond_model(mol: &Molecule, model: &ParentBondModel) -> bool {
    let edges = model.edges();
    let observed: BTreeMap<_, _> = mol
        .bonds
        .values()
        .filter_map(|bond| {
            let edge = normalize_edge(bond.u, bond.v);
            edges.contains(&edge).then_some((edge, bond.order))
        })
        .collect();
    model.allowed_kekule_assignments.iter().any(|assignment| {
        assignment
            .orders
            .iter()
            .copied()
            .collect::<BTreeMap<_, _>>()
            == observed
    })
}

fn number_perimeter(
    mol: &Molecule,
    perimeter: &[usize],
    fusion_atoms: &AtomSet,
) -> Option<BTreeMap<usize, String>> {
    if fusion_atoms.contains(perimeter.first()?) && mol.atoms[&perimeter[0]].symbol == "C" {
        return None;
    }
    let mut result = BTreeMap::new();
    let mut integer = 0usize;
    let mut suffix_counts = BTreeMap::<usize, u8>::new();
    for &atom in perimeter {
        if fusion_atoms.contains(&atom) && mol.atoms[&atom].symbol == "C" {
            if integer == 0 {
                return None;
            }
            let suffix = suffix_counts.entry(integer).or_default();
            if *suffix >= 26 {
                return None;
            }
            result.insert(atom, format!("{integer}{}", char::from(b'a' + *suffix)));
            *suffix += 1;
        } else {
            integer += 1;
            result.insert(atom, integer.to_string());
        }
    }
    Some(result)
}

fn number_completed_system(
    mol: &Molecule,
    faces: &BoundedFaceModel,
    perimeter: &[usize],
    fusion_atoms: &AtomSet,
    opsin_atom_order: Option<&[usize]>,
) -> Option<BTreeMap<usize, String>> {
    let mut result = number_perimeter(mol, perimeter, fusion_atoms)?;
    let perimeter_atoms: AtomSet = perimeter.iter().copied().collect();
    let interior: AtomSet = faces
        .atom_ids
        .difference(&perimeter_atoms)
        .copied()
        .collect();
    if interior.is_empty() {
        return Some(result);
    }
    if let Some(order) = opsin_atom_order {
        let order_atoms: AtomSet = order.iter().copied().collect();
        if order_atoms.len() != order.len() || order_atoms != faces.atom_ids {
            return None;
        }
        let complete: Vec<_> = perimeter
            .iter()
            .copied()
            .chain(order.iter().copied().filter(|a| interior.contains(a)))
            .collect();
        return number_perimeter(
            mol,
            &complete,
            &fusion_atoms.union(&interior).copied().collect(),
        );
    }
    if interior.len() == 1 {
        let complete: Vec<_> = perimeter
            .iter()
            .copied()
            .chain(interior.iter().copied())
            .collect();
        return number_perimeter(
            mol,
            &complete,
            &fusion_atoms.union(&interior).copied().collect(),
        );
    }
    let (distance, anchors) = interior_distances_and_anchors(mol, &faces.atom_ids, perimeter);
    if distance.keys().copied().collect::<AtomSet>() != faces.atom_ids {
        return None;
    }
    let ranks = canonical_ranks(mol, Some(&faces.atom_ids));
    let precedence = super::rules::general_heteroatom_count_precedence();
    let mut hetero: Vec<_> = interior
        .iter()
        .copied()
        .filter(|a| mol.atoms[a].symbol != "C")
        .collect();
    hetero.sort_by_key(|a| {
        (
            distance[a],
            anchors[a]
                .iter()
                .map(|anchor| retained_locant_sort_key(&result[anchor]))
                .min(),
            precedence
                .iter()
                .position(|symbol| *symbol == mol.atoms[a].symbol)
                .unwrap_or(precedence.len()),
            ranks[a],
        )
    });
    let mut next_integer = result
        .values()
        .filter_map(|l| parse_system_locant(l).ok())
        .map(|l| l.base)
        .max()?;
    for &atom in &hetero {
        next_integer += 1;
        result.insert(atom, next_integer.to_string());
    }
    let hetero_set: AtomSet = hetero.into_iter().collect();
    let mut carbons: Vec<_> = interior.difference(&hetero_set).copied().collect();
    carbons.sort_by_key(|a| (distance[a], ranks[a]));
    for atom in carbons {
        let anchor = anchors[&atom]
            .iter()
            .min_by_key(|a| retained_locant_sort_key(&result[a]))?;
        let anchor_locant = parse_system_locant(&result[anchor]).ok()?;
        let mut locant = format!("{}{}", anchor_locant.base, anchor_locant.fusion_suffix);
        const SUPERSCRIPTS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
        locant.extend(
            distance[&atom]
                .to_string()
                .bytes()
                .map(|b| SUPERSCRIPTS[(b - b'0') as usize]),
        );
        result.insert(atom, locant);
    }
    if result.values().collect::<BTreeSet<_>>().len() != result.len() {
        return None;
    }
    Some(result)
}

fn ordered_locant_items(locants: &BTreeMap<usize, String>) -> Vec<(usize, String)> {
    let mut result: Vec<_> = locants.iter().map(|(&a, l)| (a, l.clone())).collect();
    result.sort_by_key(|(_, l)| retained_locant_sort_key(l));
    result
}

fn interior_distances_and_anchors(
    mol: &Molecule,
    atoms: &AtomSet,
    perimeter: &[usize],
) -> (BTreeMap<usize, usize>, BTreeMap<usize, AtomSet>) {
    let mut distance: BTreeMap<_, _> = perimeter.iter().map(|&a| (a, 0)).collect();
    let mut anchors: BTreeMap<_, _> = perimeter.iter().map(|&a| (a, AtomSet::from([a]))).collect();
    let mut pending: VecDeque<_> = perimeter.iter().copied().collect();
    while let Some(atom) = pending.pop_front() {
        let next_distance = distance[&atom] + 1;
        for &neighbor in mol.get_neighbors(atom) {
            if !atoms.contains(&neighbor) {
                continue;
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = distance.entry(neighbor) {
                entry.insert(next_distance);
                anchors.insert(neighbor, anchors[&atom].clone());
                pending.push_back(neighbor);
            } else if distance[&neighbor] == next_distance {
                let merged: AtomSet = anchors[&neighbor].union(&anchors[&atom]).copied().collect();
                if merged != anchors[&neighbor] {
                    anchors.insert(neighbor, merged);
                    pending.push_back(neighbor);
                }
            }
        }
    }
    (distance, anchors)
}

fn numbering_score(
    mol: &Molecule,
    locants: &BTreeMap<usize, String>,
    fusion_atoms: &AtomSet,
) -> NumberingScore {
    let hetero: Vec<_> = locants
        .iter()
        .filter(|(a, _)| mol.atoms[a].symbol != "C")
        .collect();
    let locant_tuple = |values: Vec<&String>| {
        let mut keys: Vec<_> = values.into_iter().map(|l| locant_key(l)).collect();
        keys.sort();
        ComparisonKey::Tuple(keys)
    };
    let all_hetero = locant_tuple(hetero.iter().map(|(_, l)| *l).collect());
    let by_element = ComparisonKey::Tuple(
        super::rules::general_heteroatom_count_precedence()
            .iter()
            .map(|symbol| {
                locant_tuple(
                    hetero
                        .iter()
                        .filter(|(a, _)| mol.atoms[a].symbol == *symbol)
                        .map(|(_, l)| *l)
                        .collect(),
                )
            })
            .collect(),
    );
    let fusion_carbons = locant_tuple(
        fusion_atoms
            .iter()
            .filter(|a| mol.atoms[a].symbol == "C")
            .map(|a| &locants[a])
            .collect(),
    );
    let fusion_hetero = locant_tuple(
        fusion_atoms
            .iter()
            .filter(|a| mol.atoms[a].symbol != "C")
            .map(|a| &locants[a])
            .collect(),
    );
    let indicated_h = locant_tuple(
        indicated_hydrogen_candidate_atoms(mol, locants)
            .iter()
            .map(|a| &locants[a])
            .collect(),
    );
    ComparisonKey::Tuple(vec![
        all_hetero,
        by_element,
        fusion_carbons,
        fusion_hetero,
        indicated_h,
    ])
}

fn locant_key(locant: &str) -> ComparisonKey {
    let (base, _, suffix, distance) = retained_locant_sort_key(locant);
    ComparisonKey::Tuple(vec![
        ComparisonKey::Int(base as i64),
        ComparisonKey::Str(suffix),
        ComparisonKey::Int(distance as i64),
    ])
}

pub fn indicated_hydrogen_candidate_atoms(
    mol: &Molecule,
    locants: &BTreeMap<usize, String>,
) -> Vec<usize> {
    locants
        .keys()
        .copied()
        .filter(|&atom| is_indicated_hydrogen_candidate(mol, atom, locants))
        .collect()
}

fn is_indicated_hydrogen_candidate(
    mol: &Molecule,
    atom: usize,
    locants: &BTreeMap<usize, String>,
) -> bool {
    let value = &mol.atoms[&atom];
    if value.total_h_count == 0 {
        return false;
    }
    if value.symbol == "N"
        && value.charge == 1
        && value.total_h_count == 1
        && mol
            .get_neighbors(atom)
            .iter()
            .any(|&n| mol.get_bond(atom, n).is_some_and(|b| b.order == 2))
    {
        return false;
    }
    if value.symbol != "C" {
        return true;
    }
    value.total_h_count > 1
        && mol
            .get_neighbors(atom)
            .iter()
            .filter(|n| locants.contains_key(n))
            .all(|&n| mol.get_bond(atom, n).is_some_and(|b| b.order == 1))
}

/// Infer intrinsic heteroatom H from compatible parent-to-observed bond deltas.
pub fn bond_model_indicated_hydrogen_atoms(
    mol: &Molecule,
    model: &ParentBondModel,
    candidates: &AtomSet,
) -> AtomSet {
    let edges = model.edges();
    let observed: BTreeMap<_, _> = mol
        .bonds
        .values()
        .filter_map(|bond| {
            let edge = normalize_edge(bond.u, bond.v);
            if !edges.contains(&edge) {
                return None;
            }
            let order = if mol.atoms[&bond.u].is_aromatic
                && mol.atoms[&bond.v].is_aromatic
                && model.pi_eligible_edges.contains(&edge)
            {
                None
            } else {
                Some(bond.order)
            };
            Some((edge, order))
        })
        .collect();
    let mut viable = Vec::new();
    for assignment in &model.allowed_kekule_assignments {
        let allowed: BTreeMap<_, _> = assignment
            .orders
            .iter()
            .map(|&(e, o)| (normalize_edge(e.0, e.1), o))
            .collect();
        if observed.keys().collect::<Vec<_>>() != allowed.keys().collect::<Vec<_>>() {
            continue;
        }
        let mut indicated = AtomSet::new();
        let mut compatible = true;
        for (&edge, &order) in &observed {
            if order.is_none() || order == Some(allowed[&edge]) {
                continue;
            }
            if order != Some(1) || allowed[&edge] != 2 {
                compatible = false;
                break;
            }
            let endpoints: AtomSet = [edge.0, edge.1]
                .into_iter()
                .filter(|a| {
                    candidates.contains(a)
                        || mol.atoms[a].symbol == "C"
                            && mol
                                .get_neighbors(*a)
                                .iter()
                                .all(|&n| mol.get_bond(*a, n).is_some_and(|b| b.order == 1))
                })
                .collect();
            if endpoints.len() == 1 {
                indicated.extend(endpoints.into_iter().filter(|a| mol.atoms[a].symbol != "C"));
            } else if endpoints.len() != 2 {
                compatible = false;
                break;
            }
        }
        if compatible {
            viable.push(indicated);
        }
    }
    viable
        .into_iter()
        .min_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)))
        .unwrap_or_default()
}

pub const MANCUDE_SEARCH_BUDGET: usize = 100_000;
pub const RETAINED_PARENT_BOND_CAPACITY_ERROR: &str =
    "retained template and derived parent bond model disagree on mancude capacity";
pub fn mancude_search_budget_exceeded(error: &str) -> bool {
    error.starts_with("mancude assignment search exceeded its budget of ")
}

// Preserve the source API's Molecule compatibility path alongside the graph API.
#[allow(dead_code)]
pub fn parent_bond_graph(mol: &Molecule, atoms: &AtomSet) -> FusionGraph {
    FusionGraph {
        atoms: atoms
            .iter()
            .map(|&id| {
                let atom = &mol.atoms[&id];
                let mut site = FusionGraphAtom::new(id, &atom.symbol);
                site.formal_charge = atom.charge;
                site.forced_single = atom.element().is_some_and(|e| e.mancude_forced_single);
                site
            })
            .collect(),
        bonds: mol
            .bonds
            .values()
            .filter(|b| atoms.contains(&b.u) && atoms.contains(&b.v))
            .map(|b| FusionGraphBond {
                atoms: normalize_edge(b.u, b.v),
                bond_class: "mancude".into(),
            })
            .collect(),
    }
}
fn parent_bond_loads(
    graph: &FusionGraph,
    assignment: &BondAssignment,
) -> Result<BTreeMap<usize, usize>, String> {
    let expected: EdgeSet = graph
        .bonds
        .iter()
        .map(|b| normalize_edge(b.atoms.0, b.atoms.1))
        .collect();
    let supplied: EdgeSet = assignment
        .orders
        .iter()
        .map(|&(e, _)| normalize_edge(e.0, e.1))
        .collect();
    if supplied != expected || supplied.len() != assignment.orders.len() {
        return Err("parent valence assignment must cover every parent bond exactly once".into());
    }
    let mut loads: BTreeMap<_, usize> = graph.atoms.iter().map(|a| (a.id, 0)).collect();
    for &(edge, order) in &assignment.orders {
        if !matches!(order, 1 | 2) {
            return Err("parent valence assignment requires single or double bond orders".into());
        }
        *loads.entry(edge.0).or_default() += order;
        *loads.entry(edge.1).or_default() += order;
    }
    Ok(loads)
}
pub fn validate_parent_bond_valence(
    graph: &FusionGraph,
    assignment: &BondAssignment,
) -> Result<(), String> {
    let loads = parent_bond_loads(graph, assignment)?;
    for site in &graph.atoms {
        let policy = elements::get(&site.symbol)
            .ok_or_else(|| format!("Unknown element symbol: {}", site.symbol))?;
        if let Some(limit) = policy.mancude_limit_for_charge(site.formal_charge)?
            && loads[&site.id] > limit
        {
            return Err(format!(
                "parent atom {} ({}, charge {:+}) has bond-order load {} above fixed-valence limit {}",
                site.id, site.symbol, site.formal_charge, loads[&site.id], limit
            ));
        }
    }
    Ok(())
}
pub fn parent_pi_capable_atom_ids(graph: &FusionGraph) -> Result<AtomSet, String> {
    let mut orders: Vec<_> = graph
        .bonds
        .iter()
        .map(|b| {
            (
                normalize_edge(b.atoms.0, b.atoms.1),
                if b.bond_class == "double" { 2 } else { 1 },
            )
        })
        .collect();
    orders.sort();
    let baseline = BondAssignment { orders };
    validate_parent_bond_valence(graph, &baseline)?;
    let loads = parent_bond_loads(graph, &baseline)?;
    let mut capable = AtomSet::new();
    for site in &graph.atoms {
        let policy = elements::get(&site.symbol)
            .ok_or_else(|| format!("Unknown element symbol: {}", site.symbol))?;
        if site.pi_capacity > 0
            && !site.forced_single
            && policy
                .mancude_limit_for_charge(site.formal_charge)?
                .is_none_or(|limit| loads[&site.id] < limit)
        {
            capable.insert(site.id);
        }
    }
    Ok(capable)
}
pub fn parent_bond_model(graph: &FusionGraph) -> Result<ParentBondModel, String> {
    parent_bond_model_with_budget(graph, MANCUDE_SEARCH_BUDGET)
}
pub fn parent_bond_model_with_budget(
    graph: &FusionGraph,
    search_budget: usize,
) -> Result<ParentBondModel, String> {
    let classes: BTreeMap<_, _> = graph
        .bonds
        .iter()
        .map(|b| (normalize_edge(b.atoms.0, b.atoms.1), b.bond_class.as_str()))
        .collect();
    let required_double: EdgeSet = classes
        .iter()
        .filter(|(_, c)| **c == "double")
        .map(|(&e, _)| e)
        .collect();
    let occupied: AtomSet = required_double.iter().flat_map(|&(u, v)| [u, v]).collect();
    if occupied.len() != required_double.len() * 2 {
        return Err("required-double bonds must be non-cumulative".into());
    }
    let capable = parent_pi_capable_atom_ids(graph)?;
    let eligible: EdgeSet = classes
        .iter()
        .filter(|(e, c)| {
            matches!(**c, "aromatic" | "mancude" | "fusion")
                && !occupied.contains(&e.0)
                && !occupied.contains(&e.1)
                && capable.contains(&e.0)
                && capable.contains(&e.1)
        })
        .map(|(&e, _)| e)
        .collect();
    let required = classes
        .keys()
        .filter(|e| !eligible.contains(e) && !required_double.contains(e))
        .copied()
        .collect();
    let matchings = maximum_matchings(&eligible, search_budget)?;
    Ok(ParentBondModel {
        maximum_non_cumulative_double_bonds: required_double.len()
            + matchings.iter().map(BTreeSet::len).max().unwrap_or(0),
        allowed_kekule_assignments: matchings
            .iter()
            .map(|m| BondAssignment {
                orders: classes
                    .keys()
                    .map(|&e| {
                        (
                            e,
                            if m.contains(&e) || required_double.contains(&e) {
                                2
                            } else {
                                1
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
        required_single_bonds: required,
        pi_eligible_edges: eligible,
        required_double_bonds: required_double,
    })
}
pub fn retained_template_parent_bond_model(
    template: &RetainedGraphTemplate,
    mapping: &BTreeMap<String, usize>,
    indicated_h: Option<&[String]>,
) -> Result<ParentBondModel, String> {
    retained_template_parent_bond_model_with_budget(
        template,
        mapping,
        indicated_h,
        MANCUDE_SEARCH_BUDGET,
    )
}
pub fn retained_template_parent_bond_model_with_budget(
    template: &RetainedGraphTemplate,
    mapping: &BTreeMap<String, usize>,
    indicated_h: Option<&[String]>,
    budget: usize,
) -> Result<ParentBondModel, String> {
    if mapping.keys().collect::<BTreeSet<_>>() != template.locants.iter().collect::<BTreeSet<_>>() {
        return Err("retained parent bond model requires a complete locant map".into());
    }
    let saturated_parent = template.mancude_double_bonds.is_none();
    let mut atoms = template.atoms.clone();
    if let Some(h) = indicated_h.filter(|h| *h != template.default_indicated_h.as_slice()) {
        if h.len() != template.indicated_hydrogen_count()
            || h.iter().any(|l| !mapping.contains_key(l))
        {
            return Err(
                "retained indicated H must preserve template count and locant domain".into(),
            );
        }
        let relocatable =
            super::super::retained_fused_templates::relocatable_atom_by_locant(template);
        // Template order is stable even when the locant lookup uses a sorted map.
        atoms = template
            .atoms
            .iter()
            .map(|a| {
                let mut a = relocatable[&a.locant].clone();
                if h.contains(&a.locant) {
                    a.saturated = true;
                    a.default_h = true;
                }
                a
            })
            .collect();
    }
    let mut degrees: BTreeMap<&str, usize> =
        template.locants.iter().map(|l| (l.as_str(), 0)).collect();
    for b in &template.bonds {
        *degrees.entry(&b.locants.0).or_default() += 1;
        *degrees.entry(&b.locants.1).or_default() += 1;
    }
    for a in &mut atoms {
        if a.symbol == "N" && a.charge == 0 && degrees[a.locant.as_str()] == 3 {
            a.forced_single = true;
        }
    }
    let graph = FusionGraph {
        atoms: atoms
            .iter()
            .map(|a| FusionGraphAtom {
                id: mapping[&a.locant],
                symbol: a.symbol.clone(),
                formal_charge: a.charge,
                pi_capacity: if saturated_parent {
                    0
                } else {
                    a.resolved_pi_capacity()
                },
                forced_single: saturated_parent || a.forced_single,
                indicated_h_site: a.indicated_h_site || a.default_h,
                saturated: a.saturated,
            })
            .collect(),
        bonds: template
            .bonds
            .iter()
            .map(|b| FusionGraphBond {
                atoms: normalize_edge(mapping[&b.locants.0], mapping[&b.locants.1]),
                bond_class: if saturated_parent {
                    "single".into()
                } else {
                    b.bond_class.clone()
                },
            })
            .collect(),
    };
    let model = parent_bond_model_with_budget(&graph, budget)?;
    if model.maximum_non_cumulative_double_bonds != template.mancude_double_bonds.unwrap_or(0) {
        return Err(RETAINED_PARENT_BOND_CAPACITY_ERROR.into());
    }
    Ok(model)
}
pub fn maximum_matchings(edges: &EdgeSet, budget: usize) -> Result<Vec<EdgeSet>, String> {
    let vertices: AtomSet = edges.iter().flat_map(|&(u, v)| [u, v]).collect();
    let neighbors: BTreeMap<_, Vec<_>> = vertices
        .iter()
        .map(|&v| {
            (
                v,
                edges
                    .iter()
                    .filter_map(|&(u, w)| {
                        if u == v {
                            Some(w)
                        } else if w == v {
                            Some(u)
                        } else {
                            None
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    fn search(
        available: &AtomSet,
        neighbors: &BTreeMap<usize, Vec<usize>>,
        memo: &mut BTreeMap<AtomSet, Vec<EdgeSet>>,
        states: &mut usize,
        budget: usize,
    ) -> Result<Vec<EdgeSet>, String> {
        if let Some(result) = memo.get(available) {
            return Ok(result.clone());
        }
        *states += 1;
        if *states > budget {
            return Err(format!(
                "mancude assignment search exceeded its budget of {budget} states"
            ));
        }
        let Some(&atom) = available.first() else {
            return Ok(vec![EdgeSet::new()]);
        };
        let mut without = available.clone();
        without.remove(&atom);
        let mut candidates = search(&without, neighbors, memo, states, budget)?;
        for neighbor in &neighbors[&atom] {
            if !available.contains(neighbor) {
                continue;
            }
            let mut residual = without.clone();
            residual.remove(neighbor);
            for mut matching in search(&residual, neighbors, memo, states, budget)? {
                matching.insert(normalize_edge(atom, *neighbor));
                candidates.push(matching);
            }
        }
        let maximum = candidates.iter().map(BTreeSet::len).max().unwrap_or(0);
        let result: Vec<_> = candidates
            .into_iter()
            .filter(|m| m.len() == maximum)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        memo.insert(available.clone(), result.clone());
        Ok(result)
    }
    search(&vertices, &neighbors, &mut BTreeMap::new(), &mut 0, budget)
}

#[cfg(test)]
mod tests {
    use super::super::super::molecule::{Atom, Bond};
    use super::super::faces::{GraphCycle, audit_bounded_face_model};
    use super::*;

    fn bicyclic_fixture() -> (Molecule, BoundedFaceModel, FusedLayout) {
        let mut mol = Molecule::new();
        for id in 0..10 {
            mol.add_atom(Atom::new(id, if id == 9 { "N" } else { "C" }))
                .unwrap();
        }
        let cycles = vec![
            GraphCycle::from_atoms([0, 2, 3, 4, 5, 1]).unwrap(),
            GraphCycle::from_atoms([1, 6, 7, 8, 9, 0]).unwrap(),
        ];
        let edges: EdgeSet = cycles
            .iter()
            .flat_map(|cycle| cycle.edges.iter().copied())
            .collect();
        for (id, &(u, v)) in edges.iter().enumerate() {
            mol.add_bond(Bond::new(id, u, v, 1)).unwrap();
        }
        let atom_ids = mol.atoms.keys().copied().collect();
        let audit = audit_bounded_face_model(&mol, &atom_ids, &cycles);
        assert!(audit.ok);
        let faces = BoundedFaceModel {
            atom_ids,
            edge_ids: edges,
            faces: cycles,
            outer_boundary: GraphCycle::from_atoms(audit.outer_boundary.clone()).unwrap(),
            cycle_rank: 2,
            audit,
        };
        let layout = FusedLayout {
            face_positions: vec![(0, -4, 0), (1, 4, 0)],
            atom_positions: vec![
                (0, 0, 2),
                (1, 0, -2),
                (2, -2, 4),
                (3, -6, 4),
                (4, -8, 0),
                (5, -6, -4),
                (6, 2, -4),
                (7, 6, -4),
                (8, 8, 0),
                (9, 6, 4),
            ],
            ..Default::default()
        };
        (mol, faces, layout)
    }

    #[test]
    fn layout_starts_and_ordered_heteroatom_locants_select_reflections() {
        let (mol, faces, layout) = bicyclic_fixture();
        let mut reflection = layout.clone();
        for (_, x, _) in &mut reflection.atom_positions {
            *x = -*x;
        }
        for (_, x, _) in &mut reflection.face_positions {
            *x = -*x;
        }
        let face_model = typed_face_model(&mol, &faces);
        let selection = completed_system_numbering_selection(
            &mol,
            &faces,
            Some(&face_model),
            &[layout, reflection],
            false,
        )
        .unwrap();
        assert_eq!(selection.accepted.len(), 1);
        assert_eq!(selection.rejected.len(), 1);
        let numbering = &selection.accepted[0];
        assert_eq!(numbering.start_atom, Some(9));
        assert_eq!(numbering.start_face_id, Some(1));
        assert_eq!(numbering.string_map()[&9], "1");
        assert_eq!(numbering.string_map()[&1], "4a");
        assert_eq!(numbering.string_map()[&0], "8a");
    }

    #[test]
    fn fully_tied_numberings_preserve_source_layout_enumeration() {
        let mol = super::super::super::graph_io::read_smiles("N1CCC2C(C1)C[NH2+]CC2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let faces = super::super::faces::select_bounded_face_model(&mol, &atoms)
            .unwrap()
            .unwrap();
        let model = typed_face_model(&mol, &faces);
        let layouts = preferred_intrinsic_layouts(&model).unwrap();
        let numberings = completed_system_numberings(&mol, &faces, Some(&model), &layouts).unwrap();
        let charged = mol
            .atoms
            .values()
            .find(|atom| atom.charge == 1)
            .unwrap()
            .idx;
        let locants: BTreeSet<_> = numberings
            .iter()
            .map(|numbering| numbering.string_map()[&charged].clone())
            .collect();
        assert_eq!(locants, BTreeSet::from(["2".into(), "7".into()]));
        assert_eq!(numberings[0].string_map()[&charged], "7");
    }

    #[test]
    fn perimeter_and_interior_locants_preserve_construction_order() {
        let (mut mol, mut faces, _) = bicyclic_fixture();
        mol.add_atom(Atom::new(10, "C")).unwrap();
        faces.atom_ids.insert(10);
        let perimeter = [9, 8, 7, 6, 1, 5, 4, 3, 2, 0];
        let fusion = faces.fusion_atoms();
        let numbering = number_completed_system(&mol, &faces, &perimeter, &fusion, None).unwrap();
        assert_eq!(numbering[&10], "8b");
        assert!(number_perimeter(&mol, &[0, 2, 3], &fusion).is_none());
        mol.add_atom(Atom::new(11, "N")).unwrap();
        faces.atom_ids.insert(11);
        let order: Vec<_> = perimeter.iter().copied().chain([11, 10]).collect();
        let numbering =
            number_completed_system(&mol, &faces, &perimeter, &fusion, Some(&order)).unwrap();
        assert_eq!(numbering[&11], "9");
        assert_eq!(numbering[&10], "9a");
        let invalid_order = [0, 0];
        assert!(
            number_completed_system(&mol, &faces, &perimeter, &fusion, Some(&invalid_order))
                .is_none()
        );
    }

    #[test]
    fn shortest_interior_anchors_include_all_equal_routes() {
        let mut mol = Molecule::new();
        for id in 0..5 {
            mol.add_atom(Atom::new(id, "C")).unwrap();
        }
        for (id, (u, v)) in [(0, 2), (1, 3), (2, 4), (3, 4)].into_iter().enumerate() {
            mol.add_bond(Bond::new(id, u, v, 1)).unwrap();
        }
        let (distances, anchors) = interior_distances_and_anchors(&mol, &(0..5).collect(), &[0, 1]);
        assert_eq!(distances[&4], 2);
        assert_eq!(anchors[&4], AtomSet::from([0, 1]));
    }

    #[test]
    fn face_centers_follow_clockwise_order_from_uppermost_rightmost() {
        let centers = BTreeMap::from([(0, (0, 2)), (1, (2, 0)), (2, (0, -2)), (3, (-2, 0))]);
        assert_eq!(clockwise_face_order(&centers), vec![0, 1, 2, 3]);
        assert!(clockwise_face_order(&BTreeMap::from([(0, (0, 0)), (1, (0, 0))])).is_empty());
    }

    #[test]
    fn parent_bond_deltas_distinguish_intrinsic_h_from_hydrogenation() {
        let mut mol = Molecule::new();
        let mut nitrogen = Atom::new(0, "N");
        nitrogen.total_h_count = 1;
        let mut carbon = Atom::new(1, "C");
        carbon.total_h_count = 1;
        mol.add_atom(nitrogen).unwrap();
        mol.add_atom(carbon).unwrap();
        mol.add_atom(Atom::new(2, "C")).unwrap();
        mol.add_bond(Bond::new(0, 0, 1, 1)).unwrap();
        mol.add_bond(Bond::new(1, 1, 2, 2)).unwrap();
        let model = ParentBondModel {
            allowed_kekule_assignments: vec![BondAssignment {
                orders: vec![((0, 1), 2)],
            }],
            pi_eligible_edges: EdgeSet::from([(0, 1)]),
            ..Default::default()
        };
        assert_eq!(
            bond_model_indicated_hydrogen_atoms(&mol, &model, &AtomSet::from([0])),
            AtomSet::from([0])
        );
        assert!(!observed_parent_matches_bond_model(&mol, &model));
        mol.set_bond_order(1, 1).unwrap();
        assert!(bond_model_indicated_hydrogen_atoms(&mol, &model, &AtomSet::from([0])).is_empty());
        mol.atoms.get_mut(&0).unwrap().charge = 1;
        mol.set_bond_order(0, 2).unwrap();
        assert!(
            !indicated_hydrogen_candidate_atoms(&mol, &BTreeMap::from([(0, "1".into())]))
                .contains(&0)
        );
    }

    #[test]
    fn exhaustive_assignments_preserve_order_and_budget() {
        let edges = EdgeSet::from([(0, 1), (0, 3), (1, 2), (2, 3)]);
        assert_eq!(
            maximum_matchings(&edges, MANCUDE_SEARCH_BUDGET).unwrap(),
            vec![
                EdgeSet::from([(0, 1), (2, 3)]),
                EdgeSet::from([(0, 3), (1, 2)])
            ]
        );
        assert_eq!(
            maximum_matchings(&edges, 0).unwrap_err(),
            "mancude assignment search exceeded its budget of 0 states"
        );
        let graph = FusionGraph {
            atoms: (0..4).map(|id| FusionGraphAtom::new(id, "C")).collect(),
            bonds: edges
                .iter()
                .map(|&atoms| FusionGraphBond {
                    atoms,
                    bond_class: "mancude".into(),
                })
                .collect(),
        };
        let model = parent_bond_model(&graph).unwrap();
        assert_eq!(model.maximum_non_cumulative_double_bonds, 2);
        assert_eq!(model.allowed_kekule_assignments.len(), 2);
        assert!(parent_bond_model_with_budget(&graph, 1).is_err());
    }

    #[test]
    fn mandatory_bonds_and_fixed_valence_bound_optional_pi() {
        let mut graph = FusionGraph {
            atoms: (0..4)
                .map(|id| FusionGraphAtom::new(id, if id == 0 { "O" } else { "C" }))
                .collect(),
            bonds: vec![
                FusionGraphBond {
                    atoms: (0, 1),
                    bond_class: "mancude".into(),
                },
                FusionGraphBond {
                    atoms: (0, 3),
                    bond_class: "mancude".into(),
                },
                FusionGraphBond {
                    atoms: (1, 2),
                    bond_class: "double".into(),
                },
                FusionGraphBond {
                    atoms: (2, 3),
                    bond_class: "mancude".into(),
                },
            ],
        };
        let model = parent_bond_model(&graph).unwrap();
        assert_eq!(model.required_double_bonds, EdgeSet::from([(1, 2)]));
        assert!(model.pi_eligible_edges.is_empty());
        assert_eq!(model.maximum_non_cumulative_double_bonds, 1);
        graph.bonds[1].bond_class = "double".into();
        assert!(
            parent_bond_model(&graph)
                .unwrap_err()
                .contains("above fixed-valence limit 2")
        );
    }
}
