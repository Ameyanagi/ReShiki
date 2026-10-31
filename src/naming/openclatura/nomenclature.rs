//! Nomenclature tables transcribed from Openclatura's checked-in rule registry.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

// The immutable embedded registry is checked in with the port and verified by the oracle tests.
#[allow(clippy::expect_used)]
pub static ALL_NAMER_RULES: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("data/namer_rules.json"))
        .expect("checked-in nomenclature data")
});
pub fn section(name: &str) -> &'static Value {
    &ALL_NAMER_RULES[name]["values"]
}
// Callers select only fixed schema sections in the immutable embedded registry.
#[allow(clippy::expect_used)]
pub fn table_rows(name: &str) -> &'static [Value] {
    section(name)
        .as_array()
        .expect("checked-in table section is an array")
}
// This accessor is reserved for required string columns in the immutable rule tables.
#[allow(clippy::expect_used)]
pub fn table_string(value: &Value) -> &str {
    value.as_str().expect("checked-in table column is a string")
}
pub fn string_map(name: &str) -> BTreeMap<String, String> {
    values_string_map(section(name))
}
fn values_string_map(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
        .collect()
}
pub fn string_set(name: &str) -> BTreeSet<String> {
    values_strings(section(name))
}
pub fn values_strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MultiSuffixTemplate {
    pub multiplier_positions: Vec<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionalGroupRule {
    pub key: String,
    pub role: String,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub multi_suffix: Option<MultiSuffixTemplate>,
    pub suffix_multiplier_positions: Vec<usize>,
    pub seniority: Option<Vec<i64>>,
    pub suffix_with_locant: bool,
    pub needs_locant: bool,
    pub families: BTreeSet<String>,
    pub capabilities: BTreeSet<String>,
    pub citation_suffixes: BTreeMap<String, String>,
}
impl FunctionalGroupRule {
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }
    pub fn suffix_for_citation(&self, mode: Option<&str>) -> Option<&str> {
        mode.and_then(|m| self.citation_suffixes.get(m).map(String::as_str))
            .or(self.suffix.as_deref())
    }
}
#[derive(Clone, Debug, Default)]
pub struct FunctionalGroupRules {
    pub by_key: BTreeMap<String, FunctionalGroupRule>,
}
impl FunctionalGroupRules {
    pub fn get(&self, key: &str) -> Option<&FunctionalGroupRule> {
        self.by_key.get(key)
    }
    pub fn prefix_for(&self, key: &str) -> Option<&str> {
        self.get(key)?.prefix.as_deref()
    }
    pub fn cited_prefix_for(&self, key: &str) -> Option<String> {
        let r = self.get(key)?;
        let p = r.prefix.as_ref()?;
        Some(if r.has_capability("acyl_leaving_group") {
            format!("({p})")
        } else {
            p.clone()
        })
    }
    pub fn direct_subgraph_prefix_for(&self, key: &str) -> Option<&str> {
        let r = self.get(key)?;
        (r.role == "prefix" || r.families.contains("direct_group"))
            .then_some(r.prefix.as_deref())
            .flatten()
    }
    pub fn keys_with_family(&self, family: &str) -> BTreeSet<String> {
        self.by_key
            .iter()
            .filter(|(_, r)| r.families.contains(family))
            .map(|(k, _)| k.clone())
            .collect()
    }
}
const CAPABILITIES: &[&str] = &[
    "acid_halide",
    "acyl_leaving_group",
    "amide_like",
    "anhydride",
    "blocks_structural_parent",
    "central_acid",
    "chain_external_carbonyl",
    "element_locanted_suffix",
    "ester_like",
    "front_modifier",
    "hydrazide",
    "promotes_member_groups",
    "urea",
];
fn derived_families(key: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (family, table) in [
        ("chain_external_carbonyl", "chain_external_carbonyl_groups"),
        ("prefix_skip", "prefix_groups_to_skip"),
        ("ester_like", "ester_like_prefix_groups"),
        ("peroxy_ester", "peroxy_ester_groups"),
        ("amide_like", "amide_like_prefix_groups"),
        ("carboxy_prefix", "carboxy_prefix_groups"),
        ("cyano_prefix", "cyano_prefix_groups"),
        ("peroxy_acid", "peroxy_acid_prefix_groups"),
        ("sulfonyl", "sulfonyl_prefix_groups"),
        ("front_modifier", "front_modifier_principal_groups"),
        ("hydrazone", "hydrazone_principal_groups"),
    ] {
        if section(table)
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(key)))
        {
            out.insert(family.to_owned());
        }
    }
    for (family, table) in [
        ("acid_halide", "acid_halide_prefixes"),
        ("direct_prefix", "direct_prefix_groups"),
        ("direct_group", "direct_group_prefixes"),
    ] {
        if section(table).get(key).is_some() {
            out.insert(family.to_owned());
        }
    }
    out
}
pub fn rule_from_value(key: &str, row: &Value) -> FunctionalGroupRule {
    let positions = row["multi_suffix"]["multiplier_positions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_u64)
                .map(|x| x as usize)
                .collect()
        })
        .unwrap_or_else(|| vec![0]);
    let families = row
        .get("families")
        .map_or_else(|| derived_families(key), values_strings);
    let mut capabilities = values_strings(&row["capabilities"]);
    capabilities.extend(
        families
            .iter()
            .filter(|s| CAPABILITIES.contains(&s.as_str()))
            .cloned(),
    );
    let seniority = row["seniority"].as_i64().map(|x| vec![x]).or_else(|| {
        row["seniority"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_i64).collect())
    });
    FunctionalGroupRule {
        key: key.to_owned(),
        role: row["role"].as_str().unwrap_or("principal").to_owned(),
        prefix: row["prefix"].as_str().map(str::to_owned),
        suffix: row["suffix"].as_str().map(str::to_owned),
        multi_suffix: (!row["multi_suffix"].is_null()).then(|| MultiSuffixTemplate {
            multiplier_positions: positions.clone(),
        }),
        suffix_multiplier_positions: positions,
        seniority,
        suffix_with_locant: row["suffix_with_locant"].as_bool().unwrap_or(false),
        needs_locant: row["needs_locant"].as_bool().unwrap_or(true),
        families,
        capabilities,
        citation_suffixes: values_string_map(&row["citation_suffixes"]),
    }
}
// All required fields below belong to the immutable checked-in registry schema.
#[allow(clippy::expect_used)]
fn functional_groups() -> FunctionalGroupRules {
    let mut by_key = section("functional_groups")
        .as_object()
        .expect("functional-group table")
        .iter()
        .map(|(k, v)| (k.clone(), rule_from_value(k, v)))
        .collect::<BTreeMap<_, _>>();
    let policy = &section("chalcogen_rule_policies")["acyl"]["acid_halide"];
    for item in section("chalcogen_acyl_leaving_groups")
        .as_object()
        .into_iter()
        .flatten()
        .map(|(_, v)| v)
    {
        for (external, key_field, suffix) in [
            (false, "chain_key", "oyl"),
            (true, "external_key", "carbonyl"),
        ] {
            let key = table_string(&item[key_field]);
            let word = table_string(&item["word"]);
            let prefix = table_string(&item["prefix"]);
            let mut r = rule_from_value(key, policy);
            r.role = "principal".into();
            r.prefix = Some(format!("{prefix}carbonyl"));
            r.suffix = Some(format!("{suffix} {word}"));
            r.seniority = Some(vec![
                policy["seniority"]
                    .as_i64()
                    .expect("checked-in acyl seniority"),
                item["rank"]
                    .as_i64()
                    .expect("checked-in leaving-group rank"),
            ]);
            r.multi_suffix = Some(MultiSuffixTemplate {
                multiplier_positions: vec![0, 1],
            });
            r.suffix_multiplier_positions = vec![0, 1];
            r.suffix_with_locant = external;
            by_key.insert(key.to_owned(), r);
        }
    }
    FunctionalGroupRules { by_key }
}
#[derive(Debug, Default)]
pub struct RetainedNameRules {
    pub substituent_stems: BTreeMap<String, (String, String)>,
    pub chain_functional_parents: BTreeMap<(usize, String, usize), String>,
    pub principal_citations: BTreeMap<(String, String), String>,
}
#[derive(Debug, Default)]
pub struct HeteroatomRules {
    pub alkyl_oxy_prefixes: BTreeMap<String, String>,
    pub simple_sulfanyl_prefixes: BTreeSet<String>,
    pub simple_selanyl_prefixes: BTreeSet<String>,
    pub retained_sulfonyl_group_acids: BTreeMap<String, String>,
}
#[derive(Debug, Default)]
pub struct PrefixRules {
    pub peroxy_ester_groups: BTreeSet<String>,
    pub amide_like_groups: BTreeSet<String>,
}
#[derive(Debug, Default)]
pub struct ComponentRules {
    pub mononuclear_parent_hydrides: BTreeMap<String, String>,
    pub retained_homonuclear_chain_names: BTreeMap<String, String>,
    pub replacement_parent_oxoacid_specs: Vec<Value>,
}
#[derive(Debug, Default)]
pub struct IonRules {
    pub single_atom_cations: BTreeSet<String>,
    pub single_atom_anions: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default)]
