//! Graph-backed retained and generated fusion-component registry.
use super::super::{
    locants::retained_locant_sort_key as locant_key,
    molecule::{AtomSet, Molecule},
    nomenclature::section,
    retained_fused_templates::{
        TopologyKey, match_retained_graph_template_maps_with_options, molecule_graph_topology_key,
        retained_graph_template_topology_key, retained_graph_templates,
    },
    retained_graph_model::{
        RetainedGraphAtomTemplate, RetainedGraphBondTemplate, RetainedGraphTemplate,
    },
    retained_name_policy::retained_parent_name_policy,
    rules::{retained, stems},
};
use super::{config::annulene_ring_sizes_from_data, model::*};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
#[allow(
    dead_code,
    reason = "Role filters remain part of the graph registry API; production requests both roles together"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FusionComponentRole {
    Parent,
    Attached,
}
#[derive(Clone, Debug)]
pub struct RegisteredFusionComponent {
    pub spec: FusionComponentSpec,
    pub template_names: Vec<String>,
    pub templates: Vec<FusionComponentSpec>,
    pub omit_attached_locants: bool,
    pub omit_ortho_peri_attached_locants: bool,
}
type GeneratedRingMatch = (RegisteredFusionComponent, Vec<(String, usize)>);
impl RegisteredFusionComponent {
    pub fn eligible_for(&self, role: FusionComponentRole) -> bool {
        match role {
            FusionComponentRole::Parent => self.spec.usable_as_parent,
            FusionComponentRole::Attached => self.spec.usable_as_attached,
        }
    }
    pub fn spec_for_template(&self, name: &str) -> Result<FusionComponentSpec, String> {
        if name.is_empty() || name == self.spec.template.name {
            return Ok(self.spec.clone());
        }
        self.templates
            .iter()
            .find(|s| s.template.name == name)
            .cloned()
            .ok_or_else(|| format!("unknown fusion template {name}"))
    }
}
#[derive(Clone, Debug)]
struct GeneratedHWPolicy {
    usable_as_parent: bool,
    usable_as_attached: bool,
    rule_reference: String,
}
#[derive(Clone, Debug)]
pub struct FusionComponentRegistry {
    pub version: String,
    components: Vec<RegisteredFusionComponent>,
    by_key: BTreeMap<String, usize>,
    template_by_name: BTreeMap<String, FusionComponentSpec>,
    topology_index: BTreeMap<(usize, TopologyKey), Vec<(usize, usize)>>,
    generated_hw_policy: Option<GeneratedHWPolicy>,
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn txt(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or_default().to_owned()
}
fn metadata(template: RetainedGraphTemplate, row: &Value) -> FusionComponentSpec {
    let t = &row["template"];
    let rings = t["rings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(strings)
        .collect();
    FusionComponentSpec {
        parent_name: template.output_name(),
        attached_prefix: txt(row, "attached_prefix"),
        rings,
        peripheral_order: strings(&t["peripheral_atoms"]),
        interior_atoms: strings(&t["interior_atoms"]),
        pin_component: row["pin"].as_bool().unwrap_or(false),
        template,
        ..Default::default()
    }
}
fn ordered_perimeter(mut spec: FusionComponentSpec) -> FusionComponentSpec {
    let peripheral: BTreeSet<_> = spec.peripheral_order.iter().cloned().collect();
    let rings: Vec<BTreeSet<_>> = spec
        .rings
        .iter()
        .map(|r| r.iter().cloned().collect())
        .collect();
    let mut adj: BTreeMap<String, BTreeSet<String>> = peripheral
        .iter()
        .map(|l| (l.clone(), BTreeSet::new()))
        .collect();
    for b in spec.bonds() {
        let (a, c) = &b.locants;
        if peripheral.contains(a)
            && peripheral.contains(c)
            && rings
                .iter()
                .filter(|r| r.contains(a) && r.contains(c))
                .count()
                == 1
        {
            adj.get_mut(a).map(|v| v.insert(c.clone()));
            adj.get_mut(c).map(|v| v.insert(a.clone()));
        }
    }
    if peripheral.is_empty() || adj.values().any(|v| v.len() != 2) {
        return spec;
    }
    let Some(start) = peripheral.iter().min_by_key(|l| locant_key(l)).cloned() else {
        return spec;
    };
    let first = if start == "1" && adj[&start].contains("2") {
        String::from("2")
    } else {
        adj[&start]
            .iter()
            .min_by_key(|l| locant_key(l))
            .cloned()
            .unwrap_or_default()
    };
    let mut order = vec![start.clone(), first];
    while order.len() < peripheral.len() {
        let last = &order[order.len() - 1];
        let previous = &order[order.len() - 2];
        let candidates: Vec<_> = adj[last]
            .iter()
            .filter(|l| *l != previous)
            .cloned()
            .collect();
        if candidates.len() != 1 || order.contains(&candidates[0]) {
            return spec;
        }
        order.push(candidates[0].clone())
    }
    if adj[order.last().unwrap_or(&start)].contains(&start) {
        spec.peripheral_order = order
    }
    spec
}
fn monocycle(name: &str, symbols: &[String], bond_class: &str, pin: bool) -> FusionComponentSpec {
    let locants: Vec<_> = (1..=symbols.len()).map(|i| i.to_string()).collect();
    let atoms = locants
        .iter()
        .zip(symbols)
        .map(|(l, s)| RetainedGraphAtomTemplate {
            locant: l.clone(),
            symbol: s.clone(),
            aromatic: matches!(bond_class, "aromatic" | "mancude"),
            ..Default::default()
        })
        .collect();
    let bonds = locants
        .iter()
        .enumerate()
        .map(|(i, l)| RetainedGraphBondTemplate {
            locants: (l.clone(), locants[(i + 1) % locants.len()].clone()),
            bond_class: bond_class.into(),
        })
        .collect();
    let t = RetainedGraphTemplate {
        name: name.into(),
        priority: 1000,
        locants: locants.clone(),
        atoms,
        bonds,
        family: "generated_monocycle".into(),
        aromatic_equivalence_policy: "neutral_kekule_equivalent".into(),
        charge_policy: "exact".into(),
        enabled: true,
        pre_descriptor_selection: true,
        ..Default::default()
    };
    FusionComponentSpec {
        template: t,
        rings: vec![locants.clone()],
        peripheral_order: locants,
        pin_component: pin,
        ..Default::default()
    }
}
#[allow(clippy::expect_used)]
fn construction_orders() -> BTreeMap<String, FusionComponentConstructionOrder> {
    let d: Value = serde_json::from_str(include_str!("../data/fusion_component_construction.json"))
        .expect("checked-in construction JSON");
    d["components"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, row)| {
            (
                key.clone(),
                FusionComponentConstructionOrder {
                    atom_locants: strings(&row["atom_locants"]),
                    directed_bond_locants: Vec::new(),
                    source: txt(&d, "source"),
                    version: txt(&d, "version"),
                },
            )
        })
        .map(|(key, mut c)| {
            let row = &d["components"][&key];
            c.directed_bond_locants = row["directed_bond_locants"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    (
                        e[0].as_str().unwrap_or_default().into(),
                        e[1].as_str().unwrap_or_default().into(),
                    )
                })
                .collect();
            (key, c)
        })
        .collect()
}
impl FusionComponentRegistry {
    pub fn template_spec(&self, name: &str) -> Option<&FusionComponentSpec> {
        self.template_by_name.get(name)
    }
    #[cfg(test)]
    pub fn components(&self) -> &[RegisteredFusionComponent] {
        &self.components
    }
    pub fn get(&self, key: &str) -> Option<RegisteredFusionComponent> {
        self.by_key
            .get(key)
            .map(|&i| self.components[i].clone())
            .or_else(|| self.generated_from_key(key))
    }
    pub fn spec_for_match(&self, m: &FusionComponentMatch) -> Result<FusionComponentSpec, String> {
        let c = self
            .get(&m.spec_key)
            .ok_or_else(|| format!("unknown fusion component {}", m.spec_key))?;
        c.spec_for_template(&m.template_name)
    }
    pub fn from_data(data: &Value) -> Result<Self, String> {
        if data["schema_version"] != 1 {
            return Err("unsupported fusion component schema".into());
        }
        let mut rows = BTreeMap::<String, Value>::new();
        for raw in [
            include_str!("../data/retained_fused_graph_templates.json"),
            include_str!("../data/retained_fused_hydrocarbon_templates.json"),
            include_str!("../data/retained_macrocycle_templates.json"),
        ] {
            let d: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
            for row in d["parents"].as_array().into_iter().flatten() {
                rows.insert(txt(row, "name"), row.clone());
            }
        }
        for row in section("retained_fused_polycycle_specs")
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["template"].is_object())
        {
            rows.entry(txt(row, "name")).or_insert_with(|| row.clone());
        }
        let mut registry = Self {
            version: txt(data, "registry_version"),
            components: Vec::new(),
            by_key: BTreeMap::new(),
            template_by_name: BTreeMap::new(),
            topology_index: BTreeMap::new(),
            generated_hw_policy: None,
        };
        for t in retained_graph_templates(true) {
            if let Some(row) = rows.get(&t.name) {
                registry
                    .template_by_name
                    .insert(t.name.clone(), metadata(t.clone(), row));
            }
        }
        let mut existing_sizes = BTreeSet::new();
        for row in data["generated_components"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let size = row["ring_size"].as_u64().unwrap_or(0) as usize;
            if txt(row, "generator") != "monocyclic_carbocycle" || size < 3 {
                return Err("unsupported fusion generator".into());
            }
            let name = txt(row, "key");
            let s = monocycle(
                &name,
                &vec!["C".into(); size],
                &txt(row, "bond_class"),
                row["pin_component"].as_bool().unwrap_or(true),
            );
            registry.template_by_name.insert(name, s);
            existing_sizes.insert(size);
        }
        let annulenes: Vec<_> = annulene_ring_sizes_from_data(data)?
            .into_iter()
            .filter(|s| !existing_sizes.contains(s))
            .collect();
        for &size in &annulenes {
            let name = format!("[{size}]annulene");
            registry.template_by_name.insert(
                name.clone(),
                monocycle(&name, &vec!["C".into(); size], "mancude", true),
            );
        }
        for row in data["components"].as_array().into_iter().flatten() {
            registry.register(row)?;
        }
        for size in annulenes {
            let name = format!("[{size}]annulene");
            let stem = stems::stem_for(size);
            registry.register(&json!({"key":name,"parent_name":name,"attached_prefix":format!("cyclo{stem}a"),"allow_as_parent":true,"allow_as_attached":true,"omit_attached_locants":true,"rule":"P-25.3.2.1.1"}))?;
        }
        for policy in data["retained_component_generators"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if txt(policy, "family") != "benzoheterocycle" {
                return Err("unsupported retained fusion family".into());
            }
            // The source registry iterates retained templates in their insertion order.
            let names: Vec<_> = retained_graph_templates(true)
                .iter()
                .map(|t| t.name.clone())
                .collect();
            for name in names {
                let Some(s) = registry.template_by_name.get(&name) else {
                    continue;
                };
                if registry
                    .components
                    .iter()
                    .any(|c| c.template_names.contains(&name))
                    || !is_benzoheterocycle(s)
                {
                    continue;
                }
                let h = s
                    .template
                    .default_indicated_h
                    .iter()
                    .map(|l| format!("{l}H"))
                    .collect::<Vec<_>>()
                    .join(",")
                    + "-";
                let parent = retained_parent_name_policy(&name)
                    .map(|p| p.output_name("fusion_component"))
                    .unwrap_or_else(|| s.template.output_name());
                let parent = parent.strip_prefix(&h).unwrap_or(&parent).to_owned();
                let ring_count = s.rings.len();
                registry.register(&json!({"key":name,"parent_name":parent,"allow_as_parent":policy["allow_as_parent"],"allow_as_peri_parent":policy["allow_as_peri_parent"],"allow_as_attached":policy["allow_as_attached"],"horizontal_ring_count":ring_count,"rule":policy["rule"]}))?;
            }
        }
        for p in data["systematic_component_generators"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if txt(p, "generator") != "hantzsch_widman_monocycle" {
                return Err("unsupported HW generator".into());
            }
            registry.generated_hw_policy = Some(GeneratedHWPolicy {
                usable_as_parent: p["allow_as_parent"].as_bool().unwrap_or(false),
                usable_as_attached: p["allow_as_attached"].as_bool().unwrap_or(false),
                rule_reference: txt(p, "rule"),
            });
        }
        Ok(registry)
    }
    pub fn register(&mut self, row: &Value) -> Result<FusionComponentSpec, String> {
        let key = txt(row, "key");
        if key.is_empty() || self.by_key.contains_key(&key) {
            return Err(format!("duplicate or empty fusion key {key}"));
        }
        if !row["seniority_override"].is_null() {
            return Err("seniority_override is not chemical".into());
        }
        let names = if row["template_names"].is_array() {
            strings(&row["template_names"])
        } else {
            vec![key.clone()]
        };
        let mut templates = Vec::new();
        let construction = construction_orders();
        for name in &names {
            if self
                .components
                .iter()
                .any(|c| c.template_names.contains(name))
            {
                return Err(format!("duplicate fusion template {name}"));
            }
            let mut s = ordered_perimeter(
                self.template_by_name
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("unknown fusion template {name}"))?,
            );
            s.key = key.clone();
            s.parent_name = row["parent_name"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or(s.parent_name);
            s.attached_prefix = row["preferred_fusion_prefix"]
                .as_str()
                .or_else(|| row["attached_prefix"].as_str())
                .map(str::to_owned)
                .unwrap_or(s.attached_prefix);
            s.usable_as_parent = row["allow_as_parent"].as_bool().unwrap_or(false);
            s.usable_as_attached = row["allow_as_attached"].as_bool().unwrap_or(false);
            s.usable_as_peri_parent = row["allow_as_peri_parent"].as_bool().unwrap_or(false);
            s.rule_reference = txt(row, "rule");
            s.accepted_general_prefixes = strings(&row["accepted_general_prefixes"]);
            s.horizontal_ring_count = row["horizontal_ring_count"]
                .as_u64()
                .map(|n| n as usize)
                .or_else(|| (s.rings.len() == 1).then_some(1))
                .ok_or_else(|| format!("missing horizontal ring count for {name}"))?;
            s.multiplicative_prefix_style = row["multiplicative_prefix_style"]
                .as_str()
                .unwrap_or("basic")
                .into();
            s.construction_order = construction.get(name).cloned();
            if s.construction_order
                .as_ref()
                .is_some_and(|c| c.source.is_empty())
            {
                return Err("fusion construction order requires source provenance".into());
            }
            if s.usable_as_attached && s.attached_prefix.is_empty() {
                return Err(format!("fusion component {key} has no prefix"));
            }
            templates.push(s);
        }
        let spec = templates
            .first()
            .cloned()
            .ok_or("empty component variants")?;
        let index = self.components.len();
        for (j, t) in templates.iter().enumerate() {
            self.topology_index
                .entry((
                    t.rings.len(),
                    retained_graph_template_topology_key(&t.template),
                ))
                .or_default()
                .push((index, j));
        }
        self.components.push(RegisteredFusionComponent {
            spec: spec.clone(),
            template_names: names,
            templates,
            omit_attached_locants: row["omit_attached_locants"].as_bool().unwrap_or(false),
            omit_ortho_peri_attached_locants: row["omit_ortho_peri_attached_locants"]
                .as_bool()
                .unwrap_or(false),
        });
        self.by_key.insert(key, index);
        Ok(spec)
    }
    pub fn match_face_model(
        &self,
        mol: &Molecule,
        model: &FaceModel,
    ) -> Result<Vec<FusionComponentMatch>, String> {
        self.match_faces(mol, &model.faces, None, None)
    }
    pub fn match_faces(
        &self,
        mol: &Molecule,
        faces: &[Face],
        role: Option<FusionComponentRole>,
        skeleton: Option<&BTreeMap<usize, usize>>,
    ) -> Result<Vec<FusionComponentMatch>, String> {
        if faces.is_empty() {
            return Ok(Vec::new());
        }
        let mut normalized = faces.to_vec();
        normalized.sort_by_key(|f| f.id);
        for f in &normalized {
            for &a in &f.atom_cycle {
                if !mol.atoms.contains_key(&a) {
                    return Err(format!("unknown face atom {a}"));
                }
            }
            for i in 0..f.size {
                if mol
                    .get_bond(f.atom_cycle[i], f.atom_cycle[(i + 1) % f.size])
                    .is_none()
                {
                    return Err("face has nonbonded pair".into());
                }
            }
        }
        let sizes: BTreeSet<_> = self.topology_index.keys().map(|(n, _)| *n).collect();
        let subsets = connected_face_subsets(&normalized, &sizes);
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut explicit = BTreeSet::new();
        for indices in subsets {
            let subset: Vec<_> = indices.iter().map(|&i| &normalized[i]).collect();
            let atoms: AtomSet = subset
                .iter()
                .flat_map(|f| f.atom_cycle.iter().copied())
                .collect();
            let topology = molecule_graph_topology_key(mol, &atoms);
            for &(ci, ti) in self
                .topology_index
                .get(&(subset.len(), topology.clone()))
                .into_iter()
                .flatten()
            {
                let c = &self.components[ci];
                let spec = &c.templates[ti];
                let mut matching = spec.template.clone();
                // The component parent proof can release a carbon H tautomer at fusion.
                matching.atoms = super::indicated_hydrogen::component_parent_atoms(spec)?;
                // A local component has no complete-parent pi-state decision: the merged
                // graph's independent bond-model audit decides it. The shared matcher uses
                // this optional budget only for its retained complete-parent ambiguity gate.
                matching.mancude_double_bonds = None;
                for exact in match_retained_graph_template_maps_with_options(
                    mol, &atoms, &matching, true, false,
                ) {
                    let mapped: BTreeSet<BTreeSet<usize>> = spec
                        .rings
                        .iter()
                        .map(|r| r.iter().map(|l| exact.locant_to_atom[l]).collect())
                        .collect();
                    let expected: BTreeSet<BTreeSet<usize>> = subset
                        .iter()
                        .map(|f| f.atom_cycle.iter().copied().collect())
                        .collect();
                    if mapped != expected {
                        continue;
                    }
                    if subset.len() == 1 {
                        explicit.insert(subset[0].id);
                    }
                    if role.is_some_and(|r| !c.eligible_for(r)) {
                        continue;
                    }
                    let local: Vec<_> = spec
                        .locants()
                        .iter()
                        .map(|l| (l.clone(), exact.locant_to_atom[l]))
                        .collect();
                    let face_ids: BTreeSet<_> = subset.iter().map(|f| f.id).collect();
                    let identity = (
                        spec.key.clone(),
                        spec.template.name.clone(),
                        face_ids.clone(),
                        local.clone(),
                    );
                    if !seen.insert(identity) {
                        continue;
                    }
                    let skeleton_map = local
                        .iter()
                        .map(|(l, a)| {
                            Ok((
                                l.clone(),
                                skeleton
                                    .map_or(Some(*a), |s| s.get(a).copied())
                                    .ok_or("missing skeleton map")?,
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    out.push(FusionComponentMatch {
                        occurrence_id: out.len(),
                        spec_key: spec.key.clone(),
                        covered_face_ids: face_ids,
                        local_to_input_atom: local,
                        local_to_skeleton_atom: skeleton_map,
                        topology_key: topology_key(&topology),
                        template_name: spec.template.name.clone(),
                    });
                }
            }
        }
        if let Some(policy) = &self.generated_hw_policy
            && role.is_none_or(|r| match r {
                FusionComponentRole::Parent => policy.usable_as_parent,
                FusionComponentRole::Attached => policy.usable_as_attached,
            })
        {
            for f in &normalized {
                if explicit.contains(&f.id) {
                    continue;
                }
                for (component, local) in self.generated_for_ring(mol, &f.atom_cycle)? {
                    let spec = &component.spec;
                    let face_ids = BTreeSet::from([f.id]);
                    let identity = (
                        spec.key.clone(),
                        spec.template.name.clone(),
                        face_ids.clone(),
                        local.clone(),
                    );
                    if !seen.insert(identity) {
                        continue;
                    }
                    let topology =
                        molecule_graph_topology_key(mol, &f.atom_cycle.iter().copied().collect());
                    let local_to_skeleton_atom = local
                        .iter()
                        .map(|(l, a)| {
                            Ok((
                                l.clone(),
                                skeleton
                                    .map_or(Some(*a), |s| s.get(a).copied())
                                    .ok_or("missing skeleton map")?,
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    out.push(FusionComponentMatch {
                        occurrence_id: out.len(),
                        spec_key: spec.key.clone(),
                        covered_face_ids: face_ids,
                        local_to_input_atom: local,
                        local_to_skeleton_atom,
                        topology_key: topology_key(&topology),
                        template_name: spec.template.name.clone(),
                    });
                }
            }
        }
        Ok(out)
    }
}
fn topology_key(k: &TopologyKey) -> ComparisonKey {
    ComparisonKey::tuple([
        k.size.into(),
        ComparisonKey::Tuple(
            k.symbols
                .iter()
                .map(|(s, n)| ComparisonKey::tuple([s.clone().into(), (*n).into()]))
                .collect(),
        ),
        ComparisonKey::Tuple(
            k.degrees
                .iter()
                .map(|(s, n)| ComparisonKey::tuple([(*s).into(), (*n).into()]))
                .collect(),
        ),
        ComparisonKey::Tuple(
            k.edge_degrees
                .iter()
                .map(|((a, b), n)| {
                    ComparisonKey::tuple([
                        ComparisonKey::tuple([(*a).into(), (*b).into()]),
                        (*n).into(),
                    ])
                })
                .collect(),
        ),
    ])
}
fn is_benzoheterocycle(s: &FusionComponentSpec) -> bool {
    if !s.template.enabled
        || !s.pin_component
        || s.attached_prefix.is_empty()
        || s.rings.len() != 2
        || s.atoms().iter().any(|a| a.charge != 0 || a.saturated)
        || s.atoms().iter().all(|a| a.symbol == "C")
    {
        return false;
    }
    let a: BTreeSet<_> = s.rings[0].iter().cloned().collect();
    let b: BTreeSet<_> = s.rings[1].iter().cloned().collect();
    let shared: BTreeSet<_> = a.intersection(&b).cloned().collect();
    let atoms = s.template.atom_by_locant();
    shared.len() == 2
        && a.union(&b).cloned().collect::<BTreeSet<_>>() == s.locants().iter().cloned().collect()
        && s.bonds()
            .iter()
            .any(|b| BTreeSet::from([b.locants.0.clone(), b.locants.1.clone()]) == shared)
        && a.len().min(b.len()) >= 5
        && [a, b]
            .iter()
            .any(|r| r.len() == 6 && r.iter().all(|l| atoms[l].symbol == "C"))
}
fn connected_face_subsets(faces: &[Face], sizes: &BTreeSet<usize>) -> Vec<Vec<usize>> {
    let max = sizes.last().copied().unwrap_or(0);
    let edges: Vec<BTreeSet<_>> = faces
        .iter()
        .map(|f| {
            (0..f.size)
                .map(|i| normalize_edge(f.atom_cycle[i], f.atom_cycle[(i + 1) % f.size]))
                .collect()
        })
        .collect();
    let adj: Vec<BTreeSet<_>> = (0..faces.len())
        .map(|i| {
            (0..faces.len())
                .filter(|&j| i != j && !edges[i].is_disjoint(&edges[j]))
                .collect()
        })
        .collect();
    let mut all = BTreeSet::new();
    let mut pending: Vec<_> = (0..faces.len()).map(|i| BTreeSet::from([i])).collect();
    while let Some(s) = pending.pop() {
        if s.len() > max || !all.insert(s.clone()) {
            continue;
        }
        for &i in &s {
            for &j in &adj[i] {
                if !s.contains(&j) {
                    let mut n = s.clone();
                    n.insert(j);
                    pending.push(n)
                }
            }
        }
    }
    {
        let mut result: Vec<Vec<usize>> = all
            .into_iter()
            .filter(|s| sizes.contains(&s.len()))
            .map(|s| s.into_iter().collect())
            .collect();
        result.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
        result
    }
}
#[allow(clippy::expect_used)]
static REGISTRY: LazyLock<FusionComponentRegistry> = LazyLock::new(|| {
    let d: Value = serde_json::from_str(include_str!("../data/fusion_components.json"))
        .expect("checked-in fusion registry");
    let registry =
        FusionComponentRegistry::from_data(&d).expect("valid checked-in fusion component policy");
    let config = super::config::fusion_nomenclature_config();
    assert_eq!(
        registry.version, config.registry_version,
        "fusion registry and policy versions differ"
    );
    assert_eq!(
        config.graph_source, "retained_graph_templates",
        "fusion registry graph source differs"
    );
    registry
});
pub fn fusion_component_registry() -> &'static FusionComponentRegistry {
    &REGISTRY
}
impl FusionComponentRegistry {
    fn generated_from_key(&self, key: &str) -> Option<RegisteredFusionComponent> {
        let symbols: Vec<String> = key
            .strip_prefix("generated-hw:")?
            .split('.')
            .map(str::to_owned)
            .collect();
        if !(3..=10).contains(&symbols.len())
            || symbols.iter().all(|s| s == "C")
            || symbols.iter().any(|s| {
                !super::rules::fusion_supported(s)
                    || s != "C"
                        && super::super::rules::elements::get(s)
                            .is_none_or(|e| e.hw_stem.is_none() || e.hw_priority.is_none())
            })
        {
            return None;
        }
        let policy = self.generated_hw_policy.as_ref()?;
        let hetero: Vec<_> = symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| *s != "C")
            .map(|(i, s)| (i + 1, s.clone()))
            .collect();
        let name = retained::hw_name(symbols.len(), false, &hetero)?;
        let (parent, prefix) = hw_forms(&name);
        let mut spec = monocycle(key, &symbols, "mancude", true);
        for a in &mut spec.template.atoms {
            a.aromatic = a.symbol == "C"
        }
        spec.template.family = "generated_hw_monocycle".into();
        spec.template.enforce_mancude_double_bonds = true;
        spec.template.mancude_double_bonds = Some(
            retained::mancude_bond_orders(&symbols)
                .iter()
                .filter(|&&n| n == 2)
                .count(),
        );
        spec.key = key.into();
        spec.parent_name = parent;
        spec.attached_prefix = prefix;
        spec.usable_as_parent = policy.usable_as_parent;
        spec.usable_as_attached = policy.usable_as_attached;
        spec.rule_reference = policy.rule_reference.clone();
        spec.horizontal_ring_count = 1;
        spec.multiplicative_prefix_style =
            if hetero.len() > 1 { "complex" } else { "basic" }.into();
        Some(RegisteredFusionComponent {
            template_names: vec![key.into()],
            templates: vec![spec.clone()],
            spec,
            omit_attached_locants: false,
            omit_ortho_peri_attached_locants: false,
        })
    }
    fn generated_for_ring(
        &self,
        mol: &Molecule,
        path: &[usize],
    ) -> Result<Vec<GeneratedRingMatch>, String> {
        if !(3..=10).contains(&path.len())
            || path.iter().copied().collect::<AtomSet>().len() != path.len()
        {
            return Ok(Vec::new());
        }
        let mut all_carbon = true;
        for &i in path {
            let Some(a) = mol.atoms.get(&i) else {
                return Ok(Vec::new());
            };
            all_carbon &= a.symbol == "C";
            if !(a.charge == 0 || a.charge == 1 && matches!(a.symbol.as_str(), "N" | "O"))
                || a.isotope.is_some()
                || !super::rules::fusion_supported(&a.symbol)
            {
                return Ok(Vec::new());
            }
            let Some(e) = a.element() else {
                return Ok(Vec::new());
            };
            if a.symbol != "C" && (e.hw_stem.is_none() || e.hw_priority.is_none()) {
                return Ok(Vec::new());
            }
            if a.symbol != "C" && a.charge == 0 {
                let bonding = a.total_h_count
                    + mol
                        .get_neighbors(i)
                        .iter()
                        .filter_map(|&n| {
                            mol.get_bond(i, n)
                                .filter(|b| !(b.order == 2 && mol.atoms[&n].symbol == "O"))
                        })
                        .map(|b| b.order)
                        .sum::<usize>();
                if bonding != e.standard_valence {
                    return Ok(Vec::new());
                }
            }
        }
        if all_carbon {
            return Ok(Vec::new());
        }
        let orders = retained::canonical_ring_orders(mol, path);
        let Some(first) = orders.first() else {
            return Ok(Vec::new());
        };
        let symbols: Vec<_> = first.iter().map(|i| mol.atoms[i].symbol.clone()).collect();
        let expected = retained::mancude_bond_orders(&symbols)
            .iter()
            .filter(|&&n| n == 2)
            .count();
        if expected == 0 {
            return Ok(Vec::new());
        }
        let mut actual = 0;
        let mut unavailable: Vec<_> = path
            .iter()
            .map(|i| {
                mol.atoms[i]
                    .element()
                    .is_some_and(|e| e.mancude_forced_single)
            })
            .collect();
        for p in 0..path.len() {
            let next = (p + 1) % path.len();
            let Some(b) = mol.get_bond(path[p], path[next]) else {
                return Ok(Vec::new());
            };
            if !matches!(b.order, 1 | 2) {
                return Ok(Vec::new());
            }
            if b.order == 2 {
                actual += 1;
                if unavailable[p] || unavailable[next] {
                    return Ok(Vec::new());
                }
                unavailable[p] = true;
                unavailable[next] = true
            }
        }
        if actual > expected || actual + maximum_cycle_matching(&unavailable) != expected {
            return Ok(Vec::new());
        }
        let key = format!("generated-hw:{}", symbols.join("."));
        let Some(component) = self.generated_from_key(&key) else {
            return Ok(Vec::new());
        };
        Ok(orders
            .into_iter()
            .map(|order| {
                (
                    component.clone(),
                    order
                        .into_iter()
                        .enumerate()
                        .map(|(i, a)| ((i + 1).to_string(), a))
                        .collect(),
                )
            })
            .collect())
    }
}
fn maximum_cycle_matching(unavailable: &[bool]) -> usize {
    let Some(blocked) = unavailable.iter().position(|b| *b) else {
        return unavailable.len() / 2;
    };
    let mut result = 0;
    let mut run = 0;
    for offset in 1..=unavailable.len() {
        if !unavailable[(blocked + offset) % unavailable.len()] {
            run += 1
        } else {
            result += run / 2;
            run = 0
        }
    }
    result + run / 2
}
fn hw_forms(name: &str) -> (String, String) {
    let (locants, base) = name
        .split_once('-')
        .filter(|(l, _)| l.split(',').all(|p| p.chars().all(|c| c.is_ascii_digit())))
        .map_or(("", name), |(l, b)| (l, b));
    let prefix = base
        .strip_suffix('e')
        .map_or_else(|| format!("{base}o"), |s| format!("{s}o"));
    if locants.is_empty() {
        (base.into(), prefix)
    } else {
        (format!("[{locants}]{base}"), format!("[{locants}]{prefix}"))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::graph_io::read_smiles;
    use super::super::{
        faces::{select_bounded_face_model, typed_face_model},
        rules::{component_automorphisms, component_spec_seniority_key},
    };
    use super::*;
    #[test]
    fn checked_in_registry_preserves_component_policy_and_exact_symmetries() {
        let r = fusion_component_registry();
        assert_eq!(r.version, "2026.1");
        assert!(r.components().len() > 33);
        let anthracene = r.get("anthracene").unwrap();
        assert_eq!(anthracene.spec.attached_prefix, "anthra");
        assert_eq!(anthracene.spec.accepted_general_prefixes, ["anthraceno"]);
        let benzene = r.get("benzene").unwrap();
        assert_eq!(component_automorphisms(&benzene.spec).len(), 12);
        let pyridine = r.get("pyridine").unwrap();
        assert!(
            component_spec_seniority_key(&pyridine.spec)
                < component_spec_seniority_key(&benzene.spec)
        );
        let naphthalene = r.get("naphthalene").unwrap();
        assert_eq!(
            naphthalene.spec.peripheral_order,
            ["1", "2", "3", "4", "4a", "5", "6", "7", "8", "8a"]
        );
    }
    #[test]
    fn retained_face_matches_keep_all_local_numberings() {
        let mol = read_smiles("c1ccc2ccccc2c1").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
        let model = typed_face_model(&mol, &bounded);
        let matches = fusion_component_registry()
            .match_face_model(&mol, &model)
            .unwrap();
        assert_eq!(
            matches
                .iter()
                .filter(|m| m.spec_key == "naphthalene")
                .count(),
            4
        );
        assert_eq!(
            matches.iter().filter(|m| m.spec_key == "benzene").count(),
            24
        );
    }
    #[test]
    fn generated_hw_is_stateless_and_rejects_incompatible_pi_edges() {
        let r = fusion_component_registry();
        let mol = read_smiles("c1ncncn1").unwrap();
        let f = Face {
            id: 0,
            atom_cycle: vec![0, 1, 2, 3, 4, 5],
            size: 6,
            edge_cycle: Vec::new(),
        };
        let matches = r.match_faces(&mol, &[f], None, None).unwrap();
        assert_eq!(matches.len(), 6);
        for m in &matches {
            let spec = r.spec_for_match(m).unwrap();
            assert_eq!(spec.parent_name, "[1,3,5]triazine");
            assert_eq!(spec.attached_prefix, "[1,3,5]triazino");
            assert_eq!(spec.multiplicative_prefix_style, "complex");
        }
        let rebuilt = r.get(&matches[0].spec_key).unwrap();
        assert_eq!(rebuilt.spec.template.atoms.len(), 6);
    }
}
