//! Per-name state shared by OPSIN construction passes.
use crate::{
    OpsinWarning, ParseOptions, ParsingError, WarningKind,
    fragment_manager::FragmentManager,
    graph::{FragmentId, Graph, GraphError},
    parse_tree::{Arena, NodeId},
    word_rules_omitted_space::{FragmentFacts, OmittedSpaceContext, OutAtomFacts},
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct BuildState {
    pub options: ParseOptions,
    pub fragment_manager: FragmentManager,
    pub xml_suffix_map: BTreeMap<NodeId, Vec<FragmentId>>,
    pub racemic_group_count: u32,
    pub current_word_rule: Option<String>,
    pub warnings: Vec<OpsinWarning>,
}

impl BuildState {
    pub fn new(options: ParseOptions) -> Self {
        Self {
            options,
            fragment_manager: FragmentManager::new(),
            xml_suffix_map: BTreeMap::new(),
            racemic_group_count: 1,
            current_word_rule: None,
            warnings: Vec::new(),
        }
    }
    pub fn graph(&self) -> &Graph {
        &self.fragment_manager.graph
    }
    pub fn graph_mut(&mut self) -> &mut Graph {
        &mut self.fragment_manager.graph
    }
    pub fn add_warning(&mut self, kind: WarningKind, message: impl Into<String>) {
        self.warnings.push(OpsinWarning {
            kind,
            message: message.into(),
        });
    }
    pub fn add_is_ambiguous(&mut self, message: impl Into<String>) {
        self.add_warning(WarningKind::AppearsAmbiguous, message);
    }
    pub fn clone_element(
        &mut self,
        arena: &mut Arena,
        element: NodeId,
        primes_to_add: usize,
    ) -> Result<NodeId, GraphError> {
        self.fragment_manager
            .clone_element(arena, element, primes_to_add, &mut self.xml_suffix_map)
    }
}

impl OmittedSpaceContext for BuildState {
    fn fragment_facts(&self, fragment: FragmentId) -> Result<FragmentFacts, ParsingError> {
        let graph = self.graph();
        let analysis = crate::stereo_analyser::analyse(graph, fragment)
            .map_err(|error| ParsingError(error.to_string()))?;
        let source = graph.fragment(fragment);
        let mut environments = Vec::new();
        for &atom in &source.atoms {
            if crate::fragment_tools::is_characteristic_atom(graph, atom) {
                continue;
            }
            let hydrogens = graph.determine_valency(atom, true)
                - graph.incoming_valency(atom)
                - i32::from(graph.atom(atom).spare_valency)
                - graph.atom(atom).out_valency;
            let environment = crate::ambiguity::atom_environment(graph, &analysis, atom)
                .map_err(|error| ParsingError(error.to_string()))?;
            for _ in 0..hydrogens {
                environments.push(environment.to_string());
            }
        }
        Ok(FragmentFacts {
            out_atoms: source
                .out_atoms
                .iter()
                .map(|out| OutAtomFacts {
                    element: graph.atom(out.atom).element,
                    valency: out.valency as u32,
                })
                .collect(),
            functional_atom_count: source.functional_atoms.len(),
            has_default_in_atom: source.default_in_atom.is_some(),
            substitutable_hydrogen_environments: environments,
        })
    }
    fn clone_element(
        &mut self,
        arena: &mut Arena,
        element: NodeId,
    ) -> Result<NodeId, ParsingError> {
        BuildState::clone_element(self, arena, element, 0)
            .map_err(|error| ParsingError(error.to_string()))
    }
}
