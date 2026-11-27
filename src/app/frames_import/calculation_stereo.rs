//! Perceive stereo in the explicit geometry of an opened calculation output.
// Both callers supply validated graphs. The component, neighbor and candidate
// tables are built from those graphs and share their atom and bond indices.
#![allow(clippy::indexing_slicing)]

use reshiki::chemistry::{
    document,
    graph::Graph,
    kekulize::Direction,
    ranking::Metadata,
    sanitize,
    stereo::{
        self, BondGeometry, Point3, SpatialAnnotations, SpatialOptions, perception,
        wedging::{self, Conformer},
    },
};
use reshiki::document::{AtomStereo, Document};
use reshiki::qcdata::Frame;

#[derive(Clone, Copy)]
struct Neighbor {
    atom: usize,
    bond: usize,
}

fn adjacency(graph: &Graph) -> Vec<Vec<Neighbor>> {
    let mut adjacent = vec![Vec::new(); graph.atoms.len()];
    for (bond, edge) in graph.bonds.iter().enumerate() {
        adjacent[edge.a].push(Neighbor { atom: edge.b, bond });
        adjacent[edge.b].push(Neighbor { atom: edge.a, bond });
    }
    adjacent
}

#[derive(Default)]
struct Component {
    atoms: Vec<usize>,
    bonds: Vec<usize>,
}

fn components(graph: &Graph, adjacent: &[Vec<Neighbor>]) -> Vec<Component> {
    let mut labels = vec![usize::MAX; graph.atoms.len()];
    let mut parts = Vec::new();
    let mut stack = Vec::new();
    for start in 0..labels.len() {
        if labels[start] != usize::MAX {
            continue;
        }
        let label = parts.len();
        parts.push(Component::default());
        labels[start] = label;
        stack.push(start);
        while let Some(atom) = stack.pop() {
            for neighbor in &adjacent[atom] {
                if labels[neighbor.atom] == usize::MAX {
                    labels[neighbor.atom] = label;
                    stack.push(neighbor.atom);
                }
            }
        }
    }
    // Keep the input atom and bond ordering for winding and geometry.
    for (atom, label) in labels.iter().copied().enumerate() {
        parts[label].atoms.push(atom);
    }
    for (bond, edge) in graph.bonds.iter().enumerate() {
        parts[labels[edge.a]].bonds.push(bond);
    }
    parts
}

/// Only compare ligands that end here. Equal elements on longer branches, as
/// in 2-butanol, can be distinguished by their substituents during perception.
fn distinct_terminal_ligands(
    graph: &Graph,
    adjacent: &[Vec<Neighbor>],
    neighbors: impl Iterator<Item = Neighbor> + Clone,
) -> bool {
    let normalize = |index: usize| {
        let mut atom = graph.atoms[index].clone();
        if atom.atomic_number == 1 && atom.isotope == 1 {
            atom.isotope = 0;
        }
        atom
    };
    for (i, a) in neighbors.clone().enumerate() {
        if adjacent[a.atom].len() != 1 {
            continue;
        }
        let atom = normalize(a.atom);
        for b in neighbors.clone().take(i) {
            if adjacent[b.atom].len() == 1
                && graph.bonds[a.bond].order == graph.bonds[b.bond].order
                && atom == normalize(b.atom)
            {
                return false;
            }
        }
    }
    true
}

fn signed_volume(origin: [f64; 3], points: [[f64; 3]; 3]) -> f64 {
    let [u, v, w] = points.map(|point| std::array::from_fn::<_, 3, _>(|i| point[i] - origin[i]));
    u[0] * (v[1] * w[2] - v[2] * w[1])
        + u[1] * (v[2] * w[0] - v[0] * w[2])
        + u[2] * (v[0] * w[1] - v[1] * w[0])
}

