//! Drawing tools and their status-bar hints.

use reshiki::{chains::ChainMode, graphics::GraphicKind};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Lasso,
    Tilt,
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
    pub fn hint(self) -> &'static str {
        match self {
            Self::Select => {
                "Bonded drags use Length/Angles · Option/Alt moves freely, without guides · Shift locks an axis · Ctrl/Cmd drag copies · Drag a ring edge to fuse"
            }
            Self::Lasso => "Draw around objects · Shift adds · Option/Alt drag subtracts",
            Self::Tilt => "Drag a ring or selection to tilt · Shift snaps to 15° · Escape cancels",
            Self::Chain(ChainMode::Straight) => {
                "Drag a regular zigzag toward the pointer · Ctrl bends · Shift flips · Click places the chosen number of carbons"
            }
            Self::Chain(ChainMode::Snaking) => {
                "Steer while dragging · Retrace earlier vertices to shorten · Shift flips the first turn"
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
            Self::Atom => "Click to add an atom or replace an existing element",
            Self::Ring | Self::RingPreset(_) => {
                "Click or drag onto an atom or bond to attach · Drag from a bond to choose the side"
            }
            Self::Template => {
                "Preview, then click an atom or bond to attach · Drag to choose the side · Escape cancels"
            }
            Self::Arrow => {
                "Click to place or change an arrow · Click again to switch direction or half-head side · Drag the middle handle to bend"
            }
            Self::Text => "Click to type a label · Double-click a label to edit · Escape cancels",
            Self::Erase => {
                "Drag to erase atoms, bonds and objects along the stroke · Undo restores the whole stroke"
            }
            Self::Graphic(GraphicKind::Symbol(_)) => {
                "Click an atom to attach · Drag from an atom to position · Click empty space for a free symbol"
            }
            Self::Graphic(GraphicKind::Orbital(_)) => {
                "Click to place · Drag from the node for size/direction · Shift snaps to 15°"
            }
            Self::Graphic(_) => {
                "Drag to draw · Shift constrains proportions or angle · Escape cancels"
            }
            Self::EditPoints => {
                "Drag a curve handle or attachment point only · Escape returns to Select"
            }
        }
    }
}