pub struct ParentChargeSuffixRule {
    pub suffix: String,
}
#[derive(Debug, Default)]
pub struct ChargeRules {
    pub retained_ionic_n_parents: BTreeMap<String, String>,
    pub saturated_n_ring_ionic_parents: BTreeMap<usize, String>,
    pub parent_charge_suffixes: BTreeMap<String, ParentChargeSuffixRule>,
    pub replacement_charge_prefixes: BTreeMap<String, String>,
    pub heteroatom_charge_prefixes: BTreeMap<String, String>,
    pub anion_suffix_placements: Vec<Value>,
}
#[derive(Debug, Default)]
pub struct AssemblyRules {
    pub replacement_prefix_order: BTreeMap<String, usize>,
    pub unsaturation_order: BTreeMap<String, usize>,
    pub substituent_attachment_suffixes: BTreeMap<String, String>,
    pub ambiguous_connection_substituent_stems: BTreeSet<String>,
    pub suffix_nitrogen_markers: Vec<String>,
}
#[derive(Debug)]
pub struct NomenclatureRegistry {
    pub retained: RetainedNameRules,
    pub heteroatoms: HeteroatomRules,
    pub prefixes: PrefixRules,
    pub components: ComponentRules,
    pub ions: IonRules,
    pub charges: ChargeRules,
    pub assembly: AssemblyRules,
    pub functional_groups: FunctionalGroupRules,
}
fn rows(name: &str) -> &'static [Value] {
    section(name)
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}
fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_owned()
}
fn usize_map(name: &str) -> BTreeMap<String, usize> {
    section(name)
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| v.as_u64().map(|n| (k.clone(), n as usize)))
        .collect()
}
pub static RULES: LazyLock<NomenclatureRegistry> = LazyLock::new(|| NomenclatureRegistry {
    functional_groups: functional_groups(),
    retained: RetainedNameRules {
        substituent_stems: section("retained_substituent_stems")
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| {
                Some((
                    k.clone(),
                    (v[0].as_str()?.to_owned(), v[1].as_str()?.to_owned()),
                ))
            })
            .collect(),
        chain_functional_parents: rows("retained_chain_functional_parents")
            .iter()
            .map(|v| {
                (
                    (
                        v["length"].as_u64().unwrap_or(0) as usize,
                        s(v, "group"),
                        v["count"].as_u64().unwrap_or(0) as usize,
                    ),
                    s(v, "name"),
                )
            })
            .collect(),
        principal_citations: rows("retained_principal_citations")
            .iter()
            .map(|v| ((s(v, "mode"), s(v, "name")), s(v, "citation")))
            .collect(),
    },
    heteroatoms: HeteroatomRules {
        alkyl_oxy_prefixes: string_map("alkyl_oxy_prefixes"),
        simple_sulfanyl_prefixes: string_set("simple_sulfanyl_prefixes"),
        simple_selanyl_prefixes: string_set("simple_selanyl_prefixes"),
        retained_sulfonyl_group_acids: rows("retained_sulfonyl_group_names")
            .iter()
            .filter(|v| v["acid"].is_string())
            .map(|v| (s(v, "acid"), s(v, "name")))
            .collect(),
    },
    prefixes: PrefixRules {
        peroxy_ester_groups: string_set("peroxy_ester_groups"),
        amide_like_groups: string_set("amide_like_prefix_groups"),
    },
    components: ComponentRules {
        mononuclear_parent_hydrides: string_map("mononuclear_parent_hydrides"),
        retained_homonuclear_chain_names: string_map("retained_homonuclear_chain_names"),
        replacement_parent_oxoacid_specs: rows("replacement_parent_oxoacid_specs").to_vec(),
    },
    ions: IonRules {
        single_atom_cations: string_set("single_atom_cations"),
        single_atom_anions: string_map("single_atom_anions"),
    },
    charges: ChargeRules {
        retained_ionic_n_parents: string_map("retained_ionic_n_parents"),
        saturated_n_ring_ionic_parents: string_map("saturated_n_ring_ionic_parents")
            .into_iter()
            .filter_map(|(k, v)| Some((k.parse().ok()?, v)))
            .collect(),
        parent_charge_suffixes: section("parent_charge_suffixes")
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| {
                (
                    k.clone(),
                    ParentChargeSuffixRule {
                        suffix: s(v, "suffix"),
                    },
                )
            })
            .collect(),
        replacement_charge_prefixes: string_map("replacement_charge_prefixes"),
        heteroatom_charge_prefixes: string_map("heteroatom_charge_prefixes"),
        anion_suffix_placements: rows("anion_suffix_placements").to_vec(),
    },
    assembly: AssemblyRules {
        replacement_prefix_order: usize_map("replacement_prefix_order"),
        unsaturation_order: usize_map("unsaturation_order"),
        substituent_attachment_suffixes: string_map("substituent_attachment_suffixes"),
        ambiguous_connection_substituent_stems: string_set(
            "ambiguous_connection_substituent_stems",
        ),
        suffix_nitrogen_markers: rows("suffix_nitrogen_markers")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    },
});
