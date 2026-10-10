//! Native Rust migration of OPSIN 2.9.0.
//!
//! The embedded resources and annotation algorithm are ported from the pinned
//! upstream source. The migration ledger records which structure-building
//! stages have been implemented; grammar recognition alone is not conversion.
#![forbid(unsafe_code)]

pub mod ambiguity;
pub mod api;
mod automaton;
pub mod build_results;
pub mod build_state;
mod cas;
pub mod cip;
pub mod component_generator;
pub mod component_processor;
pub mod component_processor_brackets;
pub mod component_processor_carbohydrates;
pub mod component_processor_rings;
pub mod cycle_detector;
pub mod fragment_manager;
pub mod fragment_tools;
pub mod frontend;
pub mod functional_replacement;
pub mod fused_ring_builder;
pub mod fused_ring_numberer;
pub mod graph;
pub mod isotope_specification_parser;
pub mod parse_rules;
pub mod parse_tree;
mod pipeline;
pub mod preprocess;
mod resource_data;
mod resources;
pub mod smiles;
pub mod stereo_analyser;
pub mod stereochemistry_handler;
pub mod structure_builder;
pub mod structure_building_methods;
pub mod suffix_applier;
pub mod suffix_rules;
pub mod tokenizer;
pub mod tree_tools;
mod trie;
pub mod valence;
mod word_rules;
pub mod word_rules_omitted_space;
pub mod xml_declarations;

pub use api::{
    InitializationError, OpsinResult, OpsinWarning, ParseOptions, ParsingError, SerializationError,
    Status, WarningKind,
};
use std::sync::{Arc, OnceLock};

pub const UPSTREAM_VERSION: &str = "2.9.0";
pub const UPSTREAM_COMMIT: &str = "b91b610af5ab07560fedb20730d7aef46bb2bca0";
pub const RESOURCE_FINGERPRINT: &str =
    "4269b61bf110f01d9fab808a13bac1aa4e29ec34800a7ff7cac5925d3774b9a4";
pub const PORT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Structure {
    pub graph: graph::Graph,
    pub fragment: graph::FragmentId,
}

impl Structure {
    /// Semantic CXSMILES retains atom labels, enhanced stereo and polymers.
    /// Cosmetic atom values (including locants) are intentionally excluded.
    pub fn semantic_cxsmiles(&self) -> Result<String, SerializationError> {
        smiles::write_semantic_cxsmiles(&self.graph, self.fragment)
    }
}

/// Immutable, reusable parser; each name gets independent interpretation state.
#[derive(Debug, Clone)]
pub struct Parser {
    pub(crate) resources: Arc<resources::Resources>,
}

impl Parser {
    pub fn new() -> Result<Self, InitializationError> {
        static RESOURCES: OnceLock<Result<Arc<resources::Resources>, InitializationError>> =
            OnceLock::new();
        Ok(Self {
            resources: RESOURCES
                .get_or_init(|| resources::Resources::new().map(Arc::new))
                .clone()?,
        })
    }

    /// Exposes upstream grammar annotation independently of structure assembly.
    pub fn parse_word(&self, word: &str) -> Result<parse_rules::ParseRulesResult, ParsingError> {
        parse_rules::parse_word(&self.resources, word, false)
    }

    pub fn parse_word_reverse(
        &self,
        word: &str,
    ) -> Result<parse_rules::ParseRulesResult, ParsingError> {
        parse_rules::parse_word(&self.resources, word, true)
    }

    pub fn tokenize(
        &self,
        name: &str,
        allow_space_removal: bool,
    ) -> Result<tokenizer::Tokenization, ParsingError> {
        tokenizer::tokenize(&self.resources, name, allow_space_removal, false)
    }

    pub fn tokenize_reverse(
        &self,
        name: &str,
        allow_space_removal: bool,
    ) -> Result<tokenizer::Tokenization, ParsingError> {
        tokenizer::tokenize(&self.resources, name, allow_space_removal, true)
    }

    pub fn parse_trees(
        &self,
        name: &str,
        options: &ParseOptions,
    ) -> Result<Vec<parse_tree::ParseTree>, ParsingError> {
        frontend::parse(&self.resources, &preprocess::preprocess(name)?, options)
    }

    pub fn uninvert_cas_name(&self, name: &str) -> Result<String, ParsingError> {
        cas::uninvert(&self.resources, &preprocess::preprocess(name)?)
    }

    pub fn parse(&self, name: &str, options: &ParseOptions) -> OpsinResult {
        pipeline::parse(&self.resources, name, options)
    }
}
