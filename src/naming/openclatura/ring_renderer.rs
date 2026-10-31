//! Data-backed P-23/P-24 ring descriptor rendering.
use super::nomenclature::section;

#[allow(
    clippy::expect_used,
    reason = "Ring descriptor kinds come from the checked-in registry."
)]
pub fn render_ring_descriptor(kind: &str, descriptor_numbers: &[usize]) -> String {
    let template = section("ring_descriptor_templates")[kind]
        .as_str()
        .expect("checked-in ring descriptor template");
    let numbers = descriptor_numbers
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".");
    template.replace("{numbers}", &numbers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_kinds_keep_their_distinct_naming_systems() {
        assert_eq!(render_ring_descriptor("spiro", &[3, 5]), "spiro[3.5]");
        assert_eq!(
            render_ring_descriptor("dispiro", &[3, 1, 3, 1]),
            "dispiro[3.1.3.1]"
        );
    }
}
