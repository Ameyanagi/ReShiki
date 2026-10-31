//! Structural functional-group detectors in the same priority order as Openclatura.
use super::molecule::{AtomSet, Molecule};
use super::nomenclature::{
    FunctionalGroupRule, MultiSuffixTemplate, RULES, rule_from_value, section, table_rows,
    table_string,
};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChalcogenLigand {
    pub atom: usize,
    pub element: String,
    pub role: String,
    pub center: usize,
    pub bond_id: usize,
    pub attachment_atom: Option<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeavingGroupDescriptor {
    pub kind: String,
    pub attachment_atom: usize,
    pub atom_ids: Vec<usize>,
    pub bond_ids: Vec<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionalGroupDescriptor {
    pub family: String,
    pub derivative: String,
    pub centers: Vec<usize>,
    pub ligands: Vec<ChalcogenLigand>,
    pub linker_paths: Vec<Vec<usize>>,
    pub attachment_atom: Option<usize>,
    pub is_external: bool,
    pub central_element: Option<String>,
    pub leaving_group: Option<LeavingGroupDescriptor>,
    pub shared_atoms: Vec<usize>,
}
impl FunctionalGroupDescriptor {
    pub fn atom_ids(&self) -> AtomSet {
        self.centers
            .iter()
            .copied()
            .chain(self.ligands.iter().map(|l| l.atom))
            .chain(self.ligands.iter().filter_map(|l| l.attachment_atom))
            .chain(self.linker_paths.iter().flatten().copied())
            .chain(self.shared_atoms.iter().copied())
            .chain(
                self.leaving_group
                    .iter()
                    .flat_map(|g| g.atom_ids.iter().copied()),
            )
            .collect()
    }
    pub fn ligands_with_role(&self, role: &str) -> Vec<&ChalcogenLigand> {
        self.ligands.iter().filter(|l| l.role == role).collect()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PerceivedGroup {
    pub key: String,
    pub is_principal_candidate: bool,
    pub attachment_carbon: usize,
    pub atoms_involved: AtomSet,
    pub variant: Option<String>,
    pub role: Option<String>,
    pub descriptor: Option<FunctionalGroupDescriptor>,
    pub resolved_rule: Option<FunctionalGroupRule>,
}
impl PerceivedGroup {
    pub fn new(
        key: impl Into<String>,
        principal: bool,
        attachment: usize,
        atoms: impl IntoIterator<Item = usize>,
    ) -> Self {
        let key = key.into();
        let resolved_rule = RULES.functional_groups.get(&key).cloned();
        Self {
            key,
            is_principal_candidate: principal,
            attachment_carbon: attachment,
            atoms_involved: atoms.into_iter().collect(),
            resolved_rule,
            ..Self::default()
        }
    }
    pub fn atom_ids(&self) -> AtomSet {
        let mut s = self.atoms_involved.clone();
        s.insert(self.attachment_carbon);
        s
    }
    pub fn prefix(&self) -> Option<&str> {
        self.resolved_rule.as_ref()?.prefix.as_deref()
    }
}
fn group(
    key: impl Into<String>,
    principal: bool,
    attachment: usize,
    atoms: impl IntoIterator<Item = usize>,
) -> PerceivedGroup {
    PerceivedGroup::new(key, principal, attachment, atoms)
}
fn desc_group(
    key: String,
    rule: Option<FunctionalGroupRule>,
    attachment: usize,
    atoms: impl IntoIterator<Item = usize>,
    desc: FunctionalGroupDescriptor,
) -> PerceivedGroup {
    let mut g = group(key, true, attachment, atoms);
    g.descriptor = Some(desc);
    if rule.is_some() {
        g.resolved_rule = rule;
    }
    g
}
fn order(m: &Molecule, a: usize, b: usize) -> usize {
    m.get_bond(a, b).map_or(0, |b| b.order)
}
fn symbol(m: &Molecule, a: usize) -> &str {
    &m.atoms[&a].symbol
}
fn carbon(m: &Molecule, a: usize) -> bool {
    symbol(m, a) == "C"
}
fn is_chalcogen(s: &str) -> bool {
    matches!(s, "O" | "S" | "Se" | "Te" | "Po")
}
fn is_halogen(s: &str) -> bool {
    matches!(s, "F" | "Cl" | "Br" | "I")
}
pub fn classify_chalcogen_ligand(m: &Molecule, center: usize, a: usize) -> Option<ChalcogenLigand> {
    let atom = m.atoms.get(&a)?;
    if !is_chalcogen(&atom.symbol) {
        return None;
    }
    let b = m.get_bond(center, a)?;
    // Upstream divergence: ordinary chalcogen roles require exact charge and hydrogen states.
    let (role, attachment) =
        if b.order == 2 && m.degree(a) == 1 && atom.charge == 0 && atom.total_h_count == 0 {
            ("double_bonded", None)
        } else if b.order != 1 {
            return None;
        } else {
            let side = m
                .get_neighbors(a)
                .iter()
                .copied()
                .filter(|n| *n != center)
                .collect::<Vec<_>>();
            if atom.charge == -1 && atom.total_h_count == 0 && side.is_empty() {
                ("anionic", None)
            } else if atom.charge == 0
                && atom.total_h_count == 1
                && side.is_empty()
                && atom.radical_electrons == 0
            {
                ("hydrogen_bearing", None)
            } else if atom.charge == 0 && atom.total_h_count == 0 && side.len() == 1 {
                let n = side[0];
                (
                    if is_chalcogen(symbol(m, n)) {
                        "chalcogen_link"
                    } else if carbon(m, n) {
                        "carbon_link"
                    } else {
                        "heteroatom_link"
                    },
                    Some(n),
                )
            } else {
                return None;
            }
        };
    Some(ChalcogenLigand {
        atom: a,
        element: atom.symbol.clone(),
        role: role.into(),
        center,
        bond_id: b.idx,
        attachment_atom: attachment,
    })
}
fn terminal_double(m: &Molecule, c: usize, used: &AtomSet) -> Option<ChalcogenLigand> {
    let ds = m
        .get_neighbors(c)
        .iter()
        .filter(|n| !used.contains(n))
        .filter_map(|&n| classify_chalcogen_ligand(m, c, n))
        .filter(|l| l.role == "double_bonded")
        .collect::<Vec<_>>();
    (ds.len() == 1).then(|| ds[0].clone())
}
fn cyclic_atoms(m: &Molecule) -> AtomSet {
    // An edge is cyclic exactly when its endpoints remain connected without it.
    let mut out = AtomSet::new();
    for b in m.bonds.values() {
        let mut seen = AtomSet::from([b.u]);
        let mut todo = vec![b.u];
        while let Some(a) = todo.pop() {
            for &n in m.get_neighbors(a) {
                if (a == b.u && n == b.v) || (a == b.v && n == b.u) {
                    continue;
                }
                if seen.insert(n) {
                    todo.push(n);
                }
            }
        }
        if seen.contains(&b.v) {
            out.insert(b.u);
            out.insert(b.v);
        }
    }
    out
}
fn connected_without(m: &Molecule, start: usize, target: usize, blocked: &AtomSet) -> bool {
    let mut seen = blocked.clone();
    seen.insert(start);
    let mut todo = vec![start];
    while let Some(a) = todo.pop() {
        for &n in m.get_neighbors(a) {
            if n == target {
                return true;
            }
            if seen.insert(n) {
                todo.push(n);
            }
        }
    }
    false
}
fn closes_ring(m: &Molecule, c: usize, hetero: usize, double: usize, cyclic: &AtomSet) -> bool {
    let mut seen = AtomSet::from([c, double]);
    let mut todo = VecDeque::from([hetero]);
    while let Some(a) = todo.pop_front() {
        for &n in m.get_neighbors(a) {
            if n == c && a != hetero {
                return true;
            }
            if cyclic.contains(&n) && seen.insert(n) {
                todo.push_back(n);
            }
        }
    }
    false
}
fn external_attachment(m: &Molecule, c: usize, cyclic: &AtomSet) -> (usize, bool) {
    let rings = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| cyclic.contains(n))
        .collect::<Vec<_>>();
    if !cyclic.contains(&c) && rings.len() == 1 && order(m, c, rings[0]) == 1 {
        (rings[0], true)
    } else {
        (c, false)
    }
}
fn descriptor(
    family: &str,
    derivative: &str,
    centers: Vec<usize>,
    ligands: Vec<ChalcogenLigand>,
    attachment: usize,
    external: bool,
) -> FunctionalGroupDescriptor {
    FunctionalGroupDescriptor {
        family: family.into(),
        derivative: derivative.into(),
        centers,
        ligands,
        attachment_atom: Some(attachment),
        is_external: external,
        ..Default::default()
    }
}
fn rank(element: &str) -> i64 {
    section("chalcogen_elements")[element]["rank"]
        .as_i64()
        .unwrap_or(4)
}
fn infix(elements: &[String]) -> String {
    let mut es = elements
        .iter()
        .filter(|s| s.as_str() != "O")
        .collect::<Vec<_>>();
    if es.len() == 2 && es[0] == es[1] {
        return format!(
            "di{}",
            section("chalcogen_elements")[es[0]]["replacement_infix"]
                .as_str()
                .unwrap_or("")
        );
    }
    es.sort_by_key(|e| {
        section("chalcogen_elements")[*e]["replacement_infix"]
            .as_str()
            .unwrap_or("")
    });
    es.into_iter()
        .map(|e| {
            section("chalcogen_elements")[e]["replacement_infix"]
                .as_str()
                .unwrap_or("")
        })
        .collect()
}
fn template(name: &str, values: &[(&str, String)]) -> String {
    format_template(
        section("chalcogen_templates")[name].as_str().unwrap_or(""),
        values,
    )
}
fn format_template(t: &str, values: &[(&str, String)]) -> String {
    let mut result = t.to_owned();
    for (k, v) in values {
        result = result.replace(&format!("{{{k}}}"), v);
    }
    result
}
fn simple_key(family: &str, derivative: &str, element: &str, external: bool) -> Option<String> {
    section("chalcogen_simple_group_keys")
        .as_array()?
        .iter()
        .find(|v| {
            v["family"] == family
                && v["derivative"] == derivative
                && v["element"] == element
                && v["external"].as_bool().unwrap_or(false) == external
        })?
        .get("key")?
        .as_str()
        .map(str::to_owned)
}
fn dynamic_rule(
    key: &str,
    policy: &serde_json::Value,
    prefix: Option<String>,
    suffix: Option<String>,
    seniority: Vec<i64>,
    external: bool,
    multi: bool,
) -> FunctionalGroupRule {
    let mut r = rule_from_value(key, policy);
    r.role = "principal".into();
    r.prefix = prefix;
    r.suffix = suffix;
    r.seniority = Some(seniority);
    r.suffix_with_locant = external;
    r.suffix_multiplier_positions = vec![0];
    r.multi_suffix = multi.then(|| MultiSuffixTemplate {
        multiplier_positions: vec![0],
    });
    r
}
fn resolve_acyl(d: &FunctionalGroupDescriptor) -> (String, Option<FunctionalGroupRule>) {
    let double = &d.ligands[0];
    let single = d.ligands.get(1);
    let leaving = d.leaving_group.as_ref();
    let leaving_data = leaving.map(|g| &section("chalcogen_acyl_leaving_groups")[&g.kind]);
    if matches!(d.derivative.as_str(), "acid_halide" | "acyl_pseudohalide")
        && double.element == "O"
        && let Some(ld) = leaving_data
        && let Some(k) = ld[if d.is_external {
            "external_key"
        } else {
            "chain_key"
        }]
        .as_str()
    {
        return (k.to_owned(), None);
    }
    if let Some(k) = table_rows("chalcogen_standard_acyl_keys")
        .iter()
        .find(|v| {
            v["derivative"] == d.derivative
                && v["double_element"] == double.element
                && v["single_element"].as_str() == single.map(|l| l.element.as_str())
                && v["external"].as_bool().unwrap_or(false) == d.is_external
        })
        .and_then(|v| v["key"].as_str())
    {
        return (k.to_owned(), None);
    }
    let mut sites = double.element.clone();
    if let Some(s) = single {
        sites.push('_');
        sites.push_str(&s.element);
    }
    if let Some(l) = leaving {
        sites.push('_');
        sites.push_str(&l.kind);
    }
    let key = format!(
        "acyl_{}_{}_{}",
        d.derivative,
        sites,
        if d.is_external { "external" } else { "chain" }
    );
    let policy = &section("chalcogen_rule_policies")["acyl"][&d.derivative];
    let mut es = vec![double.element.clone()];
    if let Some(s) = single {
        es.push(s.element.clone());
    }
    let mut seniority = vec![policy["seniority"].as_i64().unwrap_or(60)];
    if policy["ranked_seniority"].as_bool().unwrap_or(false) {
        seniority[0] += rank(&double.element);
    } else {
        seniority.extend(es.iter().map(|e| rank(e)));
        if let Some(ld) = leaving_data {
            seniority.push(ld["rank"].as_i64().unwrap_or(0));
        }
    }
    if es
        .iter()
        .any(|e| section("chalcogen_elements")[e]["replacement_infix"].is_null())
    {
        return (
            key.clone(),
            Some(dynamic_rule(
                &key,
                policy,
                None,
                None,
                seniority,
                d.is_external,
                false,
            )),
        );
    }
    let site = if d.derivative == "acid" {
        if let Some(single) = single.filter(|s| s.element != double.element) {
            template("acid_site_element", &[("element", single.element.clone())])
        } else {
            template("acid_site_same", &[])
        }
    } else {
        String::new()
    };
    let vals = [
        (
            "carbo",
            if d.is_external {
                "carbo".into()
            } else {
                "".into()
            },
        ),
        ("infix", infix(&es)),
        ("site", site),
        (
            "halide_word",
            leaving_data
                .and_then(|l| l["word"].as_str())
                .unwrap_or("")
                .into(),
        ),
        (
            "halide_prefix",
            leaving_data
                .and_then(|l| l["prefix"].as_str())
                .unwrap_or("")
                .into(),
        ),
    ];
    let mut r = dynamic_rule(
        &key,
        policy,
        policy["prefix"].as_str().map(|p| format_template(p, &vals)),
        policy["suffix"].as_str().map(|p| format_template(p, &vals)),
        seniority,
        d.is_external,
        policy["multi_suffix"].as_bool() != Some(false),
    );
    if let Some(t) = policy["citation_suffix"].as_str() {
        r.citation_suffixes
            .insert("anhydride_half".into(), format_template(t, &vals));
    }
    (key, Some(r))
}
fn peroxo_infix(a: &str, b: &str) -> String {
    if a == "O" && b == "O" {
        return template("peroxo", &[]);
    }
    let es = [a.to_owned(), b.to_owned()];
    let replacement = infix(&es);
    if a == b {
        template(
            "doubly_replaced_peroxo",
            &[(
                "replacement",
                section("chalcogen_elements")[a]["replacement_infix"]
                    .as_str()
                    .unwrap_or("")
                    .into(),
            )],
        )
    } else {
        template("replaced_peroxo", &[("replacement", replacement)])
    }
}
fn resolve_peroxy(d: &FunctionalGroupDescriptor) -> (String, Option<FunctionalGroupRule>) {
    let ds = &d.ligands;
    let es = ds.iter().map(|l| l.element.clone()).collect::<Vec<_>>();
    if es.iter().all(|s| s == "O")
        && let Some(k) = table_rows("chalcogen_standard_peroxy_acyl_keys")
            .iter()
            .find(|v| {
                v["derivative"] == d.derivative
                    && v["external"].as_bool().unwrap_or(false) == d.is_external
            })
            .and_then(|v| v["key"].as_str())
    {
        return (k.to_owned(), None);
    }
    let key = format!(
        "acyl_{}_{}_{}",
        d.derivative,
        es.join("_"),
        if d.is_external { "external" } else { "chain" }
    );
    let policy = &section("chalcogen_rule_policies")["peroxy_acyl"][&d.derivative];
    let mut seniority = vec![policy["seniority"].as_i64().unwrap_or(45)];
    seniority.extend(es.iter().map(|e| rank(e)));
    let vals = [
        (
            "carbo",
            if d.is_external {
                "carbo".into()
            } else {
                "".into()
            },
        ),
        ("acid_infix", infix(&es[..1])),
        ("linker", peroxo_infix(&es[1], &es[2])),
    ];
    let ordinary = es[1] == "O" && es[2] == "O";
    let suffix = if d.derivative == "acid" {
        format!(
            "{}{}",
            template(
                if ordinary {
                    "ordinary_peroxy_acid"
                } else {
                    "mixed_peroxy_acid"
                },
                &vals
            ),
            if es[1] == es[2] {
                template("acid_site_same", &[])
            } else {
                template(
                    "acid_site_pair",
                    &[("first", es[1].clone()), ("second", es[2].clone())],
                )
            }
        )
    } else {
        template(
            if ordinary {
                "ordinary_peroxy_ester"
            } else {
                "mixed_peroxy_ester"
            },
            &vals,
        )
    };
    let prefix = template("peroxy_prefix", &vals);
    let r = dynamic_rule(
        &key,
        policy,
        Some(prefix),
        Some(suffix),
        seniority,
        d.is_external,
        false,
    );
    (key, Some(r))
}
fn hydrazide_ns(m: &Molecule, c: usize, n: usize, cyclic: &AtomSet) -> Option<Vec<usize>> {
    if cyclic.contains(&n) || m.atoms[&n].charge != 0 {
        return None;
    }
    if m.get_neighbors(c)
        .iter()
        .any(|&x| x != n && symbol(m, x) == "N")
    {
        return None;
    }
    let tail = m
        .get_neighbors(n)
        .iter()
        .copied()
        .filter(|&x| x != c && symbol(m, x) == "N" && order(m, n, x) == 1)
        .collect::<Vec<_>>();
    if tail.len() != 1 {
        return None;
    }
    let t = tail[0];
    if cyclic.contains(&t)
        || m.atoms[&t].charge != 0
        || m.get_neighbors(t)
            .iter()
            .any(|&x| x != n && (!matches!(symbol(m, x), "C" | "H") || order(m, t, x) != 1))
    {
        return None;
    }
    Some(vec![n, t])
}
fn is_nitro_n(m: &Molecule, n: usize, parent: usize) -> bool {
    if symbol(m, n) != "N" || m.atoms[&n].total_h_count != 0 {
        return false;
    }
    let other = m
        .get_neighbors(n)
        .iter()
        .copied()
        .filter(|x| *x != parent)
        .collect::<Vec<_>>();
    !other.is_empty()
        && other
            .iter()
            .all(|&x| symbol(m, x) == "O" && m.degree(x) == 1)
}
fn urea_ns(m: &Molecule, c: usize, ns: &[usize], cyclic: &AtomSet) -> Option<Vec<usize>> {
    if cyclic.contains(&c) || ns.len() != 2 || m.degree(c) != 3 {
        return None;
    }
    if ns.iter().any(|&n| {
        cyclic.contains(&n)
            || m.atoms[&n].charge != 0
            || m.get_neighbors(n).iter().any(|&x| {
                x != c
                    && ((matches!(symbol(m, x), "N" | "O" | "S") && !is_nitro_n(m, x, n))
                        || order(m, n, x) != 1)
            })
    }) {
        return None;
    }
    let mut ns = ns.to_vec();
    ns.sort();
    Some(ns)
}
fn plain_amine_n(m: &Molecule, n: usize, c: usize, cyclic: &AtomSet) -> bool {
    !cyclic.contains(&n)
        && m.atoms[&n].charge == 0
        && m.get_neighbors(n).iter().all(|&x| {
            x == c
                || (order(m, n, x) == 1
                    && matches!(symbol(m, x), "C" | "H")
                    && !m.get_neighbors(x).iter().any(|&far| {
                        far != n
                            && order(m, x, far) == 2
                            && matches!(symbol(m, far), "O" | "S" | "N")
                    }))
        })
}
fn amidine(
    m: &Molecule,
    c: usize,
    dn: usize,
    ns: &[usize],
    cyclic: &AtomSet,
) -> Option<PerceivedGroup> {
    if m.atoms[&dn].charge != 0
        || cyclic.contains(&dn)
        || !matches!(m.degree(c), 2 | 3)
        || m.get_neighbors(dn)
            .iter()
            .any(|&n| n != c && (order(m, dn, n) != 1 || symbol(m, n) == "N"))
    {
        return None;
    }
    let singles = ns
        .iter()
        .copied()
        .filter(|&n| n != dn && order(m, c, n) == 1)
        .collect::<Vec<_>>();
    if singles.is_empty() || !singles.iter().all(|&n| plain_amine_n(m, n, c, cyclic)) {
        return None;
    }
    let others = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| *n != dn && !singles.contains(n))
        .collect::<Vec<_>>();
    let atoms = std::iter::once(c)
        .chain(std::iter::once(dn))
        .chain(singles.iter().copied());
    if singles.len() == 2 && others.is_empty() {
        return Some(group("guanidine", true, c, atoms));
    }
    if singles.len() == 1 && others.is_empty() && m.degree(c) == 2 {
        return Some(group("amidine", true, c, atoms));
    }
    if singles.len() != 1 || others.len() != 1 || !carbon(m, others[0]) {
        return None;
    }
    let ext = cyclic.contains(&others[0]) && order(m, c, others[0]) == 1;
    Some(group(
        if ext { "ring_amidine" } else { "amidine" },
        true,
        if ext { others[0] } else { c },
        atoms,
    ))
}
fn has_multiple(m: &Molecule, a: usize, allowed: &AtomSet) -> bool {
    m.get_neighbors(a)
        .iter()
        .any(|&n| !allowed.contains(&n) && symbol(m, n) != "H" && order(m, a, n) > 1)
}
fn amidino_tail(m: &Molecule, n: usize, allowed: &AtomSet) -> AtomSet {
    for &c in m.get_neighbors(n) {
        if allowed.contains(&c) || !carbon(m, c) || order(m, n, c) != 2 {
            continue;
        }
        let ns = m
            .get_neighbors(c)
            .iter()
            .copied()
            .filter(|&x| x != n && symbol(m, x) == "N" && order(m, c, x) == 1)
            .collect::<Vec<_>>();
        if ns.len() == 2 {
            return ns.into_iter().chain([c]).collect();
        }
    }
    AtomSet::new()
}
fn terminal_imino(m: &Molecule, n: usize, allowed: &AtomSet) -> bool {
    m.get_neighbors(n).iter().any(|&c| {
        !allowed.contains(&c)
            && matches!(symbol(m, c), "C" | "N")
            && order(m, n, c) == 2
            && (carbon(m, c)
                || m.get_neighbors(c)
                    .iter()
                    .all(|&x| x == n || symbol(m, x) == "H"))
    })
}
fn hydrazone_key_attachment(
    m: &Molecule,
    c: usize,
    cyclic: &AtomSet,
    amidino: bool,
) -> (String, usize) {
    let ring = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| cyclic.contains(n))
        .collect::<Vec<_>>();
    let cs = m.get_neighbors(c).iter().filter(|&&n| carbon(m, n)).count();
    let non_h = m
        .get_neighbors(c)
        .iter()
        .filter(|&&n| symbol(m, n) != "H")
        .count();
    let attach = if !cyclic.contains(&c) && ring.len() == 1 && cs == 1 && order(m, c, ring[0]) == 1
    {
        ring[0]
    } else {
        c
    };
    let key = if attach != c && non_h == 2 {
        if amidino {
            "ring_aldehyde_amidinohydrazone"
        } else {
            "ring_aldehyde_hydrazone"
        }
    } else if cs <= 1 && !cyclic.contains(&c) {
        if amidino {
            "aldehyde_amidinohydrazone"
        } else {
            "aldehyde_hydrazone"
        }
    } else {
        "hydrazone"
    };
    (key.into(), attach)
}
fn nitrogen_chain_roles(m: &Molecule, cyclic: &AtomSet, consumed: &AtomSet) -> Vec<PerceivedGroup> {
    let mut roles = Vec::new();
    let mut blocked = consumed.clone();
    // The two-nitrogen templates precede the three-nitrogen templates.
    for (&c, atom) in &m.atoms {
        if !atom.is_carbon() {
            continue;
        }
        for &n in m.get_neighbors(c) {
            if blocked.contains(&n)
                || cyclic.contains(&n)
                || symbol(m, n) != "N"
                || !matches!(order(m, c, n), 1 | 2)
            {
                continue;
            }
            let tails = m
                .get_neighbors(n)
                .iter()
                .copied()
                .filter(|&x| {
                    x != c && !blocked.contains(&x) && !cyclic.contains(&x) && symbol(m, x) == "N"
                })
                .collect::<Vec<_>>();
            if tails.len() != 1 {
                continue;
            }
            let t = tails[0];
            if m.get_neighbors(t)
                .iter()
                .any(|&x| x != n && symbol(m, x) != "H")
            {
                continue;
            }
            let cn = order(m, c, n);
            let nn = order(m, n, t);
            let charges = [atom.charge, m.atoms[&n].charge, m.atoms[&t].charge];
            let (key, variant) = if cn == 1 && nn == 2 && charges == [0, 0, 0] {
                ("diazenyl", "carbon_bound_neutral_diazene")
            } else if cn == 1
                && matches!(nn, 2 | 3)
                && charges[0] == 0
                && matches!(charges[1], 0 | 1)
                && matches!(charges[2], 0 | 1)
            {
                ("diazonio", "carbon_bound_diazonium")
            } else if cn == 2 && matches!(nn, 2 | 3) {
                ("diazo", "carbon_bound_diazo")
            } else {
                continue;
            };
            let mut g = group(key, key == "diazonio", c, [n, t]);
            g.variant = Some(variant.into());
            g.role = Some("nitrogen_chain".into());
            roles.push(g);
        }
    }
    blocked.extend(roles.iter().flat_map(|g| g.atoms_involved.iter().copied()));
    for (&n, atom) in &m.atoms {
        if atom.symbol != "N" || blocked.contains(&n) || cyclic.contains(&n) {
            continue;
        }
        let external = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| symbol(m, x) != "H" && (symbol(m, x) != "N" || cyclic.contains(&x)))
            .collect::<Vec<_>>();
        if external.len() != 1 || order(m, n, external[0]) != 1 {
            continue;
        }
        let mids = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| symbol(m, x) == "N" && !blocked.contains(&x) && !cyclic.contains(&x))
            .collect::<Vec<_>>();
        if mids.len() != 1 {
            continue;
        }
        let mid = mids[0];
        let tails = m
            .get_neighbors(mid)
            .iter()
            .copied()
            .filter(|&x| {
                x != n && symbol(m, x) == "N" && !blocked.contains(&x) && !cyclic.contains(&x)
            })
            .collect::<Vec<_>>();
        if tails.len() != 1 {
            continue;
        }
        let t = tails[0];
        if m.get_neighbors(t)
            .iter()
            .any(|&x| x != mid && symbol(m, x) != "H")
            || m.get_neighbors(mid)
                .iter()
                .any(|&x| x != n && x != t && symbol(m, x) != "H")
        {
            continue;
        }
        let bonds = (order(m, n, mid), order(m, mid, t));
        let charges = (atom.charge, m.atoms[&mid].charge, m.atoms[&t].charge);
        let key = match (bonds, charges) {
            ((2, 2), (0, 0, 0) | (0, 1, -1) | (-1, 1, 0)) => "azido",
            ((1, 2), (0, 0, 0)) => "diazenylamino",
            ((2, 1), (0, 0, 0)) => "aminodiazenyl",
            ((1, 1), (0, 0, 0)) => "hydrazinylamino",
            _ => continue,
        };
        let mut g = group(key, false, external[0], [n, mid, t]);
        g.role = Some("nitrogen_chain".into());
        roles.push(g);
    }
    blocked.extend(roles.iter().flat_map(|g| g.atoms_involved.iter().copied()));
    for (&n, atom) in &m.atoms {
        if atom.symbol != "N" || blocked.contains(&n) || cyclic.contains(&n) {
            continue;
        }
        let Some(c) = m
            .get_neighbors(n)
            .iter()
            .copied()
            .find(|&x| carbon(m, x) && order(m, n, x) == 2)
        else {
            continue;
        };
        let tails = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| {
                x != c && !blocked.contains(&x) && !cyclic.contains(&x) && symbol(m, x) == "N"
            })
            .collect::<Vec<_>>();
        if tails.len() != 1 || order(m, n, tails[0]) != 1 {
            continue;
        }
        let t = tails[0];
        let mut tail = amidino_tail(m, t, &AtomSet::from([n]));
        let (key, attach) = hydrazone_key_attachment(m, c, cyclic, !tail.is_empty());
        if !tail.is_empty() && !key.contains("amidino") {
            tail.clear();
        }
        if has_multiple(m, t, &AtomSet::from([n]))
            && !terminal_imino(m, t, &AtomSet::from([n]))
            && tail.is_empty()
        {
            continue;
        }
        tail.extend([n, t]);
        let mut g = group(key, true, attach, tail);
        g.variant = Some("carbon_nitrogen_double_bond".into());
        g.role = Some("nitrogen_chain".into());
        roles.push(g);
    }
    blocked.extend(roles.iter().flat_map(|g| g.atoms_involved.iter().copied()));
    for (&n, atom) in &m.atoms {
        if atom.symbol != "N" || blocked.contains(&n) || cyclic.contains(&n) || atom.charge != 0 {
            continue;
        }
        let tails = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| {
                symbol(m, x) == "N"
                    && !blocked.contains(&x)
                    && !cyclic.contains(&x)
                    && m.atoms[&x].charge == 0
            })
            .collect::<Vec<_>>();
        if tails.len() != 1 || order(m, n, tails[0]) != 1 {
            continue;
        }
        let cs = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| carbon(m, x))
            .collect::<Vec<_>>();
        if cs.len() != 1
            || order(m, n, cs[0]) != 1
            || has_multiple(m, tails[0], &AtomSet::from([n]))
        {
            continue;
        }
        let mut g = group("hydrazine", false, cs[0], [n, tails[0]]);
        g.variant = Some("prefix".into());
        g.role = Some("nitrogen_chain".into());
        roles.push(g);
    }
    let mut seen = AtomSet::new();
    roles.retain(|g| {
        if !seen.is_disjoint(&g.atoms_involved) {
            false
        } else {
            seen.extend(&g.atoms_involved);
            true
        }
    });
    roles
}
fn carbonyl_group(
    _m: &Molecule,
    c: usize,
    attachment: usize,
    ligand: ChalcogenLigand,
    derivative: &str,
) -> Option<PerceivedGroup> {
    let key = simple_key("carbonyl", derivative, &ligand.element, attachment != c)?;
    let atoms = [c, ligand.atom];
    Some(desc_group(
        key,
        None,
        attachment,
        atoms,
        descriptor(
            "carbonyl",
            derivative,
            vec![c],
            vec![ligand],
            attachment,
            attachment != c,
        ),
    ))
}
fn acyl_group(
    m: &Molecule,
    c: usize,
    used: &AtomSet,
    cyclic: &AtomSet,
) -> Option<Vec<PerceivedGroup>> {
    let ligands = m
        .get_neighbors(c)
        .iter()
        .filter(|n| !used.contains(n))
        .filter_map(|&n| classify_chalcogen_ligand(m, c, n))
        .collect::<Vec<_>>();
    let doubles = ligands
        .iter()
        .filter(|l| l.role == "double_bonded")
        .collect::<Vec<_>>();
    if doubles.len() != 1 {
        return None;
    }
    let double = doubles[0].clone();
    let (attachment, external) = external_attachment(m, c, cyclic);
    let mut ns = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| !used.contains(n) && symbol(m, *n) == "N" && order(m, c, *n) == 1)
        .collect::<Vec<_>>();
    let hs = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| !used.contains(n) && is_halogen(symbol(m, *n)) && order(m, c, *n) == 1)
        .collect::<Vec<_>>();
    let multi_acyl = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|&n| carbon(m, n))
        .any(|n| {
            m.get_neighbors(n)
                .iter()
                .copied()
                .filter(|&x| x != c && carbon(m, x))
                .any(|other| {
                    let ls = m
                        .get_neighbors(other)
                        .iter()
                        .filter_map(|&x| classify_chalcogen_ligand(m, other, x))
                        .collect::<Vec<_>>();
                    ls.iter().any(|l| l.role == "double_bonded")
                        && ls.iter().any(|l| {
                            matches!(
                                l.role.as_str(),
                                "hydrogen_bearing"
                                    | "anionic"
                                    | "carbon_link"
                                    | "heteroatom_link"
                                    | "chalcogen_link"
                            )
                        })
                })
        });
    let registered = |l: &ChalcogenLigand| {
        table_rows("chalcogen_standard_acyl_keys").iter().any(|v| {
            v["derivative"] == "ester"
                && v["double_element"] == double.element
                && v["single_element"] == l.element
                && v["external"].as_bool().unwrap_or(false) == external
        })
    };
    let singles = ligands
        .iter()
        .filter(|l| {
            matches!(
                l.role.as_str(),
                "hydrogen_bearing"
                    | "anionic"
                    | "carbon_link"
                    | "heteroatom_link"
                    | "chalcogen_link"
            ) && m
                .get_neighbors(l.atom)
                .iter()
                .all(|n| *n == c || Some(*n) == l.attachment_atom)
                && (l.role != "heteroatom_link" || multi_acyl || (!ns.is_empty() && registered(l)))
        })
        .collect::<Vec<_>>();
    // A competing nitrogen derivative retains the registered carbamate route,
    // while the full acyl descriptor records both chalcogen linkers.
    if singles.len() == 1
        && !ns.is_empty()
        && hs.is_empty()
        && singles[0].role == "chalcogen_link"
        && registered(singles[0])
        && let Some(end) = singles[0].attachment_atom
        && let Some(terminal) = classify_chalcogen_ligand(m, singles[0].atom, end)
        && matches!(
            terminal.role.as_str(),
            "hydrogen_bearing" | "anionic" | "carbon_link"
        )
    {
        let single = singles[0].clone();
        let registered_atoms = [c, double.atom, single.atom];
        let mut desc = descriptor(
            "acyl",
            "ester",
            vec![c],
            vec![double, single, terminal],
            attachment,
            external,
        );
        desc.linker_paths = vec![vec![c, desc.ligands[1].atom, end]];
        let (key, _) = resolve_acyl(&desc);
        let mut acyl = group("acyl", false, attachment, desc.atom_ids());
        acyl.descriptor = Some(desc);
        return Some(vec![acyl, group(key, true, attachment, registered_atoms)]);
    }
    if singles.len() == 1
        && hs.is_empty()
        && (ns.is_empty()
            || matches!(
                singles[0].role.as_str(),
                "hydrogen_bearing" | "anionic" | "heteroatom_link"
            )
            || (singles[0].role == "carbon_link" && registered(singles[0])))
    {
        let single = singles[0].clone();
        if single.role == "chalcogen_link" {
            let end = single.attachment_atom?;
            if cyclic.contains(&c)
                && cyclic.contains(&single.atom)
                && cyclic.contains(&end)
                && closes_ring(m, c, single.atom, double.atom, cyclic)
            {
                return carbonyl_group(m, c, attachment, double, "ketone").map(|g| vec![g]);
            }
            let terminal = classify_chalcogen_ligand(m, single.atom, end)?;
            let derivative = match terminal.role.as_str() {
                "hydrogen_bearing" => "acid",
                "anionic" => "anion",
                "carbon_link" => "ester",
                _ => return None,
            };
            let atoms = [c, double.atom, single.atom, terminal.atom];
            let mut desc = descriptor(
                "acyl",
                derivative,
                vec![c],
                vec![double, single, terminal],
                attachment,
                external,
            );
            desc.linker_paths = vec![vec![c, desc.ligands[1].atom, end]];
            let (key, rule) = resolve_peroxy(&desc);
            return Some(vec![desc_group(key, rule, attachment, atoms, desc)]);
        }
        if matches!(single.role.as_str(), "carbon_link" | "heteroatom_link")
            && cyclic.contains(&c)
            && cyclic.contains(&single.atom)
            && single.attachment_atom.is_some_and(|a| cyclic.contains(&a))
            && closes_ring(m, c, single.atom, double.atom, cyclic)
        {
            return carbonyl_group(m, c, attachment, double, "ketone").map(|g| vec![g]);
        }
        let derivative = match single.role.as_str() {
            "hydrogen_bearing" => "acid",
            "anionic" => "anion",
            _ => "ester",
        };
        let atoms = [c, double.atom, single.atom];
        let mut desc = descriptor(
            "acyl",
            derivative,
            vec![c],
            vec![double, single],
            attachment,
            external,
        );
        if let Some(n) = desc.ligands[1].attachment_atom {
            desc.linker_paths = vec![vec![c, desc.ligands[1].atom, n]];
        }
        let (key, rule) = resolve_acyl(&desc);
        return Some(vec![desc_group(key, rule, attachment, atoms, desc)]);
    }
    if hs.len() == 1 && ns.is_empty() {
        let h = hs[0];
        let atoms = [c, double.atom, h];
        let mut desc = descriptor(
            "acyl",
            "acid_halide",
            vec![c],
            vec![double],
            attachment,
            external,
        );
        desc.leaving_group = Some(leaving_descriptor(m, symbol(m, h), c, vec![h]));
        let (key, rule) = resolve_acyl(&desc);
        return Some(vec![desc_group(key, rule, attachment, atoms, desc)]);
    }
    if ns.is_empty() {
        return None;
    }
    ns.sort_by_key(|&n| {
        std::cmp::Reverse((
            !cyclic.contains(&n),
            m.get_neighbors(n)
                .iter()
                .any(|&x| x != c && symbol(m, x) != "H"),
        ))
    });
    let n = ns[0];
    if cyclic.contains(&c) && cyclic.contains(&n) && closes_ring(m, c, n, double.atom, cyclic) {
        return carbonyl_group(m, c, attachment, double, "ketone").map(|g| vec![g]);
    }
    let (mut attachment, mut external) = (attachment, external);
    let (derivative, group_ns) = if let Some(v) = hydrazide_ns(m, c, n, cyclic) {
        ("hydrazide", v)
    } else if let Some(v) = urea_ns(m, c, &ns, cyclic) {
        attachment = c;
        external = false;
        ("urea", v)
    } else if !cyclic.contains(&n) {
        ("amide", vec![n])
    } else {
        return None;
    };
    let atoms = group_ns
        .into_iter()
        .chain([c, double.atom])
        .collect::<Vec<_>>();
    let desc = descriptor(
        "acyl",
        derivative,
        vec![c],
        vec![double],
        attachment,
        external,
    );
    let (key, rule) = resolve_acyl(&desc);
    Some(vec![desc_group(key, rule, attachment, atoms, desc)])
}
fn leaving_descriptor(
    m: &Molecule,
    kind: &str,
    c: usize,
    atoms: Vec<usize>,
) -> LeavingGroupDescriptor {
    let mut path = vec![c];
    path.extend(&atoms);
    let bonds = path
        .windows(2)
        .filter_map(|w| m.get_bond(w[0], w[1]).map(|b| b.idx))
        .collect();
    LeavingGroupDescriptor {
        kind: kind.into(),
        attachment_atom: atoms[0],
        atom_ids: atoms,
        bond_ids: bonds,
    }
}
fn pseudohalide_group(
    m: &Molecule,
    c: usize,
    used: &AtomSet,
    cyclic: &AtomSet,
) -> Option<PerceivedGroup> {
    let double = terminal_double(m, c, used)?;
    let mut leaving = None;
    for &first in m.get_neighbors(c) {
        if first == double.atom || used.contains(&first) || order(m, c, first) != 1 {
            continue;
        }
        let side = m
            .get_neighbors(first)
            .iter()
            .copied()
            .filter(|n| *n != c)
            .collect::<Vec<_>>();
        if side.len() != 1 {
            continue;
        }
        let second = side[0];
        let bond = order(m, first, second);
        if symbol(m, first) == "C" && symbol(m, second) == "N" && bond == 3 && m.degree(second) == 1
        {
            leaving = Some(leaving_descriptor(m, "CN", c, vec![first, second]));
            break;
        }
        if symbol(m, first) != "N" {
            continue;
        }
        if symbol(m, second) == "C" && bond == 3 && m.degree(second) == 1 {
            leaving = Some(leaving_descriptor(m, "NC", c, vec![first, second]));
            break;
        }
        let tails = m
            .get_neighbors(second)
            .iter()
            .copied()
            .filter(|n| *n != first)
            .collect::<Vec<_>>();
        if tails.len() != 1 {
            continue;
        }
        let third = tails[0];
        let kind = if symbol(m, second) == "N"
            && symbol(m, third) == "N"
            && m.atoms[&second].charge > 0
            && m.atoms[&third].charge < 0
        {
            "N3".to_owned()
        } else if symbol(m, second) == "C"
            && bond == 2
            && order(m, second, third) == 2
            && matches!(symbol(m, third), "O" | "S" | "Se" | "Te")
        {
            format!("NC{}", symbol(m, third))
        } else {
            continue;
        };
        leaving = Some(leaving_descriptor(m, &kind, c, vec![first, second, third]));
        break;
    }
    let leaving = leaving?;
    if m.get_neighbors(c)
        .iter()
        .any(|&n| n != double.atom && n != leaving.attachment_atom && !carbon(m, n))
    {
        return None;
    }
    let (attach, external) = external_attachment(m, c, cyclic);
    let atoms = leaving
        .atom_ids
        .iter()
        .copied()
        .chain([c, double.atom])
        .collect::<Vec<_>>();
    let mut d = descriptor(
        "acyl",
        "acyl_pseudohalide",
        vec![c],
        vec![double],
        attach,
        external,
    );
    let mut path = vec![c];
    path.extend(&leaving.atom_ids);
    d.linker_paths = vec![path];
    d.leaving_group = Some(leaving);
    let (key, rule) = resolve_acyl(&d);
    Some(desc_group(key, rule, attach, atoms, d))
}
fn cyclic_imide(
    m: &Molecule,
    n: usize,
    used: &AtomSet,
    cyclic: &AtomSet,
) -> Option<PerceivedGroup> {
    if symbol(m, n) != "N" || m.atoms[&n].charge != 0 || used.contains(&n) || !cyclic.contains(&n) {
        return None;
    }
    let mut cs = m
        .get_neighbors(n)
        .iter()
        .copied()
        .filter(|&c| cyclic.contains(&c) && carbon(m, c) && order(m, n, c) == 1)
        .collect::<Vec<_>>();
    cs.sort();
    if cs.len() != 2
        || m.get_neighbors(n)
            .iter()
            .any(|&x| !cs.contains(&x) && order(m, n, x) != 1)
    {
        return None;
    }
    let l = terminal_double(m, cs[0], used)?;
    let r = terminal_double(m, cs[1], used)?;
    if !connected_without(m, cs[0], cs[1], &AtomSet::from([n, l.atom, r.atom])) {
        return None;
    }
    let atoms = [n, cs[0], cs[1], l.atom, r.atom];
    let mut d = descriptor("acyl", "imide", cs.clone(), vec![l, r], cs[0], false);
    d.linker_paths = vec![vec![cs[0], n, cs[1]]];
    d.shared_atoms = vec![n];
    let policy = &section("chalcogen_rule_policies")["imide_route"];
    let key = policy["key"]
        .as_str()
        .unwrap_or("chalcogen_imide_route")
        .to_owned();
    let rule = dynamic_rule(
        &key,
        policy,
        None,
        None,
        vec![policy["seniority"].as_i64().unwrap_or(65)],
        false,
        false,
    );
    Some(desc_group(key, Some(rule), cs[0], atoms, d))
}
fn is_complete_acyl_end(m: &Molecule, center: usize, double: usize, linkage: usize) -> bool {
    let atom = &m.atoms[&center];
    if !atom.is_carbon()
        || atom.charge != 0
        || atom.radical_electrons != 0
        || atom.is_aromatic
        || order(m, center, double) != 2
        || order(m, center, linkage) != 1
    {
        return false;
    }
    let remaining = m
        .get_neighbors(center)
        .iter()
        .copied()
        .filter(|n| *n != double && *n != linkage)
        .collect::<Vec<_>>();
    if remaining.is_empty() {
        return atom.total_h_count == 1;
    }
    if remaining.len() != 1 || atom.total_h_count != 0 || order(m, center, remaining[0]) != 1 {
        return false;
    }
    let substituent = remaining[0];
    if carbon(m, substituent) {
        return true;
    }
    let mut component = AtomSet::from([substituent]);
    let mut frontier = vec![substituent];
    while let Some(current) = frontier.pop() {
        for &n in m.get_neighbors(current) {
            if n != center && component.insert(n) {
                frontier.push(n);
            }
        }
    }
    let edges = m
        .bonds
        .values()
        .filter(|b| component.contains(&b.u) && component.contains(&b.v))
        .collect::<Vec<_>>();
    edges.len() >= component.len() || edges.iter().any(|b| b.order > 1)
}