fn tetrahedral_candidate(
    graph: &Graph,
    adjacent: &[Vec<Neighbor>],
    positions: &[[f64; 3]],
    index: usize,
) -> bool {
    let atom = &graph.atoms[index];
    // Main-group centres that support tetrahedral winding. Metal
    // coordination geometry is outside this calculation-output inference.
    if !matches!(atom.atomic_number, 5..=7 | 14..=16 | 32..=34 | 50..=52 | 82) {
        return false;
    }
    let neighbors = &adjacent[index];
    if !matches!(neighbors.len(), 3 | 4)
        || !distinct_terminal_ligands(graph, adjacent, neighbors.iter().copied())
    {
        return false;
    }
    let point = |neighbor: Neighbor| positions[neighbor.atom];
    let center = positions[index];
    match *neighbors.as_slice() {
        [a, b, c, d] => {
            let [a, b, c, d] = [a, b, c, d].map(point);
            let scale = [a, b, c, d]
                .iter()
                .map(|p| (0..3).map(|i| (p[i] - center[i]).powi(2)).sum::<f64>())
                .fold(0.0_f64, f64::max)
                .sqrt();
            // Moving the centre out of a square plane can give nonzero
            // center-relative volume while the ligand tetrahedron stays flat,
            // and a seesaw centre sits on an edge of its ligand tetrahedron:
            // a tetrahedral centre lies well inside it.
            let total = signed_volume(a, [b, c, d]);
            let inside = [
                signed_volume(center, [b, c, d]),
                signed_volume(a, [center, c, d]),
                signed_volume(a, [b, center, d]),
                signed_volume(a, [b, c, center]),
            ]
            .iter()
            .all(|part| part * total > 0. && part.abs() > 0.05 * total.abs());
            total.abs() > 0.1 * scale.powi(3) && inside
        }
        [a, b, c] => {
            (!atom.no_implicit
                || atom.explicit_hydrogens == 1
                || matches!(atom.atomic_number, 16 | 34))
                && signed_volume(center, [a, b, c].map(point)).abs() > 0.1
        }
        _ => false,
    }
}

fn alkene_candidate(graph: &Graph, adjacent: &[Vec<Neighbor>], index: usize) -> bool {
    let bond = &graph.bonds[index];
    bond.order == 2
        && [bond.a, bond.b].into_iter().all(|atom| {
            matches!(adjacent[atom].len(), 2 | 3)
                && distinct_terminal_ligands(
                    graph,
                    adjacent,
                    adjacent[atom].iter().copied().filter(|n| n.bond != index),
                )
        })
}

pub(super) fn perceive(graph: &Graph, frame: &Frame) -> BondGeometry {
    let mut result = BondGeometry {
        metadata: Metadata::unspecified(graph),
        directions: vec![Direction::None; graph.bonds.len()],
    };
    if frame.positions_angstrom.len() != graph.atoms.len() {
        return result;
    }
    let adjacent = adjacency(graph);
    let tetrahedral: Vec<_> = (0..graph.atoms.len())
        .map(|i| tetrahedral_candidate(graph, &adjacent, &frame.positions_angstrom, i))
        .collect();
    let alkenes: Vec<_> = (0..graph.bonds.len())
        .map(|i| alkene_candidate(graph, &adjacent, i))
        .collect();
    if !tetrahedral.iter().chain(&alkenes).any(|&eligible| eligible) {
        return result;
    }

    let mut local_index = vec![0; graph.atoms.len()];
    for component in components(graph, &adjacent) {
        let has_tetrahedral = component.atoms.iter().any(|&i| tetrahedral[i]);
        let has_alkene = component.bonds.iter().any(|&i| alkenes[i]);
        if !has_tetrahedral && !has_alkene {
            continue;
        }
        for (local, &global) in component.atoms.iter().enumerate() {
            local_index[global] = local;
        }
        let local = Graph {
            atoms: component
                .atoms
                .iter()
                .map(|&i| graph.atoms[i].clone())
                .collect(),
            bonds: component
                .bonds
                .iter()
                .map(|&i| {
                    let mut bond = graph.bonds[i].clone();
                    bond.a = local_index[bond.a];
                    bond.b = local_index[bond.b];
                    bond
                })
                .collect(),
        };
        let conformer = Conformer {
            positions: component
                .atoms
                .iter()
                .map(|&i| {
                    let [x, y, z] = frame.positions_angstrom[i];
                    Point3 { x, y, z }
                })
                .collect(),
            is_3d: true,
        };
        let candidates = component.atoms.iter().map(|&i| tetrahedral[i]);
        let Some(stereo) =
            perceive_component(&local, conformer, candidates, has_tetrahedral, has_alkene)
        else {
            continue;
        };
        for (i, atom) in component.atoms.iter().zip(stereo.metadata.atoms) {
            result.metadata.atoms[*i] = atom;
        }
        for ((&i, mut bond), direction) in component
            .bonds
            .iter()
            .zip(stereo.metadata.bonds)
            .zip(stereo.directions)
        {
            let Some(controls) = bond
                .stereo_atoms
                .iter()
                .map(|&atom| component.atoms.get(atom).copied())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            bond.stereo_atoms = controls;
            result.metadata.bonds[i] = bond;
            result.directions[i] = direction;
        }
    }
    result
}

