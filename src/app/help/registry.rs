//! The one list of shortcuts that Help searches and shows.
//!
//! Commands with a menu binding take their keys from `shortcuts::label`, the
//! source menus, the context row and tooltips already read, so a binding
//! changes in one place. Contextual keys come from the same tables the
//! drawing uses (`reshiki::hotkeys`), and their previews run those edits.
use super::super::{
    Message, optimization, printing,
    shortcuts::{self, TabKey, keys, tab_keys},
};
use iced::keyboard::Modifiers;
use reshiki::editing::Transform;

/// Where a shortcut applies; Help shows it beside each row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    Atom,
    Bond,
    Selection,
    Canvas,
    KeyboardDrawing,
    Anywhere,
}

impl Scope {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Atom => "Atom",
            Self::Bond => "Bond",
            Self::Selection => "Selection",
            Self::Canvas => "Empty canvas",
            Self::KeyboardDrawing => "Keyboard drawing",
            Self::Anywhere => "Anywhere",
        }
    }
}

/// A before/after drawing made by applying the key to a small fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Example {
    /// The key at the end atom of butane.
    Atom(&'static str),
    /// The key at the middle bond of butane, or of but-2-ene for keys that
    /// act on a double bond.
    Bond(&'static str),
}

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    /// As menus show it: symbols on macOS, words elsewhere.
    pub keys: String,
    pub name: &'static str,
    pub description: &'static str,
    pub scope: Scope,
    pub synonyms: &'static [&'static str],
    pub example: Option<Example>,
}

fn entry(
    keys: impl Into<String>,
    name: &'static str,
    description: &'static str,
    scope: Scope,
    synonyms: &'static [&'static str],
) -> Entry {
    Entry {
        keys: keys.into(),
        name,
        description,
        scope,
        synonyms,
        example: None,
    }
}

fn label(message: Message) -> String {
    shortcuts::label(&message).unwrap_or_default()
}

/// The shortcuts that open Help: ? away from atoms and bonds, F1, and
/// Command+? (Command+Shift+/), macOS's Help key, since Command+/ fits the drawing.
pub(crate) fn finder_keys() -> String {
    format!(
        "? / F1 / {}",
        keys(Modifiers::COMMAND | Modifiers::SHIFT, "/")
    )
}

