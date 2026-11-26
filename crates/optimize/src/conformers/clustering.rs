//! Select diverse low-energy representatives from minimized starts with Butina clustering.
//!
//! Adapted from D. Butina, J. Chem. Inf. Comput. Sci. 39, 747–750 (1999),
//! using a heavy-atom RMSD cutoff and updating neighbor counts after each assignment
//! \[Butina1999Clustering\].
//!
//! Full references: docs/references/xtb.md.

use std::{cmp::Reverse, sync::Mutex, thread};

use super::{
    Automorphisms, Error, MAX_STARTS, PerCluster, Settings, Topology, alignment::Centered,
    validate_positions,
};
use crate::KCAL_PER_HARTREE;

/// One minimized start in ångströms and hartree.
#[derive(Clone, Debug)]
pub struct Start {
    pub positions: Vec<[f64; 3]>,
    pub energy_hartree: f64,
}

/// Start indices retained at each stage of representative selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clustering {
    /// Unique starts in order of increasing energy, with input order breaking ties.
    pub kept: Vec<usize>,
    /// Members sorted by energy then index, clusters ordered by their first member.
    pub clusters: Vec<Vec<usize>>,
    /// Start indices in the order in which they receive reoptimization slots.
    pub representatives: Vec<usize>,
    pub truncated_automorphisms: bool,
}

/// Remove exact duplicates, cluster by heavy-atom RMSD, and choose representatives.
pub fn cluster(
    topology: &Topology,
    starts: &[Start],
    settings: &Settings,
) -> Result<Clustering, Error> {
    settings.validate()?;
    if starts.len() > MAX_STARTS as usize {
        return Err(Error::Invalid(format!(
            "at most {MAX_STARTS} starts are supported"
        )));
    }
    for start in starts {
        validate_positions(topology, &start.positions)?;
        if !start.energy_hartree.is_finite() {
            return Err(Error::Invalid("start energy must be finite".into()));
        }
    }
    let autos = topology.automorphisms(settings.automorphism_cap);
    let centered: Vec<_> = starts
        .iter()
        .map(|s| Centered::new(topology, &s.positions))
        .collect();
    let mut order: Vec<_> = (0..starts.len()).collect();
    // Stable sort preserves input order for equal energies, including signed zero.
    order.sort_by(|&a, &b| {
        starts[a]
            .energy_hartree
            .partial_cmp(&starts[b].energy_hartree)
            .unwrap()
    });
    let (kept, mut rows) = remove_duplicates(&order, starts, &centered, &autos);
    complete_distances(&mut rows, &kept, &centered, &autos);
    let clusters = butina(&rows, &kept, settings.diversity.threshold_angstrom());
    let representatives = select_representatives(&clusters, starts, settings);
    Ok(Clustering {
        kept,
        clusters,
        representatives,
        truncated_automorphisms: autos.truncated(),
    })
}

/// A row of the lower triangular RMSD matrix. The duplicate pass fills the suffix
/// for earlier starts within the energy tolerance, leaving its prefix to the workers.
struct DistanceRow {
    values: Vec<f64>,
    pending: usize,
}

fn remove_duplicates(
    order: &[usize],
    starts: &[Start],
    centered: &[Centered],
    autos: &Automorphisms,
) -> (Vec<usize>, Vec<DistanceRow>) {
    const ENERGY_TOLERANCE: f64 = 0.1 / KCAL_PER_HARTREE;
    let mut kept: Vec<usize> = Vec::new();
    let mut rows = Vec::new();
    let mut pending = 0;
    'starts: for &index in order {
        let energy = starts[index].energy_hartree;
        // Both the current energy and the energies in kept are nondecreasing.
        while pending < kept.len()
            && energy - starts[kept[pending]].energy_hartree >= ENERGY_TOLERANCE
        {
            pending += 1;
        }
        let mut values = Vec::with_capacity(kept.len());
        values.resize(pending, 0.0);
        for &earlier in &kept[pending..] {
            let distance = centered[earlier].distance(&centered[index], autos);
            if distance < 0.125 {
                continue 'starts;
            }
            values.push(distance);
        }
        rows.push(DistanceRow { values, pending });
        kept.push(index);
    }
    (kept, rows)
}

