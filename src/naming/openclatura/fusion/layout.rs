//! Exact integer intrinsic fused-ring layouts and parser compatibility witnesses.
use super::super::{locants::retained_locant_sort_key, molecule::AtomSet};
use super::{
    config::{RingShapeSpec, fusion_nomenclature_config},
    faces::cycle_edges,
    model::{
        ComparisonKey, Face, FaceModel, FusedLayout, FusionCitationPlan, FusionComponentSpec,
        FusionNameAst, LayoutKind,
    },
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub type Point = (i64, i64);
type Positions = BTreeMap<usize, Point>;
type Orders = Vec<(usize, Vec<usize>)>;
type Shapes = BTreeMap<usize, RingShapeSpec>;
type Adjacent = BTreeSet<(usize, usize)>;
pub const OPSIN_CONSTRUCTION_NUMBERING: &str = "opsin_component_construction_order";
pub const OPSIN_RING_MAP_NUMBERING: &str = "opsin_ring_map_occupied_rows";
pub const OPSIN_COUPLED_PENTAGON_AXES: &str = "opsin_coupled_two_port_pentagon_axes";
pub const OPSIN_ENTRY_DIRECTIONS: &str = "opsin_entry_relative_ring_directions";
#[derive(Debug)]
pub(crate) struct Budget {
    pub limit: usize,
    pub used: usize,
}
impl Budget {
    pub fn new(limit: usize) -> Self {
        Self { limit, used: 0 }
    }
    pub fn spend(&mut self) -> Result<(), String> {
        self.used += 1;
        if self.used > self.limit {
            Err(format!(
                "intrinsic layout search exceeded its budget of {} states",
                self.limit
            ))
        } else {
            Ok(())
        }
    }
}
fn pair(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}
fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.abs()
}
fn lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        0
    } else {
        a / gcd(a, b) * b
    }
}
fn shapes_for(size: usize, system: &str) -> Vec<RingShapeSpec> {
    fusion_nomenclature_config()
        .ring_shapes
        .iter()
        .filter(|s| {
            s.ring_size == size && s.directed_entry_port.is_none() && s.coordinate_system == system
        })
        .cloned()
        .collect()
}
fn all_atoms(model: &FaceModel) -> AtomSet {
    model
        .faces
        .iter()
        .flat_map(|f| f.atom_cycle.iter().copied())
        .collect()
}
fn adjacent(model: &FaceModel) -> Adjacent {
    model
        .face_adjacency
        .iter()
        .map(|&(a, b, _)| pair(a, b))
        .collect()
}
fn order_map(orders: &Orders) -> BTreeMap<usize, Vec<usize>> {
    orders.iter().cloned().collect()
}
pub fn preferred_intrinsic_layouts(model: &FaceModel) -> Result<Vec<FusedLayout>, String> {
    let c = &fusion_nomenclature_config().search;
    preferred_intrinsic_layouts_with_options(model, c.layout_states, c.maximum_layouts, false)
}
pub fn preferred_intrinsic_layouts_with_budget(
    model: &FaceModel,
    budget: usize,
) -> Result<Vec<FusedLayout>, String> {
    preferred_intrinsic_layouts_with_options(
        model,
        budget,
        fusion_nomenclature_config().search.maximum_layouts,
        false,
    )
}
#[derive(Clone)]
struct PreferredLayoutCacheEntry {
    model: FaceModel,
    budget: usize,
    max_layouts: usize,
    opsin_ring_map: bool,
    layouts: Vec<FusedLayout>,
}
thread_local! {
    static PREFERRED_LAYOUT_CACHE: std::cell::RefCell<VecDeque<PreferredLayoutCacheEntry>> = const { std::cell::RefCell::new(VecDeque::new()) };
}
pub fn preferred_intrinsic_layouts_with_options(
    model: &FaceModel,
    budget: usize,
    max_layouts: usize,
    opsin_ring_map: bool,
) -> Result<Vec<FusedLayout>, String> {
    if let Some(layouts) = PREFERRED_LAYOUT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = cache.iter().position(|entry| {
            entry.model == *model
                && entry.budget == budget
                && entry.max_layouts == max_layouts
                && entry.opsin_ring_map == opsin_ring_map
        })?;
        let entry = cache.remove(index)?;
        let layouts = entry.layouts.clone();
        cache.push_back(entry);
        Some(layouts)
    }) {
        return Ok(layouts);
    }
    let layouts = intrinsic_fused_layouts_with_options(model, budget, max_layouts, opsin_ring_map)?;
    let layouts = if let Some(first) = layouts.first() {
        let best = first.orientation_score.clone();
        layouts
            .into_iter()
            .filter(|layout| layout.orientation_score == best)
            .collect()
    } else {
        layouts
    };
    PREFERRED_LAYOUT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() == 128 {
            cache.pop_front();
        }
        cache.push_back(PreferredLayoutCacheEntry {
            model: model.clone(),
            budget,
            max_layouts,
            opsin_ring_map,
            layouts: layouts.clone(),
        });
    });
    Ok(layouts)
}
pub fn intrinsic_fused_layouts_with_options(
    model: &FaceModel,
    search_budget: usize,
    max_layouts: usize,
    opsin_ring_map: bool,
) -> Result<Vec<FusedLayout>, String> {
    if search_budget < 1 || max_layouts < 1 {
        return Err("layout search budget and result limit must be positive".into());
    }
    if opsin_ring_map && !opsin_ring_map_tree(model) {
        return Ok(vec![]);
    }
    if let Some(layouts) = terminal_ring_proxy_layouts(model, search_budget, max_layouts)? {
        return Ok(layouts);
    }
    if model.faces.len() > 2 {
        for face in &model.faces {
            if [7, 8].contains(&face.size) {
                let layouts =
                    two_port_large_ring_layouts(model, search_budget, max_layouts, Some(face.id))?;
                if !layouts.is_empty() {
                    return Ok(layouts);
                }
            }
        }
    }
    let supported = |size| {
        fusion_nomenclature_config()
            .ring_shapes
            .iter()
            .any(|s| s.ring_size == size && s.directed_entry_port.is_none())
    };
    if model.faces.iter().any(|f| !supported(f.size)) {
        return if model.faces.len() == 2 {
            ordinary_large_bicycle_layouts(model, search_budget, max_layouts)
        } else {
            two_port_large_ring_layouts(model, search_budget, max_layouts, None)
        };
    }
    if !valid_face_adjacency(model) {
        return Ok(vec![]);
    }
    let mut systems = model
        .faces
        .iter()
        .map(|f| {
            fusion_nomenclature_config()
                .ring_shapes
                .iter()
                .filter(|s| s.ring_size == f.size && s.directed_entry_port.is_none())
                .map(|s| s.coordinate_system.clone())
                .collect::<BTreeSet<_>>()
        })
        .reduce(|a, b| a.intersection(&b).cloned().collect())
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    systems.sort_by_key(|s| (s != "eisenstein", s.clone()));
    let mut roots = model.faces.iter().collect::<Vec<_>>();
    roots.sort_by_key(|f| (f.size, f.id));
    let mut state = SearchState {
        budget: Budget::new(search_budget),
        completed: Vec::new(),
        completed_keys: BTreeSet::new(),
        embeddings: BTreeSet::new(),
        best_distortion: None,
        max_layouts,
        opsin_ring_map,
    };
    for system in systems {
        for root in &roots {
            for shape in shapes_for(root.size, &system) {
                if state
                    .best_distortion
                    .is_some_and(|b| shape.distortion_rank as i64 > b)
                {
                    continue;
                }
                for offset in 0..root.size {
                    for reverse in [false, true] {
                        state.budget.spend()?;
                        let order = oriented_cycle(&root.atom_cycle, offset, reverse);
                        let positions = order
                            .iter()
                            .copied()
                            .zip(shape.vertices.iter().copied())
                            .collect();
                        search_layouts(
                            model,
                            &vec![(root.id, order)],
                            &BTreeMap::from([(root.id, shape.clone())]),
                            &positions,
                            &system,
                            &mut state,
                        )?
                    }
                }
            }
        }
        if !state.completed.is_empty() {
            break;
        }
    }
    let mut layouts = state.completed;
    sort_layouts(&mut layouts);
    Ok(layouts)
}
type GeometryKey = (Vec<(usize, i64, i64)>, Vec<(usize, String)>);
type EmbeddingKey = (Vec<(usize, String)>, Vec<i128>);
struct SearchState {
    budget: Budget,
    completed: Vec<FusedLayout>,
    completed_keys: BTreeSet<GeometryKey>,
    embeddings: BTreeSet<EmbeddingKey>,
    best_distortion: Option<i64>,
    max_layouts: usize,
    opsin_ring_map: bool,
}
fn search_layouts(
    model: &FaceModel,
    orders: &Orders,
    shapes: &Shapes,
    positions: &Positions,
    system: &str,
    state: &mut SearchState,
) -> Result<(), String> {
    let distortion = layout_distortion(orders, shapes, positions, system);
    if state.best_distortion.is_some_and(|b| distortion > b) {
        return Ok(());
    }
    if orders.len() == model.faces.len() {
        if audit_layout(model, orders, positions, true) {
            let key = intrinsic_embedding_key(orders, shapes, positions, system);
            if !state.embeddings.insert(key) {
                return Ok(());
            }
            if state.best_distortion.is_none_or(|b| distortion < b) {
                state.best_distortion = Some(distortion);
                state.completed.clear();
                state.completed_keys.clear();
            }
            for layout in
                materialize_layouts(orders, shapes, positions, system, state.opsin_ring_map)
            {
                if state
                    .completed_keys
                    .insert((layout.atom_positions.clone(), layout.face_shapes.clone()))
                {
                    state.completed.push(layout);
                }
            }
            if state.completed.len() > state.max_layouts {
                return Err(format!(
                    "intrinsic layout search exceeded its budget of {} completed layouts",
                    state.max_layouts
                ));
            }
        }
        return Ok(());
    }
    let Some((next, neighbor, edge)) = next_face(model, orders) else {
        return Ok(());
    };
    #[allow(
        clippy::unwrap_used,
        reason = "next_face returns only IDs from the validated face adjacency"
    )]
    let face = model.faces.iter().find(|f| f.id == next).unwrap();
    #[allow(
        clippy::unwrap_used,
        reason = "the selected placed neighbor belongs to the validated face model"
    )]
    let placed = model.faces.iter().find(|f| f.id == neighbor).unwrap();
    let Some(endpoints) = edge_endpoints(placed, edge) else {
        return Ok(());
    };
    if !positions.contains_key(&endpoints.0) || !positions.contains_key(&endpoints.1) {
        return Ok(());
    }
    let scaled = positions
        .iter()
        .map(|(&a, &(x, y))| (a, (x * 4, y * 4)))
        .collect::<Positions>();
    #[allow(
        clippy::unwrap_used,
        reason = "next_face selects a neighbor already present in placed orders"
    )]
    let existing = orders
        .iter()
        .find(|(f, _)| *f == neighbor)
        .unwrap()
        .1
        .iter()
        .find(|&&a| a != endpoints.0 && a != endpoints.1)
        .map(|a| scaled[a]);
    let Some(existing) = existing else {
        return Ok(());
    };
    for ends in [endpoints, (endpoints.1, endpoints.0)] {
        for order in orders_starting_with_edge(&face.atom_cycle, ends) {
            for shape in shapes_for(face.size, system) {
                if state
                    .best_distortion
                    .is_some_and(|b| distortion + shape.distortion_rank as i64 > b)
                {
                    continue;
                }
                state.budget.spend()?;
                let candidate =
                    place_shape(&shape, &order, scaled[&ends.0], scaled[&ends.1], system);
                if !opposite_side(
                    scaled[&ends.0],
                    scaled[&ends.1],
                    existing,
                    candidate[&order[2]],
                ) {
                    continue;
                }
                if candidate
                    .iter()
                    .any(|(a, p)| scaled.get(a).is_some_and(|old| old != p))
                {
                    continue;
                }
                let mut merged = scaled.clone();
                merged.extend(candidate);
                let mut new_orders = orders.clone();
                new_orders.push((face.id, order.clone()));
                if !partial_layout_is_valid(model, &new_orders, &merged, Some(face.id)) {
                    continue;
                }
                let mut new_shapes = shapes.clone();
                new_shapes.insert(face.id, shape);
                search_layouts(model, &new_orders, &new_shapes, &merged, system, state)?
            }
        }
    }
    Ok(())
}
#[allow(
    clippy::unwrap_used,
    reason = "known face membership is checked before face lookup in the short-circuit adjacency gate"
)]
fn valid_face_adjacency(model: &FaceModel) -> bool {
    let known = model.faces.iter().map(|f| f.id).collect::<AtomSet>();
    if known.is_empty() {
        return false;
    }
    let mut seen_edges = AtomSet::new();
    let mut neighbors: BTreeMap<usize, Vec<usize>> = known.iter().map(|&a| (a, vec![])).collect();
    for &(a, b, e) in &model.face_adjacency {
        if a == b
            || !known.contains(&a)
            || !known.contains(&b)
            || !seen_edges.insert(e)
            || !model
                .faces
                .iter()
                .find(|f| f.id == a)
                .unwrap()
                .edge_cycle
                .contains(&e)
            || !model
                .faces
                .iter()
                .find(|f| f.id == b)
                .unwrap()
                .edge_cycle
                .contains(&e)
        {
            return false;
        }
        #[allow(
            clippy::unwrap_used,
            reason = "adjacency endpoints were proved known before filling the neighbor map"
        )]
        neighbors.get_mut(&a).unwrap().push(b);
        #[allow(
            clippy::unwrap_used,
            reason = "adjacency endpoints were proved known before filling the neighbor map"
        )]
        neighbors.get_mut(&b).unwrap().push(a)
    }
    let mut seen = AtomSet::new();
    #[allow(
        clippy::unwrap_used,
        reason = "the empty face set was rejected at function entry"
    )]
    let mut pending = vec![*known.first().unwrap()];
    while let Some(n) = pending.pop() {
        if seen.insert(n) {
            pending.extend(&neighbors[&n]);
        }
    }
    seen == known
}
fn next_face(model: &FaceModel, orders: &Orders) -> Option<(usize, usize, usize)> {
    let rank = orders
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (*f, i))
        .collect::<BTreeMap<_, _>>();
    let mut options = vec![];
    for &(a, b, e) in &model.face_adjacency {
        if rank.contains_key(&a) == rank.contains_key(&b) {
            continue;
        }
        let (unplaced, neighbor) = if rank.contains_key(&a) {
            (b, a)
        } else {
            (a, b)
        };
        let count = model
            .face_adjacency
            .iter()
            .filter(|&&(a, b, _)| {
                (a == unplaced && rank.contains_key(&b)) || (b == unplaced && rank.contains_key(&a))
            })
            .count();
        options.push((-(count as i64), rank[&neighbor], unplaced, neighbor, e))
    }
    options.into_iter().min().map(|(_, _, f, n, e)| (f, n, e))
}
fn edge_endpoints(face: &Face, edge: usize) -> Option<(usize, usize)> {
    let index = face.edge_cycle.iter().position(|&e| e == edge)?;
    Some((
        face.atom_cycle[index],
        face.atom_cycle[(index + 1) % face.size],
    ))
}
fn oriented_cycle(cycle: &[usize], offset: usize, reverse: bool) -> Vec<usize> {
    let seq = if reverse {
        cycle.iter().rev().copied().collect::<Vec<_>>()
    } else {
        cycle.to_vec()
    };
    seq[offset..]
        .iter()
        .chain(&seq[..offset])
        .copied()
        .collect()
}
fn orders_starting_with_edge(cycle: &[usize], ends: (usize, usize)) -> Vec<Vec<usize>> {
    let mut result = vec![];
    for reverse in [false, true] {
        for offset in 0..cycle.len() {
            let order = oriented_cycle(cycle, offset, reverse);
            if order[0] == ends.0 && order[1] == ends.1 {
                result.push(order)
            }
        }
    }
    result
}
fn place_shape(
    shape: &RingShapeSpec,
    order: &[usize],
    start: Point,
    end: Point,
    system: &str,
) -> Positions {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    order
        .iter()
        .copied()
        .zip(shape.vertices.iter().map(|&(x, y)| {
            if system == "eisenstein" {
                (
                    start.0 + (x * dx - y * dy).div_euclid(4),
                    start.1 + (x * dy + y * dx + y * dy).div_euclid(4),
                )
            } else {
                (
                    start.0 + (x * dx).div_euclid(4) - (y * dy).div_euclid(4),
                    start.1 + (x * dy).div_euclid(4) + (y * dx).div_euclid(4),
                )
            }
        }))
        .collect()
}
fn opposite_side(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (c, d) = (cross(a, b, c), cross(a, b, d));
    c != 0 && d != 0 && (c > 0) != (d > 0)
}
fn partial_layout_is_valid(
    model: &FaceModel,
    orders: &Orders,
    positions: &Positions,
    extending: Option<usize>,
) -> bool {
    if positions.values().copied().collect::<BTreeSet<_>>().len() != positions.len() {
        return false;
    }
    let mut edges = vec![];
    let mut seen = BTreeSet::new();
    let mut polygons = vec![];
    let ordered = orders
        .iter()
        .filter(|(id, _)| Some(*id) != extending)
        .chain(orders.iter().filter(|(id, _)| Some(*id) == extending));
    let mut settled_edges = 0;
    let mut settled_faces = 0;
    for (id, order) in ordered {
        if Some(*id) == extending {
            settled_edges = edges.len();
            settled_faces = polygons.len()
        }
        #[allow(
            clippy::unwrap_used,
            reason = "placed orders are constructed only from faces in this model"
        )]
        let face = model.faces.iter().find(|f| f.id == *id).unwrap();
        for i in 0..face.size {
            let (a, b) = (face.atom_cycle[i], face.atom_cycle[(i + 1) % face.size]);
            let (Some(&p), Some(&q)) = (positions.get(&a), positions.get(&b)) else {
                return false;
            };
            if seen.insert(pair(a, b)) {
                edges.push((a, b, p, q))
            }
        }
        polygons.push((*id, order.iter().map(|a| positions[a]).collect::<Vec<_>>()));
    }
    for i in 0..edges.len() {
        let (a, b, p, q) = edges[i];
        for &(c, d, r, s) in &edges[(i + 1).max(settled_edges)..] {
            if a == c || a == d || b == c || b == d {
                continue;
            }
            if segments_intersect(p, q, r, s) {
                return false;
            }
        }
    }
    let adjacency = adjacent(model);
    for i in 0..polygons.len() {
        let (a, ref p) = polygons[i];
        for (b, q) in &polygons[(i + 1).max(settled_faces)..] {
            if !adjacency.contains(&pair(a, *b))
                && (polygon_center_strictly_inside(p, q) || polygon_center_strictly_inside(q, p))
            {
                return false;
            }
        }
    }
    true
}
fn model_boundary_is_consistent(model: &FaceModel) -> bool {
    let mut mult = BTreeMap::new();
    let mut perimeter = BTreeSet::new();
    for f in &model.faces {
        for e in cycle_edges(&f.atom_cycle) {
            *mult.entry(e).or_insert(0usize) += 1
        }
        for &e in &f.edge_cycle {
            if model.perimeter_edges.contains(&e)
                && let Some((a, b)) = edge_endpoints(f, e)
            {
                perimeter.insert(pair(a, b));
            }
        }
    }
    let geometric = mult
        .iter()
        .filter(|(_, n)| **n == 1)
        .map(|(&e, _)| e)
        .collect::<BTreeSet<_>>();
    perimeter == geometric && geometric == cycle_edges(&model.outer_boundary)
}
fn audit_layout(
    model: &FaceModel,
    orders: &Orders,
    positions: &Positions,
    incremental: bool,
) -> bool {
    orders.iter().map(|(f, _)| *f).collect::<AtomSet>()
        == model.faces.iter().map(|f| f.id).collect()
        && (incremental || partial_layout_is_valid(model, orders, positions, None))
        && model_boundary_is_consistent(model)
}
pub(crate) fn cross(a: Point, b: Point, c: Point) -> i128 {
    (b.0 - a.0) as i128 * (c.1 - a.1) as i128 - (b.1 - a.1) as i128 * (c.0 - a.0) as i128
}
pub(crate) fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let (one, two, three, four) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    let on = |p: Point, q: Point, r: Point| {
        p.0.min(q.0) <= r.0 && r.0 <= p.0.max(q.0) && p.1.min(q.1) <= r.1 && r.1 <= p.1.max(q.1)
    };
    (one == 0 && on(a, b, c))
        || (two == 0 && on(a, b, d))
        || (three == 0 && on(c, d, a))
        || (four == 0 && on(c, d, b))
        || ((one > 0) != (two > 0) && (three > 0) != (four > 0))
}
fn polygon_center_strictly_inside(source: &[Point], polygon: &[Point]) -> bool {
    let n = source.len() as i128;
    let (x, y) = source.iter().fold((0i128, 0i128), |(x, y), &(a, b)| {
        (x + a as i128, y + b as i128)
    });
    let signs = (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            (b.0 - a.0) as i128 * (y - n * a.1 as i128)
                - (b.1 - a.1) as i128 * (x - n * a.0 as i128)
        })
        .collect::<Vec<_>>();
    signs.iter().all(|&v| v > 0) || signs.iter().all(|&v| v < 0)
}
fn ring_axis_center(order: &[usize], positions: &Positions) -> Point {
    let mut centers = BTreeSet::new();
    let edges = (0..order.len())
        .map(|i| {
            (
                positions[&order[i]],
                positions[&order[(i + 1) % order.len()]],
            )
        })
        .collect::<Vec<_>>();
    for (i, &(a, b)) in edges.iter().enumerate() {
        for &(c, d) in &edges[i + 1..] {
            if (b.0 - a.0, b.1 - a.1) == (c.0 - d.0, c.1 - d.1) {
                centers.insert((
                    (a.0 + b.0 + c.0 + d.0).div_euclid(4),
                    (a.1 + b.1 + c.1 + d.1).div_euclid(4),
                ));
            }
        }
    }
    if centers.len() == 1 {
        #[allow(
            clippy::unwrap_used,
            reason = "this branch requires exactly one center"
        )]
        *centers.first().unwrap()
    } else {
        let n = order.len() as i64;
        (
            order
                .iter()
                .map(|a| positions[a].0)
                .sum::<i64>()
                .div_euclid(n),
            order
                .iter()
                .map(|a| positions[a].1)
                .sum::<i64>()
                .div_euclid(n),
        )
    }
}
fn normalize_integer_layout(positions: &Positions, centers: &Positions) -> (Positions, Positions) {
    let x = positions.values().map(|p| p.0).min().unwrap_or(0);
    let y = positions.values().map(|p| p.1).min().unwrap_or(0);
    let shift = |values: &Positions| {
        values
            .iter()
            .map(|(&a, &(px, py))| (a, (px - x, py - y)))
            .collect::<Positions>()
    };
    let (mut positions, mut centers) = (shift(positions), shift(centers));
    let divisor = positions
        .values()
        .chain(centers.values())
        .fold(0, |g, p| gcd(gcd(g, p.0), p.1));
    if divisor > 1 {
        for p in positions.values_mut().chain(centers.values_mut()) {
            p.0 /= divisor;
            p.1 /= divisor
        }
    }
    (positions, centers)
}
fn normalized_embedding_geometry(
    orders: &Orders,
    positions: &Positions,
    system: &str,
) -> (Positions, Positions) {
    let scale = orders.iter().fold(4, |a, (_, o)| lcm(a, o.len() as i64));
    let integer = positions
        .iter()
        .map(|(&a, &(x, y))| {
            if system == "eisenstein" {
                (a, ((2 * x + y) * scale, 3 * y * scale))
            } else {
                (a, (x * scale, y * scale))
            }
        })
        .collect::<Positions>();
    let centers = orders
        .iter()
        .map(|(f, o)| (*f, ring_axis_center(o, &integer)))
        .collect();
    normalize_integer_layout(&integer, &centers)
}
fn intrinsic_embedding_key(
    orders: &Orders,
    shapes: &Shapes,
    positions: &Positions,
    system: &str,
) -> EmbeddingKey {
    let (integer, _) = normalized_embedding_geometry(orders, positions, system);
    let values = integer.values().copied().collect::<Vec<_>>();
    let metric = if system == "eisenstein" { 3 } else { 1 };
    let mut distances = vec![];
    for (i, &(x, y)) in values.iter().enumerate() {
        for &(a, b) in &values[i + 1..] {
            distances.push(
                metric * (x - a) as i128 * (x - a) as i128 + (y - b) as i128 * (y - b) as i128,
            )
        }
    }
    (
        shapes
            .iter()
            .map(|(&f, s)| (f, s.shape_id.clone()))
            .collect(),
        distances,
    )
}
fn layout_distortion(orders: &Orders, shapes: &Shapes, positions: &Positions, system: &str) -> i64 {
    let scales = orders
        .iter()
        .map(|(f, o)| {
            let (a, b) = (positions[&o[0]], positions[&o[1]]);
            let (s, t) = (shapes[f].vertices[0], shapes[f].vertices[1]);
            let (dx, dy, sx, sy) = (
                (b.0 - a.0) as i128,
                (b.1 - a.1) as i128,
                (t.0 - s.0) as i128,
                (t.1 - s.1) as i128,
            );
            let (mut p, mut q) = (dx * dx + dy * dy, sx * sx + sy * sy);
            if system == "eisenstein" {
                p += dx * dy;
                q += sx * sy
            }
            (p, q)
        })
        .collect::<Vec<_>>();
    let smallest = scales
        .iter()
        .min_by(|a, b| (a.0 * b.1).cmp(&(b.0 * a.1)))
        .copied()
        .unwrap_or((0, 1));
    shapes
        .values()
        .map(|s| s.distortion_rank as i64)
        .sum::<i64>()
        + scales
            .iter()
            .filter(|&&(p, q)| p * smallest.1 > smallest.0 * q)
            .count() as i64
}
fn direction_grid_centers(centers: &Positions, adjacent: &Adjacent) -> Option<Positions> {
    let mut neighbors: BTreeMap<_, Vec<_>> = centers.keys().map(|&f| (f, vec![])).collect();
    for &(a, b) in adjacent {
        neighbors.get_mut(&a)?.push(b);
        neighbors.get_mut(&b)?.push(a);
    }
    let root = *centers.keys().next()?;
    let mut result = BTreeMap::from([(root, (0, 0))]);
    let mut queue = VecDeque::from([root]);
    while let Some(f) = queue.pop_front() {
        for &other in &neighbors[&f] {
            let (sx, sy) = (
                (centers[&other].0 - centers[&f].0).signum(),
                (centers[&other].1 - centers[&f].1).signum(),
            );
            let step = if sy == 0 {
                (4 * sx, 0)
            } else if sx == 0 {
                (0, 2 * sy)
            } else {
                (2 * sx, sy)
            };
            let point = (result[&f].0 + step.0, result[&f].1 + step.1);
            if let Some(old) = result.get(&other) {
                if *old != point {
                    return None;
                }
            } else {
                result.insert(other, point);
                queue.push_back(other)
            }
        }
    }
    if result.len() == centers.len()
        && result.values().collect::<BTreeSet<_>>().len() == result.len()
    {
        Some(result)
    } else {
        None
    }
}
pub(crate) fn center_row_orientation_score(centers: &Positions, row: &[usize]) -> [i64; 3] {
    let middle = row.len() / 2;
    let axis_x = if row.len() % 2 == 1 {
        2 * centers[&row[middle]].0
    } else {
        centers[&row[middle - 1]].0 + centers[&row[middle]].0
    };
    let axis_y = centers[&row[0]].1;
    let (mut ur, mut ll, mut above) = (0, 0, 0);
    for &(x, y) in centers.values() {
        let right = if 2 * x == axis_x {
            1
        } else if 2 * x > axis_x {
            2
        } else {
            0
        };
        let upper = if y == axis_y {
            1
        } else if y > axis_y {
            2
        } else {
            0
        };
        ur += right * upper;
        ll += (2 - right) * (2 - upper);
        above += 2 * upper
    }
    [-ur, ll, -above]
}
pub(crate) fn opsin_occupied_row_orientation(centers: &Positions) -> Vec<i64> {
    let mut groups: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (&f, &(x, y)) in centers {
        groups.entry((y, x.rem_euclid(4))).or_default().push(f)
    }
    let mut rows = vec![];
    for group in groups.values_mut() {
        group.sort_by_key(|f| centers[f].0);
        let mut row: Vec<usize> = vec![];
        for &f in group.iter() {
            if row
                .last()
                .is_some_and(|prev| centers[&f].0 - centers[prev].0 != 4)
            {
                rows.push(std::mem::take(&mut row));
            }
            row.push(f)
        }
        rows.push(row)
    }
    let longest = rows.iter().map(Vec::len).max().unwrap_or(0);
    rows.iter()
        .filter(|r| r.len() == longest)
        .map(|r| center_row_orientation_score(centers, r).to_vec())
        .min()
        .unwrap_or_default()
}
fn orientation_score(
    centers: &Positions,
    shapes: &Shapes,
    adjacent: &Adjacent,
    distortion: i64,
    orders: &Orders,
    positions: &Positions,
    opsin: bool,
) -> Vec<i64> {
    let directed = direction_grid_centers(centers, adjacent);
    let centers = directed.as_ref().unwrap_or(centers);
    let mut faces = centers.keys().copied().collect::<Vec<_>>();
    faces.sort_by_key(|f| (centers[f].1, centers[f].0));
    let mut rows: Vec<Vec<usize>> = vec![];
    for f in faces {
        if rows.last().and_then(|r| r.last()).is_none_or(|prev| {
            centers[prev].1 != centers[&f].1 || !adjacent.contains(&pair(*prev, f))
        }) {
            rows.push(vec![])
        }
        let last_row = rows.len() - 1;
        rows[last_row].push(f)
    }
    let count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let om = order_map(orders);
    let mut orientation = rows
        .iter()
        .filter(|r| r.len() == count)
        .map(|row| {
            if directed.is_some() {
                return center_row_orientation_score(centers, row).to_vec();
            }
            let m = row.len() / 2;
            let ax = if row.len() % 2 == 1 {
                2 * centers[&row[m]].0
            } else {
                om[&row[m - 1]]
                    .iter()
                    .filter(|a| om[&row[m]].contains(a))
                    .map(|a| positions[a].0)
                    .sum()
            };
            let ay = centers[&row[0]].1;
            let (mut ur, mut ll, mut above) = (0, 0, 0);
            for (_, o) in orders {
                #[allow(
                    clippy::unwrap_used,
                    reason = "face orders are simple cycles with at least three positioned atoms"
                )]
                let x0 = o.iter().map(|a| 2 * positions[a].0).min().unwrap();
                #[allow(
                    clippy::unwrap_used,
                    reason = "face orders are simple cycles with at least three positioned atoms"
                )]
                let x1 = o.iter().map(|a| 2 * positions[a].0).max().unwrap();
                #[allow(
                    clippy::unwrap_used,
                    reason = "face orders are simple cycles with at least three positioned atoms"
                )]
                let y0 = o.iter().map(|a| positions[a].1).min().unwrap();
                #[allow(
                    clippy::unwrap_used,
                    reason = "face orders are simple cycles with at least three positioned atoms"
                )]
                let y1 = o.iter().map(|a| positions[a].1).max().unwrap();
                let half = |low, high, axis| {
                    if low < axis && axis < high {
                        1
                    } else if low >= axis {
                        2
                    } else {
                        0
                    }
                };
                let r = half(x0, x1, ax);
                let u = half(y0, y1, ay);
                ur += r * u;
                ll += (2 - r) * (2 - u);
                above += 2 * u
            }
            vec![-ur, ll, -above]
        })
        .min()
        .unwrap_or_default();
    if opsin {
        orientation = opsin_occupied_row_orientation(centers)
    }
    let _ = shapes;
    let mut result = vec![distortion, -(count as i64)];
    result.extend(orientation);
    result
}
fn opsin_pentagon_chain(orders: &Orders, adjacent: &Adjacent) -> Option<Vec<usize>> {
    if adjacent.len() + 1 != orders.len() {
        return None;
    }
    let om = order_map(orders);
    let mut neighbors: BTreeMap<usize, Vec<usize>> =
        orders.iter().map(|(f, _)| (*f, vec![])).collect();
    for &(a, b) in adjacent {
        neighbors.get_mut(&a)?.push(b);
        neighbors.get_mut(&b)?.push(a)
    }
    let terminals = orders
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| neighbors[f].len() == 1)
        .collect::<Vec<_>>();
    if terminals.len() != 2 || neighbors.values().any(|v| v.len() != 1 && v.len() != 2) {
        return None;
    }
    let mut path = vec![terminals[0]];
    while path.last() != Some(&terminals[1]) {
        let choices = neighbors[&path[path.len() - 1]]
            .iter()
            .filter(|f| !path.contains(f))
            .copied()
            .collect::<Vec<_>>();
        if choices.len() != 1 {
            return None;
        }
        path.push(choices[0])
    }
    if path.len() != orders.len() || path.len() < 4 || terminals.iter().any(|f| om[f].len() != 6) {
        return None;
    }
    for f in &path[1..path.len() - 1] {
        let order = &om[f];
        if order.len() != 5 {
            return None;
        }
        let edges = neighbors[f]
            .iter()
            .flat_map(|n| cycle_edges(&om[n]))
            .collect::<BTreeSet<_>>();
        let ports = (0..5)
            .filter(|&i| edges.contains(&pair(order[i], order[(i + 1) % 5])))
            .collect::<Vec<_>>();
        if ports.len() != 2 || ![2, 3].contains(&((ports[0] + 5 - ports[1]) % 5)) {
            return None;
        }
    }
    Some(path)
}
fn opsin_ring_map_tree(model: &FaceModel) -> bool {
    opsin_pentagon_chain(
        &model
            .faces
            .iter()
            .map(|f| (f.id, f.atom_cycle.clone()))
            .collect(),
        &adjacent(model),
    )
    .is_some()
        || (model.face_adjacency.len() + 1 == model.faces.len()
            && model.faces.iter().any(|f| f.size == 3)
            && model.faces.iter().any(|f| f.size == 6)
            && model.faces.iter().all(|f| {
                f.size == 6
                    || ([3, 5].contains(&f.size)
                        && f.edge_cycle
                            .iter()
                            .filter(|e| model.fusion_edges.contains(e))
                            .count()
                            == 1)
            }))
}
fn two_port_pentagon_axes(orders: &Orders, centers: &Positions, adjacent: &Adjacent) -> Positions {
    if adjacent.len() + 1 != orders.len() {
        return centers.clone();
    }
    let mut neighbors: BTreeMap<usize, Vec<usize>> =
        orders.iter().map(|(f, _)| (*f, vec![])).collect();
    for &(a, b) in adjacent {
        #[allow(
            clippy::unwrap_used,
            reason = "adjacency endpoints were proved known before filling the neighbor map"
        )]
        neighbors.get_mut(&a).unwrap().push(b);
        #[allow(
            clippy::unwrap_used,
            reason = "adjacency endpoints were proved known before filling the neighbor map"
        )]
        neighbors.get_mut(&b).unwrap().push(a)
    }
    let edges = orders
        .iter()
        .map(|(f, o)| (*f, cycle_edges(o)))
        .collect::<BTreeMap<_, _>>();
    let mut targets = BTreeMap::new();
    for (f, o) in orders {
        if o.len() != 5 || neighbors[f].len() != 2 {
            continue;
        }
        let ports = (0..5)
            .filter(|&i| {
                neighbors[f]
                    .iter()
                    .any(|n| edges[n].contains(&pair(o[i], o[(i + 1) % 5])))
            })
            .collect::<Vec<_>>();
        if ports.len() == 2 && [2, 3].contains(&((ports[0] + 5 - ports[1]) % 5)) {
            targets.insert(*f, (neighbors[f][0], neighbors[f][1]));
        }
    }
    let mut result = centers.clone();
    for (&f, &(a, b)) in &targets {
        if targets.contains_key(&a) || targets.contains_key(&b) {
            continue;
        }
        let (x, y) = (centers[&a].0 + centers[&b].0, centers[&a].1 + centers[&b].1);
        if x % 2 == 0 && y % 2 == 0 {
            result.insert(f, (x / 2, y / 2));
        }
    }
    if result.values().collect::<BTreeSet<_>>().len() == result.len() {
        result
    } else {
        centers.clone()
    }
}
fn materialize_layouts(
    orders: &Orders,
    shapes: &Shapes,
    positions: &Positions,
    system: &str,
    opsin: bool,
) -> Vec<FusedLayout> {
    let distortion = layout_distortion(orders, shapes, positions, system);
    let (integer, centers) = normalized_embedding_geometry(orders, positions, system);
    let mut owners: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (f, o) in orders {
        for edge in cycle_edges(o) {
            owners.entry(edge).or_default().push(*f)
        }
    }
    let adjacent = owners
        .values()
        .filter(|v| v.len() == 2)
        .map(|v| pair(v[0], v[1]))
        .collect::<Adjacent>();
    let integer = integer
        .iter()
        .map(|(&a, &(x, y))| (a, (2 * x, 2 * y)))
        .collect::<Positions>();
    let centers = centers
        .iter()
        .map(|(&a, &(x, y))| (a, (2 * x, 2 * y)))
        .collect();
    let mut centers = two_port_pentagon_axes(orders, &centers, &adjacent);
    let chain = if opsin {
        opsin_pentagon_chain(orders, &adjacent)
    } else {
        None
    };
    if let Some(path) = &chain {
        let (a, b) = (centers[&path[0]], centers[&path[path.len() - 1]]);
        let n = (path.len() - 1) as i64;
        centers = path
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                let i = i as i64;
                (f, ((n - i) * a.0 + i * b.0, (n - i) * a.1 + i * b.1))
            })
            .collect();
    }
    let mut directions = BTreeSet::from([(1, 0)]);
    for &(a, b) in &adjacent {
        let (mut dx, mut dy) = (centers[&b].0 - centers[&a].0, centers[&b].1 - centers[&a].1);
        let d = gcd(dx, dy);
        if d == 0 {
            continue;
        }
        dx /= d;
        dy /= d;
        if dx < 0 || (dx == 0 && dy < 0) {
            dx = -dx;
            dy = -dy
        }
        directions.insert((dx, dy));
    }
    let metric = if system == "eisenstein" { 3 } else { 1 };
    let mut best = None;
    let mut candidates = Vec::new();
    let mut candidate_keys = BTreeSet::new();
    for (dx, dy) in directions {
        for sx in [-1, 1] {
            for sy in [-1, 1] {
                let orient = |p: &Positions| {
                    p.iter()
                        .map(|(&a, &(x, y))| {
                            (
                                a,
                                (sx * (metric * x * dx + y * dy), sy * (-x * dy + y * dx)),
                            )
                        })
                        .collect::<Positions>()
                };
                let (mut oc, op) = (orient(&centers), orient(&integer));
                if opsin && direction_grid_centers(&oc, &adjacent).is_none() {
                    continue;
                }
                let score =
                    orientation_score(&oc, shapes, &adjacent, distortion, orders, &op, opsin);
                if best.as_ref().is_some_and(|b| &score > b) {
                    continue;
                }
                if best.as_ref().is_none_or(|b| &score < b) {
                    best = Some(score.clone());
                    candidates.clear();
                    candidate_keys.clear();
                }
                oc = direction_grid_centers(&oc, &adjacent).unwrap_or(oc);
                let (op, oc) = normalize_integer_layout(&op, &oc);
                let mut evidence = vec![
                    "all face boundaries represented",
                    "shared edge coordinates agree",
                    "unrelated edges do not cross",
                    "nonadjacent face interiors do not overlap",
                    "geometric and topological perimeters agree",
                    "preferred axis derived from ring-center rows",
                ]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>();
                if opsin {
                    evidence.push(OPSIN_RING_MAP_NUMBERING.into())
                }
                if chain.is_some() {
                    evidence.push(OPSIN_COUPLED_PENTAGON_AXES.into())
                }
                let layout = FusedLayout {
                    face_positions: oc.iter().map(|(&f, &(x, y))| (f, x, y)).collect(),
                    atom_positions: op.iter().map(|(&a, &(x, y))| (a, x, y)).collect(),
                    face_shapes: shapes
                        .iter()
                        .map(|(&f, s)| (f, s.shape_id.clone()))
                        .collect(),
                    orientation_score: score,
                    audit_evidence: evidence,
                    ..Default::default()
                };
                if candidate_keys
                    .insert((layout.atom_positions.clone(), layout.face_shapes.clone()))
                {
                    candidates.push(layout);
                }
            }
        }
    }
    let mut result = candidates;
    sort_layouts(&mut result);
    result
}
fn sort_layouts(layouts: &mut [FusedLayout]) {
    layouts.sort_by_key(|l| {
        let mut shapes = l
            .face_shapes
            .iter()
            .map(|(_, s)| s.clone())
            .collect::<Vec<_>>();
        shapes.sort();
        let mut geometry = l
            .atom_positions
            .iter()
            .map(|&(_, x, y)| (x, y))
            .collect::<Vec<_>>();
        geometry.sort();
        (l.orientation_score.clone(), shapes, geometry)
    });
}
fn subdivided_face_model(model: &FaceModel, faces: Vec<Face>, removed: &AtomSet) -> FaceModel {
    let mut owners: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for f in &faces {
        for &e in &f.edge_cycle {
            owners.entry(e).or_default().push(f.id)
        }
    }
    FaceModel {
        faces,
        edge_to_faces: owners.iter().map(|(&e, v)| (e, v.clone())).collect(),
        perimeter_edges: owners
            .iter()
            .filter(|(_, v)| v.len() == 1)
            .map(|(&e, _)| e)
            .collect(),
        outer_boundary: model
            .outer_boundary
            .iter()
            .filter(|a| !removed.contains(a))
            .copied()
            .collect(),
        ..model.clone()
    }
}
fn expand_proxy_layouts(
    model: &FaceModel,
    layouts: Vec<FusedLayout>,
    paths: &[Vec<usize>],
    prefixes: &BTreeMap<usize, String>,
    evidence: &[String],
) -> Vec<FusedLayout> {
    let scale = paths.iter().fold(1, |n, p| lcm(n, (p.len() - 1) as i64));
    let orders = model
        .faces
        .iter()
        .map(|f| (f.id, f.atom_cycle.clone()))
        .collect();
    let mut result = vec![];
    for mut layout in layouts {
        let mut positions = layout
            .atom_positions
            .iter()
            .map(|&(a, x, y)| (a, (x * scale, y * scale)))
            .collect::<Positions>();
        let centers = layout
            .face_positions
            .iter()
            .map(|&(f, x, y)| (f, (x * scale, y * scale)))
            .collect();
        for path in paths {
            let (start, end) = (positions[&path[0]], positions[&path[path.len() - 1]]);
            let n = (path.len() - 1) as i64;
            for (i, &a) in path.iter().enumerate().skip(1).take(path.len() - 2) {
                positions.insert(
                    a,
                    (
                        start.0 + (i as i64 * (end.0 - start.0)).div_euclid(n),
                        start.1 + (i as i64 * (end.1 - start.1)).div_euclid(n),
                    ),
                );
            }
        }
        if !audit_layout(model, &orders, &positions, false) {
            continue;
        }
        let (positions, centers) = normalize_integer_layout(&positions, &centers);
        layout.atom_positions = positions.iter().map(|(&a, &(x, y))| (a, x, y)).collect();
        layout.face_positions = centers.iter().map(|(&a, &(x, y))| (a, x, y)).collect();
        for (f, shape) in &mut layout.face_shapes {
            if let Some(prefix) = prefixes.get(f) {
                *shape = format!("{prefix}{shape}")
            }
        }
        layout.audit_evidence.extend_from_slice(evidence);
        layout.audit_evidence.push(
            "only nonfusion paths subdivided; proxy polygon and ring-axis centers preserved".into(),
        );
        layout
            .audit_evidence
            .push("expanded original face model passes the complete geometry audit".into());
        result.push(layout)
    }
    sort_layouts(&mut result);
    result
}
fn terminal_ring_proxy_layouts(
    model: &FaceModel,
    budget: usize,
    max: usize,
) -> Result<Option<Vec<FusedLayout>>, String> {
    if model.faces.len() < 3 || model.face_adjacency.len() + 1 != model.faces.len() {
        return Ok(None);
    }
    let targets = model
        .faces
        .iter()
        .filter(|f| {
            [7, 8].contains(&f.size)
                && f.edge_cycle
                    .iter()
                    .filter(|e| model.fusion_edges.contains(e))
                    .count()
                    == 1
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(None);
    }
    if !valid_face_adjacency(model) {
        return Ok(Some(vec![]));
    }
    let mut faces = model.faces.clone();
    let mut next_edge = model
        .faces
        .iter()
        .flat_map(|f| &f.edge_cycle)
        .max()
        .copied()
        .unwrap_or(0)
        + 1;
    let mut paths = vec![];
    let mut removed = AtomSet::new();
    for face in &targets {
        #[allow(
            clippy::unwrap_used,
            reason = "targets require exactly one fusion edge in this face"
        )]
        let edge = *face
            .edge_cycle
            .iter()
            .find(|e| model.fusion_edges.contains(e))
            .unwrap();
        #[allow(
            clippy::unwrap_used,
            reason = "the edge was selected from this face edge cycle"
        )]
        let endpoints = edge_endpoints(face, edge).unwrap();
        let order = orders_starting_with_edge(&face.atom_cycle, endpoints).remove(0);
        let original = (0..face.size)
            .map(|i| {
                (
                    pair(face.atom_cycle[i], face.atom_cycle[(i + 1) % face.size]),
                    face.edge_cycle[i],
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut indices = vec![0, 1];
        indices.extend((1..5).map(|i| 1 + i * (face.size - 1) / 5));
        let cycle = indices.iter().map(|&i| order[i]).collect();
        let mut edges = vec![];
        let mut face_removed = AtomSet::new();
        for i in 0..indices.len() {
            let start = indices[i];
            let end = indices.get(i + 1).copied().unwrap_or(face.size);
            let path = (start..=end)
                .map(|i| order[i % face.size])
                .collect::<Vec<_>>();
            face_removed.extend(path[1..path.len() - 1].iter().copied());
            edges.push(if end == start + 1 {
                original[&pair(path[0], path[1])]
            } else {
                let e = next_edge;
                next_edge += 1;
                e
            });
            paths.push(path)
        }
        if model
            .faces
            .iter()
            .filter(|f| f.id != face.id)
            .any(|f| f.atom_cycle.iter().any(|a| face_removed.contains(a)))
        {
            return Ok(None);
        }
        removed.extend(face_removed);
        #[allow(
            clippy::unwrap_used,
            reason = "the proxy face vector is cloned from the original model"
        )]
        let proxy_face = faces.iter_mut().find(|f| f.id == face.id).unwrap();
        *proxy_face = Face {
            id: face.id,
            atom_cycle: cycle,
            edge_cycle: edges,
            size: 6,
        };
    }
    let proxy = subdivided_face_model(model, faces, &removed);
    let layouts = intrinsic_fused_layouts_with_options(&proxy, budget, max, false)?;
    let prefixes = targets
        .iter()
        .map(|f| (f.id, format!("terminal-{}:", f.size)))
        .collect();
    Ok(Some(expand_proxy_layouts(
        model,
        layouts,
        &paths,
        &prefixes,
        &["terminal nonfusion arcs subdivided without changing fusion axes".into()],
    )))
}
fn two_port_large_ring_layouts(
    model: &FaceModel,
    budget: usize,
    max: usize,
    face_id: Option<usize>,
) -> Result<Vec<FusedLayout>, String> {
    let c = fusion_nomenclature_config();
    let large = model
        .faces
        .iter()
        .filter(|f| {
            face_id.map_or_else(
                || {
                    !c.ring_shapes
                        .iter()
                        .any(|s| s.ring_size == f.size && s.directed_entry_port.is_none())
                },
                |id| f.id == id,
            )
        })
        .collect::<Vec<_>>();
    if large.len() != 1 {
        return Ok(vec![]);
    }
    let face = large[0];
    if !c.annulene_ring_sizes.contains(&face.size)
        || (face.size % 2 == 1 && !(face_id.is_some() && face.size == 7))
        || model.face_adjacency.len() + 1 != model.faces.len()
        || !valid_face_adjacency(model)
        || model.outer_boundary.iter().copied().collect::<AtomSet>() != all_atoms(model)
    {
        return Ok(vec![]);
    }
    let ports = face
        .edge_cycle
        .iter()
        .filter(|e| model.fusion_edges.contains(e))
        .copied()
        .collect::<BTreeSet<_>>();
    if ports.len() != 2 {
        return Ok(vec![]);
    }
    #[allow(
        clippy::unwrap_used,
        reason = "the preceding gate requires exactly two ports"
    )]
    let first = *ports.first().unwrap();
    #[allow(
        clippy::unwrap_used,
        reason = "ports are selected from this face edge cycle"
    )]
    let entrance = edge_endpoints(face, first).unwrap();
    #[allow(
        clippy::unwrap_used,
        reason = "exactly two distinct ports were selected from this face edge cycle"
    )]
    let exit = edge_endpoints(face, *ports.iter().find(|&&e| e != first).unwrap()).unwrap();
    let mut distances = vec![face.size / 2 - 1];
    if face_id.is_some() && [7, 8].contains(&face.size) {
        distances.push(face.size / 2)
    }
    let mut chosen = None;
    for distance in distances {
        for endpoints in [entrance, (entrance.1, entrance.0)] {
            let order = orders_starting_with_edge(&face.atom_cycle, endpoints).remove(0);
            if pair(order[distance], order[distance + 1]) == pair(exit.0, exit.1) {
                chosen = Some((distance, order));
                break;
            }
        }
        if chosen.is_some() {
            break;
        }
    }
    let Some((distance, order)) = chosen else {
        return Ok(vec![]);
    };
    let short = order[1..=distance].to_vec();
    let long = order[distance + 1..]
        .iter()
        .chain(&order[..1])
        .copied()
        .collect::<Vec<_>>();
    let straight = distance == face.size / 2;
    let (paths, proxy_order) = if straight {
        let sm = (short.len() - 1) / 2;
        let lm = (long.len() - 1) / 2;
        (
            vec![
                short[..=sm].to_vec(),
                short[sm..].to_vec(),
                long[..=lm].to_vec(),
                long[lm..].to_vec(),
            ],
            vec![
                order[0],
                order[1],
                short[sm],
                order[distance],
                long[0],
                long[lm],
            ],
        )
    } else {
        let steps = long.len() - 1;
        let shoulder = (steps + 1) / 3;
        let middle = steps - 2 * shoulder;
        let paths = vec![
            short,
            long[..=shoulder].to_vec(),
            long[shoulder..=shoulder + middle].to_vec(),
            long[shoulder + middle..].to_vec(),
        ];
        let po = vec![
            order[0],
            order[1],
            order[distance],
            paths[1][0],
            paths[2][0],
            paths[3][0],
        ];
        (paths, po)
    };
    let removed = paths
        .iter()
        .flat_map(|p| p[1..p.len() - 1].iter().copied())
        .collect::<AtomSet>();
    if model
        .faces
        .iter()
        .filter(|f| f.id != face.id)
        .any(|f| f.atom_cycle.iter().any(|a| removed.contains(a)))
    {
        return Ok(vec![]);
    }
    let original = (0..face.size)
        .map(|i| {
            (
                pair(face.atom_cycle[i], face.atom_cycle[(i + 1) % face.size]),
                face.edge_cycle[i],
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut next_edge = model
        .faces
        .iter()
        .flat_map(|f| &f.edge_cycle)
        .max()
        .copied()
        .unwrap_or(0)
        + 1;
    let proxy_edges = (0..proxy_order.len())
        .map(|i| {
            let e = pair(proxy_order[i], proxy_order[(i + 1) % proxy_order.len()]);
            original.get(&e).copied().unwrap_or_else(|| {
                let e = next_edge;
                next_edge += 1;
                e
            })
        })
        .collect();
    let mut faces = model.faces.clone();
    #[allow(
        clippy::unwrap_used,
        reason = "the proxy face vector is cloned from the original model"
    )]
    let proxy_face = faces.iter_mut().find(|f| f.id == face.id).unwrap();
    *proxy_face = Face {
        id: face.id,
        atom_cycle: proxy_order,
        edge_cycle: proxy_edges,
        size: 6,
    };
    let proxy = subdivided_face_model(model, faces, &removed);
    let layouts = intrinsic_fused_layouts_with_options(&proxy, budget, max, false)?;
    let evidence = vec![format!(
        "two-port fusion distances have a {} hexagon direction witness",
        if straight { "straight" } else { "angular" }
    )];
    Ok(expand_proxy_layouts(
        model,
        layouts,
        &paths,
        &BTreeMap::from([(face.id, format!("two-port-{}:", face.size))]),
        &evidence,
    ))
}
fn ordinary_large_bicycle_layouts(
    model: &FaceModel,
    budget: usize,
    max: usize,
) -> Result<Vec<FusedLayout>, String> {
    let c = fusion_nomenclature_config();
    if model.faces.len() != 2
        || model.fusion_edges.len() != 1
        || model.face_adjacency.len() != 1
        || !c
            .annulene_ring_sizes
            .contains(&model.faces.iter().map(|f| f.size).max().unwrap_or(0))
        || model.faces.iter().map(|f| f.size).min().unwrap_or(0) < c.rules.minimum_ring_size
        || !valid_face_adjacency(model)
    {
        return Ok(vec![]);
    }
    #[allow(
        clippy::unwrap_used,
        reason = "the preceding gate requires exactly one fusion edge"
    )]
    let edge = *model.fusion_edges.first().unwrap();
    let left = &model.faces[0];
    let right = &model.faces[1];
    let Some((start, end)) = edge_endpoints(left, edge) else {
        return Ok(vec![]);
    };
    if edge_endpoints(right, edge).map(|(a, b)| pair(a, b)) != Some(pair(start, end))
        || left
            .atom_cycle
            .iter()
            .filter(|a| right.atom_cycle.contains(a))
            .copied()
            .collect::<AtomSet>()
            != AtomSet::from([start, end])
    {
        return Ok(vec![]);
    }
    let scale = model
        .faces
        .iter()
        .fold(1, |n, f| lcm(n, ((f.size - 1).pow(2) * f.size) as i64));
    let mut positions = BTreeMap::from([(start, (0, -scale)), (end, (0, scale))]);
    let mut orders = vec![];
    for (sign, face) in [-1, 1].into_iter().zip(&model.faces) {
        #[allow(
            clippy::unwrap_used,
            reason = "both faces were proved to contain the common fusion edge"
        )]
        let offset = face.atom_cycle.iter().position(|&a| a == start).unwrap();
        let mut order = oriented_cycle(&face.atom_cycle, offset, false);
        if order[1] == end {
            order[1..].reverse()
        }
        if order.last() != Some(&end) {
            return Ok(vec![]);
        }
        let steps = (face.size - 1) as i64;
        for (i, &a) in order.iter().enumerate().skip(1).take(order.len() - 2) {
            let i = i as i64;
            positions.insert(
                a,
                (
                    (sign * 4 * i * (steps - i) * scale).div_euclid(steps * steps),
                    ((2 * i - steps) * scale).div_euclid(steps),
                ),
            );
        }
        orders.push((face.id, order));
    }
    if !audit_layout(model, &orders, &positions, false) {
        return Ok(vec![]);
    }
    let centers = model
        .faces
        .iter()
        .map(|f| {
            (
                f.id,
                (
                    f.atom_cycle
                        .iter()
                        .map(|a| positions[a].0)
                        .sum::<i64>()
                        .div_euclid(f.size as i64),
                    0,
                ),
            )
        })
        .collect::<Positions>();
    let adjacent = BTreeSet::from([pair(left.id, right.id)]);
    let mut budget = Budget::new(budget);
    let mut layouts = vec![];
    for sx in [-1, 1] {
        for sy in [-1, 1] {
            budget.spend()?;
            let orient = |p: &Positions| {
                p.iter()
                    .map(|(&a, &(x, y))| (a, (sx * x, sy * y)))
                    .collect()
            };
            let (op, oc) = normalize_integer_layout(&orient(&positions), &orient(&centers));
            layouts.push(FusedLayout {
                face_positions: oc.iter().map(|(&f, &(x, y))| (f, x, y)).collect(),
                atom_positions: op.iter().map(|(&a, &(x, y))| (a, x, y)).collect(),
                face_shapes: model
                    .faces
                    .iter()
                    .map(|f| (f.id, format!("ordinary-bicycle-{}", f.size)))
                    .collect(),
                orientation_score: orientation_score(
                    &oc,
                    &BTreeMap::new(),
                    &adjacent,
                    1,
                    &orders,
                    &op,
                    false,
                ),
                audit_evidence: vec![
                    "two convex faces share only their vertical common edge",
                    "shared edge coordinates agree",
                    "unrelated edges do not cross",
                    "geometric and topological perimeters agree",
                    "four reflected horizontal-row orientation witnesses",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                ..Default::default()
            });
            if layouts.len() > max {
                return Err(format!(
                    "intrinsic layout search exceeded its budget of {max} layouts"
                ));
            }
        }
    }
    sort_layouts(&mut layouts);
    Ok(layouts)
}
pub fn opsin_construction_atom_order(
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> Option<Vec<usize>> {
    let fallback;
    let plan = if let Some(p) = ast.citation_plan.as_ref() {
        p
    } else {
        fallback = FusionCitationPlan::from_tree(ast.citation_tree.clone()?, &ast.joins);
        &fallback
    };
    if plan.roots.len() != 1
        || !plan.interparent_occurrences.is_empty()
        || !plan.cycle_closing_join_indices.is_empty()
    {
        return None;
    }
    let matches = ast
        .component_occurrences
        .iter()
        .map(|m| (m.occurrence_id, m))
        .collect::<BTreeMap<_, _>>();
    let groups = ast
        .multiplicative_groups
        .iter()
        .flat_map(|g| {
            g.occurrence_ids
                .iter()
                .map(|&o| (o, g.occurrence_ids.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut blocks = vec![];
    let mut emitted = AtomSet::new();
    for &occurrence in &plan.render_order {
        if !emitted.contains(&occurrence) {
            let block = groups
                .get(&occurrence)
                .cloned()
                .unwrap_or_else(|| vec![occurrence]);
            emitted.extend(block.iter().copied());
            blocks.push(block)
        }
    }
    let order = plan
        .parent_occurrences()
        .into_iter()
        .chain(blocks.iter().rev().flat_map(|b| b.iter().copied()))
        .collect::<Vec<_>>();
    if order.len() != matches.len()
        || order.iter().copied().collect::<AtomSet>() != matches.keys().copied().collect()
    {
        return None;
    }
    let mut result = vec![];
    let mut seen = AtomSet::new();
    let mut incorporated = AtomSet::new();
    for occurrence in order {
        let spec = specs.get(&occurrence)?;
        let local = matches.get(&occurrence)?.input_atom_by_locant();
        if local.keys().cloned().collect::<BTreeSet<_>>()
            != spec.template.locants.iter().cloned().collect()
        {
            return None;
        }
        let joins = ast
            .joins
            .iter()
            .filter(|j| j.attached_occurrence() == occurrence)
            .collect::<Vec<_>>();
        if joins
            .iter()
            .any(|j| !incorporated.contains(&j.host_occurrence()))
        {
            return None;
        }
        let overlap = local
            .values()
            .filter(|a| seen.contains(a))
            .copied()
            .collect::<AtomSet>();
        let shared = joins
            .iter()
            .flat_map(|j| j.shared_input_atoms())
            .collect::<AtomSet>();
        if overlap != shared {
            return None;
        }
        for join in joins {
            let i = &join.interface;
            let host = matches.get(&join.host_occurrence())?.input_atom_by_locant();
            if i.attached_path
                .iter()
                .map(|l| local.get(&l.text).copied())
                .collect::<Option<Vec<_>>>()?
                != i.ordered_input_atoms
                || i.host_path
                    .iter()
                    .map(|l| host.get(&l.text).copied())
                    .collect::<Option<Vec<_>>>()?
                    != i.ordered_input_atoms
            {
                return None;
            }
        }
        let mut locants = local.keys().collect::<Vec<_>>();
        locants.sort_by_key(|s| retained_locant_sort_key(s));
        for locant in locants {
            let atom = local[locant];
            if seen.insert(atom) {
                result.push(atom)
            }
        }
        incorporated.insert(occurrence);
    }
    Some(result)
}
/// Whether the face topology lies within the independently certified parser-numbering tier.
pub fn parser_numbering_is_supported(model: &FaceModel) -> bool {
    // Upstream divergence: an audited plane embedding proves molecular topology,
    // but does not prove OPSIN's construction-dependent perimeter for coupled
    // large rings or a large ring with three fusion ports. Terminal and two-port
    // proxies independently certify one large-ring axis; composing those proxies
    // or selecting a branched entry needs an additional graph-bound parser witness.
    let large_faces = model
        .faces
        .iter()
        .filter(|face| face.size >= 7)
        .collect::<Vec<_>>();
    let coupled_large_rings = model.faces.len() >= 3 && large_faces.len() >= 2;
    let branched_large_ring = large_faces.iter().any(|face| {
        face.edge_cycle
            .iter()
            .filter(|edge| model.fusion_edges.contains(edge))
            .count()
            >= 3
    });
    !coupled_large_rings && !branched_large_ring
}

pub fn can_use_component_entry_layout(model: &FaceModel) -> bool {
    model.faces.len() == 4
        && model.face_adjacency.len() == 3
        && fusion_nomenclature_config()
            .ring_shapes
            .iter()
            .filter(|s| s.directed_entry_port.is_some())
            .any(|s| {
                model.faces.iter().any(|f| {
                    f.size == s.ring_size
                        && model
                            .face_adjacency
                            .iter()
                            .all(|&(a, b, _)| a == f.id || b == f.id)
                })
            })
}
fn symmetric_terminal_shape(size: usize) -> Option<RingShapeSpec> {
    if size % 2 == 1 {
        return None;
    }
    let shapes = shapes_for(size, "cartesian")
        .into_iter()
        .filter(|s| {
            s.distortion_rank == 0
                && (0..size / 2)
                    .map(|i| {
                        (
                            s.vertices[i].0 + s.vertices[i + size / 2].0,
                            s.vertices[i].1 + s.vertices[i + size / 2].1,
                        )
                    })
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 1
        })
        .collect::<Vec<_>>();
    if shapes.len() == 1 {
        shapes.into_iter().next()
    } else {
        None
    }
}
fn cited_terminal_cycle(
    model: &FaceModel,
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> Option<Vec<usize>> {
    if model.faces.len() != 4 || model.face_adjacency.len() != 3 {
        return None;
    }
    let plan = ast.citation_plan.as_ref()?;
    let centers = model
        .faces
        .iter()
        .filter(|f| {
            f.size == 7
                && model
                    .face_adjacency
                    .iter()
                    .all(|&(a, b, _)| a == f.id || b == f.id)
        })
        .collect::<Vec<_>>();
    if centers.len() != 1 {
        return None;
    }
    let terminals = model
        .faces
        .iter()
        .filter(|f| f.id != centers[0].id)
        .map(|f| (f.id, f))
        .collect::<BTreeMap<_, _>>();
    let mut sizes = terminals.values().map(|f| f.size).collect::<Vec<_>>();
    sizes.sort();
    if sizes != vec![5, 5, 6] {
        return None;
    }
    let matches = ast
        .component_occurrences
        .iter()
        .map(|m| (m.occurrence_id, m))
        .collect::<BTreeMap<_, _>>();
    let first = plan
        .render_order
        .iter()
        .filter_map(|o| matches.get(o).copied())
        .find(|m| {
            m.covered_face_ids.len() == 1
                && m.covered_face_ids.iter().all(|f| terminals.contains_key(f))
        })?;
    let face = terminals[first.covered_face_ids.first()?];
    if face.size != 5 {
        return None;
    }
    let local = first.input_atom_by_locant();
    let cycle = specs
        .get(&first.occurrence_id)?
        .peripheral_order
        .iter()
        .map(|l| local.get(l).copied())
        .collect::<Option<Vec<_>>>()?;
    if cycle.len() != face.size
        || cycle.iter().copied().collect::<AtomSet>() != face.atom_cycle.iter().copied().collect()
        || cycle_edges(&cycle) != cycle_edges(&face.atom_cycle)
    {
        return None;
    }
    Some(cycle.into_iter().rev().collect())
}
fn entry_direction_layouts(
    model: &FaceModel,
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    budget: usize,
) -> Result<Option<Vec<FusedLayout>>, String> {
    if model.faces.len() < 4
        || !model.faces.iter().any(|f| [5, 7].contains(&f.size))
        || model.faces.iter().any(|f| f.size > 7)
    {
        return Ok(None);
    }
    let Some(order) = opsin_construction_atom_order(ast, specs) else {
        return Ok(None);
    };
    if order.iter().copied().collect::<AtomSet>() != all_atoms(model) {
        return Ok(None);
    }
    let terminal = cited_terminal_cycle(model, ast, specs);
    let Some((reference, candidates)) =
        super::entry_geometry::entry_direction_geometry_with_options(
            model,
            budget,
            terminal.as_deref(),
            false,
        )?
    else {
        return Ok(None);
    };
    if order.iter().copied().collect::<AtomSet>()
        != reference
            .atom_positions
            .iter()
            .map(|&(a, _, _)| a)
            .collect()
    {
        return Ok(None);
    }
    let positions = reference
        .atom_positions
        .iter()
        .map(|&(a, x, y)| (a, (x, y)))
        .collect::<Positions>();
    let mut layouts = vec![];
    for candidate in candidates {
        let area = (0..candidate.perimeter.len())
            .map(|i| {
                let (a, b) = (
                    positions[&candidate.perimeter[i]],
                    positions[&candidate.perimeter[(i + 1) % candidate.perimeter.len()]],
                );
                a.0 as i128 * b.1 as i128 - b.0 as i128 * a.1 as i128
            })
            .sum::<i128>();
        let sign = if area > 0 { -1 } else { 1 };
        let mut layout = reference.clone();
        for (_, x, _) in &mut layout.atom_positions {
            *x *= sign
        }
        for (_, x, _) in &mut layout.face_positions {
            *x *= sign
        }
        layout.orientation_score = candidate.orientation_score;
        layout.audit_evidence.extend([
            OPSIN_ENTRY_DIRECTIONS.into(),
            OPSIN_CONSTRUCTION_NUMBERING.into(),
        ]);
        if terminal.is_some() {
            layout
                .audit_evidence
                .push("terminal entry follows first cited component perimeter".into())
        }
        layout
            .audit_evidence
            .push("complete perimeter follows entry-relative ring directions".into());
        layout.audit_evidence.push(
            if candidate.direction_conflicts.is_empty() {
                "reciprocal entry directions and closed ring map"
            } else {
                "nonreciprocal parser directions projected from unique terminal"
            }
            .into(),
        );
        layout
            .audit_evidence
            .push("OPSIN numbering compatibility only; IUPAC PIN not certified".into());
        layout.kind = LayoutKind::OpsinEntry;
        layout.construction_atom_order = Some(order.clone());
        layout.entry_perimeter = Some(candidate.perimeter);
        layout.entry_face_id = Some(candidate.start_face);
        layout.entry_ring_positions = candidate.ring_positions;
        layout.entry_direction_conflicts = candidate.direction_conflicts;
        layouts.push(layout)
    }
    Ok(Some(layouts))
}
pub fn component_entry_layouts(
    model: &FaceModel,
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
) -> Result<Option<Vec<FusedLayout>>, String> {
    component_entry_layouts_with_budget(
        model,
        ast,
        specs,
        fusion_nomenclature_config().search.layout_states,
    )
}
pub fn component_entry_layouts_with_budget(
    model: &FaceModel,
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    search_budget: usize,
) -> Result<Option<Vec<FusedLayout>>, String> {
    if search_budget < 1 {
        return Err("layout search budget must be positive".into());
    }
    let atoms = all_atoms(model);
    let boundary = model.outer_boundary.iter().copied().collect::<AtomSet>();
    let interior = atoms.difference(&boundary).count();
    let ring_map = opsin_ring_map_tree(model);
    if interior > 1
        && !ring_map
        && let Some(layouts) = entry_direction_layouts(model, ast, specs, search_budget)?
    {
        return Ok(Some(layouts));
    }
    if interior > 1 || ring_map {
        let Some(order) = opsin_construction_atom_order(ast, specs) else {
            return Ok(None);
        };
        if order.iter().copied().collect::<AtomSet>() != atoms {
            return Ok(Some(vec![]));
        }
        let mut layouts = preferred_intrinsic_layouts_with_options(
            model,
            search_budget,
            fusion_nomenclature_config().search.maximum_layouts,
            ring_map,
        )?;
        for layout in &mut layouts {
            layout.kind = LayoutKind::OpsinConstruction;
            layout.construction_atom_order = Some(order.clone());
            layout.audit_evidence.extend([
                OPSIN_CONSTRUCTION_NUMBERING.into(),
                "OPSIN numbering compatibility only; IUPAC PIN not certified".into(),
            ]);
        }
        return Ok(Some(ordered_construction_layouts(
            model,
            ast,
            specs,
            layouts,
            search_budget,
        )));
    }
    if !can_use_component_entry_layout(model) {
        return entry_direction_layouts(model, ast, specs, search_budget);
    }
    if !valid_face_adjacency(model) {
        return Ok(None);
    }
    for shape in &fusion_nomenclature_config().ring_shapes {
        let Some(port) = shape.directed_entry_port else {
            continue;
        };
        for central in &model.faces {
            if central.size != shape.ring_size
                || model
                    .face_adjacency
                    .iter()
                    .any(|&(a, b, _)| a != central.id && b != central.id)
            {
                continue;
            }
            let terminals = model
                .faces
                .iter()
                .filter(|f| f.id != central.id)
                .collect::<Vec<_>>();
            let roots = terminals
                .iter()
                .copied()
                .filter(|f| f.size == shape.entry_component_size)
                .collect::<Vec<_>>();
            if roots.len() != 1
                || terminals
                    .iter()
                    .any(|f| f.size < shape.entry_component_size)
            {
                continue;
            }
            let root = roots[0];
            let mut terminal_shapes = terminals
                .iter()
                .filter(|f| f.id != root.id)
                .map(|f| symmetric_terminal_shape(f.size).map(|s| (f.id, s)))
                .collect::<Option<BTreeMap<_, _>>>();
            let Some(ref mut terminal_shapes) = terminal_shapes else {
                continue;
            };
            let matches = ast
                .component_occurrences
                .iter()
                .filter(|m| m.covered_face_ids == AtomSet::from([root.id]))
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                continue;
            }
            let m = matches[0];
            let local = m.input_atom_by_locant();
            let Some(spec) = specs.get(&m.occurrence_id) else {
                continue;
            };
            let Some(cycle) = spec
                .peripheral_order
                .iter()
                .map(|l| local.get(l).copied())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            if cycle.len() != root.size
                || cycle.iter().copied().collect::<AtomSet>()
                    != root.atom_cycle.iter().copied().collect()
                || cycle_edges(&cycle) != cycle_edges(&root.atom_cycle)
            {
                continue;
            }
            let shared = central
                .atom_cycle
                .iter()
                .filter(|a| cycle.contains(a))
                .copied()
                .collect::<AtomSet>();
            let interfaces = ast
                .joins
                .iter()
                .filter(|j| {
                    j.interface.attached_occurrence == m.occurrence_id
                        && j.interface
                            .ordered_input_atoms
                            .iter()
                            .copied()
                            .collect::<AtomSet>()
                            == shared
                })
                .map(|j| &j.interface)
                .collect::<Vec<_>>();
            if interfaces.len() != 1
                || shared.contains(&cycle[cycle.len() - 1])
                || shared.contains(&cycle[0])
            {
                continue;
            }
            let entry = &interfaces[0].ordered_input_atoms;
            if entry.len() != 2 {
                continue;
            }
            let orders = orders_starting_with_edge(&central.atom_cycle, (entry[0], entry[1]));
            if orders.len() != 1 {
                continue;
            }
            let mut order = orders[0].clone();
            order.rotate_right(port);
            let ports = (0..order.len())
                .filter(|&i| {
                    terminals.iter().any(|f| {
                        f.atom_cycle.contains(&order[i])
                            && f.atom_cycle.contains(&order[(i + 1) % order.len()])
                    })
                })
                .collect::<AtomSet>();
            let required = std::iter::once(port)
                .chain(shape.opposite_ports.iter().copied())
                .collect::<AtomSet>();
            if ports != required {
                continue;
            }
            let root_shapes = shapes_for(root.size, "cartesian")
                .into_iter()
                .filter(|s| s.distortion_rank == 0)
                .collect::<Vec<_>>();
            if root_shapes.len() != 1 {
                continue;
            }
            terminal_shapes.insert(root.id, root_shapes[0].clone());
            let mut budget = Budget::new(search_budget);
            budget.spend()?;
            let (dx, dy) = (
                shape.vertices[(port + 1) % shape.ring_size].0 - shape.vertices[port].0,
                shape.vertices[(port + 1) % shape.ring_size].1 - shape.vertices[port].1,
            );
            let scale = model.faces.iter().fold(1, |n, f| lcm(n, f.size as i64));
            let mut positions = order
                .iter()
                .copied()
                .zip(
                    shape
                        .vertices
                        .iter()
                        .map(|&(x, y)| ((-dx * x - dy * y) * scale, (dy * x - dx * y) * scale)),
                )
                .collect::<Positions>();
            let mut placed = vec![(central.id, order.clone())];
            for f in terminals {
                budget.spend()?;
                #[allow(
                    clippy::unwrap_used,
                    reason = "the directed port test proves each terminal shares a central cycle edge"
                )]
                let e = (0..order.len())
                    .map(|i| (order[i], order[(i + 1) % order.len()]))
                    .find(|&(a, b)| f.atom_cycle.contains(&a) && f.atom_cycle.contains(&b))
                    .unwrap();
                let ends = (e.1, e.0);
                let fo = orders_starting_with_edge(&f.atom_cycle, ends).remove(0);
                for p in positions.values_mut() {
                    p.0 *= 4;
                    p.1 *= 4
                }
                let candidate = place_shape(
                    &terminal_shapes[&f.id],
                    &fo,
                    positions[&ends.0],
                    positions[&ends.1],
                    "cartesian",
                );
                positions.extend(candidate);
                placed.push((f.id, fo));
            }
            if !audit_layout(model, &placed, &positions, false) {
                return Ok(Some(vec![]));
            }
            let centers = placed
                .iter()
                .map(|(f, o)| (*f, ring_axis_center(o, &positions)))
                .collect();
            let (positions, centers) = normalize_integer_layout(&positions, &centers);
            let mut fs = vec![(central.id, shape.shape_id.clone())];
            fs.extend(
                terminal_shapes
                    .iter()
                    .map(|(&f, s)| (f, s.shape_id.clone())),
            );
            fs.sort();
            return Ok(Some(vec![FusedLayout {
                face_positions: centers.iter().map(|(&f, &(x, y))| (f, x, y)).collect(),
                atom_positions: positions.iter().map(|(&a, &(x, y))| (a, x, y)).collect(),
                face_shapes: fs,
                orientation_score: orientation_score(
                    &centers,
                    &BTreeMap::new(),
                    &adjacent(model),
                    shape.distortion_rank as i64,
                    &placed,
                    &positions,
                    false,
                ),
                component_entry_edge: Some((entry[0], entry[1])),
                audit_evidence: vec![
                    "configured opposite-port direction witness",
                    "entry follows the graph-bound ordered fusion interface",
                    "complete original face geometry audited",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                ..Default::default()
            }]));
        }
    }
    entry_direction_layouts(model, ast, specs, search_budget)
}
fn ordered_construction_layouts(
    model: &FaceModel,
    ast: &FusionNameAst,
    specs: &BTreeMap<usize, FusionComponentSpec>,
    layouts: Vec<FusedLayout>,
    budget: usize,
) -> Vec<FusedLayout> {
    let atoms = all_atoms(model);
    let boundary = model.outer_boundary.iter().copied().collect::<AtomSet>();
    if layouts.is_empty()
        || atoms.difference(&boundary).count() < 2
        || model.faces.iter().any(|f| f.size != 6)
    {
        return layouts;
    }
    let reference = layouts[0]
        .atom_positions
        .iter()
        .map(|&(a, x, y)| (a, (x, y)))
        .collect::<Positions>();
    let Some(witness) = super::construction_order::ordered_hexagonal_construction(
        ast, specs, model, &reference, budget,
    ) else {
        return layouts;
    };
    let area = |p: &Positions| {
        (0..model.outer_boundary.len())
            .map(|i| {
                let (a, b) = (
                    p[&model.outer_boundary[i]],
                    p[&model.outer_boundary[(i + 1) % model.outer_boundary.len()]],
                );
                a.0 as i128 * b.1 as i128 - b.0 as i128 * a.1 as i128
            })
            .sum::<i128>()
    };
    let reference_area = area(&reference);
    let mut result = vec![];
    let mut witnessed = BTreeSet::new();
    for layout in &layouts {
        let Some(&(top, _, _)) = layout
            .face_positions
            .iter()
            .max_by_key(|&&(_, x, y)| (y, x))
        else {
            return layouts;
        };
        let positions = layout
            .atom_positions
            .iter()
            .map(|&(a, x, y)| (a, (x, y)))
            .collect();
        let winding = if area(&positions) * reference_area > 0 {
            -1
        } else {
            1
        };
        let entry = (top, winding);
        let priority = witness.entries.iter().position(|e| *e == entry);
        if priority.is_some() {
            witnessed.insert(entry);
        }
        let mut layout = layout.clone();
        layout.construction_atom_order = Some(witness.atom_order.clone());
        layout.construction_numbering_priority = priority.map(|i| ComparisonKey::Int(i as i64));
        layout
            .audit_evidence
            .push("ordered hexagonal construction enumeration".into());
        result.push(layout)
    }
    if witnessed != witness.entries.iter().copied().collect() {
        layouts
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::openclatura::{
        fusion::faces::{select_bounded_face_model, typed_face_model},
        graph_io::read_smiles,
    };
    #[test]
    fn hexagonal_bicycle_retains_all_preferred_reflections() {
        let mol = read_smiles("c1ccc2ccccc2c1").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
        let model = typed_face_model(&mol, &bounded);
        let layouts = preferred_intrinsic_layouts(&model).unwrap();
        assert_eq!(layouts.len(), 4);
        for layout in layouts {
            assert_eq!(layout.orientation_score, vec![0, -2, -2, 2, -4]);
            assert_eq!(layout.atom_positions.len(), 10);
            let positions = layout
                .atom_positions
                .iter()
                .map(|&(a, x, y)| (a, (x, y)))
                .collect();
            assert!(audit_layout(
                &model,
                &model
                    .faces
                    .iter()
                    .map(|f| (f.id, f.atom_cycle.clone()))
                    .collect(),
                &positions,
                false
            ));
        }
    }
    #[test]
    fn preferred_geometry_preserves_atom_permutation_independence() {
        let mol = read_smiles("c1ccc2cc3ccccc3cc2c1").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
        let model = typed_face_model(&mol, &bounded);
        let layouts = preferred_intrinsic_layouts(&model).unwrap();
        assert!(!layouts.is_empty());
        let n = mol.atoms.len();
        let mut permuted = model.clone();
        for f in &mut permuted.faces {
            for a in &mut f.atom_cycle {
                *a = n - 1 - *a
            }
        }
        for a in &mut permuted.outer_boundary {
            *a = n - 1 - *a
        }
        let other = preferred_intrinsic_layouts(&permuted).unwrap();
        assert_eq!(layouts[0].orientation_score, other[0].orientation_score);
        let geometry = |layouts: &[FusedLayout]| {
            layouts
                .iter()
                .map(|l| {
                    let mut points = l
                        .atom_positions
                        .iter()
                        .map(|&(_, x, y)| (x, y))
                        .collect::<Vec<_>>();
                    points.sort();
                    points
                })
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(geometry(&layouts), geometry(&other));
    }
    #[test]
    fn two_port_macrocycle_retains_the_source_orientation_witness() {
        let mol = read_smiles(
            "C=C(C)[C@H]1CC[C@@]23O[C@@H]2[C@@H](C/C(C)=C2/C(=O)C=C(C)[C@@]2(O)CC1)OC3=O",
        )
        .unwrap();
        let atoms = AtomSet::from([3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 16, 17, 19, 21, 22]);
        let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
        let model = typed_face_model(&mol, &bounded);
        let mut sizes = model.faces.iter().map(|f| f.size).collect::<Vec<_>>();
        sizes.sort();
        assert_eq!(sizes, vec![3, 5, 12]);
        let layouts = preferred_intrinsic_layouts(&model).unwrap();
        assert!(!layouts.is_empty());
        for layout in layouts {
            assert_eq!(layout.orientation_score, vec![0, -2, -6, 2, -8]);
            assert!(
                layout
                    .face_shapes
                    .iter()
                    .any(|(_, shape)| shape.starts_with("two-port-12:"))
            );
        }
    }

    #[test]
    fn coupled_and_branched_large_rings_need_an_extra_parser_witness() {
        use crate::naming::openclatura::molecule::{Atom, Bond, Molecule};
        let model = |cycles: Vec<Vec<usize>>| {
            let edges = cycles
                .iter()
                .flat_map(|cycle| cycle_edges(cycle))
                .collect::<BTreeSet<_>>();
            let mut mol = Molecule::new();
            for atom in edges.iter().flat_map(|&(a, b)| [a, b]).collect::<AtomSet>() {
                mol.add_atom(Atom::new(atom, "C")).unwrap();
            }
            for (id, (a, b)) in edges.into_iter().enumerate() {
                mol.add_bond(Bond::new(id, a, b, 1)).unwrap();
            }
            let atoms = mol.atoms.keys().copied().collect();
            typed_face_model(
                &mol,
                &select_bounded_face_model(&mol, &atoms).unwrap().unwrap(),
            )
        };
        // One terminal large ring remains supported, including macrocycles.
        assert!(parser_numbering_is_supported(&model(vec![
            (0..7).collect(),
            vec![0, 1, 7, 8, 9, 10]
        ])));
        assert!(parser_numbering_is_supported(&model(vec![
            (0..12).collect(),
            vec![0, 1, 12, 13, 14]
        ])));
        // Two large rings connected through a third ring require a joint direction proof.
        assert!(!parser_numbering_is_supported(&model(vec![
            (0..7).collect(),
            vec![0, 1, 7, 8, 9, 10],
            vec![8, 9, 11, 12, 13, 14, 15]
        ])));
        // Three fusion interfaces require a construction-bound entry for both odd and even rings.
        for size in [7, 8] {
            assert!(!parser_numbering_is_supported(&model(vec![
                (0..size).collect(),
                vec![0, 1, size, size + 1, size + 2, size + 3],
                vec![2, 3, size + 4, size + 5, size + 6, size + 7],
                vec![4, 5, size + 8, size + 9, size + 10, size + 11]
            ])));
        }
    }
}
