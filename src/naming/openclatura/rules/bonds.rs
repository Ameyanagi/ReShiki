use super::multipliers;
#[derive(Clone, Copy, Debug)]
pub struct BondType {
    pub suffix: &'static str,
    pub needs_locant: bool,
}
fn get(key: &str) -> Option<BondType> {
    Some(match key {
        "single" => BondType {
            suffix: "an",
            needs_locant: false,
        },
        "double" => BondType {
            suffix: "en",
            needs_locant: true,
        },
        "triple" => BondType {
            suffix: "yn",
            needs_locant: true,
        },
        _ => return None,
    })
}
pub fn unsaturation_infix(key: &str, count: usize) -> String {
    let Some(b) = get(key) else {
        return String::new();
    };
    if count == 1 {
        b.suffix.into()
    } else {
        assert!(b.needs_locant);
        format!("a{}{}", multipliers::basic(count), b.suffix)
    }
}
