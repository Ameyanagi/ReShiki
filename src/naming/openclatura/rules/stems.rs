use super::multipliers;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stem {
    pub length: usize,
    pub stem: String,
    pub retained: bool,
}
const STEMS: [&str; 31] = [
    "", "meth", "eth", "prop", "but", "pent", "hex", "hept", "oct", "non", "dec", "undec", "dodec",
    "tridec", "tetradec", "pentadec", "hexadec", "heptadec", "octadec", "nonadec", "icos",
    "henicos", "docos", "tricos", "tetracos", "pentacos", "hexacos", "heptacos", "octacos",
    "nonacos", "triacont",
];
pub fn get(length: usize) -> Stem {
    assert!((1..=1000).contains(&length));
    Stem {
        length,
        stem: if length <= 30 {
            STEMS[length].to_owned()
        } else {
            multipliers::numerical_term(length)
                .trim_end_matches('a')
                .to_owned()
        },
        retained: length <= 4,
    }
}
pub fn stem_for(length: usize) -> String {
    get(length).stem
}
pub fn terminal_stem(name: &str) -> Option<Stem> {
    (1..=1000)
        .map(get)
        .filter(|s| name.ends_with(&format!("{}yl", s.stem)))
        .max_by_key(|s| s.stem.len())
}
