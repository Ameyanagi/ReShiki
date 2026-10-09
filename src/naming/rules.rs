//! Original compositional nomenclature profile, independent of OPSIN.
//!
//! This is a declared general-systematic-name subset, not a PIN validator.
//! Every successful AST accounts for every heavy atom and every unsaturation;
//! a separate local OPSIN roundtrip is required before publication.
mod groups;
mod parent;
mod render;
#[cfg(test)]
pub(super) mod tests;

use crate::chemistry::{self, stereo::perception::State};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const PROFILE: &str = "ReShiki organic rules 1";
pub(super) const MAX_ATOMS: usize = 64;
const WORK_LIMIT: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum GroupKind {
    Amine,
    Alcohol,
    Ketone,
    Aldehyde,
    Nitrile,
    Amide,
    Ester,
    Acid,
}
#[derive(Clone, Debug)]
struct Group {
    kind: GroupKind,
    anchor: usize,
    atoms: Vec<usize>,
    /// Ether-side carbon for an ester, not part of the acid parent.
    alcohol: Option<usize>,
}
#[derive(Clone, Debug)]
struct Prefix {
    locant: usize,
    text: String,
    complex: bool,
}
#[derive(Clone, Debug)]
enum ParentKind {
    Chain,
    Cycle,
    Retained(String),
}
#[derive(Clone, Debug)]
struct Ast {
    parent: Vec<usize>,
    kind: ParentKind,
    senior: Option<GroupKind>,
    suffix_locants: Vec<usize>,
    prefixes: Vec<Prefix>,
    doubles: Vec<usize>,
    triples: Vec<usize>,
    stereo: Vec<(usize, char)>,
    ester_alcohol: Option<String>,
}

struct Context {
    state: State,
    adjacent: Vec<Vec<(usize, u8, usize)>>,
    hydrogens: Vec<u8>,
    groups: Vec<Group>,
    stereo_labels: Vec<(chemistry::stereo::cip::configuration::Target, char)>,
    work: usize,
}
fn at<T>(items: &[T], index: usize) -> Result<&T, String> {
    items
        .get(index)
        .ok_or_else(|| "Invalid naming graph index".into())
}
impl Context {
    fn new(smiles: &str) -> Result<Self, String> {
        let identity = super::canonical_smiles(smiles)?;
        if identity.contains('.') {
            return Err("Local rule generation supports one connected molecule".into());
        }
        let state = chemistry::smiles::read(&identity)
            .map_err(|e| e.to_string())?
            .prepared
            .state;
        if state.graph.atoms.is_empty() || state.graph.atoms.len() > MAX_ATOMS {
            return Err("Local name generation supports 1–64 heavy atoms".into());
        }
        for atom in &state.graph.atoms {
            if atom.isotope != 0 || atom.charge != 0 {
                return Err("This local naming profile does not yet support isotope or formal-charge descriptors; no unlabeled or neutral name was generated".into());
            }
            if !matches!(atom.atomic_number, 6 | 7 | 8 | 9 | 16 | 17 | 35 | 53) {
                return Err("Local organic naming supports C, N, O, F, Cl, Br, I and selected aromatic S parents; other elements are unsupported".into());
            }
        }
        if state.metadata.atoms.iter().any(|m| m.chiral_tag > 2)
            || state.metadata.bonds.iter().any(|m| m.stereo > 5)
        {
            return Err(
                "This naming profile supports tetrahedral R/S and ordinary alkene E/Z only".into(),
            );
        }
        let mut adjacent = vec![vec![]; state.graph.atoms.len()];
        for (i, bond) in state.graph.bonds.iter().enumerate() {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                adjacent
                    .get_mut(a)
                    .ok_or("Invalid naming bond")?
                    .push((b, bond.order, i));
            }
        }
        let hydrogens = state
            .graph
            .atom_facts()?
            .into_iter()
            .map(|a| a.hydrogens)
            .collect();
        let stereo_labels = collect_stereo(&state)?;
        let mut result = Self {
            state,
            adjacent,
            hydrogens,
            groups: vec![],
            stereo_labels,
            work: WORK_LIMIT,
        };
        result.groups = groups::recognize(&result)?;
        Ok(result)
    }
    fn spend(&mut self, n: usize) -> Result<(), String> {
        self.work = self.work.checked_sub(n).ok_or("Local naming exhausted its exhaustive-search work budget; no approximate name was generated")?;
        Ok(())
    }
    fn number(&self, atom: usize) -> Result<u8, String> {
        Ok(at(&self.state.graph.atoms, atom)?.atomic_number)
    }
    fn edges(&self, atom: usize) -> Result<&[(usize, u8, usize)], String> {
        Ok(at(&self.adjacent, atom)?.as_slice())
    }
    fn h(&self, atom: usize) -> Result<u8, String> {
        at(&self.hydrogens, atom).copied()
    }
    fn order(&self, a: usize, b: usize) -> Result<u8, String> {
        self.edges(a)?
            .iter()
            .find(|e| e.0 == b)
            .map(|e| e.1)
            .ok_or("Missing naming parent edge".into())
    }
    fn senior(&self) -> Option<GroupKind> {
        self.groups.iter().map(|g| g.kind).max()
    }
    fn carbon_component(&self, start: usize) -> Result<BTreeSet<usize>, String> {
        let mut seen = BTreeSet::new();
        let mut todo = vec![start];
        while let Some(atom) = todo.pop() {
            if !seen.insert(atom) {
                continue;
            }
            if self.number(atom)? != 6 {
                return Err("Carbon parent has a noncarbon atom".into());
            }
            todo.extend(self.edges(atom)?.iter().filter_map(|&(b, _, _)| {
                (self.number(b).ok() == Some(6) && !seen.contains(&b)).then_some(b)
            }));
        }
        Ok(seen)
    }
}

