//! Semantics and keyboard behavior attached to the controls that are drawn.
//!
//! This layer does not install a native accessibility adapter. Its operations
//! inspect the current Iced widget tree, so an action cannot outlive a removed
//! control or use an earlier enabled state. Platform adapters must additionally
//! validate their window generation before issuing an operation.

mod button;
mod operations;
mod scope;

pub use button::{Button, button};
pub use operations::{Activate, Collect, Snapshot};
pub use scope::focus_scope;

use iced::Rectangle;

/// A control's semantic role. Only implemented roles are exposed here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Button,
    ToggleButton,
}

/// A snapshot produced by the actual control during a widget operation.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Stable, explicit identity, independent of label, position and locale.
    pub id: String,
    pub name: String,
    pub role: Role,
    pub enabled: bool,
    pub focused: bool,
    pub checked: Option<bool>,
    pub value: Option<String>,
    /// Window-local logical coordinates, with scroll translations applied.
    pub bounds: Rectangle,
    /// Clipped to the window and every containing scroll viewport.
    pub visible_bounds: Option<Rectangle>,
}

/// A live action offered only while its current control is being traversed.
struct LiveAction<Message> {
    id: String,
    message: Option<Message>,
}

#[cfg(test)]
mod tests;