/// Use the spatial and bond-geometry passes from 3D molfile import on one
/// fragment. Unsupported valences in another fragment cannot affect this one.
fn perceive_component(
    graph: &Graph,
    conformer: Conformer,
    candidates: impl Iterator<Item = bool>,
    has_tetrahedral: bool,
    has_alkene: bool,
) -> Option<BondGeometry> {
    let mut metadata = Metadata::unspecified(graph);
    let directions = vec![Direction::None; graph.bonds.len()];
    if has_tetrahedral {
        let spatial = stereo::from_3d(
            graph,
            &metadata,
            &directions,
            Some(&conformer),
            &SpatialAnnotations {
                non_explicit: vec![None; graph.atoms.len()],
                done: None,
            },
            SpatialOptions {
                allow_nontetrahedral: false,
                ..SpatialOptions::default()
            },
        )
        .ok()?;
        metadata = spatial.metadata;
        for (atom, eligible) in metadata.atoms.iter_mut().zip(candidates) {
            if !eligible {
                atom.chiral_tag = 0;
            }
        }
        if !has_alkene && metadata.atoms.iter().all(|a| a.chiral_tag == 0) {
            return None;
        }
    }
    let clean = sanitize::sanitize(graph, &metadata, &directions).ok()?;
    let geometry = if has_alkene {
        stereo::detect_bond_stereo(
            &clean.graph,
            &clean.metadata,
            &clean.directions,
            Some(&conformer.positions),
            &clean.rings,
        )
        .ok()?
    } else {
        BondGeometry {
            metadata: clean.metadata,
            directions: clean.directions,
        }
    };
    let state = perception::perceive(
        &perception::State {
            properties: perception::Properties::unspecified(&clean.graph),
            graph: clean.graph,
            metadata: geometry.metadata,
            directions: geometry.directions,
            valences: clean.valences,
            conjugated: clean.conjugated,
            hybridizations: clean.hybridizations,
            rings: perception::RingCache {
                kind: perception::RingKind::Symmetric,
                atoms: clean.rings,
            },
            conformer: Some(conformer),
        },
        perception::Options {
            clean: true,
            force: true,
            flag_possible: false,
        },
    )
    .ok()?;
    Some(BondGeometry {
        metadata: state.metadata,
        directions: state.directions,
    })
}

/// Preserve the folded graph's winding and double-bond controls by drawing ID.
/// Seed ordinary stereo marks so 2D cleanup can adjust them to its new layout.
pub(super) fn draw(doc: &mut Document, graph: &Graph, metadata: &Metadata) {
    if doc.atoms.len() != graph.atoms.len() || doc.bonds.len() != graph.bonds.len() {
        return;
    }
    let ids: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
    for (bond, metadata) in doc.bonds.iter_mut().zip(&metadata.bonds) {
        if let (2, &[a, b]) = (bond.order, metadata.stereo_atoms.as_slice())
            && let (Some(&a), Some(&b)) = (ids.get(a), ids.get(b))
        {
            bond.stereo = match metadata.stereo {
                2 => Some("z"),
                3 => Some("e"),
                4 => Some("cis"),
                5 => Some("trans"),
                _ => None,
            }
            .map(str::to_owned);
            if bond.stereo.is_some() {
                bond.stereo_atoms = vec![a, b];
                bond.stereo_authoritative = true;
            }
        }
    }
    if !metadata.atoms.iter().any(|a| matches!(a.chiral_tag, 1 | 2)) {
        return;
    }
    let adjacent = adjacency(graph);
    for (i, (atom, meta)) in doc.atoms.iter_mut().zip(&metadata.atoms).enumerate() {
        if matches!(meta.chiral_tag, 1 | 2) {
            atom.stereo = Some(AtomStereo {
                winding: if meta.chiral_tag == 1 { "cw" } else { "ccw" }.into(),
                neighbors: adjacent[i].iter().map(|n| ids[n.atom]).collect(),
            });
        }
    }
    for component in components(graph, &adjacent) {
        if component
            .atoms
            .iter()
            .any(|&i| doc.atoms[i].stereo.is_some())
        {
            draw_component_wedges(doc, &component);
        }
    }
}

