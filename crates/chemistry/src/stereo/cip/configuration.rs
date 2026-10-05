//! CIP configurations adapted from RDKit configs/{Configuration,Tetrahedral,
//! Sp2Bond,AtropisomerBond}.{h,cpp}, Copyright (C) 2020 Schrödinger, LLC.
//! BSD-3-Clause; see licenses/rdkit/. Graph ownership stays with the caller so
//! auxiliary evaluation can use another center's expansion.
mod bond;
mod parity;
mod tetrahedral;
use super::{
    Error, Molecule, at,
    digraph::{Descriptor, Digraph},
    invalid,
    rules::{Context, Iterations, Rules, Sorted},
};
pub use parity::parity4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Tetrahedral,
    Sp2Bond,
    AtropisomerBond,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Atom(usize),
    Bond(usize),
}

/// Validated primary-label write, including computed native neighbor order.
/// `None` in neighbor order denotes native IMPLICITH (unsigned integer max).
#[derive(Debug)]
pub struct PrimaryLabel {
    pub target: Target,
    pub descriptor: Descriptor,
    pub neighbor_order: Vec<Option<usize>>,
    /// Only Sp2Bond updates structural stereo and its two control atoms.
    pub bond_stereo: Option<(u8, [usize; 2])>,
}

pub struct Configuration {
    kind: Kind,
    target: Target,
    foci: Vec<usize>,
    carriers: Vec<Option<usize>>,
    config: u8,
    ranked: Vec<Option<usize>>,
}
impl Configuration {
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn target(&self) -> Target {
        self.target
    }
    pub fn focus(&self) -> Result<usize, Error> {
        at(&self.foci, 0).copied()
    }
    pub fn foci(&self) -> &[usize] {
        &self.foci
    }
    /// Tetrahedral central-atom carrier represents implicit H, whereas `None`
    /// is the distinct lowest-priority phantom of a trigonal pyramid.
    pub fn carriers(&self) -> &[Option<usize>] {
        &self.carriers
    }
    pub fn ranked_anchors(&self) -> &[Option<usize>] {
        &self.ranked
    }
    pub fn make_digraph<'a>(&self, mol: &Molecule<'a>) -> Result<Digraph<'a>, Error> {
        Digraph::new(mol, self.focus()?, self.kind == Kind::AtropisomerBond)
    }
    pub fn primary_label(&self, descriptor: Descriptor) -> Result<PrimaryLabel, Error> {
        let valid = match self.kind {
            Kind::Tetrahedral => matches!(
                descriptor,
                Descriptor::R | Descriptor::S | Descriptor::PseudoR | Descriptor::PseudoS
            ),
            Kind::Sp2Bond => matches!(
                descriptor,
                Descriptor::E | Descriptor::Z | Descriptor::SeqTrans | Descriptor::SeqCis
            ),
            Kind::AtropisomerBond => matches!(
                descriptor,
                Descriptor::M | Descriptor::P | Descriptor::PseudoM | Descriptor::PseudoP
            ),
        };
        if !valid {
            return Err(invalid("Unsupported primary CIP descriptor"));
        }
        Ok(PrimaryLabel {
            target: self.target,
            descriptor,
            neighbor_order: self.ranked.clone(),
            bond_stereo: if self.kind == Kind::Sp2Bond {
                Some((self.config, [self.carrier(0)?, self.carrier(1)?]))
            } else {
                None
            },
        })
    }
    fn carrier(&self, index: usize) -> Result<usize, Error> {
        at(&self.carriers, index)?.ok_or_else(|| invalid("Missing CIP carrier"))
    }
    pub fn label<'a>(
        &mut self,
        mol: &mut Molecule<'a>,
        graph: &mut Digraph<'a>,
        rules: &Rules,
        iterations: &mut Iterations,
    ) -> Result<Descriptor, Error> {
        graph.check(mol)?;
        let root = graph.original_root();
        if graph.node(root)?.atom != Some(self.focus()?) {
            return Err(invalid("Configuration and original CIP root differ"));
        }
        if graph.current_root() != root {
            graph.change_root(mol, root)?;
        }
        if self.kind == Kind::Tetrahedral {
            self.ranked.clear();
            self.label_tetrahedral(mol, graph, root, rules, iterations)
        } else {
            self.label_at(mol, graph, root, rules, iterations)
        }
    }
    pub fn label_at<'a>(
        &mut self,
        mol: &mut Molecule<'a>,
        graph: &mut Digraph<'a>,
        node: usize,
        rules: &Rules,
        iterations: &mut Iterations,
    ) -> Result<Descriptor, Error> {
        graph.check(mol)?;
        graph.node(node)?;
        match self.kind {
            Kind::Tetrahedral => {
                graph.change_root(mol, node)?;
                self.ranked.clear();
                self.label_tetrahedral(mol, graph, node, rules, iterations)
            }
            Kind::Sp2Bond | Kind::AtropisomerBond => {
                self.ranked.clear();
                self.label_bond(mol, graph, node, rules, iterations)
            }
        }
    }
}

fn sort<'a>(
    mol: &mut Molecule<'a>,
    graph: &mut Digraph<'a>,
    iterations: &mut Iterations,
    rules: &Rules,
    node: usize,
    edges: &[usize],
) -> Result<Sorted, Error> {
    rules.sort(
        &mut Context::new(mol, graph, iterations)?,
        node,
        edges,
        true,
    )
}
fn end_atom(graph: &Digraph<'_>, edge: usize) -> Result<Option<usize>, Error> {
    Ok(graph.node(graph.edge(edge)?.end)?.atom)
}
