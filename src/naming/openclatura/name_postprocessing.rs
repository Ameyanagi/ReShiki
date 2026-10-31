//! Ordered data-driven final name rewrites.
use super::nomenclature::section;
pub fn apply_data_postprocessing(name: &str) -> String {
    let mut name = name.to_owned();
    if let Some(rules) = section("postprocess_literal_replacements").as_array() {
        for rule in rules {
            if let (Some(from), Some(to)) = (rule["pattern"].as_str(), rule["replacement"].as_str())
            {
                name = name.replace(from, to);
            }
        }
    }
    name
}
