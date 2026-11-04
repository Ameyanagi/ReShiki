//! Graph-backed retained and generated fusion-component registry.
use super::super::{
    locants::{LocantKey, retained_locant_sort_key as locant_key},
    molecule::{AtomSet, Molecule},
    nomenclature::section,
    retained_fused_templates::{
        TopologyKey, match_retained_graph_template_maps_with_options,
        match_retained_graph_template_topology_maps, molecule_graph_topology_key,
        retained_graph_template_topology_key, retained_graph_templates,
    },
    retained_graph_model::{
        RetainedGraphAtomTemplate, RetainedGraphBondTemplate, RetainedGraphTemplate,
    },
    retained_name_policy::retained_parent_name_policy,
    rules::{retained, stems},
};
use super::{config::annulene_ring_sizes_from_data, model::*, rules::component_spec_seniority_key};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};
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
}
#[derive(Clone, Debug)]
pub struct FusionComponentRegistry {
    pub version: String,
    components: Vec<RegisteredFusionComponent>,
    by_key: BTreeMap<String, usize>,
    template_by_name: BTreeMap<String, FusionComponentSpec>,
    topology_index: BTreeMap<(usize, TopologyKey), Vec<(usize, usize)>>,
    generated_hw_policy: Option<GeneratedHWPolicy>,
    retained_component_status: Vec<FusionComponentSpec>,
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
            if let Some(neighbors) = adj.get_mut(a) {
                neighbors.insert(c.clone());
            }
            if let Some(neighbors) = adj.get_mut(c) {
                neighbors.insert(a.clone());
            }
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
static CONSTRUCTION_ORDERS: LazyLock<BTreeMap<String, FusionComponentConstructionOrder>> =
    LazyLock::new(|| {
        let d: Value =
            serde_json::from_str(include_str!("../data/fusion_component_construction.json"))
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
                        directed_bond_locants: row["directed_bond_locants"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|e| {
                                (
                                    e[0].as_str().unwrap_or_default().into(),
                                    e[1].as_str().unwrap_or_default().into(),
                                )
                            })
                            .collect(),
                        source: txt(&d, "source"),
                        version: txt(&d, "version"),
                    },
                )
            })
            .collect()
    });
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
        if let Some(&i) = self.by_key.get(&m.spec_key) {
            return self.components[i].spec_for_template(&m.template_name);
        }
        let c = self
            .generated_from_key(&m.spec_key)
            .ok_or_else(|| format!("unknown fusion component {}", m.spec_key))?;
        let mut spec = c.spec_for_template(&m.template_name)?;
        if m.standalone_benzo_parent
            && let Some(name) = &spec.benzo_parent_name
        {
            spec.parent_name = name.render(&[]);
        }
        Ok(spec)
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
            retained_component_status: Vec::new(),
        };
        for t in retained_graph_templates(true) {
            if let Some(row) = rows.get(&t.name) {
                registry
                    .template_by_name
                    .insert(t.name.clone(), metadata(t.clone(), row));
            }
        }
        for row in data["retained_component_status"]["components"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let name = txt(row, "template_name");
            let mut spec = registry
                .template_by_name
                .get(&name)
                .cloned()
                .ok_or_else(|| format!("unknown retained status template {name}"))?;
            spec.horizontal_ring_count = row["horizontal_ring_count"]
                .as_u64()
                .ok_or_else(|| format!("missing horizontal ring count for {name}"))?
                as usize;
            registry.retained_component_status.push(spec);
        }
        // P-22.2.1, Table 2.2: the Se/Te analogues retain pyran numbering
        // and indicated hydrogen, but have no standalone graph templates yet.
        for (name, symbol) in [("selenopyran", "Se"), ("telluropyran", "Te")] {
            let mut spec = registry.template_by_name["thiopyran"].clone();
            spec.template.name = name.into();
            spec.parent_name = name.into();
            for atom in &mut spec.template.atoms {
                if atom.symbol == "S" {
                    atom.symbol = symbol.into();
                }
            }
            registry.template_by_name.insert(name.into(), spec);
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
            registry.register(&json!({"key":name,"parent_name":name,"attached_prefix":format!("cyclo{stem}a"),"allow_as_parent":true,"allow_as_attached":true,"omit_attached_locants":true}))?;
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
            for template in retained_graph_templates(true) {
                let name = &template.name;
                let Some(s) = registry.template_by_name.get(name) else {
                    continue;
                };
                if registry
                    .components
                    .iter()
                    .any(|c| c.template_names.contains(name))
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
                let parent = retained_parent_name_policy(name)
                    .map(|p| p.output_name("fusion_component"))
                    .unwrap_or_else(|| s.template.output_name());
                let parent = parent.strip_prefix(&h).unwrap_or(&parent).to_owned();
                let ring_count = s.rings.len();
                // Retained trivial bicycles participate directly in FR-3.1.
                // Only FR-2.2.8 benzo components inherit the deferred-parent
                // policy; otherwise a junior explicit component can win before
                // the intrinsic seniority comparison sees the retained parent.
                let allow_as_parent = is_trivial_benzo_name(&parent)
                    || policy["allow_as_parent"].as_bool().unwrap_or(false);
                registry.register(&json!({
                    "key": name,
                    "parent_name": parent,
                    "allow_as_parent": allow_as_parent,
                    "allow_as_peri_parent": policy["allow_as_peri_parent"],
                    "allow_as_attached": policy["allow_as_attached"],
                    "horizontal_ring_count": ring_count,
                }))?;
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
        let construction = &*CONSTRUCTION_ORDERS;
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
            s.benzoheterocycle_component =
                is_benzoheterocycle(&s) && !is_trivial_benzo_name(&s.parent_name);
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
        self.match_faces(mol, &model.faces, None)
    }
    /// P-25.3.2.4: assess the rendered parent without adding planning candidates.
    pub fn has_senior_unregistered_retained_component(
        &self,
        mol: &Molecule,
        plan: &FusionParentPlan,
    ) -> bool {
        let Some(parent_key) = plan
            .ast
            .component_occurrences
            .iter()
            .filter(|m| plan.ast.parent_occurrences.contains(&m.occurrence_id))
            .filter_map(|m| self.spec_for_match(m).ok())
            .map(|s| component_spec_seniority_key(&s))
            .min()
        else {
            return false;
        };
        let faces: Vec<_> = plan.numbering.selected_face_model.faces.iter().collect();
        let candidates: Vec<_> = self
            .retained_component_status
            .iter()
            .filter(|s| {
                // A complete retained parent is not a bypassed fusion component.
                s.rings.len() < faces.len()
                    && component_spec_seniority_key(s) < parent_key
                    && !self
                        .components
                        .iter()
                        .any(|c| c.template_names.contains(&s.template.name))
            })
            .collect();
        let sizes = candidates.iter().map(|s| s.rings.len()).collect();
        for indices in connected_face_subsets(&faces, &sizes) {
            let subset: Vec<_> = indices.iter().map(|&i| faces[i]).collect();
            let atoms = subset
                .iter()
                .flat_map(|f| f.atom_cycle.iter().copied())
                .collect();
            let topology = molecule_graph_topology_key(mol, &atoms);
            let expected: BTreeSet<AtomSet> = subset
                .iter()
                .map(|f| f.atom_cycle.iter().copied().collect())
                .collect();
            for spec in &candidates {
                if spec.rings.len() != subset.len()
                    || retained_graph_template_topology_key(&spec.template) != topology
                {
                    continue;
                }
                for map in match_retained_graph_template_topology_maps(mol, &atoms, &spec.template)
                {
                    let mapped: BTreeSet<AtomSet> = spec
                        .rings
                        .iter()
                        .map(|r| r.iter().map(|l| map[l]).collect())
                        .collect();
                    if mapped == expected {
                        return true;
                    }
                }
            }
        }
        false
    }
    pub fn match_faces(
        &self,
        mol: &Molecule,
        faces: &[Face],
        skeleton: Option<&BTreeMap<usize, usize>>,
    ) -> Result<Vec<FusionComponentMatch>, String> {
        if faces.is_empty() {
            return Ok(Vec::new());
        }
        let mut normalized: Vec<_> = faces.iter().collect();
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
            let subset: Vec<_> = indices.iter().map(|&i| normalized[i]).collect();
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
                        standalone_benzo_parent: false,
                    });
                }
            }
        }
        if self.generated_hw_policy.is_some() {
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
                        standalone_benzo_parent: false,
                    });
                }
            }
        }
        Ok(out)
    }
    fn generated_from_key(&self, key: &str) -> Option<RegisteredFusionComponent> {
        if key.starts_with("generated-benzo:") {
            return self.generated_benzo_from_key(key);
        }
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
        // Retained pyran components must not fall back to HW oxine/thiine
        // names when their explicit component match fails.
        if symbols.len() == 6
            && hetero.len() == 1
            && matches!(hetero[0].1.as_str(), "O" | "S" | "Se" | "Te")
        {
            return None;
        }
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
    /// FR-3.5 combines already selected components, rather than adding benzo
    /// bicycles to the component-cover search. The key keeps the source template
    /// and its oriented ring, so this operation never mutates the registry.
    pub(super) fn benzoheterocycle_matches(
        &self,
        benzo: &FusionComponentMatch,
        hetero: &FusionComponentMatch,
    ) -> Result<Vec<FusionComponentMatch>, String> {
        let benzene = self.spec_for_match(benzo)?;
        let monocycle = self.spec_for_match(hetero)?;
        if benzene.rings.len() != 1
            || benzene.locants().len() != 6
            || benzene.atoms().iter().any(|a| a.symbol != "C")
            || monocycle.rings.len() != 1
            || monocycle.locants().len() < 5
            || monocycle.atoms().iter().all(|a| a.symbol == "C")
            || !benzo.covered_face_ids.is_disjoint(&hetero.covered_face_ids)
        {
            return Ok(Vec::new());
        }
        let benzo_map = benzo.input_atom_by_locant();
        let hetero_map = hetero.input_atom_by_locant();
        let shared: AtomSet = benzo_map
            .values()
            .filter(|a| hetero_map.values().any(|b| *a == b))
            .copied()
            .collect();
        if shared.len() != 2 {
            return Ok(Vec::new());
        }
        let hetero_atoms = monocycle.template.atom_by_locant();
        if hetero_map
            .iter()
            .any(|(l, a)| shared.contains(a) && hetero_atoms[l].symbol != "C")
        {
            return Ok(Vec::new());
        }
        let benzo_ring = &benzene.rings[0];
        let hetero_ring = &monocycle.rings[0];
        let hetero_locant_by_input: BTreeMap<_, _> =
            hetero_map.iter().map(|(l, a)| (*a, l.clone())).collect();
        let mut skeleton_by_input = BTreeMap::new();
        for matched in [benzo, hetero] {
            let skeleton = matched.skeleton_atom_by_locant();
            for (l, a) in &matched.local_to_input_atom {
                let s = *skeleton.get(l).ok_or("missing benzo skeleton map")?;
                if skeleton_by_input.insert(*a, s).is_some_and(|old| old != s) {
                    return Err("inconsistent benzo skeleton maps".into());
                }
            }
        }
        let junctions: Vec<_> = shared.into_iter().collect();
        let mut candidates = Vec::new();
        for (junction_from, junction_to) in
            [(junctions[0], junctions[1]), (junctions[1], junctions[0])]
        {
            let Some(mut path) =
                ring_path_without_edge(hetero_ring, &hetero_map, junction_to, junction_from)
            else {
                continue;
            };
            let Some(benzo_path) =
                ring_path_without_edge(benzo_ring, &benzo_map, junction_from, junction_to)
            else {
                continue;
            };
            // Non-fusion heterocyclic atoms, then the two fusion carbons.
            path.rotate_left(1);
            let ring: Vec<_> = path
                .iter()
                .map(|a| hetero_locant_by_input[a].clone())
                .collect();
            let n = ring.len();
            let canonical_ring = canonical_benzo_source_ring(&monocycle, &ring);
            let encoded =
                serde_json::to_string(&(&hetero.spec_key, &hetero.template_name, canonical_ring))
                    .map_err(|e| e.to_string())?;
            let key = format!("generated-benzo:{encoded}");
            let Some(component) = self.generated_benzo_from_key(&key) else {
                continue;
            };
            let spec = &component.spec;
            let mut input = BTreeMap::new();
            for (l, source) in benzo_locants(n)[..n - 1].iter().zip(&ring[..n - 1]) {
                input.insert(l.clone(), hetero_map[source]);
            }
            let locants = benzo_locants(n);
            input.insert(locants.last().cloned().unwrap_or_default(), junction_to);
            for (l, a) in locants[n - 1..n + 3].iter().zip(&benzo_path[1..5]) {
                input.insert(l.clone(), *a);
            }
            let local_to_input_atom: Vec<_> = spec
                .locants()
                .iter()
                .map(|l| (l.clone(), input[l]))
                .collect();
            let local_to_skeleton_atom = local_to_input_atom
                .iter()
                .map(|(l, a)| (l.clone(), skeleton_by_input[a]))
                .collect();
            let matched = FusionComponentMatch {
                occurrence_id: hetero.occurrence_id,
                spec_key: key.clone(),
                covered_face_ids: benzo
                    .covered_face_ids
                    .union(&hetero.covered_face_ids)
                    .copied()
                    .collect(),
                local_to_input_atom,
                local_to_skeleton_atom,
                topology_key: topology_key(&retained_graph_template_topology_key(&spec.template)),
                template_name: key,
                standalone_benzo_parent: false,
            };
            candidates.push((benzo_numbering_key(spec), matched));
        }
        let best = candidates.iter().map(|(k, _)| k).min().cloned();
        let mut matches = Vec::new();
        for (numbering, matched) in candidates {
            if Some(&numbering) == best.as_ref() && !matches.contains(&matched) {
                matches.push(matched);
            }
        }
        Ok(matches)
    }
    pub(super) fn benzoheterocycle_parent_matches(
        &self,
        benzo: &FusionComponentMatch,
        hetero: &FusionComponentMatch,
    ) -> Result<Vec<FusionComponentMatch>, String> {
        let mut parents = Vec::new();
        // A retained bicycle such as quinoline keeps its retained name here.
        for mut matched in self.benzoheterocycle_matches(benzo, hetero)? {
            matched.standalone_benzo_parent = true;
            parents.push(matched);
        }
        Ok(parents)
    }
    fn generated_benzo_from_key(&self, key: &str) -> Option<RegisteredFusionComponent> {
        let (source_key, source_template, ring): (String, String, Vec<String>) =
            serde_json::from_str(key.strip_prefix("generated-benzo:")?).ok()?;
        // Benzo components cannot recursively be used as heteromonocycles.
        if source_key.starts_with("generated-benzo:") {
            return None;
        }
        let source = self
            .get(&source_key)?
            .spec_for_template(&source_template)
            .ok()?;
        let n = ring.len();
        if source.rings.len() != 1
            || n < 5
            || ring.iter().collect::<BTreeSet<_>>() != source.locants().iter().collect()
            || ring.len() != source.locants().len()
        {
            return None;
        }
        let source_atoms = source.template.atom_by_locant();
        if source_atoms[&ring[n - 2]].symbol != "C"
            || source_atoms[&ring[n - 1]].symbol != "C"
            || source_atoms.values().all(|a| a.symbol == "C")
        {
            return None;
        }
        let source_bonds: BTreeMap<_, _> = source
            .bonds()
            .iter()
            .map(|b| (ordered_locant_edge(&b.locants.0, &b.locants.1), b))
            .collect();
        if (0..n)
            .any(|i| !source_bonds.contains_key(&ordered_locant_edge(&ring[i], &ring[(i + 1) % n])))
        {
            return None;
        }
        let locants = benzo_locants(n);
        let renamed: BTreeMap<_, _> = ring
            .iter()
            .zip(locants[..n - 1].iter().chain(locants.last()))
            .map(|(old, new)| (old.clone(), new.clone()))
            .collect();
        let mut template = source.template.clone();
        template.name = key.into();
        template.family = "generated_benzoheterocycle".into();
        template.locants = locants.clone();
        template.default_indicated_h = source
            .template
            .default_indicated_h
            .iter()
            .map(|l| renamed[l].clone())
            .collect();
        template.fusion_atoms = vec![locants[n - 2].clone(), locants[n + 3].clone()];
        template.atoms = source
            .atoms()
            .iter()
            .map(|a| {
                let mut a = a.clone();
                a.locant = renamed[&a.locant].clone();
                a.fusion = template.fusion_atoms.contains(&a.locant);
                a
            })
            .collect();
        let benzene = self.get("benzene")?.spec;
        let carbon = benzene.atoms().first()?;
        for l in &locants[n - 1..n + 3] {
            let mut atom = carbon.clone();
            atom.locant = l.clone();
            atom.fusion = false;
            template.atoms.push(atom);
        }
        template.atoms.sort_by_key(|a| locant_key(&a.locant));
        template.bonds = source
            .bonds()
            .iter()
            .map(|b| RetainedGraphBondTemplate {
                locants: (renamed[&b.locants.0].clone(), renamed[&b.locants.1].clone()),
                bond_class: if template.fusion_atoms.contains(&renamed[&b.locants.0])
                    && template.fusion_atoms.contains(&renamed[&b.locants.1])
                {
                    "fusion".into()
                } else {
                    b.bond_class.clone()
                },
            })
            .collect();
        for edge in locants[n - 2..].windows(2) {
            template.bonds.push(RetainedGraphBondTemplate {
                locants: (edge[0].clone(), edge[1].clone()),
                bond_class: benzene.bonds().first()?.bond_class.clone(),
            });
        }
        template.mancude_double_bonds = source.template.mancude_double_bonds.map(|n| n + 2);
        let hetero_ring = ring.iter().map(|l| renamed[l].clone()).collect();
        let benzene_ring = locants[n - 2..].to_vec();
        let mut spec = FusionComponentSpec {
            key: key.into(),
            template,
            rings: vec![hetero_ring, benzene_ring],
            peripheral_order: locants,
            usable_as_parent: true,
            usable_as_attached: true,
            horizontal_ring_count: 2,
            multiplicative_prefix_style: "complex".into(),
            pin_component: true,
            ..Default::default()
        };
        (
            spec.parent_name,
            spec.attached_prefix,
            spec.benzo_parent_name,
        ) = benzo_forms(&source, &spec)?;
        // Preserve retained trivial names and the declared attached stems of
        // matching preferred benzo components (for example, benzimidazolo).
        if let Some(retained) = self.components.iter().flat_map(|c| &c.templates).find(|s| {
            s.rings.len() == 2
                && s.usable_as_attached
                && (is_trivial_benzo_name(&s.parent_name) || s.parent_name == spec.parent_name)
                && same_numbered_component_graph(s, &spec)
        }) {
            spec.parent_name = retained.parent_name.clone();
            spec.attached_prefix = retained.attached_prefix.clone();
            if is_trivial_benzo_name(&retained.parent_name) {
                spec.benzo_parent_name = None;
            }
        }
        spec.benzoheterocycle_component = !is_trivial_benzo_name(&spec.parent_name);
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
        if actual > expected {
            return Ok(Vec::new());
        }
        if actual + maximum_cycle_matching(&unavailable) != expected {
            // Fusion can pair the remaining pi sites in adjoining components;
            // the completed-parent audit still proves the merged pi state.
            let external = path
                .iter()
                .enumerate()
                .filter(|&(index, atom)| {
                    !unavailable[index]
                        && mol.get_neighbors(*atom).iter().any(|neighbor| {
                            !path.contains(neighbor)
                                && mol
                                    .get_bond(*atom, *neighbor)
                                    .is_some_and(|bond| bond.order == 2)
                        })
                })
                .count();
            if 2 * actual + external < 2 * expected {
                return Ok(Vec::new());
            }
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
        || s.atoms().iter().any(|a| {
            a.charge != 0
                || a.saturated
                    && !(a.symbol == "C" && s.template.default_indicated_h.contains(&a.locant))
        })
        || s.atoms().iter().all(|a| a.symbol == "C")
    {
        return false;
    }
    let a: BTreeSet<_> = s.rings[0].iter().collect();
    let b: BTreeSet<_> = s.rings[1].iter().collect();
    let shared: BTreeSet<_> = a.intersection(&b).copied().collect();
    let atoms: BTreeMap<_, _> = s.atoms().iter().map(|a| (&a.locant, a)).collect();
    shared.len() == 2
        && a.union(&b).copied().collect::<BTreeSet<_>>() == s.locants().iter().collect()
        && s.bonds()
            .iter()
            .any(|b| BTreeSet::from([&b.locants.0, &b.locants.1]) == shared)
        && a.len().min(b.len()) >= 5
        && [a, b]
            .iter()
            .any(|r| r.len() == 6 && r.iter().all(|l| atoms[l].symbol == "C"))
}
fn connected_face_subsets(faces: &[&Face], sizes: &BTreeSet<usize>) -> Vec<Vec<usize>> {
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
    let mut result: Vec<Vec<usize>> = all
        .into_iter()
        .filter(|s| sizes.contains(&s.len()))
        .map(|s| s.into_iter().collect())
        .collect();
    result.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
    result
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
    let prefix = format!("{}o", base.strip_suffix('e').unwrap_or(base));
    if locants.is_empty() {
        (base.into(), prefix)
    } else {
        (format!("[{locants}]{base}"), format!("[{locants}]{prefix}"))
    }
}

fn ordered_locant_edge(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.into(), b.into())
    } else {
        (b.into(), a.into())
    }
}
fn benzo_locants(size: usize) -> Vec<String> {
    (1..=size - 2)
        .map(|i| i.to_string())
        .chain([format!("{}a", size - 2)])
        .chain((size - 1..=size + 2).map(|i| i.to_string()))
        .chain([format!("{}a", size + 2)])
        .collect()
}
fn ring_path_without_edge(
    ring: &[String],
    atoms: &BTreeMap<String, usize>,
    from: usize,
    to: usize,
) -> Option<Vec<usize>> {
    let start = ring.iter().position(|l| atoms[l] == from)?;
    for reverse in [false, true] {
        let path: Vec<_> = (0..ring.len())
            .map(|i| {
                atoms[&ring[if reverse {
                    (start + ring.len() - i) % ring.len()
                } else {
                    (start + i) % ring.len()
                }]]
            })
            .collect();
        if path.last() == Some(&to) {
            return Some(path);
        }
    }
    None
}
fn canonical_benzo_source_ring(spec: &FusionComponentSpec, ring: &[String]) -> Vec<String> {
    let atoms = spec.template.atom_by_locant();
    let defaults: BTreeSet<_> = spec.template.default_indicated_h.iter().cloned().collect();
    super::rules::component_automorphisms(spec)
        .into_iter()
        .filter(|map| {
            // The general automorphism helper's roles omit some optional H/pi
            // annotations; a generated key must preserve those too.
            atoms.iter().all(|(l, atom)| {
                let mut moved = atom.clone();
                moved.locant = map[l].clone();
                moved == atoms[&map[l]]
            }) && defaults
                .iter()
                .map(|l| map[l].clone())
                .collect::<BTreeSet<_>>()
                == defaults
        })
        .map(|map| ring.iter().map(|l| map[l].clone()).collect::<Vec<_>>())
        .min_by_key(|order| order.iter().map(|l| locant_key(l)).collect::<Vec<_>>())
        .unwrap_or_else(|| ring.to_vec())
}
fn benzo_numbering_key(spec: &FusionComponentSpec) -> Vec<Vec<LocantKey>> {
    // FR-5.3 excludes fusion carbons from the integer sequence. For these
    // ortho-bicycles their locants are fixed, so FR-5.4(a), (b), and (f) decide
    // between the two directions along the heteromonocyclic perimeter.
    let mut hetero = Vec::new();
    let mut by_element: BTreeMap<usize, Vec<LocantKey>> = BTreeMap::new();
    let mut indicated = Vec::new();
    for a in spec.atoms() {
        let number = locant_key(&a.locant);
        if a.symbol != "C" {
            hetero.push(number.clone());
            by_element
                .entry(
                    super::super::rules::elements::get(&a.symbol)
                        .and_then(|e| e.hw_priority)
                        .unwrap_or(usize::MAX),
                )
                .or_default()
                .push(number.clone());
        }
        if spec.template.default_indicated_h.contains(&a.locant)
            || a.default_h
            || !a.aromatic && a.resolved_pi_capacity() != 0 && !a.fusion
        {
            indicated.push(number);
        }
    }
    hetero.sort_unstable();
    let mut result = vec![hetero];
    for locants in by_element.values_mut() {
        locants.sort_unstable();
        result.push(locants.clone());
    }
    indicated.sort_unstable();
    result.push(indicated);
    result
}
fn benzo_forms(
    source: &FusionComponentSpec,
    fused: &FusionComponentSpec,
) -> Option<(String, String, Option<BenzoParentName>)> {
    let trivial = matches!(
        source.template.name.as_str(),
        "furan"
            | "thiophene"
            | "selenophene"
            | "tellurophene"
            | "pyrrole"
            | "pyrazole"
            | "imidazole"
            | "pyridine"
            | "pyridazine"
            | "pyrimidine"
            | "pyrazine"
            | "pyran"
            | "thiopyran"
            | "selenopyran"
            | "telluropyran"
    );
    let (base, prefix) = if trivial {
        (source.parent_name.clone(), source.attached_prefix.clone())
    } else {
        let hetero: Vec<_> = source.rings[0]
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let a = source.atoms().iter().find(|a| a.locant == *l)?;
                (a.symbol != "C").then(|| (i + 1, a.symbol.clone()))
            })
            .collect();
        let systematic = retained::hw_name(source.locants().len(), false, &hetero)?;
        let (_, base) = systematic.split_once('-').unwrap_or(("", &systematic));
        hw_forms(base)
    };
    let join = |word: &str| {
        let benzo = if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "benz"
        } else {
            "benzo"
        };
        format!("{benzo}{word}")
    };
    let mut hetero: Vec<_> = fused.atoms().iter().filter(|a| a.symbol != "C").collect();
    hetero.sort_by_key(|a| {
        (
            super::super::rules::elements::get(&a.symbol).and_then(|e| e.hw_priority),
            locant_key(&a.locant),
        )
    });
    let heteroatom_locants: Vec<_> = hetero.iter().map(|a| a.locant.clone()).collect();
    let locants = heteroatom_locants.join(",");
    // P-25.2.2.4 cites whole-bicycle locants before the standalone name;
    // FR-2.2.8 encloses them when the bicycle is a fusion component.
    Some((
        format!("[{locants}]{}", join(&base)),
        format!("[{locants}]{}", join(&prefix)),
        Some(BenzoParentName {
            heteroatom_locants,
            stem: join(&base),
        }),
    ))
}
fn is_trivial_benzo_name(name: &str) -> bool {
    !name.contains("benz")
}
fn same_numbered_component_graph(a: &FusionComponentSpec, b: &FusionComponentSpec) -> bool {
    let atoms = |s: &FusionComponentSpec| {
        s.atoms()
            .iter()
            .map(|a| (a.locant.clone(), a.symbol.clone(), a.charge))
            .collect::<BTreeSet<_>>()
    };
    let bonds = |s: &FusionComponentSpec| {
        s.bonds()
            .iter()
            .map(|b| ordered_locant_edge(&b.locants.0, &b.locants.1))
            .collect::<BTreeSet<_>>()
    };
    atoms(a) == atoms(b) && bonds(a) == bonds(b)
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
    fn unregistered_retained_status_requires_exact_faces_and_template_identity() {
        let mut mol = read_smiles("c1ccc2c(c1)Cc1c(ccc3ccccc13)S2").unwrap();
        let atoms = mol.atoms.keys().copied().collect();
        let mut plan = super::super::planner::plan_fusion_parent(&mol, &atoms)
            .unwrap()
            .unwrap();
        let mut registry = fusion_component_registry().clone();
        assert!(registry.has_senior_unregistered_retained_component(&mol, &plan));

        // Bond orders and retained hydrogenation are irrelevant to this proof.
        for order in [1, 2] {
            for bond in mol.bonds.values_mut() {
                bond.order = order;
            }
            assert!(registry.has_senior_unregistered_retained_component(&mol, &plan));
        }

        // Preserve the element inventory and degrees, but move S off the middle ring.
        let sulfur = *mol.atoms.iter().find(|(_, a)| a.symbol == "S").unwrap().0;
        let carbon = *mol
            .atoms
            .iter()
            .find(|(id, a)| a.symbol == "C" && a.is_aromatic && mol.get_neighbors(**id).len() == 2)
            .unwrap()
            .0;
        mol.atoms.get_mut(&sulfur).unwrap().symbol = "C".into();
        mol.atoms.get_mut(&carbon).unwrap().symbol = "S".into();
        assert!(!registry.has_senior_unregistered_retained_component(&mol, &plan));
        mol.atoms.get_mut(&sulfur).unwrap().symbol = "S".into();
        mol.atoms.get_mut(&carbon).unwrap().symbol = "C".into();
        assert!(registry.has_senior_unregistered_retained_component(&mol, &plan));

        // The same atom union alone is insufficient: selected faces must match.
        let faces = &mut plan.numbering.selected_face_model.faces;
        let original = faces.clone();
        for face in faces {
            face.atom_cycle = atoms.iter().copied().collect();
            face.size = atoms.len();
        }
        assert!(!registry.has_senior_unregistered_retained_component(&mol, &plan));
        plan.numbering.selected_face_model.faces = original;

        // Even an exact atom graph must map the template rings to these faces.
        let mut wrong_rings = registry.clone();
        wrong_rings
            .retained_component_status
            .iter_mut()
            .find(|s| s.template.name == "9H-thioxanthene")
            .unwrap()
            .rings[0][0] = "9".into();
        assert!(!wrong_rings.has_senior_unregistered_retained_component(&mol, &plan));

        // Display names do not register a retained template.
        registry.components[0].spec.parent_name = "9H-thioxanthene".into();
        assert!(registry.has_senior_unregistered_retained_component(&mol, &plan));
        registry.components[0]
            .template_names
            .push("9H-thioxanthene".into());
        assert!(!registry.has_senior_unregistered_retained_component(&mol, &plan));
    }
    #[test]
    fn unregistered_retained_status_requires_a_strictly_senior_proper_component() {
        let registry = fusion_component_registry();
        for (smiles, parent, expected) in [
            ("c1ccc2c(c1)Cc1c(ccc3ccccc13)O2", "pyran", true),
            ("c1ccc2c(c1)Cc1cc3ncccc3cc1O2", "quinoline", false),
            ("c1ccc2c(c1)Cc1ccccc1O2", "pyran", false),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let atoms = mol.atoms.keys().copied().collect();
            let bounded = select_bounded_face_model(&mol, &atoms).unwrap().unwrap();
            let mut plan = FusionParentPlan::default();
            plan.numbering.selected_face_model = typed_face_model(&mol, &bounded);
            plan.ast.parent_occurrences = vec![0];
            plan.ast.component_occurrences = vec![FusionComponentMatch {
                spec_key: parent.into(),
                ..Default::default()
            }];
            assert_eq!(
                registry.has_senior_unregistered_retained_component(&mol, &plan),
                expected,
                "{smiles}"
            );
            if expected {
                let mut equal = registry.clone();
                let index = equal.by_key[parent];
                equal.components[index].spec = registry.template_spec("xanthene").unwrap().clone();
                equal.components[index].spec.horizontal_ring_count = 3;
                assert!(!equal.has_senior_unregistered_retained_component(&mol, &plan));
            }
        }
    }
    #[test]
    fn retained_tricycles_preserve_numbering_and_seniority() {
        let r = fusion_component_registry();
        for (key, prefix, nitrogen, horizontal, perimeter) in [
            (
                "acridine",
                "acridino",
                "10",
                3,
                [
                    "1", "2", "3", "4", "4a", "10", "10a", "5", "6", "7", "8", "8a", "9", "9a",
                ],
            ),
            (
                "phenanthridine",
                "phenanthridino",
                "5",
                2,
                [
                    "1", "2", "3", "4", "4a", "5", "6", "6a", "7", "8", "9", "10", "10a", "10b",
                ],
            ),
        ] {
            let spec = r.get(key).unwrap().spec;
            assert!(spec.usable_as_parent && spec.usable_as_attached);
            assert_eq!(spec.attached_prefix, prefix);
            assert_eq!(spec.rings.len(), 3);
            assert_eq!(spec.horizontal_ring_count, horizontal);
            assert_eq!(spec.template.atom_by_locant()[nitrogen].symbol, "N");
            assert_eq!(spec.peripheral_order, perimeter);
            let sides = super::super::descriptor::component_sides(&spec).unwrap();
            assert_eq!(sides[1].letter, "b");
            assert_eq!(sides[1].start_locant, "2");
            assert_eq!(sides[1].end_locant, "3");
        }
        assert!(
            component_spec_seniority_key(&r.get("acridine").unwrap().spec)
                < component_spec_seniority_key(&r.get("phenanthridine").unwrap().spec)
        );
        // P-25.2.1, Table 2.8: these retained isomers compete directly with
        // acridine, preserving phenanthroline numbering and angular geometry.
        for locants in [
            "1,7", "1,8", "1,9", "1,10", "2,7", "2,8", "2,9", "3,7", "3,8", "4,7",
        ] {
            let spec = r.get(&format!("{locants}-phenanthroline")).unwrap().spec;
            assert!(spec.usable_as_parent && spec.usable_as_attached);
            assert_eq!(spec.parent_name, format!("[{locants}]phenanthroline"));
            assert_eq!(spec.attached_prefix, format!("[{locants}]phenanthrolino"));
            assert_eq!(spec.rings.len(), 3);
            assert_eq!(spec.horizontal_ring_count, 2);
            assert_eq!(
                spec.peripheral_order,
                [
                    "1", "2", "3", "4", "4a", "5", "6", "6a", "7", "8", "9", "10", "10a", "10b",
                ]
            );
            let sides = super::super::descriptor::component_sides(&spec).unwrap();
            for (index, letter, start, end) in [(1, "b", "2", "3"), (9, "j", "8", "9")] {
                assert_eq!(sides[index].letter, letter);
                assert_eq!(sides[index].start_locant, start);
                assert_eq!(sides[index].end_locant, end);
            }
            assert!(spec.template.default_indicated_h.is_empty());
            assert_eq!(
                spec.template
                    .atoms
                    .iter()
                    .filter(|atom| atom.symbol == "N")
                    .map(|atom| atom.locant.as_str())
                    .collect::<Vec<_>>(),
                locants.split(',').collect::<Vec<_>>()
            );
            assert!(
                component_spec_seniority_key(&spec)
                    < component_spec_seniority_key(&r.get("acridine").unwrap().spec)
            );
        }
    }
    #[test]
    fn retained_pyrans_exclude_generated_hw_citations() {
        let r = fusion_component_registry();
        for (symbol, name) in [
            ("O", "pyran"),
            ("S", "thiopyran"),
            ("Se", "selenopyran"),
            ("Te", "telluropyran"),
        ] {
            let spec = r.get(name).unwrap().spec;
            assert!(spec.usable_as_parent && spec.usable_as_attached);
            assert_eq!(spec.parent_name, name);
            assert_eq!(spec.attached_prefix, format!("{name}o"));
            assert_eq!(spec.template.atom_by_locant()["1"].symbol, symbol);
            assert_eq!(spec.template.default_indicated_h, ["2"]);
            assert_eq!(spec.template.mancude_double_bonds, Some(2));
            for position in 0..6 {
                let mut symbols = ["C"; 6];
                symbols[position] = symbol;
                assert!(
                    r.get(&format!("generated-hw:{}", symbols.join(".")))
                        .is_none()
                );
            }
            for smiles in [
                format!("[{symbol}]1CC=CC=C1"),
                format!("[{symbol}]1C=CCC=C1"),
            ] {
                let mol = read_smiles(&smiles).unwrap();
                let face = Face {
                    id: 0,
                    atom_cycle: (0..6).collect(),
                    size: 6,
                    edge_cycle: Vec::new(),
                };
                let matches = r.match_faces(&mol, &[face], None).unwrap();
                assert!(!matches.is_empty(), "{smiles}");
                assert!(matches.iter().all(|m| m.spec_key == name), "{smiles}");
            }
        }
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
        let matches = r.match_faces(&mol, &[f], None).unwrap();
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
    fn benzo_test_pair(
        source: &str,
        edge: [&str; 2],
    ) -> (FusionComponentMatch, FusionComponentMatch) {
        let r = fusion_component_registry();
        let hetero = r.get(source).unwrap().spec;
        let input: BTreeMap<_, _> = hetero
            .locants()
            .iter()
            .enumerate()
            .map(|(i, l)| (l.clone(), i))
            .collect();
        let benzene = r.get("benzene").unwrap().spec;
        let benzo_input = [input[edge[0]], input[edge[1]], 100, 101, 102, 103];
        let make = |spec: &FusionComponentSpec, face, mapping: Vec<(String, usize)>| {
            FusionComponentMatch {
                occurrence_id: face,
                spec_key: spec.key.clone(),
                template_name: spec.template.name.clone(),
                covered_face_ids: BTreeSet::from([face]),
                local_to_skeleton_atom: mapping
                    .iter()
                    .map(|(l, a)| (l.clone(), a + 1000))
                    .collect(),
                local_to_input_atom: mapping,
                ..Default::default()
            }
        };
        (
            make(
                &benzene,
                0,
                benzene.locants().iter().cloned().zip(benzo_input).collect(),
            ),
            make(&hetero, 1, input.into_iter().collect()),
        )
    }
    #[test]
    fn benzo_components_generate_bicyclic_locants_and_round_trip_without_registration() {
        let r = fusion_component_registry();
        for (source, edge, parent, prefix, junctions) in [
            (
                "oxazole",
                ["4", "5"],
                "[1,3]benzoxazole",
                "[1,3]benzoxazolo",
                ["3a", "7a"],
            ),
            (
                "isothiazole",
                ["3", "4"],
                "[2,1]benzothiazole",
                "[2,1]benzothiazolo",
                ["3a", "7a"],
            ),
            (
                "generated-hw:S.C.C.N.C.C",
                ["5", "6"],
                "[1,4]benzothiazine",
                "[1,4]benzothiazino",
                ["4a", "8a"],
            ),
            (
                "generated-hw:O.C.C.C.C.C.C",
                ["6", "7"],
                "[1]benzoxepine",
                "[1]benzoxepino",
                ["5a", "9a"],
            ),
            (
                "furan",
                ["4", "5"],
                "[1]benzofuran",
                "[1]benzofuro",
                ["3a", "7a"],
            ),
            (
                "imidazole",
                ["4", "5"],
                "[1,3]benzimidazole",
                "[1,3]benzimidazolo",
                ["3a", "7a"],
            ),
            (
                "generated-hw:N.N.N.C.C",
                ["4", "5"],
                "[1,2,3]benzotriazole",
                "[1,2,3]benzotriazolo",
                ["3a", "7a"],
            ),
        ] {
            let (benzo, hetero) = benzo_test_pair(source, edge);
            let matches = r.benzoheterocycle_matches(&benzo, &hetero).unwrap();
            assert!(!matches.is_empty(), "{source}");
            for matched in matches {
                let spec = r.spec_for_match(&matched).unwrap();
                assert_eq!(spec.parent_name, parent);
                assert_eq!(spec.attached_prefix, prefix);
                assert_eq!(spec.template.fusion_atoms, junctions);
                assert!(spec.usable_as_parent && spec.usable_as_attached);
                assert_eq!(matched.covered_face_ids, BTreeSet::from([0, 1]));
                assert_eq!(spec.atoms().len(), hetero.local_to_input_atom.len() + 4);
                assert_eq!(spec.bonds().len(), hetero.local_to_input_atom.len() + 5);
                assert_eq!(spec.rings.len(), 2);
                assert!(
                    matched
                        .local_to_input_atom
                        .iter()
                        .zip(&matched.local_to_skeleton_atom)
                        .all(|((l, a), (s, b))| l == s && *b == a + 1000)
                );
                let rebuilt = r.get(&matched.spec_key).unwrap();
                assert_eq!(rebuilt.spec.atoms(), spec.atoms());
                assert_eq!(rebuilt.spec.bonds(), spec.bonds());
                for (old, a) in &hetero.local_to_input_atom {
                    let new = matched
                        .local_to_input_atom
                        .iter()
                        .find(|(_, b)| a == b)
                        .unwrap();
                    let old_atom =
                        r.spec_for_match(&hetero).unwrap().template.atom_by_locant()[old].clone();
                    let new_atom = spec.template.atom_by_locant()[&new.0].clone();
                    assert_eq!(old_atom.aromatic, new_atom.aromatic);
                    assert_eq!(old_atom.saturated, new_atom.saturated);
                    assert_eq!(old_atom.pi_capacity, new_atom.pi_capacity);
                }
            }
        }
    }
    #[test]
    fn benzo_generation_respects_exceptional_trivial_bicycle_names() {
        let r = fusion_component_registry();
        let (benzo, hetero) = benzo_test_pair("pyridine", ["5", "6"]);
        let matches = r.benzoheterocycle_matches(&benzo, &hetero).unwrap();
        assert!(!matches.is_empty());
        for matched in matches {
            let spec = r.spec_for_match(&matched).unwrap();
            assert_eq!(spec.parent_name, "quinoline");
            assert_eq!(spec.attached_prefix, "quinolino");
        }
    }
    #[test]
    fn standalone_benzo_usage_preserves_component_identity_and_retained_names() {
        let r = fusion_component_registry();
        for (source, edge, attached_parent, standalone_parent) in [
            (
                "isothiazole",
                ["3", "4"],
                "[2,1]benzothiazole",
                "2,1-benzothiazole",
            ),
            ("pyridine", ["5", "6"], "quinoline", "quinoline"),
            (
                "imidazole",
                ["4", "5"],
                "[1,3]benzimidazole",
                "1,3-benzimidazole",
            ),
            (
                "generated-hw:N.N.N.C.C",
                ["4", "5"],
                "[1,2,3]benzotriazole",
                "1,2,3-benzotriazole",
            ),
        ] {
            let (benzo, hetero) = benzo_test_pair(source, edge);
            let components = r.benzoheterocycle_matches(&benzo, &hetero).unwrap();
            let parents = r.benzoheterocycle_parent_matches(&benzo, &hetero).unwrap();
            assert!(!parents.is_empty());
            assert_eq!(components.len(), parents.len());
            for (component, parent) in components.iter().zip(&parents) {
                let mut expected = component.clone();
                expected.standalone_benzo_parent = true;
                assert_eq!(*parent, expected);
                assert_eq!(
                    r.spec_for_match(component).unwrap().parent_name,
                    attached_parent
                );
                assert_eq!(
                    r.spec_for_match(parent).unwrap().parent_name,
                    standalone_parent
                );
            }
        }
    }

    #[test]
    fn initial_components_distinguish_fr228_from_retained_trivial_bicycles() {
        let r = fusion_component_registry();
        // P-25.2.2.4: 1-benzopyran, not chromene, is the PIN component.
        for key in [
            "1-benzofuran",
            "1-benzothiophene",
            "1H-benzotriazole",
            "benzimidazole",
            "2H-1-benzopyran",
            "1H-2-benzopyran",
        ] {
            assert!(r.get(key).unwrap().spec.benzoheterocycle_component, "{key}");
        }
        for key in [
            "indole",
            "1H-isoindole",
            "2H-isoindole",
            "indazole",
            "quinoline",
            "isoquinoline",
            "quinazoline",
            "quinoxaline",
            "cinnoline",
            "phthalazine",
        ] {
            let spec = r.get(key).unwrap().spec;
            assert!(!spec.benzoheterocycle_component, "{key}");
            assert!(spec.usable_as_parent, "{key}");
            assert!(spec.usable_as_attached, "{key}");
        }
    }

    /// P-25.3.2.4(b,d,g): ring count, heteroatom count, then horizontal rings.
    #[test]
    fn retained_tricycles_compete_as_parent_components() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("c1ccc2nc3ccccc3cc2c1", "acridine"),
            ("c1ccc2c(c1)cnc1ccccc12", "phenanthridine"),
            ("c1ccc2c(c1)[nH]c1ccccc12", "9H-carbazole"),
            ("c1ccc2nc3ccccc3nc2c1", "phenazine"),
            ("n1cccc2ccc3cccnc3c12", "1,10-phenanthroline"),
            (
                "C1=CC=NC=2C=CC=3C=C4C(=NC3C12)C=CC=C4",
                "benzo[b][1,7]phenanthroline",
            ),
            (
                "N1=CC=CC2=CC=C3N=C4C(=CC3=C12)C=CC=C4",
                "benzo[j][1,7]phenanthroline",
            ),
            (
                "N1=CC=CC2=CC=C3C=C4C(=NC3=C12)C=CC=C4",
                "benzo[b][1,10]phenanthroline",
            ),
            ("c1c3ncncc3cc4nc2ccccc2cc14", "pyrimido[4,5-b]acridine"),
            (
                "c2c4c1ccccc1ncc4cc5nc3ccccc3cc25",
                "quinolino[4,3-b]acridine",
            ),
        ] {
            let name = super::super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            assert_eq!(
                name.status,
                crate::naming::NameStatus::Preferred,
                "{smiles}"
            );
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(parsed.status, opsin::Status::Success, "{}", parsed.message);
            assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
            let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(smiles, &reconstructed).unwrap();
        }
    }

    /// P-25.3.2.4: retained bicycles compete as parents, e.g. quinoxaline
    /// (2 rings) over pyrazine (b), and quinazoline (6,6) over indole (c).
    #[test]
    fn retained_bicycles_compete_as_parent_components() {
        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            (
                "Cc1c2ccccc2nc2cc3ccccc3n12",
                "12-methylindolo[2,1-b]quinazoline",
            ),
            (
                "c1ccc2c(c1)[nH]c1nc3ccccc3nc12",
                "6H-indolo[2,3-b]quinoxaline",
            ),
            ("c1ccc2cc3nccnc3cc2c1", "benzo[g]quinoxaline"),
            (
                "CSc1nc(N)c2ccc3ccccc3c2n1",
                "2-(methylsulfanyl)benzo[h]quinazolin-4-amine",
            ),
            ("c1ccc2c(c1)[nH]c1ncccc12", "9H-pyrido[2,3-b]indole"),
            ("c1ccc2c(c1)[nH]c1ccncc12", "5H-pyrido[4,3-b]indole"),
            (
                "C1=NN=C2C1=C1C(=NC=3C=CC=CC13)C=CO2",
                "pyrazolo[4',3':6,7]oxepino[4,5-b]indole",
            ),
            (
                "CC1=C2C(=CC=C1)C3=C(N2)N=C(N=N3)NN=C4C5=C(C=CC(=C5)[N+](=O)[O-])N(C4=O)C",
                "1-methyl-3-{[(6-methyl-5H-[1,2,4]triazino[5,6-b]indol-3-yl)amino]imino}-5-nitro-1,3-dihydro-2H-indol-2-one",
            ),
        ] {
            let name = super::super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected, "{smiles}");
            let parsed = parser.parse(&name.text, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{}: {}",
                name.text,
                parsed.message
            );
            assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
            let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(smiles, &reconstructed).unwrap();
        }
    }

    #[test]
    fn retained_benzo_components_cite_whole_bicycle_heteroatom_locants() {
        let registry = fusion_component_registry();
        for (key, prefix) in [
            ("1-benzofuran", "[1]benzofuro"),
            ("1-benzothiophene", "[1]benzothieno"),
            ("1,4-benzodioxine", "[1,4]benzodioxino"),
            ("1,3-benzodioxole", "[1,3]benzodioxolo"),
            ("1,4-benzoxazine", "[1,4]benzoxazino"),
        ] {
            assert_eq!(registry.get(key).unwrap().spec.attached_prefix, prefix);
        }
        for (key, parent, prefix) in [
            ("benzimidazole", "[1,3]benzimidazole", "[1,3]benzimidazolo"),
            (
                "1H-benzotriazole",
                "[1,2,3]benzotriazole",
                "[1,2,3]benzotriazolo",
            ),
        ] {
            let component = registry.get(key).unwrap().spec;
            assert_eq!(component.parent_name, parent);
            assert_eq!(component.attached_prefix, prefix);
            assert!(component.benzoheterocycle_component);
            assert!(component.template.default_indicated_h.is_empty());
        }
    }

    #[test]
    fn retained_phenazine_remains_available_before_benzo_component_combination() {
        let registry = fusion_component_registry();
        let phenazine = registry.get("phenazine").unwrap().spec;
        assert!(phenazine.usable_as_parent && phenazine.usable_as_attached);
        assert!(!phenazine.benzoheterocycle_component);
        assert_eq!(phenazine.parent_name, "phenazine");
        assert_eq!(phenazine.attached_prefix, "phenazino");
        assert_eq!(phenazine.rings.len(), 3);
        assert_eq!(phenazine.horizontal_ring_count, 3);
        // FR-2.3(b): the retained three-ring N component precedes pyrazine
        // before FR-3.5 can combine any initially selected benzo components.
        assert!(
            component_spec_seniority_key(&phenazine)
                < component_spec_seniority_key(&registry.get("pyrazine").unwrap().spec)
        );

        let parser = opsin::Parser::new().unwrap();
        for (smiles, expected) in [
            ("c1oc2cc3nc4ccccc4nc3cc2c1", "furo[2,3-b]phenazine"),
            ("c1sc2cc3nc4ccccc4nc3cc2c1", "thieno[2,3-b]phenazine"),
        ] {
            // Parse the specified fusion name independently of name generation.
            let reference = parser.parse(expected, &opsin::ParseOptions::strict());
            assert_eq!(
                reference.status,
                opsin::Status::Success,
                "{expected}: {}",
                reference.message
            );
            let reference_smiles = reference.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(smiles, &reference_smiles).unwrap();

            let name = super::super::super::name_smiles(smiles).unwrap();
            assert_eq!(name.text, expected);
            assert_eq!(
                name.status,
                crate::naming::NameStatus::Unverified(crate::naming::NameReason::OpsinFusionLayout)
            );
            for candidate in std::iter::once(name.text.as_str()).chain(
                name.alternatives
                    .iter()
                    .map(|alternative| alternative.name.as_str()),
            ) {
                let parsed = parser.parse(candidate, &opsin::ParseOptions::strict());
                assert_eq!(
                    parsed.status,
                    opsin::Status::Success,
                    "{candidate}: {}",
                    parsed.message
                );
                let reconstructed = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
                crate::naming::verify_identity(smiles, &reconstructed)
                    .unwrap_or_else(|e| panic!("{smiles} -> {candidate} -> {reconstructed}: {e}"));
            }
        }
    }
}
