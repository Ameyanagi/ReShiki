// SPDX-License-Identifier: MIT
// Origin: adapted from OPSIN; source paths and pins follow.
//   OPSIN 3.0-SNAPSHOT, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/NameToStructure.java @ 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91.
//   OPSIN 2.9.0, opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/NameToStructure.java @ b91b610af5ab07560fedb20730d7aef46bb2bca0.
//   ReShiki, crates/opsin/src/pipeline.rs @ 89f30ab6d32736a8beccf3bab9c355c68f178f00.
// Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).
// Copyright 2017 Daniel Lowe
// Changes: Rust ownership, arena/container and error-handling adaptation; integrated native changes are documented in PORTING.md.

//! End-to-end candidate interpretation from OPSIN's NameToStructure.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::{
    Budget, OpsinResult, OpsinWarning, ParseOptions, ParsingError, Status, Structure,
    build_state::BuildState,
    component_generator::{ComponentGenerationContext, process_components},
    component_processor, frontend,
    parse_tree::{ParseTree, sort_parses},
    preprocess,
    resources::Resources,
    structure_builder,
};

pub(crate) fn parse(
    resources: &Resources,
    input: &str,
    options: &ParseOptions,
    budget: &Budget,
) -> OpsinResult {
    let mut result = OpsinResult {
        status: Status::Failure,
        input: input.to_owned(),
        normalized_name: None,
        message: String::new(),
        warnings: Vec::new(),
        structure: None,
    };
    let normalized = match preprocess::preprocess(input) {
        Ok(name) => name,
        Err(error) => {
            result.message = error.to_string();
            return result;
        }
    };
    result.normalized_name = Some(normalized.clone());
    let mut candidates = match frontend::parse(resources, &normalized, options) {
        Ok(parses) => parses,
        Err(error) => {
            result.message = error.to_string();
            return result;
        }
    };
    sort_parses(&mut candidates);
    let mut first_warning = None;
    for mut candidate in candidates {
        match interpret_candidate(resources, &mut candidate, *options, budget) {
            Ok((structure, warnings)) if warnings.is_empty() => {
                result.status = Status::Success;
                result.message.clear();
                result.structure = Some(structure);
                return result;
            }
            Ok(success) if first_warning.is_none() => first_warning = Some(success),
            Ok(_) => {}
            Err(error) if result.message.is_empty() => result.message = error.to_string(),
            Err(_) => {}
        }
    }
    if let Some((structure, warnings)) = first_warning {
        result.status = Status::Warning;
        result.message = warnings
            .iter()
            .map(|warning| format!("{}: {}", warning.kind.as_str(), warning.message))
            .collect::<Vec<_>>()
            .join("; ");
        result.warnings = warnings;
        result.structure = Some(structure);
    }
    result
}

fn interpret_candidate(
    resources: &Resources,
    tree: &mut ParseTree,
    options: ParseOptions,
    budget: &Budget,
) -> Result<(Structure, Vec<OpsinWarning>), ParsingError> {
    let mut state = BuildState::with_budget(options, budget.clone());
    let mut generation = ComponentGenerationContext {
        options,
        warnings: Vec::new(),
    };
    process_components(tree, &mut generation).map_err(|error| ParsingError(error.to_string()))?;
    state.warnings = generation.warnings;
    component_processor::process_components(tree, &mut state, &resources.suffix_rules)?;
    let fragment = structure_builder::build_fragment(&mut state, &mut tree.arena, tree.root)
        .map_err(|error| ParsingError(error.to_string()))?;
    Ok((
        Structure {
            graph: state.fragment_manager.graph,
            fragment,
            principal_parent_atoms: state.principal_parent_atoms,
        },
        state.warnings,
    ))
}
