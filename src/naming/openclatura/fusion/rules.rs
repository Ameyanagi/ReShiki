//! Pure fusion eligibility, component identity, and chemical seniority.
use super::super::locants::{LocantKey, retained_locant_sort_key as locant_key};
use super::{config::fusion_nomenclature_config, model::*};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChemicalComponentSeniorityKey {
    pub earliest_special_heteroatom: usize,
    pub ring_count: i64,
    pub ring_size_vector: Vec<i64>,
    pub heteroatom_count: i64,
    pub heteroatom_kind_count: i64,
    pub heteroatom_counts_by_priority: Vec<i64>,
    pub horizontal_row_count: i64,
    pub all_heteroatom_locants: Vec<LocantKey>,
    pub per_element_locants: Vec<Vec<LocantKey>>,
    pub peripheral_fusion_carbon_locants: Vec<LocantKey>,
}
impl ChemicalComponentSeniorityKey {
    pub fn as_tuple(&self) -> ComparisonKey {
        let lk = |l: &LocantKey| {
            ComparisonKey::tuple([
                l.0.into(),
                (l.1 as i64).into(),
                ComparisonKey::Str(l.2.clone()),
                l.3.into(),
            ])
        };
        ComparisonKey::tuple([
            self.earliest_special_heteroatom.into(),
            self.ring_count.into(),
            seq(&self.ring_size_vector, |n| (*n).into()),
            self.heteroatom_count.into(),
            self.heteroatom_kind_count.into(),
            seq(&self.heteroatom_counts_by_priority, |n| (*n).into()),
            self.horizontal_row_count.into(),
            seq(&self.all_heteroatom_locants, lk),
            seq(&self.per_element_locants, |v| seq(v, lk)),
            seq(&self.peripheral_fusion_carbon_locants, lk),
        ])
    }
}
#[allow(clippy::expect_used)]
static ELEMENT_DATA: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/elements.json")).expect("checked-in element policy")
});
fn precedence(field: &str) -> Vec<String> {
    let mut rows: Vec<_> = ELEMENT_DATA["elements"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r[field].as_u64()?, r["symbol"].as_str()?.to_owned())))
        .collect();
    rows.sort();
    rows.into_iter().map(|(_, s)| s).collect()
}
pub fn earliest_special_heteroatom_precedence() -> &'static [String] {
    static PRECEDENCE: LazyLock<Vec<String>> =
        LazyLock::new(|| precedence("fusion_special_priority"));
    &PRECEDENCE
}
pub fn general_heteroatom_count_precedence() -> &'static [String] {
    static PRECEDENCE: LazyLock<Vec<String>> =
        LazyLock::new(|| precedence("fusion_general_priority"));
    &PRECEDENCE
}
pub fn fusion_supported(symbol: &str) -> bool {
    ELEMENT_DATA["elements"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|r| r["symbol"] == symbol && r["fusion_supported"] == true)
}
pub fn fusion_ring_size_gate(sizes: &[usize]) -> bool {
    let r = &fusion_nomenclature_config().rules;
    sizes.iter().filter(|&&n| n >= r.minimum_ring_size).count() >= r.minimum_ring_count
}
pub fn component_spec_seniority_key(spec: &FusionComponentSpec) -> ChemicalComponentSeniorityKey {
    let special = earliest_special_heteroatom_precedence();
    let general = general_heteroatom_count_precedence();
    let hetero: Vec<_> = spec.atoms().iter().filter(|a| a.symbol != "C").collect();
    let mut counts = BTreeMap::<&str, i64>::new();
    for a in &hetero {
        *counts.entry(&a.symbol).or_default() += 1
    }
    let mut all = hetero
        .iter()
        .map(|a| locant_key(&a.locant))
        .collect::<Vec<_>>();
    all.sort();
    let per = general
        .iter()
        .map(|s| {
            let mut v = hetero
                .iter()
                .filter(|a| a.symbol == *s)
                .map(|a| locant_key(&a.locant))
                .collect::<Vec<_>>();
            v.sort();
            v
        })
        .collect();
    let mut sizes = spec.ring_sizes();
    sizes.sort_by(|a, b| b.cmp(a));
    let mut fusion = spec
        .fusion_carbon_locants()
        .iter()
        .map(|l| locant_key(l))
        .collect::<Vec<_>>();
    fusion.sort();
    ChemicalComponentSeniorityKey {
        earliest_special_heteroatom: counts
            .keys()
            .filter_map(|s| special.iter().position(|x| x == s))
            .min()
            .unwrap_or(special.len()),
        ring_count: -(spec.rings.len() as i64),
        ring_size_vector: sizes.into_iter().map(|n| -(n as i64)).collect(),
        heteroatom_count: -(hetero.len() as i64),
        heteroatom_kind_count: -(counts.len() as i64),
        heteroatom_counts_by_priority: general
            .iter()
            .map(|s| -counts.get(s.as_str()).copied().unwrap_or(0))
            .collect(),
        horizontal_row_count: -(spec.horizontal_ring_count as i64),
        all_heteroatom_locants: all,
        per_element_locants: per,
        peripheral_fusion_carbon_locants: fusion,
    }
}
pub fn component_parent_eligible(
    component: &FusionComponentMatch,
    spec: &FusionComponentSpec,
    components: &[FusionComponentMatch],
) -> bool {
    if spec.usable_as_parent {
        return true;
    }
    if !spec.usable_as_peri_parent || !spec.usable_as_attached || spec.rings.len() < 2 {
        return false;
    }
    components
        .iter()
        .filter(|o| component.covered_face_ids.is_disjoint(&o.covered_face_ids))
        .any(|o| {
            let atoms: BTreeSet<_> = o.local_to_input_atom.iter().map(|(_, a)| a).collect();
            let shared: BTreeSet<_> = component
                .local_to_input_atom
                .iter()
                .filter(|(_, a)| atoms.contains(a))
                .map(|(l, _)| l)
                .collect();
            shared.len() >= 3
                && !spec
                    .rings
                    .iter()
                    .any(|ring| shared.iter().all(|l| ring.contains(l)))
        })
}
fn text(s: impl Into<String>) -> ComparisonKey {
    ComparisonKey::Str(s.into())
}
fn seq<T>(values: impl IntoIterator<Item = T>, f: impl Fn(T) -> ComparisonKey) -> ComparisonKey {
    ComparisonKey::Tuple(values.into_iter().map(f).collect())
}
pub fn component_variant_identity(spec: &FusionComponentSpec) -> ComparisonKey {
    let atoms = seq(spec.atoms(), |a| {
        ComparisonKey::tuple([
            text(&a.locant),
            text(&a.symbol),
            a.charge.into(),
            i64::from(a.aromatic).into(),
            i64::from(a.fusion).into(),
            i64::from(a.saturated).into(),
            i64::from(spec.interior_atoms.contains(&a.locant)).into(),
        ])
    });
    let mut bonds: Vec<_> = spec
        .bonds()
        .iter()
        .map(|b| {
            let mut pair = vec![b.locants.0.clone(), b.locants.1.clone()];
            pair.sort();
            ComparisonKey::tuple([seq(pair, text), text(&b.bond_class)])
        })
        .collect();
    bonds.sort();
    let mut rings: Vec<_> = spec
        .rings
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r.sort_by_key(|l| locant_key(l));
            seq(r, text)
        })
        .collect();
    rings.sort();
    ComparisonKey::tuple([
        text(&spec.template.name),
        atoms,
        ComparisonKey::Tuple(bonds),
        ComparisonKey::Tuple(rings),
        text(&spec.multiplicative_prefix_style),
    ])
}
pub fn component_canonicalization_key(spec: &FusionComponentSpec) -> ComparisonKey {
    ComparisonKey::tuple([
        text(&spec.parent_name),
        text(&spec.attached_prefix),
        component_variant_identity(spec),
    ])
}
/// All typed automorphisms, with exact bond classes, atom roles, and face sets.
pub fn component_automorphisms(spec: &FusionComponentSpec) -> Vec<BTreeMap<String, String>> {
    let n = spec.locants().len();
    let atoms = spec.template.atom_by_locant();
    let idx: BTreeMap<_, _> = spec
        .locants()
        .iter()
        .enumerate()
        .map(|(i, l)| (l, i))
        .collect();
    let mut edges = BTreeMap::new();
    let mut adj = vec![BTreeSet::new(); n];
    for b in spec.bonds() {
        let a = idx[&b.locants.0];
        let c = idx[&b.locants.1];
        edges.insert((a.min(c), a.max(c)), &b.bond_class);
        adj[a].insert(c);
        adj[c].insert(a);
    }
    let role = |i: usize| {
        let l = &spec.locants()[i];
        let a = &atoms[l];
        (
            &a.symbol,
            a.charge,
            a.aromatic,
            a.fusion,
            a.default_h,
            a.saturated,
            spec.interior_atoms.contains(l),
            spec.peripheral_order.contains(l),
        )
    };
    let candidates: Vec<Vec<usize>> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| role(i) == role(j) && adj[i].len() == adj[j].len())
                .collect()
        })
        .collect();
    let mut assignments = Vec::new();
    fn walk(
        adj: &[BTreeSet<usize>],
        edges: &BTreeMap<(usize, usize), &String>,
        candidates: &[Vec<usize>],
        mapped: &mut Vec<Option<usize>>,
        used: &mut BTreeSet<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if used.len() == mapped.len() {
            out.push(mapped.iter().flatten().copied().collect());
            return;
        }
        let i = (0..mapped.len())
            .filter(|&i| mapped[i].is_none())
            .min_by_key(|&i| {
                (
                    usize::MAX - adj[i].iter().filter(|&&j| mapped[j].is_some()).count(),
                    candidates[i].len(),
                    i,
                )
            })
            .unwrap_or(0);
        for &j in &candidates[i] {
            if used.contains(&j) {
                continue;
            }
            let good = mapped.iter().enumerate().all(|(a, b)| {
                b.is_none_or(|b| {
                    edges.get(&(i.min(a), i.max(a))) == edges.get(&(j.min(b), j.max(b)))
                })
            });
            if !good {
                continue;
            }
            mapped[i] = Some(j);
            used.insert(j);
            walk(adj, edges, candidates, mapped, used, out);
            used.remove(&j);
            mapped[i] = None;
        }
    }
    walk(
        &adj,
        &edges,
        &candidates,
        &mut vec![None; n],
        &mut BTreeSet::new(),
        &mut assignments,
    );
    let rings: BTreeSet<BTreeSet<usize>> = spec
        .rings
        .iter()
        .map(|r| r.iter().map(|l| idx[l]).collect())
        .collect();
    assignments
        .into_iter()
        .filter(|a| {
            spec.rings
                .iter()
                .map(|r| r.iter().map(|l| a[idx[l]]).collect::<BTreeSet<_>>())
                .collect::<BTreeSet<_>>()
                == rings
        })
        .map(|a| {
            spec.locants()
                .iter()
                .cloned()
                .zip(a.into_iter().map(|i| spec.locants()[i].clone()))
                .collect()
        })
        .collect()
}
pub fn component_interface_orbit(spec: &FusionComponentSpec, path: &[String]) -> Vec<String> {
    let mut variants = Vec::new();
    for a in component_automorphisms(spec) {
        let mapped = path.iter().map(|l| a[l].clone()).collect::<Vec<_>>();
        variants.push(mapped.clone());
        variants.push(mapped.into_iter().rev().collect())
    }
    variants
        .into_iter()
        .min_by_key(|p| p.iter().map(|l| locant_key(l)).collect::<Vec<_>>())
        .unwrap_or_else(|| path.to_vec())
}
pub fn multiplicative_attachment_key(
    spec: &FusionComponentSpec,
    join: &FusionJoin,
) -> ComparisonKey {
    ComparisonKey::tuple([
        component_variant_identity(spec),
        text(join.kind().as_str()),
        join.order.into(),
        text(if join.host_sides().is_empty() {
            "locants"
        } else {
            "sides"
        }),
        seq(
            component_interface_orbit(
                spec,
                &join
                    .interface
                    .attached_path
                    .iter()
                    .map(|l| l.text.clone())
                    .collect::<Vec<_>>(),
            ),
            text,
        ),
    ])
}
fn locant_comparison(l: &str) -> ComparisonKey {
    let (n, b, s, d) = locant_key(l);
    ComparisonKey::tuple([n.into(), (b as i64).into(), text(s), d.into()])
}
pub fn multiplicative_member_order_key(join: &FusionJoin) -> ComparisonKey {
    ComparisonKey::tuple([
        seq(join.host_sides(), |s| {
            seq(s.letter.bytes(), |c| ((c - b'a') as i64).into())
        }),
        seq(join.attached_locants(), |l| locant_comparison(&l.text)),
    ])
}
