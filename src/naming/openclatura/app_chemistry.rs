// SPDX-License-Identifier: MIT OR Apache-2.0
// Origin: original
// Source: ReShiki | src/naming/openclatura/app_chemistry.rs | 327d32fed63a5b3df15b7b8dc5ab84a7c36e0865
// Copyright (c) 2026 Ameyanagi and ReShiki contributors

//! Plain-data adapters for identity helpers retained by ReShiki in Phase A.
use crate::naming::kernel_bridge;
use nazuchem_kernel::stereo::perception::State;

fn bytes(state: &State) -> Result<Vec<u8>, String> {
    serde_json::to_vec(state).map_err(|error| error.to_string())
}

pub(super) fn apply_axis_markers(state: &State) -> Result<State, String> {
    let decoded = kernel_bridge::apply_axis_markers(&bytes(state)?)?;
    serde_json::from_slice(&decoded).map_err(|error| error.to_string())
}

pub(super) fn canonical_identity(state: &State) -> Result<String, String> {
    kernel_bridge::canonical_identity(&bytes(state)?)
}

pub(super) fn is_pseudoasymmetric(state: &State, atom: usize) -> Result<bool, String> {
    kernel_bridge::is_pseudoasymmetric(&bytes(state)?, atom)
}