pub(crate) fn entries() -> Vec<Entry> {
    use Scope::*;
    let (command, shift, alt) = (Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::ALT);
    let none = Modifiers::empty();
    let mut list = vec![
        entry(
            finder_keys(),
            "Shortcut finder",
            "Search every shortcut, with a drawing of what it does",
            Anywhere,
            &["help", "keyboard", "reference", "keys", "cheat sheet"],
        ),
        entry(
            "Esc",
            "Cancel / return to Select",
            "Cancel the current operation, or return to the selection tool",
            Anywhere,
            &["escape", "abort", "stop"],
        ),
        entry(
            label(Message::KeyboardDrawing(
                super::super::keyboard_drawing::Action::Toggle,
            )),
            "Keyboard drawing",
            "Turn keyboard drawing on or off; an orange hotspot shows the active atom or bond",
            Anywhere,
            &["hotspot", "accessibility", "without mouse", "cursor"],
        ),
        // Tools, when the pointer is over empty canvas.
        entry(
            "Space / v",
            "Select tool",
            "Choose the selection tool; Space with Select active selects the last edited molecule; v needs keyboard drawing off (F8)",
            Canvas,
            &["pointer", "arrow cursor", "pick"],
        ),
        entry(
            "l",
            "Lasso tool",
            "Freehand selection; needs keyboard drawing off (F8)",
            Canvas,
            &["freehand select", "marquee"],
        ),
        entry(
            "x / 1 / b",
            "Single bond tool",
            "Draw single bonds; needs keyboard drawing off (F8)",
            Canvas,
            &["line", "draw bond", "pen"],
        ),
        entry(
            "2 / 3 / 4",
            "Double / triple / quadruple bond tools",
            "Draw multiple bonds; needs keyboard drawing off (F8)",
            Canvas,
            &["alkene", "alkyne", "multiple bond"],
        ),
        entry(
            keys(shift, "X"),
            "Straight chain tool",
            "Drag to draw a zigzag carbon chain",
            Canvas,
            &["alkyl", "zigzag", "carbon chain"],
        ),
        entry(
            format!("{}–{}", keys(shift, "3"), keys(shift, "8")),
            "Saturated ring tool (3–8)",
            "Choose a saturated ring of 3 to 8 members without changing the drawing",
            Canvas,
            &[
                "cyclopropane",
                "cyclobutane",
                "cyclopentane",
                "cyclohexane",
                "cycloalkane",
                "ring size",
            ],
        ),
        entry(
            "r",
            "Ring tool",
            "Choose the ring tool, keeping its last size; needs keyboard drawing off (F8)",
            Canvas,
            &["cycle", "polygon"],
        ),
        entry(
            label(Message::ToggleSelectedRing),
            "Toggle saturated / aromatic ring",
            "Switch the ring tool between saturated and aromatic at the same size; converts a selected complete ring",
            Canvas,
            &["aromaticity", "benzene", "kekule"],
        ),
        entry(
            format!("j / {}", keys(shift, "J")),
            "Benzene / cyclopentadiene tool",
            "Choose the benzene or cyclopentadiene template; needs keyboard drawing off (F8)",
            Canvas,
            &["phenyl", "aromatic", "template", "cp"],
        ),
        entry(
            "e / a",
            "Reaction arrow tool",
            "Draw reaction arrows; needs keyboard drawing off (F8)",
            Canvas,
            &["arrow", "scheme", "reaction"],
        ),
        entry(
            "t",
            "Text tool",
            "Add a caption or text label",
            Canvas,
            &["caption", "annotation", "label", "write"],
        ),
        entry(
            keys(shift, "T"),
            "Brackets tool",
            "Draw brackets around part of a structure",
            Canvas,
            &["polymer", "parentheses", "repeat unit"],
        ),
        entry(
            format!("{} / {}", keys(shift, "E"), keys(shift, "G")),
            "Circled plus / p orbital",
            "Draw a circled-plus symbol or a p orbital; E needs keyboard drawing off (F8)",
            Canvas,
            &["charge symbol", "orbital", "graphic"],
        ),
        entry(
            format!("{} drag", keys(alt, "")),
            "Free drawing",
            "Draw or move freely, without bond constraints, smart guides or orbital snapping",
            Canvas,
            &["option drag", "snap off", "unconstrained"],
        ),
        entry(
            format!("{}-click", keys(command, "")),
            "Delocalized ring",
            "With a ring tool, place the ring with a delocalized circle",
            Canvas,
            &["aromatic circle", "robinson"],
        ),
        // Keyboard drawing.
        entry(
            format!("Arrows / {} arrows", keys(shift, "")),
            "Move the hotspot",
            "Navigate atom–bond–atom, or skip to the next target of the same kind",
            KeyboardDrawing,
            &["navigate", "cursor", "arrow keys"],
        ),
        entry(
            "[ / ]",
            "Mark / connect atoms",
            "Mark an atom, then connect the active atom to it with a single bond",
            KeyboardDrawing,
            &["bond between", "link", "close ring"],
        ),
        entry(
            "}",
            "Coordinate to metal",
            "Coordinate the marked donor to the active metal with a dative bond",
            KeyboardDrawing,
            &["dative", "ligand", "organometallic"],
        ),
        // Selection and arrangement.
        entry(
            label(Message::Copy(false)),
            "Copy",
            "Copy the selection for ReShiki, ChemDraw, Office and image apps",
            Selection,
            &["clipboard", "duplicate"],
        ),
        entry(
            label(Message::Copy(true)),
            "Cut",
            "Copy the selection and remove it",
            Selection,
            &["clipboard"],
        ),
        entry(
            label(Message::Paste),
            "Paste",
            "Paste from the clipboard",
            Anywhere,
            &["clipboard", "insert"],
        ),
        entry(
            label(Message::CopyImage),
            "Copy as image",
            "Copy the selection as a picture",
            Selection,
            &["png", "picture", "clipboard"],
        ),
        entry(
            format!(
                "{} / {} / {}",
                keys(command, "D"),
                keys(command | alt, "C"),
                keys(command | alt, "O")
            ),
            "Copy CDXML / chemical text / MOL",
            "Copy the selection as CDXML, SMILES (or reaction JSON) or MOL text",
            Selection,
            &["smiles", "molfile", "chemdraw", "export text"],
        ),
        entry(
            label(Message::SelectAll),
            "Select all",
            "Select everything in the drawing",
            Anywhere,
            &["everything"],
        ),
        entry(
            label(Message::InvertSelection),
            "Invert selection",
            "Select everything that is not selected",
            Selection,
            &["reverse", "inverse"],
        ),
        entry(
            format!("{} / {}", label(Message::Group), label(Message::Ungroup)),
            "Group / ungroup",
            "Group the selection so it moves together, or split a group",
            Selection,
            &["combine", "lock together"],
        ),
        entry(
            label(Message::Delete),
            "Delete",
            "Delete the selection",
            Selection,
            &["remove", "erase", "backspace"],
        ),
        entry(
            format!("{} / {}", label(Message::Undo), label(Message::Redo)),
            "Undo / redo",
            "Step back or forward through your edits",
            Anywhere,
            &["revert", "history"],
        ),
        entry(
            "Drag side / corner handle",
            "Resize",
            "Drag a side handle to change width or height, or a corner handle to resize proportionally",
            Selection,
            &["scale", "stretch", "size"],
        ),
        entry(
            "Drag round handle",
            "Rotate freely",
            "Drag the round handle to rotate the selection; hold Shift to snap to 15°",
            Selection,
            &["turn", "spin", "angle"],
        ),
        entry(
            "Drag",
            "Smart guides",
            "Snap to other objects' edges, centers and equal gaps",
            Selection,
            &["move", "align", "snap"],
        ),
        entry(
            format!(
                "{} drag / {} drag",
                keys(command, ""),
                keys(command | shift, "")
            ),
            "Drag a copy",
            "Drag a copy, leaving the original in place, optionally along one axis; release the modifier, then Esc, to cancel",
            Selection,
            &["duplicate", "clone"],
        ),
        entry(
            format!("{} drag", keys(shift, "")),
            "Move along one axis",
            "Move horizontally or vertically only, snapping along that axis",
            Selection,
            &["constrain", "straight"],
        ),
        entry(
            keys(command | shift, "Right"),
            "Reaction arrow and copy",
            "Add a reaction arrow and a copy of the selected molecule to its right",
            Selection,
            &["scheme", "product", "next step"],
        ),
        entry(
            format!("Arrows / {} arrows", keys(shift, "")),
            "Nudge",
            "Move the selection 1 or 10 units; nudges never snap",
            Selection,
            &["move", "shift"],
        ),
        entry(
            format!(
                "{} arrows / {} arrows",
                keys(alt, ""),
                keys(alt | shift, "")
            ),
            "Rotate / 3D tilt",
            "Rotate by 15° (up/down) or 1° (left/right), or tilt in 3D by 12°",
            Selection,
            &["turn", "spin", "perspective"],
        ),
        entry(
            format!(
                "{} / {}",
                label(Message::Transform(Transform::FlipHorizontal)),
                label(Message::Transform(Transform::FlipVertical))
            ),
            "Flip horizontally / vertically",
            "Mirror the selection",
            Selection,
            &["mirror", "reflect"],
        ),
        entry(
            format!(
                "{} L / C / R / T / M / B / H / V",
                keys(command | shift | alt, "")
            ),
            "Align / distribute",
            "Align edges or midpoints, or distribute the selected objects evenly",
            Selection,
            &["arrange", "line up", "spacing"],
        ),
        entry(
            format!(
                "{} / {}",
                label(Message::BondDepth(false)),
                label(Message::BondDepth(true))
            ),
            "Send behind / bring forward",
            "Change which bond is drawn on top at a crossing",
            Selection,
            &["z order", "crossing", "depth"],
        ),
        entry(
            label(Message::Shortcut(shortcuts::Action::Join)),
            "Join",
            super::super::joining::JOIN_COMMAND_HINT,
            Selection,
            &["merge", "fuse", "connect", "combine"],
        ),
        entry(
            keys(command | shift, "K"),
            "Clean up",
            "Preview a cleaned-up layout for the selection",
            Selection,
            &["tidy", "beautify", "layout"],
        ),
        entry(
            label(Message::Optimization(optimization::Action::Begin)),
            "3D optimization",
            "Generate an optimized 3D conformer and preview it before Apply",
            Selection,
            &["conformer", "mmff", "uff", "geometry"],
        ),
        entry(
            if crate::platform::windows() {
                keys(alt, "K")
            } else {
                keys(command | alt, "K")
            },
            "Aromatic circle / alternating bonds",
            "Switch a selected aromatic ring between a circle and alternating bonds; a also works",
            Selection,
            &["kekule", "delocalized", "benzene"],
        ),
        entry(
            format!("{} / =", keys(none, "Enter")),
            "Edit atom label",
            "Edit the selected atom's label (M, L, X, Boc…); with keyboard drawing off (F8), several selected atoms are contracted",
            Atom,
            &["rename", "abbreviation", "type label"],
        ),
        entry(
            "g / ?",
            "Select / properties",
            "Select the atom or bond under the pointer, or open its properties",
            Atom,
            &["inspect", "info"],
        ),
    ];
    list.extend(atom_entries());
    list.extend(bond_entries());
    list.extend([
        // Files, tabs and view.
        entry(
            format!("{} / {}", label(Message::New), label(Message::Open)),
            "New / open",
            "Create or open a drawing, each in a new tab",
            Anywhere,
            &["file", "document", "load"],
        ),
        entry(
            format!("{} / {}", label(Message::Save), label(Message::SaveAs)),
            "Save / save as",
            "Save the drawing, or save a copy under a new name",
            Anywhere,
            &["file", "write", "store"],
        ),
        entry(
            tab_keys(TabKey::Close),
            "Close tab",
            "Close the current tab",
            Anywhere,
            &["file", "document"],
        ),
        entry(
            format!(
                "{} / {}",
                tab_keys(TabKey::Next),
                tab_keys(TabKey::Previous)
            ),
            "Next / previous tab",
            "Switch between open drawings",
            Anywhere,
            &["cycle", "document", "switch"],
        ),
        entry(
            format!(
                "{}–{} / {}",
                keys(command, "1"),
                keys(command, "8"),
                keys(command, "9")
            ),
            "Go to tab",
            "Show tab 1–8, or the last tab",
            Anywhere,
            &["switch", "document"],
        ),
        entry(
            format!("{} / {}", keys(command, "I"), keys(command | shift, "E")),
            "Import / export",
            "Import structures or names, or open Export",
            Anywhere,
            &["file", "smiles", "name", "pdf", "svg"],
        ),
        entry(
            label(Message::Printing(printing::Action::Start(
                reshiki::printing::Scope::Document,
            ))),
            "Print",
            "Print the drawing",
            Anywhere,
            &["paper", "pdf"],
        ),
        entry(
            format!("{} / {}", keys(command, "L"), keys(command, "E")),
            "Fixed bond length / angles",
            "Turn the bond length or angle constraint on or off",
            Anywhere,
            &["constraint", "snap", "geometry"],
        ),
        entry(
            label(Message::Fit),
            "Fit drawing",
            "Zoom so the whole drawing fits the window",
            Anywhere,
            &["zoom", "view", "show all"],
        ),
        entry(
            format!("{} / {}", keys(command, ";"), keys(command | alt, "X")),
            "Rulers / crosshair",
            "Show or hide the rulers or the crosshair",
            Anywhere,
            &["guides", "view", "measure"],
        ),
        entry(
            if crate::platform::mac() {
                format!(
                    "{} / {} scroll",
                    keys(command, ""),
                    keys(crate::platform::control_modifier(), "")
                )
            } else {
                format!("{} scroll", keys(command, ""))
            },
            "Zoom",
            "Zoom at the pointer; plain scrolling pans",
            Anywhere,
            &["magnify", "pan", "scroll"],
        ),
    ]);
    list
}

