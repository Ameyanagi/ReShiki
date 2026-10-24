//! The fonts the browser editor kept from earlier visits, by SHA-256, in the
//! order they are added as fallbacks once text needs them.

/// Forget `hash`, which is added now and so not read later, keeping the
/// others in order: their order picks which fallback draws a character.
/// Whether it was kept.
pub(crate) fn forget(kept: &mut Vec<String>, hash: &str) -> bool {
    let index = kept.iter().position(|kept| kept == hash);
    index.map(|index| kept.remove(index)).is_some()
}

#[cfg(test)]
mod tests {
    use super::forget;

    #[test]
    fn forgetting_a_kept_font_keeps_the_fallback_order() {
        let mut kept = ["fixture", "naskh", "sans-arabic"]
            .map(String::from)
            .to_vec();
        assert!(forget(&mut kept, "fixture"));
        assert_eq!(kept, ["naskh", "sans-arabic"]);
        assert!(!forget(&mut kept, "fixture"));
        assert_eq!(kept, ["naskh", "sans-arabic"]);
    }
}
