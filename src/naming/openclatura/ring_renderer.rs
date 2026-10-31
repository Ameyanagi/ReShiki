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

pub fn von_baeyer_cycle_count(descriptor: Option<&str>) -> Option<usize> {
    let prefix = descriptor?.split_once("cyclo[")?.0;
    if !prefix.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    let rendered = format!("{prefix}cyclo");
    if let Some(count) =
        section("polycycle_prefixes")
            .as_object()?
            .iter()
            .find_map(|(count, term)| {
                (term.as_str() == Some(&rendered))
                    .then(|| count.parse::<usize>().ok())
                    .flatten()
            })
    {
        return Some(count + 1);
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
        assert!(!is_von_baeyer_descriptor(Some("dispiro[3.1.3.1]")));
        assert!(!is_von_baeyer_descriptor(Some("bicyclo[2.2.1]")));
        assert!(is_von_baeyer_descriptor(Some("tricyclo[3.2.1.0]")));
    }
}
