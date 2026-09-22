//! Default RDKit ring templates and bounded native-order matching.
//!
//! Adapted from RDKit 2026.03.6 Templates.h/.cpp, TemplateSmarts.h and
//! EmbeddedFrag.cpp. BSD-3-Clause; see licenses/rdkit/NOTICE. Builtin query
//! predicates are generated data, not a general SMARTS language implementation.
mod catalog;
mod matching;

use super::{attachment, geometry, rings, seeds};
use crate::chemistry::{graph::Graph, ranking::Metadata, stereo::perception::RingCache};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

pub const MAX_WORK: usize = 50_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Invalid depiction template input: {0}")]
    Invalid(&'static str),
    #[error("Depiction template work or storage limit exceeded")]
    Limit,
    #[error(transparent)]
    Rings(#[from] rings::Error),
    #[error(transparent)]
    Seeds(#[from] seeds::Error),
}
type Result<T> = std::result::Result<T, Error>;
fn at<T>(values: &[T], index: usize) -> Result<&T> {
    values
        .get(index)
        .ok_or(Error::Invalid("index outside input"))
}
struct Work(usize);
impl Work {
    fn spend(&mut self, amount: usize) -> Result<()> {
        self.0 = self.0.checked_sub(amount).ok_or(Error::Limit)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateMatch {
    /// Original TEMPLATE_SMARTS position, before indexing the catalog by size.
    pub ordinal: usize,
    /// Template atom index -> original full-molecule atom index.
    pub mapping: Vec<usize>,
    pub fragment: rings::Fragment,
}
#[derive(Debug, Clone, Serialize)]
pub struct TemplateEmbedding {
    pub fragment: rings::Fragment,
    pub template: Option<usize>,
    pub core: bool,
}

/// Prepared full-molecule properties. Ring queries exclude outside atoms while
/// explicit-degree predicates retain their full original degrees. Inputs are
/// borrowed and every successful operation returns a new detached fragment.
pub struct Input<'a> {
    graph: &'a Graph,
    metadata: &'a Metadata,
    cache: &'a RingCache,
    seeds: seeds::Input<'a>,
    adjacent: Vec<Vec<usize>>,
    bonds: HashMap<(usize, usize), usize>,
    work_limit: usize,
}
impl<'a> Input<'a> {
    pub fn new(
        graph: &'a Graph,
        metadata: &'a Metadata,
        cache: &'a RingCache,
        data: &'a [attachment::AtomData],
    ) -> Result<Self> {
        let seeds = seeds::Input::new(graph, metadata, data, cache)?;
        let mut adjacent = vec![Vec::new(); graph.atoms.len()];
        let mut bonds = HashMap::new();
        for (index, bond) in graph.bonds.iter().enumerate() {
            for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
                adjacent
                    .get_mut(a)
                    .ok_or(Error::Invalid("bond endpoint"))?
                    .push(b);
                bonds.insert((a, b), index);
            }
        }
        Ok(Self {
            graph,
            metadata,
            cache,
            seeds,
            adjacent,
            bonds,
            work_limit: MAX_WORK,
        })
    }
    pub fn with_work_limit(mut self, limit: usize) -> Self {
        self.work_limit = limit.min(MAX_WORK);
        self.seeds = self.seeds.with_work_limit(self.work_limit);
        self
    }
    /// Native EmbeddedFrag(molecule, selected rings, useRingTemplates=true).
    /// Default template coordinates are absolute and are not bond-length scaled.
    pub fn embed(&self, selected: &[usize], bond_length: f64) -> Result<TemplateEmbedding> {
        self.embed_with_budget(selected, bond_length, &mut { self.work_limit })
    }
    pub(super) fn embed_with_budget(
        &self,
        selected: &[usize],
        bond_length: f64,
        remaining: &mut usize,
    ) -> Result<TemplateEmbedding> {
        let mut work = Work((*remaining).min(self.work_limit));
        let result = self.embed_work(selected, bond_length, &mut work);
        *remaining = work.0;
        result
    }
    fn embed_work(
        &self,
        selected: &[usize],
        bond_length: f64,
        work: &mut Work,
    ) -> Result<TemplateEmbedding> {
        work.spend(self.graph.atoms.len() + self.graph.bonds.len())?;
        let rings = rings::Input::new(self.graph, self.metadata, self.cache, selected)?
            .with_work_limit(self.work_limit);
        let union = |ids: &[usize], work: &mut Work| -> Result<Vec<usize>> {
            let mut result = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for &id in ids {
                let ring = at(&self.cache.atoms, id)?;
                work.spend(ring.len())?;
                for &atom in ring {
                    if seen.insert(atom) {
                        result.push(atom);
                    }
                }
            }
            Ok(result)
        };
        if (selected.len() > 1
            || (selected.len() == 1 && at(&self.cache.atoms, *at(selected, 0)?)?.len() > 8))
            && let Some(matched) = self.match_with_work(&union(selected, work)?, work)?
        {
            return Ok(TemplateEmbedding {
                fragment: matched.fragment,
                template: Some(matched.ordinal),
                core: false,
            });
        }
        let construction = rings.begin_with_budget(bond_length, &mut work.0)?;
        let core = rings.core_with_budget(&mut work.0)?;
        if core.len() > 1 && core.len() < selected.len() {
            let source = core
                .iter()
                .map(|&i| at(selected, i).copied())
                .collect::<Result<Vec<_>>>()?;
            if let Some(matched) = self.match_with_work(&union(&source, work)?, work)? {
                return Ok(TemplateEmbedding {
                    fragment: construction
                        .finish_with_budget(Some((matched.fragment, core)), &mut work.0)?,
                    template: Some(matched.ordinal),
                    core: true,
                });
            }
        }
        Ok(TemplateEmbedding {
            fragment: construction.finish_with_budget(None, &mut work.0)?,
            template: None,
            core: false,
        })
    }
    pub fn match_system(&self, atoms: &[usize]) -> Result<Option<TemplateMatch>> {
        self.match_with_work(atoms, &mut Work(self.work_limit))
    }
    fn match_with_work(&self, atoms: &[usize], work: &mut Work) -> Result<Option<TemplateMatch>> {
        if atoms.len() > geometry::MAX_POINTS {
            return Err(Error::Limit);
        }
        let catalog = catalog::builtin()?;
        work.spend(self.graph.atoms.len() + atoms.len())?;
        let mut included = vec![false; self.graph.atoms.len()];
        for &id in atoms {
            let slot = included
                .get_mut(id)
                .ok_or(Error::Invalid("ring atom index"))?;
            if *slot {
                return Err(Error::Invalid("repeated ring-system atom"));
            }
            *slot = true;
        }
        let Some(ordinals) = catalog.by_size.get(&atoms.len()) else {
            return Ok(None);
        };
        let mut degree_counts = [0usize; 5];
        let mut bond_count = 0;
        for &id in atoms {
            work.spend(at(&self.adjacent, id)?.len())?;
            let degree = at(&self.adjacent, id)?
                .iter()
                .filter(|&&other| included.get(other) == Some(&true))
                .count();
            *degree_counts.get_mut(degree.min(4)).ok_or(Error::Limit)? += 1;
            bond_count += degree;
        }
        bond_count /= 2;
        for &ordinal in ordinals {
            work.spend(1)?;
            let template = at(&catalog.templates, ordinal)?;
            if template.edges.len() != bond_count || template.counts != degree_counts {
                continue;
            }
            let Some(mapping) = self.first_match(template, &included, work)? else {
                continue;
            };
            // Native maxMatches=1: a stereo-rejected mapping advances to the
            // next template, never to a second mapping of the same query.
            if !self.stereo_matches(template, &mapping, work)? {
                continue;
            }
            let coordinates: BTreeMap<_, _> = mapping
                .iter()
                .copied()
                .zip(template.positions.iter().copied())
                .collect();
            let fragment = self
                .seeds
                .coordinates_with_budget(&coordinates, &mut work.0)?;
            return Ok(Some(TemplateMatch {
                ordinal,
                mapping,
                fragment,
            }));
        }
        Ok(None)
    }
}
