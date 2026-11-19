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
    /// Places the piece armed in the Equipment browser.
    Equipment,
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
    /// Chemical bond and chain tools may turn a caption into an atom label.
    pub fn bonds_from_text(self) -> bool {
        matches!(
            self,
            Self::Chain(_)
                | Self::Bond(_)
                | Self::StyledBond(_)
                | Self::Wedge
                | Self::Hash
                | Self::Wavy
        ) && self.bond_preset() != Some(reshiki::bonds::BondPreset::Dotted)
    }

    pub fn selects(self) -> bool {
        matches!(self, Self::Select | Self::Lasso)
    }
    /// The tool's usage hint, shown in the tooltip on the context row's tool
    /// name and reused by the status after a gesture starts. The one source of
    /// tool instructions: the context row adds only live keys (Ring's ⇧R).
    pub fn hint(self) -> String {
        use reshiki::rings::Preset;
        let alt = crate::platform::alt_key();
        match self {
            Self::Select => "Double-click selects a molecule · Shift-click adds".into(),
            Self::Lasso => format!("Draw around objects · Shift adds · {alt}-drag removes"),
            Self::Tilt => "Drag a ring or selection to tilt · Shift snaps to 15°".into(),
            Self::StretchBond { .. } => {
                "Drag the moving branch along the bond · Esc cancels".into()
            }
            Self::Chain(ChainMode::Straight) => format!(
                "Drag a regular zigzag toward the pointer · Ctrl bends · Shift flips the start · {alt} draws freely"
            ),
            Self::Chain(ChainMode::Snaking) => format!(
                "Steer while dragging · Retrace to shorten · Shift flips the start · {alt} draws freely"
            ),
            Self::Bond(2) => {
                "Click a bond to make it double · Click again to shift centered / left / right"
                    .into()
            }
            Self::Bond(_) => {
                "Click an endpoint to grow · Drag to draw · Click a bond to cycle single → double → triple"
                    .into()
            }
            Self::Wedge | Self::Hash | Self::Wavy | Self::StyledBond(_) => {
                "Click an endpoint to grow a chain · Drag to choose direction · Click a bond to change it"
                    .into()
            }
            Self::Atom => {
                "Click to add or replace an atom · Drag from an atom to add it with a bond".into()
            }
            Self::Ring | Self::RingPreset(Preset::Regular) => format!(
                "Click to place or attach · Drag rotates in 15° steps · {alt} rotates freely"
            ),
            Self::RingPreset(Preset::Cyclopentadiene) => format!(
                "Click to place or attach · Drag rotates in 15° steps · {alt} rotates freely or connects · Shift swaps double bonds"
            ),
            Self::RingPreset(_) => format!(
                "Click to place or attach · Drag rotates in 15° steps · {alt} rotates freely or connects by a bond"
            ),
            Self::Template => "Click to place or attach · Drag to orient · Esc cancels".into(),
            Self::Equipment => "Choose a piece, then click to place it · Esc cancels".into(),
            Self::Arrow => {
                "Click to place or change an arrow · Click again to switch · Drag to draw".into()
            }
            Self::Text => format!(
                "Click an atom to name it · Double-click or {alt}-click another atom to repeat the label · Click empty space for a caption · Esc clears the remembered label"
            ),
            Self::Erase => "Drag to erase atoms, bonds and objects along the stroke".into(),
            Self::Graphic(GraphicKind::Symbol(_)) => format!(
                "Click an atom to attach · Drag from it to orbit in 15° steps · {alt} or Shift places freely · Click empty space for a free symbol"
            ),
            Self::Graphic(GraphicKind::Orbital(_)) => format!(
                "Click to place · Drag from the node for size and direction · Shift snaps to 15° · {alt} bypasses atom snapping"
            ),
            Self::Graphic(GraphicKind::Path) => {
                "Drag the first segment · Click adds a line · Drag adds a curve · Click the first node to close · Esc cancels"
                    .into()
            }
            Self::Graphic(GraphicKind::Arc) => {
                "Drag an ellipse frame · Shift makes it circular · Esc cancels".into()
            }
            Self::Graphic(_) => {
                "Drag to draw · Shift constrains proportions or angle · Esc cancels".into()
            }
            Self::EditPoints => format!(
                "Drag anchors or control points · Double-click nodes to change type · {alt}-drag a handle makes a corner · Click a selected arrow's shaft to add a node · Atom marks orbit in 15° steps; {alt} or Shift frees · Esc returns to Select"
            ),
        }
    }
}
