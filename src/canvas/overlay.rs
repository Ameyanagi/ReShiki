//! Shared colors for editor overlays.

use iced::Color;

/// Editor accent teal.
pub(crate) const ACCENT: Color = Color::from_rgb8(19, 135, 116);
/// Soft accent fill for handles and notices.
pub(crate) const ACCENT_TINT: Color = Color::from_rgb8(225, 242, 237);
/// The status bar's error red, for the atom a failed 3D build names.
pub(crate) const PROBLEM: Color = Color::from_rgb8(168, 52, 47);
/// Darker accent for strong marks, such as the ring around the picked arrow
/// source.
pub(crate) const ACCENT_STRONG: Color = Color::from_rgb8(19, 105, 89);
