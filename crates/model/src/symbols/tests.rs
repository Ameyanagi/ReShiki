use super::*;

/// Type `keys` one at a time, expanding a shortcut after each, as the
/// caption editor does.
fn typed(keys: &str) -> String {
    let mut text = String::new();
    for key in keys.chars() {
        text.push(key);
        if let Some((len, symbol)) = expand(&text) {
            text.truncate(text.len() - len);
            text.push_str(symbol);
        }
    }
    text
}

#[test]
fn every_shortcut_types_its_symbol() {
    for symbol in GROUPS.iter().flat_map(|group| group.symbols) {
        if let Some(shortcut) = symbol.shortcut {
            assert_eq!(typed(shortcut), symbol.text, "{shortcut}");
            assert!(shortcut.is_ascii(), "{shortcut} must be typeable");
        }
    }
}

#[test]
fn yen_prefixes_type_every_backslash_command_including_combined_symbols() {
    for symbol in GROUPS.iter().flat_map(|group| group.symbols) {
        if let Some(shortcut) = symbol.shortcut.filter(|shortcut| shortcut.contains('\\')) {
            for prefix in ["¥", "￥"] {
                let keys = shortcut.replace('\\', prefix);
                assert_eq!(typed(&keys), symbol.text, "{keys}");
            }
        }
    }
}

#[test]
fn command_expansion_returns_the_actual_utf8_suffix_length() {
    for prefix in ["\\", "¥", "￥"] {
        let command = format!("{prefix}alpha");
        let text = format!("反応 Δ: {command}");
        let (len, symbol) = expand(&text).unwrap();
        assert_eq!(len, command.len());
        assert_eq!(symbol, "α");
        assert_eq!(&text[text.len() - len..], command);
        assert_eq!(typed(&format!("反応 Δ: {command}, B")), "反応 Δ: α, B");
        assert_eq!(expand(&format!("{prefix}Alpha")), None, "case stays exact");
    }
}

#[test]
fn yen_currency_and_unknown_commands_stay_literal() {
    for text in [
        "¥",
        "￥",
        "価格 ¥500 / ￥３００",
        "¥unknown",
        "￥unknown",
        "\\unknown",
        "¥ alpha",
        "￥ alpha",
        "¥αλφα",
        "￥ａｌｐｈａ",
    ] {
        assert_eq!(typed(text), text);
        assert_eq!(expand(text), None);
    }
    assert_eq!(expand("¥alphabet"), None);
    assert_eq!(expand("￥alphabet"), None);
}

#[test]
fn longest_shortcut_wins_and_text_around_it_stays() {
    assert_eq!(typed("A <=> B"), "A ⇌ B");
    assert_eq!(typed("A --> B"), "A ⟶ B");
    assert_eq!(typed("A <-> B"), "A ↔ B");
    assert_eq!(typed("25 \\degC +- 2"), "25 °C ± 2");
    assert_eq!(typed("h\\nu, \\Delta"), "hν, Δ");
    assert_eq!(typed("Na^+ Cl^-"), "Na⁺ Cl⁻");
}

#[test]
fn ordinary_chemistry_text_stays_as_typed() {
    for text in [
        "CH3-CH2-OH",
        "1-(2-naphthyl)ethanol",
        "pH = 7",
        "x < 5, y > 3",
        "Pd/C, H2 (1 atm)",
        "R-NH2 + HCl",
        "\\",
    ] {
        assert_eq!(typed(text), text);
    }
}

#[test]
fn symbols_are_listed_once_per_group_and_found_by_text() {
    for group in GROUPS {
        for (i, symbol) in group.symbols.iter().enumerate() {
            assert!(
                !group.symbols[..i].iter().any(|s| s.text == symbol.text),
                "{} twice in {}",
                symbol.text,
                group.name
            );
            assert_eq!(find(symbol.text).map(|s| s.text), Some(symbol.text));
        }
    }
    assert_eq!(find("not a symbol"), None);
    // Names label the picker's buttons and key their accessibility ids.
    let names: Vec<_> = GROUPS
        .iter()
        .flat_map(|g| g.symbols)
        .map(|s| s.name)
        .collect();
    for (i, name) in names.iter().enumerate() {
        assert!(!names[..i].contains(name), "{name} twice");
    }
}
