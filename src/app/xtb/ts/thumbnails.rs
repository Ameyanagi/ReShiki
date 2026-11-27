//! Saved-geometry preparation for the reactant, TS and product profile models.

use crate::app::models3d::thumbnail::Thumbnail;
use reshiki::document::Document;
use reshiki::qcdata::{CalcResult, Frame, FrameHash, IrcPath};
use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::app) struct BondChanges {
    pub forming: Vec<[usize; 2]>,
    pub breaking: Vec<[usize; 2]>,
}

/// Planned changes have already been put in the arrow direction by the
/// search worker. Legacy results fall back to their saved endpoint graphs.
pub(in crate::app) fn bond_changes(
    result: &CalcResult,
    reactant: &Frame,
    product: &Frame,
) -> BondChanges {
    let count = reactant.numbers.len();
    if reactant.numbers != product.numbers {
        return BondChanges::default();
    }
    if let Some(search) = super::saved_search(result)
        && (!search.forming_bonds.is_empty() || !search.breaking_bonds.is_empty())
    {
        let valid = |pairs: &[[usize; 2]]| {
            pairs
                .iter()
                .copied()
                .filter(|&[a, b]| a < count && b < count && a != b)
                .map(|[a, b]| [a.min(b), a.max(b)])
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        return BondChanges {
            forming: valid(&search.forming_bonds),
            breaking: valid(&search.breaking_bonds),
        };
    }
    endpoint_changes(reactant, product)
}

pub(super) fn endpoint_changes(reactant: &Frame, product: &Frame) -> BondChanges {
    if reactant.numbers != product.numbers {
        return BondChanges::default();
    }
    let (Some(reactant), Some(product)) = (bonds(reactant), bonds(product)) else {
        return BondChanges::default();
    };
    let reactant: BTreeSet<_> = reactant.into_iter().collect();
    let product: BTreeSet<_> = product.into_iter().collect();
    BondChanges {
        forming: product.difference(&reactant).copied().collect(),
        breaking: reactant.difference(&product).copied().collect(),
    }
}

fn bonds(frame: &Frame) -> Option<Vec<[usize; 2]>> {
    let positions: Vec<_> = frame
        .positions_angstrom
        .iter()
        .flatten()
        .map(|value| value / reshiki_optimize::ANGSTROM_PER_BOHR)
        .collect();
    reshiki_optimize::ts::bonds(&frame.numbers, &positions)
        .ok()
        .map(|pairs| pairs.into_iter().map(|(a, b)| [a, b]).collect())
}

#[derive(PartialEq, Eq)]
struct Key {
    frames: [FrameHash; 3],
    changes: BondChanges,
}

thread_local! {
    // The UI rebuilds charts as selections and playback change. Preparing a
    // saved model once avoids repeating graph perception and sphere shading.
    static PREPARED: RefCell<VecDeque<(Key, [Thumbnail; 3])>> = const {
        RefCell::new(VecDeque::new())
    };
}

/// The result's exact optimized R / refined TS / optimized P geometries. A
/// common view rotation makes atom correspondence visible across the models.
/// Missing saved endpoints omit the models rather than substitute drawings.
pub(in crate::app) fn from_result(doc: &Document, result: &CalcResult) -> Option<[Thumbnail; 3]> {
    let endpoints = result.endpoints?;
    let reactant = doc.science.frame(&endpoints.reactant)?;
    let transition = doc.science.frame(&result.frame)?;
    let product = doc.science.frame(&endpoints.product)?;
    prepare(
        doc,
        result,
        [reactant, transition, product],
        [endpoints.reactant, result.frame, endpoints.product],
    )
}

/// Full IRC profiles show the geometries that produced their endpoint energies,
/// which may differ from the optimized NEB endpoint geometries.
pub(in crate::app) fn from_irc(
    doc: &Document,
    result: &CalcResult,
    path: &IrcPath,
) -> Option<[Thumbnail; 3]> {
    let reactant = path.relaxed_frame(0)?;
    let transition = path.frame(path.ts_index)?;
    let product = path.relaxed_frame(1)?;
    let frames = [&reactant, &transition, &product];
    // Hash only these three displayed frames, never the full IRC trajectory.
    let hashes = frames.map(Frame::hash);
    prepare(doc, result, frames, hashes)
}

fn prepare(
    doc: &Document,
    result: &CalcResult,
    [reactant, transition, product]: [&Frame; 3],
    hashes: [FrameHash; 3],
) -> Option<[Thumbnail; 3]> {
    if reactant.numbers != transition.numbers || reactant.numbers != product.numbers {
        return None;
    }
    let key = Key {
        frames: hashes,
        changes: super::saved_search(result)
            .map(|search| BondChanges {
                forming: search.forming_bonds.clone(),
                breaking: search.breaking_bonds.clone(),
            })
            .unwrap_or_default(),
    };
    if let Some(models) = PREPARED.with_borrow_mut(|cache| {
        let index = cache.iter().position(|(cached, _)| cached == &key)?;
        let entry = cache.remove(index)?;
        let models = entry.1.clone();
        cache.push_front(entry);
        Some(models)
    }) {
        return Some(models.map(|model| model.in_document(doc)));
    }
    let reactant_bonds = bonds(reactant)?;
    let product_bonds = bonds(product)?;
    let changes = bond_changes(result, reactant, product);
    // Retain stable endpoint bonds at the saddle. Planned changing bonds get
    // dashed overlays even when their TS distances exceed the bond cutoff.
    let transition_bonds: Vec<_> = reactant_bonds
        .iter()
        .chain(&product_bonds)
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let points: Vec<_> = transition
        .positions_angstrom
        .iter()
        .map(|point| point.map(|value| value as f32))
        .collect();
    let rotation = reshiki::models3d::initial_rotation(&points);
    let models = [
        Thumbnail::new(reactant, &reactant_bonds, &[], &[], rotation)?,
        Thumbnail::new(
            transition,
            &transition_bonds,
            &changes.forming,
            &changes.breaking,
            rotation,
        )?,
        Thumbnail::new(product, &product_bonds, &[], &[], rotation)?,
    ];
    PREPARED.with_borrow_mut(|cache| {
        cache.push_front((key, models.clone()));
        cache.truncate(8);
    });
    Some(models.map(|model| model.in_document(doc)))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::app::xtb::run;
    use reshiki::qcdata::{RunSummary, Task, TransitionStateMapping, TransitionStateRun};

    fn frame(positions_angstrom: Vec<[f64; 3]>) -> Frame {
        Frame {
            numbers: vec![6, 6, 8],
            positions_angstrom,
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        }
    }

    fn result(frame: &Frame) -> CalcResult {
        let mut result = run::example_computed(-5., 1.)
            .to_calc_result(frame, Task::Energy, &run::example_engine(), None)
            .unwrap();
        result.task = Task::TransitionState;
        result
    }

    #[test]
    fn legacy_changes_come_from_saved_endpoint_graphs_in_frame_order() {
        let reactant = frame(vec![[0., 0., 0.], [1.4, 0., 0.], [4., 0., 0.]]);
        let product = frame(vec![[0., 0., 0.], [4., 0., 0.], [1.3, 0., 0.]]);
        let changes = bond_changes(&result(&reactant), &reactant, &product);
        assert_eq!(changes.forming, [[0, 2]]);
        assert_eq!(changes.breaking, [[0, 1]]);
    }

    #[test]
    fn saved_planned_changes_override_endpoint_perception() {
        let frame = frame(vec![[0., 0., 0.], [1.4, 0., 0.], [4., 0., 0.]]);
        let mut result = result(&frame);
        result.run = Some(RunSummary {
            seconds: 1.,
            scf_iterations: vec![],
            finished: None,
            search: None,
            transition_state: Some(Box::new(TransitionStateRun {
                method: Default::default(),
                refined_from: None,
                irc: None,
                arrow: None,
                context: None,
                free_energy: false,
                endpoint_energies_hartree: None,
                endpoint_thermo: None,
                reached_positions_angstrom: vec![],
                saddle_converged: true,
                imaginary: 1,
                imaginary_cm1: Some(-393.),
                climbing_image: 1,
                forming_bonds: vec![[2, 0]],
                breaking_bonds: vec![[0, 1]],
                mapping: TransitionStateMapping {
                    formed: vec![],
                    broken: vec![],
                    hydrogen_moves: vec![],
                    automatic: false,
                },
                diagnostics: vec![],
            })),
        });
        let changes = bond_changes(&result, &frame, &frame);
        assert_eq!(changes.forming, [[0, 2]]);
        assert_eq!(changes.breaking, [[0, 1]]);
    }

    #[test]
    fn irc_models_use_relaxed_ends_without_neb_endpoint_substitution() {
        use reshiki::qcdata::{IrcEndpoint, IrcRelaxedEnd, Trajectory};
        let frames = [
            frame(vec![[0., 0., 0.], [1.4, 0., 0.], [4., 0., 0.]]),
            frame(vec![[0., 0., 0.], [2., 0.2, 0.], [3., 0., 0.]]),
            frame(vec![[0., 0., 0.], [4., 0., 0.], [1.3, 0., 0.]]),
        ];
        let trajectory = Trajectory {
            numbers: frames[0].numbers.clone(),
            isotopes: vec![],
            masses_amu: vec![12., 12., 16.],
            charge: 0,
            multiplicity: 1,
            positions_angstrom: frames
                .iter()
                .flat_map(|frame| frame.positions_angstrom.iter().flatten().copied())
                .collect(),
            energies_hartree: vec![-5.01, -4.97, -5.02],
            labels: vec![],
            minimizer_energies_hartree: vec![],
            cluster_sizes: vec![],
            rmsd_to_best_angstrom: vec![],
        };
        let mut path = IrcPath::new(
            trajectory,
            1,
            [IrcEndpoint::Reactant, IrcEndpoint::Product],
            [true; 2],
            [0, 2].map(|i| IrcRelaxedEnd {
                energy_hartree: -5.05,
                positions_angstrom: frames[i]
                    .positions_angstrom
                    .iter()
                    .flatten()
                    .copied()
                    .collect(),
            }),
        )
        .unwrap();
        let doc = Document::default();
        let result = result(&path.frame(1).unwrap());
        // There are no optimized endpoint frames to fall back to in this doc.
        assert!(from_result(&doc, &result).is_none());
        assert!(from_irc(&doc, &result, &path).is_some());
        let hashes = [path.relaxed_frame(0), path.frame(1), path.relaxed_frame(1)]
            .map(|frame| frame.unwrap().hash());
        PREPARED.with_borrow(|cache| assert_eq!(cache.front().unwrap().0.frames, hashes));
        // The same saddle with a different terminal geometry has a distinct key.
        path.relaxed_ends.as_mut().unwrap()[1].positions_angstrom[6] += 0.5;
        assert!(from_irc(&doc, &result, &path).is_some());
        PREPARED.with_borrow(|cache| {
            let actual = cache.front().unwrap().0.frames;
            assert_eq!(&actual[..2], &hashes[..2]);
            assert_ne!(actual[2], hashes[2]);
        });
        path.trajectory.positions_angstrom.clear();
        assert!(from_irc(&doc, &result, &path).is_none());
    }
}
