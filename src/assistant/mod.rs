//! Reviewed, editable drawing proposals. Model output never mutates the document.
//! The GUI-free proposal layer lives in reshiki-agent; the Codex app-server
//! client and its preferences stay in this crate.
pub mod codex;
pub mod settings;
pub use reshiki_agent::{
    DrawingSettings, Molecule, Proposal, Step, candidate, canvas_tools, composition, progress,
    render, render_progress, review, schema, sketch,
};