fn complete_distances(
    rows: &mut [DistanceRow],
    kept: &[usize],
    centered: &[Centered],
    autos: &Automorphisms,
) {
    let fill = |offset: usize, block: &mut [DistanceRow]| {
        for (within_block, row) in block.iter_mut().enumerate() {
            let mobile = &centered[kept[offset + within_block]];
            for (value, &earlier) in row.values[..row.pending].iter_mut().zip(kept) {
                *value = centered[earlier].distance(mobile, autos);
            }
        }
    };
    let workers = if rows.len() > 64 {
        thread::available_parallelism().map_or(1, |n| n.get().min(8))
    } else {
        1
    };
    if workers == 1 {
        fill(0, rows);
        return;
    }
    // Blocks go through a queue that the calling thread also drains, so a
    // thread that cannot start only means less parallelism, never a panic.
    let block_size = rows.len().div_ceil(workers);
    let queue = Mutex::new(rows.chunks_mut(block_size).enumerate().collect::<Vec<_>>());
    let work = || {
        loop {
            let next = queue.lock().expect("no worker panics").pop();
            let Some((block, chunk)) = next else { break };
            fill(block * block_size, chunk);
        }
    };
    thread::scope(|scope| {
        for _ in 1..workers {
            if thread::Builder::new().spawn_scoped(scope, work).is_err() {
                break;
            }
        }
        work();
    });
}

fn butina(rows: &[DistanceRow], kept: &[usize], cutoff: f64) -> Vec<Vec<usize>> {
    let mut neighbors = vec![Vec::new(); kept.len()];
    for (i, row) in rows.iter().enumerate() {
        for (j, &distance) in row.values.iter().enumerate() {
            if distance <= cutoff {
                neighbors[i].push(j);
                neighbors[j].push(i);
            }
        }
    }
    // Indices in kept already encode the energy and input-index tie breakers.
    let mut degrees: Vec<_> = neighbors.iter().map(|list| Some(list.len())).collect();
    let mut clusters = Vec::new();
    while let Some((_, Reverse(center))) = degrees
        .iter()
        .enumerate()
        .filter_map(|(i, &degree)| degree.map(|count| (count, Reverse(i))))
        .max()
    {
        let mut members = vec![center];
        members.extend(
            neighbors[center]
                .iter()
                .copied()
                .filter(|&i| degrees[i].is_some()),
        );
        for &i in &members {
            degrees[i] = None;
        }
        for &i in &members {
            for &j in &neighbors[i] {
                if let Some(count) = &mut degrees[j] {
                    *count -= 1;
                }
            }
        }
        members.sort_unstable();
        clusters.push(members);
    }
    clusters.sort_unstable_by_key(|members| members[0]);
    for members in &mut clusters {
        for i in members {
            *i = kept[*i];
        }
    }
    clusters
}

fn select_representatives(
    clusters: &[Vec<usize>],
    starts: &[Start],
    settings: &Settings,
) -> Vec<usize> {
    let Some(first) = clusters.first() else {
        return Vec::new();
    };
    let ceiling = starts[first[0]].energy_hartree + settings.window_kcal / KCAL_PER_HARTREE;
    let rounds = match settings.per_cluster {
        PerCluster::One => 1,
        PerCluster::Three => 3,
    };
    (0..rounds)
        .flat_map(|round| {
            clusters
                .iter()
                .filter_map(move |members| members.get(round).copied())
        })
        .filter(|&i| starts[i].energy_hartree <= ceiling)
        .take(settings.max_representatives)
        .collect()
}