/// Keys under the pointer or hotspot at an atom.
fn atom_entries() -> Vec<Entry> {
    const ATOMS: &[(&str, &str, &str, &[&str])] = &[
        ("c", "Carbon", "Change the atom to C", &["element", "c"]),
        (
            "n / w",
            "Nitrogen",
            "Change the atom to N",
            &["element", "amine"],
        ),
        (
            "o / q",
            "Oxygen",
            "Change the atom to O",
            &["element", "alcohol", "hydroxy"],
        ),
        ("s", "Sulfur", "Change the atom to S", &["element", "thiol"]),
        ("p", "Phosphorus", "Change the atom to P", &["element"]),
        (
            "f",
            "Fluorine",
            "Change the atom to F",
            &["element", "halogen"],
        ),
        ("h", "Hydrogen", "Change the atom to H", &["element"]),
        (
            "b",
            "Bromine",
            "Change the atom to Br",
            &["element", "halogen"],
        ),
        (
            "C / l",
            "Chlorine",
            "Change the atom to Cl",
            &["element", "halogen"],
        ),
        ("B", "Boron", "Change the atom to B", &["element"]),
        (
            "i",
            "Iodine",
            "Change the atom to I",
            &["element", "halogen"],
        ),
        (
            "L",
            "Lithium",
            "Change the atom to Li",
            &["element", "metal"],
        ),
        ("S", "Silicon", "Change the atom to Si", &["element"]),
        (
            "d",
            "Deuterium",
            "Change the atom to D (²H)",
            &["isotope", "element"],
        ),
        (
            "+",
            "Increase charge",
            "Add one positive formal charge",
            &["cation", "plus"],
        ),
        (
            "-",
            "Decrease charge",
            "Add one negative formal charge",
            &["anion", "minus", "−"],
        ),
        (
            "r",
            "Variable R",
            "Label the atom R",
            &["substituent", "generic", "markush"],
        ),
        (
            "x",
            "Variable X",
            "Label the atom X",
            &["halide", "generic", "markush"],
        ),
        (
            "m",
            "Methyl (Me)",
            "Attach a methyl group label",
            &["ch3", "group"],
        ),
        ("e", "Ethyl (Et)", "Attach an ethyl group label", &["group"]),
        (
            "y",
            "Boc",
            "Attach a tert-butoxycarbonyl protecting group",
            &["protecting group"],
        ),
        (
            "P",
            "Phenyl (Ph)",
            "Attach a phenyl group label",
            &["group", "aryl"],
        ),
        (
            "A",
            "Acetyl (Ac)",
            "Attach an acetyl group label",
            &["group"],
        ),
        (
            "E",
            "Ester (CO₂Me)",
            "Attach a methyl ester group label",
            &["co2me", "group"],
        ),
        (
            "F",
            "Trifluoromethyl (CF₃)",
            "Attach a CF₃ group label",
            &["cf3", "group"],
        ),
        (
            "H",
            "Cbz",
            "Attach a benzyloxycarbonyl protecting group",
            &["protecting group", "z"],
        ),
        (
            "N",
            "Nitro (NO₂)",
            "Attach a nitro group label",
            &["no2", "group"],
        ),
        (
            "O",
            "Methoxy (OMe)",
            "Attach a methoxy group label",
            &["ome", "group"],
        ),
        (
            "Q",
            "Fmoc",
            "Attach a fluorenylmethoxycarbonyl protecting group",
            &["protecting group"],
        ),
        (
            "M",
            "MgBr",
            "Attach a Grignard MgBr group",
            &["grignard", "group"],
        ),
        ("Z", "Azide (N₃)", "Attach an azide group", &["n3", "group"]),
        (
            "1",
            "Extend chain",
            "Grow a carbon along the chain; the new end becomes active",
            &["grow", "add carbon", "sprout"],
        ),
        (
            "0",
            "Add branch",
            "Add a carbon branch; the origin stays active",
            &["grow", "substituent"],
        ),
        (
            "2",
            "Add carbonyl",
            "Make a terminal acetyl group or an internal ketone",
            &["ketone", "c=o", "oxo"],
        ),
        (
            "4 / 5",
            "Grow through a wedge / hash",
            "Add a carbon through a wedged or hashed bond",
            &["wedge", "hash", "3d"],
        ),
        (
            "8",
            "Add =CH₂",
            "Add an exocyclic methylene",
            &["methylene", "alkene"],
        ),
        (
            "9",
            "Dimethyl",
            "Add two methyl groups",
            &["gem-dimethyl", "isopropyl"],
        ),
        (
            "K",
            "tert-Butyl",
            "Add a tert-butyl group",
            &["tbu", "t-butyl"],
        ),
        ("k", "Sulfonyl", "Add a sulfonyl group", &["so2", "sulfone"]),
        (
            "z",
            "Alkyne",
            "Add a carbon through a triple bond",
            &["triple", "ethynyl"],
        ),
        (
            "3 / a",
            "Attach phenyl",
            "Attach a benzene ring",
            &["benzene", "aryl", "ring"],
        ),
        (
            "6 / 7 / v / u",
            "Attach a ring",
            "Attach a 6, 5, 3 or 4 member ring",
            &["cyclohexyl", "cyclopentyl", "cyclopropyl", "cyclobutyl"],
        ),
        (
            "j / J",
            "Cp / arene ligand",
            "Add a tilted Cp or arene ligand; repeat at a metal to add another",
            &["cyclopentadienyl", "hapto", "organometallic"],
        ),
    ];
    ATOMS
        .iter()
        .map(|(key, name, description, synonyms)| Entry {
            example: (*key != "j / J")
                .then(|| Example::Atom(key.split(" / ").next().unwrap_or(key))),
            ..entry(*key, name, description, Scope::Atom, synonyms)
        })
        .collect()
}

