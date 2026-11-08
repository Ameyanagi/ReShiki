//! IUPAC 2-Carb-38 extended and condensed oligosaccharide symbols.
//!
//! The whole input must be a glycan before anything is expanded. In
//! particular, these case-sensitive symbols are never replaced inside names.
use super::Expansion;
use crate::naming::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anomer {
    Alpha,
    Beta,
}

impl Anomer {
    fn name(self) -> &'static str {
        match self {
            Self::Alpha => "alpha",
            Self::Beta => "beta",
        }
    }
}

fn parse_anomer(input: &str) -> Option<(Anomer, &str)> {
    for (prefix, value) in [
        ("alpha", Anomer::Alpha),
        ("beta", Anomer::Beta),
        ("α", Anomer::Alpha),
        ("β", Anomer::Beta),
        ("a", Anomer::Alpha),
        ("b", Anomer::Beta),
    ] {
        if let Some(rest) = input.strip_prefix(prefix) {
            return Some((value, rest));
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sugar {
    Glc,
    Gal,
    Man,
    Fru,
    Rib,
    DeoxyRib,
    Xyl,
    Fuc,
    Rha,
    GlcNAc,
    GalNAc,
    GlcA,
    Neu5Ac,
}

impl Sugar {
    fn anomeric_position(self) -> u8 {
        match self {
            Self::Fru | Self::Neu5Ac => 2,
            _ => 1,
        }
    }

    fn stem(self) -> &'static str {
        match self {
            Self::Glc | Self::GlcNAc | Self::GlcA => "gluco",
            Self::Gal | Self::GalNAc => "galacto",
            Self::Man => "manno",
            Self::Fru => "fructo",
            Self::Rib | Self::DeoxyRib => "ribo",
            Self::Xyl => "xylo",
            Self::Fuc => "fuco",
            Self::Rha => "rhamno",
            Self::Neu5Ac => "galacto-non-2-ulo",
        }
    }

    fn acceptor_positions(self, ring: Ring) -> &'static [u8] {
        match (self, ring) {
            (Self::Glc | Self::Gal | Self::Man, Ring::Pyranose) => &[1, 2, 3, 4, 6],
            (Self::Glc | Self::Gal | Self::Man, Ring::Furanose) => &[1, 2, 3, 5, 6],
            (Self::Fru, Ring::Pyranose) => &[1, 2, 3, 4, 5],
            (Self::Fru, Ring::Furanose) => &[1, 2, 3, 4, 6],
            (Self::Rib | Self::Xyl | Self::Fuc | Self::Rha | Self::GlcA, Ring::Pyranose) => {
                &[1, 2, 3, 4]
            }
            (Self::Rib | Self::Xyl | Self::Fuc | Self::Rha | Self::GlcA, Ring::Furanose) => {
                &[1, 2, 3, 5]
            }
            (Self::DeoxyRib, Ring::Pyranose) => &[1, 3, 4],
            (Self::DeoxyRib, Ring::Furanose) => &[1, 3, 5],
            (Self::GlcNAc | Self::GalNAc, Ring::Pyranose) => &[1, 3, 4, 6],
            (Self::GlcNAc | Self::GalNAc, Ring::Furanose) => &[1, 3, 5, 6],
            (Self::Neu5Ac, Ring::Pyranose) => &[2, 4, 7, 8, 9],
            // Its 5-deoxy/5-acetamido substitution prevents a 2,5 ring.
            (Self::Neu5Ac, Ring::Furanose) => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ring {
    Pyranose,
    Furanose,
}

impl Ring {
    fn stem(self) -> &'static str {
        match self {
            Self::Pyranose => "pyran",
            Self::Furanose => "furan",
        }
    }
}

#[derive(Debug)]
struct Residue {
    sugar: Sugar,
    anomer: Option<Anomer>,
    configuration: Option<char>,
    ring: Option<Ring>,
}

impl Residue {
    fn name(&self, glycosyl: bool) -> String {
        // Defaults have been filled and validated before rendering.
        let configuration = self.configuration.unwrap_or('D');
        let ring = self.ring.unwrap_or(Ring::Pyranose).stem();
        let stereo = self
            .anomer
            .map(|value| format!("{}-", value.name()))
            .unwrap_or_default();
        let stem = self.sugar.stem();
        match self.sugar {
            Sugar::Neu5Ac => {
                let suffix = if glycosyl { "osylonic" } else { "osonic" };
                let name = format!(
                    "5-acetamido-3,5-dideoxy-{configuration}-glycero-{stereo}{configuration}-{stem}{ring}{suffix} acid"
                );
                if glycosyl { format!("({name})") } else { name }
            }
            Sugar::GlcA => {
                if glycosyl {
                    format!("({stereo}{configuration}-{stem}{ring}osyluronic acid)")
                } else {
                    format!("{stereo}{configuration}-{stem}{ring}uronic acid")
                }
            }
            _ => {
                let prefix = match self.sugar {
                    Sugar::GlcNAc | Sugar::GalNAc => "2-acetamido-2-deoxy-",
                    Sugar::DeoxyRib => "2-deoxy-",
                    _ => "",
                };
                let suffix = if glycosyl { "osyl" } else { "ose" };
                format!("{prefix}{stereo}{configuration}-{stem}{ring}{suffix}")
            }
        }
    }
}

#[derive(Debug)]
struct Link {
    anomer: Option<Anomer>,
    donor: u8,
    acceptor: u8,
    acceptor_anomer: Option<Anomer>,
}

#[derive(Debug)]
struct Node {
    residue: Residue,
    children: Vec<Node>,
    outgoing: Option<Link>,
}

impl Node {
    fn has_condensed_link(&self) -> bool {
        self.outgoing
            .as_ref()
            .is_some_and(|link| link.anomer.is_some())
            || self.children.iter().any(Self::has_condensed_link)
    }

    fn has_extended_anomer(&self) -> bool {
        self.residue.anomer.is_some() || self.children.iter().any(Self::has_extended_anomer)
    }

    fn render(&self, output: &mut String) {
        for (index, child) in self.children.iter().enumerate() {
            if index != 0 {
                output.push('[');
            }
            child.render(output);
            if index != 0 {
                output.push(']');
            }
            output.push('-');
        }
        output.push_str(&self.residue.name(self.outgoing.is_some()));
        if let Some(link) = &self.outgoing {
            output.push_str(&format!("-({}->{})", link.donor, link.acceptor));
        }
    }
}

struct Parser<'a> {
    rest: &'a str,
}

impl<'a> Parser<'a> {
    fn trim(&mut self) {
        self.rest = self.rest.trim_start_matches(' ');
    }

    fn consume(&mut self, value: &str) -> bool {
        self.trim();
        if let Some(rest) = self.rest.strip_prefix(value) {
            self.rest = rest;
            true
        } else {
            false
        }
    }

    fn separator(&mut self) {
        self.consume("-");
    }

    fn residue(&mut self) -> Option<Residue> {
        self.trim();
        let prefix_anomer = parse_anomer(self.rest).map(|(value, rest)| {
            self.rest = rest;
            self.separator();
            value
        });
        let configuration = if self.consume("D-") {
            Some('D')
        } else if self.consume("L-") {
            Some('L')
        } else {
            None
        };
        // Longer symbols precede their prefixes. Both GlcNAcp and the
        // conventional GlcpNAc position of the ring marker are understood.
        let (sugar, modifier) = [
            ("Neu5Ac", Sugar::Neu5Ac, ""),
            ("GlcNAc", Sugar::GlcNAc, ""),
            ("GalNAc", Sugar::GalNAc, ""),
            ("GlcA", Sugar::GlcA, ""),
            ("dRib", Sugar::DeoxyRib, ""),
            ("Glc", Sugar::Glc, ""),
            ("Gal", Sugar::Gal, ""),
            ("Man", Sugar::Man, ""),
            ("Fru", Sugar::Fru, ""),
            ("Rib", Sugar::Rib, ""),
            ("Xyl", Sugar::Xyl, ""),
            ("Fuc", Sugar::Fuc, ""),
            ("Rha", Sugar::Rha, ""),
            ("Neu", Sugar::Neu5Ac, "5Ac"),
        ]
        .into_iter()
        .find_map(|(symbol, sugar, modifier)| {
            self.rest.strip_prefix(symbol).map(|rest| {
                self.rest = rest;
                (sugar, modifier)
            })
        })?;
        let ring = if self.consume("p") {
            Some(Ring::Pyranose)
        } else if self.consume("f") {
            Some(Ring::Furanose)
        } else {
            None
        };
        if !modifier.is_empty() && (ring.is_none() || !self.consume(modifier)) {
            return None;
        }
        let sugar = match sugar {
            Sugar::Glc if ring.is_some() && self.consume("NAc") => Sugar::GlcNAc,
            Sugar::Gal if ring.is_some() && self.consume("NAc") => Sugar::GalNAc,
            Sugar::Glc if ring.is_some() && self.consume("A") => Sugar::GlcA,
            _ => sugar,
        };
        Some(Residue {
            sugar,
            anomer: prefix_anomer,
            configuration,
            ring,
        })
    }

    fn link(&mut self) -> Option<Link> {
        let saved = self.rest;
        let parsed = self.link_inner();
        if parsed.is_none() {
            self.rest = saved;
        }
        parsed
    }

    fn link_inner(&mut self) -> Option<Link> {
        let saved = self.rest;
        self.separator();
        if !self.consume("(") {
            self.rest = saved;
            return None;
        }
        let (content, rest) = self.rest.split_once(')')?;
        let content = content.replace(' ', "");
        let (anomer, content) = if let Some((value, rest)) = parse_anomer(&content) {
            (Some(value), rest)
        } else {
            (None, content.as_str())
        };
        let separator = content.find(|character: char| !character.is_ascii_digit())?;
        let donor = content.get(..separator)?.parse().ok()?;
        let tail = content.get(separator..)?;
        let tail = tail
            .strip_prefix("<->")
            .or_else(|| tail.strip_prefix("↔"))
            .or_else(|| tail.strip_prefix("->"))
            .or_else(|| tail.strip_prefix("→"))
            .or_else(|| tail.strip_prefix('-'))?;
        let digits = tail
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(tail.len());
        let acceptor = tail.get(..digits)?.parse().ok()?;
        let tail = tail.get(digits..)?;
        let acceptor_anomer = if tail.is_empty() {
            None
        } else {
            let (value, tail) = parse_anomer(tail)?;
            if !tail.is_empty() {
                return None;
            }
            Some(value)
        };
        self.rest = rest;
        Some(Link {
            anomer,
            donor,
            acceptor,
            acceptor_anomer,
        })
    }

    fn chain(&mut self) -> Option<Node> {
        let mut node = Node {
            residue: self.residue()?,
            children: vec![],
            outgoing: None,
        };
        loop {
            node.outgoing = self.link();
            if node.outgoing.is_none() {
                return Some(node);
            }
            self.separator();
            let mut branches = vec![];
            while self.consume("[") {
                let branch = self.chain()?;
                if branch.outgoing.is_none() || !self.consume("]") {
                    return None;
                }
                branches.push(branch);
                self.separator();
            }
            self.trim();
            if self.rest.is_empty() || self.rest.starts_with(']') {
                return branches.is_empty().then_some(node);
            }
            let residue = self.residue()?;
            let mut children = vec![node];
            children.extend(branches);
            node = Node {
                residue,
                children,
                outgoing: None,
            };
        }
    }
}

#[derive(Default)]
struct Assumptions {
    d: bool,
    l_fuc: bool,
    pyranose: bool,
    donor_anomer: bool,
}

fn rejected(message: &str) -> Error {
    Error::Rejected(message.into())
}

fn set_anomer(residue: &mut Residue, value: Option<Anomer>) -> Result<(), Error> {
    if let Some(value) = value {
        if residue.anomer.is_some_and(|existing| existing != value) {
            return Err(rejected(
                "The glycan gives conflicting anomeric descriptors for one residue",
            ));
        }
        residue.anomer = Some(value);
    }
    Ok(())
}

fn validate(node: &mut Node, condensed: bool, assumptions: &mut Assumptions) -> Result<(), Error> {
    let residue = &mut node.residue;
    if !condensed && (residue.configuration.is_none() || residue.ring.is_none()) {
        return Err(rejected(
            "Extended glycan symbols need an explicit D-/L- configuration and p/f ring size; omitted extended-form details are unknown",
        ));
    }
    if residue.configuration.is_none() {
        if residue.sugar == Sugar::Fuc {
            residue.configuration = Some('L');
            assumptions.l_fuc = true;
        } else {
            residue.configuration = Some('D');
            assumptions.d = true;
        }
    }
    if residue.ring.is_none() {
        residue.ring = Some(Ring::Pyranose);
        assumptions.pyranose = true;
    }
    let ring = residue.ring.unwrap_or(Ring::Pyranose);
    if residue.sugar == Sugar::Neu5Ac && ring == Ring::Furanose {
        return Err(rejected(
            "Neu5Ac furanose cannot be built locally; use its pyranose form",
        ));
    }
    if let Some(link) = &node.outgoing {
        if link.donor != residue.sugar.anomeric_position() {
            return Err(rejected(
                "A glycan linkage must start at the donor residue's anomeric carbon",
            ));
        }
        set_anomer(residue, link.anomer)?;
        assumptions.donor_anomer |= residue.anomer.is_none();
    }
    let mut used = Vec::new();
    for child in &mut node.children {
        let link = child
            .outgoing
            .as_ref()
            .ok_or_else(|| rejected("A glycan branch needs a linkage to its parent residue"))?;
        if !residue
            .sugar
            .acceptor_positions(ring)
            .contains(&link.acceptor)
        {
            return Err(rejected(
                "A glycan linkage points to a position without an available hydroxyl group",
            ));
        }
        if used.contains(&link.acceptor)
            || (node.outgoing.is_some() && link.acceptor == residue.sugar.anomeric_position())
        {
            return Err(rejected("Two glycan linkages use the same hydroxyl group"));
        }
        used.push(link.acceptor);
        if link.acceptor_anomer.is_some() && link.acceptor != residue.sugar.anomeric_position() {
            return Err(rejected(
                "An acceptor anomer can only describe a linkage to its anomeric carbon",
            ));
        }
        set_anomer(residue, link.acceptor_anomer)?;
        validate(child, condensed, assumptions)?;
    }
    Ok(())
}

pub(super) fn expand(name: &str) -> Result<Option<Expansion>, Error> {
    let mut parser = Parser {
        rest: name.trim_matches(' '),
    };
    let Some(mut root) = parser.chain() else {
        return Ok(None);
    };
    parser.trim();
    if !parser.rest.is_empty() || root.outgoing.is_some() {
        return Ok(None);
    }
    let condensed = root.has_condensed_link() || !root.has_extended_anomer();
    let mut assumptions = Assumptions::default();
    validate(&mut root, condensed, &mut assumptions)?;
    let mut warnings = vec![];
    if assumptions.d {
        warnings.push(
            "Assumed D configuration for glycan residues without an explicit D-/L- prefix".into(),
        );
    }
    if assumptions.l_fuc {
        warnings.push(
            "Assumed L configuration for Fuc residues without an explicit D-/L- prefix".into(),
        );
    }
    if assumptions.pyranose {
        warnings.push(
            "Assumed pyranose rings for glycan residues without an explicit p/f ring size".into(),
        );
    }
    if assumptions.donor_anomer {
        warnings.push("Some glycosyl donor anomers are unspecified".into());
    }
    let root_anomer_linked = root.children.iter().any(|child| {
        child
            .outgoing
            .as_ref()
            .is_some_and(|link| link.acceptor == root.residue.sugar.anomeric_position())
    });
    if root.residue.anomer.is_none() {
        warnings.push(if root_anomer_linked {
            "The anomer at the right-hand glycosidic end is unspecified".into()
        } else {
            "Reducing-end anomer is unspecified".into()
        });
    }
    let mut expanded = String::new();
    root.render(&mut expanded);
    Ok(Some(Expansion {
        name: expanded,
        warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expansion(input: &str) -> Expansion {
        expand(input).unwrap().unwrap()
    }

    #[test]
    fn complete_condensed_and_extended_notation_expands() {
        for input in ["Gal(b1-4)GlcNAc", "Gal(β1→4)GlcNAc", "Gal(beta1->4)GlcNAc"] {
            assert_eq!(
                expansion(input).name,
                "beta-D-galactopyranosyl-(1->4)-2-acetamido-2-deoxy-D-glucopyranose"
            );
        }
        for input in [
            "a-D-Glcp-(1->4)-D-Glcp",
            "alpha-D-Glcp-(1→4)-D-Glcp",
            "α-D-Glcp-(1-4)-D-Glcp",
        ] {
            assert_eq!(
                expansion(input).name,
                "alpha-D-glucopyranosyl-(1->4)-D-glucopyranose"
            );
        }
        assert_eq!(
            expansion("Man(a1-3)[Man(a1-6)]Man(b1-4)GlcNAc").name,
            "alpha-D-mannopyranosyl-(1->3)-[alpha-D-mannopyranosyl-(1->6)]-beta-D-mannopyranosyl-(1->4)-2-acetamido-2-deoxy-D-glucopyranose"
        );
        assert_eq!(
            expansion("α-D-Glcp-(1→4)[α-D-Glcp-(1→6)]-D-Glcp").name,
            "alpha-D-glucopyranosyl-(1->4)-[alpha-D-glucopyranosyl-(1->6)]-D-glucopyranose"
        );
        assert_eq!(
            expansion("Neu5Ac(a2-3)Gal(b1-4)Glc").name,
            "(5-acetamido-3,5-dideoxy-D-glycero-alpha-D-galacto-non-2-ulopyranosylonic acid)-(2->3)-beta-D-galactopyranosyl-(1->4)-D-glucopyranose"
        );
        assert_eq!(
            expansion("GlcA(b1-4)Glc").name,
            "(beta-D-glucopyranosyluronic acid)-(1->4)-D-glucopyranose"
        );
        for input in ["Glc(a1-2b)Fruf", "α-D-Glcp-(1↔2)-β-D-Fruf"] {
            assert_eq!(
                expansion(input).name,
                "alpha-D-glucopyranosyl-(1->2)-beta-D-fructofuranose"
            );
            assert!(
                !expansion(input)
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("anomer"))
            );
        }
    }

    #[test]
    fn nested_branches_and_ring_marker_positions_are_preserved() {
        assert_eq!(
            expansion("Gal(b1-4)[Fuc(a1-3)[Man(a1-2)]Gal(b1-6)]GlcNAc").name,
            "beta-D-galactopyranosyl-(1->4)-[alpha-L-fucopyranosyl-(1->3)-[alpha-D-mannopyranosyl-(1->2)]-beta-D-galactopyranosyl-(1->6)]-2-acetamido-2-deoxy-D-glucopyranose"
        );
        assert_eq!(
            expansion("GlcNAcp(b1-4)GlcpA").name,
            expansion("GlcpNAc(b1-4)GlcAp").name
        );
        assert_eq!(
            expansion("Neu5Acp(a2-3)Gal").name,
            expansion("Neup5Ac(a2-3)Gal").name
        );
        assert_eq!(
            expansion("D-Fucf(a1-3)L-Galp").name,
            "alpha-D-fucofuranosyl-(1->3)-L-galactopyranose"
        );
    }

    #[test]
    fn warnings_describe_only_assumptions_that_were_applied() {
        let condensed = expansion("Fuc(a1-2)Gal");
        assert!(condensed.name.starts_with("alpha-L-fucopyranosyl"));
        assert_eq!(condensed.warnings.len(), 4);
        for detail in [
            "D configuration",
            "L configuration for Fuc",
            "pyranose",
            "Reducing-end anomer",
        ] {
            assert!(
                condensed
                    .warnings
                    .iter()
                    .any(|warning| warning.contains(detail)),
                "{detail}"
            );
        }
        // Follow the IUPAC condensed default literally, including rhamnose.
        assert_eq!(
            expansion("Rha(a1-4)Glc").name,
            "alpha-D-rhamnopyranosyl-(1->4)-D-glucopyranose"
        );
        assert_eq!(expansion("D-Fucf(a1-3)L-Galp").warnings.len(), 1);
        assert_eq!(expansion("a-D-Glcp-(1->4)-D-Glcp").warnings.len(), 1);
        assert!(
            expansion("alpha-D-Glcp-(1→4)-beta-D-Glcp")
                .warnings
                .is_empty()
        );
        assert!(matches!(
            expand("a-D-Glc-(1->4)-D-Glcp"),
            Err(Error::Rejected(_))
        ));
    }

    #[test]
    fn ordinary_names_unknown_symbols_and_partial_notation_pass_through() {
        for input in [
            "glucose",
            "D-glucopyranose",
            "alpha-D-glucopyranosyl-(1->4)-D-glucopyranose",
            "galactose",
            "Galactose",
            "N-acetylglucosamine",
            "GlcN(a1-4)Glc",
            "Gal(b1-4)Unknown",
            "Gal(b1-4)Glc extra",
            "Glc(",
            "Glc()",
            "Glc(a1-4)",
            "Glc(a1-4)[Glc(a1-6)]",
            "Glc(a1-4)[Glc]Glc",
            "Glc(a1-4)[Glc(a1-6)Glc",
            "Glc(a1-4)Glc]",
            "Glc(a1-4)(Glc)",
        ] {
            assert!(expand(input).unwrap().is_none(), "{input}");
        }
    }

    #[test]
    fn contradictory_or_impossible_complete_glycans_are_rejected() {
        for input in [
            "Glc(a2-4)Glc",
            "Gal(b1-2)GlcNAc",
            "Gal(b1-5)Glcp",
            "Gal(b1-4)[Fuc(a1-4)]GlcNAc",
            "Gal(b1-4b)Glc",
            "a-D-Glcp(b1-4)Glc",
            "Glc(a1-2b)alpha-D-Fruf",
            "Neu5Acf(a2-3)Gal",
        ] {
            assert!(matches!(expand(input), Err(Error::Rejected(_))), "{input}");
        }
    }

    #[test]
    fn all_supported_glycosyl_and_parent_forms_build_in_real_opsin() {
        let parser = opsin::Parser::new().unwrap();
        for symbol in [
            "Glc", "Gal", "Man", "Fru", "Rib", "dRib", "Xyl", "Fuc", "Rha", "GlcNAc", "GalNAc",
            "GlcA", "Neu5Ac",
        ] {
            for ring in ["p", "f"] {
                if symbol == "Neu5Ac" && ring == "f" {
                    continue;
                }
                let donor = if matches!(symbol, "Fru" | "Neu5Ac") {
                    2
                } else {
                    1
                };
                for configuration in ["D", "L"] {
                    for input in [
                        format!("{configuration}-{symbol}{ring}"),
                        format!("{configuration}-{symbol}{ring}(a{donor}-4)Glc"),
                    ] {
                        let expanded = expansion(&input).name;
                        let parsed = parser.parse(&expanded, &opsin::ParseOptions::strict());
                        assert_eq!(
                            parsed.status,
                            opsin::Status::Success,
                            "{input}: {}",
                            parsed.message
                        );
                        assert!(parsed.warnings.is_empty(), "{input}: {:?}", parsed.warnings);
                        assert!(
                            parsed.structure.unwrap().semantic_cxsmiles().is_ok(),
                            "{input}"
                        );
                    }
                }
            }
        }
    }

    /// These SMILES follow the pyranose ring from O5 through C1 to C5;
    /// substituent positions and D/L Haworth configurations are specified
    /// independently of OPSIN's glycosyl-name expansion and atom traversal.
    #[test]
    fn real_glycans_match_independently_written_stereochemical_graphs() {
        let parser = opsin::Parser::new().unwrap();
        for (input, expected) in [
            (
                "Gal(b1-4)Glc",
                "O1C(O)[C@H](O)[C@@H](O)[C@H](O[C@H]2[C@H](O)[C@@H](O)[C@@H](O)[C@@H](CO)O2)[C@H]1CO",
            ),
            (
                "Gal(b1-4)GlcNAc",
                "O1C(O)[C@H](NC(C)=O)[C@@H](O)[C@H](O[C@H]2[C@H](O)[C@@H](O)[C@@H](O)[C@@H](CO)O2)[C@H]1CO",
            ),
            (
                "a-D-Glcp-(1->4)-D-Glcp",
                "O1C(O)[C@H](O)[C@@H](O)[C@H](O[C@@H]2[C@H](O)[C@@H](O)[C@H](O)[C@@H](CO)O2)[C@H]1CO",
            ),
            (
                "Fuc(a1-2)Gal",
                "O1C(O)[C@H](O[C@H]2[C@@H](O)[C@H](O)[C@H](O)[C@H](C)O2)[C@@H](O)[C@@H](O)[C@H]1CO",
            ),
            (
                "Man(a1-3)[Man(a1-6)]Man",
                "O1C(O)[C@@H](O)[C@@H](O[C@@H]2[C@@H](O)[C@@H](O)[C@H](O)[C@@H](CO)O2)[C@H](O)[C@H]1CO[C@@H]3[C@@H](O)[C@@H](O)[C@H](O)[C@@H](CO)O3",
            ),
        ] {
            let expanded = expansion(input).name;
            let parsed = parser.parse(&expanded, &opsin::ParseOptions::strict());
            assert_eq!(
                parsed.status,
                opsin::Status::Success,
                "{input}: {}",
                parsed.message
            );
            assert!(parsed.warnings.is_empty(), "{input}: {:?}", parsed.warnings);
            let actual = parsed.structure.unwrap().semantic_cxsmiles().unwrap();
            crate::naming::verify_identity(expected, &actual)
                .unwrap_or_else(|error| panic!("{input}: {error}; {actual}"));
        }
    }
}