/// Name the graph by the pinned profile; callers must independently decode it.
pub(super) fn generate(smiles: &str) -> Result<String, String> {
    let mut context = Context::new(smiles)?;
    let candidates = parent::candidates(&mut context)?;
    let longest = candidates
        .iter()
        .map(|(p, _)| p.len())
        .max()
        .ok_or("No supported naming parent")?;
    let mut ranked = Vec::new();
    for (path, kind) in candidates.into_iter().filter(|(p, _)| p.len() == longest) {
        context.spend(path.len())?;
        // Do not choose a shorter/easier parent when an equally senior parent
        // exposes a feature outside this profile. Fail rather than truncate.
        let ast = render::build(&mut context, path, kind)?;
        if let Ok(name) = render::name(&ast) {
            ranked.push((parent::score(&context, &ast), name));
        }
    }
    ranked.sort();
    let name = ranked.into_iter().next().map(|(_, n)| n)
        .ok_or("The graph is outside the declared local organic naming profile (parent, functional group, branch or stereo context is unsupported)")?;
    if name.len() > 2048 {
        return Err("Generated local name exceeds the supported length".into());
    }
    Ok(name)
}

fn stem(n: usize) -> Result<&'static str, String> {
    const STEMS: [&str; 21] = [
        "", "meth", "eth", "prop", "but", "pent", "hex", "hept", "oct", "non", "dec", "undec",
        "dodec", "tridec", "tetradec", "pentadec", "hexadec", "heptadec", "octadec", "nonadec",
        "icos",
    ];
    STEMS.get(n).copied().filter(|s| !s.is_empty()).ok_or(
        "This naming profile supports parent and substituent chains of 1–20 carbon atoms".into(),
    )
}
fn multiplier(n: usize) -> Result<&'static str, String> {
    match n {
        1 => Ok(""),
        2 => Ok("di"),
        3 => Ok("tri"),
        4 => Ok("tetra"),
        5 => Ok("penta"),
        6 => Ok("hexa"),
        7 => Ok("hepta"),
        8 => Ok("octa"),
        _ => Err("More than eight equal nomenclature groups are unsupported".into()),
    }
}
fn locants(values: &[usize]) -> String {
    values
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Simple multiplicative prefixes are added after sorting; compound prefix
/// names keep their internal letters but ignore numeric locants/punctuation.
fn alphabetic_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn collect_stereo(
    state: &State,
) -> Result<Vec<(chemistry::stereo::cip::configuration::Target, char)>, String> {
    use chemistry::stereo::cip::{configuration::Target, digraph::Descriptor, label};
    let expected_atoms = state
        .metadata
        .atoms
        .iter()
        .filter(|m| matches!(m.chiral_tag, 1 | 2))
        .count();
    let expected_bonds = state
        .metadata
        .bonds
        .iter()
        .filter(|m| (2..=5).contains(&m.stereo))
        .count();
    if expected_atoms + expected_bonds == 0 {
        return Ok(vec![]);
    }
    let assignment = label::assign(
        state,
        &label::Options {
            max_iterations: 200_000,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let mut labels = vec![];
    for label in assignment.labels {
        let descriptor=match (label.target,label.descriptor){
            (Target::Atom(_),Descriptor::R)=>'R',
            (Target::Atom(_),Descriptor::S)=>'S',
            (Target::Bond(_),Descriptor::E)=>'E',
            (Target::Bond(_),Descriptor::Z)=>'Z',
            _=>return Err("Pseudoasymmetric, axial or unresolved CIP descriptors are unsupported by this naming profile".into()),
        };
        labels.push((label.target, descriptor));
    }
    if labels.len() != expected_atoms + expected_bonds {
        return Err(
            "Not every specified stereochemical element received a supported CIP descriptor".into(),
        );
    }
    Ok(labels)
}
fn stereo(
    context: &Context,
    locant_map: &BTreeMap<usize, usize>,
) -> Result<Vec<(usize, char)>, String> {
    use chemistry::stereo::cip::configuration::Target;
    let mut labels = BTreeMap::new();
    for &(target, descriptor) in &context.stereo_labels {
        let atom = match target {
            Target::Atom(a) => a,
            Target::Bond(b) => {
                let edge = at(&context.state.graph.bonds, b)?;
                let a = *locant_map
                    .get(&edge.a)
                    .ok_or("Specified branch alkene stereo is unsupported")?;
                let c = *locant_map
                    .get(&edge.b)
                    .ok_or("Specified branch alkene stereo is unsupported")?;
                if a.abs_diff(c) != 1 {
                    return Err(
                        "Specified cyclic alkene stereo is outside this naming profile".into(),
                    );
                }
                labels.insert((a.min(c), descriptor), ());
                continue;
            }
        };
        let locant = *locant_map.get(&atom).ok_or("Specified stereochemistry in a substituent is outside this naming profile; no descriptor was dropped")?;
        labels.insert((locant, descriptor), ());
    }
    if labels.len() != context.stereo_labels.len() {
        return Err(
            "Not every specified stereochemical element received a supported CIP descriptor".into(),
        );
    }
    Ok(labels.into_keys().collect())
}