/// Keys under the pointer or hotspot at a bond.
fn bond_entries() -> Vec<Entry> {
    const BONDS: &[(&str, &str, &str, &[&str])] = &[
        ("1", "Single bond", "Make the bond single", &["order"]),
        (
            "2",
            "Double bond",
            "Make the bond double; repeat to cycle the second line",
            &["order", "alkene"],
        ),
        (
            "3",
            "Triple bond",
            "Make the bond triple",
            &["order", "alkyne"],
        ),
        (
            "w",
            "Wedge",
            "Make a solid wedge bond",
            &["stereo", "up", "bold wedge", "chirality"],
        ),
        (
            "h / W",
            "Hashed wedge",
            "Make a hashed wedge bond",
            &["stereo", "down", "dash wedge", "chirality"],
        ),
        ("b", "Bold bond", "Make a bold bond", &["thick"]),
        ("B", "Bold double", "Make a bold double bond", &["thick"]),
        (
            "d",
            "Dashed bond",
            "Make a dashed bond",
            &["dotted", "partial"],
        ),
        (
            "D",
            "Partial double",
            "Make a dashed double bond",
            &["delocalized", "resonance"],
        ),
        ("H", "Hashed bond", "Make a hashed bond", &["hash"]),
        (
            "y",
            "Wavy bond",
            "Make a wavy bond",
            &["undefined stereo", "squiggly"],
        ),
        (
            "l / c / r",
            "Double line position",
            "Place the second line left, centered or right",
            &["placement", "offset"],
        ),
        (
            "f",
            "Bring forward",
            "Draw the bond on top at a crossing",
            &["crossing", "depth"],
        ),
        (
            "v / 4 / 5 / 6 / 7 / 8",
            "Fuse a ring",
            "Fuse a 3–8 member ring on the bond",
            &["fused", "bicyclic", "annulate"],
        ),
        (
            "a",
            "Fuse benzene",
            "Fuse a benzene ring on the bond",
            &["naphthalene", "aromatic"],
        ),
        (
            "z",
            "Fuse diene",
            "Fuse a cyclopentadiene ring on the bond",
            &["diene", "cyclopentadiene"],
        ),
        (
            "9 / 0",
            "Fuse chair",
            "Fuse a chair cyclohexane in either orientation",
            &["conformation", "cyclohexane"],
        ),
    ];
    BONDS
        .iter()
        .map(|(key, name, description, synonyms)| Entry {
            example: (*key != "f").then(|| {
                // Show the size most people mean: benzene-sized for the ring row.
                let first = key.split(" / ").next().unwrap_or(key);
                Example::Bond(if first == "v" { "6" } else { first })
            }),
            ..entry(*key, name, description, Scope::Bond, synonyms)
        })
        .collect()
}
