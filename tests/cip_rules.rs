#[path = "support/cip_rule_case.rs"]
mod cip_rule_case;
#[test]
fn comparisons_priorities_and_groups_match_native_rdkit() -> anyhow::Result<()> {
    cip_rule_case::check("cip_rules_reference.py", "cip-rules", 5_000)
}
