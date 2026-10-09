//! Ordered destructive semantic parsing, ported from OPSIN `ComponentGenerator`.
//! Source: OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::api::{OpsinWarning, ParseOptions, WarningKind};
use crate::parse_tree::{Arena, NodeId, ParseTree};
use crate::tree_tools::{fix_locant_capitalisation, remove_dash};
use crate::xml_declarations::*;
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Default)]
pub struct ComponentGenerationContext {
    pub options: ParseOptions,
    pub warnings: Vec<OpsinWarning>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentGenerationError(pub String);
impl fmt::Display for ComponentGenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ComponentGenerationError {}
type Result<T = ()> = std::result::Result<T, ComponentGenerationError>;
fn err<T>(s: impl Into<String>) -> Result<T> {
    Err(ComponentGenerationError(s.into()))
}
fn rx(s: &str) -> Regex {
    // Upstream compiles its fixed micro-syntax patterns once. Only immutable
    // patterns are shared; parse mutations and warnings stay in the context.
    static PATTERNS: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let mut patterns = PATTERNS
        .get_or_init(Default::default)
        .lock()
        .expect("pattern cache");
    patterns
        .entry(s.to_string())
        .or_insert_with(|| Regex::new(s).expect("source regular expression"))
        .clone()
}
fn matches(s: &str, pattern: &str) -> bool {
    rx(&format!("^(?:{pattern})$")).is_match(s)
}
const ELEMENT_SYMBOLS: &str = "(?:He|Li|Be|B|C|N|O|F|Ne|Na|Mg|Al|Si|P|S|Cl|Ar|K|Ca|Sc|Ti|V|Cr|Mn|Fe|Co|Ni|Cu|Zn|Ga|Ge|As|Se|Br|Kr|Rb|Sr|Y|Zr|Nb|Mo|Tc|Ru|Rh|Pd|Ag|Cd|In|Sn|Sb|Te|I|Xe|Cs|Ba|La|Ce|Pr|Nd|Pm|Sm|Eu|Gd|Tb|Dy|Ho|Er|Tm|Yb|Lu|Hf|Ta|W|Re|Os|Ir|Pt|Au|Hg|Tl|Pb|Po|At|Rn|Fr|Ra|Ac|Th|Pa|U|Np|Pu|Am|Cm|Bk|Cf|Es|Fm|Md|No|Lr|Rf|Db|Sg|Bh|Hs|Mt|Ds)";
const STEREO: &str = "(.*?)(SR|R/?S|r/?s|[Rr]\\^?[*]|[Ss]\\^?[*]|[Ee][Zz]|[EZ][*]|[RSEZrsezabx]|(?i:cis|trans|alpha|beta|xi|exo|endo|syn|anti)|M|P|Ra|Sa|Sp|Rp|R(?i:or|and)S|S(?i:or|and)R|E(?i:or|and)Z|Z(?i:or|and)E)";
const RS: &str = "[Rr]/?\\^?[*Ss]?|[Ss]\\^?[*Rr]?|R(?i:or|and)S|S(?i:or|and)R";
const EZ: &str = "[EZez]|[Ee][Zz]|[EZ]\\*|EandZ|EorZ";
const AB: &str = "a|b|x|(?i:alpha|beta|xi)";
const RAC: &str = "(?i:rac(\\.|em(\\.|ic)?)?-?)";

pub fn process_components(
    tree: &mut ParseTree,
    context: &mut ComponentGenerationContext,
) -> Result {
    ComponentGenerator::new(&mut tree.arena, context).process_parse(tree.root)
}

pub struct ComponentGenerator<'a> {
    pub arena: &'a mut Arena,
    pub context: &'a mut ComponentGenerationContext,
}
impl<'a> ComponentGenerator<'a> {
    pub fn new(arena: &'a mut Arena, context: &'a mut ComponentGenerationContext) -> Self {
        Self { arena, context }
    }
    fn val(&self, id: NodeId) -> String {
        self.arena.value(id)
    }
    fn attr(&self, id: NodeId, key: &str) -> String {
        self.arena[id].attribute(key).unwrap_or("").to_string()
    }
    fn is(&self, id: Option<NodeId>, name: &str) -> bool {
        id.is_some_and(|id| self.arena[id].name == name)
    }
    fn attr_is(&self, id: NodeId, key: &str, val: &str) -> bool {
        self.arena[id].attribute(key) == Some(val)
    }
    fn num(&self, id: NodeId, key: &str) -> Result<usize> {
        self.attr(id, key).parse().map_err(|_| {
            ComponentGenerationError(format!("Malformed numerical {key} on {}", self.val(id)))
        })
    }
    fn set(&mut self, id: NodeId, key: &str, value: impl Into<String>) {
        self.arena[id].set_attribute(key, value);
    }
    fn add(&mut self, id: NodeId, key: &str, value: impl Into<String>) {
        self.arena[id].add_attribute(key, value);
    }
    fn token(
        &mut self,
        name: &str,
        value: impl Into<String>,
        attributes: &[(&str, &str)],
    ) -> NodeId {
        let id = self.arena.token(name, value);
        for &(k, v) in attributes {
            self.add(id, k, v);
        }
        id
    }
    pub fn process_parse(&mut self, parse: NodeId) -> Result {
        let subs = self
            .arena
            .descendants_named_any(parse, &[SUBSTITUENT_EL, ROOT_EL]);
        for &sub in &subs {
            self.resolve_ambiguities(sub)?;
            self.process_locants(sub)?;
            self.convert_ortho_meta_para_to_locants(sub)?;
            self.form_alkane_stems_from_components(sub)?;
            self.process_alkane_stem_modifications(sub)?;
            self.process_heterogenous_hydrides(sub)?;
            self.process_indicated_hydrogens(sub)?;
            self.process_stereochemistry(sub)?;
            self.process_infixes(sub)?;
            self.process_suffix_prefixes(sub)?;
            self.process_lambda_convention(sub)?;
        }
        let groups = self.arena.descendants_named(parse, GROUP_EL);
        let mut brackets = Vec::new();
        self.find_and_structure_brackets(&subs, &mut brackets)?;
        for &sub in &subs {
            self.process_hydrocarbon_rings(sub)?;
            self.handle_suffix_irregularities(sub)?;
        }
        for group in groups {
            self.detect_alkane_fused_ring_bridges(group);
            self.process_rings(group)?;
            self.handle_group_irregularities(group)?;
        }
        for bracket in brackets {
            self.move_detachable_het_atom_repl(bracket)?;
        }
        Ok(())
    }
    pub fn resolve_ambiguities(&mut self, sub: NodeId) -> Result {
        for m in self.arena.children_named(sub, MULTIPLIER_EL) {
            if ![BASIC_TYPE_VAL, VONBAEYER_TYPE_VAL].contains(&self.attr(m, TYPE_ATR).as_str()) {
                continue;
            }
            let count = self.num(m, VALUE_ATR)?;
            let Some(next) = self.arena.next_sibling(m) else {
                continue;
            };
            if count >= 3 && self.arena[next].name == ALKANESTEMCOMPONENT {
                let len = self.num(next, VALUE_ATR)?;
                let prev = self.arena.previous_sibling(m);
                if len >= 10
                    && len > count
                    && !prev.is_some_and(|p| {
                        self.arena[p].name == LOCANT_EL && self.val(p).split(',').count() == count
                    })
                {
                    return err(format!(
                        "{}{} should not have been lexed as two tokens!",
                        self.val(m),
                        self.val(next)
                    ));
                }
            }
            if count >= 4
                && self.arena[next].name == HYDROCARBONFUSEDRINGSYSTEM_EL
                && self.val(next) == "phen"
                && !self.attr_is(next, SUBSEQUENTUNSEMANTICTOKEN_ATR, "e")
                && self.is(self.arena.next_sibling(next), SUFFIX_EL)
            {
                let prev = self.arena.previous_sibling(m);
                if !prev.is_some_and(|p| {
                    self.arena[p].name == LOCANT_EL && self.val(p).split(',').count() == 1
                }) {
                    return err(format!(
                        "{}{} should not have been lexed as one token!",
                        self.val(m),
                        self.val(next)
                    ));
                }
            }
            if count > 4
                && !self.val(m).ends_with('a')
                && self.arena[next].name == GROUP_EL
                && matches(
                    &self.val(next),
                    "carbonyl|oxy|sulfenyl|sulfinyl|sulfonyl|selenenyl|seleninyl|selenonyl|tellurenyl|tellurinyl|telluronyl",
                )
            {
                return err(format!(
                    "{}{} should have been lexed as [alkane stem, inline suffix], not [multiplier, group]!",
                    self.val(m),
                    self.val(next)
                ));
            }
        }
        for fusion in self.arena.children_named(sub, FUSION_EL) {
            let value = self.val(fusion);
            if matches(&value, r"\[\d+(,\d+)*\]")
                && let Some(hw) = self
                    .arena
                    .next_sibling_ignoring(fusion, &[MULTIPLIER_EL, HETEROATOM_EL])
                && self.attr_is(hw, SUBTYPE_ATR, HANTZSCHWIDMAN_SUBTYPE_VAL)
            {
                let mut count = 0;
                let mut factor = 1;
                let mut current = self.arena.next_sibling(fusion);
                while let Some(id) = current {
                    if self.arena[id].name == GROUP_EL {
                        break;
                    }
                    if self.arena[id].name == HETEROATOM_EL {
                        count += factor;
                        factor = 1
                    } else if self.arena[id].name == MULTIPLIER_EL {
                        factor = self.num(id, VALUE_ATR)?
                    }
                    current = self.arena.next_sibling(id);
                }
                let locants: Vec<_> = value[1..value.len() - 1].split(',').collect();
                if locants.len() == count
                    && locants.iter().all(|s| {
                        s.parse::<usize>()
                            .is_ok_and(|n| n <= self.attr(hw, VALUE_ATR).len().saturating_sub(2))
                    })
                {
                    return err(
                        "This fusion bracket is in fact more likely to be a description of the locants of a HW ring",
                    );
                }
            }
        }
        if let Some(g) = self.arena.first_child_named(sub, GROUP_EL)
            && self.val(g) == "then"
            && let Some(p) = self.arena.previous_element(g, true)
            && self.arena[p].name == SUFFIX_EL
            && self.arena[p]
                .attribute(SUBSEQUENTUNSEMANTICTOKEN_ATR)
                .is_none()
            && ["ylidene", "ylidyne"].contains(&self.val(p).as_str())
        {
            return err("Group should be ethenyl, not thenyl");
        }
        Ok(())
    }
    pub fn process_locants(&mut self, sub: NodeId) -> Result {
        for id in self.arena[sub].children.clone() {
            if self.arena[id].name == LOCANT_EL {
                let mut locants = split_into_individual_locants(remove_dash(&self.val(id)));
                for text in &mut locants {
                    if text.ends_with([')', ']', '}']) {
                        let Some(start) = text.rfind(['(', '[', '{']) else {
                            return err("OPSIN bug: malformed locant text");
                        };
                        let inner = text[start + 1..text.len() - 1].to_string();
                        if matches(&inner, r"(?i:[1-9][0-9]*[a-g]?'*H(,[1-9][0-9]*[a-g]?'*H)*)") {
                            *text = remove_dash(&text[..start]).to_string();
                            for h in inner.split(',') {
                                let hloc = fix_locant_capitalisation(&h[..h.len() - 1]);
                                let h = self.token(ADDEDHYDROGEN_EL, "", &[(LOCANT_ATR, &hloc)]);
                                self.arena.insert_before(id, h);
                            }
                            if self.arena[id].attribute(TYPE_ATR).is_none() {
                                self.add(id, TYPE_ATR, ADDEDHYDROGENLOCANT_TYPE_VAL);
                            }
                        } else if matches(&inner, r"(?i:[RS]|R[,/]?S)") {
                            *text = remove_dash(&text[..start]).to_string();
                            let rs = rx(r"\W").replace_all(&inner, "");
                            let stereo = self.token(
                                STEREOCHEMISTRY_EL,
                                format!("({}{rs})", standardize_locant_variants(text)),
                                &[(TYPE_ATR, STEREOCHEMISTRYBRACKET_TYPE_VAL)],
                            );
                            self.arena.insert_before(id, stereo);
                        }
                    }
                    *text = standardize_locant_variants(text);
                }
                self.arena[id].set_value(locants.join(","));
                if self.arena.next_sibling(id).is_none() {
                    return err(format!(
                        "Nothing after locant tag: {}",
                        self.arena.to_xml(id)
                    ));
                }
                if locants.len() == 1 {
                    self.convert_carbohydrate_locant(id)?;
                }
            } else if self.arena[id].name == COLONORSEMICOLONDELIMITEDLOCANT_EL {
                let value = remove_dash(&self.val(id)).to_string();
                let mut out = String::new();
                let mut current = String::new();
                for ch in value.chars() {
                    if [',', ':', ';'].contains(&ch) {
                        out.push_str(&standardize_locant_variants(&current));
                        current.clear();
                        out.push(ch)
                    } else {
                        current.push(ch)
                    }
                }
                out.push_str(&standardize_locant_variants(&current));
                self.arena[id].set_value(out);
            }
        }
        Ok(())
    }
    fn convert_carbohydrate_locant(&mut self, id: NodeId) -> Result {
        let value = self.val(id);
        if !matches(&value, "[A-Z][a-z]?") {
            return Ok(());
        }
        let Some(m) = self
            .arena
            .previous_sibling(id)
            .filter(|&x| self.arena[x].name == MULTIPLIER_EL)
        else {
            return Ok(());
        };
        let count = self.num(m, VALUE_ATR)?;
        if let Some(p) = self.arena.previous_sibling(m) {
            let locants: Vec<_> = self
                .val(p)
                .split(',')
                .map(|s| format!("{value}{s}"))
                .collect();
            if locants.len() == count {
                self.arena[p].set_value(locants.join(","));
                self.arena.detach(id)
            }
        } else {
            let value = (0..count)
                .map(|n| format!("{value}{}", "'".repeat(n)))
                .collect::<Vec<_>>()
                .join(",");
            let locant = self.token(LOCANT_EL, value, &[]);
            self.arena.insert_before(m, locant);
            self.arena.detach(id);
        }
        Ok(())
    }
    fn convert_ortho_meta_para_to_locants(&mut self, sub: NodeId) -> Result {
        for id in self.arena.children_named(sub, ORTHOMETAPARA_EL) {
            let value = self.val(id);
            self.arena[id].name = LOCANT_EL.into();
            self.add(id, TYPE_ATR, ORTHOMETAPARA_TYPE_VAL);
            let name = match value.chars().next().map(|ch| ch.to_ascii_lowercase()) {
                Some('o') => "ortho",
                Some('m') => "meta",
                Some('p') => "para",
                _ => {
                    return err(format!(
                        "{value} was not identified as being either ortho, meta or para but according to the chemical grammar it should of been"
                    ));
                }
            };
            let two = self.arena.next_sibling(id).is_some_and(|n| {
                if self.arena[n].name == MULTIPLIER_EL && self.attr_is(n, VALUE_ATR, "2") {
                    return true;
                }
                if self.arena[n]
                    .attribute(OUTIDS_ATR)
                    .is_some_and(|s| s.split(',').count() > 1)
                {
                    return true;
                }
                self.arena[n].name == GROUP_EL
                    && self.arena.next_sibling(n).is_some_and(|m| {
                        self.arena[m].name == MULTIPLIER_EL
                            && self.attr_is(m, VALUE_ATR, "2")
                            && self.is(
                                self.arena
                                    .next_sibling_ignoring(m, &[INFIX_EL, SUFFIXPREFIX_EL]),
                                SUFFIX_EL,
                            )
                    })
            });
            self.arena[id].set_value(if two {
                format!("1,{name}")
            } else {
                name.into()
            });
        }
        Ok(())
    }
    fn form_alkane_stems_from_components(&mut self, sub: NodeId) -> Result {
        let mut components = VecDeque::from(self.arena.children_named(sub, ALKANESTEMCOMPONENT));
        while let Some(mut id) = components.pop_front() {
            let mut count = self.num(id, VALUE_ATR)?;
            let mut name = self.val(id);
            while components
                .front()
                .is_some_and(|&next| self.arena.next_sibling(id) == Some(next))
            {
                self.arena.detach(id);
                id = components.pop_front().unwrap();
                count += self.num(id, VALUE_ATR)?;
                name.push_str(&self.val(id));
            }
            let smiles = "C".repeat(count);
            let g = self.token(
                GROUP_EL,
                name,
                &[
                    (TYPE_ATR, CHAIN_TYPE_VAL),
                    (SUBTYPE_ATR, ALKANESTEM_SUBTYPE_VAL),
                    (VALUE_ATR, &smiles),
                    (USABLEASJOINER_ATR, "yes"),
                    (LABELS_ATR, NUMERIC_LABELS_VAL),
                ],
            );
            self.arena.insert_after(id, g);
            self.arena.detach(id);
        }
        Ok(())
    }
    fn process_alkane_stem_modifications(&mut self, sub: NodeId) -> Result {
        for m in self.arena.children_named(sub, ALKANESTEMMODIFIER_EL) {
            let alk = self.arena.next_sibling(m);
            let value = self.val(m);
            let kind = self.arena[m]
                .attribute(VALUE_ATR)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    match value.as_str() {
                        "n-" => "normal",
                        "i-" => "iso",
                        "s-" => "sec",
                        _ => "",
                    }
                    .to_string()
                });
            self.arena.detach(m);
            let Some(alk) = alk else {
                return err("OPSIN Bug: AlkaneStem not found after alkaneStemModifier");
            };
            let amyl = self.attr_is(alk, SUBTYPE_ATR, AMYL_SUBTYPE_VAL);
            if !amyl
                && !(self.attr_is(alk, TYPE_ATR, CHAIN_TYPE_VAL)
                    && self.attr_is(alk, SUBTYPE_ATR, ALKANESTEM_SUBTYPE_VAL))
            {
                return err("OPSIN Bug: AlkaneStem not found after alkaneStemModifier");
            }
            let len = if amyl {
                5
            } else {
                self.attr(alk, VALUE_ATR).len()
            };
            let suffix = amyl || !self.arena.children_named(sub, SUFFIX_EL).is_empty();
            let mut labels = NONE_LABELS_VAL.to_string();
            let smiles = match kind.as_str() {
                "normal" => {
                    if (len == 1 || len == 2) && value == "n-" {
                        let n = self.token(LOCANT_EL, "N", &[]);
                        self.arena.insert_before(alk, n)
                    }
                    continue;
                }
                "tert" => {
                    if len < 4 {
                        return err(format!(
                            "ChainLength to small for tert modifier, required minLength 4. Found: {len}"
                        ));
                    }
                    if len > 8 {
                        return err(format!(
                            "Interpretation of tert on an alkane chain of length: {len} is ambiguous"
                        ));
                    }
                    if len == 8 {
                        "C(C)(C)CC(C)(C)C".into()
                    } else {
                        format!("C(C)(C)C{}", "C".repeat(len - 4))
                    }
                }
                "iso" => {
                    if len < 3 {
                        return err(format!(
                            "ChainLength to small for iso modifier, required minLength 3. Found: {len}"
                        ));
                    }
                    if len == 3 && !suffix {
                        return err(
                            "iso has no meaning without a suffix on an alkane chain of length 3",
                        );
                    }
                    if len == 8 && !suffix {
                        "C(C)(C)CC(C)(C)C".into()
                    } else {
                        labels = (1..=len - 2).map(|n| format!("{n}/")).collect::<String>() + "/";
                        format!("{}C(C)C", "C".repeat(len - 3))
                    }
                }
                "sec" => {
                    if len < 3 {
                        return err(format!(
                            "ChainLength to small for sec modifier, required minLength 3. Found: {len}"
                        ));
                    }
                    if !suffix {
                        return err("sec has no meaning without a suffix on an alkane chain");
                    }
                    format!("C(C)C{}", "C".repeat(len - 3))
                }
                "neo" => {
                    if len < 5 {
                        return err(format!(
                            "ChainLength to small for neo modifier, required minLength 5. Found: {len}"
                        ));
                    }
                    format!("{}CC(C)(C)C", "C".repeat(len - 5))
                }
                _ => return err("Unrecognised alkaneStem modifier"),
            };
            let smiles = if amyl {
                format!("{}{smiles}", &self.attr(alk, VALUE_ATR)[..1])
            } else {
                smiles
            };
            self.set(alk, VALUE_ATR, smiles);
            self.set(alk, LABELS_ATR, labels);
            self.arena[alk].remove_attribute(USABLEASJOINER_ATR);
        }
        Ok(())
    }
    fn process_heterogenous_hydrides(&mut self, sub: NodeId) -> Result {
        let mut remaining = Vec::new();
        for m in self.arena.children_named(sub, MULTIPLIER_EL) {
            if self.attr_is(m, TYPE_ATR, GROUP_TYPE_VAL) {
                continue;
            }
            let Some(g) = self.arena.next_sibling(m) else {
                return err("OPSIN bug: missing multiplied element");
            };
            if self.arena[g].name != GROUP_EL
                || !self.attr_is(g, SUBTYPE_ATR, HETEROSTEM_SUBTYPE_VAL)
            {
                remaining.push(m);
                continue;
            }
            let count = self.num(m, VALUE_ATR)?;
            if self.arena.previous_sibling(m).is_some_and(|p| {
                self.arena[p].name == LOCANT_EL && self.val(p).split(',').count() == count
            }) && let Some(s) = self.arena.next_sibling_named(g, SUFFIX_EL)
                && self.attr_is(s, TYPE_ATR, INLINE_TYPE_VAL)
                && !self.is(self.arena.previous_sibling(s), MULTIPLIER_EL)
            {
                remaining.push(m);
                continue;
            }
            let smiles = self.attr(g, VALUE_ATR);
            if smiles == "B"
                && self.arena.previous_sibling(m).is_none()
                && self.arena.next_sibling(g).is_some_and(|s| {
                    self.arena[s].name == UNSATURATOR_EL && self.attr_is(s, VALUE_ATR, "1")
                })
            {
                return err("Polyboranes are not currently supported");
            }
            self.set(g, VALUE_ATR, smiles.repeat(count));
            self.arena.detach(m);
        }
        for m in remaining {
            let Some(first) = self
                .arena
                .next_sibling(m)
                .filter(|&x| self.arena[x].name == HETEROATOM_EL)
            else {
                continue;
            };
            let Some(second) = self
                .arena
                .next_sibling(first)
                .filter(|&x| self.arena[x].name == HETEROATOM_EL)
            else {
                continue;
            };
            let Some(end) = self
                .arena
                .next_sibling_ignoring(second, &[LOCANT_EL, MULTIPLIER_EL])
            else {
                continue;
            };
            let a = self.attr(first, VALUE_ATR);
            let b = self.attr(second, VALUE_ATR);
            let count = self.num(m, VALUE_ATR)?;
            if self.arena[end].name == UNSATURATOR_EL {
                if self.attr_is(end, VALUE_ATR, "1") {
                    check_hw_ambiguity(&a, &b, false)?;
                }
                let cyclic = self.arena.previous_sibling(m).is_some_and(|p| {
                    [CYCLO_EL, VONBAEYER_EL, SPIRO_EL].contains(&self.arena[p].name.as_str())
                });
                let smiles = if cyclic {
                    format!("{b}{a}").repeat(count)
                } else {
                    format!("{a}{b}").repeat(count.saturating_sub(1)) + &a
                };
                let smiles = rx("H[0-9]").replace_all(&smiles, "H?").to_string();
                let name = format!("{}{}{}", self.val(m), self.val(first), self.val(second));
                self.arena.detach(first);
                let g = self.token(
                    GROUP_EL,
                    name,
                    &[
                        (VALUE_ATR, &smiles),
                        (LABELS_ATR, NUMERIC_LABELS_VAL),
                        (TYPE_ATR, CHAIN_TYPE_VAL),
                        (SUBTYPE_ATR, HETEROSTEM_SUBTYPE_VAL),
                    ],
                );
                if !cyclic {
                    self.add(g, USABLEASJOINER_ATR, "yes")
                }
                self.arena.insert_after(second, g);
                self.arena.detach(second);
                self.arena.detach(m);
            } else if self.val(end) == "an"
                && self.attr_is(end, SUBTYPE_ATR, HANTZSCHWIDMAN_SUBTYPE_VAL)
                && !self.arena.previous_sibling(m).is_some_and(|p| {
                    self.arena[p].name == LOCANT_EL && self.val(p).split(',').count() == count + 1
                })
            {
                check_hw_ambiguity(&a, &b, true)?;
            }
        }
        Ok(())
    }
    fn process_indicated_hydrogens(&mut self, sub: NodeId) -> Result {
        for id in self.arena.children_named(sub, INDICATEDHYDROGEN_EL) {
            let mut value = remove_dash(&self.val(id)).to_string();
            if !value.to_ascii_lowercase().ends_with('h') {
                if value.len() < 2 {
                    return err("OPSIN Bug: malformed indicated hydrogen element!");
                }
                value = value[1..value.len() - 1].to_string();
            }
            for h in value.split(',') {
                if !h.to_ascii_lowercase().ends_with('h') {
                    return err("OPSIN Bug: malformed indicated hydrogen element!");
                }
                let loc = fix_locant_capitalisation(&h[..h.len() - 1]);
                let n = self.token(INDICATEDHYDROGEN_EL, "", &[(LOCANT_ATR, &loc)]);
                self.arena.insert_before(id, n);
            }
            self.arena.detach(id);
        }
        Ok(())
    }
    pub fn process_stereochemistry(&mut self, sub: NodeId) -> Result {
        let mut ez = Vec::new();
        for id in self.arena.children_named(sub, STEREOCHEMISTRY_EL) {
            match self.attr(id, TYPE_ATR).as_str() {
                STEREOCHEMISTRYBRACKET_TYPE_VAL => self.process_stereochemistry_bracket(id)?,
                CISORTRANS_TYPE_VAL => {
                    self.assign_stereo_locant(id);
                }
                E_OR_Z_TYPE_VAL => {
                    self.add(id, VALUE_ATR, self.val(id).to_uppercase());
                    if self.assign_stereo_locant(id) {
                        ez.push(id)
                    }
                }
                ENDO_EXO_SYN_ANTI_TYPE_VAL => {
                    if let Some(p) = self.arena.previous_element(id, true).filter(|&p| {
                        self.arena[p].name == LOCANT_EL && self.val(p).split(',').count() == 1
                    }) {
                        self.add(id, LOCANT_ATR, self.val(p));
                        if self
                            .arena
                            .next_sibling_named(id, GROUP_EL)
                            .is_some_and(|g| {
                                self.attr_is(
                                    g,
                                    SUBTYPE_ATR,
                                    CYCLICUNSATURABLEHYDROCARBON_SUBTYPE_VAL,
                                ) || self.is(self.arena.previous_sibling(g), VONBAEYER_EL)
                            })
                        {
                            self.arena.detach(p)
                        }
                    }
                }
                ALPHA_OR_BETA_TYPE_VAL => self.process_unbracketed_alpha_beta(id)?,
                RELATIVECISTRANS_TYPE_VAL => {
                    let v = remove_dash(&self.val(id)).to_string();
                    let mut locants = Vec::new();
                    for term in v.split(',') {
                        if !["c-", "t-", "r-"].iter().any(|s| term.starts_with(s)) {
                            return err("Malformed relativeCisTrans element");
                        }
                        locants.push(term[2..].to_string())
                    }
                    let loc = self.token(LOCANT_EL, locants.join(","), &[]);
                    self.arena.insert_after(id, loc);
                }
                OPTICALROTATION_TYPE_VAL => {
                    let value = self.val(id);
                    if value.starts_with("(+/-)") || value.starts_with("(+-)") {
                        let rac =
                            self.token(STEREOCHEMISTRY_EL, value, &[(TYPE_ATR, RAC_TYPE_VAL)]);
                        self.arena.insert_before(id, rac)
                    }
                }
                _ => {}
            }
        }
        let mut i = 0;
        while i < ez.len() {
            let mut group = vec![ez[i]];
            while i + 1 < ez.len() && self.arena.next_sibling(ez[i]) == Some(ez[i + 1]) {
                i += 1;
                group.push(ez[i])
            }
            let last = *group.last().unwrap();
            let mut next = self.arena.next_sibling(last);
            if group.len() > 1 {
                if !next.is_some_and(|m| {
                    self.arena[m].name == MULTIPLIER_EL
                        && self.attr(m, VALUE_ATR) == group.len().to_string()
                }) {
                    i += 1;
                    continue;
                }
                next = self.arena.next_sibling(next.unwrap());
            }
            if let Some(n) = next {
                let name = self.arena[n].name.as_str();
                if name == UNSATURATOR_EL || name == SUFFIX_EL {
                    if (name == UNSATURATOR_EL && self.attr_is(n, VALUE_ATR, "2"))
                        || (name == SUFFIX_EL && self.attr_is(n, VALUE_ATR, "ylidene"))
                    {
                        let value = group
                            .iter()
                            .map(|&s| self.attr(s, LOCANT_ATR))
                            .collect::<Vec<_>>()
                            .join(",");
                        let loc = self.token(LOCANT_EL, value, &[]);
                        self.arena.insert_after(last, loc);
                    } else {
                        return err(format!(
                            "After E/Z stereo expected {} but found: {}",
                            if name == UNSATURATOR_EL {
                                "ene"
                            } else {
                                "yldiene"
                            },
                            self.val(n)
                        ));
                    }
                }
            }
            i += 1;
        }
        Ok(())
    }
    fn assign_stereo_locant(&mut self, id: NodeId) -> bool {
        if let Some(p) = self
            .arena
            .previous_element(id, true)
            .filter(|&p| self.arena[p].name == LOCANT_EL && self.val(p).split(',').count() == 1)
        {
            self.add(id, LOCANT_ATR, self.val(p));
            self.arena.detach(p);
            true
        } else {
            false
        }
    }
    fn process_stereochemistry_bracket(&mut self, id: NodeId) -> Result {
        let outcome = self.expand_stereochemistry_bracket(id);
        self.arena.detach(id);
        outcome
    }
    fn expand_stereochemistry_bracket(&mut self, id: NodeId) -> Result {
        let original = self.val(id);
        let mut text = original.clone();
        let mut group = None;
        if text.to_ascii_lowercase().starts_with("rel-") {
            group = Some("Rel");
            text = text[4..].to_string()
        }
        text = remove_dash(&text).to_string();
        if let Some(m) = rx(&format!("^{RAC}")).find(&text) {
            let end = m.end();
            text = text[end..].to_string();
            group = Some("Rac")
        }
        text = match normalise_binary_brackets(&text) {
            Ok(s) => s,
            Err(error) => {
                if self
                    .context
                    .options
                    .warn_rather_than_fail_on_uninterpretable_stereochemistry
                {
                    self.context.warnings.push(OpsinWarning {
                        kind: WarningKind::StereochemistryIgnored,
                        message: error.0,
                    });
                    return Ok(());
                }
                return Err(error);
            }
        };
        let descriptors = if text.is_empty() {
            Vec::new()
        } else {
            split_stereo_descriptors(&text)
        };
        let exclusive = descriptors.len() == 1
            && (descriptors[0].eq_ignore_ascii_case("rel") || matches(&descriptors[0], RAC));
        if exclusive {
            group = Some(if descriptors[0].eq_ignore_ascii_case("rel") {
                "Rel"
            } else {
                "Rac"
            })
        }
        if text.is_empty() || exclusive {
            if let Some(kind) = group {
                let n = self.token(
                    STEREOCHEMISTRY_EL,
                    original,
                    &[(
                        TYPE_ATR,
                        if kind == "Rac" {
                            RAC_TYPE_VAL
                        } else {
                            REL_TYPE_VAL
                        },
                    )],
                );
                self.arena.insert_before(id, n)
            }
            return Ok(());
        }
        let pattern = rx(&format!("^{STEREO}$"));
        for descriptor in descriptors {
            let Some(c) = pattern.captures(&descriptor) else {
                return err(format!("Malformed stereochemistry element: {original}"));
            };
            let n = self.token(STEREOCHEMISTRY_EL, descriptor.clone(), &[]);
            if !c[1].is_empty() {
                self.add(n, LOCANT_ATR, fix_locant_capitalisation(remove_dash(&c[1])))
            }
            self.arena.insert_before(id, n);
            let symbol = &c[2];
            if matches(symbol, RS) {
                self.add(n, TYPE_ATR, R_OR_S_TYPE_VAL);
                let mut symbol = symbol.to_uppercase().replace('/', "");
                let mut local = group;
                if ["RS", "SR", "RANDS", "SANDR"].contains(&symbol.as_str()) {
                    if local.is_none() {
                        local = Some("Rac")
                    }
                    symbol = symbol[..1].to_string()
                } else if ["R*", "S*", "R^*", "S^*", "RORS", "SORR"].contains(&symbol.as_str()) {
                    if local.is_none() {
                        local = Some("Rel")
                    }
                    symbol = symbol[..1].to_string()
                }
                self.add(n, VALUE_ATR, symbol);
                self.add(n, STEREOGROUP_ATR, local.unwrap_or("Abs"));
            } else if matches(symbol, EZ) {
                self.add(n, TYPE_ATR, E_OR_Z_TYPE_VAL);
                let mut symbol = symbol.to_uppercase();
                if ["EANDZ", "EORZ", "E*", "Z*"].contains(&symbol.as_str()) {
                    symbol = "EZ".into()
                }
                self.add(n, VALUE_ATR, symbol);
            } else if matches(symbol, AB) {
                self.add(n, TYPE_ATR, ALPHA_OR_BETA_TYPE_VAL);
                self.add(n, VALUE_ATR, alpha_beta_symbol(symbol)?);
            } else if matches(symbol, "(?i:cis|trans)") {
                self.add(n, TYPE_ATR, CISORTRANS_TYPE_VAL);
                self.add(n, VALUE_ATR, symbol.to_lowercase());
            } else if matches(symbol, "(?i:endo|exo|syn|anti)") {
                self.add(n, TYPE_ATR, ENDO_EXO_SYN_ANTI_TYPE_VAL);
                self.add(n, VALUE_ATR, symbol.to_lowercase());
            } else if matches(symbol, "M|P|Ra|Sa|Sp|Rp") {
                self.add(n, TYPE_ATR, AXIAL_TYPE_VAL);
                self.add(n, VALUE_ATR, symbol);
            } else {
                return err(format!("Malformed stereochemistry element: {original}"));
            }
        }
        Ok(())
    }
    fn process_unbracketed_alpha_beta(&mut self, id: NodeId) -> Result {
        let txt = remove_dash(&self.val(id)).to_string();
        let mut locants = Vec::new();
        let mut create = false;
        for descriptor in txt.split(',') {
            if let Some(m) = rx("^[0-9]+").find(descriptor) {
                let loc = m.as_str();
                let symbol = rx("[0-9]+").replace_all(descriptor, "").to_string();
                locants.push(loc.to_string());
                if matches(&symbol, AB) {
                    let n = self.token(
                        STEREOCHEMISTRY_EL,
                        descriptor,
                        &[
                            (LOCANT_ATR, loc),
                            (TYPE_ATR, ALPHA_OR_BETA_TYPE_VAL),
                            (VALUE_ATR, alpha_beta_symbol(&symbol)?),
                        ],
                    );
                    self.arena.insert_before(id, n)
                } else {
                    create = true
                }
            }
        }
        if !create {
            create = !self
                .arena
                .next_siblings_named(id, GROUP_EL)
                .iter()
                .any(|&g| {
                    self.arena[g]
                        .attribute(ALPHABETACLOCKWISEATOMORDERING_ATR)
                        .is_some()
                });
        }
        if create {
            let n = self.token(LOCANT_EL, locants.join(","), &[]);
            self.arena.insert_after(id, n)
        }
        self.arena.detach(id);
        Ok(())
    }
    fn process_suffix_prefixes(&mut self, sub: NodeId) -> Result {
        for id in self.arena.children_named(sub, SUFFIXPREFIX_EL) {
            let Some(s) = self
                .arena
                .next_sibling(id)
                .filter(|&s| self.arena[s].name == SUFFIX_EL)
            else {
                return err(format!(
                    "OPSIN bug: suffix not found after suffixPrefix: {}",
                    self.val(id)
                ));
            };
            self.add(s, SUFFIXPREFIX_ATR, self.attr(id, VALUE_ATR));
            self.arena.detach(id);
        }
        Ok(())
    }
    fn process_infixes(&mut self, sub: NodeId) -> Result {
        for id in self.arena.children_named(sub, INFIX_EL) {
            let Some(s) = self
                .arena
                .next_sibling_ignoring(id, &[INFIX_EL, SUFFIXPREFIX_EL, MULTIPLIER_EL])
                .filter(|&s| self.arena[s].name == SUFFIX_EL)
            else {
                return err(format!(
                    "No suffix found next next to infix: {}",
                    self.val(id)
                ));
            };
            let mut infos = self.arena[s]
                .attribute(INFIX_ATR)
                .map(|v| v.split(';').map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            if self.arena[s].attribute(INFIX_ATR).is_none() {
                self.add(s, INFIX_ATR, "")
            }
            let value = self.attr(id, VALUE_ATR);
            infos.push(value.clone());
            let mut multiplier = self.arena.previous_sibling(id);
            let mut known = false;
            let bracket;
            if self.is(multiplier, MULTIPLIER_EL) {
                known = self.is(
                    self.arena
                        .previous_sibling_ignoring(id, &[MULTIPLIER_EL, INFIX_EL]),
                    SUFFIXPREFIX_EL,
                );
                let before = self.arena.previous_sibling(multiplier.unwrap());
                known |= self.is(before, MULTIPLIER_EL) || infos.len() > 1;
                bracket = before;
            } else {
                bracket = multiplier;
                multiplier = None;
                self.arena.detach(id)
            }
            if self.is(bracket, STRUCTURALOPENBRACKET_EL) {
                let close = self.arena.next_sibling(s);
                if !self.is(close, STRUCTURALCLOSEBRACKET_EL) {
                    return err("Matching closing bracket not found around infix/suffix block");
                }
                if let Some(m) = multiplier {
                    for _ in 1..self.num(m, VALUE_ATR)? {
                        infos.push(value.clone())
                    }
                    self.arena.detach(m);
                    self.arena.detach(id)
                }
                self.arena.detach(bracket.unwrap());
                self.arena.detach(close.unwrap());
            } else if known {
                let m = multiplier.unwrap();
                for _ in 1..self.num(m, VALUE_ATR)? {
                    infos.push(value.clone())
                }
                self.arena.detach(m);
                self.arena.detach(id);
            } else if multiplier.is_some_and(|m| self.attr_is(m, TYPE_ATR, GROUP_TYPE_VAL)) {
                self.arena.detach(id)
            }
            self.set(s, INFIX_ATR, infos.join(";"));
        }
        Ok(())
    }
    fn process_lambda_convention(&mut self, sub: NodeId) -> Result {
        let fused = self.arena.children_named(sub, GROUP_EL).len() > 1;
        for id in self.arena.children_named(sub, LAMBDACONVENTION_EL) {
            let mut values = remove_dash(&self.val(id))
                .split(',')
                .map(str::to_string)
                .collect::<Vec<_>>();
            let mut current = self.arena.next_sibling(id);
            let mut count = 0;
            let mut factor = 1;
            while let Some(c) = current {
                if self.arena[c].name == HETEROATOM_EL {
                    count += factor;
                    factor = 1
                } else if self.arena[c].name == MULTIPLIER_EL {
                    factor = self.num(c, VALUE_ATR)?
                } else {
                    break;
                }
                current = self.arena.next_sibling(c)
            }
            let assign = values.len() == count
                && !current.is_some_and(|c| {
                    fused
                        && self.arena[c].name == GROUP_EL
                        && self.attr_is(c, SUBTYPE_ATR, HANTZSCHWIDMAN_SUBTYPE_VAL)
                });
            let front = values.len() != count
                && current.is_some_and(|c| {
                    (count == 0
                        && self.arena.next_sibling(id) == Some(c)
                        && fused
                        && self.arena[c].name == GROUP_EL
                        && ["benzo", "benz"].contains(&self.val(c).as_str())
                        && !self.is(self.arena.next_sibling(c), FUSION_EL)
                        && !self.is(self.arena.next_sibling(c), LOCANT_EL))
                        || (self.arena[c].name == POLYCYCLICSPIRO_EL
                            && ["spirobi", "spiroter"].contains(&self.attr(c, VALUE_ATR).as_str()))
                });
            let mut hetero = Vec::new();
            if assign {
                let mut multiplier = None;
                let mut c = self.arena.next_sibling(id);
                while let Some(n) = c {
                    if self.arena[n].name == HETEROATOM_EL {
                        hetero.push(n);
                        if let Some(m) = multiplier {
                            for _ in 1..self.num(m, VALUE_ATR)? {
                                let copy = self.arena.copy(n);
                                self.arena.insert_before(n, copy);
                                hetero.push(copy)
                            }
                            self.arena.detach(m);
                            multiplier = None
                        }
                    } else if self.arena[n].name == MULTIPLIER_EL {
                        if multiplier.is_some() {
                            break;
                        }
                        multiplier = Some(n)
                    } else {
                        break;
                    }
                    c = self.arena.next_sibling(n);
                }
            }
            let pattern = rx(r"(?i)^(\S+)?lambda\D*(\d+)\D*$");
            for (i, value) in values.iter_mut().enumerate() {
                if let Some(c) = pattern.captures(value) {
                    let loc = c.get(1).map(|m| fix_locant_capitalisation(m.as_str()));
                    let lambda = c[2].to_string();
                    if front {
                        let Some(loc) = &loc else {
                            return err(
                                "Locant not found for lambda convention before a benzo fused ring system",
                            );
                        };
                        *value = loc.clone()
                    }
                    let n = if assign {
                        hetero[i]
                    } else {
                        let n = self.token(LAMBDACONVENTION_EL, "", &[]);
                        self.arena.insert_before(id, n);
                        n
                    };
                    self.add(n, LAMBDA_ATR, lambda);
                    if let Some(loc) = loc {
                        self.add(n, LOCANT_ATR, loc)
                    }
                } else {
                    *value = fix_locant_capitalisation(value);
                    if assign {
                        self.add(hetero[i], LOCANT_ATR, value.clone())
                    } else if !front {
                        return err(format!(
                            "Lambda convention not specified for locant: {value}"
                        ));
                    }
                }
            }
            if front {
                self.arena[id].name = LOCANT_EL.into();
                self.arena[id].set_value(values.join(","))
            } else {
                self.arena.detach(id)
            }
        }
        Ok(())
    }
    fn find_and_structure_brackets(
        &mut self,
        subs: &[NodeId],
        brackets: &mut Vec<NodeId>,
    ) -> Result {
        let mut level = 0i32;
        let mut open = None;
        let mut nested = false;
        for &sub in subs {
            for id in self.arena[sub].children.clone() {
                if self.arena[id].name == OPENBRACKET_EL {
                    level += 1;
                    if open.is_none() {
                        open = Some(id)
                    } else {
                        nested = true
                    }
                } else if self.arena[id].name == CLOSEBRACKET_EL {
                    level -= 1;
                    if level < 0 {
                        return err("Brackets do not match!");
                    }
                    if level == 0 {
                        let Some(open_id) = open else {
                            return err("Brackets do not match!");
                        };
                        let b = self.structure_brackets(open_id, id)?;
                        brackets.push(b);
                        if nested {
                            let subs = self
                                .arena
                                .descendants_named_any(b, &[SUBSTITUENT_EL, ROOT_EL]);
                            self.find_and_structure_brackets(&subs, brackets)?
                        }
                        open = None;
                        nested = false;
                    }
                }
            }
        }
        if level != 0 {
            return err("Brackets do not match!");
        }
        Ok(())
    }
    fn structure_brackets(&mut self, open: NodeId, close: NodeId) -> Result<NodeId> {
        let bracket = self.arena.grouping(BRACKET_EL);
        let Some(mut current) = self.arena[open].parent else {
            return err("Brackets within a word do not match!");
        };
        self.arena.insert_before(current, bracket);
        while self.arena[current].children.first().copied() != Some(open) {
            let first = self.arena[current].children[0];
            self.arena.detach(first);
            self.arena.add_child(bracket, first)
        }
        while Some(current) != self.arena[close].parent {
            let next = self.arena.next_sibling(current);
            self.arena.detach(current);
            self.arena.add_child(bracket, current);
            let Some(next) = next else {
                return err("Brackets within a word do not match!");
            };
            current = next;
        }
        self.arena.detach(current);
        self.arena.add_child(bracket, current);
        let mut current = self.arena.next_sibling(close);
        while let Some(n) = current {
            current = self.arena.next_sibling(n);
            self.arena.detach(n);
            self.arena.add_child(bracket, n)
        }
        self.arena.detach(open);
        self.arena.detach(close);
        Ok(bracket)
    }
    fn process_hydrocarbon_rings(&mut self, sub: NodeId) -> Result {
        for id in self.arena.children_named(sub, ANNULEN_EL) {
            let value = self.val(id);
            let pattern = rx(r"(?i)^[\[\(\{]([1-9]\d*)[\]\)\}]annul(en|yn)$");
            let Some(c) = pattern.captures(&value) else {
                return err("Invalid annulen tag");
            };
            let size = c[1]
                .parse::<usize>()
                .map_err(|_| ComponentGenerationError("Invalid annulene size".into()))?;
            if size < 3 {
                return err("Invalid annulene size");
            }
            let smiles = format!(
                "{}{}1",
                if c[2].eq_ignore_ascii_case("yn") {
                    "C1#C"
                } else {
                    "c1c"
                },
                "c".repeat(size - 2)
            );
            let g = self.token(
                GROUP_EL,
                value,
                &[
                    (VALUE_ATR, &smiles),
                    (LABELS_ATR, NUMERIC_LABELS_VAL),
                    (TYPE_ATR, RING_TYPE_VAL),
                    (SUBTYPE_ATR, RING_SUBTYPE_VAL),
                ],
            );
            let parent = self.arena[id].parent.unwrap();
            self.arena.replace_child(parent, id, g);
        }
        for id in self
            .arena
            .children_named(sub, HYDROCARBONFUSEDRINGSYSTEM_EL)
        {
            let Some(m) = self
                .arena
                .previous_sibling(id)
                .filter(|&x| self.arena[x].name == MULTIPLIER_EL)
            else {
                return err("Invalid semi-trivially named hydrocarbon fused ring system");
            };
            let count = self.num(m, VALUE_ATR)?;
            let kind = self.attr(id, VALUE_ATR);
            let mut smiles = String::new();
            match kind.as_str() {
                "polyacene" => {
                    if count <= 3 {
                        return err("Invalid polyacene");
                    }
                    smiles.push_str("c1ccc");
                    for j in 2..=count {
                        smiles.push_str(&format!("c{}c", ring_closure(j)))
                    }
                    smiles.push_str("ccc");
                    for j in (3..=count).rev() {
                        smiles.push_str(&format!("c{}c", ring_closure(j)))
                    }
                    smiles.push_str("c12")
                }
                "polyaphene" => {
                    if count <= 3 {
                        return err("Invalid polyaphene");
                    }
                    smiles.push_str("c1ccc");
                    let above = (count - 1) / 2;
                    let on = if count % 2 == 0 { above + 1 } else { above };
                    let mut ring = 2;
                    for _ in 0..above {
                        smiles.push_str(&format!("c{}c", ring_closure(ring)));
                        ring += 1
                    }
                    for _ in 0..on {
                        smiles.push_str(&format!("cc{}", ring_closure(ring)));
                        ring += 1
                    }
                    smiles.push_str("ccc");
                    ring -= 1;
                    for _ in 0..on {
                        smiles.push_str(&format!("cc{}", ring_closure(ring)));
                        ring -= 1
                    }
                    for _ in 1..above {
                        smiles.push_str(&format!("c{}c", ring_closure(ring)));
                        ring -= 1
                    }
                    smiles.push_str("c12");
                }
                "polyalene" => {
                    if count < 5 {
                        return err("Invalid polyalene");
                    }
                    smiles = format!("c1{}c2{}c12", "c".repeat(count - 3), "c".repeat(count - 2))
                }
                "polyphenylene" => {
                    if count < 2 {
                        return err("Invalid polyphenylene");
                    }
                    smiles = format!("c1cccc2{}c12", "c3ccccc3".repeat(count - 1))
                }
                "polynaphthylene" => {
                    if count < 3 {
                        return err("Invalid polynaphthylene");
                    }
                    smiles = format!("c1cccc2cc3{}c3cc12", "c4cc5ccccc5cc4".repeat(count - 1))
                }
                "polyhelicene" => {
                    if count < 4 {
                        return err("Invalid polyhelicene");
                    }
                    smiles.push_str("c1c");
                    let mut ring = 2;
                    for _ in 1..count {
                        smiles.push_str(&format!("ccc{}", ring_closure(ring)));
                        ring += 1
                    }
                    smiles.push_str("cccc");
                    ring -= 1;
                    for _ in 2..count {
                        smiles.push_str(&format!("c{}", ring_closure(ring)));
                        ring -= 1
                    }
                    smiles.push_str("c12")
                }
                _ => return err("Unknown semi-trivially named hydrocarbon fused ring system"),
            }
            let name = self.val(m) + &self.val(id);
            let g = self.token(
                GROUP_EL,
                name,
                &[
                    (VALUE_ATR, &smiles),
                    (LABELS_ATR, FUSEDRING_LABELS_VAL),
                    (TYPE_ATR, RING_TYPE_VAL),
                    (SUBTYPE_ATR, HYDROCARBONFUSEDRINGSYSTEM_EL),
                ],
            );
            let parent = self.arena[id].parent.unwrap();
            self.arena.replace_child(parent, id, g);
            self.arena.detach(m);
        }
        Ok(())
    }
    fn handle_suffix_irregularities(&mut self, sub: NodeId) -> Result {
        for s in self.arena.children_named(sub, SUFFIX_EL) {
            let value = self.val(s);
            match value.as_str() {
                "ic" | "ous" => {
                    if !self.context.options.interpret_acids_without_the_word_acid
                        && self.arena.next_element(s, true).is_none()
                    {
                        return err(format!("\"acid\" not found after {value}"));
                    }
                }
                "quinone" | "quinon" => {
                    self.arena[s].remove_attribute(ADDITIONALVALUE_ATR);
                    self.arena[s].set_value("one");
                    if let Some(m) = self
                        .arena
                        .previous_sibling(s)
                        .filter(|&x| self.arena[x].name == MULTIPLIER_EL)
                    {
                        let count = self.num(m, VALUE_ATR)? * 2;
                        self.set(m, VALUE_ATR, count.to_string())
                    } else {
                        let m = self.token(MULTIPLIER_EL, "di", &[(VALUE_ATR, "2")]);
                        self.arena.insert_before(s, m)
                    }
                }
                "ylene" | "ylen" => {
                    self.arena[s].remove_attribute(ADDITIONALVALUE_ATR);
                    self.arena[s].set_value("yl");
                    let Some(g) = self.arena.previous_sibling_named(s, GROUP_EL) else {
                        return err("OPSIN bug: no group before ylene");
                    };
                    self.arena[g].remove_attribute(USABLEASJOINER_ATR);
                    let m = self.token(MULTIPLIER_EL, "di", &[(VALUE_ATR, "2")]);
                    self.arena.insert_before(s, m)
                }
                "ylium"
                    if self.attr_is(s, VALUE_ATR, "acylium")
                        && self.arena[s].attribute(SUFFIXPREFIX_ATR).is_none()
                        && self.arena[s].attribute(INFIX_ATR).is_none() =>
                {
                    let g = self.arena.previous_sibling_named(s, GROUP_EL);
                    if !g.is_some_and(|g| {
                        [
                            ACIDSTEM_TYPE_VAL,
                            CHALCOGENACIDSTEM_TYPE_VAL,
                            NONCARBOXYLICACID_TYPE_VAL,
                        ]
                        .contains(&self.attr(g, TYPE_ATR).as_str())
                    }) {
                        let ends_o = self.arena.previous_sibling(s).is_some_and(|p| {
                            self.attr(p, SUBSEQUENTUNSEMANTICTOKEN_ATR)
                                .to_ascii_lowercase()
                                .ends_with('o')
                        });
                        if !ends_o {
                            if let Some(g) = g.filter(|&g| {
                                self.attr_is(g, SUBTYPE_ATR, ARYLSUBSTITUENT_SUBTYPE_VAL)
                            }) {
                                let _ = g;
                                self.set(s, VALUE_ATR, "ylium");
                                self.set(s, TYPE_ATR, CHARGE_TYPE_VAL);
                                self.arena[s].remove_attribute(SUBTYPE_ATR);
                            } else {
                                return err(
                                    "ylium is intended to be the removal of H- in this context not the formation of an acylium ion",
                                );
                            }
                        }
                    }
                }
                "nitrolic acid" | "nitrolicacid"
                    if self.arena.previous_sibling_named(s, GROUP_EL).is_none() =>
                {
                    if self.arena[sub].children.len() != 1 {
                        return err("OPSIN Bug: nitrolic acid not expected to have sibilings");
                    }
                    let Some(prev) = self
                        .arena
                        .previous_sibling(sub)
                        .filter(|&p| self.arena[p].name == SUBSTITUENT_EL)
                    else {
                        return err("Expected substituent before nitrolic acid");
                    };
                    let suffixes = self.arena.children_named(prev, SUFFIX_EL);
                    if suffixes.len() != 1 {
                        return err(
                            "Only the nitrolic acid case where it is preceded by an yl suffix is supported",
                        );
                    }
                    if self.val(suffixes[0]) != "yl" {
                        return err("Unexpected suffix found before nitrolic acid");
                    }
                    self.arena.detach(suffixes[0]);
                    for child in self.arena[prev].children.clone() {
                        self.arena.detach(child);
                        self.arena.insert_before(s, child)
                    }
                    self.arena.detach(prev);
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn detect_alkane_fused_ring_bridges(&mut self, g: NodeId) {
        if self.attr_is(g, SUBTYPE_ATR, ALKANESTEM_SUBTYPE_VAL)
            && let Some(u) = self
                .arena
                .next_sibling(g)
                .filter(|&x| self.arena[x].name == UNSATURATOR_EL)
            && let Some(o) = self
                .arena
                .next_sibling_ignoring(g, &[UNSATURATOR_EL])
                .filter(|&x| self.arena[x].name == BRIDGEFORMINGO_EL)
        {
            self.arena[g].name = FUSEDRINGBRIDGE_EL.into();
            self.set(g, VALUE_ATR, format!("-{}-", self.attr(g, VALUE_ATR)));
            self.arena.detach(o);
            self.arena.detach(u)
        }
    }
    fn process_rings(&mut self, g: NodeId) -> Result {
        if let Some(p) = self.arena.previous_sibling_ignoring(g, &[LOCANT_EL]) {
            match self.arena[p].name.as_str() {
                SPIRO_EL => self.process_spiro_system(g, p)?,
                VONBAEYER_EL => self.process_von_baeyer_system(g, p)?,
                CYCLO_EL => self.process_cyclised_chain(g, p)?,
                _ => {}
            }
        }
        Ok(())
    }
    fn process_spiro_system(&mut self, g: NodeId, s: NodeId) -> Result {
        let mut bridges = spiro_bridges(remove_dash(&self.val(s)))?;
        if bridges.len() < 2 {
            return err("Invalid spiro descriptor");
        }
        let mut count = 1;
        if let Some(m) = self.arena.previous_sibling(s).filter(|&m| {
            self.arena[m].name == MULTIPLIER_EL && self.attr_is(m, TYPE_ATR, BASIC_TYPE_VAL)
        }) {
            count = self.num(m, VALUE_ATR)?;
            self.arena.detach(m)
        }
        let mut atoms = bridges.iter().map(|b| b.len).sum::<usize>() + count;
        let expected = self.attr(g, VALUE_ATR).len();
        if atoms != expected {
            if atoms > expected && bridges.len() > 2 && !bridges.iter().any(|b| b.explicit) {
                let mut inferred = 0;
                for (i, b) in bridges.iter_mut().enumerate() {
                    let mut n = b.len;
                    if i > 1 && n >= 11 {
                        let text = n.to_string();
                        n = text[..1].parse().unwrap();
                        let loc = text[1..].parse::<usize>().unwrap();
                        if loc > 0 {
                            *b = SpiroBridge {
                                len: n,
                                locant: Some(loc),
                                explicit: false,
                            }
                        }
                    }
                    inferred += n
                }
                inferred += count;
                if inferred == expected {
                    atoms = inferred
                }
            }
            if atoms != expected {
                return err(format!(
                    "Disagreement between number of atoms in spiro descriptor: {atoms} and number of atoms in chain: {expected}"
                ));
            }
        }
        let mut opened = 1;
        let mut index = 2usize;
        let mut smiles = format!("C0{}10(", "C".repeat(bridges[0].len));
        for b in &bridges[1..] {
            if let Some(loc) = b.locant {
                let mut pos = find_index_of_ring_openings(&smiles, loc)?;
                let mut label = smiles
                    .get(pos..pos + 1)
                    .ok_or_else(|| ComponentGenerationError("Invalid spiro ring opening".into()))?
                    .to_string();
                pos += 1;
                if label == "%" {
                    while pos < smiles.len() && smiles.as_bytes()[pos].is_ascii_digit() {
                        label.push(smiles.as_bytes()[pos] as char);
                        pos += 1
                    }
                }
                if smiles[pos..].contains(&format!("C{label}")) {
                    smiles.insert_str(pos, &ring_closure(index));
                    smiles.push_str(&format!("({}{})", "C".repeat(b.len), ring_closure(index)));
                    index += 1
                } else {
                    smiles.push_str(&format!("{}{label})", "C".repeat(b.len)))
                }
            } else if opened >= count {
                smiles.push_str(&"C".repeat(b.len));
                index = index
                    .checked_sub(1)
                    .ok_or_else(|| ComponentGenerationError("Invalid spiro descriptor".into()))?;
                smiles.push_str(&format!("{})", ring_closure(index)))
            } else {
                smiles.push_str(&format!("{}C{}(", "C".repeat(b.len), ring_closure(index)));
                index += 1;
                opened += 1
            }
        }
        self.set(g, VALUE_ATR, smiles);
        self.set(g, TYPE_ATR, RING_TYPE_VAL);
        self.arena[g].remove_attribute(USABLEASJOINER_ATR);
        self.arena.detach(s);
        Ok(())
    }
    fn process_von_baeyer_system(&mut self, g: NodeId, s: NodeId) -> Result {
        let Some(m) = self.arena.previous_sibling(s) else {
            return err("Missing Von Baeyer multiplier");
        };
        let rings = self.num(m, VALUE_ATR)?;
        self.arena.detach(m);
        let mut elements = unbranched_elements(&self.attr(g, VALUE_ATR))?;
        let chain_len = elements.len();
        let text = remove_dash(&self.val(s)).to_string();
        let prefix = if text.find('-') == Some(5) { 7 } else { 6 };
        let Some(text) = text.get(prefix..text.len().saturating_sub(1)) else {
            return err("Invalid Von Baeyer descriptor");
        };
        let descriptors = rx("[.,]")
            .split(text)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut bridges = Vec::new();
        let mut locations: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut labels = 3;
        let mut total = 0;
        let mut i = 0;
        while i < descriptors.len() {
            let desc = &descriptors[i];
            let mut b = VonBaeyerBridge::default();
            if i > 2 {
                i += 1;
                let Some(second) = descriptors.get(i) else {
                    return err("Invalid Von Baeyer secondary bridge descriptor");
                };
                let second = rx(r"\D+").replace_all(second, "").to_string();
                let numbers = rx(r"\D+")
                    .split(desc)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                let (len, first) = if numbers.len() == 1 {
                    match desc.len() {
                        2 | 3 => (parse_number(&desc[..1])?, desc[1..].to_string()),
                        4 => (parse_number(&desc[..2])?, desc[2..].to_string()),
                        _ => {
                            return err(format!(
                                "Unsupported Von Baeyer locant description: {desc}"
                            ));
                        }
                    }
                } else if numbers.len() > 1 {
                    (parse_number(&numbers[0])?, numbers[1].clone())
                } else {
                    return err("Invalid Von Baeyer descriptor");
                };
                let mut a = parse_number(&first)?;
                let mut c = parse_number(&second)?;
                if a > chain_len || c > chain_len {
                    return err(format!(
                        "Indicated bridge position is not on chain: {a},{c}"
                    ));
                }
                if c > a {
                    std::mem::swap(&mut a, &mut c)
                }
                b.len = len;
                b.larger = Some(a);
                b.smaller = Some(c);
                b.larger_label = labels;
                locations.entry(a).or_default().push(labels);
                labels += 1;
                b.smaller_label = if len == 0 { labels - 1 } else { labels };
                locations.entry(c).or_default().push(b.smaller_label);
                labels += 1;
            } else {
                b.len = parse_number(desc)?
            }
            total += b.len;
            bridges.push(b);
            i += 1;
        }
        if total + 2 != chain_len {
            return err("Disagreement between lengths of bridges and alkyl chain length");
        }
        if rings + 1 != bridges.len() {
            return err("Disagreement between number of rings and number of bridges");
        }
        let mut smiles = String::new();
        let mut atom = 1;
        for (index, b) in bridges.iter().take(3).enumerate() {
            if index == 0 {
                smiles.push_str(&pop_element(&mut elements)?);
                smiles.push('1');
                append_ring_locations(&mut smiles, &locations, atom);
                smiles.push('(')
            }
            for _ in 0..b.len {
                atom += 1;
                smiles.push_str(&pop_element(&mut elements)?);
                append_ring_locations(&mut smiles, &locations, atom)
            }
            if index == 0 {
                atom += 1;
                smiles.push_str(&pop_element(&mut elements)?);
                smiles.push('2');
                append_ring_locations(&mut smiles, &locations, atom)
            }
            if index == 1 {
                smiles.push_str("1)")
            }
            if index == 2 {
                smiles.push('2')
            }
        }
        let mut secondary = bridges
            .into_iter()
            .filter(|b| b.larger.is_some() && b.len > 0)
            .collect::<Vec<_>>();
        // Retain descriptor order when the descending bridge keys are equal.
        secondary
            .sort_by_key(|bridge| std::cmp::Reverse((bridge.larger, bridge.smaller, bridge.len)));
        while !secondary.is_empty() {
            let mut dependent = Vec::new();
            let prev = secondary.len();
            for b in secondary {
                if b.larger.unwrap() > atom {
                    dependent.push(b);
                    continue;
                }
                smiles.push('.');
                for j in 0..b.len {
                    atom += 1;
                    smiles.push_str(&pop_element(&mut elements)?);
                    if j == 0 {
                        smiles.push_str(&ring_closure(b.larger_label))
                    }
                    append_ring_locations(&mut smiles, &locations, atom)
                }
                smiles.push_str(&ring_closure(b.smaller_label))
            }
            if dependent.len() == prev {
                return err("Unable to resolve all dependant bridges!!!");
            }
            secondary = dependent;
        }
        self.set(g, VALUE_ATR, smiles);
        self.set(g, TYPE_ATR, RING_TYPE_VAL);
        self.arena[g].remove_attribute(USABLEASJOINER_ATR);
        self.arena.detach(s);
        Ok(())
    }
    fn process_cyclised_chain(&mut self, g: NodeId, c: NodeId) -> Result {
        let mut smiles = self.attr(g, VALUE_ATR);
        let len = smiles
            .chars()
            .filter(|ch| ch.is_uppercase() && *ch != 'H')
            .count();
        if len < 3 {
            return err(format!(
                "Heteroatom chain too small to create a ring: {len}"
            ));
        }
        smiles.push('1');
        let end = if smiles.starts_with('[') {
            smiles
                .find(']')
                .ok_or_else(|| ComponentGenerationError("Malformed chain SMILES".into()))?
                + 1
        } else if smiles.as_bytes().get(1).is_some_and(u8::is_ascii_lowercase) {
            2
        } else {
            1
        };
        smiles.insert(end, '1');
        self.set(g, VALUE_ATR, smiles);
        if len == 6 {
            self.set(g, LABELS_ATR, "1/2,ortho/3,meta/4,para/5/6")
        }
        self.set(g, TYPE_ATR, RING_TYPE_VAL);
        self.arena[g].remove_attribute(USABLEASJOINER_ATR);
        self.arena.detach(c);
        Ok(())
    }
    fn handle_group_irregularities(&mut self, g: NodeId) -> Result {
        let value = self.val(g);
        let kind = self.attr(g, TYPE_ATR);
        let subtype = self.attr(g, SUBTYPE_ATR);
        if !self.context.options.interpret_acids_without_the_word_acid
            && self.arena[g].attribute(FUNCTIONALIDS_ATR).is_some()
            && (value.ends_with("ic") || value.ends_with("ous"))
            && self.arena.next_element(g, true).is_none()
        {
            return err(format!("\"acid\" not found after {value}"));
        }
        if subtype == OUSICATOM_SUBTYPE_VAL && self.arena.next_element(g, false).is_none() {
            return err(format!("counter anion not found after {value}"));
        }
        // This is one ordered conditional chain upstream: specialized names must
        // take precedence over the subtype-based clauses at the end.
        if ["thiophen", "selenophen", "tellurophen"].contains(&value.as_str()) {
            if !self.attr_is(g, SUBSEQUENTUNSEMANTICTOKEN_ATR, "e")
                && let Some(s) = self
                    .arena
                    .next_sibling(g)
                    .filter(|&s| self.arena[s].name == SUFFIX_EL && self.val(s).starts_with("ol"))
            {
                let _ = s;
                if !self.arena.previous_sibling(g).is_some_and(|p| {
                    self.arena[p].name == LOCANT_EL && self.val(p).split(',').count() == 1
                }) {
                    return err(format!(
                        "{value}ol has been incorrectly interpreted as {value}, ol instead of phenol with the oxgen replaced"
                    ));
                }
            }
        } else if value == "chromen" {
            if self.arena.previous_sibling(g).is_some_and(|p| {
                self.arena[p].name == LOCANT_EL && ["2", "3"].contains(&self.val(p).as_str())
            }) && (self.arena.next_sibling(g).is_none()
                || self.is(self.arena.next_sibling(g), LOCANT_EL))
            {
                self.set(g, VALUE_ATR, "O1CCCc2ccccc12");
                self.add(g, ADDBOND_ATR, "2 locant required");
                self.add(g, FRONTLOCANTSEXPECTED_ATR, "2,3");
            }
        } else if ["methylene", "methylen"].contains(&value.as_str()) {
            if !self.is(self.arena.previous_sibling(g), MULTIPLIER_EL) {
                self.combine_dioxy(g, "C(O)O", "2,3")?
            }
        } else if ["ethylene", "ethylen"].contains(&value.as_str()) {
            if let Some(m) = self
                .arena
                .previous_sibling(g)
                .filter(|&m| self.arena[m].name == MULTIPLIER_EL)
            {
                let count = self.num(m, VALUE_ATR)?;
                let parent = self.arena[g].parent.unwrap();
                let root = self.arena.next_sibling(parent);
                if root.is_none()
                    && self
                        .arena
                        .parent_word_rule(g)
                        .is_some_and(|w| self.attr_is(w, WORDRULE_ATR, "glycol"))
                {
                    self.set(
                        g,
                        VALUE_ATR,
                        format!("CC{}", "OCC".repeat(count.saturating_sub(1))),
                    );
                    self.set(
                        g,
                        OUTIDS_ATR,
                        format!("1,{}", 3 * count.saturating_sub(1) + 2),
                    );
                    self.arena.detach(m);
                    self.set(g, LABELS_ATR, NUMERIC_LABELS_VAL);
                } else if let Some(root) = root
                    .filter(|&r| self.arena[r].name == ROOT_EL && self.arena[r].children.len() == 2)
                {
                    let children = self.arena[root].children.clone();
                    let mult = children[0];
                    let amine = children[1];
                    if self.arena[mult].name == MULTIPLIER_EL
                        && ["amine", "amin"].contains(&self.val(amine).as_str())
                    {
                        if self.num(mult, VALUE_ATR)? != count + 1 {
                            return err("Invalid polyethylene amine!");
                        }
                        self.arena[g].remove_attribute(OUTIDS_ATR);
                        self.set(g, VALUE_ATR, format!("{}N", "NCC".repeat(count)));
                        self.arena.detach(m);
                        self.arena.detach(root);
                        self.arena[parent].name = ROOT_EL.into();
                        self.set(g, LABELS_ATR, NUMERIC_LABELS_VAL);
                    }
                }
            } else {
                self.combine_dioxy(g, "C(O)CO", "2,4")?
            }
        } else if ["propylene", "propylen"].contains(&value.as_str()) {
            if let Some(m) = self
                .arena
                .previous_sibling(g)
                .filter(|&m| self.arena[m].name == MULTIPLIER_EL)
            {
                let count = self.num(m, VALUE_ATR)?;
                let parent = self.arena[g].parent.unwrap();
                if self.arena.next_sibling(parent).is_none()
                    && self
                        .arena
                        .parent_word_rule(g)
                        .is_some_and(|w| self.attr_is(w, WORDRULE_ATR, "glycol"))
                {
                    self.set(
                        g,
                        VALUE_ATR,
                        format!("CCC{}", "OC(C)C".repeat(count.saturating_sub(1))),
                    );
                    self.set(
                        g,
                        OUTIDS_ATR,
                        format!("2,{}", 4 * count.saturating_sub(1) + 3),
                    );
                    self.set(g, LABELS_ATR, NONE_LABELS_VAL);
                    self.arena.detach(m);
                }
            }
        } else if [
            "anthr",
            "anthran",
            "phenanthr",
            "acrid",
            "xanth",
            "thioxanth",
            "selenoxanth",
            "telluroxanth",
            "xanthen",
        ]
        .contains(&value.as_str())
        {
            if !self.is(self.arena.previous_sibling(g), LOCANT_EL)
                && let Some(s) = self.arena.next_sibling(g)
            {
                if self.attr_is(s, VALUE_ATR, "one") {
                    let loc = self.token(LOCANT_EL, "9", &[]);
                    self.arena.insert_before(s, loc);
                    let h = self.token(ADDEDHYDROGEN_EL, "", &[(LOCANT_ATR, "10")]);
                    self.arena.insert_before(loc, h);
                } else if self.arena[s].name == SUFFIX_EL
                    && ["xanth", "thioxanth", "selenoxanth", "telluroxanth"]
                        .contains(&value.as_str())
                    && ["ic", "ate"].contains(&self.attr(s, VALUE_ATR).as_str())
                {
                    return err(format!(
                        "{value}{} is not a derivative of xanthene",
                        self.val(s)
                    ));
                }
            }
        } else if value == "phospho" {
            if let Some(w) = self.arena.parent_word_rule(g) {
                for other in self.arena.descendants_named(w, GROUP_EL) {
                    let t = self.attr(other, TYPE_ATR);
                    let s = self.attr(other, SUBTYPE_ATR);
                    let v = self.val(other);
                    if s == BIOCHEMICAL_SUBTYPE_VAL
                        || t == CARBOHYDRATE_TYPE_VAL
                        || t == AMINOACID_TYPE_VAL
                        || (s == YLFORACYL_SUBTYPE_VAL
                            && ["glycol", "diglycol"].contains(&v.as_str()))
                        || (s == YLFORYL_SUBTYPE_VAL && v == "glycer")
                    {
                        self.set(g, VALUE_ATR, "-P(=O)(O)O");
                        self.add(g, USABLEASJOINER_ATR, "yes");
                        break;
                    }
                }
            }
        } else if value == "hydrogen" {
            let parent = self.arena[g].parent.unwrap();
            if let Some(next) = self.arena.next_sibling(parent) {
                let Some(&ate) = self.arena[next].children.first() else {
                    return err("Hydrogen is not meant as a substituent in this context!");
                };
                if self.arena[ate].name != GROUP_EL
                    || !self.attr_is(ate, TYPE_ATR, NONCARBOXYLICACID_TYPE_VAL)
                {
                    return err("Hydrogen is not meant as a substituent in this context!");
                }
                let mut count = "1".to_string();
                if let Some(m) = self
                    .arena
                    .previous_sibling(g)
                    .filter(|&m| self.arena[m].name == MULTIPLIER_EL)
                {
                    count = self.attr(m, VALUE_ATR);
                    self.arena.detach(m)
                }
                self.add(ate, NUMBEROFFUNCTIONALATOMSTOREMOVE_ATR, count);
                self.arena.detach(g);
                for child in self.arena[parent].children.clone().into_iter().rev() {
                    self.arena.detach(child);
                    self.arena.insert_child(next, child, 0)
                }
                self.arena.detach(parent);
            }
        } else if value == "acryl" {
            if subtype == SIMPLESUBSTITUENT_SUBTYPE_VAL
                && self
                    .arena
                    .next_element(g, true)
                    .is_some_and(|n| self.val(n) == "amid")
            {
                return err("amide in acrylamide is not [NH2-]");
            }
        } else if [
            "azo",
            "azoxy",
            "nno-azoxy",
            "non-azoxy",
            "onn-azoxy",
            "diazoamino",
            "hydrazo",
        ]
        .contains(&value.as_str())
        {
            let parent = self.arena[g].parent.unwrap();
            let mut next = self.arena.next_sibling_ignoring(parent, &[HYPHEN_EL]);
            if next.is_none()
                && self.arena.previous_sibling(parent).is_none()
                && let Some(p) = self.arena[parent].parent
            {
                next = self.arena.next_sibling_ignoring(p, &[HYPHEN_EL])
            }
            if let Some(next) = next.filter(|&n| self.arena[n].name == ROOT_EL)
                && !self.is(self.arena[next].children.first().copied(), MULTIPLIER_EL)
                && self.arena.children_named(next, SUFFIX_EL).is_empty()
            {
                let m = self.token(MULTIPLIER_EL, "", &[(VALUE_ATR, "2")]);
                self.arena.insert_child(next, m, 0);
                if let Some(p) = self
                    .arena
                    .previous_element(g, true)
                    .filter(|&p| self.arena[p].name != HYPHEN_EL)
                {
                    let h = self.token(HYPHEN_EL, "", &[]);
                    self.arena.insert_after(p, h)
                }
            }
        } else if ["coenzyme a", "coa"].contains(&value.as_str()) {
            self.set_prior_multiacid_suffix(g, true)?;
            let parent = self.arena[g].parent.unwrap();
            let outer = self.arena[parent].parent.unwrap();
            let index = self.arena.index_of(outer, parent).unwrap();
            if index > 0 {
                let bracket = self.arena.grouping(BRACKET_EL);
                let preceding = self.arena[outer].children[..index].to_vec();
                for p in preceding {
                    self.arena.detach(p);
                    self.arena.add_child(bracket, p)
                }
                self.arena.insert_before(parent, bracket)
            }
        } else if matches(
            &value,
            "(?:e?icosa)?sphinganin[e]?|phytosphingosin[e]?|sphingosin[e]?",
        ) {
            if let Some(parent) = self.arena[g].parent
                && let Some(prev) = self.arena.previous_sibling(parent)
                && let Some(acid) = self
                    .arena
                    .descendants_named(prev, GROUP_EL)
                    .last()
                    .copied()
                    .filter(|&a| self.attr_is(a, SUBTYPE_ATR, ALKANESTEM_SUBTYPE_VAL))
            {
                let suffixes = self.arena.children_with_attribute(
                    self.arena[acid].parent.unwrap(),
                    SUFFIX_EL,
                    TYPE_ATR,
                    INLINE_TYPE_VAL,
                );
                if suffixes.len() == 1 && self.attr_is(suffixes[0], VALUE_ATR, "yl") {
                    self.set(suffixes[0], VALUE_ATR, "oyl")
                }
            }
        } else if value == "sel" {
            if subtype == HETEROSTEM_SUBTYPE_VAL
                && self.arena[g]
                    .attribute(SUBSEQUENTUNSEMANTICTOKEN_ATR)
                    .is_none()
                && let Some(u) = self
                    .arena
                    .next_sibling(g)
                    .filter(|&u| self.arena[u].name == UNSATURATOR_EL && self.val(u) == "en")
                && self
                    .arena
                    .next_sibling(u)
                    .is_some_and(|s| self.arena[s].name == SUFFIX_EL && self.val(s) == "ium")
            {
                return err(
                    "<multiplier>selenium does not indicate a chain of selenium atoms with a double bond and a positive charge",
                );
            }
        } else if ["keto", "aldehydo"].contains(&value.as_str())
            && subtype == SIMPLESUBSTITUENT_SUBTYPE_VAL
        {
            self.process_open_chain_carbohydrate(g)?;
        } else if [
            "bor",
            "antimon",
            "arsen",
            "phosphor",
            "phosphate",
            "phosphat",
            "silicicacid",
            "silicic acid",
            "silicate",
            "silicat",
        ]
        .contains(&value.as_str())
        {
            self.process_inorganic_acid(g)?;
        } else if value == "pyruv" {
            let parent = self.arena[g].parent.unwrap();
            if let Some(prev) = self
                .arena
                .previous_sibling(parent)
                .filter(|&p| self.arena.previous_sibling(p).is_some())
                && let Some(sub) = self.arena.first_child_named(prev, GROUP_EL).filter(|&s| {
                    matches(&self.val(s), "thio|seleno|telluro")
                        && self.arena.next_sibling(s).is_none()
                })
            {
                let hyphen = self.token(HYPHEN_EL, "", &[]);
                self.arena.insert_after(sub, hyphen)
            }
        } else if subtype == ENDINIC_SUBTYPE_VAL && kind == AMINOACID_TYPE_VAL {
            if self.attr(g, SUFFIXAPPLIESTO_ATR).split(',').count() == 2
                && let Some(yl) = self.arena.next_sibling(g).filter(|&s| {
                    self.attr_is(s, VALUE_ATR, "yl")
                        && self.arena[s].attribute(ADDITIONALVALUE_ATR).is_none()
                })
            {
                self.add(yl, ADDITIONALVALUE_ATR, "ic")
            }
        } else if subtype == SALTCOMPONENT_SUBTYPE_VAL {
            let mut parse = g;
            while let Some(p) = self.arena[parse].parent {
                parse = p
            }
            if self.arena[parse].children.len() <= 1 {
                return err(format!(
                    "Group expected to be part of a salt but only one component found. Could be a class of compound: {value}"
                ));
            }
            if value
                .as_bytes()
                .first()
                .is_some_and(|c| (b'1'..=b'9').contains(c))
            {
                if self.is(self.arena.previous_sibling(g), MULTIPLIER_EL) {
                    return err(format!("Unepxected multiplier found before: {value}"));
                }
                let digit = &value[..1];
                let m = self.token(
                    MULTIPLIER_EL,
                    digit,
                    &[(TYPE_ATR, BASIC_TYPE_VAL), (VALUE_ATR, digit)],
                );
                self.arena.insert_before(g, m);
                self.arena[g].set_value(&value[1..]);
            }
        } else if kind == ELEMENTARYATOM_TYPE_VAL
            && let Some(m) = self
                .arena
                .previous_sibling(g)
                .filter(|&m| self.attr_is(m, VALUE_ATR, "2"))
        {
            let mut parse = g;
            while let Some(p) = self.arena[parse].parent {
                parse = p
            }
            if self.arena.counts(parse).1 == 2 {
                let smiles = match self.attr(g, VALUE_ATR).as_str() {
                    "[H]" => Some("[H][H]"),
                    "[N]" => Some("N#N"),
                    "[O]" => Some("O=O"),
                    "[F]" => Some("FF"),
                    "[Cl]" => Some("ClCl"),
                    "[Br]" => Some("BrBr"),
                    "[I]" => Some("II"),
                    _ => None,
                };
                if let Some(smiles) = smiles {
                    let new = self.token(
                        GROUP_EL,
                        value,
                        &[
                            (TYPE_ATR, SIMPLEGROUP_TYPE_VAL),
                            (SUBTYPE_ATR, SIMPLEGROUP_SUBTYPE_VAL),
                            (VALUE_ATR, smiles),
                        ],
                    );
                    self.arena.insert_after(g, new);
                    self.arena.detach(g);
                    self.arena.detach(m)
                }
            }
        }
        if kind == AMINOACID_TYPE_VAL {
            self.set_prior_multiacid_suffix(g, false)?
        }
        Ok(())
    }
    fn combine_dioxy(&mut self, g: NodeId, smiles: &str, out: &str) -> Result {
        let parent = self.arena[g].parent.unwrap();
        if let Some(next) = self.arena.next_sibling(parent).filter(|&n| {
            self.arena[n].name == SUBSTITUENT_EL && self.arena.next_sibling(g).is_none()
        }) {
            let children = self.arena[next].children.clone();
            if children.len() >= 2
                && self.val(children[0]) == "di"
                && self.val(children[1]) == "oxy"
            {
                let value = self.val(g) + "dioxy";
                self.arena[g].set_value(value);
                self.set(g, VALUE_ATR, smiles);
                self.set(g, OUTIDS_ATR, out);
                self.set(g, SUBTYPE_ATR, EPOXYLIKE_SUBTYPE_VAL);
                // Upstream uses NONE_SUBTYPE_VAL when adding a missing label.
                let labels = if self.arena[g].attribute(LABELS_ATR).is_some() {
                    NONE_LABELS_VAL
                } else {
                    NONE_SUBTYPE_VAL
                };
                self.set(g, LABELS_ATR, labels);
                self.arena.detach(next);
                for &child in children[2..].iter().rev() {
                    self.arena.detach(child);
                    self.arena.insert_after(g, child)
                }
            }
        }
        Ok(())
    }
    fn set_prior_multiacid_suffix(&mut self, g: NodeId, coa: bool) -> Result {
        let Some(parent) = self.arena[g].parent else {
            return Ok(());
        };
        let Some(prev) = self.arena.previous_sibling(parent) else {
            return Ok(());
        };
        if let Some(acid) = self
            .arena
            .descendants_named(prev, GROUP_EL)
            .last()
            .copied()
            .filter(|&a| self.attr_is(a, TYPE_ATR, ACIDSTEM_TYPE_VAL))
        {
            if self.arena[acid]
                .attribute(SUFFIXAPPLIESTO_ATR)
                .is_some_and(|s| s.split(',').count() > 1)
                && let Some(s) = self
                    .arena
                    .next_sibling_named(acid, SUFFIX_EL)
                    .filter(|&s| self.arena[s].attribute(ADDITIONALVALUE_ATR).is_none())
            {
                self.add(s, ADDITIONALVALUE_ATR, "ic")
            }
            if coa
                && [YLFORYL_SUBTYPE_VAL, YLFORNOTHING_SUBTYPE_VAL]
                    .contains(&self.attr(acid, SUBTYPE_ATR).as_str())
            {
                self.set(acid, SUBTYPE_ATR, YLFORACYL_SUBTYPE_VAL)
            }
        }
        Ok(())
    }
    fn process_open_chain_carbohydrate(&mut self, g: NodeId) -> Result {
        let value = self.val(g);
        if value != "aldehydo" && self.is(self.arena.previous_sibling(g), LOCANT_EL) {
            return Ok(());
        }
        let parent = self.arena[g].parent.unwrap();
        let next = self.arena.next_sibling(parent);
        let mut current = next;
        let mut carbohydrate = None;
        while let Some(p) = current {
            if let Some(c) = self
                .arena
                .first_child_named(p, GROUP_EL)
                .filter(|&c| self.attr_is(c, TYPE_ATR, CARBOHYDRATE_TYPE_VAL))
            {
                carbohydrate = Some((p, c));
                break;
            }
            current = self.arena.next_sibling(p)
        }
        if let Some((p, c)) = carbohydrate {
            if !self
                .arena
                .children_named(p, CARBOHYDRATERINGSIZE_EL)
                .is_empty()
            {
                return err(format!(
                    "Carbohydrate has a specified ring size but {value} indicates the open chain form!"
                ));
            }
            if self
                .arena
                .children_named(p, SUFFIX_EL)
                .iter()
                .any(|&s| self.attr_is(s, VALUE_ATR, "yl"))
            {
                return err(format!(
                    "Carbohydrate appears to be a glycosyl, but {value} indicates the open chain form!"
                ));
            }
            if self
                .arena
                .previous_sibling_ignoring(c, &[STEREOCHEMISTRY_EL])
                .is_some_and(|l| {
                    self.arena[l].name == LOCANT_EL
                        && ["alpha", "beta", "alpha,beta", "beta,alpha"]
                            .contains(&self.val(l).as_str())
                })
            {
                return err(format!(
                    "Carbohydrate has alpha/beta anomeric form but {value} indicates the open chain form!"
                ));
            }
            self.arena.detach(g);
            for child in self.arena[parent].children.clone().into_iter().rev() {
                if self.arena[child].name != HYPHEN_EL {
                    self.arena.detach(child);
                    self.arena.insert_child(next.unwrap(), child, 0)
                }
            }
            self.arena.detach(parent);
            if self.attr_is(c, SUBTYPE_ATR, RING_SUBTYPE_VAL) {
                let Some(smiles) = self.arena[c]
                    .attribute(ADDITIONALVALUE_ATR)
                    .map(str::to_string)
                else {
                    return err(format!(
                        "{} can only describe the cyclic form but {value} indicates the open chain form!",
                        self.val(c)
                    ));
                };
                self.set(c, VALUE_ATR, smiles)
            }
        } else if value == "aldehydo" {
            return err("aldehydo is only a valid prefix when it precedes a carbohydrate!");
        }
        Ok(())
    }
    fn process_inorganic_acid(&mut self, g: NodeId) -> Result {
        let value = self.val(g);
        let mut suffix = None;
        let mut acid = None;
        if value.ends_with("acid") {
            if self.arena.next_element(g, true).is_none() {
                acid = Some(true)
            }
        } else if value.ends_with("ate") || value.ends_with("at") {
            if self.arena.next_element(g, true).is_none() {
                acid = Some(false)
            }
        } else {
            suffix = self.arena.next_sibling(g);
            if let Some(s) = suffix.filter(|&s| {
                self.arena[s].name == SUFFIX_EL
                    && self.arena[s].attribute(INFIX_ATR).is_none()
                    && self.arena.next_element(s, true).is_none()
            }) {
                acid = match self.attr(s, VALUE_ATR).as_str() {
                    "ic" => Some(true),
                    "ate" => Some(false),
                    _ => None,
                }
            }
        }
        let Some(acid) = acid else { return Ok(()) };
        let parent = self.arena[g].parent.unwrap();
        let Some(sub) = self
            .arena
            .previous_sibling(parent)
            .filter(|&s| [SUBSTITUENT_EL, BRACKET_EL].contains(&self.arena[s].name.as_str()))
        else {
            return Ok(());
        };
        let children = self.arena[sub].children.clone();
        let Some(&first) = children.first() else {
            return Ok(());
        };
        let mut smiles = None;
        if children.len() == 1
            && self.arena[first].name == GROUP_EL
            && ["fluoro", "fluor"].contains(&self.val(first).as_str())
        {
            let base = if value == "bor" {
                Some("F[B-](F)(F)F")
            } else if value == "antimon" {
                Some("F[Sb-](F)(F)(F)(F)F")
            } else if value.starts_with("silicic") || value.starts_with("silicat") {
                Some("F[Si|6-2](F)(F)(F)(F)F")
            } else {
                None
            };
            if let Some(base) = base {
                smiles = Some(format!(
                    "{base}{}",
                    if acid {
                        if value.starts_with("silic") {
                            ".[H+].[H+]"
                        } else {
                            ".[H+]"
                        }
                    } else {
                        ""
                    }
                ));
                self.arena.detach(sub)
            }
        } else if self.arena[first].name == MULTIPLIER_EL {
            let count = self.attr(first, VALUE_ATR);
            let base = if value == "bor"
                && (count == "4" || (count == "3" && self.arena.previous_sibling(sub).is_some()))
            {
                Some("[B-]")
            } else if value == "antimon" && count == "6" {
                Some("[Sb-]")
            } else if value == "arsen" && count == "6" {
                Some("[As-]")
            } else if value.starts_with("phosph") && count == "6" {
                Some("[P-]")
            } else if value.starts_with("silic") && count == "6" {
                Some("[Si|6-2]")
            } else {
                None
            };
            if let Some(base) = base {
                smiles = Some(format!(
                    "{base}{}",
                    if acid {
                        if value.starts_with("silic") {
                            ".[H+].[H+]"
                        } else {
                            ".[H+]"
                        }
                    } else {
                        ""
                    }
                ))
            }
        }
        if let Some(smiles) = smiles {
            self.set(g, VALUE_ATR, smiles);
            self.set(g, TYPE_ATR, SIMPLEGROUP_TYPE_VAL);
            self.set(g, SUBTYPE_ATR, SIMPLEGROUP_SUBTYPE_VAL);
            for key in [
                USABLEASJOINER_ATR,
                ACCEPTSADDITIVEBONDS_ATR,
                FUNCTIONALIDS_ATR,
            ] {
                self.arena[g].remove_attribute(key);
            }
            if let Some(s) = suffix {
                self.arena.detach(s)
            }
        }
        Ok(())
    }
    fn move_detachable_het_atom_repl(&mut self, bracket: NodeId) -> Result {
        let index = self.arena[bracket]
            .children
            .iter()
            .rposition(|&c| self.arena[c].name == HETEROATOM_EL);
        if let Some(index) = index {
            let mut group = None;
            let mut next = self.arena.next_sibling(bracket);
            while let Some(n) = next {
                if let Some(g) = self.arena.first_child_named(n, GROUP_EL) {
                    group = Some(g)
                }
                next = self.arena.next_sibling(n)
            }
            let Some(g) = group else {
                return err(format!(
                    "Unable to find group for: {} to apply to!",
                    self.arena[bracket]
                        .children
                        .first()
                        .map(|&c| self.val(c))
                        .unwrap_or_default()
                ));
            };
            let parent = self.arena[g].parent.unwrap();
            for i in (0..=index).rev() {
                let child = self.arena[bracket].children[i];
                self.arena.detach(child);
                self.arena.insert_child(parent, child, 0)
            }
        }
        Ok(())
    }
}

pub fn standardize_locant_variants(text: &str) -> String {
    let mut text = text.to_string();
    if text.contains('-') {
        let pattern = rx(&format!("^(\\d+'*)-({ELEMENT_SYMBOLS}'*)(.*)$"));
        if let Some(c) = pattern.captures(&text) {
            text = format!("{}{}{}", &c[2], &c[1], &c[3])
        }
    }
    if text.contains('\'')
        && let Some(c) = rx(r"^(\d+)('+)([a-z])$").captures(&text)
    {
        text = format!("{}{}{}", &c[1], &c[3], &c[2])
    }
    if text.chars().next().is_some_and(char::is_alphabetic) {
        let pattern = rx(&format!(
            "^({ELEMENT_SYMBOLS}'*)[\\^\\[\\(\\{{~\\*<]*(?:[sS][uU][pP][ ]?)?([^\\^\\[\\(\\{{~\\*<\\]\\)\\}}>]+)[^\\[\\(\\{{]*"
        ));
        if let Some(c) = pattern.captures(&text) {
            let replacement = format!("{}{}", &c[1], &c[2]);
            text = pattern.replace(&text, replacement.as_str()).to_string()
        }
        if text.len() >= 3 {
            text = rx("(?i:alpha|beta|gamma|delta|epsilon|zeta|eta|omega)")
                .replace_all(&text, |c: &regex::Captures<'_>| c[0].to_lowercase())
                .to_string()
        }
    }
    fix_locant_capitalisation(&text)
}
pub fn split_into_individual_locants(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut in_bracket = false;
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        match ch {
            ',' if !in_bracket => {
                result.push(text[start..i].to_string());
                start = i + 1
            }
            '(' | '[' | '{' => in_bracket = true,
            ')' | ']' | '}' => in_bracket = false,
            _ => {}
        }
    }
    result.push(text[start..].to_string());
    result
}
pub fn normalise_binary_brackets(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(input.into());
    }
    let Some(close) = input[1..].find(')').map(|i| i + 1) else {
        return Ok(input.into());
    };
    if close == input.len() - 1 {
        return Ok(input.into());
    }
    let first = &input[1..close];
    let mut index = close + 1;
    if input[index..].starts_with('-') {
        index += 1
    }
    let tail = input[index..].to_ascii_uppercase();
    let rac = if tail.starts_with("AND") {
        true
    } else if tail.starts_with("OR") {
        false
    } else {
        return Ok(input.into());
    };
    let Some(open) = input[index..].find('(').map(|i| i + index) else {
        return Ok(input.into());
    };
    let Some(close2) = input[open + 1..].find(')').map(|i| i + open + 1) else {
        return Ok(input.into());
    };
    let second = &input[open + 1..close2];
    if first.len() != second.len() {
        return err(format!(
            "Alternative stereochemistry brackets are different lengths: {first} {second}"
        ));
    }
    let mut out = "(".to_string();
    for (a, b) in first.chars().zip(second.chars()) {
        out.push(a);
        if a == b {
            continue;
        }
        if ['R', 'S', 'r', 's', 'E', 'Z'].contains(&a) {
            out.push(if rac { b } else { '*' })
        } else {
            return err(format!("Invalid combination of stereo brackets: {a} {b}"));
        }
    }
    out.push(')');
    Ok(out)
}
fn split_stereo_descriptors(text: &str) -> Vec<String> {
    if text.len() < 2 {
        return vec![text.into()];
    }
    let mut result = Vec::new();
    let mut part = String::new();
    for ch in text[1..text.len() - 1].chars() {
        if ch == ',' || (ch == '-' && matches(&part, STEREO)) {
            result.push(part);
            part = String::new()
        } else {
            part.push(ch)
        }
    }
    result.push(part);
    result
}
fn alpha_beta_symbol(symbol: &str) -> Result<&'static str> {
    match symbol.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a') => Ok("alpha"),
        Some('b') => Ok("beta"),
        Some('x') => Ok("xi"),
        _ => err("Malformed alpha/beta stereochemistry element"),
    }
}
fn check_hw_ambiguity(a: &str, b: &str, hydride: bool) -> Result {
    let element = rx("[A-Z][a-z]?");
    let Some(a) = element.find(a).map(|m| m.as_str()) else {
        return err("Failed to extract element from heteroatom");
    };
    let Some(b) = element.find(b).map(|m| m.as_str()) else {
        return err("Failed to extract element from heteroatom");
    };
    let priority = |s: &str| {
        [
            "Hg", "Tl", "In", "Ga", "Al", "B", "Pb", "Sn", "Ge", "Si", "Bi", "Sb", "As", "P", "N",
            "Te", "Se", "S", "O", "I", "Br", "Cl", "F",
        ]
        .iter()
        .position(|&v| v == s)
        .map(|i| i + 1)
        .unwrap_or(0)
    };
    if hydride {
        if priority(b) > priority(a) {
            return err(
                "heterogeneous hydride with alternating atoms misparsed as a Hantzch-widman ring",
            );
        }
    } else if priority(a) > priority(b)
        && ["O", "S", "Se", "Te", "Bi", "Hg"].contains(&b)
        && ![a, b].iter().any(|s| ["Si", "Ge", "Sn", "Pb"].contains(s))
    {
        return err(
            "Hantzch-widman ring misparsed as a heterogeneous hydride with alternating atoms",
        );
    }
    Ok(())
}
#[derive(Debug, Clone)]
struct SpiroBridge {
    len: usize,
    locant: Option<usize>,
    explicit: bool,
}
fn spiro_bridges(text: &str) -> Result<Vec<SpiroBridge>> {
    let prefix = if text.find('-') == Some(5) { 7 } else { 6 };
    let Some(text) = text.get(prefix..text.len().saturating_sub(1)) else {
        return err("Invalid spiro descriptor");
    };
    let mut result = Vec::new();
    for desc in rx("[.,]").split(text) {
        let digits = rx(r"\D+")
            .split(desc)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let b = if digits.len() > 1 {
            SpiroBridge {
                len: parse_number(digits[0])?,
                locant: Some(parse_number(&digits[1..].concat())?),
                explicit: true,
            }
        } else if desc.starts_with('0') && desc.len() > 1 {
            SpiroBridge {
                len: 0,
                locant: Some(parse_number(&desc[1..])?),
                explicit: false,
            }
        } else {
            SpiroBridge {
                len: parse_number(desc)?,
                locant: None,
                explicit: false,
            }
        };
        result.push(b)
    }
    Ok(result)
}
pub fn ring_closure(index: usize) -> String {
    if index > 9 {
        format!("%{index}")
    } else {
        index.to_string()
    }
}
fn find_index_of_ring_openings(smiles: &str, locant: usize) -> Result<usize> {
    if locant == 0 {
        return err(
            "Unable to find atom corresponding to number indicated by superscript in spiro descriptor",
        );
    }
    smiles.char_indices().filter(|&(_,c)|c=='C').nth(locant - 1).map(|(i,_)|i+1).ok_or_else(||ComponentGenerationError("Unable to find atom corresponding to number indicated by superscript in spiro descriptor".into()))
}
#[derive(Debug, Clone, Default)]
struct VonBaeyerBridge {
    len: usize,
    larger: Option<usize>,
    smaller: Option<usize>,
    larger_label: usize,
    smaller_label: usize,
}
fn parse_number(value: &str) -> Result<usize> {
    value
        .parse()
        .map_err(|_| ComponentGenerationError(format!("Malformed number in descriptor: {value}")))
}
fn unbranched_elements(smiles: &str) -> Result<VecDeque<String>> {
    let mut result = VecDeque::new();
    let mut index = 0;
    while index < smiles.len() {
        if smiles.as_bytes()[index] == b'[' {
            let Some(close) = smiles[index..].find(']').map(|i| i + index) else {
                return err("Malformed unbranched chain SMILES");
            };
            result.push_back(smiles[index..=close].to_string());
            index = close + 1
        } else {
            result.push_back(smiles[index..index + 1].to_string());
            index += 1
        }
    }
    Ok(result)
}
fn pop_element(elements: &mut VecDeque<String>) -> Result<String> {
    elements.pop_front().ok_or_else(|| {
        ComponentGenerationError("Disagreement between bridges and chain length".into())
    })
}
fn append_ring_locations(smiles: &mut String, locations: &HashMap<usize, Vec<usize>>, atom: usize) {
    if let Some(labels) = locations.get(&atom) {
        for &label in labels {
            smiles.push_str(&ring_closure(label))
        }
    }
}
