//! CIP sequence comparison and stable insertion sorting, adapted from RDKit
//! SequenceRule.cpp, Sort.cpp, Rules.h and the individual sequence rules.
//! Copyright (C) 2020 Schrödinger, LLC. BSD-3-Clause; see licenses/rdkit/.
mod machine;
mod pairing;
mod scalar;
#[cfg(test)]
mod tests;
use super::{
    Error, Molecule,
    digraph::{Descriptor, Digraph},
    invalid,
};
use machine::{Frame, Machine, Scope, Spec, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    AtomicNumber,
    RingDuplicate,
    Isotope,
    DoubleBondStereo,
    DescriptorType,
    DescriptorPair,
    PseudoDescriptor,
    LegacyDescriptor,
    PseudoPair,
    ReferenceAtom,
}

/// Shared per-labeling-pass counter. A zero requested limit has the native
/// unlimited meaning; graph, storage and machine-work limits still apply.
pub struct Iterations {
    remaining: u32,
}
impl Iterations {
    pub fn new(limit: u32) -> Self {
        Self {
            remaining: if limit == 0 { u32::MAX } else { limit },
        }
    }
    pub fn remaining(&self) -> u32 {
        self.remaining
    }
    fn consume(&mut self) -> Result<(), Error> {
        self.remaining = self.remaining.saturating_sub(1);
        if self.remaining == 0 {
            Err(Error::Iterations)
        } else {
            Ok(())
        }
    }
}

pub struct Context<'r, 'a> {
    mol: &'r mut Molecule<'a>,
    graph: &'r mut Digraph<'a>,
    iterations: &'r mut Iterations,
}
impl<'r, 'a> Context<'r, 'a> {
    pub fn new(
        mol: &'r mut Molecule<'a>,
        graph: &'r mut Digraph<'a>,
        iterations: &'r mut Iterations,
    ) -> Result<Self, Error> {
        graph.check(mol)?;
        Ok(Self {
            mol,
            graph,
            iterations,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Sorted {
    pub edges: Vec<usize>,
    pub unique: bool,
    pub pseudo: bool,
}

/// Each subrule sorts with its own prefix of the rule list, as in the native
/// composite. This is distinct from sorting every depth with the whole list.
pub struct Rules {
    rules: Vec<Rule>,
    combined: bool,
    reference: Descriptor,
}
impl Rules {
    pub fn new(rules: &[Rule]) -> Result<Self, Error> {
        if rules.is_empty() || rules.len() > 16 {
            return Err(invalid("Expected 1–16 CIP rules"));
        }
        Ok(Self {
            rules: rules.to_vec(),
            combined: true,
            reference: Descriptor::None,
        })
    }
    pub fn single(rule: Rule) -> Self {
        Self {
            rules: vec![rule],
            combined: false,
            reference: Descriptor::None,
        }
    }
    pub fn constitutional() -> Self {
        Self {
            rules: vec![Rule::AtomicNumber, Rule::RingDuplicate, Rule::Isotope],
            combined: true,
            reference: Descriptor::None,
        }
    }
    pub fn full() -> Self {
        Self {
            rules: vec![
                Rule::AtomicNumber,
                Rule::RingDuplicate,
                Rule::Isotope,
                Rule::DoubleBondStereo,
                Rule::DescriptorType,
                Rule::DescriptorPair,
                Rule::PseudoDescriptor,
                Rule::PseudoPair,
                Rule::ReferenceAtom,
            ],
            combined: true,
            reference: Descriptor::None,
        }
    }
    pub fn with_reference(rule: Rule, reference: Descriptor) -> Result<Self, Error> {
        if !matches!(rule, Rule::DescriptorPair | Rule::PseudoPair) {
            return Err(invalid("Only descriptor-pair rules accept a reference"));
        }
        Ok(Self {
            rules: vec![rule],
            combined: false,
            reference,
        })
    }
    fn spec(&self, index: usize) -> Spec {
        Spec {
            index,
            reference: if self.combined {
                Descriptor::None
            } else {
                self.reference
            },
            isolated: false,
        }
    }
    fn scope(&self) -> Scope {
        if self.combined {
            Scope::Composite
        } else {
            Scope::Prefix(1)
        }
    }
    pub fn compare(
        &self,
        ctx: &mut Context<'_, '_>,
        a: usize,
        b: usize,
        deep: bool,
    ) -> Result<i8, Error> {
        ctx.graph.edge(a)?;
        ctx.graph.edge(b)?;
        let frame = if self.combined {
            Frame::composite(a, b)
        } else if deep {
            Frame::sequence(self.spec(0), a, b)
        } else {
            Frame::direct(self.spec(0), a, b)
        };
        match Machine::run(self, ctx, frame)? {
            Value::Number(n) => Ok(n),
            _ => Err(invalid("Missing CIP comparison")),
        }
    }
    pub fn sort(
        &self,
        ctx: &mut Context<'_, '_>,
        node: usize,
        edges: &[usize],
        deep: bool,
    ) -> Result<Sorted, Error> {
        ctx.graph.node(node)?;
        if edges.len() > 500_000 {
            return Err(Error::Limit);
        }
        for &edge in edges {
            ctx.graph.edge(edge)?.other(node)?;
        }
        match Machine::run(
            self,
            ctx,
            Frame::sort(node, edges.to_vec(), self.scope(), deep),
        )? {
            Value::Sorted(sorted) => Ok(sorted),
            _ => Err(invalid("Missing CIP sort result")),
        }
    }
    pub fn groups(
        &self,
        ctx: &mut Context<'_, '_>,
        edges: &[usize],
    ) -> Result<Vec<Vec<usize>>, Error> {
        if edges.len() > 500_000 {
            return Err(Error::Limit);
        }
        for &e in edges {
            ctx.graph.edge(e)?;
        }
        match Machine::run(self, ctx, Frame::groups(edges.to_vec(), self.scope()))? {
            Value::Groups(groups) => Ok(groups),
            _ => Err(invalid("Missing CIP groups")),
        }
    }
}