fn anhydride_groups(m: &Molecule, used: &AtomSet) -> Vec<PerceivedGroup> {
    let mut groups = Vec::new();
    let mut seen = BTreeSet::new();
    for (&c, a) in &m.atoms {
        if !a.is_carbon() || used.contains(&c) {
            continue;
        }
        let Some(left) = terminal_double(m, c, used) else {
            continue;
        };
        for &first in m.get_neighbors(c) {
            if first == left.atom || used.contains(&first) {
                continue;
            }
            let Some(l) = classify_chalcogen_ligand(m, c, first) else {
                continue;
            };
            let Some(second) = l.attachment_atom else {
                continue;
            };
            let (other, bridge) = if l.role == "carbon_link" {
                (second, vec![first])
            } else if l.role == "chalcogen_link" {
                let Some(l2) = classify_chalcogen_ligand(m, first, second) else {
                    continue;
                };
                if l2.role != "carbon_link" {
                    continue;
                }
                let Some(other) = l2.attachment_atom else {
                    continue;
                };
                (other, vec![first, second])
            } else {
                continue;
            };
            if c == other || !carbon(m, other) {
                continue;
            }
            let Some(right) = terminal_double(m, other, used) else {
                continue;
            };
            let Some(&last_bridge) = bridge.last() else {
                continue;
            };
            if !is_complete_acyl_end(m, c, left.atom, first)
                || !is_complete_acyl_end(m, other, right.atom, last_bridge)
                || connected_without(m, c, other, &bridge.iter().copied().collect())
            {
                continue;
            }
            let mut sorted = bridge.clone();
            sorted.sort();
            if !seen.insert((c.min(other), c.max(other), sorted)) {
                continue;
            }
            let (cs, ds, bridge) = if c <= other {
                (vec![c, other], vec![left.clone(), right], bridge)
            } else {
                (
                    vec![other, c],
                    vec![right, left.clone()],
                    bridge.into_iter().rev().collect(),
                )
            };
            if bridge.iter().any(|&n| {
                m.atoms[&n].charge != 0
                    || m.atoms[&n].total_h_count != 0
                    || m.atoms[&n].radical_electrons != 0
            }) {
                continue;
            }
            let mut path = vec![cs[0]];
            path.extend(&bridge);
            path.push(cs[1]);
            let mut ligands = ds.clone();
            for w in path.windows(2).take(path.len() - 2) {
                if let Some(l) = classify_chalcogen_ligand(m, w[0], w[1]) {
                    ligands.push(l);
                }
            }
            let mut d = descriptor("acyl", "anhydride", cs.clone(), ligands, cs[0], false);
            d.linker_paths = vec![path];
            let standard = ds.iter().all(|l| l.element == "O")
                && bridge.len() == 1
                && symbol(m, bridge[0]) == "O";
            let (key, rule) = if standard {
                ("anhydride".into(), None)
            } else {
                let mut sites = vec![ds[0].element.clone()];
                sites.extend(bridge.iter().map(|&n| symbol(m, n).to_owned()));
                sites.push(ds[1].element.clone());
                let key = format!("anhydride_{}", sites.join("_"));
                let es = bridge
                    .iter()
                    .map(|&n| symbol(m, n).to_owned())
                    .collect::<Vec<_>>();
                let suffix = if es.len() == 1 {
                    if es[0] == "O" {
                        "anhydride".into()
                    } else {
                        format!("{}anhydride", infix(&es))
                    }
                } else if es.iter().all(|e| e == "O") {
                    "peroxyanhydride".into()
                } else {
                    format!("{}peroxyanhydride", infix(&es))
                };
                let mut seniority = vec![30];
                seniority.extend(es.iter().map(|e| rank(e)));
                let r = dynamic_rule(
                    &key,
                    &section("chalcogen_rule_policies")["anhydride"],
                    None,
                    Some(suffix),
                    seniority,
                    false,
                    false,
                );
                (key, Some(r))
            };
            let atoms = ds
                .iter()
                .map(|l| l.atom)
                .chain(bridge.iter().copied())
                .collect::<Vec<_>>();
            groups.push(desc_group(key, rule, cs[0], atoms, d));
        }
    }
    groups
}
fn central_group(
    m: &Molecule,
    c: usize,
    used: &AtomSet,
    cyclic: &AtomSet,
) -> Option<PerceivedGroup> {
    let a = &m.atoms[&c];
    if !matches!(a.symbol.as_str(), "S" | "Se" | "Te")
        || used.contains(&c)
        || cyclic.contains(&c)
        || a.charge != 0
        || a.stereo.is_some()
        || a.raw_stereo.is_some()
    {
        return None;
    }
    if m.get_neighbors(c)
        .iter()
        .any(|&n| !is_chalcogen(symbol(m, n)) && order(m, c, n) > 1)
    {
        return None;
    }
    let anchors = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|&n| carbon(m, n) && order(m, c, n) == 1)
        .collect::<Vec<_>>();
    if anchors.len() != 1 {
        return None;
    }
    let anchor = anchors[0];
    let ligands = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| *n != anchor && !used.contains(n))
        .filter_map(|n| classify_chalcogen_ligand(m, c, n))
        .collect::<Vec<_>>();
    let doubles = ligands
        .iter()
        .filter(|l| l.role == "double_bonded")
        .cloned()
        .collect::<Vec<_>>();
    if !matches!(doubles.len(), 1 | 2) {
        return None;
    }
    let terminals = ligands
        .iter()
        .filter(|l| {
            matches!(
                l.role.as_str(),
                "hydrogen_bearing" | "anionic" | "carbon_link" | "chalcogen_link"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let ns = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| {
            !used.contains(n)
                && symbol(m, *n) == "N"
                && m.atoms[n].charge == 0
                && order(m, c, *n) == 1
        })
        .collect::<Vec<_>>();
    let hs = m
        .get_neighbors(c)
        .iter()
        .copied()
        .filter(|n| !used.contains(n) && is_halogen(symbol(m, *n)) && order(m, c, *n) == 1)
        .collect::<Vec<_>>();
    let mut group_atoms: AtomSet = doubles.iter().map(|l| l.atom).chain([c]).collect();
    let mut dligands = doubles.clone();
    let mut leaving = None;
    let derivative = if terminals.len() == 1 && ns.is_empty() && hs.is_empty() {
        let t = terminals[0].clone();
        group_atoms.insert(t.atom);
        dligands.push(t.clone());
        if t.role == "chalcogen_link" {
            let end = classify_chalcogen_ligand(m, t.atom, t.attachment_atom?)?;
            let der = match end.role.as_str() {
                "hydrogen_bearing" => "acid",
                "anionic" => "anion",
                "carbon_link" => "ester",
                _ => return None,
            };
            group_atoms.insert(end.atom);
            dligands.push(end);
            der
        } else {
            match t.role.as_str() {
                "hydrogen_bearing" => "acid",
                "anionic" => "anion",
                _ => "ester",
            }
        }
    } else if ns.len() == 1 {
        let n = ns[0];
        if cyclic.contains(&n) {
            return None;
        }
        if let Some(pair) = hydrazide_ns(m, c, n, cyclic) {
            let t = pair[1];
            if m.get_neighbors(t).iter().any(|&x| {
                x != n
                    && carbon(m, x)
                    && m.get_neighbors(x).iter().any(|&far| {
                        far != t && is_chalcogen(symbol(m, far)) && order(m, x, far) > 1
                    })
            }) {
                return None;
            }
            group_atoms.extend(pair);
            "hydrazide"
        } else {
            if m.get_neighbors(n)
                .iter()
                .any(|&x| x != c && (!matches!(symbol(m, x), "C" | "H") || order(m, n, x) != 1))
            {
                return None;
            }
            group_atoms.insert(n);
            "amide"
        }
    } else if hs.len() == 1 {
        let h = hs[0];
        group_atoms.insert(h);
        leaving = Some(leaving_descriptor(m, symbol(m, h), c, vec![h]));
        "acid_halide"
    } else {
        return None;
    };
    if m.get_neighbors(c)
        .iter()
        .any(|n| *n != anchor && !group_atoms.contains(n))
    {
        return None;
    }
    let mut d = descriptor("central_acid", derivative, vec![c], dligands, anchor, false);
    d.central_element = Some(a.symbol.clone());
    d.leaving_group = leaving;
    let (key, rule) = resolve_central(&d)?;
    Some(desc_group(key, rule, anchor, group_atoms, d))
}
fn resolve_central(d: &FunctionalGroupDescriptor) -> Option<(String, Option<FunctionalGroupRule>)> {
    let central = d.central_element.as_deref()?;
    let doubles = d
        .ligands
        .iter()
        .filter(|l| l.role == "double_bonded")
        .collect::<Vec<_>>();
    let terminal = d
        .ligands
        .iter()
        .filter(|l| l.role != "double_bonded")
        .collect::<Vec<_>>();
    let origin = section("chalcogen_central_acid_origins")
        .as_array()?
        .iter()
        .find(|v| {
            v["element"] == central && v["double_bond_count"].as_u64() == Some(doubles.len() as u64)
        })?;
    let stem = origin["stem"].as_str()?;
    let origin_rank = origin["rank"].as_i64()?;
    let all_o = d.ligands.iter().all(|l| l.element == "O");
    let standard_field = match d.derivative.as_str() {
        "acid" => "acid_key",
        "anion" | "ester" => "ester_key",
        "amide" => "amide_key",
        _ => "",
    };
    if all_o
        && terminal.len() != 2
        && let Some(k) = origin[standard_field].as_str()
    {
        return Some((k.into(), None));
    }
    let es = d
        .ligands
        .iter()
        .map(|l| l.element.clone())
        .collect::<Vec<_>>();
    let key = format!("central_{}_{}_{}", central, d.derivative, es.join("_"));
    if terminal.len() == 2 {
        let policy = &section("chalcogen_rule_policies")["central_peroxy"][&d.derivative];
        let di = infix(
            &doubles
                .iter()
                .map(|l| l.element.clone())
                .collect::<Vec<_>>(),
        );
        let origin_prefix = if di.is_empty() {
            stem.into()
        } else {
            format!("{stem}o{di}")
        };
        let ordinary = terminal.iter().all(|l| l.element == "O");
        let vals = [
            ("origin", origin_prefix),
            (
                "linker",
                peroxo_infix(&terminal[0].element, &terminal[1].element),
            ),
            (
                "site",
                template(
                    "acid_site_pair",
                    &[
                        ("first", terminal[0].element.clone()),
                        ("second", terminal[1].element.clone()),
                    ],
                ),
            ),
        ];
        let suffix = template(
            match (d.derivative.as_str() == "acid", ordinary) {
                (true, true) => "central_peroxy_acid",
                (true, false) => "central_mixed_peroxy_acid",
                (false, true) => "central_peroxy_ester",
                (false, false) => "central_mixed_peroxy_ester",
            },
            &vals,
        );
        let seniority = if d.derivative == "acid" {
            std::iter::once(origin_rank)
                .chain(es.iter().map(|e| rank(e)))
                .collect()
        } else {
            vec![policy["seniority"].as_i64().unwrap_or(45), origin_rank]
        };
        let rule = dynamic_rule(
            &key,
            policy,
            Some(template("central_peroxy_prefix", &vals)),
            Some(suffix),
            seniority,
            true,
            false,
        );
        return Some((key, Some(rule)));
    }
    let policy = &section("chalcogen_rule_policies")["central"][&d.derivative];
    let infix = infix(&es);
    let suffix_stem = if infix.is_empty() {
        stem.into()
    } else {
        format!("{stem}o{infix}")
    };
    let mut seniority = if d.derivative == "acid" {
        vec![origin_rank]
    } else {
        vec![policy["seniority"].as_i64().unwrap_or(66), origin_rank]
    };
    if matches!(d.derivative.as_str(), "amide" | "hydrazide") {
        seniority.extend(doubles.iter().map(|l| rank(&l.element)));
    } else if d.derivative == "acid_halide" {
        seniority.push(
            d.leaving_group
                .as_ref()
                .map(|l| {
                    section("chalcogen_acyl_leaving_groups")[&l.kind]["rank"]
                        .as_i64()
                        .unwrap_or(0)
                })
                .unwrap_or(0),
        );
    } else {
        seniority.extend(es.iter().map(|e| rank(e)));
    }
    let site = if !infix.is_empty() && !terminal.is_empty() {
        template(
            "acid_site_element",
            &[("element", terminal[0].element.clone())],
        )
    } else {
        " acid".into()
    };
    let vals = [
        ("stem", suffix_stem.clone()),
        ("site", site),
        (
            "halide_word",
            d.leaving_group
                .as_ref()
                .and_then(|l| section("chalcogen_acyl_leaving_groups")[&l.kind]["word"].as_str())
                .unwrap_or("")
                .into(),
        ),
    ];
    let suffix = if d.derivative == "hydrazide" && infix.is_empty() {
        format!("{stem}ohydrazide")
    } else {
        template(&format!("central_{}_suffix", d.derivative), &vals)
    };
    let prefix = if d.derivative == "acid" {
        if let Some(prefix) = origin["acid_prefix"].as_str().filter(|_| all_o) {
            prefix.into()
        } else {
            format!("{stem}o{infix}o")
        }
    } else {
        template(
            &format!("central_{}_prefix", d.derivative),
            &[
                ("stem", stem.into()),
                (
                    "halide_prefix",
                    d.leaving_group
                        .as_ref()
                        .and_then(|l| {
                            section("chalcogen_acyl_leaving_groups")[&l.kind]["prefix"].as_str()
                        })
                        .unwrap_or("")
                        .into(),
                ),
            ],
        )
    };
    let rule = dynamic_rule(
        &key,
        policy,
        Some(prefix),
        Some(suffix),
        seniority,
        true,
        true,
    );
    Some((key, Some(rule)))
}
fn composable_groups(m: &Molecule) -> Vec<PerceivedGroup> {
    let mut result = Vec::new();
    for b in m.bonds.values() {
        if b.order != 1
            || !is_chalcogen(symbol(m, b.u))
            || !is_chalcogen(symbol(m, b.v))
            || [b.u, b.v]
                .iter()
                .any(|n| m.atoms[n].charge != 0 || m.atoms[n].radical_electrons != 0)
        {
            continue;
        }
        let left = m
            .get_neighbors(b.u)
            .iter()
            .copied()
            .filter(|n| *n != b.v)
            .collect::<Vec<_>>();
        let right = m
            .get_neighbors(b.v)
            .iter()
            .copied()
            .filter(|n| *n != b.u)
            .collect::<Vec<_>>();
        if left.len() != 1
            || right.len() != 1
            || !carbon(m, left[0])
            || !carbon(m, right[0])
            || connected_without(m, left[0], right[0], &AtomSet::from([b.u, b.v]))
        {
            continue;
        }
        if [(left[0], b.u), (right[0], b.v)].iter().any(|&(c, link)| {
            m.get_neighbors(c).iter().any(|&n| {
                n != link
                    && classify_chalcogen_ligand(m, c, n).is_some_and(|l| l.role == "double_bonded")
            })
        }) {
            continue;
        }
        let mut d = descriptor(
            "peroxide",
            "neutral_link",
            vec![b.u, b.v],
            Vec::new(),
            left[0],
            false,
        );
        d.linker_paths = vec![vec![left[0], b.u, b.v, right[0]]];
        let mut g = group("peroxide", false, left[0], [b.u, b.v]);
        g.descriptor = Some(d);
        result.push(g);
    }
    for (&n, a) in &m.atoms {
        if a.symbol != "N" || a.charge <= 0 {
            continue;
        }
        for &e in m.get_neighbors(n) {
            if !is_chalcogen(symbol(m, e))
                || m.atoms[&e].charge != -1
                || m.degree(e) != 1
                || order(m, n, e) != 1
            {
                continue;
            }
            let others = m
                .get_neighbors(n)
                .iter()
                .copied()
                .filter(|x| *x != e)
                .collect::<Vec<_>>();
            let cs = others
                .iter()
                .copied()
                .filter(|&x| carbon(m, x))
                .collect::<Vec<_>>();
            if cs.is_empty()
                || others.iter().any(|&x| !carbon(m, x))
                || cs.iter().any(|&c| order(m, n, c) == 3)
            {
                continue;
            }
            let Some(ligand) = classify_chalcogen_ligand(m, n, e) else {
                continue;
            };
            let mut g = group("nitrogen_chalcogenide", false, cs[0], [n, e]);
            g.descriptor = Some(descriptor(
                "nitrogen_chalcogenide",
                "zwitterion",
                vec![n],
                vec![ligand],
                cs[0],
                false,
            ));
            result.push(g);
        }
    }
    result
}
/// Perceive characteristic groups. Detector order and atom consumption follow Python.
pub fn perceive_groups(m: &Molecule) -> Vec<PerceivedGroup> {
    let mut groups = composable_groups(m);
    let mut consumed = AtomSet::new();
    let cyclic = cyclic_atoms(m);
    // Nitro and nitroso precede all nitrogen chains and amine catch-alls.
    for (&n, a) in &m.atoms {
        if a.symbol != "N" || consumed.contains(&n) || cyclic.contains(&n) {
            continue;
        }
        let os = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| symbol(m, x) == "O")
            .collect::<Vec<_>>();
        let adj = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|x| !os.contains(x))
            .collect::<Vec<_>>();
        let hydroxyl = os.iter().any(|&o| {
            m.atoms[&o].charge == 0 && order(m, n, o) == 1 && m.atoms[&o].total_h_count > 0
        });
        if os.len() == 2
            && adj.len() == 1
            && a.total_h_count == 0
            && !hydroxyl
            && (a.charge == 1 || os.iter().any(|&o| order(m, n, o) == 2))
        {
            let atoms = os.iter().copied().chain([n]).collect::<Vec<_>>();
            groups.push(group("nitro", false, adj[0], atoms.clone()));
            consumed.extend(atoms);
        } else if os.len() == 1 && adj.len() == 1 && order(m, n, os[0]) == 2 {
            groups.push(group("nitroso", false, adj[0], [n, os[0]]));
            consumed.extend([n, os[0]]);
        }
    }
    for &n in m.atoms.keys() {
        if let Some(g) = cyclic_imide(m, n, &consumed, &cyclic) {
            groups.push(g);
        }
    }
    // Every hydrazide end is discovered before claiming shared N-N atoms.
    let mut hydrazides = Vec::new();
    let mut claimed = AtomSet::new();
    for (&c, a) in &m.atoms {
        if !a.is_carbon() || consumed.contains(&c) || cyclic.contains(&c) {
            continue;
        }
        let Some(double) = terminal_double(m, c, &consumed) else {
            continue;
        };
        for &n in m.get_neighbors(c) {
            if symbol(m, n) != "N" || consumed.contains(&n) || order(m, c, n) != 1 {
                continue;
            }
            let Some(ns) = hydrazide_ns(m, c, n, &cyclic) else {
                continue;
            };
            let (attach, external) = external_attachment(m, c, &cyclic);
            let atoms = ns
                .iter()
                .copied()
                .chain([c, double.atom])
                .collect::<Vec<_>>();
            let d = descriptor(
                "acyl",
                "hydrazide",
                vec![c],
                vec![double.clone()],
                attach,
                external,
            );
            let (key, rule) = resolve_acyl(&d);
            hydrazides.push(desc_group(key, rule, attach, atoms, d));
            claimed.extend(ns);
            claimed.insert(double.atom);
            break;
        }
    }
    groups.extend(hydrazides);
    consumed.extend(claimed);
    for &c in m.atoms.keys() {
        if let Some(g) = central_group(m, c, &consumed, &cyclic) {
            consumed.extend(
                g.atoms_involved
                    .iter()
                    .filter(|&&n| n != g.attachment_carbon)
                    .copied(),
            );
            groups.push(g);
        }
    }
    for (&c, a) in &m.atoms {
        if a.is_carbon()
            && !consumed.contains(&c)
            && let Some(g) = pseudohalide_group(m, c, &consumed, &cyclic)
        {
            consumed.extend(
                g.atoms_involved
                    .iter()
                    .filter(|&&n| n != g.attachment_carbon)
                    .copied(),
            );
            groups.push(g);
        }
    }
    let nitrogen_chains = nitrogen_chain_roles(m, &cyclic, &consumed);
    let has_nitrogen_chains = !nitrogen_chains.is_empty();
    for g in nitrogen_chains {
        consumed.extend(&g.atoms_involved);
        groups.push(g);
    }
    // Terminal imino on noncarbon centres is a prefix, subject to senior N ligands.
    for (&n, a) in &m.atoms {
        if a.symbol != "N" || consumed.contains(&n) || cyclic.contains(&n) {
            continue;
        }
        let doubles = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| order(m, n, x) == 2)
            .collect::<Vec<_>>();
        if doubles.len() != 1 || carbon(m, doubles[0]) {
            continue;
        }
        let center = doubles[0];
        if m.get_neighbors(n)
            .iter()
            .any(|&x| x != center && order(m, n, x) != 1)
        {
            continue;
        }
        let senior = m.get_neighbors(n).iter().any(|&x| {
            x != center
                && symbol(m, x) != "H"
                && (symbol(m, x) == "N"
                    || (carbon(m, x)
                        && m.get_neighbors(x).iter().any(|&far| {
                            matches!(symbol(m, far), "O" | "S" | "N") && order(m, x, far) == 2
                        })))
        });
        if m.degree(n) != 1 && (!cyclic.contains(&center) || senior) {
            continue;
        }
        let mut g = group("imino_prefix", false, center, [n]);
        g.variant = Some(
            if m.degree(n) == 1 {
                "terminal_heteroatom_imino"
            } else {
                "substituted_heteroatom_imino"
            }
            .into(),
        );
        g.role = Some("chalcogen_imide".into());
        groups.push(g);
        consumed.insert(n);
    }
    for (&n, a) in &m.atoms {
        if a.symbol != "N" || consumed.contains(&n) || cyclic.contains(&n) {
            continue;
        }
        let ns = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| symbol(m, x) == "N")
            .collect::<Vec<_>>();
        let adj = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|x| !ns.contains(x))
            .collect::<Vec<_>>();
        if adj.len() != 1 || ns.len() != 1 || order(m, n, adj[0]) != 1 {
            continue;
        }
        let mid = ns[0];
        let tails = m
            .get_neighbors(mid)
            .iter()
            .copied()
            .filter(|&x| x != n && symbol(m, x) == "N")
            .collect::<Vec<_>>();
        if tails.len() == 1 && m.degree(tails[0]) == 1 {
            groups.push(group("azido", false, adj[0], [n, mid, tails[0]]));
            consumed.extend([n, mid, tails[0]]);
        }
    }
    for g in anhydride_groups(m, &consumed) {
        if g.is_principal_candidate {
            consumed.extend(
                g.atoms_involved
                    .iter()
                    .filter(|&&n| n != g.attachment_carbon)
                    .copied(),
            );
        }
        groups.push(g);
    }
    for (&s, a) in &m.atoms {
        if a.symbol != "S" || consumed.contains(&s) {
            continue;
        }
        let os = m
            .get_neighbors(s)
            .iter()
            .copied()
            .filter(|&x| symbol(m, x) == "O")
            .collect::<Vec<_>>();
        let adj = m
            .get_neighbors(s)
            .iter()
            .copied()
            .filter(|x| !os.contains(x))
            .collect::<Vec<_>>();
        if !cyclic.contains(&s)
            && os.len() == 2
            && adj.len() == 2
            && os.iter().all(|&o| order(m, s, o) == 2 && m.degree(o) == 1)
        {
            let cs = adj
                .iter()
                .copied()
                .filter(|&c| carbon(m, c))
                .collect::<Vec<_>>();
            let hs = adj
                .iter()
                .copied()
                .filter(|&x| !carbon(m, x))
                .collect::<Vec<_>>();
            if cs.len() == 1 && hs.len() == 1 {
                let h = hs[0];
                let key = if m.atoms[&s].charge == 0 && m.atoms[&h].charge == 0 {
                    if symbol(m, h) == "N"
                        && !cyclic.contains(&h)
                        && m.get_neighbors(h).iter().all(|&x| {
                            x == s
                                || (!matches!(symbol(m, x), "N" | "O" | "S") && order(m, h, x) == 1)
                        })
                    {
                        Some("sulfonamide".to_owned())
                    } else if is_halogen(symbol(m, h)) && m.degree(h) == 1 {
                        Some(format!(
                            "sulfonyl_{}",
                            table_string(
                                &section("chalcogen_acyl_leaving_groups")[symbol(m, h)]["word"]
                            )
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(key) = key {
                    let atoms = os.iter().copied().chain([s, h]).collect::<Vec<_>>();
                    groups.push(group(key, true, cs[0], atoms.clone()));
                    consumed.extend(atoms);
                    continue;
                }
            }
        }
        if os.len() >= 3
            && adj.len() == 1
            && carbon(m, adj[0])
            && os.iter().filter(|&&o| order(m, s, o) == 2).count() >= 2
        {
            let singles = os
                .iter()
                .copied()
                .filter(|&o| order(m, s, o) == 1)
                .collect::<Vec<_>>();
            if cyclic.contains(&s) && singles.iter().any(|o| cyclic.contains(o)) {
                continue;
            }
            let ester = singles.iter().any(|&o| m.degree(o) == 2);
            let anion = singles.iter().any(|o| m.atoms[o].charge == -1);
            let key = if ester || anion {
                Some("sulfonate")
            } else if !singles.is_empty() {
                Some("sulfonic_acid")
            } else {
                None
            };
            if let Some(key) = key {
                let atoms = os.iter().copied().chain([s]).collect::<Vec<_>>();
                groups.push(group(key, true, adj[0], atoms.clone()));
                consumed.extend(atoms);
            }
        }
    }
    // Heterocumulenes and amidines precede nitriles and carbonyl derivatives.
    for (&c, a) in &m.atoms {
        if !a.is_carbon() || consumed.contains(&c) {
            continue;
        }
        let ns = m
            .get_neighbors(c)
            .iter()
            .copied()
            .filter(|n| symbol(m, *n) == "N" && !consumed.contains(n))
            .collect::<Vec<_>>();
        let os = m
            .get_neighbors(c)
            .iter()
            .copied()
            .filter(|n| symbol(m, *n) == "O" && !consumed.contains(n))
            .collect::<Vec<_>>();
        let ss = m
            .get_neighbors(c)
            .iter()
            .copied()
            .filter(|n| symbol(m, *n) == "S" && !consumed.contains(n))
            .collect::<Vec<_>>();
        let dn = ns.iter().copied().find(|&n| order(m, c, n) == 2);
        let double_o = os
            .iter()
            .copied()
            .find(|&n| order(m, c, n) == 2 && m.degree(n) == 1);
        let double_s = ss
            .iter()
            .copied()
            .find(|&n| order(m, c, n) == 2 && m.degree(n) == 1);
        if let Some(n) = dn {
            if let Some(e) = double_s.or(double_o)
                && let Some(attach) = m.get_neighbors(n).iter().copied().find(|x| *x != c)
            {
                let key = if double_s.is_some() {
                    "isothiocyanato"
                } else {
                    "isocyanato"
                };
                groups.push(group(key, false, attach, [c, n, e]));
                consumed.extend([c, n, e]);
                continue;
            }
            if !cyclic.contains(&c)
                && let Some(g) = amidine(m, c, n, &ns, &cyclic)
            {
                consumed.extend(
                    g.atoms_involved
                        .iter()
                        .filter(|&&x| x != g.attachment_carbon)
                        .copied(),
                );
                groups.push(g);
                continue;
            }
        }
        if let Some(n) = ns.iter().copied().find(|&n| order(m, c, n) == 3) {
            let single_s = ss.iter().copied().find(|&x| order(m, c, x) == 1);
            let single_o = os.iter().copied().find(|&x| order(m, c, x) == 1);
            if let Some(e) = single_s.or(single_o)
                && let Some(attach) = m.get_neighbors(e).iter().copied().find(|x| *x != c)
            {
                let key = if single_s.is_some() {
                    "thiocyanato"
                } else {
                    "cyanato"
                };
                groups.push(group(key, false, attach, [c, n, e]));
                consumed.extend([c, n, e]);
            }
        }
    }
    for (&c, a) in &m.atoms {
        if !a.is_carbon() || consumed.contains(&c) {
            continue;
        }
        let Some(n) = m
            .get_neighbors(c)
            .iter()
            .copied()
            .find(|n| symbol(m, *n) == "N" && !consumed.contains(n) && order(m, c, *n) == 3)
        else {
            continue;
        };
        let terminal = m.get_neighbors(n).iter().copied().find(|&e| {
            e != c
                && is_chalcogen(symbol(m, e))
                && m.atoms[&e].charge == -1
                && m.degree(e) == 1
                && order(m, n, e) == 1
        });
        if let Some(e) = terminal.filter(|_| m.atoms[&n].charge == 1) {
            let (attach, external) = external_attachment(m, c, &cyclic);
            let element = symbol(m, e);
            let key = format!("{}nitrile_{}", if external { "ring_" } else { "" }, element);
            let policy = &section("chalcogen_rule_policies")["nitrile_chalcogenide"];
            let vals = [
                ("carbo", if external { "carbo".into() } else { "".into() }),
                (
                    "class_name",
                    section("chalcogenide_class_names")[element]
                        .as_str()
                        .unwrap_or("oxide")
                        .into(),
                ),
                (
                    "additive_prefix",
                    section("chalcogen_elements")[element]["additive_prefix"]
                        .as_str()
                        .unwrap_or("oxido")
                        .into(),
                ),
            ];
            let r = dynamic_rule(
                &key,
                policy,
                Some(format_template(table_string(&policy["prefix"]), &vals)),
                Some(format_template(table_string(&policy["suffix"]), &vals)),
                vec![policy["seniority"].as_i64().unwrap_or(5), rank(element)],
                external,
                true,
            );
            let Some(ligand) = classify_chalcogen_ligand(m, n, e) else {
                continue;
            };
            let d = descriptor(
                "nitrile_chalcogenide",
                "zwitterion",
                vec![c, n],
                vec![ligand],
                attach,
                external,
            );
            groups.push(desc_group(key, Some(r), attach, [c, n, e], d));
            consumed.extend([n, e]);
            continue;
        }
        if let Some(attach) = m.get_neighbors(n).iter().copied().find(|x| *x != c) {
            groups.push(group("isocyano", false, attach, [c, n]));
            consumed.extend([c, n]);
        } else {
            let (attach, external) = external_attachment(m, c, &cyclic);
            groups.push(group(
                if external { "ring_nitrile" } else { "nitrile" },
                true,
                attach,
                [c, n],
            ));
            consumed.insert(n);
        }
    }
    for (&c, a) in &m.atoms {
        if a.is_carbon()
            && let Some(acyl_groups) = acyl_group(m, c, &consumed, &cyclic)
        {
            for g in acyl_groups {
                if g.is_principal_candidate {
                    consumed.extend(
                        g.atoms_involved
                            .iter()
                            .filter(|&&n| n != g.attachment_carbon)
                            .copied(),
                    );
                }
                groups.push(g);
            }
        }
    }
    // Remaining carbon-bound C=N nitrogens become imines/hydrazones.
    for (&n, a) in &m.atoms {
        if a.symbol != "N" || consumed.contains(&n) || cyclic.contains(&n) {
            continue;
        }
        let Some(c) = m
            .get_neighbors(n)
            .iter()
            .copied()
            .find(|&x| carbon(m, x) && order(m, n, x) == 2)
        else {
            continue;
        };
        let ns = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| symbol(m, x) == "N")
            .collect::<Vec<_>>();
        if ns
            .iter()
            .any(|&x| order(m, n, x) == 1 && is_nitro_n(m, x, n))
        {
            groups.push(group(
                if a.charge > 0 { "iminium" } else { "imine" },
                true,
                c,
                [n],
            ));
            consumed.insert(n);
            continue;
        }
        if let Some(&n2) = ns.first()
            && order(m, n, n2) == 1
        {
            let tail = amidino_tail(m, n2, &AtomSet::from([n]));
            if has_multiple(m, n2, &AtomSet::from([n])) && tail.is_empty() {
                continue;
            }
            if !cyclic.contains(&n2) {
                let (key, attach) = hydrazone_key_attachment(m, c, &cyclic, !tail.is_empty());
                let mut atoms = tail;
                atoms.extend([n, n2]);
                if attach != c {
                    atoms.insert(c);
                }
                groups.push(group(key, true, attach, atoms.clone()));
                consumed.extend(atoms);
                continue;
            }
        }
        groups.push(group(
            if a.charge > 0 { "iminium" } else { "imine" },
            true,
            c,
            [n],
        ));
        consumed.insert(n);
    }
    for (&n, a) in &m.atoms {
        if a.symbol != "N" || consumed.contains(&n) || cyclic.contains(&n) || a.charge != 0 {
            continue;
        }
        let cs = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| carbon(m, x))
            .collect::<Vec<_>>();
        let ns = m
            .get_neighbors(n)
            .iter()
            .copied()
            .filter(|&x| symbol(m, x) == "N" && m.atoms[&x].charge == 0)
            .collect::<Vec<_>>();
        if let (Some(&c), Some(&n2)) = (cs.first(), ns.first()) {
            // Python truthiness of index zero is intentionally preserved here.
            if c != 0
                && order(m, n, c) == 1
                && order(m, n, n2) == 1
                && !has_multiple(m, n2, &AtomSet::from([n]))
            {
                if !cyclic.contains(&n2) {
                    let mut g = group("hydrazine", false, c, [n, n2]);
                    g.variant = Some("prefix".into());
                    g.role = Some("nitrogen_chain".into());
                    groups.push(g);
                    consumed.extend([n, n2]);
                } else {
                    groups.push(group("amine", true, c, [n]));
                    consumed.insert(n);
                }
            }
        }
    }
    if !has_nitrogen_chains {
        for (&end, a) in &m.atoms {
            if consumed.contains(&end)
                || cyclic.contains(&end)
                || !is_chalcogen(&a.symbol)
                || m.degree(end) != 1
                || a.total_h_count == 0
                || a.radical_electrons != 0
            {
                continue;
            }
            let first = m.get_neighbors(end)[0];
            if !is_chalcogen(symbol(m, first)) || consumed.contains(&first) {
                continue;
            }
            let anchors = m
                .get_neighbors(first)
                .iter()
                .copied()
                .filter(|n| *n != end)
                .collect::<Vec<_>>();
            if anchors.len() != 1 || !carbon(m, anchors[0]) {
                continue;
            }
            let anchor = anchors[0];
            if m.get_neighbors(anchor).iter().any(|&n| {
                n != first
                    && classify_chalcogen_ligand(m, anchor, n)
                        .is_some_and(|l| l.role == "double_bonded")
            }) {
                continue;
            }
            let (Some(l), Some(t)) = (
                classify_chalcogen_ligand(m, anchor, first),
                classify_chalcogen_ligand(m, first, end),
            ) else {
                continue;
            };
            if l.role != "chalcogen_link" || t.role != "hydrogen_bearing" {
                continue;
            }
            let mut d = descriptor(
                "peroxol",
                "alcohol",
                vec![anchor],
                vec![l, t],
                anchor,
                false,
            );
            d.linker_paths = vec![vec![anchor, first, end]];
            if matches!((symbol(m, first), symbol(m, end)), ("O", "S") | ("S", "O")) {
                let mut g = group("peroxol", false, anchor, [first, end]);
                g.descriptor = Some(d);
                groups.push(g);
                continue;
            }
            let policy = section("chalcogen_peroxol_rules")
                .as_array()
                .and_then(|rows| {
                    rows.iter()
                        .find(|r| r["first"] == symbol(m, first) && r["terminal"] == symbol(m, end))
                });
            if let Some(policy) = policy {
                let key = format!("peroxol_{}_{}", symbol(m, first), symbol(m, end));
                let fp = &section("chalcogen_rule_policies")["peroxol"];
                let mut rule = dynamic_rule(
                    &key,
                    fp,
                    None,
                    policy["suffix"].as_str().map(str::to_owned),
                    vec![105, policy["rank"].as_i64().unwrap_or(0)],
                    true,
                    true,
                );
                rule.capabilities = super::nomenclature::values_strings(&policy["capabilities"]);
                groups.push(desc_group(key, Some(rule), anchor, [first, end], d));
                consumed.extend([first, end]);
            }
        }
    }
    for (&e, a) in &m.atoms {
        if consumed.contains(&e)
            || cyclic.contains(&e)
            || !is_chalcogen(&a.symbol)
            || m.degree(e) != 1
        {
            continue;
        }
        let center = m.get_neighbors(e)[0];
        let Some(l) = classify_chalcogen_ligand(m, center, e) else {
            continue;
        };
        if l.role == "double_bonded" {
            if !carbon(m, center) {
                continue;
            }
            if l.element != "O"
                && (m.atoms.values().any(|a| a.charge != 0)
                    || m.get_neighbors(center)
                        .iter()
                        .any(|&x| x != e && !carbon(m, x)))
            {
                continue;
            }
            let (attach, ring_external) = external_attachment(m, center, &cyclic);
            let external = ring_external && m.atoms[&center].total_h_count > 0;
            let derivative = if m.atoms[&center].total_h_count > 0 && !cyclic.contains(&center) {
                "aldehyde"
            } else {
                "ketone"
            };
            let Some(key) = simple_key("carbonyl", derivative, &l.element, external) else {
                continue;
            };
            let attach = if external { attach } else { center };
            let atoms = if derivative == "aldehyde" {
                vec![center, e]
            } else {
                vec![e]
            };
            let d = descriptor(
                "carbonyl",
                derivative,
                vec![center],
                vec![l],
                attach,
                external,
            );
            groups.push(desc_group(key, None, attach, atoms, d));
            consumed.insert(e);
            continue;
        }
        if !matches!(l.role.as_str(), "hydrogen_bearing" | "anionic") {
            continue;
        }
        let charge_separated = m.atoms[&center].charge > 0
            && m.get_neighbors(center).iter().any(|&n| {
                n != e
                    && classify_chalcogen_ligand(m, center, n).is_some_and(|other| {
                        other.element == l.element && other.role == "double_bonded"
                    })
            });
        let noncarbon_anion = l.role == "anionic" && l.element != "O" && !charge_separated;
        if !carbon(m, center) && !noncarbon_anion {
            continue;
        }
        let derivative = if a.charge < 0 { "anion" } else { "alcohol" };
        let Some(key) = simple_key("hydroxy", derivative, &l.element, false) else {
            continue;
        };
        let d = descriptor("hydroxy", derivative, vec![center], vec![l], center, false);
        groups.push(desc_group(key, None, center, [e], d));
        consumed.insert(e);
    }
    for (&n, a) in &m.atoms {
        if a.symbol != "N" || consumed.contains(&n) || cyclic.contains(&n) {
            continue;
        }
        let adj = m.get_neighbors(n);
        if adj.is_empty() {
            continue;
        }
        let key = if a.charge > 0 {
            "aminium"
        } else if a.charge == -1 {
            "aminide"
        } else {
            "amine"
        };
        let principal = key != "aminium" || adj.iter().all(|&x| order(m, n, x) == 1);
        for &c in adj {
            groups.push(group(key, principal, c, [n]));
        }
        consumed.insert(n);
    }
    for (&h, a) in &m.atoms {
        let key = match a.symbol.as_str() {
            "F" => "fluoro",
            "Cl" => "chloro",
            "Br" => "bromo",
            "I" => "iodo",
            _ => continue,
        };
        // Upstream divergence: neutral halogen prefixes do not credit formal charges or attached hydrogens.
        if !consumed.contains(&h)
            && !cyclic.contains(&h)
            && m.degree(h) == 1
            && a.charge == 0
            && a.total_h_count == 0
        {
            groups.push(group(key, false, m.get_neighbors(h)[0], [h]));
            consumed.insert(h);
        }
    }
    // A cation in the same covalent component as an anionic group is a prefix.
    let anions: AtomSet = groups
        .iter()
        .filter(|g| {
            matches!(
                g.key.as_str(),
                "olate" | "thiolate" | "aminide" | "carboxylate" | "ring_carboxylate" | "sulfonate"
            )
        })
        .map(|g| g.attachment_carbon)
        .collect();
    if !anions.is_empty() {
        for g in &mut groups {
            if g.is_principal_candidate
                && matches!(g.key.as_str(), "aminium" | "iminium" | "diazonio")
                && anions.iter().any(|&a| {
                    a == g.attachment_carbon
                        || connected_without(m, g.attachment_carbon, a, &AtomSet::new())
                })
            {
                g.is_principal_candidate = false;
            }
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::super::graph_io::read_smiles;
    use super::*;
    #[test]
    fn detects_group_priority_and_exocyclic_attachment() {
        for (smiles, key) in [
            ("CCO", "alcohol"),
            ("CC(=O)O", "carboxylic_acid"),
            ("CC(=O)[O-]", "carboxylate"),
            ("CC(=O)OC", "ester"),
            ("CC(N)=O", "amide"),
            ("NC(N)=O", "urea"),
            ("NC(N)=S", "thiourea"),
            ("CC(=O)NN", "hydrazide"),
            ("CC(=N)N", "amidine"),
            ("NC(N)=N", "guanidine"),
            ("CC#N", "nitrile"),
            ("CC=O", "aldehyde"),
            ("CC(C)=O", "ketone"),
            ("CC=N", "imine"),
            ("CC=NN", "aldehyde_hydrazone"),
            ("CN", "amine"),
            ("C[N+](=O)[O-]", "nitro"),
            ("CS(=O)(=O)N", "sulfonamide"),
            ("CS(=O)(=O)O", "sulfonic_acid"),
            ("CC(=O)Cl", "acid_chloride"),
            ("CC(=O)OC(C)=O", "anhydride"),
            ("COO", "peroxol_O_O"),
            ("Cc1ccc(C(=O)SC(=S)SCC#N)cc1", "anhydride_O_S_S"),
            ("C1CCCCC1C(=O)O", "ring_carboxylic_acid"),
            ("C1CCCCC1C#N", "ring_nitrile"),
            ("C1CCCCC1C=O", "ring_aldehyde"),
        ] {
            let mol = read_smiles(smiles).unwrap();
            let groups = perceive_groups(&mol);
            assert!(
                groups.iter().any(|g| g.key == key),
                "{smiles}: wanted {key}, got {:?}",
                groups.iter().map(|g| &g.key).collect::<Vec<_>>()
            );
        }
        let groups = perceive_groups(&read_smiles("C[N+](C)(C)CC(=O)[O-]").unwrap());
        assert!(
            groups
                .iter()
                .any(|g| g.key == "aminium" && !g.is_principal_candidate)
        );
        assert!(
            groups
                .iter()
                .any(|g| g.key == "carboxylate" && g.is_principal_candidate)
        );
    }
}
