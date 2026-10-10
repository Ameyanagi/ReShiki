//! End-to-end candidate interpretation from OPSIN's NameToStructure.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::{
    OpsinResult, OpsinWarning, ParseOptions, ParsingError, Status, Structure,
    build_state::BuildState,
    component_generator::{ComponentGenerationContext, process_components},
    component_processor, frontend,
    parse_tree::{ParseTree, sort_parses},
    preprocess,
    resources::Resources,
    structure_builder,
};

pub(crate) fn parse(resources: &Resources, input: &str, options: &ParseOptions) -> OpsinResult {
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
        match interpret_candidate(resources, &mut candidate, *options) {
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
) -> Result<(Structure, Vec<OpsinWarning>), ParsingError> {
    let mut state = BuildState::new(options);
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
        },
        state.warnings,
    ))
}