fn draw_component_wedges(doc: &mut Document, component: &Component) {
    let fragment = Document {
        atoms: component
            .atoms
            .iter()
            .map(|&i| doc.atoms[i].clone())
            .collect(),
        bonds: component
            .bonds
            .iter()
            .map(|&i| doc.bonds[i].clone())
            .collect(),
        ..Document::default()
    };
    let Ok(molecule) = document::prepare(&fragment) else {
        return;
    };
    let state = molecule.state;
    let Ok(wedged) = wedging::wedge_molecule(
        &wedging::WedgeState {
            graph: state.graph,
            metadata: state.metadata,
            directions: state.directions,
            rings: state.rings,
        },
        &wedging::WedgeProperties {
            valences: state.valences,
            attachment_points: vec![false; molecule.ids.len()],
        },
        state.conformer.as_ref(),
        doc.drawing_style.two_wedge_stereocentres,
    ) else {
        return;
    };
    for ((&i, wedged), direction) in component
        .bonds
        .iter()
        .zip(wedged.graph.bonds)
        .zip(wedged.directions)
    {
        let display = match direction {
            Direction::Wedge => "wedge",
            Direction::Hash => "hash",
            _ => continue,
        };
        let bond = &mut doc.bonds[i];
        if bond.order == 1
            && let (Some(&a), Some(&b)) = (molecule.ids.get(wedged.a), molecule.ids.get(wedged.b))
        {
            bond.a = a;
            bond.b = b;
            bond.display = display.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::chemistry::graph::{Atom, Bond};

    fn atom(atomic_number: u8) -> Atom {
        Atom {
            atomic_number,
            no_implicit: true,
            ..Atom::default()
        }
    }

    fn single(a: usize, b: usize) -> Bond {
        Bond {
            a,
            b,
            order: 1,
            aromatic: false,
        }
    }

    #[test]
    fn tetrahedral_prefilter_checks_both_the_element_and_the_ligand_volume() {
        let mut graph = Graph {
            atoms: [6, 9, 17, 35, 53].map(atom).to_vec(),
            bonds: (1..5).map(|i| single(0, i)).collect(),
        };
        let adjacent = adjacency(&graph);
        let near_planar = [
            [0.0, 0.0, 0.05],
            [1.5, 0.0, 0.0],
            [0.0, 1.5, 0.0],
            [-1.5, 0.0, 0.0],
            [0.0, -1.5, 0.01],
        ];
        let tetrahedral = [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, -1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
        ];
        assert!(!tetrahedral_candidate(&graph, &adjacent, &near_planar, 0));
        assert!(tetrahedral_candidate(&graph, &adjacent, &tetrahedral, 0));
        graph.atoms[0].atomic_number = 78;
        assert!(!tetrahedral_candidate(&graph, &adjacent, &tetrahedral, 0));
    }

    #[test]
    fn terminal_hydrogens_rule_out_ordinary_alkane_centres_before_perception() {
        let carbons = 660;
        let mut graph = Graph {
            atoms: vec![atom(6); carbons],
            bonds: (1..carbons).map(|i| single(i - 1, i)).collect(),
        };
        let mut positions: Vec<_> = (0..carbons)
            .map(|i| [i as f64, (i % 2) as f64, 0.0])
            .collect();
        for center in 0..carbons {
            let [x, y, _] = positions[center];
            // Away from both carbon neighbours, so each centre is tetrahedral.
            let away = if y == 0.0 { -0.6 } else { 0.6 };
            for z in [-1.0, 1.0] {
                graph.bonds.push(single(center, graph.atoms.len()));
                graph.atoms.push(atom(1));
                positions.push([x, y + away, z]);
            }
        }
        for (center, direction) in [(0, -1.0), (carbons - 1, 1.0)] {
            let [x, y, _] = positions[center];
            graph.bonds.push(single(center, graph.atoms.len()));
            graph.atoms.push(atom(1));
            positions.push([x + direction, y, 0.0]);
        }
        let adjacent = adjacency(&graph);
        assert!(
            (0..graph.atoms.len())
                .all(|i| !tetrahedral_candidate(&graph, &adjacent, &positions, i))
        );
        // A deuterated branch still needs ranking, even though the two
        // hydrogen ligands have the same element.
        graph.atoms[carbons + 4].isotope = 2;
        assert!(tetrahedral_candidate(&graph, &adjacent, &positions, 2));
    }
}
