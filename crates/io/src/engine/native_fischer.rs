//! Atomic conversion between explicitly marked Fischer crosses and ordinary
//! stereochemical drawings. Unsupported geometry never produces a partial edit.
use super::{Request, Response, native_cleanup, native_response};
use crate::{
    chemistry::{document as molecular, smiles},
    document::{AtomStereo, Document, Point},
};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unsupported protocol version")]
    Protocol,
    #[error("Unknown Fischer conversion operation")]
    Operation,
    #[error("Missing Fischer conversion drawing")]
    MissingDocument,
    #[error("{0}")]
    Drawing(String),
    #[error("Select a molecule containing a Fischer projection")]
    NoFischer,
    #[error("Select atoms in an available molecule")]
    Selection,
    #[error("Select one acyclic molecule to convert to a Fischer projection")]
    SelectOne,
    #[error(
        "Fischer conversion supports acyclic molecules without double-bond stereochemistry whose assigned stereocenters lie on the longest carbon chain"
    )]
    Unsupported,
    #[error("Fischer conversion could not preserve stereochemistry; drawing unchanged")]
    Stereo,
    #[error(transparent)]
    Chemistry(#[from] molecular::Error),
    #[error(transparent)]
    Smiles(#[from] smiles::write::Error),
    #[error(transparent)]
    Cleanup(#[from] native_cleanup::Error),
    #[error(transparent)]
    Analysis(#[from] native_response::Error),
    #[error("Fischer conversion failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

type Result<T> = std::result::Result<T, Error>;
type Adjacent = HashMap<u64, Vec<u64>>;

fn components(document: &Document, selection: &[u64]) -> Result<(Adjacent, Vec<Vec<u64>>)> {
    document.validate().map_err(Error::Drawing)?;
    let mut adjacent: Adjacent = document
        .atoms
        .iter()
        .map(|atom| (atom.id, vec![]))
        .collect();
    for bond in &document.bonds {
        adjacent
            .get_mut(&bond.a)
            .ok_or(Error::Selection)?
            .push(bond.b);
        adjacent
            .get_mut(&bond.b)
            .ok_or(Error::Selection)?
            .push(bond.a);
    }
    let selected: HashSet<_> = selection.iter().copied().collect();
    if selected.iter().any(|id| !adjacent.contains_key(id)) {
        return Err(Error::Selection);
    }
    let mut visited = HashSet::new();
    let mut result = Vec::new();
    for atom in &document.atoms {
        if !visited.insert(atom.id) {
            continue;
        }
        let mut component = Vec::new();
        let mut pending = vec![atom.id];
        while let Some(id) = pending.pop() {
            component.push(id);
            for &other in adjacent.get(&id).ok_or(Error::Selection)? {
                if visited.insert(other) {
                    pending.push(other);
                }
            }
        }
        if selected.is_empty() || component.iter().any(|id| selected.contains(id)) {
            result.push(component);
        }
    }
    Ok((adjacent, result))
}

/// Prepare an ordinary cleanup request for entire selected Fischer molecules.
/// The cleanup pipeline reads Fischer chemistry before replacing its crosses
/// with native wedges, and verifies both chemical and visible stereochemistry.
pub fn wedge_request(request: &Request) -> Result<Request> {
    if request.protocol != super::PROTOCOL {
        return Err(Error::Protocol);
    }
    let source = request.document.as_ref().ok_or(Error::MissingDocument)?;
    let (_, components) = components(source, request.selected_ids.as_deref().unwrap_or_default())?;
    let centers = crate::fischer::marked_centers(source);
    let mut selected = Vec::new();
    for component in components {
        selected.extend(component.into_iter().filter(|id| centers.contains(id)));
    }
    if selected.is_empty() {
        return Err(Error::NoFischer);
    }
    let mut clean = Request::molecule("clean", source.clone());
    clean.selected_ids = Some(selected);
    clean.cleanup = Some(crate::cleanup::Options {
        scope: crate::cleanup::Scope::SelectedMolecules,
        keep_orientation: false,
    });
    Ok(clean)
}

pub async fn execute(
    request: Request,
    config: Option<native_response::Config>,
) -> Result<Response> {
    if request.protocol != super::PROTOCOL {
        return Err(Error::Protocol);
    }
    if request.operation == "fischer_wedge" {
        return Ok(native_cleanup::execute(wedge_request(&request)?, config).await?);
    }
    if request.operation != "fischer" {
        return Err(Error::Operation);
    }
    let source = request.document.ok_or(Error::MissingDocument)?;
    let selected = request.selected_ids.unwrap_or_default();
    let document = crate::task::spawn_blocking(move || to_fischer(&source, &selected)).await??;
    Ok(
        native_response::execute_with_config(Request::molecule("analyze", document), config)
            .await?,
    )
}

fn atom(document: &Document, id: u64) -> Result<&crate::document::Atom> {
    document.atom(id).ok_or(Error::Selection)
}

fn farthest(start: u64, adjacent: &Adjacent) -> Result<(u64, HashMap<u64, u64>)> {
    let mut queue = VecDeque::from([start]);
    let mut parent = HashMap::from([(start, start)]);
    let mut last = start;
    while let Some(id) = queue.pop_front() {
        last = id;
        for &other in adjacent.get(&id).ok_or(Error::Unsupported)? {
            if let std::collections::hash_map::Entry::Vacant(entry) = parent.entry(other) {
                entry.insert(id);
                queue.push_back(other);
            }
        }
    }
    Ok((last, parent))
}

fn longest_carbon_chain(
    source: &Document,
    component: &[u64],
    adjacent: &Adjacent,
) -> Result<Vec<u64>> {
    let mut carbon = Adjacent::new();
    for &id in component {
        if atom(source, id)?.element == "C" {
            carbon.insert(
                id,
                adjacent
                    .get(&id)
                    .ok_or(Error::Selection)?
                    .iter()
                    .copied()
                    .filter(|other| source.atom(*other).is_some_and(|a| a.element == "C"))
                    .collect(),
            );
        }
    }
    let mut visited = HashSet::new();
    let mut longest = Vec::new();
    for &id in component {
        if !carbon.contains_key(&id) || visited.contains(&id) {
            continue;
        }
        let (end, group) = farthest(id, &carbon)?;
        visited.extend(group.keys().copied());
        let (mut last, parent) = farthest(end, &carbon)?;
        let mut chain = vec![last];
        while last != end {
            last = *parent.get(&last).ok_or(Error::Unsupported)?;
            chain.push(last);
        }
        if chain.len() > longest.len() {
            longest = chain;
        }
    }
    if longest.len() < 3 {
        return Err(Error::Unsupported);
    }
    let oxidation = |id| -> u32 {
        source
            .bonds
            .iter()
            .filter_map(|bond| {
                let other = if bond.a == id {
                    bond.b
                } else if bond.b == id {
                    bond.a
                } else {
                    return None;
                };
                source
                    .atom(other)
                    .filter(|a| a.element != "C" && a.element != "H")
                    .map(|_| u32::from(bond.order))
            })
            .sum()
    };
    if oxidation(*longest.first().ok_or(Error::Unsupported)?)
        < oxidation(*longest.last().ok_or(Error::Unsupported)?)
    {
        longest.reverse();
    }
    Ok(longest)
}

fn translate_subtree(
    document: &mut Document,
    adjacent: &Adjacent,
    root: u64,
    parent: u64,
    target: Point,
) -> Result<()> {
    let position = atom(document, root)?.position;
    let shift = Point::new(target.x - position.x, target.y - position.y);
    let mut pending = vec![(root, parent)];
    while let Some((id, parent)) = pending.pop() {
        let value = document.atom_mut(id).ok_or(Error::Selection)?;
        value.position = value.position.offset(shift.x, shift.y);
        for &other in adjacent.get(&id).ok_or(Error::Selection)? {
            if other != parent {
                pending.push((other, id));
            }
        }
    }
    Ok(())
}

/// Lay out the longest carbon chain vertically, more oxidized end first.
/// Select exactly one component. Off-chain subtrees move rigidly; a stereocenter's missing H becomes a real
/// explicit atom. Cycles and centers outside that chain require a manual edit.
pub fn to_fischer(source: &Document, selection: &[u64]) -> Result<Document> {
    let (mut adjacent, components) = components(source, selection)?;
    if selection.is_empty() || components.len() != 1 {
        return Err(Error::SelectOne);
    }
    let mut document = source.clone();
    let length = document.drawing_style.bond_length_world;
    for component in components {
        let members: HashSet<_> = component.iter().copied().collect();
        let isolated = Document {
            version: source.version,
            atoms: source
                .atoms
                .iter()
                .filter(|atom| members.contains(&atom.id))
                .cloned()
                .collect(),
            bonds: source
                .bonds
                .iter()
                .filter(|bond| members.contains(&bond.a))
                .cloned()
                .collect(),
            abbreviations: source
                .abbreviations
                .iter()
                .filter(|group| members.contains(&group.anchor))
                .cloned()
                .collect(),
            drawing_style: source.drawing_style.clone(),
            atom_labels: source.atom_labels.clone(),
            ..Default::default()
        };
        if crate::attachments::present(&isolated) || crate::generic::touches(source, &component) {
            return Err(Error::Unsupported);
        }
        let molecule = molecular::prepare(&isolated)?;
        let draft = molecular::for_drawing(&molecule, &isolated)?;
        let labels = draft.labels()?;
        let assigned = draft.finish(labels)?;
        // Off-chain subtrees move rigidly around a reoriented backbone, which
        // can turn a drawn E/Z configuration over.
        if assigned.bonds.iter().any(|b| b.stereo.is_some()) {
            return Err(Error::Unsupported);
        }
        let bonds = source
            .bonds
            .iter()
            .filter(|bond| members.contains(&bond.a))
            .count();
        if bonds + 1 != component.len()
            || component
                .iter()
                .any(|id| crate::models3d::hides(source, *id))
        {
            return Err(Error::Unsupported);
        }
        let chain = longest_carbon_chain(source, &component, &adjacent)?;
        let chain_members: HashSet<_> = chain.iter().copied().collect();
        let mut stereo: HashMap<u64, AtomStereo> = assigned
            .atoms
            .iter()
            .filter(|a| members.contains(&a.id))
            .filter_map(|a| a.stereo.clone().map(|stereo| (a.id, stereo)))
            .collect();
        if stereo.is_empty() || stereo.keys().any(|id| !chain_members.contains(id)) {
            return Err(Error::Unsupported);
        }
        for pair in chain.windows(2) {
            if source
                .bond_between(
                    *pair.first().ok_or(Error::Unsupported)?,
                    *pair.last().ok_or(Error::Unsupported)?,
                )
                .is_none_or(|(_, bond)| bond.order != 1)
            {
                return Err(Error::Unsupported);
            }
        }
        let center = component.iter().try_fold(Point::default(), |sum, id| {
            let p = atom(source, *id)?.position;
            Ok::<_, Error>(sum.offset(p.x / component.len() as f32, p.y / component.len() as f32))
        })?;
        for &id in &component {
            document.atom_mut(id).ok_or(Error::Selection)?.depth = 0.;
        }
        for bond in &mut document.bonds {
            if members.contains(&bond.a) {
                bond.projection = false;
                if matches!(
                    bond.display.as_str(),
                    "wedge" | "hash" | "hollow_wedge" | "hashed"
                ) {
                    bond.display = "plain".into();
                }
            }
        }
        for (index, &id) in chain.iter().enumerate() {
            let target = Point::new(
                center.x,
                center.y + (index as f32 - (chain.len() - 1) as f32 / 2.) * length,
            );
            document.atom_mut(id).ok_or(Error::Selection)?.position = target;
            let mut branches: Vec<_> = adjacent
                .get(&id)
                .ok_or(Error::Selection)?
                .iter()
                .copied()
                .filter(|other| !chain_members.contains(other))
                .collect();
            if let Some(assignment) = stereo.get_mut(&id) {
                if index == 0
                    || index + 1 == chain.len()
                    || adjacent
                        .get(&id)
                        .ok_or(Error::Selection)?
                        .iter()
                        .any(|other| {
                            source
                                .bond_between(id, *other)
                                .is_none_or(|(_, b)| b.order != 1)
                        })
                {
                    return Err(Error::Unsupported);
                }
                if branches.len() == 1 && assignment.neighbors.len() == 3 {
                    let molecule_index = molecule
                        .ids
                        .iter()
                        .position(|other| *other == id)
                        .ok_or(Error::Selection)?;
                    let graph_atom = molecule
                        .state
                        .graph
                        .atoms
                        .get(molecule_index)
                        .ok_or(Error::Selection)?;
                    let valence = molecule
                        .state
                        .valences
                        .get(molecule_index)
                        .ok_or(Error::Selection)?;
                    if u32::from(graph_atom.explicit_hydrogens) + valence.implicit_hydrogens != 1 {
                        return Err(Error::Unsupported);
                    }
                    let hydrogen = document.add_atom("H", target);
                    document.add_bond(id, hydrogen, 1, "plain");
                    adjacent
                        .get_mut(&id)
                        .ok_or(Error::Selection)?
                        .push(hydrogen);
                    adjacent.insert(hydrogen, vec![id]);
                    let value = document.atom_mut(id).ok_or(Error::Selection)?;
                    value.explicit_h = 0;
                    value.label_h = 0;
                    value.no_implicit = true;
                    assignment.neighbors.push(hydrogen);
                    branches.push(hydrogen);
                }
                let [left, right] = branches.as_mut_slice() else {
                    return Err(Error::Unsupported);
                };
                let up = *chain.get(index - 1).ok_or(Error::Unsupported)?;
                let down = *chain.get(index + 1).ok_or(Error::Unsupported)?;
                if !crate::fischer::equivalent(
                    assignment,
                    &crate::fischer::reading(up, *right, down, *left),
                ) {
                    std::mem::swap(left, right);
                }
                if !crate::fischer::equivalent(
                    assignment,
                    &crate::fischer::reading(up, *right, down, *left),
                ) {
                    return Err(Error::Stereo);
                }
            }
            for (branch_index, &branch) in branches.iter().enumerate() {
                let end = index == 0 || index + 1 == chain.len();
                let (dx, dy) = if end {
                    let vertical = if index == 0 { -0.6 } else { 0.6 };
                    (if branch_index % 2 == 0 { -0.8 } else { 0.8 }, vertical)
                } else {
                    (if branch_index == 0 { -1. } else { 1. }, 0.)
                };
                translate_subtree(
                    &mut document,
                    &adjacent,
                    branch,
                    id,
                    target.offset(dx * length, dy * length),
                )?;
            }
        }
        for (id, assignment) in stereo {
            document.atom_mut(id).ok_or(Error::Selection)?.stereo = Some(assignment);
            for bond in &mut document.bonds {
                if bond.a == id || bond.b == id {
                    bond.display = "plain".into();
                    bond.projection = true;
                }
            }
            let recognized =
                crate::fischer::recognize_center(&document, id).ok_or(Error::Stereo)?;
            if !crate::fischer::equivalent(
                atom(&document, id)?.stereo.as_ref().ok_or(Error::Stereo)?,
                &recognized.reading(),
            ) {
                return Err(Error::Stereo);
            }
        }
    }
    document.validate().map_err(Error::Drawing)?;
    Ok(document)
}

#[cfg(test)]
mod tests;
