//! The symbol picker's table and the typed shortcuts that insert its symbols.
//! A shortcut turns into its symbol as soon as its last character is typed,
//! so no shortcut may start another one; the longest one that ends at the
//! cursor wins, which lets `<=>` beat `=>`.

#[derive(Debug, PartialEq, Eq)]
pub struct Symbol {
    pub text: &'static str,
    pub name: &'static str,
    /// What to type for it; a shortcut that combines others, such as `e^-`,
    /// works because each part expands in turn.
    pub shortcut: Option<&'static str>,
}

pub struct Group {
    pub name: &'static str,
    pub symbols: &'static [Symbol],
}

const fn s(text: &'static str, name: &'static str, shortcut: &'static str) -> Symbol {
    Symbol {
        text,
        name,
        shortcut: if shortcut.is_empty() {
            None
        } else {
            Some(shortcut)
        },
    }
}

/// Chemistry first, in the owner's order.
pub const GROUPS: &[Group] = &[
    Group {
        name: "Arrows",
        symbols: &[
            s("→", "Right arrow", "->"),
            s("⇌", "Equilibrium", "<=>"),
            s("⇄", "Exchange arrows", "\\rightleftarrows"),
            s("↑", "Gas evolved", "\\uparrow"),
            s("↓", "Precipitate", "\\downarrow"),
            s("⟶", "Long arrow", "-->"),
            s("↻", "Clockwise", "\\circlearrowright"),
            s("↺", "Counterclockwise", "\\circlearrowleft"),
            s("←", "Left arrow", "<--"),
            s("↔", "Resonance", "<->"),
            s("⇒", "Implies", "=>"),
        ],
    },
    Group {
        name: "Math",
        symbols: &[
            s("±", "Plus or minus", "+-"),
            s("×", "Times", "\\times"),
            s("°", "Degree", "\\deg"),
            s("‡", "Transition state", "\\ddagger"),
            s("≈", "About", "~="),
            s("·", "Middle dot", "\\cdot"),
            s("÷", "Divide", "\\div"),
            s("≠", "Not equal", "\\neq"),
            s("≤", "Less or equal", "\\leq"),
            s("≥", "Greater or equal", "\\geq"),
            s("∝", "Proportional to", "\\propto"),
            s("∞", "Infinity", "\\infty"),
            s("√", "Square root", "\\sqrt"),
            s("†", "Dagger", "\\dagger"),
        ],
    },
    Group {
        name: "Greek",
        symbols: &[
            s("α", "Alpha", "\\alpha"),
            s("β", "Beta", "\\beta"),
            s("γ", "Gamma", "\\gamma"),
            s("δ", "Delta", "\\delta"),
            s("ε", "Epsilon", "\\epsilon"),
            s("ζ", "Zeta", "\\zeta"),
            s("η", "Eta", "\\eta"),
            s("θ", "Theta", "\\theta"),
            s("ι", "Iota", "\\iota"),
            s("κ", "Kappa", "\\kappa"),
            s("λ", "Lambda", "\\lambda"),
            s("μ", "Mu", "\\mu"),
            s("ν", "Nu", "\\nu"),
            s("ξ", "Xi", "\\xi"),
            s("π", "Pi", "\\pi"),
            s("ρ", "Rho", "\\rho"),
            s("σ", "Sigma", "\\sigma"),
            s("τ", "Tau", "\\tau"),
            s("υ", "Upsilon", "\\upsilon"),
            s("φ", "Phi", "\\phi"),
            s("χ", "Chi", "\\chi"),
            s("ψ", "Psi", "\\psi"),
            s("ω", "Omega", "\\omega"),
            s("Γ", "Capital gamma", "\\Gamma"),
            s("Δ", "Capital delta", "\\Delta"),
            s("Θ", "Capital theta", "\\Theta"),
            s("Λ", "Capital lambda", "\\Lambda"),
            s("Ξ", "Capital xi", "\\Xi"),
            s("Π", "Capital pi", "\\Pi"),
            s("Σ", "Capital sigma", "\\Sigma"),
            s("Φ", "Capital phi", "\\Phi"),
            s("Ψ", "Capital psi", "\\Psi"),
            s("Ω", "Capital omega", "\\Omega"),
        ],
    },
    Group {
        name: "Charges",
        symbols: &[
            s("⊕", "Circled plus", "\\oplus"),
            s("⊖", "Circled minus", "\\ominus"),
            s("•", "Radical", "\\bullet"),
            s("⁺", "Superscript plus", "^+"),
            s("⁻", "Superscript minus", "^-"),
            s("δ⁺", "Partial positive", "\\delta^+"),
            s("δ⁻", "Partial negative", "\\delta^-"),
        ],
    },
    Group {
        name: "Conditions",
        symbols: &[
            s("hν", "Light", "h\\nu"),
            s("Δ", "Heat", "\\Delta"),
            s("e⁻", "Electron", "e^-"),
        ],
    },
    Group {
        name: "Marks",
        symbols: &[
            s("✓", "Check mark", "\\checkmark"),
            s("✗", "Cross mark", "\\xmark"),
        ],
    },
];

/// The shortcuts that expand by themselves. A longer symbol's shortcut, such
/// as `h\nu`, combines these and expands one part at a time.
fn shortcuts() -> impl Iterator<Item = (&'static str, &'static str)> {
    GROUPS
        .iter()
        .flat_map(|group| group.symbols)
        .filter(|symbol| symbol.text.chars().count() == 1)
        .filter_map(|symbol| Some((symbol.shortcut?, symbol.text)))
}

/// The shortcut that `typed` ends with: its length in bytes and its symbol.
pub fn expand(typed: &str) -> Option<(usize, &'static str)> {
    shortcuts()
        .filter(|(shortcut, _)| typed.ends_with(shortcut))
        .max_by_key(|(shortcut, _)| shortcut.len())
        .map(|(shortcut, text)| (shortcut.len(), text))
}

/// The table's entry for `text`, so stored recents map back to it.
pub fn find(text: &str) -> Option<&'static Symbol> {
    GROUPS
        .iter()
        .flat_map(|group| group.symbols)
        .find(|symbol| symbol.text == text)
}

#[cfg(test)]
mod tests;
