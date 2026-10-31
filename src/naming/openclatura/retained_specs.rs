//! Retained parent attachment spelling and locant policy.
use super::nomenclature::section;
#[derive(Clone, Debug, Default)]
pub struct AttachmentLocantPolicy {
    pub print_substituent_locant: bool,
    pub use_parent_attachment_equivalence: bool,
}
#[derive(Clone, Debug)]
pub struct RetainedParentSpec {
    pub substituent_stem: Option<String>,
    pub substituent_terminal: Option<String>,
    pub attachment_policy: AttachmentLocantPolicy,
}
pub fn retained_parent_spec(name: Option<&str>) -> Option<RetainedParentSpec> {
    let name = name.filter(|n| !n.is_empty())?;
    let data = &section("retained_substituent_stems")[name];
    let stem = data[0].as_str().map(str::to_owned);
    let terminal = data[1].as_str().map(str::to_owned);
    let normalized = stem.as_deref().unwrap_or(name).to_lowercase();
    let normalized = normalized.trim_end_matches('e');
    let needs_locant = section("ambiguous_connection_substituent_stems")
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .any(|s| normalized.ends_with(s));
    Some(RetainedParentSpec {
        substituent_stem: stem,
        substituent_terminal: terminal,
        attachment_policy: AttachmentLocantPolicy {
            print_substituent_locant: needs_locant,
            use_parent_attachment_equivalence: true,
        },
    })
}
