//! Zoom-independent pointer tolerances in screen pixels, converted to world
//! units with `Camera::world_len`. Canvas previews and app commits share these
//! constants so their picking and placement agree.

/// Radius for atom/object picking and placement snapping.
pub(crate) const PICK_RADIUS_PX: f32 = 10.;
/// Radius for grabbing a selected object's handle, node or edit point.
pub(crate) const HANDLE_PICK_PX: f32 = 8.;
/// Radius for picking an attachment target for a curved arrow.
pub(crate) const ARROW_TARGET_PICK_PX: f32 = 8.;
/// Movement needed to treat a pointer gesture as a drag rather than a click.
pub(crate) const DRAG_SLOP_PX: f32 = 3.;
/// Release distance below which stretching or selection counts as no movement.
pub(crate) const CLICK_SLOP_PX: f32 = 1.;
