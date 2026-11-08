// SPDX-License-Identifier: MIT
// Origin: mixed adaptation of OPSIN with ledger-marked local functions; source paths and pins follow.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/NameToStructure.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinResult.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/ParseRules.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/Tokeniser.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 2.9.0, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/NameToStructure.java @ b91b610af5ab07560fedb20730d7aef46bb2bca0.
//   OPSIN 2.9.0, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinResult.java @ b91b610af5ab07560fedb20730d7aef46bb2bca0.
//   OPSIN 2.9.0, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/ParseRules.java @ b91b610af5ab07560fedb20730d7aef46bb2bca0.
//   OPSIN 2.9.0, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/Tokeniser.java @ b91b610af5ab07560fedb20730d7aef46bb2bca0.
//   ReShiki, crates/opsin/src/lib.rs @ 89f30ab6d32736a8beccf3bab9c355c68f178f00.
//   ReShiki, crates/opsin/src/lib.rs @ 2df788dc456793c687b934afd6d4b317e3c73ef8.
//   ReShiki, crates/opsin/src/lib.rs @ 777cdd3f98dfb955e165b17e352d3ce6da768b0c.
// Copyright 2017 Daniel Lowe
// Changes: Rust ownership, arena/container and error-handling adaptation; integrated native changes are documented in PORTING.md. Original function boundaries are marked below.

//! Native Rust migration of OPSIN main, commit 42e87ac6.
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
mod modern_cip;
pub mod parse_rules;
pub mod parse_tree;
pub mod phane_amplifier;
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
pub use nazuchem_kernel::budget::Budget;
use std::sync::{Arc, OnceLock};

pub const UPSTREAM_VERSION: &str = "3.0-SNAPSHOT";
pub const UPSTREAM_COMMIT: &str = "42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91";
pub const RESOURCE_FINGERPRINT: &str =
    "c4c6dbda518cb38ff80867fe6491441e3fbd11c76795d8f0cc4fe15c6ffd6302";
pub const PORT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Axial stereochemistry in the atom order of the accompanying SMILES.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Axis {
    pub atoms: [usize; 2],
    /// Reference neighbour at each corresponding axis end.
    pub refs: [usize; 2],
    /// Whether the rotation from `refs[0]` to `refs[1]` is clockwise when
    /// looking from `atoms[0]` to `atoms[1]`.
    pub clockwise: bool,
    /// Requested CIP descriptor, normalized to M or P.
    pub descriptor: String,
}

/// Semantic SMILES, parent locants, and axes in the same written atom order.
pub type SerializedStructure = (String, Vec<Option<String>>, Vec<Axis>);

#[derive(Debug, Clone)]
pub struct Structure {
    pub graph: graph::Graph,
    pub fragment: graph::FragmentId,
    /// Parent-group atoms captured before word-rule assembly, excluding suffix atoms.
    pub principal_parent_atoms: Vec<graph::AtomId>,
}

impl Structure {
    /// Semantic CXSMILES retains atom labels, enhanced stereo and polymers.
    /// Cosmetic atom values (including locants) are intentionally excluded.
    /// Axial configurations require `semantic_cxsmiles_with_locants_and_axes`.
    pub fn semantic_cxsmiles(&self) -> Result<String, SerializationError> {
        smiles::write_semantic_cxsmiles(&self.graph, self.fragment)
    }

    /// Semantic CXSMILES and the first principal-parent locant for each written
    /// atom, in SMILES order. Folded hydrogens have no entry in this vector.
    /// Returns an error when axes would otherwise be omitted.
    // Original nazuchem code (not in OPSIN).
    pub fn semantic_cxsmiles_with_locants(
        &self,
    ) -> Result<(String, Vec<Option<String>>), SerializationError> {
        smiles::write_semantic_cxsmiles_with_locants(
            &self.graph,
            self.fragment,
            &self.principal_parent_atoms,
        )
    }

    /// Serialize SMILES, parent locants, and axial configurations together.
    /// Axes use the same written atom indices as the locants vector.
    // Original nazuchem code (not in OPSIN).
    pub fn semantic_cxsmiles_with_locants_and_axes(
        &self,
    ) -> Result<SerializedStructure, SerializationError> {
        smiles::write_semantic_cxsmiles_with_locants_and_axes(
            &self.graph,
            self.fragment,
            &self.principal_parent_atoms,
        )
    }
}

/// Immutable, reusable parser; each name gets independent interpretation state.
#[derive(Debug, Clone)]
pub struct Parser {
    pub(crate) resources: Arc<resources::Resources>,
    budget: Budget,
}

impl Parser {
    /// Initialize a parser with an unbounded chemistry budget.
    /// Hosts that limit chemistry analysis should use [`Self::with_budget`].
    pub fn new() -> Result<Self, InitializationError> {
        Self::with_budget(Budget::default())
    }

    /// Initialize a parser using the host's chemistry limits and reservation hook.
    /// The budget is shared by clones and applied independently to each analysis.
    // Original nazuchem code (not in OPSIN).
    pub fn with_budget(budget: Budget) -> Result<Self, InitializationError> {
        static RESOURCES: OnceLock<Result<Arc<resources::Resources>, InitializationError>> =
            OnceLock::new();
        Ok(Self {
            resources: RESOURCES
                .get_or_init(|| resources::Resources::new().map(Arc::new))
                .clone()?,
            budget,
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
        pipeline::parse(&self.resources, name, options, &self.budget)
    }
}
