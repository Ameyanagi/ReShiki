//! Data-backed P-23/P-24 ring descriptor rendering.
use super::nomenclature::section;
use super::rules::multipliers;

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

/// The bridge count is one less than the number of independent cycles.
pub fn render_von_baeyer_descriptor(bridge_count: usize, descriptor_body: &str) -> String {
    format!("{}{descriptor_body}", von_baeyer_kind(bridge_count + 1))
}

pub fn von_baeyer_kind(cycle_count: usize) -> String {
    section("polycycle_prefixes")[cycle_count.saturating_sub(1).to_string()]
        .as_str()
        .map_or_else(
            || format!("{}cyclo", multipliers::basic(cycle_count)),
            str::to_owned,
        )
}

pub fn von_baeyer_cycle_count(descriptor: Option<&str>) -> Option<usize> {
    let prefix = descriptor?.split_once("cyclo[")?.0;
    if let Some(count) = section("polycycle_prefixes")
        .as_object()
        .and_then(|prefixes| {
            prefixes.iter().find_map(|(count, rendered)| {
                (rendered.as_str() == Some(&format!("{prefix}cyclo")))
                    .then(|| count.parse::<usize>().ok().map(|count| count + 1))
                    .flatten()
            })
        })
    {
        return Some(count);
    }
    (2..=1000).find(|&count| multipliers::basic(count) == prefix)
}

pub fn is_von_baeyer_descriptor(descriptor: Option<&str>) -> bool {
    von_baeyer_cycle_count(descriptor).is_some_and(|count| count >= 3)
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
        assert_eq!(von_baeyer_kind(2), "bicyclo");
        assert_eq!(von_baeyer_cycle_count(Some("bicyclo[2.2.1]")), Some(2));
        assert_eq!(
            render_von_baeyer_descriptor(2, "[3.2.2.0^{1,5}]"),
            "tricyclo[3.2.2.0^{1,5}]"
        );
        assert!(is_von_baeyer_descriptor(Some("undecacyclo[1.1.1]")));
    }
}
