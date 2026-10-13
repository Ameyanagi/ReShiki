//! Bounded finite enumeration and typed chemistry/drawing completion.
use super::{
    Alternative, AlternativeSite, ExpandedDrawing, Generic, Limits, LinkNode, RepeatUnit, Work,
    budget, depiction, drawing_size, ink, intersects,
    limits::{MAX_OUTPUTS, MAX_SOURCE_ATOMS, MAX_SOURCE_BONDS},
    materialize::{group, materialize},
    probe::{self, Stage},
    references, symmetry,
};
use crate::{
    chemistry::document::Molecule,
    document::{Document, Point},
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(super) enum Site {
    Alternative(AlternativeSite),
    Variable(u64, Vec<u64>),
    Link(LinkNode),
    Repeat(RepeatUnit),
}
impl Site {
    fn count(&self) -> Result<usize, String> {
        match self {
            Self::Alternative(s) => {
                if s.excluded {
                    return Err("An element NOT list has no finite enumeration".into());
                }
                if s.choices
                    .iter()
                    .any(|c| matches!(c, Alternative::Unresolved { .. }))
                {
                    return Err(
                        "A generic nickname needs a defined fragment before expansion".into(),
                    );
                }
                Ok(s.choices.len())
            }
            Self::Variable(_, v) => Ok(v.len()),
            Self::Link(s) => Ok((s.max - s.min + 1) as usize),
            Self::Repeat(s) => {
                let (min, max) = s.count.range()?;
                Ok((max - min + 1) as usize)
            }
        }
    }
}

/// Chemistry enumerates graphs only. Drawing also poses, depicts and measures
/// every product under the presentation allowance inspected for its source.
#[derive(Clone, Copy)]
pub(super) enum Mode<'a> {
    Chemistry,
    Drawing(&'a budget::Presentation),
}

/// A drawing product, its measured bounds and the hydrogen counts its labels
/// were measured with.
pub(super) struct Tile {
    pub(super) document: Document,
    pub(super) bounds: (Point, Point),
    pub(super) hydrogens: Vec<u32>,
}

/// Drawing products with the remaining shared allowance and the presentation
/// that priced them; assembly continues under the same allowance.
pub(super) struct DrawingBatch {
    pub(super) tiles: Vec<Tile>,
    pub(super) work: Work,
    pub(super) presentation: budget::Presentation,
}

/// Enumerate chemistry without changing the materializers' coordinates. This
/// API retains large finite graph enumeration under its existing work bounds;
/// generated canvas layout/ink measurement has a separate, stricter cost.
/// Verification entry point used by io round-trip and app tests.
#[doc(hidden)]
pub fn expand(source: &Document, limits: Limits) -> Result<Vec<Document>, String> {
    positive_limits(limits)?;
    expand_chemistry(source, limits, Work::new(limits.max_work))
}

/// Materialize every finite choice atomically. Link/repeat components are
/// depicted; a detachable variable substituent receives only a rigid exterior
/// pose. All products share one work allowance.
/// Changed components with generated-name, attached-arrow or depth-scope
/// dependencies fail closed. Independent native associations are inherited
/// exactly; this API does not freshly verify arbitrary saved name claims.
/// Verification entry point used by io round-trip and app tests.
#[doc(hidden)]
pub fn expand_drawings(source: &Document, limits: Limits) -> Result<Vec<ExpandedDrawing>, String> {
    Ok(expand_drawing_inner(source, limits)?
        .tiles
        .into_iter()
        .map(|tile| ExpandedDrawing {
            document: tile.document,
            bounds: tile.bounds,
        })
        .collect())
}

fn positive_limits(limits: Limits) -> Result<(), String> {
    if limits.max_outputs == 0 || limits.max_total_atoms == 0 || limits.max_work == 0 {
        return Err("Expansion limits must be positive".into());
    }
    Ok(())
}

fn total_limit(source: &Document, limits: Limits) -> Result<(), String> {
    if source.atoms.len() > limits.max_total_atoms
        || source.atoms.len() > MAX_SOURCE_ATOMS
        || source.bonds.len() > MAX_SOURCE_BONDS
    {
        return Err("Generic expansion exceeds the total atom or bond limit".into());
    }
    Ok(())
}

impl Limits {
    /// Live selection checks retain their source-size refusal; an already
    /// detached drawing checks the total-size limit instead.
    pub(super) fn check(self, source: &Document, selected: Option<&[u64]>) -> Result<(), String> {
        positive_limits(self)?;
        if let Some(selected) = selected {
            if source.atoms.len() > MAX_SOURCE_ATOMS
                || source.bonds.len() > MAX_SOURCE_BONDS
                || selected.is_empty() && source.atoms.len() > self.max_total_atoms
            {
                return Err("Generic expansion exceeds the source atom or bond limit".into());
            }
            Ok(())
        } else {
            total_limit(source, self)
        }
    }
}

pub(super) fn expand_drawing_inner(
    source: &Document,
    limits: Limits,
) -> Result<DrawingBatch, String> {
    limits.check(source, None)?;
    let mut work = Work::new(limits.max_work);
    let presentation = budget::Presentation::inspect(source, &mut work)?;
    expand_drawing(source, limits, presentation, work)
}

pub(super) fn expand_chemistry(
    source: &Document,
    limits: Limits,
    mut work: Work,
) -> Result<Vec<Document>, String> {
    let (products, _) = expand_with_work(source, limits, Mode::Chemistry, &mut work)?;
    finish_chemistry(products, &mut work)
}

pub(super) fn expand_drawing(
    source: &Document,
    limits: Limits,
    presentation: budget::Presentation,
    mut work: Work,
) -> Result<DrawingBatch, String> {
    let (products, replicated) =
        expand_with_work(source, limits, Mode::Drawing(&presentation), &mut work)?;
    let tiles = finish_tiles(source, products, &replicated, &presentation, &mut work)?;
    Ok(DrawingBatch {
        tiles,
        work,
        presentation,
    })
}

fn replicated_atoms(source: &Document) -> HashSet<u64> {
    source
        .generic
        .links
        .iter()
        .map(|s| s.atom)
        .chain(
            source
                .generic
                .repeats
                .iter()
                .flat_map(|s| s.members.iter().copied()),
        )
        .collect()
}

fn expand_with_work(
    source: &Document,
    limits: Limits,
    mode: Mode<'_>,
    work: &mut Work,
) -> Result<(Vec<Document>, HashSet<u64>), String> {
    let replicated = admit_structure(source, limits, mode, work)?;
    let sites = collect_sites(source)?;
    let fragments = plan_budget(source, limits, &sites, mode, work)?;
    let products = enumerate(source, limits, sites, &fragments, mode, work)?;
    Ok((products, replicated))
}

fn admit_structure(
    source: &Document,
    limits: Limits,
    mode: Mode<'_>,
    work: &mut Work,
) -> Result<HashSet<u64>, String> {
    total_limit(source, limits)?;
    let source_size = drawing_size(source);
    // Validation scans repeat boundaries and attachment target lists. Debit a
    // conservative bound before validation or catalog construction starts.
    let definitions = source.generic.alternatives.len()
        + source.generic.links.len()
        + source.generic.repeats.len()
        + source.atoms.iter().map(|a| a.centroid.len()).sum::<usize>();
    work.charge(source_size.saturating_mul(definitions.saturating_add(10)))?;
    source.validate()?;
    if let Mode::Drawing(_) = mode {
        references::protect_references(source, work)?;
    }
    if source
        .reactions
        .iter()
        .any(|r| intersects(source, &r.ids()))
    {
        return Err("Generic expansion involving explicit reaction membership is unsupported; expand a separate generic structure first".into());
    }
    // Built only after the admission charge above has prepaid the scan.
    let replicated = replicated_atoms(source);
    if source
        .atoms
        .iter()
        .any(|a| replicated.contains(&a.id) && a.map_num > 0)
    {
        return Err("Mapped link nodes or polymer members cannot be replicated without an explicit mapping policy".into());
    }
    let mut owners = HashMap::<u64, usize>::new();
    for (index, unit) in source.generic.repeats.iter().enumerate() {
        for &id in &unit.members {
            owners.insert(id, index);
        }
    }
    let links: HashSet<_> = source.generic.links.iter().map(|s| s.atom).collect();
    for unit in &source.generic.repeats {
        if unit.crossings.iter().any(|c| {
            links.contains(&c.outside)
                || owners
                    .get(&c.inside)
                    .zip(owners.get(&c.outside))
                    .is_some_and(|(a, b)| a != b)
        }) {
            return Err("Directly adjacent repeating units or link nodes are retained but cannot be expanded together".into());
        }
    }
    let mut repeat_members = HashSet::new();
    for unit in &source.generic.repeats {
        let members: HashSet<_> = unit.members.iter().copied().collect();
        if unit.members.iter().any(|id| !repeat_members.insert(*id))
            || source
                .generic
                .alternatives
                .iter()
                .any(|s| members.contains(&s.atom))
            || source
                .generic
                .links
                .iter()
                .any(|s| members.contains(&s.atom))
            || source.atoms.iter().any(|a| {
                a.attachment.is_some()
                    && (members.contains(&a.id) || a.centroid.iter().any(|id| members.contains(id)))
            })
        {
            return Err(
                "Nested or overlapping generic repeating units are retained but cannot be expanded"
                    .into(),
            );
        }
    }
    Ok(replicated)
}

fn collect_sites(source: &Document) -> Result<Vec<Site>, String> {
    let mut sites = Vec::new();
    sites.extend(
        source
            .generic
            .alternatives
            .iter()
            .cloned()
            .map(Site::Alternative),
    );
    sites.extend(
        source
            .atoms
            .iter()
            .filter(|a| a.is_variable_attachment())
            .map(|a| Site::Variable(a.id, a.centroid.clone())),
    );
    sites.extend(source.generic.links.iter().cloned().map(Site::Link));
    sites.extend(source.generic.repeats.iter().cloned().map(Site::Repeat));
    if sites.is_empty() {
        return Err(
            "Select a structure containing alternatives, variable attachments or finite repeats"
                .into(),
        );
    }
    Ok(sites)
}

fn plan_budget(
    source: &Document,
    limits: Limits,
    sites: &[Site],
    mode: Mode<'_>,
    work: &mut Work,
) -> Result<HashMap<String, Document>, String> {
    let source_size = drawing_size(source);
    let mut outputs = 1usize;
    for site in sites {
        outputs = outputs
            .checked_mul(site.count()?)
            .ok_or("Too many generic combinations")?;
        if outputs > limits.max_outputs.min(MAX_OUTPUTS) {
            return Err(
                "Generic expansion exceeds the output limit (maximum 500 structures)".into(),
            );
        }
    }
    let mut fragments = HashMap::<String, Document>::new();
    let mut upper_atoms = source.atoms.len();
    let mut upper_size = source_size;
    let mut total_atoms = (source.atoms.len() as i128) * (outputs as i128);
    let mut partial_outputs = 1usize;
    let mut construction_copies = 1usize;
    let mut planned_work = 0usize;
    for site in sites {
        let count = site.count()?;
        let (extra, graph_extra, summed_atoms) = match site {
            Site::Alternative(s) => {
                let mut max_atoms = 0;
                let mut max_graph = 0;
                let mut sum_atoms = 0i128;
                for c in &s.choices {
                    match c {
                        Alternative::Element { .. } => {}
                        Alternative::Group { label } => {
                            if !fragments.contains_key(label) {
                                // Charge once, BEFORE catalog graph preparation.
                                work.charge(4096)?;
                                let mut fragment = Document::default();
                                let id = fragment.add_atom("*", Point::new(0., 0.));
                                fragments.insert(label.clone(), group(&fragment, id, label)?);
                            }
                            let f = fragments.get(label).ok_or("Missing fragment")?;
                            let atoms = f.atoms.len().saturating_sub(1);
                            max_atoms = max_atoms.max(atoms);
                            max_graph = max_graph.max(atoms + f.bonds.len());
                            sum_atoms += atoms as i128;
                        }
                        Alternative::Unresolved { .. } => {
                            return Err("Unresolved alternative".into());
                        }
                    }
                }
                (max_atoms, max_graph, sum_atoms)
            }
            Site::Variable(..) => (0, 0, -(count as i128)),
            Site::Link(s) => {
                let n = s.max.saturating_sub(1) as usize;
                let sum: i128 = (s.min..=s.max).map(|n| n.saturating_sub(1) as i128).sum();
                (n, n.saturating_mul(2), sum)
            }
            Site::Repeat(s) => {
                let (min, max) = s.count.range()?;
                let members: HashSet<_> = s.members.iter().copied().collect();
                let internal = source
                    .bonds
                    .iter()
                    .filter(|b| members.contains(&b.a) && members.contains(&b.b))
                    .count();
                let copies = max.saturating_sub(1) as usize;
                let sum_copies: i128 = (min..=max).map(|n| n.saturating_sub(1) as i128).sum();
                (
                    s.members.len().saturating_mul(copies),
                    (s.members.len() + internal + 1).saturating_mul(copies),
                    sum_copies * s.members.len() as i128,
                )
            }
        };
        upper_atoms = upper_atoms
            .checked_add(extra)
            .ok_or("Generic atom count overflow")?;
        upper_size = upper_size
            .checked_add(graph_extra)
            .ok_or("Generic work count overflow")?;
        total_atoms += summed_atoms * (outputs / count) as i128;
        partial_outputs = partial_outputs
            .checked_mul(count)
            .ok_or("Too many generic combinations")?;
        construction_copies = construction_copies.saturating_add(partial_outputs);
        planned_work = planned_work.saturating_add(
            upper_size
                .saturating_mul(partial_outputs)
                .saturating_mul(10),
        );
    }
    if upper_atoms > MAX_SOURCE_ATOMS
        || total_atoms < 0
        || total_atoms > limits.max_total_atoms as i128
    {
        return Err("Generic expansion exceeds the total atom limit".into());
    }
    // Reserve construction, validation and final graph preparation before
    // cloning even the first candidate. Every graph append below is linear.
    work.charge(
        planned_work.saturating_add(upper_size.saturating_mul(outputs).saturating_mul(10)),
    )?;
    if let Mode::Drawing(presentation) = mode {
        // Seed + every intermediate Cartesian materializer and its validation
        // (which may clone a theme reference), then bounded final snapshots,
        // validation-theme copies and app assembly for each output.
        // All owned text/style/path buffers are reserved before the seed copy.
        presentation.reserve(
            source,
            upper_atoms,
            upper_size,
            construction_copies
                .saturating_mul(2)
                .saturating_add(outputs.saturating_mul(8)),
            work,
        )?;
    }
    Ok(fragments)
}

fn enumerate(
    source: &Document,
    limits: Limits,
    sites: Vec<Site>,
    fragments: &HashMap<String, Document>,
    mode: Mode<'_>,
    work: &mut Work,
) -> Result<Vec<Document>, String> {
    probe::enter(Stage::Snapshot);
    let mut seed = source.clone();
    seed.generic = Generic::default();
    for link in &source.generic.links {
        // A `*` link atom keeps its typed text for materialize::link's refusal.
        if let Some(atom) = seed.atom_mut(link.atom)
            && atom.element != "*"
        {
            atom.display.set_variable(None);
        }
    }
    let mut products = vec![seed];
    for site in sites {
        let count = site.count()?;
        let mut next = Vec::new();
        let mut atoms = 0usize;
        for product in products {
            for i in 0..count {
                probe::enter(Stage::Materialize);
                let mut candidate = materialize(&product, &site, i, fragments, mode, work)?;
                candidate.validate()?;
                atoms = atoms.saturating_add(candidate.atoms.len());
                if atoms > limits.max_total_atoms {
                    return Err("Generic expansion exceeds the total atom limit".into());
                }
                crate::atom_labels::clear_computed(&mut candidate);
                next.push(candidate);
            }
        }
        products = next;
    }
    Ok(products)
}

/// A finished product has no attachment, wildcard or centroid left.
fn require_concrete(product: &Document) -> Result<(), String> {
    if let Some(atom) = product
        .atoms
        .iter()
        .find(|a| a.attachment.is_some() || a.element == "*" || !a.centroid.is_empty())
    {
        if let Some(label) = super::errors::text_label(atom) {
            return Err(super::errors::outside_text_label(label));
        }
        return Err("Expansion still contains an unresolved attachment or atom".into());
    }
    Ok(())
}

/// Final chemistry preparation shared by both modes; its Molecule is reused.
fn prepare_product(product: &Document) -> Result<Molecule, String> {
    probe::enter(Stage::Prepare);
    crate::chemistry::document::prepare(product)
        .map_err(|e| format!("Expanded structure has invalid chemistry: {e}"))
}

fn finish_chemistry(products: Vec<Document>, work: &mut Work) -> Result<Vec<Document>, String> {
    let mut documents = Vec::with_capacity(products.len());
    let mut unique = symmetry::Deduplicator::default();
    for product in products {
        require_concrete(&product)?;
        let molecule = prepare_product(&product)?;
        if !unique.insert(&molecule, work)? {
            continue;
        }
        documents.push(product);
    }
    Ok(documents)
}

fn finish_tiles(
    source: &Document,
    products: Vec<Document>,
    anchors: &HashSet<u64>,
    presentation: &budget::Presentation,
    work: &mut Work,
) -> Result<Vec<Tile>, String> {
    let mut tiles = Vec::with_capacity(products.len());
    let mut unique = symmetry::Deduplicator::default();
    for mut product in products {
        require_concrete(&product)?;
        // Index/copy/ink/clearance work is reserved before the existing final
        // preparation. Its Molecule is reused; no cleanup/identity/rewedge call.
        let plan = depiction::Plan::new(&product, anchors, work)?;
        let molecule = prepare_product(&product)?;
        if !unique.insert(&molecule, work)? {
            continue;
        }
        work.charge(
            product
                .atoms
                .len()
                .saturating_mul(std::mem::size_of::<u32>()),
        )?;
        let values: Result<Vec<_>, String> = molecule
            .state
            .graph
            .atoms
            .iter()
            .zip(&molecule.state.valences)
            .map(|(a, v)| {
                u32::from(a.explicit_hydrogens)
                    .checked_add(v.implicit_hydrogens)
                    .ok_or_else(|| "Generated hydrogen label overflow".into())
            })
            .collect();
        let values = values?;
        ink::normalize_arrows(&mut product, &values, source, presentation, work)?;
        let bounds = plan.finish(&mut product, &molecule, work)?;
        tiles.push(Tile {
            document: product,
            bounds,
            hydrogens: values,
        });
    }
    Ok(tiles)
}
