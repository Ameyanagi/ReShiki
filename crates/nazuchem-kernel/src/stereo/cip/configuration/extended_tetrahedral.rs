// SPDX-License-Identifier: BSD-2-Clause
// Origin: adapted
// Source: SiMolecule centres | core/src/main/java/com/simolecule/centres/config/ExtendedTetrahedral.java | d4b2ca46228bdf8814ff0c1f00465be4085af8c9
// Source: ReShiki | crates/chemistry/src/stereo/cip/configuration/extended_tetrahedral.rs | be63a799aa03a83dc6ec74bb294f4d42b89298d3
// Copyright (c) 2020 John Mayfield
// Changes: Rust adaptation; the internal chemistry-to-kernel copy and any extraction changes remain recorded below.

//! Extended-tetrahedral labeling adapted from SiMolecule centres
//! config/ExtendedTetrahedral.java, revision d4b2ca46228bdf8814ff0c1f00465be4085af8c9.
//
// Copyright (c) 2020 John Mayfield
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
//    this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

// Copied unchanged from crates/chemistry/src/stereo/cip/configuration/extended_tetrahedral.rs at be63a799aa03a83dc6ec74bb294f4d42b89298d3
use super::*;
use crate::stereo::allene;

impl Configuration {
    /// AL1/AL2 describe the four peripheral atoms as an elongated tetrahedron,
    /// in graph bond order, with a missing terminal H last in its pair.
    pub fn extended_tetrahedral(mol: &Molecule<'_>, focus: usize) -> Result<Self, Error> {
        let metadata = at(&mol.state.metadata.atoms, focus)?;
        let config = match (metadata.chiral_tag, metadata.chiral_permutation) {
            (5, Some(config @ (1 | 2))) => config as u8,
            _ => return Err(invalid("Expected extended-tetrahedral winding")),
        };
        let ends = allene::terminals(&mol.state.graph, focus)
            .ok_or_else(|| invalid("Expected an allene center"))?;
        let carriers = allene::carriers(&mol.state.graph, focus)
            .ok_or_else(|| invalid("Expected four allene carriers"))?;
        Ok(Self {
            kind: Kind::ExtendedTetrahedral,
            target: Target::Atom(focus),
            foci: vec![focus, ends[0], ends[1]],
            carriers: carriers.to_vec(),
            config,
            ranked: Vec::new(),
        })
    }

    pub(super) fn label_extended_tetrahedral<'a>(
        &mut self,
        mol: &mut Molecule<'a>,
        graph: &mut Digraph<'a>,
        node: usize,
        rules: &Rules,
        iterations: &mut Iterations,
    ) -> Result<Descriptor, Error> {
        if graph.node(node)?.atom != Some(self.focus()?) {
            return Ok(Descriptor::Unknown);
        }
        graph.change_root(mol, node)?;
        let mut ends = [None; 2];
        for edge in graph.edges(mol, node)?.to_vec() {
            let other = graph.edge(edge)?.other(node)?;
            let terminal = graph.node(other)?;
            if terminal.is_duplicate() {
                continue;
            }
            for (index, end) in ends.iter_mut().enumerate() {
                if terminal.atom == Some(*at(&self.foci, index + 1)?) {
                    *end = Some(other);
                }
            }
        }
        let [Some(near), Some(far)] = ends else {
            return Ok(Descriptor::Unknown);
        };
        let near_edges = terminal_edges(mol, graph, near, node)?;
        let far_edges = terminal_edges(mol, graph, far, node)?;
        if near_edges.len() != 2 || far_edges.len() != 2 {
            return Ok(Descriptor::Unknown);
        }
        let mut priorities = sort_ends(
            mol,
            graph,
            [near, far],
            [&near_edges, &far_edges],
            rules,
            iterations,
        )?;
        // Centres' rule-6 reference resolves equivalent ring paths together.
        if !priorities[0].unique
            && !priorities[1].unique
            && ring_edge(mol, graph, *at(&near_edges, 0)?)?
            && ring_edge(mol, graph, *at(&far_edges, 0)?)?
        {
            let reference = graph
                .node(graph.edge(*at(&near_edges, 0)?)?.other(near)?)?
                .atom;
            graph.set_rule6_reference(reference)?;
            let result = sort_ends(
                mol,
                graph,
                [near, far],
                [&near_edges, &far_edges],
                rules,
                iterations,
            );
            graph.set_rule6_reference(None)?;
            priorities = result?;
        }
        if priorities.iter().any(|priority| !priority.unique) {
            return Ok(Descriptor::Unknown);
        }
        let mut flip = false;
        for ((end, priority), reference) in [near, far]
            .into_iter()
            .zip(&priorities)
            .zip(self.carriers.as_chunks::<2>().0)
        {
            let ordered = [
                graph
                    .node(graph.edge(*at(&priority.edges, 0)?)?.other(end)?)?
                    .atom,
                graph
                    .node(graph.edge(*at(&priority.edges, 1)?)?.other(end)?)?
                    .atom,
            ];
            let reference = [*at(reference, 0)?, *at(reference, 1)?];
            if ordered == [reference[1], reference[0]] {
                flip ^= true;
            } else if ordered != reference {
                return Ok(Descriptor::Unknown);
            }
            self.ranked.extend(ordered);
        }
        // Near ligands precede far ligands; within each pair CIP decides order.
        // With this ordering AL2 is M (AL1 is P), following centres and the
        // OpenSMILES elongated-tetrahedron convention. Two pseudo ends cancel.
        let minus = (self.config == 2) ^ flip;
        let pseudo = priorities[0].pseudo != priorities[1].pseudo;
        Ok(match (minus, pseudo) {
            (true, false) => Descriptor::M,
            (false, false) => Descriptor::P,
            (true, true) => Descriptor::PseudoM,
            (false, true) => Descriptor::PseudoP,
        })
    }
}

fn terminal_edges<'a>(
    mol: &mut Molecule<'a>,
    graph: &mut Digraph<'a>,
    terminal: usize,
    center: usize,
) -> Result<Vec<usize>, Error> {
    let mut edges = Vec::new();
    for edge in graph.edges(mol, terminal)?.to_vec() {
        let other = graph.edge(edge)?.other(terminal)?;
        if other != center && !graph.node(other)?.is_duplicate() {
            edges.push(edge);
        }
    }
    Ok(edges)
}

fn sort_ends<'a>(
    mol: &mut Molecule<'a>,
    graph: &mut Digraph<'a>,
    ends: [usize; 2],
    edges: [&[usize]; 2],
    rules: &Rules,
    iterations: &mut Iterations,
) -> Result<[Sorted; 2], Error> {
    graph.change_root(mol, ends[0])?;
    let near = sort(mol, graph, iterations, rules, ends[0], edges[0])?;
    graph.change_root(mol, ends[1])?;
    let far = sort(mol, graph, iterations, rules, ends[1], edges[1])?;
    Ok([near, far])
}

fn ring_edge(mol: &mut Molecule<'_>, graph: &Digraph<'_>, edge: usize) -> Result<bool, Error> {
    match graph.edge(edge)?.bond {
        Some(bond) => mol.is_in_ring(bond),
        None => Ok(false),
    }
}
