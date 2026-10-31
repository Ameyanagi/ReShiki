//! Small grammar utilities shared by the plain-name formatters.
pub fn parse_locant(text: &str) -> (u8, usize, String) {
    let before = text.split('(').next().unwrap_or(text);
    let digits: String = before.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty()
        && before[digits.len()..]
            .chars()
            .all(|c| c.is_ascii_alphabetic())
    {
        return (
            1,
            digits.parse().unwrap_or(0),
            before[digits.len()..].to_owned(),
        );
    }
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !digits.is_empty() {
        return (1, digits.parse().unwrap_or(0), text.to_owned());
    }
    (0, 0, text.to_owned())
}
pub fn needs_hyphen(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if right.starts_with(|c: char| c.is_ascii_digit())
        || left.ends_with(|c: char| c.is_ascii_digit())
    {
        return true;
    }
    if right
        .strip_prefix(['N', 'O', 'S', 'P'])
        .is_some_and(|tail| tail.trim_start_matches('\'').starts_with([',', '-']))
    {
        return true;
    }
    // A stereo descriptor such as `(1R,2E)` opening the right-hand side.
    right
        .strip_prefix('(')
        .and_then(|s| s.split_once(')'))
        .is_some_and(|(inner, _)| {
            inner.split(',').all(|term| {
                let letters = term.trim_start_matches(|c: char| c.is_ascii_digit());
                letters.len() < term.len()
                    && letters.ends_with(['R', 'S', 'E', 'Z'])
                    && letters
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == '\'')
            })
        })
}
pub fn is_fully_enclosed(text: &str) -> bool {
    if !text.starts_with('(') || !text.ends_with(')') {
        return false;
    }
    let mut depth = 0_i32;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => (),
        }
        if depth == 0 && i < text.len() - 1 {
            return false;
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::needs_hyphen;

    #[test]
    fn only_a_leading_stereo_descriptor_takes_a_hyphen() {
        assert!(needs_hyphen("methyl", "(1R,2E)-but-2-enyl"));
        assert!(needs_hyphen("methyl", "(5'E)-1'-butyl"));
        assert!(!needs_hyphen(
            "(cyclopentyl)",
            "(4-(((3R)-thiolan-3-yl)sulfanyl)pyridin-3-yl)"
        ));
        assert!(!needs_hyphen("methyl", "SO-x"));
    }
}
