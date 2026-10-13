//! Shared colors for editor overlays.

use iced::Color;

/// Editor accent teal.
pub(crate) const ACCENT: Color = Color::from_rgb8(19, 135, 116);
/// Soft accent fill for handles and notices.
pub(crate) const ACCENT_TINT: Color = Color::from_rgb8(225, 242, 237);
/// Darker accent for strong marks, such as the ring around the picked arrow
/// source.
pub(crate) const ACCENT_STRONG: Color = Color::from_rgb8(19, 105, 89);
