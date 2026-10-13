//! Drawing tools and their usage hints.

use reshiki::{chains::ChainMode, graphics::GraphicKind};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Lasso,
    Tilt,
    /// Keep a bridge bond's original axis while moving its donor-side fragment.
    StretchBond {
        fixed: u64,
        moving: u64,
    },
    Chain(ChainMode),
    Bond(u8),
    StyledBond(reshiki::bonds::BondPreset),
    Wedge,
    Hash,
    Wavy,
    Atom,
    Ring,
    RingPreset(reshiki::rings::Preset),
    Template,
    Arrow,
    Text,
    Erase,
    Graphic(GraphicKind),
    EditPoints,
}
impl Tool {
    pub fn bond_preset(self) -> Option<reshiki::bonds::BondPreset> {
        use reshiki::bonds::BondPreset as P;
        Some(match self {
            Self::Bond(1) => P::Single,
            Self::Bond(2) => P::Double,
            Self::Bond(3) => P::Triple,
            Self::Wedge => P::Wedge,
            Self::Hash => P::HashedWedge,
            Self::Wavy => P::Wavy,
            Self::StyledBond(p) => p,
            _ => return None,
        })
    }
    pub fn selects(self) -> bool {
        matches!(self, Self::Select | Self::Lasso)
    }
    /// The tool's usage hint, shown in the tooltip on the context row's tool
    /// name and reused by the status after a gesture starts. The one source of
    /// tool instructions: the context row adds only live keys (Ring's ⇧R).
    pub fn hint(self) -> &'static str {
        use reshiki::rings::Preset;
        match self {
            Self::Select => "Double-click selects a molecule · Shift-click adds",
            Self::Lasso => "Draw around objects · Shift adds · Option/Alt drag removes",
            Self::Tilt => "Drag a ring or selection to tilt · Shift snaps to 15°",
            Self::StretchBond { .. } => "Drag the moving branch along the bond · Escape cancels",
            Self::Chain(ChainMode::Straight) => {
                "Drag a regular zigzag toward the pointer · Ctrl bends · Shift flips the start · Option/Alt draws freely"
            }
            Self::Chain(ChainMode::Snaking) => {
                "Steer while dragging · Retrace to shorten · Shift flips the start · Option/Alt draws freely"
            }
            Self::Bond(2) => {
                "Click a bond to make it double · Click again to shift centered / left / right"
            }
            Self::Bond(_) => {
                "Click an endpoint to grow · Drag to draw · Click a bond to cycle single → double → triple"
            }
            Self::Wedge | Self::Hash | Self::Wavy | Self::StyledBond(_) => {
                "Click an endpoint to grow a chain · Drag to choose direction · Click a bond to change it"
            }
            Self::Atom => {
                "Click to add or replace an atom · Drag from an atom to add it with a bond"
            }
            Self::Ring | Self::RingPreset(Preset::Regular) => "Click or drag to attach",
            Self::RingPreset(Preset::Cyclopentadiene) => {
                "Click or drag to attach · Option/Alt connects · Shift swaps double bonds"
            }
            Self::RingPreset(_) => "Click or drag to attach · Option/Alt connects by a bond",
            Self::Template => "Click to place or attach · Drag to orient · Escape cancels",
            Self::Arrow => {
                "Click to place or change an arrow · Click again to switch · Drag to draw"
            }
            Self::Text => {
                "Click an atom to name it · Click empty space for a caption · Escape cancels"
            }
            Self::Erase => {
                "Drag to erase atoms, bonds and objects along the stroke · Undo restores the whole stroke"
            }
            Self::Graphic(GraphicKind::Symbol(_)) => {
                "Click an atom to attach · Click empty space for a free symbol · Drag to position"
            }
            Self::Graphic(GraphicKind::Orbital(_)) => {
                "Click to place · Drag from the node for size and direction · Shift snaps to 15° · Option/Alt bypasses atom snapping"
            }
            Self::Graphic(GraphicKind::Path) => {
                "Drag the first segment · Click adds a line · Drag adds a curve · Click the first node to close · Escape cancels"
            }
            Self::Graphic(GraphicKind::Arc) => {
                "Drag an ellipse frame · Shift makes it circular · Escape cancels"
            }
            Self::Graphic(_) => {
                "Drag to draw · Shift constrains proportions or angle · Escape cancels"
            }
            Self::EditPoints => "Drag anchors or control points · Escape returns to Select",
        }
    }
}
