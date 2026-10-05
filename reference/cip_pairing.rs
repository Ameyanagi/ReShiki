#[path = "support/cip_rule_case.rs"]
mod cip_rule_case;
#[test]
fn descriptor_pair_rules_match_native_rdkit() -> anyhow::Result<()> {
    cip_rule_case::check("cip_pairing_reference.py", "cip-pairing", 5_000)
}
